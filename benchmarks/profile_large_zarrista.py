#!/usr/bin/env python3

import argparse
import gc
from pathlib import Path

from zarrista import Array
from zarrista.store import FilesystemStore

parser = argparse.ArgumentParser()
parser.add_argument("--store", required=True, type=Path)
args = parser.parse_args()
store_path = args.store.resolve(strict=True)
manifest = {}
for line in (store_path / "benchmark-fixture.dcf").read_text().splitlines():
    key, value = line.split(":", maxsplit=1)
    manifest[key.strip()] = value.strip()
shape = tuple(int(value) for value in manifest["shape"].split("x"))
if len(shape) != 2 or any(value <= 0 for value in shape):
    raise ValueError("profile fixture must have a positive two-dimensional shape")

array = Array.open(FilesystemStore(str(store_path)), path="/")

def materialize():
    value = array[...].to_numpy()
    if value.shape != shape:
        raise ValueError(f"unexpected shape: {value.shape}")
    return value

value = materialize()
del value
gc.collect()
for _ in range(3):
    value = materialize()
    del value
