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
- [Visual comparisons](#visual-comparisons)
- [Network-bounded S3 matrix](#network-bounded-s3-matrix)
- [Compilation provenance and controlled rebuild
  gate](#compilation-provenance-and-controlled-rebuild-gate)
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
allocation measurements. Package loading occurs before `bench::mark()`;
Zarrista likewise imports its Python API before `time.perf_counter()`
starts. Thus `median_s` measures a meaningful request—open the
store/array and fully materialize its payload—not R/Python process or
library startup. GNU `time -v` deliberately remains process-inclusive,
but is used only for peak RSS and CPU diagnostics, not the elapsed-time
denominator. Each implementation runs in a separate, fresh process. The
shell runner binds that process to one physical CPU and its NUMA memory
node with `taskset` and `numactl`, records GNU `time -v`, and alternates
tool order by replicate.

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
#> 1                     1 0.31603994 0.31603994         NA 0.3160399
#> 2                     1 0.32904216 0.32904216         NA 0.3290422
#> 3                     1 0.31678329 0.31678329         NA 0.3167833
#> 4                     1 0.31484004 0.31484004         NA 0.3148400
#> 5                     1 0.31549690 0.31549690         NA 0.3154969
#> 6                     1 0.85823738 0.85823738         NA 0.8582374
#> 7                     1 0.88729990 0.88729990         NA 0.8872999
#> 8                     1 0.85166806 0.85166806         NA 0.8516681
#> 9                     1 0.87299482 0.87299482         NA 0.8729948
#> 10                    1 0.85399855 0.85399855         NA 0.8539985
#> 11                    1 0.18549142 0.18549142         NA 0.1854914
#> 12                    1 0.18092794 0.18092794         NA 0.1809279
#> 13                    1 0.17847855 0.17847855         NA 0.1784786
#> 14                    1 0.18035478 0.18035478         NA 0.1803548
#> 15                    1 0.18386489 0.18386489         NA 0.1838649
#> 16                    1 0.73556148 0.73556148         NA 0.7355615
#> 17                    1 0.73625696 0.73625696         NA 0.7362570
#> 18                    1 0.76573469 0.76573469         NA 0.7657347
#> 19                    1 0.74702589 0.74702589         NA 0.7470259
#> 20                    1 0.74129033 0.74129033         NA 0.7412903
#> 21                    1 0.26596268 0.26596268 0.26596268 0.2659627
#> 22                    1 0.26167173 0.26167173 0.26167173 0.2616717
#> 23                    1 0.26045045 0.26045045 0.26045045 0.2604504
#> 24                    1 0.26138106 0.26138106 0.26138106 0.2613811
#> 25                    1 0.26201873 0.26201873 0.26201873 0.2620187
#> 26                    1 0.19019399 0.19019399 0.19019399 0.1901940
#> 27                    1 0.19023862 0.19023862 0.19023862 0.1902386
#> 28                    1 0.18437882 0.18437882 0.18437882 0.1843788
#> 29                    1 0.17982074 0.17982074 0.17982074 0.1798207
#> 30                    1 0.18962223 0.18962223 0.18962223 0.1896222
#> 31                    5 0.26647327 0.29011933         NA 1.4612487
#> 32                    5 0.26590727 0.29064253         NA 1.4611875
#> 33                    5 0.26454637 0.29026806         NA 1.4609315
#> 34                    5 0.26391916 0.29080303         NA 1.4577625
#> 35                    5 0.26255677 0.28952439         NA 1.4621039
#> 36                    5 0.83478512 0.85684983         NA 4.2670034
#> 37                    5 0.82087941 0.83135917         NA 4.2005790
#> 38                    5 0.82728028 0.83635938         NA 4.2086511
#> 39                    5 0.83043430 0.83448650         NA 4.2224146
#> 40                    5 0.82738273 0.83807966         NA 4.2180482
#> 41                    5 0.14128822 0.14503137         NA 0.7238185
#> 42                    5 0.14171986 0.14570346         NA 0.7297083
#> 43                    5 0.13879194 0.14315802         NA 0.7231311
#> 44                    5 0.13992403 0.14749039         NA 0.7272735
#> 45                    5 0.13980377 0.14635783         NA 0.7262257
#> 46                    5 0.71074654 0.72019015         NA 3.6286255
#> 47                    5 0.71118048 0.71660520         NA 3.6182070
#> 48                    5 0.70588468 0.72525671         NA 3.6098870
#> 49                    5 0.70224736 0.70801273         NA 3.5809895
#> 50                    5 0.70157044 0.70753109         NA 3.5877947
#> 51                    5 0.16156030 0.16364576 0.16348115 0.8174058
#> 52                    5 0.16217620 0.16281385 0.16314667 0.8157334
#> 53                    5 0.16162873 0.16299920 0.16307323 0.8153661
#> 54                    5 0.16321390 0.16479233 0.16489930 0.8244965
#> 55                    5 0.16211769 0.16404862 0.16385995 0.8192997
#> 56                    5 0.02340581 0.02487613 0.02469716 0.1234858
#> 57                    5 0.02339384 0.02439881 0.02457748 0.1228874
#> 58                    5 0.02381537 0.02526651 0.02492383 0.1246192
#> 59                    5 0.02418276 0.02508442 0.02500220 0.1250110
#> 60                    5 0.02371060 0.02424605 0.02457812 0.1228906
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
#> 1          numeric-gzip.zarr      67108864 gzip(level=1)    394.1641
#> 2          numeric-gzip.zarr      67108864 gzip(level=1)    394.4766
#> 3          numeric-gzip.zarr      67108864 gzip(level=1)    394.4766
#> 4          numeric-gzip.zarr      67108864 gzip(level=1)    394.4766
#> 5          numeric-gzip.zarr      67108864 gzip(level=1)    394.3203
#> 6          numeric-gzip.zarr      67108864 gzip(level=1)    315.1445
#> 7          numeric-gzip.zarr      67108864 gzip(level=1)    315.1484
#> 8          numeric-gzip.zarr      67108864 gzip(level=1)    314.8320
#> 9          numeric-gzip.zarr      67108864 gzip(level=1)    314.9883
#> 10         numeric-gzip.zarr      67108864 gzip(level=1)    314.9844
#> 11 numeric-uncompressed.zarr      67108864         bytes    369.3203
#> 12 numeric-uncompressed.zarr      67108864         bytes    369.0078
#> 13 numeric-uncompressed.zarr      67108864         bytes    369.1641
#> 14 numeric-uncompressed.zarr      67108864         bytes    369.3203
#> 15 numeric-uncompressed.zarr      67108864         bytes    369.3242
#> 16 numeric-uncompressed.zarr      67108864         bytes    313.9297
#> 17 numeric-uncompressed.zarr      67108864         bytes    314.0859
#> 18 numeric-uncompressed.zarr      67108864         bytes    313.9258
#> 19 numeric-uncompressed.zarr      67108864         bytes    314.2422
#> 20 numeric-uncompressed.zarr      67108864         bytes    313.9297
#> 21         numeric-gzip.zarr      67108864 gzip(level=1)    106.6914
#> 22         numeric-gzip.zarr      67108864 gzip(level=1)    106.6914
#> 23         numeric-gzip.zarr      67108864 gzip(level=1)    106.5352
#> 24         numeric-gzip.zarr      67108864 gzip(level=1)    106.6914
#> 25         numeric-gzip.zarr      67108864 gzip(level=1)    106.6953
#> 26 numeric-uncompressed.zarr      67108864         bytes    105.9531
#> 27 numeric-uncompressed.zarr      67108864         bytes    105.7266
#> 28 numeric-uncompressed.zarr      67108864         bytes    105.9570
#> 29 numeric-uncompressed.zarr      67108864         bytes    105.8906
#> 30 numeric-uncompressed.zarr      67108864         bytes    105.7305
#> 31         numeric-gzip.zarr      67108864 gzip(level=1)    434.6016
#> 32         numeric-gzip.zarr      67108864 gzip(level=1)    434.6016
#> 33         numeric-gzip.zarr      67108864 gzip(level=1)    434.4453
#> 34         numeric-gzip.zarr      67108864 gzip(level=1)    434.6016
#> 35         numeric-gzip.zarr      67108864 gzip(level=1)    434.4492
#> 36         numeric-gzip.zarr      67108864 gzip(level=1)    443.5938
#> 37         numeric-gzip.zarr      67108864 gzip(level=1)    443.7461
#> 38         numeric-gzip.zarr      67108864 gzip(level=1)    443.5859
#> 39         numeric-gzip.zarr      67108864 gzip(level=1)    443.5898
#> 40         numeric-gzip.zarr      67108864 gzip(level=1)    443.5938
#> 41 numeric-uncompressed.zarr      67108864         bytes    398.0508
#> 42 numeric-uncompressed.zarr      67108864         bytes    397.8984
#> 43 numeric-uncompressed.zarr      67108864         bytes    398.0547
#> 44 numeric-uncompressed.zarr      67108864         bytes    398.2031
#> 45 numeric-uncompressed.zarr      67108864         bytes    398.0547
#> 46 numeric-uncompressed.zarr      67108864         bytes    442.6797
#> 47 numeric-uncompressed.zarr      67108864         bytes    442.8359
#> 48 numeric-uncompressed.zarr      67108864         bytes    442.8359
#> 49 numeric-uncompressed.zarr      67108864         bytes    442.6797
#> 50 numeric-uncompressed.zarr      67108864         bytes    442.6797
#> 51         numeric-gzip.zarr      67108864 gzip(level=1)    107.3984
#> 52         numeric-gzip.zarr      67108864 gzip(level=1)    107.4102
#> 53         numeric-gzip.zarr      67108864 gzip(level=1)    107.2383
#> 54         numeric-gzip.zarr      67108864 gzip(level=1)    107.2617
#> 55         numeric-gzip.zarr      67108864 gzip(level=1)    107.2578
#> 56 numeric-uncompressed.zarr      67108864         bytes    106.5820
#> 57 numeric-uncompressed.zarr      67108864         bytes    106.5312
#> 58 numeric-uncompressed.zarr      67108864         bytes    106.3789
#> 59 numeric-uncompressed.zarr      67108864         bytes    106.5781
#> 60 numeric-uncompressed.zarr      67108864         bytes    106.5273
#>    cpu_percent throughput_mib_s
#> 1           89        202.50605
#> 2           90        194.50395
#> 3           89        202.03086
#> 4           89        203.27783
#> 5           89        202.85461
#> 6           92         74.57144
#> 7           93         72.12894
#> 8           93         75.14665
#> 9           93         73.31086
#> 10          93         74.94158
#> 11          85        345.02944
#> 12          85        353.73199
#> 13          84        358.58651
#> 14          85        354.85614
#> 15          85        348.08168
#> 16          90         87.00836
#> 17          91         86.92617
#> 18          91         83.57986
#> 19          90         85.67307
#> 20          91         86.33594
#> 21          72        240.63526
#> 22          73        244.58126
#> 23          74        245.72812
#> 24          74        244.85325
#> 25          74        244.25735
#> 26          52        336.49854
#> 27          52        336.41960
#> 28          49        347.11145
#> 29          51        355.91000
#> 30          53        337.51318
#> 31          99        220.59888
#> 32          99        220.20177
#> 33          99        220.48585
#> 34          99        220.08024
#> 35          99        221.05219
#> 36          99         74.69220
#> 37          99         76.98237
#> 38          99         76.52213
#> 39          99         76.69387
#> 40          99         76.36506
#> 41          98        441.28383
#> 42          99        439.24832
#> 43          98        447.05843
#> 44          99        433.92656
#> 45          99        437.28442
#> 46          99         88.86542
#> 47          99         89.30998
#> 48          99         88.24462
#> 49          99         90.39385
#> 50          99         90.45539
#> 51          99        391.08866
#> 52          99        393.08696
#> 53          99        392.63996
#> 54          99        388.36759
#> 55          99        390.12825
#> 56          99       2572.74795
#> 57          99       2623.07862
#> 58         100       2532.99691
#> 59         100       2551.38400
#> 60          99       2639.60541
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
source_revision: 713b65f3bbadb0fbab249f841d0130e5b3f36a9a
source_status:
date_utc: 2026-08-17T20:37:33+00:00
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
current_build_environment_not_retrospective_flags:
CARGO_ENCODED_RUSTFLAGS=<unset>
CARGO_PROFILE_RELEASE_LTO=<unset>
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=<unset>
RUSTFLAGS=<unset>
CC=<unset>
CXX=<unset>
CFLAGS=<unset>
CXXFLAGS=<unset>
CPPFLAGS=<unset>
LDFLAGS=<unset>
MAKEFLAGS=<unset>
current_R_build_configuration:
CC=x86_64-linux-gnu-gcc -std=gnu2x
CFLAGS=-g -O2 -fno-omit-frame-pointer -mno-omit-leaf-frame-pointer -ffile-prefix-map=/build/r-base-cbKgDj/r-base-4.6.0=. -fstack-protector-strong -fstack-clash-protection -Wformat -Werror=format-security -fcf-protection -fdebug-prefix-map=/build/r-base-cbKgDj/r-base-4.6.0=/usr/src/r-base-4.6.0-2.2404.0 -Wdate-time -D_FORTIFY_SOURCE=3
CPPFLAGS=
CXX=x86_64-linux-gnu-g++ -std=gnu++20
CXXFLAGS=-g -O2 -fno-omit-frame-pointer -mno-omit-leaf-frame-pointer -ffile-prefix-map=/build/r-base-cbKgDj/r-base-4.6.0=. -fstack-protector-strong -fstack-clash-protection -Wformat -Werror=format-security -fcf-protection -fdebug-prefix-map=/build/r-base-cbKgDj/r-base-4.6.0=/usr/src/r-base-4.6.0-2.2404.0 -Wdate-time -D_FORTIFY_SOURCE=3
CXX11='config' variable 'CXX11' is defunct
CXX11FLAGS='config' variable 'CXX11FLAGS' is defunct
LDFLAGS=-Wl,-Bsymbolic-functions -flto=auto -ffat-lto-objects -Wl,-z,relro
Rust_toolchain:
rustc 1.96.1 (31fca3adb 2026-06-26)
binary: rustc
commit-hash: 31fca3adb283cc9dfd56b49cdee9a96eb9c96ffd
commit-date: 2026-06-26
host: x86_64-unknown-linux-gnu
release: 1.96.1
LLVM version: 22.1.2
cargo 1.96.1 (356927216 2026-06-26)
Rzarrs_Cargo_release_profile:
46:[profile.release]
47-panic = "abort"
48-lto = true
49-codegen-units = 1
installed_reader_artifacts:
package=Rzarrs
version=0.1.0
artifact=/usr/local/lib/R/site-library/Rzarrs/libs/Rzarrs.so
2c617e61ba3088d0a2878239ce8e879c7b088cd58edf72d8f4791eaef26e11c3  /usr/local/lib/R/site-library/Rzarrs/libs/Rzarrs.so

String dump of section '.comment':
  [     0]  GCC: (Ubuntu 13.3.0-6ubuntu2~24.04.1) 13.3.0
  [    2d]  rustc version 1.91.1 (ed61e7d7e 2025-11-07)


package=Rarr
version=2.1.35
artifact=/usr/local/lib/R/site-library/Rarr/libs/Rarr.so
3ecb66836520282d9f33e0538f21865d4bcb9d18764778fcdfe6fc6d3858dfee  /usr/local/lib/R/site-library/Rarr/libs/Rarr.so

String dump of section '.comment':
  [     0]  GCC: (Ubuntu 13.3.0-6ubuntu2~24.04.1) 13.3.0


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
CPU(s) scaling MHz:                   31%
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
node 0 free: 56355 MB
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
source_revision: 713b65f3bbadb0fbab249f841d0130e5b3f36a9a
source_status:
date_utc: 2026-08-17T20:38:20+00:00
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
current_build_environment_not_retrospective_flags:
CARGO_ENCODED_RUSTFLAGS=<unset>
CARGO_PROFILE_RELEASE_LTO=<unset>
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=<unset>
RUSTFLAGS=<unset>
CC=<unset>
CXX=<unset>
CFLAGS=<unset>
CXXFLAGS=<unset>
CPPFLAGS=<unset>
LDFLAGS=<unset>
MAKEFLAGS=<unset>
Rust_toolchain:
rustc 1.96.1 (31fca3adb 2026-06-26)
binary: rustc
commit-hash: 31fca3adb283cc9dfd56b49cdee9a96eb9c96ffd
commit-date: 2026-06-26
host: x86_64-unknown-linux-gnu
release: 1.96.1
LLVM version: 22.1.2
cargo 1.96.1 (356927216 2026-06-26)
Python_and_Zarrista_artifacts:
python_executable=/tmp/zarrista-venv/bin/python
zarrista_distribution=0.1.0
artifact=/tmp/zarrista-venv/lib/python3.13/site-packages/zarrista/_zarrista.cpython-313-x86_64-linux-gnu.so
sha256=4fe67fd37abe3fe8859aece5ef42b8bccd34c142aa5a81f2dc4a4efbfcfec5e0

String dump of section '.comment':
  [     0]  Linker: LLD 22.1.2 (/checkout/src/llvm-project/llvm 1cb4e3833c1919c2e6fb579a23ac0e2b22587b7e)
  [    5f]  GCC: (Ubuntu 13.3.0-6ubuntu2~24.04.1) 13.3.0
  [    8c]  rustc version 1.96.1 (31fca3adb 2026-06-26)


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
CPU(s) scaling MHz:                   34%
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
node 0 free: 58412 MB
node distances:
node   0
  0:  10
```

### warm environment

``` text
command: tools/run_rzarrs_rarr_bench.sh /tmp/rzarrs-rarr-results/warm/environment.txt --fixtures /tmp/rzarrs-rarr-fixtures --out /tmp/rzarrs-rarr-results --cpuset 0 --numa-node 0 --mode warm --reps 5 --iterations 5
source_revision: 713b65f3bbadb0fbab249f841d0130e5b3f36a9a
source_status:
date_utc: 2026-08-17T20:36:09+00:00
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
current_build_environment_not_retrospective_flags:
CARGO_ENCODED_RUSTFLAGS=<unset>
CARGO_PROFILE_RELEASE_LTO=<unset>
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=<unset>
RUSTFLAGS=<unset>
CC=<unset>
CXX=<unset>
CFLAGS=<unset>
CXXFLAGS=<unset>
CPPFLAGS=<unset>
LDFLAGS=<unset>
MAKEFLAGS=<unset>
current_R_build_configuration:
CC=x86_64-linux-gnu-gcc -std=gnu2x
CFLAGS=-g -O2 -fno-omit-frame-pointer -mno-omit-leaf-frame-pointer -ffile-prefix-map=/build/r-base-cbKgDj/r-base-4.6.0=. -fstack-protector-strong -fstack-clash-protection -Wformat -Werror=format-security -fcf-protection -fdebug-prefix-map=/build/r-base-cbKgDj/r-base-4.6.0=/usr/src/r-base-4.6.0-2.2404.0 -Wdate-time -D_FORTIFY_SOURCE=3
CPPFLAGS=
CXX=x86_64-linux-gnu-g++ -std=gnu++20
CXXFLAGS=-g -O2 -fno-omit-frame-pointer -mno-omit-leaf-frame-pointer -ffile-prefix-map=/build/r-base-cbKgDj/r-base-4.6.0=. -fstack-protector-strong -fstack-clash-protection -Wformat -Werror=format-security -fcf-protection -fdebug-prefix-map=/build/r-base-cbKgDj/r-base-4.6.0=/usr/src/r-base-4.6.0-2.2404.0 -Wdate-time -D_FORTIFY_SOURCE=3
CXX11='config' variable 'CXX11' is defunct
CXX11FLAGS='config' variable 'CXX11FLAGS' is defunct
LDFLAGS=-Wl,-Bsymbolic-functions -flto=auto -ffat-lto-objects -Wl,-z,relro
Rust_toolchain:
rustc 1.96.1 (31fca3adb 2026-06-26)
binary: rustc
commit-hash: 31fca3adb283cc9dfd56b49cdee9a96eb9c96ffd
commit-date: 2026-06-26
host: x86_64-unknown-linux-gnu
release: 1.96.1
LLVM version: 22.1.2
cargo 1.96.1 (356927216 2026-06-26)
Rzarrs_Cargo_release_profile:
46:[profile.release]
47-panic = "abort"
48-lto = true
49-codegen-units = 1
installed_reader_artifacts:
package=Rzarrs
version=0.1.0
artifact=/usr/local/lib/R/site-library/Rzarrs/libs/Rzarrs.so
2c617e61ba3088d0a2878239ce8e879c7b088cd58edf72d8f4791eaef26e11c3  /usr/local/lib/R/site-library/Rzarrs/libs/Rzarrs.so

String dump of section '.comment':
  [     0]  GCC: (Ubuntu 13.3.0-6ubuntu2~24.04.1) 13.3.0
  [    2d]  rustc version 1.91.1 (ed61e7d7e 2025-11-07)


package=Rarr
version=2.1.35
artifact=/usr/local/lib/R/site-library/Rarr/libs/Rarr.so
3ecb66836520282d9f33e0538f21865d4bcb9d18764778fcdfe6fc6d3858dfee  /usr/local/lib/R/site-library/Rarr/libs/Rarr.so

String dump of section '.comment':
  [     0]  GCC: (Ubuntu 13.3.0-6ubuntu2~24.04.1) 13.3.0


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
node 0 free: 56683 MB
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
source_revision: 713b65f3bbadb0fbab249f841d0130e5b3f36a9a
source_status:
date_utc: 2026-08-17T20:38:13+00:00
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
current_build_environment_not_retrospective_flags:
CARGO_ENCODED_RUSTFLAGS=<unset>
CARGO_PROFILE_RELEASE_LTO=<unset>
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=<unset>
RUSTFLAGS=<unset>
CC=<unset>
CXX=<unset>
CFLAGS=<unset>
CXXFLAGS=<unset>
CPPFLAGS=<unset>
LDFLAGS=<unset>
MAKEFLAGS=<unset>
Rust_toolchain:
rustc 1.96.1 (31fca3adb 2026-06-26)
binary: rustc
commit-hash: 31fca3adb283cc9dfd56b49cdee9a96eb9c96ffd
commit-date: 2026-06-26
host: x86_64-unknown-linux-gnu
release: 1.96.1
LLVM version: 22.1.2
cargo 1.96.1 (356927216 2026-06-26)
Python_and_Zarrista_artifacts:
python_executable=/tmp/zarrista-venv/bin/python
zarrista_distribution=0.1.0
artifact=/tmp/zarrista-venv/lib/python3.13/site-packages/zarrista/_zarrista.cpython-313-x86_64-linux-gnu.so
sha256=4fe67fd37abe3fe8859aece5ef42b8bccd34c142aa5a81f2dc4a4efbfcfec5e0

String dump of section '.comment':
  [     0]  Linker: LLD 22.1.2 (/checkout/src/llvm-project/llvm 1cb4e3833c1919c2e6fb579a23ac0e2b22587b7e)
  [    5f]  GCC: (Ubuntu 13.3.0-6ubuntu2~24.04.1) 13.3.0
  [    8c]  rustc version 1.96.1 (31fca3adb 2026-06-26)


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
node 0 free: 58240 MB
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
| cold | numeric-uncompressed.zarr | bytes         | Rarr           |  0.18093 |          353.730 |      369.32 |          85 |       321.570 |
| warm | numeric-uncompressed.zarr | bytes         | Rarr           |  0.14570 |          439.250 |      398.05 |          99 |       320.650 |
| cold | numeric-gzip.zarr         | gzip(level=1) | Rarr           |  0.31604 |          202.510 |      394.48 |          89 |       407.730 |
| warm | numeric-gzip.zarr         | gzip(level=1) | Rarr           |  0.29027 |          220.490 |      434.60 |          99 |       406.800 |
| cold | numeric-uncompressed.zarr | bytes         | Rzarrs         |  0.74129 |           86.336 |      313.93 |          91 |        64.087 |
| warm | numeric-uncompressed.zarr | bytes         | Rzarrs         |  0.71661 |           89.310 |      442.68 |          99 |        64.034 |
| cold | numeric-gzip.zarr         | gzip(level=1) | Rzarrs         |  0.85824 |           74.571 |      314.99 |          93 |        64.087 |
| warm | numeric-gzip.zarr         | gzip(level=1) | Rzarrs         |  0.83636 |           76.522 |      443.59 |          99 |        64.034 |

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
| cold | numeric-uncompressed.zarr | bytes         |     0.1809279 |       0.7412903 |                  0.24407 |
| warm | numeric-uncompressed.zarr | bytes         |     0.1457035 |       0.7166052 |                  0.20332 |
| cold | numeric-gzip.zarr         | gzip(level=1) |     0.3160399 |       0.8582374 |                  0.36824 |
| warm | numeric-gzip.zarr         | gzip(level=1) |     0.2902681 |       0.8363594 |                  0.34706 |

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
| cold | numeric-uncompressed.zarr | bytes         | 0.189620 |           337.51 |      105.89 |          52 |
| warm | numeric-uncompressed.zarr | bytes         | 0.024876 |          2572.70 |      106.53 |          99 |
| cold | numeric-gzip.zarr         | gzip(level=1) | 0.261670 |           244.58 |      106.69 |          74 |
| warm | numeric-gzip.zarr         | gzip(level=1) | 0.163650 |           391.09 |      107.26 |          99 |

## Visual comparisons

These plots include all three readers. Zarrista remains a Python/NumPy
context point; visual proximity is not an R-binding claim. The timed
operation is the loaded-runtime, open-and-materialize request recorded
in `measurement_scope`.

``` r
if (nrow(r_runs) && nrow(zarrista_runs)) {
  plot_runs <- rbind(
    r_runs[c("implementation", "mode", "codec", "median_s", "throughput_mib_s", "max_rss_mib")],
    zarrista_runs[c("implementation", "mode", "codec", "median_s", "throughput_mib_s", "max_rss_mib")]
  )
  plot_runs$workload <- paste(plot_runs$mode, plot_runs$codec, sep = " / ")
  summary_runs <- stats::aggregate(
    plot_runs[c("median_s", "throughput_mib_s", "max_rss_mib")],
    by = list(implementation = plot_runs$implementation, workload = plot_runs$workload),
    FUN = stats::median,
    na.rm = TRUE
  )
  implementations <- c("Rarr", "Rzarrs", "Zarrista")
  workloads <- c("cold / bytes", "warm / bytes", "cold / gzip(level=1)", "warm / gzip(level=1)")
  colours <- c(Rarr = "#0072B2", Rzarrs = "#D55E00", Zarrista = "#009E73")

  draw_metric <- function(metric, label) {
    old <- par(mfrow = c(2, 2), mar = c(4, 8, 3, 2))
    on.exit(par(old), add = TRUE)
    for (workload in workloads) {
      values <- summary_runs[summary_runs$workload == workload, c("implementation", metric)]
      values <- values[match(implementations, values$implementation), ]
      values <- values[!is.na(values[[metric]]), ]
      x <- values[[metric]]
      pad <- c(min(x) / 1.8, max(x) * 1.8)
      plot(x, seq_along(x), log = "x", xlim = pad, yaxt = "n", pch = 19,
           col = colours[values$implementation], xlab = label, ylab = "", main = workload)
      axis(2, at = seq_along(x), labels = values$implementation, las = 1)
      text(x, seq_along(x), labels = format(signif(x, 4), trim = TRUE), pos = 4, cex = 0.8)
      grid(col = "grey90")
    }
  }

  draw_metric("median_s", "Median seconds (lower is better; log scale)")
  draw_metric("throughput_mib_s", "MiB/s (higher is better; log scale)")
  draw_metric("max_rss_mib", "Maximum RSS MiB (process diagnostic; log scale)")
} else {
  cat("Rarr, Rzarrs, and Zarrista artifacts are all required for plots.\n")
}
```

![](benchmark_rzarrs_rarr_files/figure-gfm/reader-plots-1.png)<!-- -->![](benchmark_rzarrs_rarr_files/figure-gfm/reader-plots-2.png)<!-- -->![](benchmark_rzarrs_rarr_files/figure-gfm/reader-plots-3.png)<!-- -->

## Network-bounded S3 matrix

The local-store table above is not a network result. The remote version
is a separate S3-compatible workload: stage the immutable fixture once
into a local MinIO bucket, run each client in a dedicated network
namespace, and apply the same `tc netem` rate and latency in both
directions on its veth pair. Record MinIO revision, bucket/prefix,
object count/bytes, endpoint, latency, rate, page-cache mode, and client
thread limits. Do not use an unbounded public endpoint or fold local and
remote results together.

It is gated on a successful equivalent full-array remote probe for all
three clients: Rarr through its explicit `s3_client`, Rzarrs through
`ZarrObjectStore`, and Zarrista through an Obstore S3 store. Until that
probe is executable and equality-checked, the report must say that an
all-three network comparison is unavailable rather than presenting a
two-reader result as equivalent.

## Compilation provenance and controlled rebuild gate

The environment blocks record toolchains, binary hashes, and
build-affecting environment variables. They identify what ran, but
compiler identity alone does **not** prove flag parity—especially for an
already-installed Rarr binary. Do not attribute the observed gap to
library design until the controlled rebuild campaign below reproduces
it.

1.  Build Rarr and Rzarrs from source in a clean R environment with no
    user or site Makevars overrides; save every compile and link
    command.
2.  Pin and record the Rust toolchain, Cargo release profile, C/C++
    compiler, C/C++ flags, linker flags, enabled features, and binary
    SHA-256 hashes.
3.  Re-run this exact fixture, CPU/NUMA/thread/cache matrix on those
    artifacts.
4.  Report both the installed-artifact and controlled-rebuild tables;
    only a stable gap across them rules out unknown compilation flags as
    the cause.

## Interpretation limits

This report attributes no cost by itself. The difference between
bytes-only and gzip fixtures estimates the combined codec contribution
for this workload, not compression time in isolation. Profiling comes
next only if the measured result makes an extension worthwhile; it must
distinguish I/O, decompression, data layout conversion, and R object
materialization.
