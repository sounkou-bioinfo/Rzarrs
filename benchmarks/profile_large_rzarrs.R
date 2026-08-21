#!/usr/bin/env Rscript

args <- commandArgs(trailingOnly = TRUE)
if (length(args) != 2L || !identical(args[[1L]], "--store")) {
  stop("usage: profile_large_rzarrs.R --store PATH", call. = FALSE)
}
store_path <- normalizePath(args[[2L]], mustWork = TRUE)
manifest_path <- file.path(store_path, "benchmark-fixture.dcf")
stopifnot(file.exists(manifest_path))
manifest <- read.dcf(manifest_path)
shape <- as.integer(strsplit(manifest[1L, "shape"], "x", fixed = TRUE)[[1L]])
stopifnot(length(shape) == 2L, !anyNA(shape), all(shape > 0L))

array <- Rzarrs::ZarrArray$open(Rzarrs::ZarrStore$open(store_path), "/")
materialize <- function() {
  value <- array$retrieve()
  stopifnot(identical(dim(value), shape))
  value
}

invisible(materialize())
gc(full = TRUE)
for (iteration in seq_len(3L)) invisible(materialize())
