## Rzarrs tinytest suite
##
## Primary fixtures are bundled under inst/testdata/ — no external dependencies.
## See tools/make_fixtures.R to regenerate the bundled fixtures.

library(Rzarrs)

fixture <- function(name) {
  system.file("testdata", name, package = "Rzarrs")
}

int32_path   <- fixture("int32.zarr")
float16_path <- fixture("float16.zarr")
bfloat16_path <- fixture("bfloat16.zarr")
float16_special_path <- fixture("float16_special.zarr")
bfloat16_special_path <- fixture("bfloat16_special.zarr")
float32_path <- fixture("float32.zarr")
uint8_path <- fixture("uint8.zarr")
complex64_path <- fixture("complex64.zarr")
complex128_path <- fixture("complex128.zarr")

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
# uint8 bundled fixture — numeric unsigned byte values become R integers
# ---------------------------------------------------------------------------

u8 <- ZarrArray$open(ZarrStore$open(uint8_path), "/")
expect_equal(u8$dtype(), "uint8")
u8data <- u8$retrieve(NULL, NULL)
expect_true(is.integer(u8data))
expect_equal(dim(u8data), c(4L, 6L))
expect_equal(u8data[1L, 1L], 1L)
expect_equal(u8data[4L, 6L], 24L)

# ---------------------------------------------------------------------------
# low-precision float bundled fixtures — promoted exactly to R double
# ---------------------------------------------------------------------------

f16 <- ZarrArray$open(ZarrStore$open(float16_path), "/")
expect_equal(f16$dtype(), "float16")
f16data <- f16$retrieve(NULL, NULL)
expect_true(is.double(f16data))
expect_equal(dim(f16data), c(4L, 6L))
expect_equal(f16data[1L, 1L], 1.0)
expect_equal(f16data[4L, 6L], 24.0)

bf16 <- ZarrArray$open(ZarrStore$open(bfloat16_path), "/")
expect_equal(bf16$dtype(), "bfloat16")
bf16data <- bf16$retrieve(NULL, NULL)
expect_true(is.double(bf16data))
expect_equal(dim(bf16data), c(4L, 6L))
expect_equal(bf16data[1L, 1L], 1.0)
expect_equal(bf16data[4L, 6L], 24.0)

f16_special <- ZarrArray$open(ZarrStore$open(float16_special_path), "/")$retrieve(NULL, NULL)
expect_equal(f16_special[1L], 0.0)
expect_true(is.nan(f16_special[2L]))
expect_true(is.infinite(f16_special[3L]) && f16_special[3L] > 0)
expect_true(is.infinite(f16_special[4L]) && f16_special[4L] < 0)

bf16_special <- ZarrArray$open(ZarrStore$open(bfloat16_special_path), "/")$retrieve(NULL, NULL)
expect_equal(bf16_special[1L], 0.0)
expect_true(is.nan(bf16_special[2L]))
expect_true(is.infinite(bf16_special[3L]) && bf16_special[3L] > 0)
expect_true(is.infinite(bf16_special[4L]) && bf16_special[4L] < 0)

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
# complex bundled fixtures
# ---------------------------------------------------------------------------

c64 <- ZarrArray$open(ZarrStore$open(complex64_path), "/")
expect_equal(c64$dtype(), "complex64")
c64data <- c64$retrieve(NULL, NULL)
expect_true(is.complex(c64data))
expect_equal(dim(c64data), c(4L, 6L))
expect_equal(c64data[1L, 1L], 1 - 1i)
expect_equal(c64data[4L, 6L], 24 - 24i)
expect_true(inherits(c64data, "Rzarrs_complex64"))
expect_warning(c64_real <- as.double(c64data), "imaginary part is discarded")
expect_equal(dim(c64_real), dim(c64data))
expect_equal(c64_real[1L, 1L], 1)
expect_equal(sum(c64data), 300 - 300i)
expect_error(min(c64data), "Summary operation 'min' is not implemented for Rzarrs_complex64")

c128 <- ZarrArray$open(ZarrStore$open(complex128_path), "/")
expect_equal(c128$dtype(), "complex128")
c128data <- c128$retrieve(NULL, NULL)
expect_true(is.complex(c128data))
expect_true(inherits(c128data, "Rzarrs_complex128"))
expect_equal(dim(c128data), c(4L, 6L))
expect_equal(c128data[1L, 1L], 1 - 1i)
expect_equal(c128data[4L, 6L], 24 - 24i)
expect_warning(c128_real <- as.double(c128data), "imaginary part is discarded")
expect_equal(dim(c128_real), dim(c128data))
expect_equal(c128_real[1L, 1L], 1)
expect_equal(sum(c128data), 300 - 300i)
expect_error(min(c128data), "Summary operation 'min' is not implemented for Rzarrs_complex128")

# ---------------------------------------------------------------------------
# Rust-side R indexing validation
# ---------------------------------------------------------------------------
expect_error(arr$retrieve(c(1L, 1L), NULL), "starts and ends")
expect_error(arr$retrieve(c(1.5, 1), c(2, 2)), "whole-number")
expect_error(arr$retrieve(c(0L, 1L), c(1L, 2L)), ">= 1")
expect_error(arr$retrieve(c(1L, 1L), c(5L, 2L)), "out of range")
