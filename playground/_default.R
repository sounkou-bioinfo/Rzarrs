#' ---
#' title: Read a local Zarr array with Rzarrs
#' ---

#' Rzarrs includes a tiny Zarr V3 fixture, so this example works entirely
#' inside the browser after the wasm package is installed from r-universe.

library(Rzarrs)

path <- system.file("testdata", "int32.zarr", package = "Rzarrs")
stopifnot(nzchar(path))

store <- ZarrStore$open(path)
arr <- ZarrArray$open(store, "/")

list(
  dtype = arr$dtype(),
  shape = arr$shape(),
  chunk_shape = arr$chunk_shape()
)

#' R indices are 1-based and both range ends are inclusive.
arr$retrieve(starts = c(1L, 1L), ends = c(2L, 3L))
