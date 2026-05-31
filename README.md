
<!-- README.md is generated from README.Rmd. Please edit that file -->

# Rzarrs

<!-- badges: start -->

[![R-CMD-check](https://github.com/sounkou-bioinfo/Rzarrs/actions/workflows/R-CMD-check.yaml/badge.svg)](https://github.com/sounkou-bioinfo/Rzarrs/actions/workflows/R-CMD-check.yaml)
[![Rzarrs status
badge](https://sounkou-bioinfo.r-universe.dev/Rzarrs/badges/version)](https://sounkou-bioinfo.r-universe.dev/Rzarrs)
<!-- badges: end -->

R bindings to the [`zarrs`](https://github.com/zarrs/zarrs) Rust library
for reading Zarr V3 (and compatible Zarr V2) stores — from local disk or
over HTTP/HTTPS.

## Overview

Rzarrs exposes three R6-style reference objects:

| Object            | Purpose                                                       |
|-------------------|---------------------------------------------------------------|
| `ZarrStore`       | Local filesystem store                                        |
| `ZarrHttpStore`   | Remote store over plain HTTP/HTTPS (blocking reqwest)         |
| `ZarrObjectStore` | S3, GCS, Azure Blob, or any `object_store`-compatible backend |
| `ZarrArray`       | A single array within any store                               |

Zarr dtypes are mapped to R types automatically:

| Zarr dtype                 | R type    | Notes                      |
|----------------------------|-----------|----------------------------|
| `float32` / `float64`      | `double`  | NaN → `NA_real_`           |
| `int8` / `int16` / `int32` | `integer` | `i32::MIN` → `NA_integer_` |
| `int64`                    | `double`  | exact to 2^53              |
| `uint8` / `uint16`         | `integer` | always fits                |
| `uint32` / `uint64`        | `double`  |                            |
| `bool`                     | `logical` |                            |

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

# Retrieve the full array — returns an R integer vector with a dim attribute
data <- arr$retrieve(NULL, NULL)
dim(data)
#> [1] 4 6
data
#>      [,1] [,2] [,3] [,4] [,5] [,6]
#> [1,]    1   10    9   13   22   21
#> [2,]    7    5   11   19   17   23
#> [3,]    2    8    6   14   20   18
#> [4,]    4    3   12   16   15   24
```

## Subsetting (0-based, exclusive end)

``` r
# First two rows, first three columns
sub <- arr$retrieve(starts = c(0L, 0L), ends = c(2L, 3L))
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

## Remote store over HTTP/HTTPS

Zarr V2 OME-Zarr microscopy data from the [Image Data
Resource](https://idr.openmicroscopy.org/):

``` r
hs  <- ZarrHttpStore$open(
  "https://uk1s3.embassy.ebi.ac.uk/idr/zarr/v0.4/idr0062A/6001240.zarr"
)
hs$url()
#> [1] "https://uk1s3.embassy.ebi.ac.uk/idr/zarr/v0.4/idr0062A/6001240.zarr"

img <- ZarrArray$open_http(hs, "/0")
img$dtype()
#> [1] "uint16"
img$shape()   # t, c, z, y, x
#> [1]   2 236 275 271

# Fetch a small spatial patch: first time point, first channel, first Z slice
patch <- img$retrieve(
  starts = c(0L, 0L, 0L, 0L),
  ends   = c(1L, 1L, 1L, 64L)
)
dim(patch)
#> [1]  1  1  1 64
range(patch)
#> [1]  7 15
```

## Object store (S3 / GCS / Azure / any `object_store` backend)

`ZarrObjectStore` uses the
[`object_store`](https://docs.rs/object_store) crate and dispatches on
the URL scheme. Credentials are **never passed as R arguments** — set
the standard provider environment variables in your R session before
calling `open()`:

| Provider     | Env vars                                                                                             |
|--------------|------------------------------------------------------------------------------------------------------|
| AWS S3       | `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_REGION` (+ `AWS_ENDPOINT_URL` for MinIO / custom) |
| Google Cloud | `GOOGLE_APPLICATION_CREDENTIALS` (or instance metadata)                                              |
| Azure Blob   | `AZURE_STORAGE_ACCOUNT`, `AZURE_STORAGE_ACCESS_KEY` or `AZURE_CLIENT_ID` + `AZURE_CLIENT_SECRET`     |

``` r
# Public HTTPS endpoint — no credentials needed
os   <- ZarrObjectStore$open(
  "https://uk1s3.embassy.ebi.ac.uk/idr/zarr/v0.4/idr0062A/6001240.zarr"
)
oarr <- ZarrArray$open_object_store(os, "/0")
oarr$dtype()
#> [1] "uint16"
oarr$shape()
#> [1]   2 236 275 271

# Retrieve a small patch
patch <- oarr$retrieve(c(0L, 0L, 0L, 0L), c(1L, 1L, 1L, 8L))
as.vector(patch)
#> [1]  8  9  8 10  8 11  9  9
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
