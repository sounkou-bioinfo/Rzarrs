
<!-- README.md is generated from README.Rmd. Please edit that file -->

# Rzarrs

<!-- badges: start -->

[![R-CMD-check](https://github.com/sounkou-bioinfo/Rzarrs/actions/workflows/R-CMD-check.yaml/badge.svg)](https://github.com/sounkou-bioinfo/Rzarrs/actions/workflows/R-CMD-check.yaml)
[![Rzarrs status
badge](https://sounkou-bioinfo.r-universe.dev/Rzarrs/badges/version)](https://sounkou-bioinfo.r-universe.dev/Rzarrs)
<!-- badges: end -->

R bindings to the [`zarrs`](https://github.com/zarrs/zarrs) Rust library
for reading Zarr V3 (and compatible Zarr V2) stores — from local disk,
over HTTP/HTTPS, or from S3 / GCS / Azure object storage.

## Overview

Rzarrs exposes three R6-style reference objects:

| Object            | Purpose                                                               |
|-------------------|-----------------------------------------------------------------------|
| `ZarrStore`       | Local filesystem store                                                |
| `ZarrObjectStore` | S3, GCS, Azure Blob, HTTP/HTTPS, or any `object_store`-compatible URL |
| `ZarrArray`       | A single array within any store                                       |

Zarr dtypes are mapped to R types automatically:

| Zarr dtype                 | R type    | Notes                      |
|----------------------------|-----------|----------------------------|
| `float32` / `float64`      | `double`  | NaN → `NA_real_`           |
| `int8` / `int16` / `int32` | `integer` | `i32::MIN` → `NA_integer_` |
| `int64`                    | `double`  | exact to 2^53              |
| `uint8` / `uint16`         | `integer` | always fits                |
| `uint32` / `uint64`        | `double`  |                            |
| `bool`                     | `logical` |                            |

Indices are **1-based and inclusive** on both ends — the same convention
as all other R array operations.

SIMD-accelerated codec paths (gzip, zstd, blosc, crc32c) are selected
automatically at runtime by the underlying Rust dependency crates.

## Installation

``` r
install.packages(
  "Rzarrs",
  repos = c(
    "https://sounkou-bioinfo.r-universe.dev",
    "https://cloud.r-project.org"
  )
)
```

A Rust toolchain (cargo + rustc \>= 1.82) is required at install time.

## Local store

``` r
library(Rzarrs)

# The package ships a tiny bundled fixture for illustration
path <- system.file("testdata", "int32.zarr", package = "Rzarrs")

store <- ZarrStore$open(path)
store$path()
#> [1] "/usr/local/lib/R/site-library/Rzarrs/testdata/int32.zarr"

arr <- ZarrArray$open(store, "/")
arr$dtype()
#> [1] "int32"
arr$shape()
#> [1] 4 6
arr$chunk_shape()
#> [1] 2 3

# Retrieve the full array — returns an R integer matrix
data <- arr$retrieve()
dim(data)
#> [1] 4 6
data
#>      [,1] [,2] [,3] [,4] [,5] [,6]
#> [1,]    1    2    3    4    5    6
#> [2,]    7    8    9   10   11   12
#> [3,]   13   14   15   16   17   18
#> [4,]   19   20   21   22   23   24
```

## Subsetting (1-based, inclusive)

``` r
# First two rows, first three columns
sub <- arr$retrieve(starts = c(1L, 1L), ends = c(2L, 3L))
dim(sub)
#> [1] 2 3
sub
#>      [,1] [,2] [,3]
#> [1,]    1    2    3
#> [2,]    7    8    9
```

## Array metadata

``` r
arr$ndim()
#> [1] 2
arr$dimension_names()   # NULL when absent
#> NULL
jsonlite::fromJSON(arr$metadata_json())$data_type
#> [1] "int32"
```

## Remote store (HTTP/HTTPS or S3/GCS/Azure)

`ZarrObjectStore` uses the
[`object_store`](https://docs.rs/object_store) crate and dispatches on
the URL scheme. For plain `https://` URLs no credentials are needed. For
private cloud buckets, set the standard provider environment variables
before calling `open()`:

| Provider     | Env vars                                                                                                       |
|--------------|----------------------------------------------------------------------------------------------------------------|
| AWS S3       | `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_REGION` (+ `AWS_ENDPOINT_URL` for MinIO / custom endpoints) |
| Google Cloud | `GOOGLE_APPLICATION_CREDENTIALS` (or instance metadata)                                                        |
| Azure Blob   | `AZURE_STORAGE_ACCOUNT`, `AZURE_STORAGE_ACCESS_KEY` or `AZURE_CLIENT_ID` + `AZURE_CLIENT_SECRET`               |

``` r
# Public HTTPS endpoint — no credentials needed
os   <- ZarrObjectStore$open(
  "https://uk1s3.embassy.ebi.ac.uk/idr/zarr/v0.4/idr0062A/6001240.zarr"
)
oarr <- ZarrArray$open_object_store(os, "/0")
oarr$dtype()
oarr$shape()   # t, c, z, y, x

# Retrieve a small spatial patch: first time point, first channel, first Z,
# first 8 pixels in Y — indices are 1-based inclusive
patch <- oarr$retrieve(c(1L, 1L, 1L, 1L, 1L), c(1L, 1L, 1L, 8L, 8L))
dim(patch)
range(patch)
```

For private buckets it is the same call — just set the credentials
first:

``` r
Sys.setenv(
  AWS_ACCESS_KEY_ID     = "...",
  AWS_SECRET_ACCESS_KEY = "...",
  AWS_REGION            = "us-east-1"
)
os <- ZarrObjectStore$open("s3://my-bucket/path/to/array.zarr")
```

## License

GPL (\>= 3)
