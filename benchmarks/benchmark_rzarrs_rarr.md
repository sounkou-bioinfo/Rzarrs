Rzarrs versus Rarr: numeric Zarr read benchmark
================

- [Scope](#scope)
- [Method](#method)
- [Recorded environment](#recorded-environment)
  - [cold environment](#cold-environment)
  - [zarrista environment](#zarrista-environment)
  - [warm environment](#warm-environment)
  - [zarrista environment](#zarrista-environment-1)
- [Results](#results)
- [Zarrista context baseline](#zarrista-context-baseline)
- [Interpretation limits](#interpretation-limits)

## Scope

This report is the decision baseline before extending Rzarrs with
deferred materialization, ALTREP, or an asynchronous native API. It
compares the common, validated surface only: a complete local Zarr V3
`int32` array read into R.

The fixture generator makes two equivalent workloads:

- `numeric-uncompressed.zarr` uses only the bytes codec;
- `numeric-gzip.zarr` adds gzip level 1 after bytes.

The runner verifies bitwise-equivalent dimensions and values from Rzarrs
and Rarr before measuring either implementation. Zarr-VCF, ZIP stores,
strings, partial reads, transpose, and remote object stores are outside
this baseline. They require their own interoperable fixture and equality
contract.

Zarrista is included as a separate contextual point using the same
fixtures, CPU/NUMA placement, and thread limits. It materializes a
Python/NumPy array, not an R array, so it must never be read as an
R-binding comparison or folded into the Rzarrs-versus-Rarr speedup.

## Method

The driver uses `bench::mark()` for per-process elapsed-time and
allocation measurements. Each implementation runs in a separate, fresh R
process. The shell runner binds that process to one physical CPU and its
NUMA memory node with `taskset` and `numactl`, records GNU `time -v`,
and alternates tool order by replicate.

The baseline is deliberately single-threaded. Neither implementation has
a matched documented read-thread knob, so a multi-core taskset would not
establish equivalent thread budgets. The runner additionally pins
`BLOSC_NTHREADS`, `RAYON_NUM_THREADS`, `TOKIO_WORKER_THREADS`, and
OpenMP/BLAS thread variables to one; their exact values are recorded in
`environment.txt`. The Zarrista runner also sets `RAYON_NUM_THREADS=1`,
and records its exact upstream Git revision and Python environment. Warm
and cold cache data are distinct workloads and must never be pooled.

``` sh
Rscript tools/make_benchmark_fixtures.R \
  --out "$HOME/.cache/Rzarrs/benchmarks/fixtures"

tools/run_rzarrs_rarr_bench.sh \
  --fixtures "$HOME/.cache/Rzarrs/benchmarks/fixtures" \
  --out "$HOME/.cache/Rzarrs/benchmarks/results" \
  --cpuset 2 --numa-node 0 --mode warm --reps 5 --iterations 5

sudo tools/run_rzarrs_rarr_bench.sh \
  --fixtures "$HOME/.cache/Rzarrs/benchmarks/fixtures" \
  --out "$HOME/.cache/Rzarrs/benchmarks/results" \
  --cpuset 2 --numa-node 0 --mode cold --reps 5

# ZARRISTA_PYTHON has zarrista + numpy installed from ZARRISTA_REVISION.
tools/run_zarrista_bench.sh \
  --fixtures "$HOME/.cache/Rzarrs/benchmarks/fixtures" \
  --out "$HOME/.cache/Rzarrs/benchmarks/results" \
  --python "$ZARRISTA_PYTHON" --zarrista-revision "$ZARRISTA_REVISION" \
  --cpuset 2 --numa-node 0 --mode warm --reps 5 --iterations 5

RZARRS_BENCH_RESULTS="$HOME/.cache/Rzarrs/benchmarks/results" \
  Rscript -e 'rmarkdown::render("benchmarks/benchmark_rzarrs_rarr.Rmd")'
```

## Recorded environment

Every run directory contains `bench.rds`, `summary.csv`, GNU `time -v`
output, the exact command line, R session information, and its fixture
manifest. Each cache-mode directory has `environment.txt`, including
source revision/status, CPU and NUMA topology, package versions, and run
configuration. The report below only aggregates artifacts that share
this result directory.

``` r
read_time_v <- function(path) {
  if (!file.exists(path)) return(c(max_rss_mib = NA_real_, cpu_percent = NA_real_))
  lines <- readLines(path, warn = FALSE)
  rss_line <- grep("Maximum resident set size", lines, value = TRUE)
  cpu_line <- grep("Percent of CPU this job got", lines, value = TRUE)
  rss_kib <- if (length(rss_line)) {
    as.numeric(sub(".*: *", "", rss_line[[1L]]))
  } else {
    NA_real_
  }
  cpu_percent <- if (length(cpu_line)) {
    as.numeric(sub("%.*", "", sub(".*: *", "", cpu_line[[1L]])))
  } else {
    NA_real_
  }
  c(max_rss_mib = rss_kib / 1024, cpu_percent = cpu_percent)
}

read_manifest <- function(path) {
  manifest <- file.path(dirname(path), "fixture.dcf")
  if (!file.exists(manifest)) return(c(logical_bytes = NA_real_, codec = NA_character_))
  lines <- trimws(readLines(manifest, warn = FALSE))
  pairs <- regmatches(lines, regexec("^([^:=]+)[:=] *(.*)$", lines))
  if (any(lengths(pairs) != 3L)) stop("malformed fixture manifest: ", manifest)
  fields <- stats::setNames(
    vapply(pairs, `[[`, character(1L), 3L),
    vapply(pairs, `[[`, character(1L), 2L)
  )
  c(
    logical_bytes = as.numeric(fields[["logical_bytes"]]),
    codec = unname(fields[["codec"]])
  )
}

read_run <- function(path) {
  result <- utils::read.csv(path, stringsAsFactors = FALSE)
  if (!"runtime" %in% names(result)) result$runtime <- "R"
  common_fields <- c(
    "implementation", "runtime", "mode", "store", "iterations_requested",
    "iterations_completed", "min_s", "median_s", "mean_s", "total_s",
    "mem_alloc_bytes", "gc_count", "r_version", "rzarrs_version",
    "rarr_version", "bench_version"
  )
  for (field in setdiff(common_fields, names(result))) result[[field]] <- NA
  result <- result[common_fields]
  result$run_dir <- dirname(path)
  result$fixture <- basename(result$store)
  manifest <- read_manifest(path)
  result$logical_bytes <- as.numeric(manifest[["logical_bytes"]])
  result$codec <- manifest[["codec"]]
  resources <- read_time_v(file.path(result$run_dir, "time-v.txt"))
  result$max_rss_mib <- resources[["max_rss_mib"]]
  result$cpu_percent <- resources[["cpu_percent"]]
  result
}

if (has_results) {
  summary_paths <- list.files(
    results_dir, pattern = "summary\\.csv$", recursive = TRUE, full.names = TRUE
  )
  if (!length(summary_paths)) stop("no summary.csv files under: ", results_dir)
  runs <- do.call(rbind, lapply(summary_paths, read_run))
  runs$throughput_mib_s <- runs$logical_bytes / runs$median_s / 1024^2
  runs
} else {
  data.frame()
}
#>    implementation runtime mode
#> 1            Rarr       R cold
#> 2            Rarr       R cold
#> 3            Rarr       R cold
#> 4            Rarr       R cold
#> 5            Rarr       R cold
#> 6          Rzarrs       R cold
#> 7          Rzarrs       R cold
#> 8          Rzarrs       R cold
#> 9          Rzarrs       R cold
#> 10         Rzarrs       R cold
#> 11           Rarr       R cold
#> 12           Rarr       R cold
#> 13           Rarr       R cold
#> 14           Rarr       R cold
#> 15           Rarr       R cold
#> 16         Rzarrs       R cold
#> 17         Rzarrs       R cold
#> 18         Rzarrs       R cold
#> 19         Rzarrs       R cold
#> 20         Rzarrs       R cold
#> 21       Zarrista  Python cold
#> 22       Zarrista  Python cold
#> 23       Zarrista  Python cold
#> 24       Zarrista  Python cold
#> 25       Zarrista  Python cold
#> 26       Zarrista  Python cold
#> 27       Zarrista  Python cold
#> 28       Zarrista  Python cold
#> 29       Zarrista  Python cold
#> 30       Zarrista  Python cold
#> 31           Rarr       R warm
#> 32           Rarr       R warm
#> 33           Rarr       R warm
#> 34           Rarr       R warm
#> 35           Rarr       R warm
#> 36         Rzarrs       R warm
#> 37         Rzarrs       R warm
#> 38         Rzarrs       R warm
#> 39         Rzarrs       R warm
#> 40         Rzarrs       R warm
#> 41           Rarr       R warm
#> 42           Rarr       R warm
#> 43           Rarr       R warm
#> 44           Rarr       R warm
#> 45           Rarr       R warm
#> 46         Rzarrs       R warm
#> 47         Rzarrs       R warm
#> 48         Rzarrs       R warm
#> 49         Rzarrs       R warm
#> 50         Rzarrs       R warm
#> 51       Zarrista  Python warm
#> 52       Zarrista  Python warm
#> 53       Zarrista  Python warm
#> 54       Zarrista  Python warm
#> 55       Zarrista  Python warm
#> 56       Zarrista  Python warm
#> 57       Zarrista  Python warm
#> 58       Zarrista  Python warm
#> 59       Zarrista  Python warm
#> 60       Zarrista  Python warm
#>                                                  store iterations_requested
#> 1          /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    1
#> 2          /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    1
#> 3          /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    1
#> 4          /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    1
#> 5          /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    1
#> 6          /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    1
#> 7          /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    1
#> 8          /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    1
#> 9          /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    1
#> 10         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    1
#> 11 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    1
#> 12 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    1
#> 13 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    1
#> 14 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    1
#> 15 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    1
#> 16 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    1
#> 17 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    1
#> 18 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    1
#> 19 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    1
#> 20 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    1
#> 21         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    1
#> 22         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    1
#> 23         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    1
#> 24         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    1
#> 25         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    1
#> 26 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    1
#> 27 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    1
#> 28 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    1
#> 29 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    1
#> 30 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    1
#> 31         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    5
#> 32         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    5
#> 33         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    5
#> 34         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    5
#> 35         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    5
#> 36         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    5
#> 37         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    5
#> 38         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    5
#> 39         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    5
#> 40         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    5
#> 41 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    5
#> 42 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    5
#> 43 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    5
#> 44 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    5
#> 45 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    5
#> 46 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    5
#> 47 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    5
#> 48 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    5
#> 49 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    5
#> 50 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    5
#> 51         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    5
#> 52         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    5
#> 53         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    5
#> 54         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    5
#> 55         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr                    5
#> 56 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    5
#> 57 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    5
#> 58 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    5
#> 59 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    5
#> 60 /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr                    5
#>    iterations_completed      min_s   median_s     mean_s   total_s
#> 1                     1 0.31599651 0.31599651         NA 0.3159965
#> 2                     1 0.31765519 0.31765519         NA 0.3176552
#> 3                     1 0.32197893 0.32197893         NA 0.3219789
#> 4                     1 0.31720492 0.31720492         NA 0.3172049
#> 5                     1 0.32083730 0.32083730         NA 0.3208373
#> 6                     1 0.85746759 0.85746759         NA 0.8574676
#> 7                     1 0.85168095 0.85168095         NA 0.8516809
#> 8                     1 0.85355194 0.85355194         NA 0.8535519
#> 9                     1 0.85967981 0.85967981         NA 0.8596798
#> 10                    1 0.86041003 0.86041003         NA 0.8604100
#> 11                    1 0.17995519 0.17995519         NA 0.1799552
#> 12                    1 0.18380203 0.18380203         NA 0.1838020
#> 13                    1 0.18096327 0.18096327         NA 0.1809633
#> 14                    1 0.18198819 0.18198819         NA 0.1819882
#> 15                    1 0.17965752 0.17965752         NA 0.1796575
#> 16                    1 0.73387640 0.73387640         NA 0.7338764
#> 17                    1 0.72903540 0.72903540         NA 0.7290354
#> 18                    1 0.74046821 0.74046821         NA 0.7404682
#> 19                    1 0.73478915 0.73478915         NA 0.7347892
#> 20                    1 0.72963147 0.72963147         NA 0.7296315
#> 21                    1 0.26010010 0.26010010 0.26010010 0.2601001
#> 22                    1 0.25250842 0.25250842 0.25250842 0.2525084
#> 23                    1 0.25455625 0.25455625 0.25455625 0.2545562
#> 24                    1 0.25374630 0.25374630 0.25374630 0.2537463
#> 25                    1 0.25542741 0.25542741 0.25542741 0.2554274
#> 26                    1 0.18308101 0.18308101 0.18308101 0.1830810
#> 27                    1 0.17812832 0.17812832 0.17812832 0.1781283
#> 28                    1 0.17896134 0.17896134 0.17896134 0.1789613
#> 29                    1 0.17865147 0.17865147 0.17865147 0.1786515
#> 30                    1 0.18353335 0.18353335 0.18353335 0.1835334
#> 31                    5 0.26347468 0.29052644         NA 1.4565345
#> 32                    5 0.26450352 0.29423072         NA 1.4670741
#> 33                    5 0.26477590 0.29061265         NA 1.4642787
#> 34                    5 0.26909645 0.28907810         NA 1.4698760
#> 35                    5 0.26626920 0.29336680         NA 1.4769648
#> 36                    5 0.83576277 0.84436259         NA 4.2415980
#> 37                    5 0.82781512 0.84026756         NA 4.2109205
#> 38                    5 0.83157292 0.83707326         NA 4.2157881
#> 39                    5 0.82615969 0.83594256         NA 4.2041925
#> 40                    5 0.82609125 0.83448202         NA 4.2123609
#> 41                    5 0.14048681 0.14435522         NA 0.7271419
#> 42                    5 0.14091057 0.14421901         NA 0.7274651
#> 43                    5 0.14062472 0.14358213         NA 0.7257756
#> 44                    5 0.13817422 0.14376706         NA 0.7161481
#> 45                    5 0.13874239 0.14371393         NA 0.7242585
#> 46                    5 0.70189920 0.71215310         NA 3.6312625
#> 47                    5 0.70885757 0.72301229         NA 3.6443612
#> 48                    5 0.70481503 0.70908980         NA 3.6017998
#> 49                    5 0.70816702 0.72356946         NA 3.6301896
#> 50                    5 0.71039815 0.72389632         NA 3.6396807
#> 51                    5 0.16330017 0.16349549 0.16412651 0.8206326
#> 52                    5 0.16311178 0.16368497 0.16379692 0.8189846
#> 53                    5 0.16217038 0.16440826 0.16432453 0.8216227
#> 54                    5 0.16081855 0.16175614 0.16225143 0.8112571
#> 55                    5 0.16175194 0.16277824 0.16289980 0.8144990
#> 56                    5 0.02351166 0.02436167 0.02465782 0.1232891
#> 57                    5 0.02389186 0.02476281 0.02480405 0.1240203
#> 58                    5 0.02389761 0.02469123 0.02464208 0.1232104
#> 59                    5 0.02402825 0.02495804 0.02495052 0.1247526
#> 60                    5 0.02468920 0.02485490 0.02511981 0.1255990
#>    mem_alloc_bytes gc_count r_version rzarrs_version rarr_version bench_version
#> 1        427535464        5     4.6.0          0.1.0       2.1.35         1.1.4
#> 2        427535464        5     4.6.0          0.1.0       2.1.35         1.1.4
#> 3        427535464        5     4.6.0          0.1.0       2.1.35         1.1.4
#> 4        427535464        5     4.6.0          0.1.0       2.1.35         1.1.4
#> 5        427535464        5     4.6.0          0.1.0       2.1.35         1.1.4
#> 6         67200256        1     4.6.0          0.1.0       2.1.35         1.1.4
#> 7         67200256        1     4.6.0          0.1.0       2.1.35         1.1.4
#> 8         67200256        1     4.6.0          0.1.0       2.1.35         1.1.4
#> 9         67200256        1     4.6.0          0.1.0       2.1.35         1.1.4
#> 10        67200256        1     4.6.0          0.1.0       2.1.35         1.1.4
#> 11       337191856        5     4.6.0          0.1.0       2.1.35         1.1.4
#> 12       337191856        5     4.6.0          0.1.0       2.1.35         1.1.4
#> 13       337191856        5     4.6.0          0.1.0       2.1.35         1.1.4
#> 14       337191856        5     4.6.0          0.1.0       2.1.35         1.1.4
#> 15       337191856        5     4.6.0          0.1.0       2.1.35         1.1.4
#> 16        67200256        1     4.6.0          0.1.0       2.1.35         1.1.4
#> 17        67200256        1     4.6.0          0.1.0       2.1.35         1.1.4
#> 18        67200256        1     4.6.0          0.1.0       2.1.35         1.1.4
#> 19        67200256        1     4.6.0          0.1.0       2.1.35         1.1.4
#> 20        67200256        1     4.6.0          0.1.0       2.1.35         1.1.4
#> 21              NA       NA      <NA>           <NA>         <NA>          <NA>
#> 22              NA       NA      <NA>           <NA>         <NA>          <NA>
#> 23              NA       NA      <NA>           <NA>         <NA>          <NA>
#> 24              NA       NA      <NA>           <NA>         <NA>          <NA>
#> 25              NA       NA      <NA>           <NA>         <NA>          <NA>
#> 26              NA       NA      <NA>           <NA>         <NA>          <NA>
#> 27              NA       NA      <NA>           <NA>         <NA>          <NA>
#> 28              NA       NA      <NA>           <NA>         <NA>          <NA>
#> 29              NA       NA      <NA>           <NA>         <NA>          <NA>
#> 30              NA       NA      <NA>           <NA>         <NA>          <NA>
#> 31       426562040       28     4.6.0          0.1.0       2.1.35         1.1.4
#> 32       426562040       28     4.6.0          0.1.0       2.1.35         1.1.4
#> 33       426562040       28     4.6.0          0.1.0       2.1.35         1.1.4
#> 34       426562040       28     4.6.0          0.1.0       2.1.35         1.1.4
#> 35       426562040       28     4.6.0          0.1.0       2.1.35         1.1.4
#> 36        67144792        4     4.6.0          0.1.0       2.1.35         1.1.4
#> 37        67144792        4     4.6.0          0.1.0       2.1.35         1.1.4
#> 38        67144792        4     4.6.0          0.1.0       2.1.35         1.1.4
#> 39        67144792        4     4.6.0          0.1.0       2.1.35         1.1.4
#> 40        67144792        4     4.6.0          0.1.0       2.1.35         1.1.4
#> 41       336228936       19     4.6.0          0.1.0       2.1.35         1.1.4
#> 42       336228936       19     4.6.0          0.1.0       2.1.35         1.1.4
#> 43       336228936       19     4.6.0          0.1.0       2.1.35         1.1.4
#> 44       336228936       19     4.6.0          0.1.0       2.1.35         1.1.4
#> 45       336228936       19     4.6.0          0.1.0       2.1.35         1.1.4
#> 46        67144792        4     4.6.0          0.1.0       2.1.35         1.1.4
#> 47        67144792        4     4.6.0          0.1.0       2.1.35         1.1.4
#> 48        67144792        4     4.6.0          0.1.0       2.1.35         1.1.4
#> 49        67144792        4     4.6.0          0.1.0       2.1.35         1.1.4
#> 50        67144792        4     4.6.0          0.1.0       2.1.35         1.1.4
#> 51              NA       NA      <NA>           <NA>         <NA>          <NA>
#> 52              NA       NA      <NA>           <NA>         <NA>          <NA>
#> 53              NA       NA      <NA>           <NA>         <NA>          <NA>
#> 54              NA       NA      <NA>           <NA>         <NA>          <NA>
#> 55              NA       NA      <NA>           <NA>         <NA>          <NA>
#> 56              NA       NA      <NA>           <NA>         <NA>          <NA>
#> 57              NA       NA      <NA>           <NA>         <NA>          <NA>
#> 58              NA       NA      <NA>           <NA>         <NA>          <NA>
#> 59              NA       NA      <NA>           <NA>         <NA>          <NA>
#> 60              NA       NA      <NA>           <NA>         <NA>          <NA>
#>                                                                            run_dir
#> 1                       /tmp/rzarrs-rarr-results/cold/numeric-gzip.zarr/Rarr/rep-1
#> 2                       /tmp/rzarrs-rarr-results/cold/numeric-gzip.zarr/Rarr/rep-2
#> 3                       /tmp/rzarrs-rarr-results/cold/numeric-gzip.zarr/Rarr/rep-3
#> 4                       /tmp/rzarrs-rarr-results/cold/numeric-gzip.zarr/Rarr/rep-4
#> 5                       /tmp/rzarrs-rarr-results/cold/numeric-gzip.zarr/Rarr/rep-5
#> 6                     /tmp/rzarrs-rarr-results/cold/numeric-gzip.zarr/Rzarrs/rep-1
#> 7                     /tmp/rzarrs-rarr-results/cold/numeric-gzip.zarr/Rzarrs/rep-2
#> 8                     /tmp/rzarrs-rarr-results/cold/numeric-gzip.zarr/Rzarrs/rep-3
#> 9                     /tmp/rzarrs-rarr-results/cold/numeric-gzip.zarr/Rzarrs/rep-4
#> 10                    /tmp/rzarrs-rarr-results/cold/numeric-gzip.zarr/Rzarrs/rep-5
#> 11              /tmp/rzarrs-rarr-results/cold/numeric-uncompressed.zarr/Rarr/rep-1
#> 12              /tmp/rzarrs-rarr-results/cold/numeric-uncompressed.zarr/Rarr/rep-2
#> 13              /tmp/rzarrs-rarr-results/cold/numeric-uncompressed.zarr/Rarr/rep-3
#> 14              /tmp/rzarrs-rarr-results/cold/numeric-uncompressed.zarr/Rarr/rep-4
#> 15              /tmp/rzarrs-rarr-results/cold/numeric-uncompressed.zarr/Rarr/rep-5
#> 16            /tmp/rzarrs-rarr-results/cold/numeric-uncompressed.zarr/Rzarrs/rep-1
#> 17            /tmp/rzarrs-rarr-results/cold/numeric-uncompressed.zarr/Rzarrs/rep-2
#> 18            /tmp/rzarrs-rarr-results/cold/numeric-uncompressed.zarr/Rzarrs/rep-3
#> 19            /tmp/rzarrs-rarr-results/cold/numeric-uncompressed.zarr/Rzarrs/rep-4
#> 20            /tmp/rzarrs-rarr-results/cold/numeric-uncompressed.zarr/Rzarrs/rep-5
#> 21         /tmp/rzarrs-rarr-results/cold/zarrista/numeric-gzip.zarr/Zarrista/rep-1
#> 22         /tmp/rzarrs-rarr-results/cold/zarrista/numeric-gzip.zarr/Zarrista/rep-2
#> 23         /tmp/rzarrs-rarr-results/cold/zarrista/numeric-gzip.zarr/Zarrista/rep-3
#> 24         /tmp/rzarrs-rarr-results/cold/zarrista/numeric-gzip.zarr/Zarrista/rep-4
#> 25         /tmp/rzarrs-rarr-results/cold/zarrista/numeric-gzip.zarr/Zarrista/rep-5
#> 26 /tmp/rzarrs-rarr-results/cold/zarrista/numeric-uncompressed.zarr/Zarrista/rep-1
#> 27 /tmp/rzarrs-rarr-results/cold/zarrista/numeric-uncompressed.zarr/Zarrista/rep-2
#> 28 /tmp/rzarrs-rarr-results/cold/zarrista/numeric-uncompressed.zarr/Zarrista/rep-3
#> 29 /tmp/rzarrs-rarr-results/cold/zarrista/numeric-uncompressed.zarr/Zarrista/rep-4
#> 30 /tmp/rzarrs-rarr-results/cold/zarrista/numeric-uncompressed.zarr/Zarrista/rep-5
#> 31                      /tmp/rzarrs-rarr-results/warm/numeric-gzip.zarr/Rarr/rep-1
#> 32                      /tmp/rzarrs-rarr-results/warm/numeric-gzip.zarr/Rarr/rep-2
#> 33                      /tmp/rzarrs-rarr-results/warm/numeric-gzip.zarr/Rarr/rep-3
#> 34                      /tmp/rzarrs-rarr-results/warm/numeric-gzip.zarr/Rarr/rep-4
#> 35                      /tmp/rzarrs-rarr-results/warm/numeric-gzip.zarr/Rarr/rep-5
#> 36                    /tmp/rzarrs-rarr-results/warm/numeric-gzip.zarr/Rzarrs/rep-1
#> 37                    /tmp/rzarrs-rarr-results/warm/numeric-gzip.zarr/Rzarrs/rep-2
#> 38                    /tmp/rzarrs-rarr-results/warm/numeric-gzip.zarr/Rzarrs/rep-3
#> 39                    /tmp/rzarrs-rarr-results/warm/numeric-gzip.zarr/Rzarrs/rep-4
#> 40                    /tmp/rzarrs-rarr-results/warm/numeric-gzip.zarr/Rzarrs/rep-5
#> 41              /tmp/rzarrs-rarr-results/warm/numeric-uncompressed.zarr/Rarr/rep-1
#> 42              /tmp/rzarrs-rarr-results/warm/numeric-uncompressed.zarr/Rarr/rep-2
#> 43              /tmp/rzarrs-rarr-results/warm/numeric-uncompressed.zarr/Rarr/rep-3
#> 44              /tmp/rzarrs-rarr-results/warm/numeric-uncompressed.zarr/Rarr/rep-4
#> 45              /tmp/rzarrs-rarr-results/warm/numeric-uncompressed.zarr/Rarr/rep-5
#> 46            /tmp/rzarrs-rarr-results/warm/numeric-uncompressed.zarr/Rzarrs/rep-1
#> 47            /tmp/rzarrs-rarr-results/warm/numeric-uncompressed.zarr/Rzarrs/rep-2
#> 48            /tmp/rzarrs-rarr-results/warm/numeric-uncompressed.zarr/Rzarrs/rep-3
#> 49            /tmp/rzarrs-rarr-results/warm/numeric-uncompressed.zarr/Rzarrs/rep-4
#> 50            /tmp/rzarrs-rarr-results/warm/numeric-uncompressed.zarr/Rzarrs/rep-5
#> 51         /tmp/rzarrs-rarr-results/warm/zarrista/numeric-gzip.zarr/Zarrista/rep-1
#> 52         /tmp/rzarrs-rarr-results/warm/zarrista/numeric-gzip.zarr/Zarrista/rep-2
#> 53         /tmp/rzarrs-rarr-results/warm/zarrista/numeric-gzip.zarr/Zarrista/rep-3
#> 54         /tmp/rzarrs-rarr-results/warm/zarrista/numeric-gzip.zarr/Zarrista/rep-4
#> 55         /tmp/rzarrs-rarr-results/warm/zarrista/numeric-gzip.zarr/Zarrista/rep-5
#> 56 /tmp/rzarrs-rarr-results/warm/zarrista/numeric-uncompressed.zarr/Zarrista/rep-1
#> 57 /tmp/rzarrs-rarr-results/warm/zarrista/numeric-uncompressed.zarr/Zarrista/rep-2
#> 58 /tmp/rzarrs-rarr-results/warm/zarrista/numeric-uncompressed.zarr/Zarrista/rep-3
#> 59 /tmp/rzarrs-rarr-results/warm/zarrista/numeric-uncompressed.zarr/Zarrista/rep-4
#> 60 /tmp/rzarrs-rarr-results/warm/zarrista/numeric-uncompressed.zarr/Zarrista/rep-5
#>                      fixture logical_bytes         codec max_rss_mib
#> 1          numeric-gzip.zarr      67108864 gzip(level=1)    394.3203
#> 2          numeric-gzip.zarr      67108864 gzip(level=1)    394.4766
#> 3          numeric-gzip.zarr      67108864 gzip(level=1)    394.3164
#> 4          numeric-gzip.zarr      67108864 gzip(level=1)    394.4805
#> 5          numeric-gzip.zarr      67108864 gzip(level=1)    394.3203
#> 6          numeric-gzip.zarr      67108864 gzip(level=1)    314.9844
#> 7          numeric-gzip.zarr      67108864 gzip(level=1)    314.9883
#> 8          numeric-gzip.zarr      67108864 gzip(level=1)    314.8320
#> 9          numeric-gzip.zarr      67108864 gzip(level=1)    314.8359
#> 10         numeric-gzip.zarr      67108864 gzip(level=1)    315.1406
#> 11 numeric-uncompressed.zarr      67108864         bytes    369.1641
#> 12 numeric-uncompressed.zarr      67108864         bytes    369.0078
#> 13 numeric-uncompressed.zarr      67108864         bytes    369.1680
#> 14 numeric-uncompressed.zarr      67108864         bytes    369.1641
#> 15 numeric-uncompressed.zarr      67108864         bytes    369.3203
#> 16 numeric-uncompressed.zarr      67108864         bytes    314.0859
#> 17 numeric-uncompressed.zarr      67108864         bytes    313.9336
#> 18 numeric-uncompressed.zarr      67108864         bytes    313.9336
#> 19 numeric-uncompressed.zarr      67108864         bytes    314.2461
#> 20 numeric-uncompressed.zarr      67108864         bytes    314.0859
#> 21         numeric-gzip.zarr      67108864 gzip(level=1)    106.5273
#> 22         numeric-gzip.zarr      67108864 gzip(level=1)    106.6719
#> 23         numeric-gzip.zarr      67108864 gzip(level=1)    106.5195
#> 24         numeric-gzip.zarr      67108864 gzip(level=1)    106.5117
#> 25         numeric-gzip.zarr      67108864 gzip(level=1)    106.6758
#> 26 numeric-uncompressed.zarr      67108864         bytes    105.6289
#> 27 numeric-uncompressed.zarr      67108864         bytes    105.9336
#> 28 numeric-uncompressed.zarr      67108864         bytes    105.8984
#> 29 numeric-uncompressed.zarr      67108864         bytes    105.8555
#> 30 numeric-uncompressed.zarr      67108864         bytes    105.8984
#> 31         numeric-gzip.zarr      67108864 gzip(level=1)    434.4453
#> 32         numeric-gzip.zarr      67108864 gzip(level=1)    434.4453
#> 33         numeric-gzip.zarr      67108864 gzip(level=1)    434.4453
#> 34         numeric-gzip.zarr      67108864 gzip(level=1)    434.2891
#> 35         numeric-gzip.zarr      67108864 gzip(level=1)    434.2930
#> 36         numeric-gzip.zarr      67108864 gzip(level=1)    443.4414
#> 37         numeric-gzip.zarr      67108864 gzip(level=1)    443.4336
#> 38         numeric-gzip.zarr      67108864 gzip(level=1)    443.4297
#> 39         numeric-gzip.zarr      67108864 gzip(level=1)    443.2773
#> 40         numeric-gzip.zarr      67108864 gzip(level=1)    443.4375
#> 41 numeric-uncompressed.zarr      67108864         bytes    397.8945
#> 42 numeric-uncompressed.zarr      67108864         bytes    397.8945
#> 43 numeric-uncompressed.zarr      67108864         bytes    397.8984
#> 44 numeric-uncompressed.zarr      67108864         bytes    397.7383
#> 45 numeric-uncompressed.zarr      67108864         bytes    398.0508
#> 46 numeric-uncompressed.zarr      67108864         bytes    442.3633
#> 47 numeric-uncompressed.zarr      67108864         bytes    442.3711
#> 48 numeric-uncompressed.zarr      67108864         bytes    442.6914
#> 49 numeric-uncompressed.zarr      67108864         bytes    442.2070
#> 50 numeric-uncompressed.zarr      67108864         bytes    442.3672
#> 51         numeric-gzip.zarr      67108864 gzip(level=1)    106.7773
#> 52         numeric-gzip.zarr      67108864 gzip(level=1)    107.0938
#> 53         numeric-gzip.zarr      67108864 gzip(level=1)    106.9102
#> 54         numeric-gzip.zarr      67108864 gzip(level=1)    106.9336
#> 55         numeric-gzip.zarr      67108864 gzip(level=1)    106.9375
#> 56 numeric-uncompressed.zarr      67108864         bytes    106.0391
#> 57 numeric-uncompressed.zarr      67108864         bytes    106.0469
#> 58 numeric-uncompressed.zarr      67108864         bytes    106.2422
#> 59 numeric-uncompressed.zarr      67108864         bytes    106.0391
#> 60 numeric-uncompressed.zarr      67108864         bytes    106.0938
#>    cpu_percent throughput_mib_s
#> 1           89        202.53388
#> 2           89        201.47632
#> 3           89        198.77077
#> 4           89        201.76232
#> 5           89        199.47805
#> 6           92         74.63839
#> 7           93         75.14551
#> 8           93         74.98079
#> 9           93         74.44632
#> 10          93         74.38314
#> 11          85        355.64409
#> 12          85        348.20073
#> 13          84        353.66293
#> 14          85        351.67117
#> 15          84        356.23335
#> 16          91         87.20815
#> 17          90         87.78723
#> 18          90         86.43180
#> 19          90         87.09982
#> 20          90         87.71551
#> 21          74        246.05912
#> 22          76        253.45690
#> 23          77        251.41791
#> 24          76        252.22043
#> 25          76        250.56042
#> 26          54        349.57202
#> 27          54        359.29155
#> 28          52        357.61913
#> 29          52        358.23942
#> 30          54        348.71046
#> 31          99        220.28976
#> 32          99        217.51638
#> 33          99        220.22441
#> 34          99        221.39346
#> 35          99        218.15693
#> 36          99         75.79682
#> 37          99         76.16622
#> 38          99         76.45687
#> 39          99         76.56028
#> 40          99         76.69428
#> 41          99        443.35078
#> 42          99        443.76952
#> 43          99        445.73793
#> 44          99        445.16457
#> 45          99        445.32914
#> 46          99         89.86832
#> 47          99         88.51855
#> 48          99         90.25655
#> 49          99         88.45039
#> 50          99         88.41045
#> 51          99        391.44810
#> 52          99        390.99497
#> 53          99        389.27484
#> 54         100        395.65731
#> 55          99        393.17294
#> 56          99       2627.07770
#> 57         100       2584.52098
#> 58         100       2592.01367
#> 59          99       2564.30425
#> 60          99       2574.94476
r_runs <- if (nrow(runs)) subset(runs, runtime == "R") else runs
zarrista_runs <- if (nrow(runs)) subset(runs, implementation == "Zarrista") else runs
```

``` r
if (has_results) {
  environment_paths <- list.files(
    results_dir, pattern = "^environment\\.txt$", recursive = TRUE, full.names = TRUE
  )
  if (!length(environment_paths)) stop("no environment.txt files under: ", results_dir)
  for (environment_path in environment_paths) {
    cat("### ", basename(dirname(environment_path)), " environment\n\n", sep = "")
    cat("```text\n")
    cat(readLines(environment_path, warn = FALSE), sep = "\n")
    cat("\n```\n\n")
  }
} else {
  cat("Render with `RZARRS_BENCH_RESULTS` set to a runner output directory.\n")
}
```

### cold environment

``` text
command: tools/run_rzarrs_rarr_bench.sh /tmp/rzarrs-rarr-results/cold/environment.txt --fixtures /tmp/rzarrs-rarr-fixtures --out /tmp/rzarrs-rarr-results --cpuset 0 --numa-node 0 --mode cold --reps 5
source_revision: ab0b24cc8b206e008a9a1d8d1ecf207cd03e15d8
source_status:
date_utc: 2026-08-17T19:42:23+00:00
uname: Linux Ubuntu-2404-noble-amd64-base 6.8.0-78-generic #78-Ubuntu SMP PREEMPT_DYNAMIC Tue Aug 12 11:34:18 UTC 2025 x86_64 x86_64 x86_64 GNU/Linux
cpuset: 0
numa_node: 0
mode: cold
reps: 5
warm_iterations: 5
thread_environment:
BLOSC_NTHREADS=1
OMP_NUM_THREADS=1
OPENBLAS_NUM_THREADS=1
MKL_NUM_THREADS=1
VECLIB_MAXIMUM_THREADS=1
RAYON_NUM_THREADS=1
TOKIO_WORKER_THREADS=1
lscpu:
Architecture:                         x86_64
CPU op-mode(s):                       32-bit, 64-bit
Address sizes:                        46 bits physical, 48 bits virtual
Byte Order:                           Little Endian
CPU(s):                               20
On-line CPU(s) list:                  0-19
Vendor ID:                            GenuineIntel
BIOS Vendor ID:                       Intel(R) Corporation
Model name:                           13th Gen Intel(R) Core(TM) i5-13500
BIOS Model name:                      13th Gen Intel(R) Core(TM) i5-13500 To Be Filled By O.E.M. CPU @ 2.4GHz
BIOS CPU family:                      205
CPU family:                           6
Model:                                191
Thread(s) per core:                   2
Core(s) per socket:                   14
Socket(s):                            1
Stepping:                             2
CPU(s) scaling MHz:                   35%
CPU max MHz:                          4800.0000
CPU min MHz:                          800.0000
BogoMIPS:                             4992.00
Flags:                                fpu vme de pse tsc msr pae mce cx8 apic sep mtrr pge mca cmov pat pse36 clflush dts acpi mmx fxsr sse sse2 ss ht tm pbe syscall nx pdpe1gb rdtscp lm constant_tsc art arch_perfmon pebs bts rep_good nopl xtopology nonstop_tsc cpuid aperfmperf tsc_known_freq pni pclmulqdq dtes64 monitor ds_cpl vmx smx est tm2 ssse3 sdbg fma cx16 xtpr pdcm sse4_1 sse4_2 x2apic movbe popcnt tsc_deadline_timer aes xsave avx f16c rdrand lahf_lm abm 3dnowprefetch cpuid_fault epb ssbd ibrs ibpb stibp ibrs_enhanced tpr_shadow flexpriority ept vpid ept_ad fsgsbase tsc_adjust bmi1 avx2 smep bmi2 erms invpcid rdseed adx smap clflushopt clwb intel_pt sha_ni xsaveopt xsavec xgetbv1 xsaves split_lock_detect user_shstk avx_vnni dtherm ida arat pln pts hwp hwp_notify hwp_act_window hwp_epp hwp_pkg_req hfi vnmi umip pku ospke waitpkg gfni vaes vpclmulqdq tme rdpid movdiri movdir64b fsrm md_clear serialize pconfig arch_lbr ibt flush_l1d arch_capabilities
Virtualization:                       VT-x
L1d cache:                            544 KiB (14 instances)
L1i cache:                            704 KiB (14 instances)
L2 cache:                             11.5 MiB (8 instances)
L3 cache:                             24 MiB (1 instance)
NUMA node(s):                         1
NUMA node0 CPU(s):                    0-19
Vulnerability Gather data sampling:   Not affected
Vulnerability Itlb multihit:          Not affected
Vulnerability L1tf:                   Not affected
Vulnerability Mds:                    Not affected
Vulnerability Meltdown:               Not affected
Vulnerability Mmio stale data:        Not affected
Vulnerability Reg file data sampling: Mitigation; Clear Register File
Vulnerability Retbleed:               Not affected
Vulnerability Spec rstack overflow:   Not affected
Vulnerability Spec store bypass:      Mitigation; Speculative Store Bypass disabled via prctl
Vulnerability Spectre v1:             Mitigation; usercopy/swapgs barriers and __user pointer sanitization
Vulnerability Spectre v2:             Mitigation; Enhanced / Automatic IBRS; IBPB conditional; RSB filling; PBRSB-eIBRS SW sequence; BHI BHI_DIS_S
Vulnerability Srbds:                  Not affected
Vulnerability Tsx async abort:        Not affected
numactl_hardware:
available: 1 nodes (0)
node 0 cpus: 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19
node 0 size: 64081 MB
node 0 free: 58375 MB
node distances:
node   0
  0:  10
r_packages:
Rzarrs=0.1.0
Rarr=2.1.35
bench=1.1.4
```

### zarrista environment

``` text
command: tools/run_zarrista_bench.sh /tmp/rzarrs-rarr-results/cold/zarrista/environment.txt --fixtures /tmp/rzarrs-rarr-fixtures --out /tmp/rzarrs-rarr-results --python /tmp/zarrista-venv/bin/python --zarrista-revision 92d26b65b90e9715d5c658c71b9216449f25ae64 --cpuset 0 --numa-node 0 --mode cold --reps 5
source_revision: ab0b24cc8b206e008a9a1d8d1ecf207cd03e15d8
source_status:
date_utc: 2026-08-17T19:43:09+00:00
uname: Linux Ubuntu-2404-noble-amd64-base 6.8.0-78-generic #78-Ubuntu SMP PREEMPT_DYNAMIC Tue Aug 12 11:34:18 UTC 2025 x86_64 x86_64 x86_64 GNU/Linux
cpuset: 0
numa_node: 0
mode: cold
reps: 5
warm_iterations: 5
zarrista_revision: 92d26b65b90e9715d5c658c71b9216449f25ae64
python: Python 3.13.12
zarrista:
0.1.0
/tmp/zarrista-venv/lib/python3.13/site-packages/zarrista/__init__.py
thread_environment:
BLOSC_NTHREADS=1
OMP_NUM_THREADS=1
OPENBLAS_NUM_THREADS=1
MKL_NUM_THREADS=1
VECLIB_MAXIMUM_THREADS=1
RAYON_NUM_THREADS=1
TOKIO_WORKER_THREADS=1
lscpu:
Architecture:                         x86_64
CPU op-mode(s):                       32-bit, 64-bit
Address sizes:                        46 bits physical, 48 bits virtual
Byte Order:                           Little Endian
CPU(s):                               20
On-line CPU(s) list:                  0-19
Vendor ID:                            GenuineIntel
BIOS Vendor ID:                       Intel(R) Corporation
Model name:                           13th Gen Intel(R) Core(TM) i5-13500
BIOS Model name:                      13th Gen Intel(R) Core(TM) i5-13500 To Be Filled By O.E.M. CPU @ 2.4GHz
BIOS CPU family:                      205
CPU family:                           6
Model:                                191
Thread(s) per core:                   2
Core(s) per socket:                   14
Socket(s):                            1
Stepping:                             2
CPU(s) scaling MHz:                   29%
CPU max MHz:                          4800.0000
CPU min MHz:                          800.0000
BogoMIPS:                             4992.00
Flags:                                fpu vme de pse tsc msr pae mce cx8 apic sep mtrr pge mca cmov pat pse36 clflush dts acpi mmx fxsr sse sse2 ss ht tm pbe syscall nx pdpe1gb rdtscp lm constant_tsc art arch_perfmon pebs bts rep_good nopl xtopology nonstop_tsc cpuid aperfmperf tsc_known_freq pni pclmulqdq dtes64 monitor ds_cpl vmx smx est tm2 ssse3 sdbg fma cx16 xtpr pdcm sse4_1 sse4_2 x2apic movbe popcnt tsc_deadline_timer aes xsave avx f16c rdrand lahf_lm abm 3dnowprefetch cpuid_fault epb ssbd ibrs ibpb stibp ibrs_enhanced tpr_shadow flexpriority ept vpid ept_ad fsgsbase tsc_adjust bmi1 avx2 smep bmi2 erms invpcid rdseed adx smap clflushopt clwb intel_pt sha_ni xsaveopt xsavec xgetbv1 xsaves split_lock_detect user_shstk avx_vnni dtherm ida arat pln pts hwp hwp_notify hwp_act_window hwp_epp hwp_pkg_req hfi vnmi umip pku ospke waitpkg gfni vaes vpclmulqdq tme rdpid movdiri movdir64b fsrm md_clear serialize pconfig arch_lbr ibt flush_l1d arch_capabilities
Virtualization:                       VT-x
L1d cache:                            544 KiB (14 instances)
L1i cache:                            704 KiB (14 instances)
L2 cache:                             11.5 MiB (8 instances)
L3 cache:                             24 MiB (1 instance)
NUMA node(s):                         1
NUMA node0 CPU(s):                    0-19
Vulnerability Gather data sampling:   Not affected
Vulnerability Itlb multihit:          Not affected
Vulnerability L1tf:                   Not affected
Vulnerability Mds:                    Not affected
Vulnerability Meltdown:               Not affected
Vulnerability Mmio stale data:        Not affected
Vulnerability Reg file data sampling: Mitigation; Clear Register File
Vulnerability Retbleed:               Not affected
Vulnerability Spec rstack overflow:   Not affected
Vulnerability Spec store bypass:      Mitigation; Speculative Store Bypass disabled via prctl
Vulnerability Spectre v1:             Mitigation; usercopy/swapgs barriers and __user pointer sanitization
Vulnerability Spectre v2:             Mitigation; Enhanced / Automatic IBRS; IBPB conditional; RSB filling; PBRSB-eIBRS SW sequence; BHI BHI_DIS_S
Vulnerability Srbds:                  Not affected
Vulnerability Tsx async abort:        Not affected
numactl_hardware:
available: 1 nodes (0)
node 0 cpus: 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19
node 0 size: 64081 MB
node 0 free: 58610 MB
node distances:
node   0
  0:  10
```

### warm environment

``` text
command: tools/run_rzarrs_rarr_bench.sh /tmp/rzarrs-rarr-results/warm/environment.txt --fixtures /tmp/rzarrs-rarr-fixtures --out /tmp/rzarrs-rarr-results --cpuset 0 --numa-node 0 --mode warm --reps 5 --iterations 5
source_revision: ab0b24cc8b206e008a9a1d8d1ecf207cd03e15d8
source_status:
date_utc: 2026-08-17T19:41:00+00:00
uname: Linux Ubuntu-2404-noble-amd64-base 6.8.0-78-generic #78-Ubuntu SMP PREEMPT_DYNAMIC Tue Aug 12 11:34:18 UTC 2025 x86_64 x86_64 x86_64 GNU/Linux
cpuset: 0
numa_node: 0
mode: warm
reps: 5
warm_iterations: 5
thread_environment:
BLOSC_NTHREADS=1
OMP_NUM_THREADS=1
OPENBLAS_NUM_THREADS=1
MKL_NUM_THREADS=1
VECLIB_MAXIMUM_THREADS=1
RAYON_NUM_THREADS=1
TOKIO_WORKER_THREADS=1
lscpu:
Architecture:                         x86_64
CPU op-mode(s):                       32-bit, 64-bit
Address sizes:                        46 bits physical, 48 bits virtual
Byte Order:                           Little Endian
CPU(s):                               20
On-line CPU(s) list:                  0-19
Vendor ID:                            GenuineIntel
BIOS Vendor ID:                       Intel(R) Corporation
Model name:                           13th Gen Intel(R) Core(TM) i5-13500
BIOS Model name:                      13th Gen Intel(R) Core(TM) i5-13500 To Be Filled By O.E.M. CPU @ 2.4GHz
BIOS CPU family:                      205
CPU family:                           6
Model:                                191
Thread(s) per core:                   2
Core(s) per socket:                   14
Socket(s):                            1
Stepping:                             2
CPU(s) scaling MHz:                   32%
CPU max MHz:                          4800.0000
CPU min MHz:                          800.0000
BogoMIPS:                             4992.00
Flags:                                fpu vme de pse tsc msr pae mce cx8 apic sep mtrr pge mca cmov pat pse36 clflush dts acpi mmx fxsr sse sse2 ss ht tm pbe syscall nx pdpe1gb rdtscp lm constant_tsc art arch_perfmon pebs bts rep_good nopl xtopology nonstop_tsc cpuid aperfmperf tsc_known_freq pni pclmulqdq dtes64 monitor ds_cpl vmx smx est tm2 ssse3 sdbg fma cx16 xtpr pdcm sse4_1 sse4_2 x2apic movbe popcnt tsc_deadline_timer aes xsave avx f16c rdrand lahf_lm abm 3dnowprefetch cpuid_fault epb ssbd ibrs ibpb stibp ibrs_enhanced tpr_shadow flexpriority ept vpid ept_ad fsgsbase tsc_adjust bmi1 avx2 smep bmi2 erms invpcid rdseed adx smap clflushopt clwb intel_pt sha_ni xsaveopt xsavec xgetbv1 xsaves split_lock_detect user_shstk avx_vnni dtherm ida arat pln pts hwp hwp_notify hwp_act_window hwp_epp hwp_pkg_req hfi vnmi umip pku ospke waitpkg gfni vaes vpclmulqdq tme rdpid movdiri movdir64b fsrm md_clear serialize pconfig arch_lbr ibt flush_l1d arch_capabilities
Virtualization:                       VT-x
L1d cache:                            544 KiB (14 instances)
L1i cache:                            704 KiB (14 instances)
L2 cache:                             11.5 MiB (8 instances)
L3 cache:                             24 MiB (1 instance)
NUMA node(s):                         1
NUMA node0 CPU(s):                    0-19
Vulnerability Gather data sampling:   Not affected
Vulnerability Itlb multihit:          Not affected
Vulnerability L1tf:                   Not affected
Vulnerability Mds:                    Not affected
Vulnerability Meltdown:               Not affected
Vulnerability Mmio stale data:        Not affected
Vulnerability Reg file data sampling: Mitigation; Clear Register File
Vulnerability Retbleed:               Not affected
Vulnerability Spec rstack overflow:   Not affected
Vulnerability Spec store bypass:      Mitigation; Speculative Store Bypass disabled via prctl
Vulnerability Spectre v1:             Mitigation; usercopy/swapgs barriers and __user pointer sanitization
Vulnerability Spectre v2:             Mitigation; Enhanced / Automatic IBRS; IBPB conditional; RSB filling; PBRSB-eIBRS SW sequence; BHI BHI_DIS_S
Vulnerability Srbds:                  Not affected
Vulnerability Tsx async abort:        Not affected
numactl_hardware:
available: 1 nodes (0)
node 0 cpus: 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19
node 0 size: 64081 MB
node 0 free: 58742 MB
node distances:
node   0
  0:  10
r_packages:
Rzarrs=0.1.0
Rarr=2.1.35
bench=1.1.4
```

### zarrista environment

``` text
command: tools/run_zarrista_bench.sh /tmp/rzarrs-rarr-results/warm/zarrista/environment.txt --fixtures /tmp/rzarrs-rarr-fixtures --out /tmp/rzarrs-rarr-results --python /tmp/zarrista-venv/bin/python --zarrista-revision 92d26b65b90e9715d5c658c71b9216449f25ae64 --cpuset 0 --numa-node 0 --mode warm --reps 5 --iterations 5
source_revision: ab0b24cc8b206e008a9a1d8d1ecf207cd03e15d8
source_status:
date_utc: 2026-08-17T19:43:02+00:00
uname: Linux Ubuntu-2404-noble-amd64-base 6.8.0-78-generic #78-Ubuntu SMP PREEMPT_DYNAMIC Tue Aug 12 11:34:18 UTC 2025 x86_64 x86_64 x86_64 GNU/Linux
cpuset: 0
numa_node: 0
mode: warm
reps: 5
warm_iterations: 5
zarrista_revision: 92d26b65b90e9715d5c658c71b9216449f25ae64
python: Python 3.13.12
zarrista:
0.1.0
/tmp/zarrista-venv/lib/python3.13/site-packages/zarrista/__init__.py
thread_environment:
BLOSC_NTHREADS=1
OMP_NUM_THREADS=1
OPENBLAS_NUM_THREADS=1
MKL_NUM_THREADS=1
VECLIB_MAXIMUM_THREADS=1
RAYON_NUM_THREADS=1
TOKIO_WORKER_THREADS=1
lscpu:
Architecture:                         x86_64
CPU op-mode(s):                       32-bit, 64-bit
Address sizes:                        46 bits physical, 48 bits virtual
Byte Order:                           Little Endian
CPU(s):                               20
On-line CPU(s) list:                  0-19
Vendor ID:                            GenuineIntel
BIOS Vendor ID:                       Intel(R) Corporation
Model name:                           13th Gen Intel(R) Core(TM) i5-13500
BIOS Model name:                      13th Gen Intel(R) Core(TM) i5-13500 To Be Filled By O.E.M. CPU @ 2.4GHz
BIOS CPU family:                      205
CPU family:                           6
Model:                                191
Thread(s) per core:                   2
Core(s) per socket:                   14
Socket(s):                            1
Stepping:                             2
CPU(s) scaling MHz:                   30%
CPU max MHz:                          4800.0000
CPU min MHz:                          800.0000
BogoMIPS:                             4992.00
Flags:                                fpu vme de pse tsc msr pae mce cx8 apic sep mtrr pge mca cmov pat pse36 clflush dts acpi mmx fxsr sse sse2 ss ht tm pbe syscall nx pdpe1gb rdtscp lm constant_tsc art arch_perfmon pebs bts rep_good nopl xtopology nonstop_tsc cpuid aperfmperf tsc_known_freq pni pclmulqdq dtes64 monitor ds_cpl vmx smx est tm2 ssse3 sdbg fma cx16 xtpr pdcm sse4_1 sse4_2 x2apic movbe popcnt tsc_deadline_timer aes xsave avx f16c rdrand lahf_lm abm 3dnowprefetch cpuid_fault epb ssbd ibrs ibpb stibp ibrs_enhanced tpr_shadow flexpriority ept vpid ept_ad fsgsbase tsc_adjust bmi1 avx2 smep bmi2 erms invpcid rdseed adx smap clflushopt clwb intel_pt sha_ni xsaveopt xsavec xgetbv1 xsaves split_lock_detect user_shstk avx_vnni dtherm ida arat pln pts hwp hwp_notify hwp_act_window hwp_epp hwp_pkg_req hfi vnmi umip pku ospke waitpkg gfni vaes vpclmulqdq tme rdpid movdiri movdir64b fsrm md_clear serialize pconfig arch_lbr ibt flush_l1d arch_capabilities
Virtualization:                       VT-x
L1d cache:                            544 KiB (14 instances)
L1i cache:                            704 KiB (14 instances)
L2 cache:                             11.5 MiB (8 instances)
L3 cache:                             24 MiB (1 instance)
NUMA node(s):                         1
NUMA node0 CPU(s):                    0-19
Vulnerability Gather data sampling:   Not affected
Vulnerability Itlb multihit:          Not affected
Vulnerability L1tf:                   Not affected
Vulnerability Mds:                    Not affected
Vulnerability Meltdown:               Not affected
Vulnerability Mmio stale data:        Not affected
Vulnerability Reg file data sampling: Mitigation; Clear Register File
Vulnerability Retbleed:               Not affected
Vulnerability Spec rstack overflow:   Not affected
Vulnerability Spec store bypass:      Mitigation; Speculative Store Bypass disabled via prctl
Vulnerability Spectre v1:             Mitigation; usercopy/swapgs barriers and __user pointer sanitization
Vulnerability Spectre v2:             Mitigation; Enhanced / Automatic IBRS; IBPB conditional; RSB filling; PBRSB-eIBRS SW sequence; BHI BHI_DIS_S
Vulnerability Srbds:                  Not affected
Vulnerability Tsx async abort:        Not affected
numactl_hardware:
available: 1 nodes (0)
node 0 cpus: 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19
node 0 size: 64081 MB
node 0 free: 58469 MB
node distances:
node   0
  0:  10
```

## Results

`bench` repeats inside a process. The table therefore reports the median
of each process’s `bench` median across process-level replicates.
`max_rss_mib` and `cpu_percent` are likewise process-level medians from
GNU `time -v`.

``` r
if (nrow(r_runs)) {
  reported <- stats::aggregate(
    r_runs[c("median_s", "mem_alloc_bytes", "throughput_mib_s", "max_rss_mib", "cpu_percent")],
    by = list(
      mode = r_runs$mode,
      fixture = r_runs$fixture,
      codec = r_runs$codec,
      implementation = r_runs$implementation
    ),
    FUN = stats::median,
    na.rm = TRUE
  )
  names(reported)[names(reported) == "Group.1"] <- "mode"
  names(reported)[names(reported) == "Group.2"] <- "fixture"
  names(reported)[names(reported) == "Group.3"] <- "codec"
  names(reported)[names(reported) == "Group.4"] <- "implementation"
  reported$median_s <- signif(reported$median_s, 5)
  reported$mem_alloc_mib <- signif(reported$mem_alloc_bytes / 1024^2, 5)
  reported$throughput_mib_s <- signif(reported$throughput_mib_s, 5)
  reported$max_rss_mib <- signif(reported$max_rss_mib, 5)
  reported$cpu_percent <- signif(reported$cpu_percent, 5)
  reported$mem_alloc_bytes <- NULL
  knitr::kable(reported, row.names = FALSE)
} else {
  cat("No benchmark artifacts supplied.\n")
}
```

| mode | fixture                   | codec         | implementation | median_s | throughput_mib_s | max_rss_mib | cpu_percent | mem_alloc_mib |
|:-----|:--------------------------|:--------------|:---------------|---------:|-----------------:|------------:|------------:|--------------:|
| cold | numeric-uncompressed.zarr | bytes         | Rarr           |  0.18096 |          353.660 |      369.16 |          85 |       321.570 |
| warm | numeric-uncompressed.zarr | bytes         | Rarr           |  0.14377 |          445.160 |      397.89 |          99 |       320.650 |
| cold | numeric-gzip.zarr         | gzip(level=1) | Rarr           |  0.31766 |          201.480 |      394.32 |          89 |       407.730 |
| warm | numeric-gzip.zarr         | gzip(level=1) | Rarr           |  0.29061 |          220.220 |      434.45 |          99 |       406.800 |
| cold | numeric-uncompressed.zarr | bytes         | Rzarrs         |  0.73388 |           87.208 |      314.09 |          90 |        64.087 |
| warm | numeric-uncompressed.zarr | bytes         | Rzarrs         |  0.72301 |           88.519 |      442.37 |          99 |        64.034 |
| cold | numeric-gzip.zarr         | gzip(level=1) | Rzarrs         |  0.85747 |           74.638 |      314.98 |          93 |        64.087 |
| warm | numeric-gzip.zarr         | gzip(level=1) | Rzarrs         |  0.83707 |           76.457 |      443.43 |          99 |        64.034 |

A speedup is only meaningful within one `mode`, fixture, codec, CPU
binding, and environment. For each matched pair below, a value above 1
means Rzarrs had the lower process-level `bench` median.

``` r
if (nrow(r_runs)) {
  per_run <- stats::aggregate(
    r_runs["median_s"],
    by = list(mode = r_runs$mode, fixture = r_runs$fixture, codec = r_runs$codec,
              implementation = r_runs$implementation),
    FUN = stats::median
  )
  wide <- reshape(
    per_run,
    idvar = c("mode", "fixture", "codec"),
    timevar = "implementation",
    direction = "wide"
  )
  if (all(c("median_s.Rzarrs", "median_s.Rarr") %in% names(wide))) {
    wide$rzarrs_speedup_over_rarr <- wide$median_s.Rarr / wide$median_s.Rzarrs
    wide$rzarrs_speedup_over_rarr <- signif(wide$rzarrs_speedup_over_rarr, 5)
    knitr::kable(wide, row.names = FALSE)
  } else {
    cat("Both implementations are required for a speedup calculation.\n")
  }
}
```

| mode | fixture                   | codec         | median_s.Rarr | median_s.Rzarrs | rzarrs_speedup_over_rarr |
|:-----|:--------------------------|:--------------|--------------:|----------------:|-------------------------:|
| cold | numeric-uncompressed.zarr | bytes         |     0.1809633 |       0.7338764 |                  0.24659 |
| warm | numeric-uncompressed.zarr | bytes         |     0.1437671 |       0.7230123 |                  0.19884 |
| cold | numeric-gzip.zarr         | gzip(level=1) |     0.3176552 |       0.8574676 |                  0.37046 |
| warm | numeric-gzip.zarr         | gzip(level=1) |     0.2906126 |       0.8370733 |                  0.34718 |

## Zarrista context baseline

Zarrista runs the same local V3 fixtures with the same CPU/NUMA and
native thread controls, but its result is a NumPy array. The table
intentionally omits R allocation measurements and does not calculate an
Rzarrs/Rarr-style speedup. It is a comparison point for a Rust-native
Python reader, not a prospective R binding measurement.

``` r
if (nrow(zarrista_runs)) {
  zarrista_report <- stats::aggregate(
    zarrista_runs[c("median_s", "throughput_mib_s", "max_rss_mib", "cpu_percent")],
    by = list(
      mode = zarrista_runs$mode,
      fixture = zarrista_runs$fixture,
      codec = zarrista_runs$codec
    ),
    FUN = stats::median,
    na.rm = TRUE
  )
  for (field in c("median_s", "throughput_mib_s", "max_rss_mib", "cpu_percent")) {
    zarrista_report[[field]] <- signif(as.numeric(zarrista_report[[field]]), 5)
  }
  knitr::kable(zarrista_report, row.names = FALSE)
} else {
  cat("No Zarrista artifacts supplied.\n")
}
```

| mode | fixture                   | codec         | median_s | throughput_mib_s | max_rss_mib | cpu_percent |
|:-----|:--------------------------|:--------------|---------:|-----------------:|------------:|------------:|
| cold | numeric-uncompressed.zarr | bytes         | 0.178960 |           357.62 |      105.90 |          54 |
| warm | numeric-uncompressed.zarr | bytes         | 0.024763 |          2584.50 |      106.05 |          99 |
| cold | numeric-gzip.zarr         | gzip(level=1) | 0.254560 |           251.42 |      106.53 |          76 |
| warm | numeric-gzip.zarr         | gzip(level=1) | 0.163500 |           391.45 |      106.93 |          99 |

## Interpretation limits

This report attributes no cost by itself. The difference between
bytes-only and gzip fixtures estimates the combined codec contribution
for this workload, not compression time in isolation. Profiling comes
next only if the measured result makes an extension worthwhile; it must
distinguish I/O, decompression, data layout conversion, and R object
materialization.
