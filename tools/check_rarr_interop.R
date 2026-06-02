#!/usr/bin/env Rscript
# Optional Rarr interoperability checks for generated fixtures.
# Run from the package root after installing a Rarr build with Zarr V3 support.

if (!requireNamespace("Rarr", quietly = TRUE)) {
  stop("Rarr is not installed. Install it from Bioconductor to run this interop check.", call. = FALSE)
}

if (!requireNamespace("Rzarrs", quietly = TRUE)) {
  stop("Rzarrs is not installed. Run `R CMD INSTALL .` first.", call. = FALSE)
}

fixture <- function(name) {
  file.path("inst", "testdata", name)
}

check_array <- function(name) {
  path <- fixture(name)
  rz <- Rzarrs::ZarrArray$open(Rzarrs::ZarrStore$open(path), "/")$retrieve(NULL, NULL)
  rr <- tryCatch(Rarr::read_zarr_array(path), error = identity)
  if (inherits(rr, "error")) {
    stop(
      "Installed Rarr cannot read ", name, ". This interop script requires ",
      "a Rarr build with Zarr V3 support. Rarr error: ", conditionMessage(rr),
      call. = FALSE
    )
  }

  if (!identical(dim(rz), dim(rr))) {
    stop(name, ": dimension mismatch", call. = FALSE)
  }
  if (!isTRUE(all.equal(as.array(rz), as.array(rr), check.attributes = FALSE))) {
    stop(name, ": value mismatch", call. = FALSE)
  }
  message("OK: ", name)
}

check_array("int32.zarr")
check_array("uint8.zarr")
check_array("float32.zarr")
