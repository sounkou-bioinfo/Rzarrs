#' ---
#' title: Read a local Zarr array with Rzarrs
#' ---

#' Open the tiny Zarr V3 fixture installed with Rzarrs and inspect its metadata.

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

#' Retrieve the first two rows and first three columns. Rzarrs uses 1-based,
#' inclusive range endpoints.
arr$retrieve(starts = c(1L, 1L), ends = c(2L, 3L))
