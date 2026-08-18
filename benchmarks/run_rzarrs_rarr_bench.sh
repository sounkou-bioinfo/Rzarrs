#!/usr/bin/env bash
# Run reproducible Rzarrs-versus-Rarr full-array read benchmarks.
set -euo pipefail
original_args=("$@")

usage() {
  cat <<'EOF'
Usage: benchmarks/run_rzarrs_rarr_bench.sh --fixtures DIR --out DIR --cpuset LIST --numa-node N [options]

Options:
  --fixtures DIR    directory created by make_benchmark_fixtures.R
  --out DIR         output directory, outside the source tree if possible
  --cpuset CPU      one physical CPU id for this single-thread baseline
  --numa-node N     NUMA node to bind CPU and memory allocations to
  --mode MODE       warm or cold (default: warm)
  --reps N          process-level replicates (default: 5)
  --iterations N    bench iterations per warm process (default: 5; cold uses 1)

Warm measurements pre-read the same fixture in each fresh process. Cold
measurements drop the Linux page cache before every process-level replicate,
which requires root. The order alternates by replicate.
EOF
}

die() {
  echo "error: $*" >&2
  exit 2
}

fixtures=""
out=""
cpuset=""
numa_node=""
mode="warm"
reps=5
iterations=5
while (($#)); do
  case "$1" in
    --fixtures|--out|--cpuset|--numa-node|--mode|--reps|--iterations)
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

# Keep hidden native pools from changing the single-core comparison. Rzarrs's
# shipped path uses Tokio's current-thread runtime; these settings also pin a
# future Rayon/Tokio/BLAS path unless its API deliberately overrides them.
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
benchmark="$root/benchmarks/rzarrs_rarr_bench.R"
[[ -f "$benchmark" ]] || die "missing benchmark script: $benchmark"
fixtures="$(cd "$fixtures" && pwd)"
out="$(mkdir -p "$out" && cd "$out" && pwd)"
mode_out="$out/$mode"
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
    echo "thread_environment:"
    printf '%s\n' "${thread_env[@]}"
    echo "current_build_environment_not_retrospective_flags:"
    for name in CARGO_ENCODED_RUSTFLAGS CARGO_PROFILE_RELEASE_LTO CARGO_PROFILE_RELEASE_CODEGEN_UNITS RUSTFLAGS CC CXX CFLAGS CXXFLAGS CPPFLAGS LDFLAGS MAKEFLAGS; do
      printf '%s=%s\n' "$name" "${!name-<unset>}"
    done
    echo "current_R_build_configuration:"
    for name in CC CFLAGS CPPFLAGS CXX CXXFLAGS CXX11 CXX11FLAGS LDFLAGS; do
      printf '%s=' "$name"
      R CMD config "$name" 2>&1 || true
    done
    echo "Rust_toolchain:"
    rustc -Vv 2>&1 || true
    cargo -V 2>&1 || true
    echo "Rzarrs_Cargo_release_profile:"
    grep -n -A 8 '^\[profile\.release\]' "$root/src/rust/Cargo.toml" || true
    echo "installed_reader_artifacts:"
    Rscript - <<'RS'
for (package in c("Rzarrs", "Rarr")) {
  root <- find.package(package)
  artifacts <- list.files(file.path(root, "libs"), recursive = TRUE, full.names = TRUE,
                          pattern = "\\.(so|dylib|dll)$")
  cat("package=", package, "\nversion=", as.character(packageVersion(package)), "\n", sep = "")
  for (artifact in artifacts) {
    cat("artifact=", artifact, "\n", sep = "")
    if (nzchar(Sys.which("sha256sum"))) cat(system2("sha256sum", artifact, stdout = TRUE), sep = "\n")
    if (nzchar(Sys.which("readelf"))) cat(system2("readelf", c("-p", ".comment", artifact), stdout = TRUE, stderr = TRUE), sep = "\n")
    cat("\n")
  }
}
RS
    echo "lscpu:"
    lscpu
    echo "numactl_hardware:"
    numactl --hardware
    echo "r_packages:"
    Rscript -e 'for (p in c("Rzarrs", "Rarr", "bench")) cat(p, "=", as.character(utils::packageVersion(p)), "\n", sep = "")'
  } >"$destination"
}

run_r() {
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
  local implementation="$2"
  local rep="$3"
  local run_iterations="$iterations"
  local destination="$mode_out/$(basename "$store")/$implementation/rep-$rep"
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
    Rscript "$benchmark"
    --store "$store"
    --implementation "$implementation"
    --mode "$mode"
    --iterations "$run_iterations"
    --out "$destination"
  )
  printf '%q ' "${cmd[@]}" >"$destination/command.txt"
  printf '\n' >>"$destination/command.txt"
  "${cmd[@]}" >"$destination/stdout.txt" 2>"$destination/stderr.txt"
}

record_environment "$mode_out/environment.txt" "${original_args[@]}"
for store in "${stores[@]}"; do
  run_r Rscript "$benchmark" --store "$store" --verify \
    >"$mode_out/verify-$(basename "$store").stdout.txt" \
    2>"$mode_out/verify-$(basename "$store").stderr.txt"
  for ((rep = 1; rep <= reps; rep++)); do
    if ((rep % 2)); then
      implementations=(Rzarrs Rarr)
    else
      implementations=(Rarr Rzarrs)
    fi
    for implementation in "${implementations[@]}"; do
      run_one "$store" "$implementation" "$rep"
    done
  done
done
