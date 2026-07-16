use std::ffi::{c_char, c_int, c_void};

#[cfg(not(target_pointer_width = "64"))]
compile_error!("RustQJSDom currently supports 64-bit QuickJS hosts only");

pub(crate) const JS_TAG_MODULE: i64 = -3;
pub(crate) const JS_TAG_NULL: i64 = 2;
pub(crate) const JS_TAG_UNDEFINED: i64 = 3;
pub(crate) const JS_EVAL_TYPE_GLOBAL: c_int = 0;
pub(crate) const JS_EVAL_TYPE_MODULE: c_int = 1;
pub(crate) const JS_EVAL_FLAG_COMPILE_ONLY: c_int = 1 << 5;

#[repr(C)]
pub(crate) struct JSRuntime {
    _private: [u8; 0],
}

#[repr(C)]
pub(crate) struct JSContext {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) union JSValueUnion {
    pub(crate) uint64: u64,
    pub(crate) float64: f64,
    pub(crate) ptr: *mut c_void,
    pub(crate) short_big_int: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct JSValue {
    pub(crate) u: JSValueUnion,
    pub(crate) tag: i64,
}

pub(crate) type JSValueConst = JSValue;

pub(crate) type JSCFunctionMagic = unsafe extern "C" fn(
    ctx: *mut JSContext,
    this_value: JSValueConst,
    argc: c_int,
    argv: *const JSValueConst,
    magic: c_int,
) -> JSValue;

#[repr(C)]
pub(crate) struct JSModuleDef {
    _private: [u8; 0],
}

pub(crate) type JSModuleNormalizeFunc = unsafe extern "C" fn(
    ctx: *mut JSContext,
    module_base_name: *const c_char,
    module_name: *const c_char,
    opaque: *mut c_void,
) -> *mut c_char;

pub(crate) type JSModuleLoaderFunc = unsafe extern "C" fn(
    ctx: *mut JSContext,
    module_name: *const c_char,
    opaque: *mut c_void,
) -> *mut JSModuleDef;

unsafe extern "C" {
    pub(crate) fn JS_NewRuntime() -> *mut JSRuntime;
    pub(crate) fn JS_FreeRuntime(rt: *mut JSRuntime);
    pub(crate) fn JS_NewContext(rt: *mut JSRuntime) -> *mut JSContext;
    pub(crate) fn JS_FreeContext(ctx: *mut JSContext);
    pub(crate) fn JS_GetContextOpaque(ctx: *mut JSContext) -> *mut c_void;
    pub(crate) fn JS_SetContextOpaque(ctx: *mut JSContext, opaque: *mut c_void);
    pub(crate) fn JS_SetMemoryLimit(rt: *mut JSRuntime, limit: usize);
    pub(crate) fn JS_SetMaxStackSize(rt: *mut JSRuntime, stack_size: usize);
    pub(crate) fn JS_SetModuleLoaderFunc(
        rt: *mut JSRuntime,
        module_normalize: Option<JSModuleNormalizeFunc>,
        module_loader: Option<JSModuleLoaderFunc>,
        opaque: *mut c_void,
    );
    pub(crate) fn JS_Eval(
        ctx: *mut JSContext,
        input: *const c_char,
        input_len: usize,
        filename: *const c_char,
        eval_flags: c_int,
    ) -> JSValue;
    pub(crate) fn JS_GetGlobalObject(ctx: *mut JSContext) -> JSValue;
    pub(crate) fn JS_GetPropertyStr(
        ctx: *mut JSContext,
        this_obj: JSValueConst,
        prop: *const c_char,
    ) -> JSValue;
    pub(crate) fn JS_SetPropertyStr(
        ctx: *mut JSContext,
        this_obj: JSValueConst,
        prop: *const c_char,
        value: JSValue,
    ) -> c_int;
    pub(crate) fn JS_Call(
        ctx: *mut JSContext,
        func_obj: JSValueConst,
        this_obj: JSValueConst,
        argc: c_int,
        argv: *const JSValueConst,
    ) -> JSValue;
    pub(crate) fn JS_GetException(ctx: *mut JSContext) -> JSValue;
    pub(crate) fn JS_ToCStringLen2(
        ctx: *mut JSContext,
        len: *mut usize,
        value: JSValueConst,
        cesu8: c_int,
    ) -> *const c_char;
    pub(crate) fn JS_FreeCString(ctx: *mut JSContext, value: *const c_char);
    pub(crate) fn js_malloc(ctx: *mut JSContext, size: usize) -> *mut c_void;

    pub(crate) fn rqjs_is_exception(value: JSValue) -> c_int;
    pub(crate) fn rqjs_value_tag(value: JSValue) -> c_int;
    pub(crate) fn rqjs_value_ptr(value: JSValue) -> *mut c_void;
    pub(crate) fn rqjs_free_value(ctx: *mut JSContext, value: JSValue);
    pub(crate) fn rqjs_throw_module_error(ctx: *mut JSContext, message: *const c_char);
    pub(crate) fn rqjs_parse_json(
        ctx: *mut JSContext,
        input: *const c_char,
        input_len: usize,
        filename: *const c_char,
    ) -> JSValue;
    pub(crate) fn rqjs_json_stringify(ctx: *mut JSContext, value: JSValueConst) -> JSValue;
    pub(crate) fn rqjs_is_function(ctx: *mut JSContext, value: JSValueConst) -> c_int;
    pub(crate) fn rqjs_new_c_function_magic(
        ctx: *mut JSContext,
        function: Option<JSCFunctionMagic>,
        name: *const c_char,
        length: c_int,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn rqjs_throw_host_error(ctx: *mut JSContext, message: *const c_char) -> JSValue;
}
