
// clang-format sorts includes unless SortIncludes: Never. However, the ordering
// does matter here. So, we need to disable clang-format for safety.

// clang-format off
#include <stdint.h>
#include <Rinternals.h>
#include <R_ext/Parse.h>
// clang-format on

#include "rust/api.h"

static uintptr_t TAGGED_POINTER_MASK = (uintptr_t)1;

SEXP handle_result(SEXP res_) {
    uintptr_t res = (uintptr_t)res_;

    // An error is indicated by tag.
    if ((res & TAGGED_POINTER_MASK) == 1) {
        // Remove tag
        SEXP res_aligned = (SEXP)(res & ~TAGGED_POINTER_MASK);

        // Currently, there are two types of error cases:
        //
        //   1. Error from Rust code
        //   2. Error from R's C API, which is caught by R_UnwindProtect()
        //
        if (TYPEOF(res_aligned) == CHARSXP) {
            // In case 1, the result is an error message that can be passed to
            // Rf_errorcall() directly.
            Rf_errorcall(R_NilValue, "%s", CHAR(res_aligned));
        } else {
            // In case 2, the result is the token to restart the
            // cleanup process on R's side.
            R_ContinueUnwind(res_aligned);
        }
    }

    return (SEXP)res;
}


SEXP savvy_ZarrArray_chunk_shape__impl(SEXP self__) {
    SEXP res = savvy_ZarrArray_chunk_shape__ffi(self__);
    return handle_result(res);
}

SEXP savvy_ZarrArray_dimension_names__impl(SEXP self__) {
    SEXP res = savvy_ZarrArray_dimension_names__ffi(self__);
    return handle_result(res);
}

SEXP savvy_ZarrArray_dtype__impl(SEXP self__) {
    SEXP res = savvy_ZarrArray_dtype__ffi(self__);
    return handle_result(res);
}

SEXP savvy_ZarrArray_metadata_json__impl(SEXP self__) {
    SEXP res = savvy_ZarrArray_metadata_json__ffi(self__);
    return handle_result(res);
}

SEXP savvy_ZarrArray_ndim__impl(SEXP self__) {
    SEXP res = savvy_ZarrArray_ndim__ffi(self__);
    return handle_result(res);
}

SEXP savvy_ZarrArray_open__impl(SEXP c_arg__store, SEXP c_arg__path) {
    SEXP res = savvy_ZarrArray_open__ffi(c_arg__store, c_arg__path);
    return handle_result(res);
}

SEXP savvy_ZarrArray_open_http__impl(SEXP c_arg__store, SEXP c_arg__path) {
    SEXP res = savvy_ZarrArray_open_http__ffi(c_arg__store, c_arg__path);
    return handle_result(res);
}

SEXP savvy_ZarrArray_open_object_store__impl(SEXP c_arg__store, SEXP c_arg__path) {
    SEXP res = savvy_ZarrArray_open_object_store__ffi(c_arg__store, c_arg__path);
    return handle_result(res);
}

SEXP savvy_ZarrArray_retrieve__impl(SEXP self__, SEXP c_arg__starts, SEXP c_arg__ends) {
    SEXP res = savvy_ZarrArray_retrieve__ffi(self__, c_arg__starts, c_arg__ends);
    return handle_result(res);
}

SEXP savvy_ZarrArray_shape__impl(SEXP self__) {
    SEXP res = savvy_ZarrArray_shape__ffi(self__);
    return handle_result(res);
}

SEXP savvy_ZarrHttpStore_open__impl(SEXP c_arg__url) {
    SEXP res = savvy_ZarrHttpStore_open__ffi(c_arg__url);
    return handle_result(res);
}

SEXP savvy_ZarrHttpStore_url__impl(SEXP self__) {
    SEXP res = savvy_ZarrHttpStore_url__ffi(self__);
    return handle_result(res);
}

SEXP savvy_ZarrObjectStore_open__impl(SEXP c_arg__url) {
    SEXP res = savvy_ZarrObjectStore_open__ffi(c_arg__url);
    return handle_result(res);
}

SEXP savvy_ZarrObjectStore_url__impl(SEXP self__) {
    SEXP res = savvy_ZarrObjectStore_url__ffi(self__);
    return handle_result(res);
}

SEXP savvy_ZarrStore_open__impl(SEXP c_arg__path) {
    SEXP res = savvy_ZarrStore_open__ffi(c_arg__path);
    return handle_result(res);
}

SEXP savvy_ZarrStore_path__impl(SEXP self__) {
    SEXP res = savvy_ZarrStore_path__ffi(self__);
    return handle_result(res);
}


static const R_CallMethodDef CallEntries[] = {

    {"savvy_ZarrArray_chunk_shape__impl", (DL_FUNC) &savvy_ZarrArray_chunk_shape__impl, 1},
    {"savvy_ZarrArray_dimension_names__impl", (DL_FUNC) &savvy_ZarrArray_dimension_names__impl, 1},
    {"savvy_ZarrArray_dtype__impl", (DL_FUNC) &savvy_ZarrArray_dtype__impl, 1},
    {"savvy_ZarrArray_metadata_json__impl", (DL_FUNC) &savvy_ZarrArray_metadata_json__impl, 1},
    {"savvy_ZarrArray_ndim__impl", (DL_FUNC) &savvy_ZarrArray_ndim__impl, 1},
    {"savvy_ZarrArray_open__impl", (DL_FUNC) &savvy_ZarrArray_open__impl, 2},
    {"savvy_ZarrArray_open_http__impl", (DL_FUNC) &savvy_ZarrArray_open_http__impl, 2},
    {"savvy_ZarrArray_open_object_store__impl", (DL_FUNC) &savvy_ZarrArray_open_object_store__impl, 2},
    {"savvy_ZarrArray_retrieve__impl", (DL_FUNC) &savvy_ZarrArray_retrieve__impl, 3},
    {"savvy_ZarrArray_shape__impl", (DL_FUNC) &savvy_ZarrArray_shape__impl, 1},
    {"savvy_ZarrHttpStore_open__impl", (DL_FUNC) &savvy_ZarrHttpStore_open__impl, 1},
    {"savvy_ZarrHttpStore_url__impl", (DL_FUNC) &savvy_ZarrHttpStore_url__impl, 1},
    {"savvy_ZarrObjectStore_open__impl", (DL_FUNC) &savvy_ZarrObjectStore_open__impl, 1},
    {"savvy_ZarrObjectStore_url__impl", (DL_FUNC) &savvy_ZarrObjectStore_url__impl, 1},
    {"savvy_ZarrStore_open__impl", (DL_FUNC) &savvy_ZarrStore_open__impl, 1},
    {"savvy_ZarrStore_path__impl", (DL_FUNC) &savvy_ZarrStore_path__impl, 1},
    {NULL, NULL, 0}
};

void R_init_Rzarrs(DllInfo *dll) {
    R_registerRoutines(dll, NULL, CallEntries, NULL, NULL);
    R_useDynamicSymbols(dll, FALSE);

    // Functions for initialization, if any.

}
