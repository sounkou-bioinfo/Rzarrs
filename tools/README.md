# Fixture generation

The bundled fixtures under `inst/testdata/` are generated explicitly by scripts in this directory. They are intentionally tiny, deterministic, and dependency-light so Rust/R layout and dtype behavior can be checked without network access.

## Primitive Zarr arrays

```sh
Rscript tools/make_fixtures.R
```

Creates:

- `inst/testdata/int32.zarr` — 4 x 6 `int32`, chunks 2 x 3, values 1..24
- `inst/testdata/uint8.zarr` — 4 x 6 `uint8`, chunks 2 x 3, values 1..24; Rzarrs reads this as R `integer`, not `raw`
- `inst/testdata/float16.zarr` — 4 x 6 `float16`, chunks 2 x 3, values 1..24; Rzarrs promotes this exactly to R `double`
- `inst/testdata/bfloat16.zarr` — 4 x 6 `bfloat16`, chunks 2 x 3, values 1..24; Rzarrs promotes this exactly to R `double`
- `inst/testdata/float16_special.zarr` — 1-D `float16` values `0`, `NaN`, `Inf`, `-Inf`
- `inst/testdata/bfloat16_special.zarr` — 1-D `bfloat16` values `0`, `NaN`, `Inf`, `-Inf`
- `inst/testdata/float32.zarr` — 4 x 6 `float32`, chunks 2 x 3, values 1..24
- `inst/testdata/complex64.zarr` — 4 x 6 `complex64`, chunks 2 x 3, values `n - n*i`
- `inst/testdata/complex128.zarr` — 4 x 6 `complex128`, chunks 2 x 3, values `n - n*i`

The script writes Zarr V3 metadata and bytes-codec chunks directly in base R. No Rarr, reticulate, or Python dependency is required for these core fixtures.

## VCF Zarr fixtures

```sh
Rscript tools/make_vcf_fixtures.R
```

Creates the VCF Zarr version fixtures under `inst/testdata/vcf_zarr/`, including the local `.zarr.zip` archive used by README/tests.

## Optional Rarr interop check

`Rarr` is listed in `Suggests` for optional interoperability checks. It is a Bioconductor package; use a build with Zarr V3 support if you want to run the interop script:

```r
BiocManager::install("Rarr")
```

Then run:

```sh
Rscript tools/check_rarr_interop.R
```

This compares selected package fixtures as read by Rzarrs and Rarr. The core fixture scripts above remain base-R so package tests do not depend on Rarr being present.

## Rust vendoring

The source package installs offline from `src/rust/vendor.tar.xz`. Regenerate it from `src/rust/Cargo.lock` with:

```sh
make vendor-rust
# or: tools/vendor_rust_deps.sh
```

The vendoring script runs `cargo vendor`, applies all patches in `tools/vendor-patches/`, updates the affected vendored Cargo checksum files, and writes a normalized `src/rust/vendor.tar.xz`. It also writes `Cargo.lock.in`: `R CMD build` excludes files ending in `.lock`, so the installer restores this packageable copy as `Cargo.lock` before invoking Cargo. Use `tools/vendor_rust_deps.sh --keep-vendor` when you want to inspect the patched `src/rust/vendor/` tree locally.
