use std::error::Error;
use std::ffi::{CStr, CString, NulError, c_char, c_int, c_void};
use std::fmt;
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::rc::Rc;

use serde_json::Value;

use crate::ffi::{
    JS_Call, JS_EVAL_FLAG_COMPILE_ONLY, JS_EVAL_TYPE_GLOBAL, JS_EVAL_TYPE_MODULE, JS_Eval,
    JS_FreeCString, JS_FreeContext, JS_FreeRuntime, JS_GetContextOpaque, JS_GetException,
    JS_GetGlobalObject, JS_GetPropertyStr, JS_NewContext, JS_NewRuntime, JS_SetContextOpaque,
    JS_SetMaxStackSize, JS_SetMemoryLimit, JS_SetModuleLoaderFunc, JS_SetPropertyStr,
    JS_TAG_MODULE, JS_TAG_NULL, JS_TAG_UNDEFINED, JS_ToCStringLen2, JSContext, JSModuleDef,
    JSRuntime, JSValue, js_malloc, rqjs_free_value, rqjs_is_exception, rqjs_is_function,
    rqjs_json_stringify, rqjs_new_c_function_magic, rqjs_parse_json, rqjs_throw_host_error,
    rqjs_throw_module_error, rqjs_value_ptr, rqjs_value_tag,
};

type JsonHostFunction = dyn FnMut(&[Value]) -> Result<Value, String> + 'static;

#[derive(Default)]
struct HostState {
    functions: Vec<Box<JsonHostFunction>>,
}

struct EmbeddedModule {
    specifier: &'static str,
    source: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/embedded_modules.rs"));

fn embedded_module(specifier: &str) -> Option<&'static EmbeddedModule> {
    EMBEDDED_MODULES
        .iter()
        .find(|module| module.specifier == specifier)
}

fn normalize_path(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut segments = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() && !absolute {
                    segments.push("..");
                }
            }
            value => segments.push(value),
        }
    }

    let joined = segments.join("/");
    if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}

fn normalize_module_specifier(base: &str, requested: &str) -> String {
    if requested == "parse5" {
        return String::from("/qjs/parse5/parse5.mjs");
    }
    if requested.starts_with('/') {
        return normalize_path(requested);
    }
    if requested.starts_with("./") || requested.starts_with("../") {
        let base_dir = base.rsplit_once('/').map_or("", |(directory, _)| directory);
        return normalize_path(&format!("{base_dir}/{requested}"));
    }
    String::from(requested)
}

unsafe fn qjs_strdup(ctx: *mut JSContext, value: &str) -> *mut c_char {
    let bytes = value.as_bytes();
    let output = unsafe { js_malloc(ctx, bytes.len() + 1) }.cast::<u8>();
    if output.is_null() {
        return ptr::null_mut();
    }
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), output, bytes.len());
        *output.add(bytes.len()) = 0;
    }
    output.cast::<c_char>()
}

unsafe extern "C" fn module_normalize(
    ctx: *mut JSContext,
    module_base_name: *const c_char,
    module_name: *const c_char,
    _opaque: *mut c_void,
) -> *mut c_char {
    if ctx.is_null() || module_name.is_null() {
        return ptr::null_mut();
    }

    let requested = unsafe { CStr::from_ptr(module_name) }.to_string_lossy();
    let base = if module_base_name.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(module_base_name) }
            .to_string_lossy()
            .into_owned()
    };
    let normalized = normalize_module_specifier(&base, &requested);
    unsafe { qjs_strdup(ctx, &normalized) }
}

unsafe fn throw_module_error(ctx: *mut JSContext, message: String) {
    let sanitized = message.replace('\0', "\\0");
    if let Ok(message) = CString::new(sanitized) {
        unsafe { rqjs_throw_module_error(ctx, message.as_ptr()) };
    }
}

unsafe extern "C" fn module_loader(
    ctx: *mut JSContext,
    module_name: *const c_char,
    _opaque: *mut c_void,
) -> *mut JSModuleDef {
    if ctx.is_null() || module_name.is_null() {
        return ptr::null_mut();
    }

    let specifier = unsafe { CStr::from_ptr(module_name) }.to_string_lossy();
    let Some(module) = embedded_module(&specifier) else {
        unsafe { throw_module_error(ctx, format!("embedded module not found: {specifier}")) };
        return ptr::null_mut();
    };

    let source = nul_terminated(module.source);
    let value = unsafe {
        JS_Eval(
            ctx,
            source.as_ptr().cast::<c_char>(),
            module.source.len(),
            module_name,
            JS_EVAL_TYPE_MODULE | JS_EVAL_FLAG_COMPILE_ONLY,
        )
    };
    if unsafe { rqjs_is_exception(value) } != 0
        || i64::from(unsafe { rqjs_value_tag(value) }) != JS_TAG_MODULE
    {
        if unsafe { rqjs_is_exception(value) } == 0 {
            unsafe { rqjs_free_value(ctx, value) };
            unsafe {
                throw_module_error(ctx, format!("module did not compile as ESM: {specifier}"))
            };
        }
        return ptr::null_mut();
    }

    let module_def = unsafe { rqjs_value_ptr(value) }.cast::<JSModuleDef>();
    unsafe { rqjs_free_value(ctx, value) };
    module_def
}

fn nul_terminated(source: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(source.len() + 1);
    output.extend_from_slice(source);
    output.push(0);
    output
}

unsafe fn raw_value_to_string(ctx: *mut JSContext, value: JSValue) -> Option<String> {
    let mut length = 0usize;
    let pointer = unsafe { JS_ToCStringLen2(ctx, &mut length, value, 0) };
    if pointer.is_null() {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), length) };
    let output = String::from_utf8_lossy(bytes).into_owned();
    unsafe { JS_FreeCString(ctx, pointer) };
    Some(output)
}

unsafe fn raw_exception_message(ctx: *mut JSContext) -> String {
    let exception = unsafe { JS_GetException(ctx) };
    let message = unsafe { raw_value_to_string(ctx, exception) }
        .unwrap_or_else(|| String::from("unknown JavaScript exception"));
    unsafe { rqjs_free_value(ctx, exception) };
    message
}

unsafe fn raw_value_to_json(ctx: *mut JSContext, value: JSValue) -> Result<Value, String> {
    let json = unsafe { rqjs_json_stringify(ctx, value) };
    if unsafe { rqjs_is_exception(json) } != 0 {
        return Err(unsafe { raw_exception_message(ctx) });
    }
    if matches!(
        i64::from(unsafe { rqjs_value_tag(json) }),
        JS_TAG_NULL | JS_TAG_UNDEFINED
    ) {
        unsafe { rqjs_free_value(ctx, json) };
        return Err(String::from("argument is not JSON-serializable"));
    }
    let text = unsafe { raw_value_to_string(ctx, json) };
    unsafe { rqjs_free_value(ctx, json) };
    let text = text.ok_or_else(|| String::from("failed to read JSON argument"))?;
    serde_json::from_str(&text).map_err(|error| format!("invalid JSON argument: {error}"))
}

unsafe fn raw_json_to_value(ctx: *mut JSContext, value: &Value) -> Result<JSValue, String> {
    let json = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    let terminated = nul_terminated(&json);
    let parsed = unsafe {
        rqjs_parse_json(
            ctx,
            terminated.as_ptr().cast::<c_char>(),
            json.len(),
            c"<rust-host-result>".as_ptr(),
        )
    };
    if unsafe { rqjs_is_exception(parsed) } != 0 {
        Err(unsafe { raw_exception_message(ctx) })
    } else {
        Ok(parsed)
    }
}

unsafe fn throw_host_error(ctx: *mut JSContext, message: &str) -> JSValue {
    let sanitized = message.replace('\0', "\\0");
    let message = CString::new(sanitized).unwrap_or_else(|_| {
        CString::new("Rust host callback failed").expect("literal is NUL-free")
    });
    unsafe { rqjs_throw_host_error(ctx, message.as_ptr()) }
}

unsafe fn invoke_json_host_function(
    ctx: *mut JSContext,
    argc: c_int,
    argv: *const JSValue,
    magic: c_int,
) -> Result<JSValue, String> {
    let state = unsafe { JS_GetContextOpaque(ctx) }.cast::<HostState>();
    if state.is_null() {
        return Err(String::from("QuickJS host state is unavailable"));
    }
    if magic < 0 {
        return Err(String::from("invalid Rust host function identifier"));
    }
    if argc > 0 && argv.is_null() {
        return Err(String::from("QuickJS passed null callback arguments"));
    }
    let values = if argc <= 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(argv, argc as usize) }
    };
    let mut arguments = Vec::with_capacity(values.len());
    for value in values {
        arguments.push(unsafe { raw_value_to_json(ctx, *value) }?);
    }

    // Do not borrow HostState until after argument stringification: user-defined
    // `toJSON` methods can execute JavaScript and must not re-enter an active
    // mutable Rust borrow.
    let state = unsafe { &mut *state };
    let function = state
        .functions
        .get_mut(magic as usize)
        .ok_or_else(|| String::from("Rust host function is no longer registered"))?;
    let output = function(&arguments)?;
    unsafe { raw_json_to_value(ctx, &output) }
}

unsafe extern "C" fn json_host_callback(
    ctx: *mut JSContext,
    _this_value: JSValue,
    argc: c_int,
    argv: *const JSValue,
    magic: c_int,
) -> JSValue {
    let result = catch_unwind(AssertUnwindSafe(|| unsafe {
        invoke_json_host_function(ctx, argc, argv, magic)
    }));
    match result {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => unsafe { throw_host_error(ctx, &error) },
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("Rust host callback panicked");
            unsafe { throw_host_error(ctx, message) }
        }
    }
}

/// Resource limits for a single QuickJS runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JsEngineOptions {
    pub memory_limit_bytes: usize,
    pub stack_limit_bytes: usize,
}

impl Default for JsEngineOptions {
    fn default() -> Self {
        Self {
            memory_limit_bytes: 256 * 1024 * 1024,
            stack_limit_bytes: 8 * 1024 * 1024,
        }
    }
}

#[derive(Debug)]
pub enum JsError {
    RuntimeInitialization,
    ContextInitialization,
    InvalidName(NulError),
    Exception(String),
    NotJsonSerializable,
    InvalidJson(serde_json::Error),
    MissingGlobalFunction(String),
    MissingEmbeddedModule(String),
    InvalidHostFunctionArity(usize),
    TooManyHostFunctions,
}

impl fmt::Display for JsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RuntimeInitialization => {
                formatter.write_str("QuickJS runtime initialization failed")
            }
            Self::ContextInitialization => {
                formatter.write_str("QuickJS context initialization failed")
            }
            Self::InvalidName(error) => write!(formatter, "JavaScript name contains NUL: {error}"),
            Self::Exception(message) => write!(formatter, "QuickJS exception: {message}"),
            Self::NotJsonSerializable => {
                formatter.write_str("JavaScript result is not JSON-serializable")
            }
            Self::InvalidJson(error) => write!(formatter, "invalid JSON result: {error}"),
            Self::MissingGlobalFunction(name) => {
                write!(formatter, "global JavaScript function not found: {name}")
            }
            Self::MissingEmbeddedModule(name) => {
                write!(formatter, "embedded JavaScript module not found: {name}")
            }
            Self::InvalidHostFunctionArity(arity) => {
                write!(
                    formatter,
                    "host function arity does not fit QuickJS: {arity}"
                )
            }
            Self::TooManyHostFunctions => formatter.write_str("too many QuickJS host functions"),
        }
    }
}

impl Error for JsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidName(error) => Some(error),
            Self::InvalidJson(error) => Some(error),
            _ => None,
        }
    }
}

impl From<NulError> for JsError {
    fn from(error: NulError) -> Self {
        Self::InvalidName(error)
    }
}

/// Owning, single-threaded QuickJS runtime.
///
/// It exposes owned Rust results only; raw QuickJS values and pointers never
/// escape the engine. Create one engine per worker thread and reuse it there.
pub struct JsEngine {
    runtime: *mut JSRuntime,
    context: *mut JSContext,
    host_state: Box<HostState>,
    _single_thread: PhantomData<Rc<()>>,
}

impl JsEngine {
    pub fn new() -> Result<Self, JsError> {
        Self::with_options(JsEngineOptions::default())
    }

    pub fn with_options(options: JsEngineOptions) -> Result<Self, JsError> {
        let runtime = unsafe { JS_NewRuntime() };
        if runtime.is_null() {
            return Err(JsError::RuntimeInitialization);
        }
        unsafe {
            JS_SetMemoryLimit(runtime, options.memory_limit_bytes);
            JS_SetMaxStackSize(runtime, options.stack_limit_bytes);
            JS_SetModuleLoaderFunc(
                runtime,
                Some(module_normalize),
                Some(module_loader),
                ptr::null_mut(),
            );
        }

        let context = unsafe { JS_NewContext(runtime) };
        if context.is_null() {
            unsafe { JS_FreeRuntime(runtime) };
            return Err(JsError::ContextInitialization);
        }

        let mut host_state = Box::<HostState>::default();
        unsafe {
            JS_SetContextOpaque(
                context,
                (&mut *host_state as *mut HostState).cast::<c_void>(),
            );
        }

        Ok(Self {
            runtime,
            context,
            host_state,
            _single_thread: PhantomData,
        })
    }

    pub fn bundled_module_specifiers() -> impl Iterator<Item = &'static str> {
        EMBEDDED_MODULES.iter().map(|module| module.specifier)
    }

    /// Evaluates a classic script and discards its result.
    pub fn eval_void(&mut self, source: &str, filename: &str) -> Result<(), JsError> {
        let value = self.eval_raw(source.as_bytes(), filename, JS_EVAL_TYPE_GLOBAL)?;
        unsafe { rqjs_free_value(self.context, value) };
        Ok(())
    }

    /// Evaluates a classic script and applies JavaScript `String(...)` to its result.
    pub fn eval_to_string(&mut self, source: &str, filename: &str) -> Result<String, JsError> {
        let value = self.eval_raw(source.as_bytes(), filename, JS_EVAL_TYPE_GLOBAL)?;
        let output = self
            .value_to_string(value)
            .ok_or_else(|| self.take_exception());
        unsafe { rqjs_free_value(self.context, value) };
        output
    }

    /// Evaluates a classic script and converts its result through `JSON.stringify`.
    pub fn eval_json(&mut self, source: &str, filename: &str) -> Result<Value, JsError> {
        let value = self.eval_raw(source.as_bytes(), filename, JS_EVAL_TYPE_GLOBAL)?;
        let output = self.value_to_json(value);
        unsafe { rqjs_free_value(self.context, value) };
        output
    }

    /// Loads and executes a bundled ESM entrypoint and its embedded dependencies.
    pub fn eval_embedded_module(&mut self, specifier: &str) -> Result<(), JsError> {
        let module = embedded_module(specifier)
            .ok_or_else(|| JsError::MissingEmbeddedModule(String::from(specifier)))?;
        let value = self.eval_raw(module.source, specifier, JS_EVAL_TYPE_MODULE)?;
        unsafe { rqjs_free_value(self.context, value) };
        Ok(())
    }

    /// Installs a global JavaScript function backed by a Rust JSON callback.
    ///
    /// Arguments are converted with `JSON.stringify`; the callback's returned
    /// JSON value becomes the JavaScript return value. Rust errors and panics
    /// become catchable JavaScript exceptions and never unwind across the C ABI.
    pub fn register_json_function<F>(
        &mut self,
        name: &str,
        arity: usize,
        function: F,
    ) -> Result<(), JsError>
    where
        F: FnMut(&[Value]) -> Result<Value, String> + 'static,
    {
        let name_c = CString::new(name)?;
        let arity = c_int::try_from(arity).map_err(|_| JsError::InvalidHostFunctionArity(arity))?;
        let magic = c_int::try_from(self.host_state.functions.len())
            .map_err(|_| JsError::TooManyHostFunctions)?;

        let global = unsafe { JS_GetGlobalObject(self.context) };
        if unsafe { rqjs_is_exception(global) } != 0 {
            return Err(self.take_exception());
        }
        self.host_state.functions.push(Box::new(function));
        let js_function = unsafe {
            rqjs_new_c_function_magic(
                self.context,
                Some(json_host_callback),
                name_c.as_ptr(),
                arity,
                magic,
            )
        };
        if unsafe { rqjs_is_exception(js_function) } != 0 {
            self.host_state.functions.pop();
            unsafe { rqjs_free_value(self.context, global) };
            return Err(self.take_exception());
        }

        let status = unsafe {
            // JS_SetPropertyStr consumes js_function on success and failure.
            JS_SetPropertyStr(self.context, global, name_c.as_ptr(), js_function)
        };
        unsafe { rqjs_free_value(self.context, global) };
        if status < 0 {
            self.host_state.functions.pop();
            return Err(self.take_exception());
        }
        Ok(())
    }

    /// Calls a global JavaScript function with JSON arguments and returns JSON.
    pub fn call_global_json(&mut self, name: &str, arguments: &[Value]) -> Result<Value, JsError> {
        let name_c = CString::new(name)?;
        let global = unsafe { JS_GetGlobalObject(self.context) };
        if unsafe { rqjs_is_exception(global) } != 0 {
            return Err(self.take_exception());
        }
        let function = unsafe { JS_GetPropertyStr(self.context, global, name_c.as_ptr()) };
        if unsafe { rqjs_is_exception(function) } != 0 {
            unsafe { rqjs_free_value(self.context, global) };
            return Err(self.take_exception());
        }
        if unsafe { rqjs_is_function(self.context, function) } == 0 {
            unsafe {
                rqjs_free_value(self.context, function);
                rqjs_free_value(self.context, global);
            }
            return Err(JsError::MissingGlobalFunction(String::from(name)));
        }

        let mut js_arguments = Vec::with_capacity(arguments.len());
        for argument in arguments {
            match self.json_to_value(argument) {
                Ok(value) => js_arguments.push(value),
                Err(error) => {
                    for value in js_arguments {
                        unsafe { rqjs_free_value(self.context, value) };
                    }
                    unsafe {
                        rqjs_free_value(self.context, function);
                        rqjs_free_value(self.context, global);
                    }
                    return Err(error);
                }
            }
        }

        let result = unsafe {
            JS_Call(
                self.context,
                function,
                global,
                js_arguments.len() as c_int,
                js_arguments.as_ptr(),
            )
        };
        for argument in js_arguments {
            unsafe { rqjs_free_value(self.context, argument) };
        }
        unsafe {
            rqjs_free_value(self.context, function);
            rqjs_free_value(self.context, global);
        }
        if unsafe { rqjs_is_exception(result) } != 0 {
            return Err(self.take_exception());
        }
        let output = self.value_to_json(result);
        unsafe { rqjs_free_value(self.context, result) };
        output
    }

    fn eval_raw(
        &mut self,
        source: &[u8],
        filename: &str,
        flags: c_int,
    ) -> Result<JSValue, JsError> {
        let filename = CString::new(filename)?;
        let terminated = nul_terminated(source);
        let value = unsafe {
            JS_Eval(
                self.context,
                terminated.as_ptr().cast::<c_char>(),
                source.len(),
                filename.as_ptr(),
                flags,
            )
        };
        if unsafe { rqjs_is_exception(value) } != 0 {
            Err(self.take_exception())
        } else {
            Ok(value)
        }
    }

    fn json_to_value(&self, value: &Value) -> Result<JSValue, JsError> {
        let json = serde_json::to_vec(value).map_err(JsError::InvalidJson)?;
        let terminated = nul_terminated(&json);
        let parsed = unsafe {
            rqjs_parse_json(
                self.context,
                terminated.as_ptr().cast::<c_char>(),
                json.len(),
                c"<rust-json>".as_ptr(),
            )
        };
        if unsafe { rqjs_is_exception(parsed) } != 0 {
            Err(self.take_exception())
        } else {
            Ok(parsed)
        }
    }

    fn value_to_json(&self, value: JSValue) -> Result<Value, JsError> {
        let json = unsafe { rqjs_json_stringify(self.context, value) };
        if unsafe { rqjs_is_exception(json) } != 0 {
            return Err(self.take_exception());
        }
        if matches!(
            i64::from(unsafe { rqjs_value_tag(json) }),
            JS_TAG_NULL | JS_TAG_UNDEFINED
        ) {
            unsafe { rqjs_free_value(self.context, json) };
            return Err(JsError::NotJsonSerializable);
        }

        let text = self
            .value_to_string(json)
            .ok_or_else(|| self.take_exception());
        unsafe { rqjs_free_value(self.context, json) };
        let text = text?;
        serde_json::from_str(&text).map_err(JsError::InvalidJson)
    }

    fn value_to_string(&self, value: JSValue) -> Option<String> {
        let mut length = 0usize;
        let pointer = unsafe { JS_ToCStringLen2(self.context, &mut length, value, 0) };
        if pointer.is_null() {
            return None;
        }
        let bytes = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), length) };
        let output = String::from_utf8_lossy(bytes).into_owned();
        unsafe { JS_FreeCString(self.context, pointer) };
        Some(output)
    }

    fn take_exception(&self) -> JsError {
        let exception = unsafe { JS_GetException(self.context) };
        let mut message = self
            .value_to_string(exception)
            .unwrap_or_else(|| String::from("unknown exception"));

        let stack = unsafe { JS_GetPropertyStr(self.context, exception, c"stack".as_ptr()) };
        if unsafe { rqjs_is_exception(stack) } == 0
            && !matches!(
                i64::from(unsafe { rqjs_value_tag(stack) }),
                JS_TAG_NULL | JS_TAG_UNDEFINED
            )
        {
            if let Some(stack_text) = self.value_to_string(stack) {
                if !stack_text.is_empty() && stack_text != message {
                    message.push('\n');
                    message.push_str(&stack_text);
                }
            }
        }
        unsafe {
            rqjs_free_value(self.context, stack);
            rqjs_free_value(self.context, exception);
        }
        JsError::Exception(message)
    }
}

impl Drop for JsEngine {
    fn drop(&mut self) {
        unsafe {
            if !self.context.is_null() {
                JS_SetContextOpaque(self.context, ptr::null_mut());
                JS_FreeContext(self.context);
                self.context = ptr::null_mut();
            }
            if !self.runtime.is_null() {
                JS_FreeRuntime(self.runtime);
                self.runtime = ptr::null_mut();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{JsEngine, normalize_module_specifier};

    #[test]
    fn normalizes_relative_module_paths() {
        assert_eq!(
            normalize_module_specifier("/qjs/widlib/widgets/index.mjs", "../tree.mjs"),
            "/qjs/widlib/tree.mjs"
        );
        assert_eq!(
            normalize_module_specifier("/qjs/entry.mjs", "parse5"),
            "/qjs/parse5/parse5.mjs"
        );
    }

    #[test]
    fn evaluates_json_and_calls_global_functions() {
        let mut engine = JsEngine::new().expect("engine starts");
        assert_eq!(
            engine
                .eval_json("({ answer: 6 * 7 })", "<test>")
                .expect("JSON result"),
            json!({ "answer": 42 })
        );
        engine
            .eval_void(
                "globalThis.add = (left, right) => ({ sum: left + right });",
                "<install-add>",
            )
            .expect("function installs");
        assert_eq!(
            engine
                .call_global_json("add", &[json!(20), json!(22)])
                .expect("function call"),
            json!({ "sum": 42 })
        );
    }

    #[test]
    fn reports_javascript_stack_context() {
        let mut engine = JsEngine::new().expect("engine starts");
        let error = engine
            .eval_void("throw new Error('proof failure')", "proof.js")
            .expect_err("script throws")
            .to_string();
        assert!(error.contains("proof failure"));
        assert!(error.contains("proof.js"));
    }

    #[test]
    fn exposes_json_rust_callbacks_as_javascript_functions() {
        let mut engine = JsEngine::new().expect("engine starts");
        engine
            .register_json_function("hostSum", 2, |arguments| {
                let left = arguments
                    .first()
                    .and_then(serde_json::Value::as_i64)
                    .ok_or_else(|| String::from("left must be an integer"))?;
                let right = arguments
                    .get(1)
                    .and_then(serde_json::Value::as_i64)
                    .ok_or_else(|| String::from("right must be an integer"))?;
                Ok(json!({ "sum": left + right }))
            })
            .expect("host function registers");
        assert_eq!(
            engine
                .eval_json("hostSum(20, 22)", "host-proof.js")
                .expect("host callback runs"),
            json!({ "sum": 42 })
        );

        let error = engine
            .eval_void("hostSum('bad', 2)", "host-error.js")
            .expect_err("Rust error becomes JS exception")
            .to_string();
        assert!(error.contains("left must be an integer"));
    }
}
