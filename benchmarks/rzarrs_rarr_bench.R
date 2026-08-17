#!/usr/bin/env Rscript

if (!requireNamespace("optparse", quietly = TRUE)) {
  stop("optparse must be installed to run this benchmark", call. = FALSE)
}

option_list <- list(
  optparse::make_option(
    c("-s", "--store"), type = "character",
    help = "Numeric Zarr fixture directory."
  ),
  optparse::make_option(
    c("-i", "--implementation"), type = "character",
    help = "Implementation to measure: Rzarrs or Rarr."
  ),
  optparse::make_option(
    c("-m", "--mode"), type = "character", default = "warm",
    help = "Cache mode recorded in the result: warm or cold [default: %default]."
  ),
  optparse::make_option(
    c("-n", "--iterations"), type = "integer", default = 5L,
    help = "bench iterations in this process [default: %default]."
  ),
  optparse::make_option(
    c("-o", "--out"), type = "character",
    help = "Directory for bench artifacts."
  ),
  optparse::make_option(
    "--verify", action = "store_true", default = FALSE,
    help = "Compare Rzarrs and Rarr outside the timed expression."
  )
)
parser <- optparse::OptionParser(
  usage = "%prog --store PATH --implementation Rzarrs|Rarr --out DIR [options]",
  description = paste(
    "Measure one implementation in a fresh R process.",
    "The NUMA/taskset launcher runs each implementation separately so cold-cache",
    "measurements remain fair."
  ),
  option_list = option_list
)
args <- optparse::parse_args(parser, args = commandArgs(trailingOnly = TRUE))

read_rzarrs <- function(store) {
  Rzarrs::ZarrArray$open(Rzarrs::ZarrStore$open(store), "/")$retrieve()
}

read_rarr <- function(store) {
  Rarr::read_zarr_array(store)
}

verify_store <- function(store) {
  rzarrs <- read_rzarrs(store)
  rarr <- read_rarr(store)
  if (!identical(dim(rzarrs), dim(rarr)) ||
      !identical(as.vector(rzarrs), as.vector(rarr))) {
    stop("Rzarrs and Rarr returned different values for: ", store, call. = FALSE)
  }
  invisible(NULL)
}

package_version <- function(package) {
  as.character(utils::packageVersion(package))
}

write_environment <- function(path, options) {
  lines <- c(
    capture.output(sessionInfo()),
    "",
    "Benchmark options:",
    capture.output(str(options))
  )
  writeLines(lines, path, useBytes = TRUE)
}

if (is.null(args$store) || !dir.exists(args$store)) {
  stop("--store must name an existing Zarr directory", call. = FALSE)
}
args$store <- normalizePath(args$store)
if (!args$mode %in% c("warm", "cold")) {
  stop("--mode must be warm or cold", call. = FALSE)
}
if (is.na(args$iterations) || args$iterations < 1L) {
  stop("--iterations must be a positive integer", call. = FALSE)
}
if (!requireNamespace("bench", quietly = TRUE)) {
  stop("bench must be installed to run this benchmark", call. = FALSE)
}
if (!requireNamespace("Rarr", quietly = TRUE)) {
  stop("Rarr must be installed to compare it with Rzarrs", call. = FALSE)
}
if (!requireNamespace("Rzarrs", quietly = TRUE)) {
  stop("Rzarrs must be installed to run this benchmark", call. = FALSE)
}

if (isTRUE(args$verify)) {
  verify_store(args$store)
  cat("verified ", args$store, "\n", sep = "")
  quit(status = 0L)
}
if (is.null(args$implementation) || !args$implementation %in% c("Rzarrs", "Rarr")) {
  stop("--implementation must be Rzarrs or Rarr", call. = FALSE)
}
if (is.null(args$out)) stop("--out is required unless --verify is used", call. = FALSE)

reader <- switch(args$implementation, Rzarrs = read_rzarrs, Rarr = read_rarr)
if (identical(args$mode, "warm")) invisible(reader(args$store))
gc(full = TRUE)

result <- bench::mark(
  read_full_array = reader(args$store),
  iterations = args$iterations,
  check = FALSE,
  memory = TRUE,
  filter_gc = FALSE
)

out <- normalizePath(args$out, mustWork = FALSE)
dir.create(out, recursive = TRUE, showWarnings = FALSE)
summary <- data.frame(
  implementation = args$implementation,
  runtime = "R",
  mode = args$mode,
  store = args$store,
  iterations_requested = args$iterations,
  iterations_completed = as.integer(result$n_itr),
  min_s = as.numeric(result$min),
  median_s = as.numeric(result$median),
  total_s = as.numeric(result$total_time),
  mem_alloc_bytes = as.numeric(result$mem_alloc),
  gc_count = as.integer(result$n_gc),
  r_version = as.character(getRversion()),
  rzarrs_version = package_version("Rzarrs"),
  rarr_version = package_version("Rarr"),
  bench_version = package_version("bench"),
  stringsAsFactors = FALSE
)
saveRDS(result, file.path(out, "bench.rds"))
utils::write.csv(summary, file.path(out, "summary.csv"), row.names = FALSE)
write_environment(file.path(out, "session-info.txt"), args)
fixture_manifest <- file.path(args$store, "benchmark-fixture.dcf")
if (file.exists(fixture_manifest)) {
  file.copy(fixture_manifest, file.path(out, "fixture.dcf"), overwrite = TRUE)
}
print(summary, row.names = FALSE)
