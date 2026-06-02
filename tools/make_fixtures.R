#!/usr/bin/env Rscript
# tools/make_fixtures.R
#
# Regenerate the bundled Zarr V3 test fixtures under inst/testdata/.
# Pure base R — no packages required.
#
# Usage:
#   Rscript tools/make_fixtures.R          # from package root
#
# Layout
#   inst/testdata/int32.zarr/   4×6 int32,  chunks 2×3, values 1..24
#   inst/testdata/uint8.zarr/   4×6 uint8,  chunks 2×3, values 1..24
#   inst/testdata/float16.zarr/ 4×6 float16, chunks 2×3, values 1.0..24.0
#   inst/testdata/bfloat16.zarr/ 4×6 bfloat16, chunks 2×3, values 1.0..24.0
#   inst/testdata/float32.zarr/ 4×6 float32, chunks 2×3, values 1.0..24.0
#   inst/testdata/complex64.zarr/ 4×6 complex64, chunks 2×3, values n-n*i
#   inst/testdata/complex128.zarr/ 4×6 complex128, chunks 2×3, values n-n*i

cmd <- commandArgs(FALSE)
file_arg <- sub("^--file=", "", grep("^--file=", cmd, value = TRUE)[1])
root <- if (!is.na(file_arg)) dirname(dirname(normalizePath(file_arg))) else getwd()
base <- file.path(root, "inst", "testdata")

write_json_file <- function(path, txt) {
  writeLines(txt, path, useBytes = TRUE)
}

zarr3_array_meta <- function(shape, chunk_shape, dtype) {
  fill_value <- if (dtype %in% c("complex64", "complex128")) "[0.0, 0.0]" else "0"
  sprintf(paste0(
    '{\n',
    '  "zarr_format": 3,\n',
    '  "node_type": "array",\n',
    '  "shape": [%s],\n',
    '  "data_type": "%s",\n',
    '  "chunk_grid": {"name": "regular", "configuration": {"chunk_shape": [%s]}},\n',
    '  "chunk_key_encoding": {"name": "default", "configuration": {"separator": "/"}},\n',
    '  "fill_value": %s,\n',
    '  "codecs": [{"name": "bytes", "configuration": {"endian": "little"}}]\n',
    '}'
  ),
  paste(shape, collapse = ", "), dtype, paste(chunk_shape, collapse = ", "), fill_value)
}

# values for a chunk covering rows r0:(r0+nrows-1), cols c0:(c0+ncols-1)
# C-order (row-major): t() then as.vector gives row-by-row layout.
chunk_vals <- function(rows, cols) {
  m <- outer(rows, cols, function(r, c) r * 6L + c + 1L)
  as.vector(t(m))   # transpose before as.vector → row-major (C) order
}

make_fixture <- function(dir, dtype, write_chunk) {
  for (sub in c("c/0", "c/1")) dir.create(file.path(dir, sub), recursive = TRUE, showWarnings = FALSE)
  write_json_file(file.path(dir, "zarr.json"), zarr3_array_meta(c(4, 6), c(2, 3), dtype))
  write_chunk(file.path(dir, "c/0/0"), chunk_vals(0:1, 0:2))
  write_chunk(file.path(dir, "c/0/1"), chunk_vals(0:1, 3:5))
  write_chunk(file.path(dir, "c/1/0"), chunk_vals(2:3, 0:2))
  write_chunk(file.path(dir, "c/1/1"), chunk_vals(2:3, 3:5))
  message("Written: ", dir)
}

make_1d_fixture <- function(dir, dtype, write_chunk, vals) {
  dir.create(file.path(dir, "c"), recursive = TRUE, showWarnings = FALSE)
  write_json_file(file.path(dir, "zarr.json"), zarr3_array_meta(length(vals), length(vals), dtype))
  write_chunk(file.path(dir, "c/0"), vals)
  message("Written: ", dir)
}

write_i32 <- function(path, vals) {
  con <- file(path, "wb"); on.exit(close(con))
  writeBin(as.integer(vals), con, size = 4L, endian = "little")
}

write_u8 <- function(path, vals) {
  con <- file(path, "wb"); on.exit(close(con))
  writeBin(as.raw(vals), con)
}

write_u16_le <- function(path, vals) {
  con <- file(path, "wb"); on.exit(close(con))
  writeBin(as.integer(vals), con, size = 2L, endian = "little")
}

float16_bits <- function(vals) {
  bits <- integer(length(vals))
  zero <- vals == 0
  nonzero <- !zero
  e <- floor(log2(abs(vals[nonzero])))
  frac <- round((abs(vals[nonzero]) / 2^e - 1) * 1024)
  bits[nonzero] <- (e + 15L) * 1024L + frac
  bits[vals < 0] <- bits[vals < 0] + 32768L
  bits
}

write_f16 <- function(path, vals) {
  write_u16_le(path, float16_bits(vals))
}

write_bf16 <- function(path, vals) {
  raw32 <- writeBin(as.double(vals), raw(), size = 4L, endian = "little")
  bytes <- matrix(raw32, nrow = 4L)
  con <- file(path, "wb"); on.exit(close(con))
  writeBin(as.raw(as.vector(bytes[3:4, , drop = FALSE])), con)
}

write_f16_bits <- function(path, bits) {
  write_u16_le(path, bits)
}

write_bf16_bits <- function(path, bits) {
  write_u16_le(path, bits)
}

write_f32 <- function(path, vals) {
  con <- file(path, "wb"); on.exit(close(con))
  writeBin(as.double(vals), con, size = 4L, endian = "little")
}

write_complex_parts <- function(path, vals, size) {
  z <- complex(real = vals, imaginary = -vals)
  parts <- as.vector(rbind(Re(z), Im(z)))
  con <- file(path, "wb"); on.exit(close(con))
  writeBin(as.double(parts), con, size = size, endian = "little")
}

write_c64 <- function(path, vals) {
  write_complex_parts(path, vals, 4L)
}

write_c128 <- function(path, vals) {
  write_complex_parts(path, vals, 8L)
}

make_fixture(file.path(base, "int32.zarr"),      "int32",      write_i32)
make_fixture(file.path(base, "uint8.zarr"),      "uint8",      write_u8)
make_fixture(file.path(base, "float16.zarr"),    "float16",    write_f16)
make_fixture(file.path(base, "bfloat16.zarr"),   "bfloat16",   write_bf16)
make_1d_fixture(file.path(base, "float16_special.zarr"), "float16", write_f16_bits, c(0x0000, 0x7e00, 0x7c00, 0xfc00))
make_1d_fixture(file.path(base, "bfloat16_special.zarr"), "bfloat16", write_bf16_bits, c(0x0000, 0x7fc0, 0x7f80, 0xff80))
make_fixture(file.path(base, "float32.zarr"),    "float32",    write_f32)
make_fixture(file.path(base, "complex64.zarr"),  "complex64",  write_c64)
make_fixture(file.path(base, "complex128.zarr"), "complex128", write_c128)
