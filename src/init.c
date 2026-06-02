
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

SEXP savvy_rzarrs_int64_is_na__impl(SEXP c_arg__x) {
    SEXP res = savvy_rzarrs_int64_is_na__ffi(c_arg__x);
    return handle_result(res);
}

SEXP savvy_rzarrs_int64_math__impl(SEXP c_arg__x, SEXP c_arg__op) {
    SEXP res = savvy_rzarrs_int64_math__ffi(c_arg__x, c_arg__op);
    return handle_result(res);
}

SEXP savvy_rzarrs_int64_op__impl(SEXP c_arg__x, SEXP c_arg__y, SEXP c_arg__op) {
    SEXP res = savvy_rzarrs_int64_op__ffi(c_arg__x, c_arg__y, c_arg__op);
    return handle_result(res);
}

SEXP savvy_rzarrs_int64_summary__impl(SEXP c_arg__x, SEXP c_arg__op, SEXP c_arg___na_rm) {
    SEXP res = savvy_rzarrs_int64_summary__ffi(c_arg__x, c_arg__op, c_arg___na_rm);
    return handle_result(res);
}

SEXP savvy_rzarrs_int64_to_double__impl(SEXP c_arg__x) {
    SEXP res = savvy_rzarrs_int64_to_double__ffi(c_arg__x);
    return handle_result(res);
}

SEXP savvy_rzarrs_int64_values__impl(SEXP c_arg__x) {
    SEXP res = savvy_rzarrs_int64_values__ffi(c_arg__x);
    return handle_result(res);
}

SEXP savvy_rzarrs_uint64_is_na__impl(SEXP c_arg__x) {
    SEXP res = savvy_rzarrs_uint64_is_na__ffi(c_arg__x);
    return handle_result(res);
}

SEXP savvy_rzarrs_uint64_math__impl(SEXP c_arg__x, SEXP c_arg__op) {
    SEXP res = savvy_rzarrs_uint64_math__ffi(c_arg__x, c_arg__op);
    return handle_result(res);
}

SEXP savvy_rzarrs_uint64_op__impl(SEXP c_arg__x, SEXP c_arg__y, SEXP c_arg__op) {
    SEXP res = savvy_rzarrs_uint64_op__ffi(c_arg__x, c_arg__y, c_arg__op);
    return handle_result(res);
}

SEXP savvy_rzarrs_uint64_summary__impl(SEXP c_arg__x, SEXP c_arg__op, SEXP c_arg___na_rm) {
    SEXP res = savvy_rzarrs_uint64_summary__ffi(c_arg__x, c_arg__op, c_arg___na_rm);
    return handle_result(res);
}

SEXP savvy_rzarrs_uint64_to_double__impl(SEXP c_arg__x) {
    SEXP res = savvy_rzarrs_uint64_to_double__ffi(c_arg__x);
    return handle_result(res);
}

SEXP savvy_rzarrs_uint64_values__impl(SEXP c_arg__x) {
    SEXP res = savvy_rzarrs_uint64_values__ffi(c_arg__x);
    return handle_result(res);
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

SEXP savvy_ZarrArray_dtype_plan__impl(SEXP self__) {
    SEXP res = savvy_ZarrArray_dtype_plan__ffi(self__);
    return handle_result(res);
}

SEXP savvy_ZarrArray_metadata__impl(SEXP self__) {
    SEXP res = savvy_ZarrArray_metadata__ffi(self__);
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

SEXP savvy_ZarrGroup_attributes__impl(SEXP self__) {
    SEXP res = savvy_ZarrGroup_attributes__ffi(self__);
    return handle_result(res);
}

SEXP savvy_ZarrGroup_attributes_json__impl(SEXP self__) {
    SEXP res = savvy_ZarrGroup_attributes_json__ffi(self__);
    return handle_result(res);
}

SEXP savvy_ZarrGroup_children__impl(SEXP self__, SEXP c_arg__recursive) {
    SEXP res = savvy_ZarrGroup_children__ffi(self__, c_arg__recursive);
    return handle_result(res);
}

SEXP savvy_ZarrGroup_open__impl(SEXP c_arg__store, SEXP c_arg__path) {
    SEXP res = savvy_ZarrGroup_open__ffi(c_arg__store, c_arg__path);
    return handle_result(res);
}

SEXP savvy_ZarrGroup_open_object_store__impl(SEXP c_arg__store, SEXP c_arg__path) {
    SEXP res = savvy_ZarrGroup_open_object_store__ffi(c_arg__store, c_arg__path);
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

SEXP savvy_ZarrVcf_call__impl(SEXP self__, SEXP c_arg__name) {
    SEXP res = savvy_ZarrVcf_call__ffi(self__, c_arg__name);
    return handle_result(res);
}

SEXP savvy_ZarrVcf_call_genotype_phased__impl(SEXP self__, SEXP c_arg__variants, SEXP c_arg__samples) {
    SEXP res = savvy_ZarrVcf_call_genotype_phased__ffi(self__, c_arg__variants, c_arg__samples);
    return handle_result(res);
}

SEXP savvy_ZarrVcf_contigs__impl(SEXP self__) {
    SEXP res = savvy_ZarrVcf_contigs__ffi(self__);
    return handle_result(res);
}

SEXP savvy_ZarrVcf_fields__impl(SEXP self__) {
    SEXP res = savvy_ZarrVcf_fields__ffi(self__);
    return handle_result(res);
}

SEXP savvy_ZarrVcf_filters__impl(SEXP self__) {
    SEXP res = savvy_ZarrVcf_filters__ffi(self__);
    return handle_result(res);
}

SEXP savvy_ZarrVcf_genotypes__impl(SEXP self__, SEXP c_arg__variants, SEXP c_arg__samples) {
    SEXP res = savvy_ZarrVcf_genotypes__ffi(self__, c_arg__variants, c_arg__samples);
    return handle_result(res);
}

SEXP savvy_ZarrVcf_n_samples__impl(SEXP self__) {
    SEXP res = savvy_ZarrVcf_n_samples__ffi(self__);
    return handle_result(res);
}

SEXP savvy_ZarrVcf_n_variants__impl(SEXP self__) {
    SEXP res = savvy_ZarrVcf_n_variants__ffi(self__);
    return handle_result(res);
}

SEXP savvy_ZarrVcf_open__impl(SEXP c_arg__path) {
    SEXP res = savvy_ZarrVcf_open__ffi(c_arg__path);
    return handle_result(res);
}

SEXP savvy_ZarrVcf_open_object_store__impl(SEXP c_arg__url) {
    SEXP res = savvy_ZarrVcf_open_object_store__ffi(c_arg__url);
    return handle_result(res);
}

SEXP savvy_ZarrVcf_open_store__impl(SEXP c_arg__store) {
    SEXP res = savvy_ZarrVcf_open_store__ffi(c_arg__store);
    return handle_result(res);
}

SEXP savvy_ZarrVcf_samples__impl(SEXP self__) {
    SEXP res = savvy_ZarrVcf_samples__ffi(self__);
    return handle_result(res);
}

SEXP savvy_ZarrVcf_variant__impl(SEXP self__, SEXP c_arg__name) {
    SEXP res = savvy_ZarrVcf_variant__ffi(self__, c_arg__name);
    return handle_result(res);
}

SEXP savvy_ZarrVcf_variant_allele__impl(SEXP self__) {
    SEXP res = savvy_ZarrVcf_variant_allele__ffi(self__);
    return handle_result(res);
}

SEXP savvy_ZarrVcf_variant_contig__impl(SEXP self__) {
    SEXP res = savvy_ZarrVcf_variant_contig__ffi(self__);
    return handle_result(res);
}

SEXP savvy_ZarrVcf_variant_position__impl(SEXP self__) {
    SEXP res = savvy_ZarrVcf_variant_position__ffi(self__);
    return handle_result(res);
}

SEXP savvy_ZarrVcf_version__impl(SEXP self__) {
    SEXP res = savvy_ZarrVcf_version__ffi(self__);
    return handle_result(res);
}


static const R_CallMethodDef CallEntries[] = {
    {"savvy_rzarrs_int64_is_na__impl", (DL_FUNC) &savvy_rzarrs_int64_is_na__impl, 1},
    {"savvy_rzarrs_int64_math__impl", (DL_FUNC) &savvy_rzarrs_int64_math__impl, 2},
    {"savvy_rzarrs_int64_op__impl", (DL_FUNC) &savvy_rzarrs_int64_op__impl, 3},
    {"savvy_rzarrs_int64_summary__impl", (DL_FUNC) &savvy_rzarrs_int64_summary__impl, 3},
    {"savvy_rzarrs_int64_to_double__impl", (DL_FUNC) &savvy_rzarrs_int64_to_double__impl, 1},
    {"savvy_rzarrs_int64_values__impl", (DL_FUNC) &savvy_rzarrs_int64_values__impl, 1},
    {"savvy_rzarrs_uint64_is_na__impl", (DL_FUNC) &savvy_rzarrs_uint64_is_na__impl, 1},
    {"savvy_rzarrs_uint64_math__impl", (DL_FUNC) &savvy_rzarrs_uint64_math__impl, 2},
    {"savvy_rzarrs_uint64_op__impl", (DL_FUNC) &savvy_rzarrs_uint64_op__impl, 3},
    {"savvy_rzarrs_uint64_summary__impl", (DL_FUNC) &savvy_rzarrs_uint64_summary__impl, 3},
    {"savvy_rzarrs_uint64_to_double__impl", (DL_FUNC) &savvy_rzarrs_uint64_to_double__impl, 1},
    {"savvy_rzarrs_uint64_values__impl", (DL_FUNC) &savvy_rzarrs_uint64_values__impl, 1},
    {"savvy_ZarrArray_chunk_shape__impl", (DL_FUNC) &savvy_ZarrArray_chunk_shape__impl, 1},
    {"savvy_ZarrArray_dimension_names__impl", (DL_FUNC) &savvy_ZarrArray_dimension_names__impl, 1},
    {"savvy_ZarrArray_dtype__impl", (DL_FUNC) &savvy_ZarrArray_dtype__impl, 1},
    {"savvy_ZarrArray_dtype_plan__impl", (DL_FUNC) &savvy_ZarrArray_dtype_plan__impl, 1},
    {"savvy_ZarrArray_metadata__impl", (DL_FUNC) &savvy_ZarrArray_metadata__impl, 1},
    {"savvy_ZarrArray_metadata_json__impl", (DL_FUNC) &savvy_ZarrArray_metadata_json__impl, 1},
    {"savvy_ZarrArray_ndim__impl", (DL_FUNC) &savvy_ZarrArray_ndim__impl, 1},
    {"savvy_ZarrArray_open__impl", (DL_FUNC) &savvy_ZarrArray_open__impl, 2},
    {"savvy_ZarrArray_open_object_store__impl", (DL_FUNC) &savvy_ZarrArray_open_object_store__impl, 2},
    {"savvy_ZarrArray_retrieve__impl", (DL_FUNC) &savvy_ZarrArray_retrieve__impl, 3},
    {"savvy_ZarrArray_shape__impl", (DL_FUNC) &savvy_ZarrArray_shape__impl, 1},
    {"savvy_ZarrGroup_attributes__impl", (DL_FUNC) &savvy_ZarrGroup_attributes__impl, 1},
    {"savvy_ZarrGroup_attributes_json__impl", (DL_FUNC) &savvy_ZarrGroup_attributes_json__impl, 1},
    {"savvy_ZarrGroup_children__impl", (DL_FUNC) &savvy_ZarrGroup_children__impl, 2},
    {"savvy_ZarrGroup_open__impl", (DL_FUNC) &savvy_ZarrGroup_open__impl, 2},
    {"savvy_ZarrGroup_open_object_store__impl", (DL_FUNC) &savvy_ZarrGroup_open_object_store__impl, 2},
    {"savvy_ZarrObjectStore_open__impl", (DL_FUNC) &savvy_ZarrObjectStore_open__impl, 1},
    {"savvy_ZarrObjectStore_url__impl", (DL_FUNC) &savvy_ZarrObjectStore_url__impl, 1},
    {"savvy_ZarrStore_open__impl", (DL_FUNC) &savvy_ZarrStore_open__impl, 1},
    {"savvy_ZarrStore_path__impl", (DL_FUNC) &savvy_ZarrStore_path__impl, 1},
    {"savvy_ZarrVcf_call__impl", (DL_FUNC) &savvy_ZarrVcf_call__impl, 2},
    {"savvy_ZarrVcf_call_genotype_phased__impl", (DL_FUNC) &savvy_ZarrVcf_call_genotype_phased__impl, 3},
    {"savvy_ZarrVcf_contigs__impl", (DL_FUNC) &savvy_ZarrVcf_contigs__impl, 1},
    {"savvy_ZarrVcf_fields__impl", (DL_FUNC) &savvy_ZarrVcf_fields__impl, 1},
    {"savvy_ZarrVcf_filters__impl", (DL_FUNC) &savvy_ZarrVcf_filters__impl, 1},
    {"savvy_ZarrVcf_genotypes__impl", (DL_FUNC) &savvy_ZarrVcf_genotypes__impl, 3},
    {"savvy_ZarrVcf_n_samples__impl", (DL_FUNC) &savvy_ZarrVcf_n_samples__impl, 1},
    {"savvy_ZarrVcf_n_variants__impl", (DL_FUNC) &savvy_ZarrVcf_n_variants__impl, 1},
    {"savvy_ZarrVcf_open__impl", (DL_FUNC) &savvy_ZarrVcf_open__impl, 1},
    {"savvy_ZarrVcf_open_object_store__impl", (DL_FUNC) &savvy_ZarrVcf_open_object_store__impl, 1},
    {"savvy_ZarrVcf_open_store__impl", (DL_FUNC) &savvy_ZarrVcf_open_store__impl, 1},
    {"savvy_ZarrVcf_samples__impl", (DL_FUNC) &savvy_ZarrVcf_samples__impl, 1},
    {"savvy_ZarrVcf_variant__impl", (DL_FUNC) &savvy_ZarrVcf_variant__impl, 2},
    {"savvy_ZarrVcf_variant_allele__impl", (DL_FUNC) &savvy_ZarrVcf_variant_allele__impl, 1},
    {"savvy_ZarrVcf_variant_contig__impl", (DL_FUNC) &savvy_ZarrVcf_variant_contig__impl, 1},
    {"savvy_ZarrVcf_variant_position__impl", (DL_FUNC) &savvy_ZarrVcf_variant_position__impl, 1},
    {"savvy_ZarrVcf_version__impl", (DL_FUNC) &savvy_ZarrVcf_version__impl, 1},
    {NULL, NULL, 0}
};

void R_init_Rzarrs(DllInfo *dll) {
    R_registerRoutines(dll, NULL, CallEntries, NULL, NULL);
    R_useDynamicSymbols(dll, FALSE);

    // Functions for initialization, if any.

}
