#include "quickjs.h"

int rqjs_is_exception(JSValue value) {
    return JS_IsException(value);
}

int rqjs_value_tag(JSValue value) {
    return JS_VALUE_GET_TAG(value);
}

void *rqjs_value_ptr(JSValue value) {
    return JS_VALUE_GET_PTR(value);
}

void rqjs_free_value(JSContext *ctx, JSValue value) {
    JS_FreeValue(ctx, value);
}

void rqjs_throw_module_error(JSContext *ctx, const char *message) {
    JS_ThrowReferenceError(ctx, "%s", message);
}

JSValue rqjs_parse_json(JSContext *ctx, const char *input, size_t input_len,
                        const char *filename) {
    return JS_ParseJSON(ctx, input, input_len, filename);
}

JSValue rqjs_json_stringify(JSContext *ctx, JSValueConst value) {
    return JS_JSONStringify(ctx, value, JS_UNDEFINED, JS_UNDEFINED);
}

int rqjs_is_function(JSContext *ctx, JSValueConst value) {
    return JS_IsFunction(ctx, value);
}

JSValue rqjs_new_c_function_magic(JSContext *ctx, JSCFunctionMagic *function,
                                  const char *name, int length, int magic) {
    return JS_NewCFunctionMagic(ctx, function, name, length,
                                JS_CFUNC_generic_magic, magic);
}

JSValue rqjs_throw_host_error(JSContext *ctx, const char *message) {
    return JS_ThrowInternalError(ctx, "%s", message);
}
