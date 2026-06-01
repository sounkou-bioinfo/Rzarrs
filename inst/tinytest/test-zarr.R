## Rzarrs tinytest suite
##
## Primary fixtures are bundled under inst/testdata/ — no external dependencies.
## See tools/make_fixtures.R to regenerate the bundled fixtures.

library(Rzarrs)

fixture <- function(name) {
  system.file("testdata", name, package = "Rzarrs")
}

int32_path   <- fixture("int32.zarr")
float32_path <- fixture("float32.zarr")

# ---------------------------------------------------------------------------
# ZarrStore
# ---------------------------------------------------------------------------

store <- ZarrStore$open(int32_path)
expect_inherits(store, "ZarrStore")
expect_true(nzchar(store$path()))

expect_error(ZarrStore$open("/does/not/exist/blah.zarr"))

# ---------------------------------------------------------------------------
# ZarrArray — metadata
# ---------------------------------------------------------------------------

arr <- ZarrArray$open(store, "/")
expect_inherits(arr, "ZarrArray")

expect_equal(arr$ndim(), 2L)
expect_equal(arr$shape(), c(4L, 6L))
expect_equal(arr$chunk_shape(), c(2L, 3L))
expect_equal(arr$dtype(), "int32")
expect_null(arr$dimension_names())

json <- arr$metadata_json()
expect_true(is.character(json) && nzchar(json))

# ---------------------------------------------------------------------------
# ZarrArray — retrieve full array
# ---------------------------------------------------------------------------

full <- arr$retrieve(NULL, NULL)
expect_equal(dim(full), c(4L, 6L))
expect_true(is.integer(full))
expect_equal(length(full), 24L)

# values are row*6 + col + 1 in C order
expect_equal(full[1L, 1L], 1L)
expect_equal(full[1L, 6L], 6L)
expect_equal(full[4L, 6L], 24L)

# ---------------------------------------------------------------------------
# ZarrArray — retrieve subset
# ---------------------------------------------------------------------------

# starts/ends are 1-based inclusive (R convention)
sub <- arr$retrieve(c(1L, 1L), c(2L, 3L))
expect_equal(dim(sub), c(2L, 3L))
expect_true(is.integer(sub))
expect_equal(as.vector(sub), c(1L, 7L, 2L, 8L, 3L, 9L))

# ---------------------------------------------------------------------------
# float32 bundled fixture
# ---------------------------------------------------------------------------

fa <- ZarrArray$open(ZarrStore$open(float32_path), "/")
expect_equal(fa$dtype(), "float32")
fdata <- fa$retrieve(NULL, NULL)
expect_true(is.double(fdata))
expect_equal(dim(fdata), c(4L, 6L))
expect_equal(fdata[1L, 1L], 1.0)
expect_equal(fdata[4L, 6L], 24.0)

# ---------------------------------------------------------------------------
# Rust-side R indexing validation
# ---------------------------------------------------------------------------
expect_error(arr$retrieve(c(1L, 1L), NULL), "starts and ends")
expect_error(arr$retrieve(c(1.5, 1), c(2, 2)), "whole-number")
expect_error(arr$retrieve(c(0L, 1L), c(1L, 2L)), ">= 1")
expect_error(arr$retrieve(c(1L, 1L), c(5L, 2L)), "out of range")
