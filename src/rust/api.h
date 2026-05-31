// methods and associated functions for ZarrArray
SEXP savvy_ZarrArray_chunk_shape__ffi(SEXP self__);
SEXP savvy_ZarrArray_dimension_names__ffi(SEXP self__);
SEXP savvy_ZarrArray_dtype__ffi(SEXP self__);
SEXP savvy_ZarrArray_metadata_json__ffi(SEXP self__);
SEXP savvy_ZarrArray_ndim__ffi(SEXP self__);
SEXP savvy_ZarrArray_open__ffi(SEXP c_arg__store, SEXP c_arg__path);
SEXP savvy_ZarrArray_open_object_store__ffi(SEXP c_arg__store, SEXP c_arg__path);
SEXP savvy_ZarrArray_retrieve__ffi(SEXP self__, SEXP c_arg__starts, SEXP c_arg__ends);
SEXP savvy_ZarrArray_shape__ffi(SEXP self__);

// methods and associated functions for ZarrObjectStore
SEXP savvy_ZarrObjectStore_open__ffi(SEXP c_arg__url);
SEXP savvy_ZarrObjectStore_url__ffi(SEXP self__);

// methods and associated functions for ZarrStore
SEXP savvy_ZarrStore_open__ffi(SEXP c_arg__path);
SEXP savvy_ZarrStore_path__ffi(SEXP self__);
