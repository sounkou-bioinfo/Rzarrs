## Optional Rarr interoperability tests.
## Enable explicitly with RZARRS_RUN_RARR_INTEROP=1 because Rarr is a
## Bioconductor Suggests dependency and is not needed for core package checks.

if (identical(Sys.getenv("RZARRS_RUN_RARR_INTEROP"), "1") &&
    requireNamespace("Rarr", quietly = TRUE)) {
  library(Rzarrs)

  probe_path <- system.file("testdata", "int32.zarr", package = "Rzarrs")
  probe <- tryCatch(Rarr::read_zarr_array(probe_path), error = identity)

  if (!inherits(probe, "error") && !is.null(dim(probe))) {
    check_rarr_fixture <- function(name) {
      path <- system.file("testdata", name, package = "Rzarrs")
      rz <- ZarrArray$open(ZarrStore$open(path), "/")$retrieve(NULL, NULL)
      rr <- Rarr::read_zarr_array(path)
      expect_equal(dim(rz), dim(rr))
      expect_equal(as.array(rz), as.array(rr))
    }

    check_rarr_fixture("int32.zarr")
    check_rarr_fixture("uint8.zarr")
    check_rarr_fixture("float32.zarr")
  }
}
