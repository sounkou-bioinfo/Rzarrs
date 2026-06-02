library(Rzarrs)

as_native_raw <- function(bytes_le) {
  bytes <- if (.Platform$endian == "little") bytes_le else rev(bytes_le)
  as.raw(bytes)
}

raw <- c(
  as_native_raw(c(0, 0, 0, 0, 0, 0, 0, 0)),
  as_native_raw(c(1, 0, 0, 0, 0, 0, 0, 0)),
  as_native_raw(c(255, 255, 255, 255, 255, 255, 255, 255)),
  as_native_raw(c(0, 0, 0, 0, 0, 0, 0, 128))
)

x <- readBin(raw, "double", n = 4L, size = 8L, endian = .Platform$endian)
class(x) <- "Rzarrs_int64"
attr(x, "zarr_dtype") <- "numpy.datetime64"
attr(x, "unit") <- "s"
attr(x, "scale_factor") <- 1
attr(x, "storage") <- "i64-bitpattern"

expect_inherits(x, "Rzarrs_int64")
expect_equal(attr(x, "zarr_dtype"), "numpy.datetime64")
expect_equal(attr(x, "unit"), "s")
expect_equal(as.character(x), c("0", "1", "-1", NA))
expect_equal(as.vector(is.na(x)), c(FALSE, FALSE, FALSE, TRUE))
expect_equal(as.vector(x == x), c(TRUE, TRUE, TRUE, NA))
expect_error(x + 1L, "arithmetic on numpy datetime64")
