#!/usr/bin/env bash
# Run Zarrista's Python/NumPy materialization as a contextual Zarr baseline.
set -euo pipefail
original_args=("$@")

usage() {
  cat <<'EOF'
Usage: tools/run_zarrista_bench.sh --fixtures DIR --out DIR --python PATH --zarrista-revision SHA --cpuset CPU --numa-node N [options]

Options:
  --fixtures DIR          directory created by make_benchmark_fixtures.R
  --out DIR               shared benchmark result root
  --python PATH           Python interpreter with zarrista and numpy installed
  --zarrista-revision SHA exact developmentseed/zarrista revision used to build it
  --cpuset CPU            one physical CPU id for this single-thread baseline
  --numa-node N           NUMA node to bind CPU and memory allocations to
  --mode MODE             warm or cold (default: warm)
  --reps N                process-level replicates (default: 5)
  --iterations N          Python timing iterations per warm process (default: 5; cold uses 1)

Zarrista materializes a NumPy array, whereas Rzarrs and Rarr materialize an R
array. Its results are a contextual native-library comparison point and are
reported separately from the R-to-R comparison.
EOF
}

die() {
  echo "error: $*" >&2
  exit 2
}

fixtures=""
out=""
python=""
zarrista_revision=""
cpuset=""
numa_node=""
mode="warm"
reps=5
iterations=5
while (($#)); do
  case "$1" in
    --fixtures|--out|--python|--zarrista-revision|--cpuset|--numa-node|--mode|--reps|--iterations)
      (($# >= 2)) || die "$1 requires a value"
      key="${1#--}"
      key="${key//-/_}"
      printf -v "$key" '%s' "$2"
      shift 2
      ;;
    --help)
      usage
      exit 0
      ;;
    *) die "unknown option: $1" ;;
  esac
done

[[ -n "$fixtures" && -d "$fixtures" ]] || die "--fixtures must name an existing directory"
[[ -n "$out" ]] || die "--out is required"
[[ -n "$python" && -x "$python" ]] || die "--python must name an executable interpreter"
[[ -n "$zarrista_revision" ]] || die "--zarrista-revision is required"
[[ "$cpuset" =~ ^[0-9]+$ ]] || die "--cpuset must be one physical CPU id"
[[ "$numa_node" =~ ^[0-9]+$ ]] || die "--numa-node must be a NUMA node id"
[[ "$mode" == "warm" || "$mode" == "cold" ]] || die "--mode must be warm or cold"
[[ "$reps" =~ ^[1-9][0-9]*$ ]] || die "--reps must be a positive integer"
[[ "$iterations" =~ ^[1-9][0-9]*$ ]] || die "--iterations must be a positive integer"
command -v numactl >/dev/null || die "numactl is required"
command -v taskset >/dev/null || die "taskset is required"
[[ -x /usr/bin/time ]] || die "/usr/bin/time is required"
if [[ "$mode" == "cold" && "$EUID" -ne 0 ]]; then
  die "cold measurements require root to drop the Linux page cache"
fi
"$python" -c 'import numpy, zarrista' || die "--python lacks numpy or zarrista"

thread_env=(
  BLOSC_NTHREADS=1
  OMP_NUM_THREADS=1
  OPENBLAS_NUM_THREADS=1
  MKL_NUM_THREADS=1
  VECLIB_MAXIMUM_THREADS=1
  RAYON_NUM_THREADS=1
  TOKIO_WORKER_THREADS=1
)
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
driver="$root/tools/zarrista_bench.py"
[[ -f "$driver" ]] || die "missing Zarrista benchmark driver: $driver"
fixtures="$(cd "$fixtures" && pwd)"
out="$(mkdir -p "$out" && cd "$out" && pwd)"
mode_out="$out/$mode/zarrista"
mkdir -p "$mode_out"

shopt -s nullglob
stores=("$fixtures"/*.zarr)
((${#stores[@]})) || die "no .zarr fixture directories under $fixtures"

record_environment() {
  local destination="$1"
  {
    echo "command: $0 $*"
    echo "source_revision: $(git -C "$root" rev-parse HEAD)"
    echo "source_status:"
    git -C "$root" status --short
    echo "date_utc: $(date --utc --iso-8601=seconds)"
    echo "uname: $(uname -a)"
    echo "cpuset: $cpuset"
    echo "numa_node: $numa_node"
    echo "mode: $mode"
    echo "reps: $reps"
    echo "warm_iterations: $iterations"
    echo "zarrista_revision: $zarrista_revision"
    echo "python: $($python --version)"
    echo "zarrista:"
    "$python" -c 'import zarrista; print(getattr(zarrista, "__version__", "unknown")); print(zarrista.__file__)'
    echo "thread_environment:"
    printf '%s\n' "${thread_env[@]}"
    echo "lscpu:"
    lscpu
    echo "numactl_hardware:"
    numactl --hardware
  } >"$destination"
}

run_python() {
  env "${thread_env[@]}" \
    numactl --cpunodebind="$numa_node" --membind="$numa_node" \
    taskset -c "$cpuset" "$@"
}

drop_caches() {
  sync
  printf '3\n' >/proc/sys/vm/drop_caches
}

run_one() {
  local store="$1"
  local rep="$2"
  local run_iterations="$iterations"
  local destination="$mode_out/$(basename "$store")/Zarrista/rep-$rep"
  local -a cmd

  if [[ "$mode" == "cold" ]]; then
    drop_caches
    run_iterations=1
  fi
  mkdir -p "$destination"
  cmd=(
    env "${thread_env[@]}"
    numactl --cpunodebind="$numa_node" --membind="$numa_node"
    taskset -c "$cpuset"
    /usr/bin/time -v -o "$destination/time-v.txt"
    "$python" "$driver"
    --store "$store"
    --mode "$mode"
    --iterations "$run_iterations"
    --out "$destination"
    --zarrista-revision "$zarrista_revision"
  )
  printf '%q ' "${cmd[@]}" >"$destination/command.txt"
  printf '\n' >>"$destination/command.txt"
  "${cmd[@]}" >"$destination/stdout.txt" 2>"$destination/stderr.txt"
}

record_environment "$mode_out/environment.txt" "${original_args[@]}"
for store in "${stores[@]}"; do
  run_python "$python" "$driver" --store "$store" --zarrista-revision "$zarrista_revision" --verify \
    >"$mode_out/verify-$(basename "$store").stdout.txt" \
    2>"$mode_out/verify-$(basename "$store").stderr.txt"
  for ((rep = 1; rep <= reps; rep++)); do
    run_one "$store" "$rep"
  done
done
