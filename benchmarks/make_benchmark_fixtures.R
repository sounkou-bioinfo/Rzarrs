#!/usr/bin/env Rscript

usage <- function(status = 0L) {
  cat(
    "Usage: Rscript benchmarks/make_benchmark_fixtures.R --out DIR [options]\n\n",
    "Create numeric Zarr V3 fixtures that Rzarrs and Rarr both read.\n\n",
    "Options:\n",
    "  --out DIR          destination directory (required)\n",
    "  --rows N           array rows (default: 4096)\n",
    "  --cols N           array columns (default: 4096)\n",
    "  --chunk-rows N     chunk rows (default: 512)\n",
    "  --chunk-cols N     chunk columns (default: 512)\n",
    "  --overwrite        replace existing fixture directories\n",
    sep = ""
  )
  quit(status = status)
}

parse_args <- function(args) {
  values <- list(
    out = NULL,
    rows = 4096L,
    cols = 4096L,
    chunk_rows = 512L,
    chunk_cols = 512L,
    overwrite = FALSE
  )
  i <- 1L
  while (i <= length(args)) {
    arg <- args[[i]]
    if (identical(arg, "--help")) usage()
    if (identical(arg, "--overwrite")) {
      values$overwrite <- TRUE
      i <- i + 1L
      next
    }
    key <- sub("^--", "", arg)
    key <- gsub("-", "_", key, fixed = TRUE)
    if (!startsWith(arg, "--") || !key %in% names(values) || i == length(args)) usage(2L)
    i <- i + 1L
    values[[key]] <- args[[i]]
    i <- i + 1L
  }
  for (name in c("rows", "cols", "chunk_rows", "chunk_cols")) {
    values[[name]] <- suppressWarnings(as.integer(values[[name]]))
  }
  values
}

write_metadata <- function(path, nrow, ncol, chunk_nrow, chunk_ncol, gzip) {
  codecs <- if (gzip) {
    paste0(
      '[{"name":"bytes","configuration":{"endian":"little"}},',
      '{"name":"gzip","configuration":{"level":1}}]'
    )
  } else {
    '[{"name":"bytes","configuration":{"endian":"little"}}]'
  }
  metadata <- sprintf(
    paste0(
      '{\n',
      '  "zarr_format": 3,\n',
      '  "node_type": "array",\n',
      '  "shape": [%d, %d],\n',
      '  "data_type": "int32",\n',
      '  "chunk_grid": {"name": "regular", "configuration": {"chunk_shape": [%d, %d]}},\n',
      '  "chunk_key_encoding": {"name": "default", "configuration": {"separator": "/"}},\n',
      '  "fill_value": 0,\n',
      '  "codecs": %s,\n',
      '  "attributes": {}\n',
      '}\n'
    ),
    nrow, ncol, chunk_nrow, chunk_ncol, codecs
  )
  writeLines(metadata, file.path(path, "zarr.json"), useBytes = TRUE)
}

chunk_values <- function(row_start, col_start, nrow, ncol, total_ncol) {
  rows <- row_start + seq_len(nrow)
  cols <- col_start + seq_len(ncol)
  values <- outer(rows - 1L, cols, function(row, col) row * total_ncol + col)
  as.integer(t(values))
}

gzip_payload <- function(payload) {
  path <- tempfile("rzarrs-gzip-")
  on.exit(unlink(path), add = TRUE)
  connection <- gzfile(path, open = "wb", compression = 1L)
  writeBin(payload, connection, useBytes = TRUE)
  close(connection)
  readBin(path, what = "raw", n = file.info(path)$size)
}

write_fixture <- function(root, name, nrow, ncol, chunk_nrow, chunk_ncol, gzip, overwrite) {
  path <- file.path(root, name)
  if (dir.exists(path)) {
    if (!overwrite) stop("fixture already exists: ", path, "; use --overwrite", call. = FALSE)
    unlink(path, recursive = TRUE, force = TRUE)
  }
  dir.create(file.path(path, "c"), recursive = TRUE, showWarnings = FALSE)
  write_metadata(path, nrow, ncol, chunk_nrow, chunk_ncol, gzip)

  for (row_start in seq.int(0L, nrow - 1L, by = chunk_nrow)) {
    for (col_start in seq.int(0L, ncol - 1L, by = chunk_ncol)) {
      values <- chunk_values(row_start, col_start, chunk_nrow, chunk_ncol, ncol)
      payload <- writeBin(values, raw(), size = 4L, endian = "little")
      if (gzip) payload <- gzip_payload(payload)
      chunk_path <- file.path(
        path,
        "c",
        as.character(row_start %/% chunk_nrow),
        as.character(col_start %/% chunk_ncol)
      )
      dir.create(dirname(chunk_path), recursive = TRUE, showWarnings = FALSE)
      writeBin(payload, chunk_path, useBytes = TRUE)
    }
  }

  logical_bytes <- as.double(nrow) * as.double(ncol) * 4
  manifest <- c(
    "format: Zarr V3",
    "dtype: int32",
    sprintf("shape: %dx%d", nrow, ncol),
    sprintf("chunk_shape: %dx%d", chunk_nrow, chunk_ncol),
    sprintf("chunk_count: %d", (nrow / chunk_nrow) * (ncol / chunk_ncol)),
    sprintf("logical_elements: %.0f", as.double(nrow) * as.double(ncol)),
    sprintf("logical_bytes: %.0f", logical_bytes),
    sprintf("codec: %s", if (gzip) "gzip(level=1)" else "bytes")
  )
  writeLines(manifest, file.path(path, "benchmark-fixture.dcf"), useBytes = TRUE)
  message("wrote ", path)
}

args <- parse_args(commandArgs(trailingOnly = TRUE))
if (is.null(args$out)) usage(2L)
if (any(!is.finite(unlist(args[c("rows", "cols", "chunk_rows", "chunk_cols")])))) {
  stop("array and chunk dimensions must be finite integers", call. = FALSE)
}
if (any(unlist(args[c("rows", "cols", "chunk_rows", "chunk_cols")]) <= 0L)) {
  stop("array and chunk dimensions must be positive", call. = FALSE)
}
if (args$rows %% args$chunk_rows != 0L || args$cols %% args$chunk_cols != 0L) {
  stop("array dimensions must be exact multiples of chunk dimensions", call. = FALSE)
}
if (as.double(args$rows) * as.double(args$cols) > .Machine$integer.max) {
  stop("fixture values must fit in R's integer type", call. = FALSE)
}

out <- normalizePath(args$out, mustWork = FALSE)
dir.create(out, recursive = TRUE, showWarnings = FALSE)
write_fixture(out, "numeric-uncompressed.zarr", args$rows, args$cols,
              args$chunk_rows, args$chunk_cols, FALSE, args$overwrite)
write_fixture(out, "numeric-gzip.zarr", args$rows, args$cols,
              args$chunk_rows, args$chunk_cols, TRUE, args$overwrite)
