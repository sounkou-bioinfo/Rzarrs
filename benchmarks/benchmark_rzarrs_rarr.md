Rzarrs versus Rarr: numeric Zarr read benchmark
================

- [Scope](#scope)
- [Method](#method)
- [Recorded environment](#recorded-environment)
  - [cold environment](#cold-environment)
  - [warm environment](#warm-environment)
- [Results](#results)
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
`environment.txt`. Warm and cold cache data are distinct workloads and
must never be pooled.

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
#>    implementation mode                                               store
#> 1            Rarr cold         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr
#> 2            Rarr cold         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr
#> 3            Rarr cold         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr
#> 4            Rarr cold         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr
#> 5            Rarr cold         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr
#> 6          Rzarrs cold         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr
#> 7          Rzarrs cold         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr
#> 8          Rzarrs cold         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr
#> 9          Rzarrs cold         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr
#> 10         Rzarrs cold         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr
#> 11           Rarr cold /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr
#> 12           Rarr cold /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr
#> 13           Rarr cold /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr
#> 14           Rarr cold /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr
#> 15           Rarr cold /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr
#> 16         Rzarrs cold /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr
#> 17         Rzarrs cold /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr
#> 18         Rzarrs cold /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr
#> 19         Rzarrs cold /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr
#> 20         Rzarrs cold /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr
#> 21           Rarr warm         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr
#> 22           Rarr warm         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr
#> 23           Rarr warm         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr
#> 24           Rarr warm         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr
#> 25           Rarr warm         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr
#> 26         Rzarrs warm         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr
#> 27         Rzarrs warm         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr
#> 28         Rzarrs warm         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr
#> 29         Rzarrs warm         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr
#> 30         Rzarrs warm         /tmp/rzarrs-rarr-fixtures/numeric-gzip.zarr
#> 31           Rarr warm /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr
#> 32           Rarr warm /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr
#> 33           Rarr warm /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr
#> 34           Rarr warm /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr
#> 35           Rarr warm /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr
#> 36         Rzarrs warm /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr
#> 37         Rzarrs warm /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr
#> 38         Rzarrs warm /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr
#> 39         Rzarrs warm /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr
#> 40         Rzarrs warm /tmp/rzarrs-rarr-fixtures/numeric-uncompressed.zarr
#>    iterations_requested iterations_completed     min_s  median_s   total_s
#> 1                     1                    1 0.3192407 0.3192407 0.3192407
#> 2                     1                    1 0.3173736 0.3173736 0.3173736
#> 3                     1                    1 0.3184196 0.3184196 0.3184196
#> 4                     1                    1 0.3172777 0.3172777 0.3172777
#> 5                     1                    1 0.3193296 0.3193296 0.3193296
#> 6                     1                    1 0.8667452 0.8667452 0.8667452
#> 7                     1                    1 0.8727747 0.8727747 0.8727747
#> 8                     1                    1 0.8610703 0.8610703 0.8610703
#> 9                     1                    1 0.8624066 0.8624066 0.8624066
#> 10                    1                    1 0.8654584 0.8654584 0.8654584
#> 11                    1                    1 0.1799326 0.1799326 0.1799326
#> 12                    1                    1 0.1833896 0.1833896 0.1833896
#> 13                    1                    1 0.1783209 0.1783209 0.1783209
#> 14                    1                    1 0.1782371 0.1782371 0.1782371
#> 15                    1                    1 0.1813969 0.1813969 0.1813969
#> 16                    1                    1 0.7531162 0.7531162 0.7531162
#> 17                    1                    1 0.7481557 0.7481557 0.7481557
#> 18                    1                    1 0.7526812 0.7526812 0.7526812
#> 19                    1                    1 0.7362270 0.7362270 0.7362270
#> 20                    1                    1 0.7320079 0.7320079 0.7320079
#> 21                    5                    5 0.2660683 0.2912890 1.4614663
#> 22                    5                    5 0.2640621 0.2893909 1.4549808
#> 23                    5                    5 0.2663939 0.2904742 1.4724525
#> 24                    5                    5 0.2686726 0.2884090 1.4698442
#> 25                    5                    5 0.2634361 0.2908737 1.4597966
#> 26                    5                    5 0.8229908 0.8369693 4.2108798
#> 27                    5                    5 0.8267436 0.8352021 4.1867683
#> 28                    5                    5 0.8241406 0.8442754 4.2505877
#> 29                    5                    5 0.8211692 0.8420831 4.2053229
#> 30                    5                    5 0.8254822 0.8409264 4.2211330
#> 31                    5                    5 0.1402980 0.1432068 0.7283667
#> 32                    5                    5 0.1405274 0.1433455 0.7249497
#> 33                    5                    5 0.1406663 0.1431630 0.7235535
#> 34                    5                    5 0.1377733 0.1430450 0.7235372
#> 35                    5                    5 0.1417885 0.1433125 0.7294573
#> 36                    5                    5 0.6995511 0.7214789 3.6197378
#> 37                    5                    5 0.6987775 0.7128397 3.5921255
#> 38                    5                    5 0.7057267 0.7132635 3.6140307
#> 39                    5                    5 0.7053884 0.7127324 3.5958192
#> 40                    5                    5 0.7038834 0.7099765 3.6048302
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
#> 21       426562040       28     4.6.0          0.1.0       2.1.35         1.1.4
#> 22       426562040       28     4.6.0          0.1.0       2.1.35         1.1.4
#> 23       426562040       28     4.6.0          0.1.0       2.1.35         1.1.4
#> 24       426562040       28     4.6.0          0.1.0       2.1.35         1.1.4
#> 25       426562040       28     4.6.0          0.1.0       2.1.35         1.1.4
#> 26        67144792        4     4.6.0          0.1.0       2.1.35         1.1.4
#> 27        67144792        4     4.6.0          0.1.0       2.1.35         1.1.4
#> 28        67144792        4     4.6.0          0.1.0       2.1.35         1.1.4
#> 29        67144792        4     4.6.0          0.1.0       2.1.35         1.1.4
#> 30        67144792        4     4.6.0          0.1.0       2.1.35         1.1.4
#> 31       336228936       19     4.6.0          0.1.0       2.1.35         1.1.4
#> 32       336228936       19     4.6.0          0.1.0       2.1.35         1.1.4
#> 33       336228936       19     4.6.0          0.1.0       2.1.35         1.1.4
#> 34       336228936       19     4.6.0          0.1.0       2.1.35         1.1.4
#> 35       336228936       19     4.6.0          0.1.0       2.1.35         1.1.4
#> 36        67144792        4     4.6.0          0.1.0       2.1.35         1.1.4
#> 37        67144792        4     4.6.0          0.1.0       2.1.35         1.1.4
#> 38        67144792        4     4.6.0          0.1.0       2.1.35         1.1.4
#> 39        67144792        4     4.6.0          0.1.0       2.1.35         1.1.4
#> 40        67144792        4     4.6.0          0.1.0       2.1.35         1.1.4
#>                                                                 run_dir
#> 1            /tmp/rzarrs-rarr-results/cold/numeric-gzip.zarr/Rarr/rep-1
#> 2            /tmp/rzarrs-rarr-results/cold/numeric-gzip.zarr/Rarr/rep-2
#> 3            /tmp/rzarrs-rarr-results/cold/numeric-gzip.zarr/Rarr/rep-3
#> 4            /tmp/rzarrs-rarr-results/cold/numeric-gzip.zarr/Rarr/rep-4
#> 5            /tmp/rzarrs-rarr-results/cold/numeric-gzip.zarr/Rarr/rep-5
#> 6          /tmp/rzarrs-rarr-results/cold/numeric-gzip.zarr/Rzarrs/rep-1
#> 7          /tmp/rzarrs-rarr-results/cold/numeric-gzip.zarr/Rzarrs/rep-2
#> 8          /tmp/rzarrs-rarr-results/cold/numeric-gzip.zarr/Rzarrs/rep-3
#> 9          /tmp/rzarrs-rarr-results/cold/numeric-gzip.zarr/Rzarrs/rep-4
#> 10         /tmp/rzarrs-rarr-results/cold/numeric-gzip.zarr/Rzarrs/rep-5
#> 11   /tmp/rzarrs-rarr-results/cold/numeric-uncompressed.zarr/Rarr/rep-1
#> 12   /tmp/rzarrs-rarr-results/cold/numeric-uncompressed.zarr/Rarr/rep-2
#> 13   /tmp/rzarrs-rarr-results/cold/numeric-uncompressed.zarr/Rarr/rep-3
#> 14   /tmp/rzarrs-rarr-results/cold/numeric-uncompressed.zarr/Rarr/rep-4
#> 15   /tmp/rzarrs-rarr-results/cold/numeric-uncompressed.zarr/Rarr/rep-5
#> 16 /tmp/rzarrs-rarr-results/cold/numeric-uncompressed.zarr/Rzarrs/rep-1
#> 17 /tmp/rzarrs-rarr-results/cold/numeric-uncompressed.zarr/Rzarrs/rep-2
#> 18 /tmp/rzarrs-rarr-results/cold/numeric-uncompressed.zarr/Rzarrs/rep-3
#> 19 /tmp/rzarrs-rarr-results/cold/numeric-uncompressed.zarr/Rzarrs/rep-4
#> 20 /tmp/rzarrs-rarr-results/cold/numeric-uncompressed.zarr/Rzarrs/rep-5
#> 21           /tmp/rzarrs-rarr-results/warm/numeric-gzip.zarr/Rarr/rep-1
#> 22           /tmp/rzarrs-rarr-results/warm/numeric-gzip.zarr/Rarr/rep-2
#> 23           /tmp/rzarrs-rarr-results/warm/numeric-gzip.zarr/Rarr/rep-3
#> 24           /tmp/rzarrs-rarr-results/warm/numeric-gzip.zarr/Rarr/rep-4
#> 25           /tmp/rzarrs-rarr-results/warm/numeric-gzip.zarr/Rarr/rep-5
#> 26         /tmp/rzarrs-rarr-results/warm/numeric-gzip.zarr/Rzarrs/rep-1
#> 27         /tmp/rzarrs-rarr-results/warm/numeric-gzip.zarr/Rzarrs/rep-2
#> 28         /tmp/rzarrs-rarr-results/warm/numeric-gzip.zarr/Rzarrs/rep-3
#> 29         /tmp/rzarrs-rarr-results/warm/numeric-gzip.zarr/Rzarrs/rep-4
#> 30         /tmp/rzarrs-rarr-results/warm/numeric-gzip.zarr/Rzarrs/rep-5
#> 31   /tmp/rzarrs-rarr-results/warm/numeric-uncompressed.zarr/Rarr/rep-1
#> 32   /tmp/rzarrs-rarr-results/warm/numeric-uncompressed.zarr/Rarr/rep-2
#> 33   /tmp/rzarrs-rarr-results/warm/numeric-uncompressed.zarr/Rarr/rep-3
#> 34   /tmp/rzarrs-rarr-results/warm/numeric-uncompressed.zarr/Rarr/rep-4
#> 35   /tmp/rzarrs-rarr-results/warm/numeric-uncompressed.zarr/Rarr/rep-5
#> 36 /tmp/rzarrs-rarr-results/warm/numeric-uncompressed.zarr/Rzarrs/rep-1
#> 37 /tmp/rzarrs-rarr-results/warm/numeric-uncompressed.zarr/Rzarrs/rep-2
#> 38 /tmp/rzarrs-rarr-results/warm/numeric-uncompressed.zarr/Rzarrs/rep-3
#> 39 /tmp/rzarrs-rarr-results/warm/numeric-uncompressed.zarr/Rzarrs/rep-4
#> 40 /tmp/rzarrs-rarr-results/warm/numeric-uncompressed.zarr/Rzarrs/rep-5
#>                      fixture logical_bytes         codec max_rss_mib
#> 1          numeric-gzip.zarr      67108864 gzip(level=1)    394.3203
#> 2          numeric-gzip.zarr      67108864 gzip(level=1)    394.1641
#> 3          numeric-gzip.zarr      67108864 gzip(level=1)    394.3242
#> 4          numeric-gzip.zarr      67108864 gzip(level=1)    394.6367
#> 5          numeric-gzip.zarr      67108864 gzip(level=1)    394.3203
#> 6          numeric-gzip.zarr      67108864 gzip(level=1)    315.3008
#> 7          numeric-gzip.zarr      67108864 gzip(level=1)    314.9883
#> 8          numeric-gzip.zarr      67108864 gzip(level=1)    314.9883
#> 9          numeric-gzip.zarr      67108864 gzip(level=1)    315.1445
#> 10         numeric-gzip.zarr      67108864 gzip(level=1)    314.9883
#> 11 numeric-uncompressed.zarr      67108864         bytes    369.0078
#> 12 numeric-uncompressed.zarr      67108864         bytes    369.1641
#> 13 numeric-uncompressed.zarr      67108864         bytes    369.1641
#> 14 numeric-uncompressed.zarr      67108864         bytes    369.3203
#> 15 numeric-uncompressed.zarr      67108864         bytes    369.1641
#> 16 numeric-uncompressed.zarr      67108864         bytes    314.0859
#> 17 numeric-uncompressed.zarr      67108864         bytes    314.3945
#> 18 numeric-uncompressed.zarr      67108864         bytes    314.0898
#> 19 numeric-uncompressed.zarr      67108864         bytes    314.0898
#> 20 numeric-uncompressed.zarr      67108864         bytes    314.2422
#> 21         numeric-gzip.zarr      67108864 gzip(level=1)    434.3633
#> 22         numeric-gzip.zarr      67108864 gzip(level=1)    434.4414
#> 23         numeric-gzip.zarr      67108864 gzip(level=1)    434.2930
#> 24         numeric-gzip.zarr      67108864 gzip(level=1)    434.2852
#> 25         numeric-gzip.zarr      67108864 gzip(level=1)    434.4453
#> 26         numeric-gzip.zarr      67108864 gzip(level=1)    443.4336
#> 27         numeric-gzip.zarr      67108864 gzip(level=1)    443.4375
#> 28         numeric-gzip.zarr      67108864 gzip(level=1)    443.1211
#> 29         numeric-gzip.zarr      67108864 gzip(level=1)    443.4336
#> 30         numeric-gzip.zarr      67108864 gzip(level=1)    443.2773
#> 31 numeric-uncompressed.zarr      67108864         bytes    397.5820
#> 32 numeric-uncompressed.zarr      67108864         bytes    397.5781
#> 33 numeric-uncompressed.zarr      67108864         bytes    397.8984
#> 34 numeric-uncompressed.zarr      67108864         bytes    397.9023
#> 35 numeric-uncompressed.zarr      67108864         bytes    397.8945
#> 36 numeric-uncompressed.zarr      67108864         bytes    442.6797
#> 37 numeric-uncompressed.zarr      67108864         bytes    442.5234
#> 38 numeric-uncompressed.zarr      67108864         bytes    442.5234
#> 39 numeric-uncompressed.zarr      67108864         bytes    442.3633
#> 40 numeric-uncompressed.zarr      67108864         bytes    442.2148
#>    cpu_percent throughput_mib_s
#> 1           90        200.47569
#> 2           91        201.65508
#> 3           91        200.99268
#> 4           91        201.71603
#> 5           90        200.41988
#> 6           94         73.83946
#> 7           94         73.32935
#> 8           94         74.32610
#> 9           94         74.21093
#> 10          94         73.94925
#> 11          88        355.68874
#> 12          88        348.98380
#> 13          87        358.90351
#> 14          87        359.07220
#> 15          87        352.81741
#> 16          92         84.98025
#> 17          92         85.54369
#> 18          92         85.02936
#> 19          92         86.92971
#> 20          92         87.43075
#> 21          99        219.71306
#> 22          99        221.15417
#> 23          99        220.32936
#> 24          99        221.90711
#> 25          99        220.02680
#> 26          99         76.46636
#> 27          99         76.62816
#> 28          99         75.80465
#> 29          99         76.00200
#> 30          99         76.10654
#> 31          99        446.90614
#> 32          99        446.47365
#> 33          99        447.04299
#> 34          99        447.41155
#> 35          99        446.57651
#> 36          99         88.70668
#> 37          99         89.78175
#> 38          99         89.72841
#> 39          99         89.79527
#> 40          99         90.14383
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
source_revision: 022148b620a58840a92ce9659fe41ed2f0828b2c
source_status:
date_utc: 2026-08-17T18:15:31+00:00
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
CPU(s) scaling MHz:                   44%
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
node 0 free: 56283 MB
node distances:
node   0 
  0:  10 
r_packages:
Rzarrs=0.1.0
Rarr=2.1.35
bench=1.1.4
```

### warm environment

``` text
command: tools/run_rzarrs_rarr_bench.sh /tmp/rzarrs-rarr-results/warm/environment.txt --fixtures /tmp/rzarrs-rarr-fixtures --out /tmp/rzarrs-rarr-results --cpuset 0 --numa-node 0 --mode warm --reps 5 --iterations 5
source_revision: 022148b620a58840a92ce9659fe41ed2f0828b2c
source_status:
date_utc: 2026-08-17T18:14:07+00:00
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
CPU(s) scaling MHz:                   28%
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
node 0 free: 56611 MB
node distances:
node   0 
  0:  10 
r_packages:
Rzarrs=0.1.0
Rarr=2.1.35
bench=1.1.4
```

## Results

`bench` repeats inside a process. The table therefore reports the median
of each process’s `bench` median across process-level replicates.
`max_rss_mib` and `cpu_percent` are likewise process-level medians from
GNU `time -v`.

``` r
if (nrow(runs)) {
  reported <- stats::aggregate(
    runs[c("median_s", "mem_alloc_bytes", "throughput_mib_s", "max_rss_mib", "cpu_percent")],
    by = list(
      mode = runs$mode,
      fixture = runs$fixture,
      codec = runs$codec,
      implementation = runs$implementation
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
| cold | numeric-uncompressed.zarr | bytes         | Rarr           |  0.17993 |          355.690 |      369.16 |          87 |       321.570 |
| warm | numeric-uncompressed.zarr | bytes         | Rarr           |  0.14321 |          446.910 |      397.89 |          99 |       320.650 |
| cold | numeric-gzip.zarr         | gzip(level=1) | Rarr           |  0.31842 |          200.990 |      394.32 |          91 |       407.730 |
| warm | numeric-gzip.zarr         | gzip(level=1) | Rarr           |  0.29047 |          220.330 |      434.36 |          99 |       406.800 |
| cold | numeric-uncompressed.zarr | bytes         | Rzarrs         |  0.74816 |           85.544 |      314.09 |          92 |        64.087 |
| warm | numeric-uncompressed.zarr | bytes         | Rzarrs         |  0.71284 |           89.782 |      442.52 |          99 |        64.034 |
| cold | numeric-gzip.zarr         | gzip(level=1) | Rzarrs         |  0.86546 |           73.949 |      314.99 |          94 |        64.087 |
| warm | numeric-gzip.zarr         | gzip(level=1) | Rzarrs         |  0.84093 |           76.107 |      443.43 |          99 |        64.034 |

A speedup is only meaningful within one `mode`, fixture, codec, CPU
binding, and environment. For each matched pair below, a value above 1
means Rzarrs had the lower process-level `bench` median.

``` r
if (nrow(runs)) {
  per_run <- stats::aggregate(
    runs["median_s"],
    by = list(mode = runs$mode, fixture = runs$fixture, codec = runs$codec,
              implementation = runs$implementation),
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
| cold | numeric-uncompressed.zarr | bytes         |     0.1799326 |       0.7481557 |                  0.24050 |
| warm | numeric-uncompressed.zarr | bytes         |     0.1432068 |       0.7128397 |                  0.20090 |
| cold | numeric-gzip.zarr         | gzip(level=1) |     0.3184196 |       0.8654584 |                  0.36792 |
| warm | numeric-gzip.zarr         | gzip(level=1) |     0.2904742 |       0.8409264 |                  0.34542 |

## Interpretation limits

This report attributes no cost by itself. The difference between
bytes-only and gzip fixtures estimates the combined codec contribution
for this workload, not compression time in isolation. Profiling comes
next only if the measured result makes an extension worthwhile; it must
distinguish I/O, decompression, data layout conversion, and R object
materialization.
