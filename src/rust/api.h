// methods and associated functions for ZarrArray
SEXP savvy_ZarrArray_chunk_shape__ffi(SEXP self__);
SEXP savvy_ZarrArray_dimension_names__ffi(SEXP self__);
SEXP savvy_ZarrArray_dtype__ffi(SEXP self__);
SEXP savvy_ZarrArray_metadata__ffi(SEXP self__);
SEXP savvy_ZarrArray_metadata_json__ffi(SEXP self__);
SEXP savvy_ZarrArray_ndim__ffi(SEXP self__);
SEXP savvy_ZarrArray_open__ffi(SEXP c_arg__store, SEXP c_arg__path);
SEXP savvy_ZarrArray_open_object_store__ffi(SEXP c_arg__store, SEXP c_arg__path);
SEXP savvy_ZarrArray_retrieve__ffi(SEXP self__, SEXP c_arg__starts, SEXP c_arg__ends);
SEXP savvy_ZarrArray_shape__ffi(SEXP self__);

// methods and associated functions for ZarrGroup
SEXP savvy_ZarrGroup_attributes__ffi(SEXP self__);
SEXP savvy_ZarrGroup_attributes_json__ffi(SEXP self__);
SEXP savvy_ZarrGroup_children__ffi(SEXP self__, SEXP c_arg__recursive);
SEXP savvy_ZarrGroup_open__ffi(SEXP c_arg__store, SEXP c_arg__path);
SEXP savvy_ZarrGroup_open_object_store__ffi(SEXP c_arg__store, SEXP c_arg__path);

// methods and associated functions for ZarrObjectStore
SEXP savvy_ZarrObjectStore_open__ffi(SEXP c_arg__url);
SEXP savvy_ZarrObjectStore_url__ffi(SEXP self__);

// methods and associated functions for ZarrStore
SEXP savvy_ZarrStore_open__ffi(SEXP c_arg__path);
SEXP savvy_ZarrStore_path__ffi(SEXP self__);

// methods and associated functions for ZarrVcf
SEXP savvy_ZarrVcf_call__ffi(SEXP self__, SEXP c_arg__name);
SEXP savvy_ZarrVcf_call_genotype_phased__ffi(SEXP self__, SEXP c_arg__variants, SEXP c_arg__samples);
SEXP savvy_ZarrVcf_contigs__ffi(SEXP self__);
SEXP savvy_ZarrVcf_fields__ffi(SEXP self__);
SEXP savvy_ZarrVcf_filters__ffi(SEXP self__);
SEXP savvy_ZarrVcf_genotypes__ffi(SEXP self__, SEXP c_arg__variants, SEXP c_arg__samples);
SEXP savvy_ZarrVcf_n_samples__ffi(SEXP self__);
SEXP savvy_ZarrVcf_n_variants__ffi(SEXP self__);
SEXP savvy_ZarrVcf_open__ffi(SEXP c_arg__path);
SEXP savvy_ZarrVcf_open_object_store__ffi(SEXP c_arg__url);
SEXP savvy_ZarrVcf_open_store__ffi(SEXP c_arg__store);
SEXP savvy_ZarrVcf_samples__ffi(SEXP self__);
SEXP savvy_ZarrVcf_variant__ffi(SEXP self__, SEXP c_arg__name);
SEXP savvy_ZarrVcf_variant_allele__ffi(SEXP self__);
SEXP savvy_ZarrVcf_variant_contig__ffi(SEXP self__);
SEXP savvy_ZarrVcf_variant_position__ffi(SEXP self__);
SEXP savvy_ZarrVcf_version__ffi(SEXP self__);
