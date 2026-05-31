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
#   inst/testdata/float32.zarr/ 4×6 float32, chunks 2×3, values 1.0..24.0

base <- file.path(dirname(dirname(sys.frame(1)$ofile)), "inst", "testdata")
if (!nzchar(base) || grepl("^\\.", base)) {
  # fallback when sourced interactively
  base <- file.path("inst", "testdata")
}

write_json_file <- function(path, txt) {
  writeLines(txt, path, useBytes = TRUE)
}

zarr3_array_meta <- function(shape, chunk_shape, dtype) {
  sprintf(paste0(
    '{\n',
    '  "zarr_format": 3,\n',
    '  "node_type": "array",\n',
    '  "shape": [%s],\n',
    '  "data_type": "%s",\n',
    '  "chunk_grid": {"name": "regular", "configuration": {"chunk_shape": [%s]}},\n',
    '  "chunk_key_encoding": {"name": "default", "configuration": {"separator": "/"}},\n',
    '  "fill_value": 0,\n',
    '  "codecs": [{"name": "bytes", "configuration": {"endian": "little"}}]\n',
    '}'
  ),
  paste(shape, collapse = ", "), dtype, paste(chunk_shape, collapse = ", "))
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

write_i32 <- function(path, vals) {
  con <- file(path, "wb"); on.exit(close(con))
  writeBin(as.integer(vals), con, size = 4L, endian = "little")
}

write_f32 <- function(path, vals) {
  con <- file(path, "wb"); on.exit(close(con))
  writeBin(as.double(vals), con, size = 4L, endian = "little")
}

make_fixture(file.path(base, "int32.zarr"),   "int32",   write_i32)
make_fixture(file.path(base, "float32.zarr"), "float32", write_f32)
