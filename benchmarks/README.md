# Rzarrs versus Rarr read benchmark

The canonical report source is [`benchmark_rzarrs_rarr.Rmd`](benchmark_rzarrs_rarr.Rmd).
It is the decision baseline before adding deferred materialization, ALTREP, or
an asynchronous native API to Rzarrs. It measures only a common, validated
surface: local Zarr V3 `int32` arrays materialized completely into R.

The fixture generator writes two equivalent stores:

- `numeric-uncompressed.zarr`: bytes codec only, separating metadata, chunk
  traversal, decoding, and R materialization from decompression;
- `numeric-gzip.zarr`: bytes followed by gzip level 1, adding codec work.

The benchmark refuses to time a fixture until `Rzarrs` and `Rarr` return
identical dimensions and values. It deliberately does **not** treat Zarr-VCF,
ZIP stores, strings, partial reads, transpose, or remote object stores as part
of this first comparison. Add each only after creating an interoperable fixture
and an equivalent-result check.

## Prepare fixtures

Fixtures are generated outside the source tree. The defaults contain 64 MiB of
logical `int32` data in 512 x 512 chunks:

```sh
cd /root/Rzarrs
Rscript tools/make_benchmark_fixtures.R \
  --out "$HOME/.cache/Rzarrs/benchmarks/fixtures"
```

The fixture directory contains `benchmark-fixture.dcf`, recording the input
shape, chunk shape, codec, logical element count, and logical byte count.

## Run reproducibly

This initial suite is deliberately single-threaded: provide one physical CPU
id on one NUMA node. Neither package exposes a matched, documented read-thread
knob, so allowing a multi-core taskset would not establish equal thread
budgets. The runner additionally exports `BLOSC_NTHREADS=1`, `RAYON_NUM_THREADS=1`,
`TOKIO_WORKER_THREADS=1`, and one-thread OpenMP/BLAS variables; the complete
set is recorded in each mode's `environment.txt`. Add scaling measurements
only after setting equivalent processing, decompression, and decoding budgets
explicitly. Do not use an SMT sibling or a mixed P/E core. A warm run pre-reads
the relevant fixture in each fresh R process. A cold run drops the Linux page
cache before **every** process-level replicate and consequently requires root.

```sh
cd /root/Rzarrs
CPUSET=2
NUMA_NODE=0
FIXTURES="$HOME/.cache/Rzarrs/benchmarks/fixtures"
RESULTS="$HOME/.cache/Rzarrs/benchmarks/results"

tools/run_rzarrs_rarr_bench.sh \
  --fixtures "$FIXTURES" --out "$RESULTS" \
  --cpuset "$CPUSET" --numa-node "$NUMA_NODE" \
  --mode warm --reps 5 --iterations 5

sudo tools/run_rzarrs_rarr_bench.sh \
  --fixtures "$FIXTURES" --out "$RESULTS" \
  --cpuset "$CPUSET" --numa-node "$NUMA_NODE" \
  --mode cold --reps 5
```

The runner alternates implementation order by replicate, invokes a new pinned
R process for each measurement, and records:

- `bench.rds` and `summary.csv`: `bench` time and allocation statistics;
- `time-v.txt`: GNU `time -v`, including maximum RSS and CPU percentage;
- `command.txt`, `session-info.txt`, `fixture.dcf`, stdout, and stderr for each
  replicate;
- `warm/environment.txt` and `cold/environment.txt`: source revision/status,
  exact host CPU/NUMA topology, package versions, and run configuration.

Render the report from a result directory with:

```sh
RZARRS_BENCH_RESULTS="$RESULTS" \
  Rscript -e 'rmarkdown::render("benchmarks/benchmark_rzarrs_rarr.Rmd")'
```

Do not compare results across revisions, fixture sizes/codecs, cache modes,
thread placements, or hardware as though they were the same workload.

## Zarrista context baseline

[Zarrista](https://github.com/developmentseed/zarrista) reads the same local
Zarr V3 bytes/gzip fixtures, but materializes a Python/NumPy array rather than
an R array. It is therefore a native-library context point, reported separately
from the Rzarrs-versus-Rarr table; it is not evidence about an R binding.

Build a pinned upstream revision in an isolated environment, then supply both
its interpreter and revision to the runner:

```sh
git clone https://github.com/developmentseed/zarrista.git "$HOME/src/zarrista"
cd "$HOME/src/zarrista"
ZARRISTA_REVISION="$(git rev-parse HEAD)"
git checkout "$ZARRISTA_REVISION"
python3 -m venv "$HOME/.cache/Rzarrs/zarrista-venv"
"$HOME/.cache/Rzarrs/zarrista-venv/bin/pip" install . numpy

cd /root/Rzarrs
tools/run_zarrista_bench.sh \
  --fixtures "$FIXTURES" --out "$RESULTS" \
  --python "$HOME/.cache/Rzarrs/zarrista-venv/bin/python" \
  --zarrista-revision "$ZARRISTA_REVISION" \
  --cpuset "$CPUSET" --numa-node "$NUMA_NODE" \
  --mode warm --reps 5 --iterations 5
```

The Zarrista runner uses the same one-CPU/NUMA placement and native thread
controls as the R runner, including `RAYON_NUM_THREADS=1`. It verifies output
shape, dtype, and boundary values before timing each fixture.
