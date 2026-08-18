if (!requireNamespace("Rarr", quietly = TRUE)) {
  exit_file("Rarr is required for differential property tests")
}

set.seed(20260808L)
cases <- expand.grid(
  data_type = c("integer", "double", "logical"),
  order = c("C", "F"),
  gzip = c(FALSE, TRUE),
  replicate = seq_len(2L),
  stringsAsFactors = FALSE
)

for (case_id in seq_len(nrow(cases))) {
  local({
    id <- case_id
    data_type <- cases$data_type[[id]]
    order <- cases$order[[id]]
    gzip <- cases$gzip[[id]]
    rank <- sample.int(4L, 1L)
    shape <- sample.int(6L, rank, replace = TRUE)
    chunk_shape <- vapply(shape, sample.int, integer(1L), size = 1L)
    n <- prod(shape)
    values <- switch(
      data_type,
      integer = {
        pool <- c(
          NA_integer_, -.Machine$integer.max, .Machine$integer.max,
          sample.int(2001L, 256L, replace = TRUE) - 1001L
        )
        values <- sample(pool, n, replace = TRUE)
        values[[1L]] <- NA_integer_
        values
      },
      double = {
        pool <- c(NA_real_, NaN, -Inf, Inf, stats::rnorm(256L))
        values <- sample(pool, n, replace = TRUE)
        values[[1L]] <- NA_real_
        values
      },
      logical = sample(c(FALSE, TRUE), n, replace = TRUE)
    )
    expected <- array(values, dim = shape)
    path <- tempfile(sprintf("rzarrs-rarr-property-%02d-", id), fileext = ".zarr")
    on.exit(unlink(path, recursive = TRUE), add = TRUE)

    compressor <- if (gzip) Rarr::use_gzip() else NULL
    Rarr::write_zarr_array(
      expected,
      path,
      chunk_dim = chunk_shape,
      data_type = data_type,
      order = order,
      compressor = compressor,
      zarr_version = 3L
    )

    # Rarr 2.1.35 labels zlib-wrapped chunks as gzip. Rewrite those payloads
    # as actual gzip streams so both readers are tested against valid V3 data.
    if (gzip) {
      chunk_paths <- list.files(
        file.path(path, "c"), recursive = TRUE, full.names = TRUE
      )
      for (chunk_path in chunk_paths) {
        encoded <- readBin(
          chunk_path, what = "raw", n = file.info(chunk_path)$size
        )
        decoded <- memDecompress(encoded, type = "gzip")
        gzip_path <- tempfile("rzarrs-property-gzip-")
        connection <- gzfile(gzip_path, open = "wb", compression = 6L)
        writeBin(decoded, connection, useBytes = TRUE)
        close(connection)
        stopifnot(file.copy(gzip_path, chunk_path, overwrite = TRUE))
        unlink(gzip_path)
      }
    }

    array <- ZarrArray$open(ZarrStore$open(path), "/")
    rzarrs_full <- array$retrieve(NULL, NULL)
    rarr_full <- Rarr::read_zarr_array(path)
    label <- sprintf(
      "case=%d type=%s shape=%s chunks=%s order=%s gzip=%s",
      id, data_type, paste(shape, collapse = "x"),
      paste(chunk_shape, collapse = "x"), order, gzip
    )
    expect_equal(as.vector(rzarrs_full), as.vector(rarr_full), info = label)
    expect_equal(as.vector(rzarrs_full), as.vector(expected), info = label)
    if (rank > 1L) expect_equal(dim(rzarrs_full), shape, info = label)

    starts <- vapply(shape, sample.int, integer(1L), size = 1L)
    ends <- vapply(
      seq_along(shape),
      function(axis) {
        starts[[axis]] - 1L + sample.int(shape[[axis]] - starts[[axis]] + 1L, 1L)
      },
      integer(1L)
    )
    indices <- Map(seq.int, starts, ends)
    rzarrs_subset <- array$retrieve(starts, ends)
    rarr_subset <- Rarr::read_zarr_array(path, index = indices)
    expected_subset <- do.call(
      `[`, c(list(expected), indices, list(drop = FALSE))
    )
    expect_equal(
      as.vector(rzarrs_subset), as.vector(rarr_subset), info = label
    )
    expect_equal(
      as.vector(rzarrs_subset), as.vector(expected_subset), info = label
    )
    if (rank > 1L) {
      expect_equal(dim(rzarrs_subset), lengths(indices), info = label)
    }
  })
}
