#!/usr/bin/env python3
"""Measure Zarrista full-array materialization for a validated local fixture.

This is a Python/NumPy comparison point, not an R-binding comparison. It uses
Zarrista's synchronous Array API and materializes the complete result through
FixedLengthTensor.to_numpy(). The shell runner supplies CPU/NUMA and thread
controls, process-level replicates, and GNU time -v resource measurements.
"""

from __future__ import annotations

import argparse
import csv
import gc
import json
from pathlib import Path
import shutil
import platform
import statistics
import sys
import time


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--store", required=True, type=Path, help="Zarr fixture directory")
    parser.add_argument("--mode", choices=("warm", "cold"), default="warm")
    parser.add_argument("--iterations", type=int, default=5)
    parser.add_argument("--out", type=Path, help="Directory for benchmark artifacts")
    parser.add_argument(
        "--zarrista-revision",
        required=True,
        help="Exact upstream Git revision used to build the Python environment",
    )
    parser.add_argument(
        "--verify",
        action="store_true",
        help="Validate output shape, dtype, and boundary values without timing",
    )
    args = parser.parse_args()
    if not args.store.is_dir():
        parser.error("--store must name an existing Zarr directory")
    if args.iterations < 1:
        parser.error("--iterations must be positive")
    if not args.verify and args.out is None:
        parser.error("--out is required unless --verify is used")
    return args


def fixture_contract(store: Path) -> tuple[tuple[int, int], int]:
    manifest = store / "benchmark-fixture.dcf"
    if not manifest.is_file():
        raise RuntimeError(f"missing benchmark fixture manifest: {manifest}")
    fields: dict[str, str] = {}
    for line in manifest.read_text(encoding="utf-8").splitlines():
        if not line:
            continue
        separator = ":" if ":" in line else "="
        key, value = line.split(separator, maxsplit=1)
        fields[key.strip()] = value.strip()
    rows, columns = (int(value) for value in fields["shape"].split("x"))
    return (rows, columns), rows * columns


def load_api():
    from zarrista import Array
    from zarrista.store import FilesystemStore

    return Array, FilesystemStore


def materialize(store_path: Path, api):
    array_type, store_type = api
    store = store_type(str(store_path))
    array = array_type.open(store, path="/")
    return array[...].to_numpy()


def validate(value, contract: tuple[tuple[int, int], int]) -> None:
    import numpy as np

    shape, last = contract
    if value.shape != shape:
        raise RuntimeError(f"unexpected shape: {value.shape}; expected {shape}")
    if value.dtype != np.dtype("int32"):
        raise RuntimeError(f"unexpected dtype: {value.dtype}; expected int32")
    if int(value[0, 0]) != 1 or int(value[-1, -1]) != last:
        raise RuntimeError("unexpected boundary values")


def main() -> None:
    args = parse_args()
    import zarrista

    contract = fixture_contract(args.store)
    api = load_api()
    if args.verify:
        validate(materialize(args.store, api), contract)
        print(f"verified {args.store}")
        return

    if args.mode == "warm":
        validate(materialize(args.store, api), contract)
    gc.collect()

    elapsed: list[float] = []
    for _ in range(args.iterations):
        started = time.perf_counter()
        value = materialize(args.store, api)
        elapsed.append(time.perf_counter() - started)
        validate(value, contract)
        del value

    args.out.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(args.store / "benchmark-fixture.dcf", args.out / "fixture.dcf")
    summary = {
        "implementation": "Zarrista",
        "runtime": "Python",
        "runtime_version": platform.python_version(),
        "implementation_version": zarrista.__version__,
        "benchmark_engine": "time.perf_counter",
        "benchmark_engine_version": platform.python_version(),
        "measurement_scope": "loaded runtime; open plus full array materialization",
        "startup_included": False,
        "mode": args.mode,
        "store": str(args.store.resolve()),
        "iterations_requested": args.iterations,
        "iterations_completed": len(elapsed),
        "min_s": min(elapsed),
        "median_s": statistics.median(elapsed),
        "mean_s": statistics.fmean(elapsed),
        "total_s": sum(elapsed),
    }
    with (args.out / "summary.csv").open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=summary.keys())
        writer.writeheader()
        writer.writerow(summary)
    with (args.out / "python-environment.json").open("w", encoding="utf-8") as handle:
        json.dump(
            {
                "implementation": sys.implementation.name,
                "python": sys.version,
                "platform": platform.platform(),
                "zarrista_revision": args.zarrista_revision,
                "zarrista_version": zarrista.__version__,
            },
            handle,
            indent=2,
            sort_keys=True,
        )
        handle.write("\n")
    print(json.dumps(summary, sort_keys=True))


if __name__ == "__main__":
    main()
