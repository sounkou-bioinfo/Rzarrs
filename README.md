
<!-- README.md is generated from README.Rmd. Please edit that file -->

# Rzarrs

<!-- badges: start -->

[![R-CMD-check](https://github.com/sounkou-bioinfo/Rzarrs/actions/workflows/R-CMD-check.yaml/badge.svg)](https://github.com/sounkou-bioinfo/Rzarrs/actions/workflows/R-CMD-check.yaml)
[![Rzarrs status
badge](https://sounkou-bioinfo.r-universe.dev/Rzarrs/badges/version)](https://sounkou-bioinfo.r-universe.dev/Rzarrs)
<!-- badges: end -->

R bindings to the [`zarrs`](https://github.com/zarrs/zarrs) Rust library
for reading Zarr V3 (and compatible Zarr V2) stores. The current package
is reader-first: local filesystems, HTTP/HTTPS, S3, and local
`.zarr.zip` VCF archives are supported by default. GCS and Azure Blob
can be enabled when installing from source.

## Overview

Rzarrs exposes four savvy-backed reference objects:

| Object            | Purpose                                                                          |
|-------------------|----------------------------------------------------------------------------------|
| `ZarrStore`       | Local filesystem store                                                           |
| `ZarrObjectStore` | HTTP/HTTPS and S3 by default; GCS/Azure Blob when enabled at source-install time |
| `ZarrGroup`       | A group node within a store (attributes + child listing)                         |
| `ZarrArray`       | A single array within any store                                                  |

Zarr dtypes are mapped to R types automatically:

| Zarr dtype                 | R type      | Notes                                        |
|----------------------------|-------------|----------------------------------------------|
| `float32` / `float64`      | `double`    | NaN, Inf, -Inf preserved as-is               |
| `int8` / `int16` / `int32` | `integer`   | `i32::MIN` → `NA_integer_`                   |
| `int64`                    | `double`    | exact to 2^53                                |
| `uint8` / `uint16`         | `integer`   | always fits                                  |
| `uint32` / `uint64`        | `double`    |                                              |
| `bool`                     | `logical`   |                                              |
| `string`                   | `character` | variable-length UTF-8 via `vlen-bytes` codec |

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

A Rust toolchain (cargo + rustc \>= 1.82) and GNU Make are required at
install time.

Default Rust features are `aws` and `zip`: HTTP/HTTPS, S3, and local
`.zarr.zip` VCF Zarr archives work out of the box. Source installs can
enable more providers with configure arguments:

``` r
# Enable GCS in addition to defaults
install.packages(
  "Rzarrs",
  repos = c("https://sounkou-bioinfo.r-universe.dev", "https://cloud.r-project.org"),
  configure.args = "--enable-gcp"
)

# Enable all cloud providers: AWS, GCS, and Azure Blob
install.packages(
  "Rzarrs",
  repos = c("https://sounkou-bioinfo.r-universe.dev", "https://cloud.r-project.org"),
  configure.args = "--enable-all-cloud"
)

# Exact Cargo feature control; comma or space separated features are accepted
install.packages(
  "Rzarrs",
  type = "source",
  configure.args = "--without-default-rust-features --with-rust-features=aws,gcp,azure,zip"
)
```

Equivalent environment-variable control is also supported:

``` sh
SAVVY_FEATURES="aws gcp azure zip" R CMD INSTALL Rzarrs_0.1.0.tar.gz
```

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
arr$metadata()$data_type
#> [1] "int32"
arr$metadata()$shape
#> [1] 4 6
```

## Remote store (HTTP/HTTPS, S3; optional GCS/Azure)

`ZarrObjectStore` uses the Rust object-store integration underneath and
dispatches on the URL scheme. Public `https://` URLs need no
credentials. S3 is enabled by default. GCS and Azure Blob require the
source-install features shown above. For private cloud buckets, set the
standard provider environment variables before calling `open()`:

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
#> [1] "uint16"
oarr$shape()   # t, z, y, x  (4-D: 2 timepoints × 236 Z-slices × 275 × 271)
#> [1]   2 236 275 271

# Retrieve a small spatial patch: first time point, first Z-slice,
# first 8×8 pixels in Y/X — indices are 1-based inclusive
patch <- oarr$retrieve(c(1L, 1L, 1L, 1L), c(1L, 1L, 8L, 8L))
dim(patch)
#> [1] 1 1 8 8
range(patch)
#> [1]  8 28
```

Cloud URLs use the same API; only the scheme and credentials differ:

``` r
# AWS S3 is enabled by default
Sys.setenv(
  AWS_ACCESS_KEY_ID     = "...",
  AWS_SECRET_ACCESS_KEY = "...",
  AWS_REGION            = "us-east-1"
)
s3 <- ZarrObjectStore$open("s3://my-bucket/path/to/store.zarr")

# GCS requires installing with --enable-gcp or --enable-all-cloud
Sys.setenv(GOOGLE_APPLICATION_CREDENTIALS = "/path/to/service-account.json")
gcs <- ZarrObjectStore$open("gs://my-bucket/path/to/store.zarr")

# Azure requires installing with --enable-azure or --enable-all-cloud
Sys.setenv(
  AZURE_STORAGE_ACCOUNT = "...",
  AZURE_STORAGE_ACCESS_KEY = "..."
)
az <- ZarrObjectStore$open("az://my-container/path/to/store.zarr")
```

## Group metadata

``` r
vcf_path <- system.file("testdata", "vcf_zarr", "v0.4", package = "Rzarrs")
grp <- ZarrGroup$open(ZarrStore$open(vcf_path), "/")
names(grp$attributes())
#> [1] "vcf_zarr_version"     "vcf_meta_information"
grp$children(FALSE)
#> $path
#> [1] "/call_genotype_phased" "/contig_id"            "/variant_allele"
#> [4] "/variant_contig"       "/variant_position"     "/filter_id"
#> [7] "/filter_description"   "/sample_id"            "/call_genotype"
#>
#> $kind
#> [1] "array" "array" "array" "array" "array" "array" "array" "array" "array"
```

## VCF Zarr reader

`ZarrVcf` reads VCF data stored in the [VCF Zarr
spec](https://github.com/sgkit-dev/vcf-zarr-spec) (v0.1–v0.4). It
accepts a local path, a URL, or an existing store handle.

``` r
vcf_path <- system.file("testdata", "vcf_zarr", "v0.4", package = "Rzarrs")
zv <- ZarrVcf$open(vcf_path)

zv$version()
#> [1] "0.4"
zv$n_variants()
#> [1] 5
zv$n_samples()
#> [1] 3
zv$contigs()
#> [1] "chr1" "chr2"
zv$samples()
#> [1] "S1" "S2" "S3"
zv$variant_position()
#> [1] 100 200 300  50 150
zv$variant_contig()
#> [1] "chr1" "chr1" "chr1" "chr2" "chr2"
zv$variant_allele()
#>      [,1] [,2]
#> [1,] "A"  "T"
#> [2,] "C"  "G"
#> [3,] "G"  "A"
#> [4,] "T"  "C"
#> [5,] "A"  "G"
zv$genotypes()
#> , , 1
#>
#>      [,1] [,2] [,3]
#> [1,]    0    0    1
#> [2,]    0    1    0
#> [3,]    1    0    1
#> [4,]   -1    0    1
#> [5,]    0    0    0
#>
#> , , 2
#>
#>      [,1] [,2] [,3]
#> [1,]    0    1    1
#> [2,]    1    1    0
#> [3,]    0    0    0
#> [4,]   -1    1    1
#> [5,]    0    0    1
zv$call_genotype_phased()
#>       [,1]  [,2]  [,3]
#> [1,] FALSE  TRUE FALSE
#> [2,]  TRUE FALSE  TRUE
#> [3,] FALSE FALSE FALSE
#> [4,] FALSE  TRUE  TRUE
#> [5,]  TRUE  TRUE FALSE
```

### VCF Zarr ZIP archives

VCF Zarr data are often distributed as `.zarr.zip` archives for transfer
and small examples. `ZarrVcf$open()` accepts a local `.zarr.zip` path
when the Rust `zip` feature is enabled; `zip` is part of the default
feature set.

``` r
zip_path <- system.file("testdata", "vcf_zarr", "v0.4.zarr.zip", package = "Rzarrs")
zv_zip <- ZarrVcf$open(zip_path)
zv_zip$version()
#> [1] "0.4"
zv_zip$genotypes(variants = 1:2, samples = 1:2)
#> , , 1
#>
#>      [,1] [,2]
#> [1,]    0    0
#> [2,]    0    1
#>
#> , , 2
#>
#>      [,1] [,2]
#> [1,]    0    1
#> [2,]    1    1
```

Current ZIP support is intentionally reader-first and local-file
oriented: the archive is loaded into a Rust memory store before opening
the Zarr hierarchy. That is useful for small VCF fixtures and portable
examples. Large production VCF Zarr archives should be stored as normal
directory/object-store Zarr, or the backend should be switched to
upstream `zarrs_zip` once it is vendored for this package. `zarrs_zip`
is the right long-term target because it can provide a real Zarr storage
adapter instead of R-level or ad hoc extraction.

## License

GPL (\>= 3)
