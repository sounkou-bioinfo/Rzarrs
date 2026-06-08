# Reader spec compliance status

This document tracks Rzarrs reader compliance work beyond basic primitive dtype
materialization. It is intentionally implementation-facing: each item should be
updated as support, tests, or explicit non-support decisions land.

## Completed / available

- Open local filesystem Zarr stores.
- Open object-store backed Zarr stores via `ZarrObjectStore` (`file://`, HTTP,
  HTTPS, S3; GCS/Azure when enabled at source-install time).
- Open local `.zarr.zip` / `.zip` archives through `ZarrStore$open()` for generic
  `ZarrArray` and `ZarrGroup` use.
- Inspect array shape, ndim, dtype, dtype plan, chunk shape, dimension names, and
  metadata JSON/list.
- Inspect group attributes and children when the store supports listing.
- Read dense rectangular hyperslabs with 1-based inclusive `starts`/`ends`.
- Materialize current supported primitive/exotic dtypes documented in README.
- Report codec capabilities globally and per array via `codec_capabilities()`,
  `arr$codecs()`, and `arr$codec_capabilities()`.
- Compile common codecs including `bytes`, `gzip`, `zstd`, `crc32c`,
  `sharding_indexed`, `transpose`, and `blosc`.

## Remaining work

### 1. Richer selection API

Current support is rectangular hyperslabs only.

Missing:

- strides;
- per-axis integer vectors;
- point/fancy indexing;
- boolean masks;
- partial selectors;
- R-like `drop` behavior;
- direct chunk/subchunk retrieval API.

### 2. Streaming / chunk iteration

Current reads fully materialize the selected region in memory.

Missing:

- iterate chunks;
- callback-per-chunk API;
- streaming reads into user-provided sinks;
- lazy/ALTREP array surface;
- memory-budgeted reads.

### 3. Bytes / raw dtype materialization

Still unresolved:

- fixed-width bytes representation:
  - raw array with an extra byte dimension, or
  - list array/list column of raw vectors;
- vlen bytes as list of raw vectors;
- bytes fields inside struct/list-column representations.

### 4. Nested dtype materialization

Still planned only:

- `optional[...]`;
- `list[...]` / `varlen[...]`;
- `struct[...]` / `struct{...}`.

Needs:

- validity masks;
- offsets;
- child-array semantics;
- data.frame/list-column representation;
- exact mapping for nullable fields.

### 5. Extension dtype materializers

Dtype plans exist, but many extension dtypes still require registered
binary-layout materializers before actual reads can work.

Remaining:

- `float128`;
- `decimal128`;
- `decimal256`;
- unknown/plugin dtypes;
- plugin registry or explicit materializer table.

High-precision extension dtypes currently have a planned lossy-double policy, but
actual reading still depends on a registered binary-layout data type materializer
because these are extension dtypes, not core built-in `zarrs` element types.

### 6. Storage transformers

Zarr V3 storage transformers are not surfaced or tested as a user-facing feature.

Remaining:

- transformer capability reporting;
- transformer support tests;
- explicit errors when a transformer is unsupported;
- registry design if custom transformers are needed.

### 7. Codec compliance hardening

Blosc and capability reporting are available, but fixture coverage should be
expanded.

Still useful:

- actual blosc-compressed array fixtures/tests;
- sharding fixtures/tests beyond indirect coverage;
- unsupported codec error tests;
- verification of nested codec extraction from sharding metadata;
- capability table distinction between compiled-in codecs and codecs detected in
  array metadata.

### 8. Metadata fidelity

`metadata_json()` preserves exact JSON text. `metadata()` converts JSON numbers to
R doubles.

Potential compliance gap:

- huge JSON integers may lose precision in the native R list metadata view.

Possible fixes:

- expose integer-like metadata numbers as character;
- preserve JSON scalar wrapper classes;
- document `metadata_json()` as the exact path for metadata-number fidelity.

### 9. Dimension names / attributes edge cases

Basic support exists, but fixture coverage should be expanded for:

- named dimensions;
- empty attributes;
- nested attributes;
- non-ASCII attributes;
- very large metadata values;
- Zarr v2/v3 metadata variants.

### 10. Store/listing edge cases

Need more tests for:

- HTTP stores without list support;
- object stores with prefixes;
- URL path normalization;
- stores where direct array open works but group listing does not;
- consolidated metadata, if support is planned.

### 11. Fill-value / missing-value semantics

Needs broader fixture coverage:

- absent chunks filled correctly for all core dtypes;
- float fill values including NaN;
- bool/string fill values;
- int sentinel distinction versus R missing;
- complex fill values.

### 12. Ordering / chunk layout edge cases

Need fixtures for:

- F-order / transpose codec interactions if supported by `zarrs`;
- non-2D arrays;
- scalar arrays;
- zero-length dimensions;
- non-regular grids, if relevant;
- chunk shapes that do not divide array shape.

## Suggested priority

1. Bytes/raw materialization.
2. Nested dtype materialization.
3. Streaming/chunk iterator API.
4. Fixture-based codec/fill-value compliance tests.
5. Richer selection API.
6. Metadata fidelity policy for `metadata()`.
