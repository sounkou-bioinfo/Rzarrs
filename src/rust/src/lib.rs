use savvy::savvy;
use savvy::{
    NullSexp, OwnedIntegerSexp, OwnedLogicalSexp, OwnedRealSexp, OwnedStringSexp, TypedSexp,
};

use std::path::PathBuf;
use std::sync::Arc;

use zarrs::array::Array;
use zarrs::array::ArraySubset;
use zarrs::filesystem::FilesystemStore;
use zarrs::storage::ReadableStorageTraits;
use zarrs_http::HTTPStore;

// ---------------------------------------------------------------------------
// ZarrStore
// ---------------------------------------------------------------------------

/// A handle to a local Zarr filesystem store.
///
/// @export
#[savvy]
pub struct ZarrStore {
    inner: Arc<FilesystemStore>,
    path: String,
}

/// @export
#[savvy]
impl ZarrStore {
    /// Open a local Zarr store at the given path.
    ///
    /// @param path Path to the `.zarr` directory.
    /// @returns A `ZarrStore` object.
    /// @export
    fn open(path: &str) -> savvy::Result<Self> {
        let store = FilesystemStore::new(PathBuf::from(path))
            .map_err(|e| savvy::Error::new(&format!("cannot open store: {e}")))?;
        Ok(Self {
            inner: Arc::new(store),
            path: path.to_string(),
        })
    }

    /// Path of the store root.
    ///
    /// @returns A character scalar.
    /// @export
    fn path(&self) -> savvy::Result<savvy::Sexp> {
        let mut out = OwnedStringSexp::new(1)?;
        out.set_elt(0, &self.path)?;
        Ok(out.into())
    }
}

// ---------------------------------------------------------------------------
// ZarrHttpStore
// ---------------------------------------------------------------------------

/// A handle to a remote Zarr store accessed over HTTP/HTTPS.
///
/// @export
#[savvy]
pub struct ZarrHttpStore {
    inner: Arc<HTTPStore>,
    url: String,
}

/// @export
#[savvy]
impl ZarrHttpStore {
    /// Open a remote Zarr store at the given HTTP/HTTPS URL.
    ///
    /// @param url Base URL of the `.zarr` store, e.g.
    ///   `"https://example.com/my.zarr"`.
    /// @returns A `ZarrHttpStore` object.
    /// @export
    fn open(url: &str) -> savvy::Result<Self> {
        let store = HTTPStore::new(url)
            .map_err(|e| savvy::Error::new(&format!("cannot open HTTP store: {e}")))?;
        Ok(Self {
            inner: Arc::new(store),
            url: url.to_string(),
        })
    }

    /// Base URL of the store.
    ///
    /// @returns A character scalar.
    /// @export
    fn url(&self) -> savvy::Result<savvy::Sexp> {
        let mut out = OwnedStringSexp::new(1)?;
        out.set_elt(0, &self.url)?;
        Ok(out.into())
    }
}

// ---------------------------------------------------------------------------
// ZarrArray
// ---------------------------------------------------------------------------

/// A handle to a Zarr array within a store.
///
/// @export
#[savvy]
pub struct ZarrArray {
    inner: Array<dyn ReadableStorageTraits>,
}

/// @export
#[savvy]
impl ZarrArray {
    /// Open a Zarr array at the given path within a store.
    ///
    /// @param store A `ZarrStore` object.
    /// @param path Array path within the store, e.g. `"/"`.
    /// @returns A `ZarrArray` object.
    /// @export
    fn open(store: &ZarrStore, path: &str) -> savvy::Result<Self> {
        let storage: Arc<dyn ReadableStorageTraits> = store.inner.clone();
        let array = Array::open(storage, path)
            .map_err(|e| savvy::Error::new(&format!("cannot open array '{path}': {e}")))?;
        Ok(Self { inner: array })
    }

    /// Open a Zarr array at the given path within an HTTP store.
    ///
    /// @param store A `ZarrHttpStore` object.
    /// @param path Array path within the store, e.g. `"/"`.
    /// @returns A `ZarrArray` object.
    /// @export
    fn open_http(store: &ZarrHttpStore, path: &str) -> savvy::Result<Self> {
        let storage: Arc<dyn ReadableStorageTraits> = store.inner.clone();
        let array = Array::open(storage, path)
            .map_err(|e| savvy::Error::new(&format!("cannot open array '{path}': {e}")))?;
        Ok(Self { inner: array })
    }

    /// Number of dimensions.
    ///
    /// @returns An integer scalar.
    /// @export
    fn ndim(&self) -> savvy::Result<savvy::Sexp> {
        let n = self.inner.shape().len() as i32;
        Ok(savvy::Sexp::try_from(n)?)
    }

    /// Shape of the array (one element per dimension).
    ///
    /// Dimensions exceeding `.Machine$integer.max` are returned as `NA_integer_`.
    ///
    /// @returns An integer vector.
    /// @export
    fn shape(&self) -> savvy::Result<savvy::Sexp> {
        let shape = self.inner.shape();
        let mut out = OwnedIntegerSexp::new(shape.len())?;
        for (i, &d) in shape.iter().enumerate() {
            if d > i32::MAX as u64 {
                out.set_na(i)?;
            } else {
                out[i] = d as i32;
            }
        }
        Ok(out.into())
    }

    /// Chunk shape of the array (one element per dimension).
    ///
    /// @returns An integer vector, or `NULL` if not a regular chunk grid.
    /// @export
    fn chunk_shape(&self) -> savvy::Result<savvy::Sexp> {
        let ndim = self.inner.shape().len();
        match self.inner.chunk_shape(&vec![0u64; ndim]) {
            Ok(shape) => {
                let mut out = OwnedIntegerSexp::new(shape.len())?;
                for (i, d) in shape.iter().enumerate() {
                    let v = d.get();
                    if v > i32::MAX as u64 {
                        out.set_na(i)?;
                    } else {
                        out[i] = v as i32;
                    }
                }
                Ok(out.into())
            }
            Err(_) => Ok(NullSexp.into()),
        }
    }

    /// Data type name.
    ///
    /// @returns A character scalar, e.g. `"float32"`, `"int32"`, `"bool"`.
    /// @export
    fn dtype(&self) -> savvy::Result<savvy::Sexp> {
        let raw = self.inner.data_type().to_string();
        let name = raw.split(" / ").next().unwrap_or(&raw);
        let mut out = OwnedStringSexp::new(1)?;
        out.set_elt(0, name)?;
        Ok(out.into())
    }

    /// Dimension names, if any.
    ///
    /// @returns A character vector, or `NULL` if the array has no dimension names.
    /// @export
    fn dimension_names(&self) -> savvy::Result<savvy::Sexp> {
        match self.inner.dimension_names() {
            None => Ok(NullSexp.into()),
            Some(names) => {
                let mut out = OwnedStringSexp::new(names.len())?;
                for (i, n) in names.iter().enumerate() {
                    match n {
                        Some(s) => out.set_elt(i, s.as_str())?,
                        None => out.set_na(i)?,
                    }
                }
                Ok(out.into())
            }
        }
    }

    /// Pretty-printed JSON array metadata.
    ///
    /// @returns A character scalar containing the raw Zarr metadata JSON.
    /// @export
    fn metadata_json(&self) -> savvy::Result<savvy::Sexp> {
        let json = serde_json::to_string_pretty(self.inner.metadata())
            .map_err(|e| savvy::Error::new(&e.to_string()))?;
        let mut out = OwnedStringSexp::new(1)?;
        out.set_elt(0, &json)?;
        Ok(out.into())
    }

    /// Retrieve array data as an R array (vector with a `dim` attribute).
    ///
    /// Zarr dtypes are mapped to R types as follows:
    ///
    /// | Zarr dtype           | R type    | Notes                               |
    /// |----------------------|-----------|-------------------------------------|
    /// | float32 / float64    | `double`  | NaN -> `NA_real_`; Inf preserved    |
    /// | int8 / int16 / int32 | `integer` | `i32::MIN` -> `NA_integer_`         |
    /// | int64                | `double`  | exact to 2^53                       |
    /// | uint8 / uint16       | `integer` | always fits                         |
    /// | uint32 / uint64      | `double`  |                                     |
    /// | bool                 | `logical` |                                     |
    ///
    /// @param starts Integer or double vector of 0-based start indices (one per
    ///   dimension), or `NULL` to start from the origin.
    /// @param ends Integer or double vector of 0-based exclusive end indices (one
    ///   per dimension), or `NULL` to use the full extent.
    /// @returns A vector of the appropriate R type with a `dim` attribute.
    /// @export
    fn retrieve(&self, starts: savvy::Sexp, ends: savvy::Sexp) -> savvy::Result<savvy::Sexp> {
        let shape = self.inner.shape();
        let ndim = shape.len();

        let ranges: Vec<std::ops::Range<u64>> = if starts.is_null() {
            shape.iter().map(|&d| 0..d).collect()
        } else {
            let sv = coerce_to_f64(starts)?;
            let ev = coerce_to_f64(ends)?;
            if sv.len() != ndim || ev.len() != ndim {
                return Err(savvy::Error::new(&format!(
                    "starts and ends must each have length ndim ({ndim})"
                )));
            }
            (0..ndim).map(|i| sv[i] as u64..ev[i] as u64).collect()
        };

        let dims: Vec<i32> = ranges.iter().map(|r| (r.end - r.start) as i32).collect();
        let subset = ArraySubset::new_with_ranges(&ranges);
        let dtype = self.inner.data_type().to_string();
        // DataType::Display emits "v3name / v2name" for V2 arrays; take the V3 name.
        let dtype = dtype.split(" / ").next().unwrap_or(&dtype).to_string();
        let mut out = retrieve_typed(&self.inner, &subset, &dtype)?;
        out.set_dim(&dims)?;
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn coerce_to_f64(s: savvy::Sexp) -> savvy::Result<Vec<f64>> {
    match s.into_typed() {
        TypedSexp::Integer(v) => Ok(v.iter().map(|&x| x as f64).collect()),
        TypedSexp::Real(v) => Ok(v.iter().copied().collect()),
        _ => Err(savvy::Error::new(
            "starts/ends must be an integer or double vector",
        )),
    }
}

fn retrieve_typed(
    array: &Array<dyn ReadableStorageTraits>,
    subset: &ArraySubset,
    dtype: &str,
) -> savvy::Result<savvy::Sexp> {
    let n: usize = subset.num_elements() as usize;

    match dtype {
        "float32" | "float64" => {
            let data: Vec<f64> = array
                .retrieve_array_subset::<Vec<f64>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = OwnedRealSexp::new(n)?;
            for (i, &v) in data.iter().enumerate() {
                if v.is_nan() {
                    out.set_na(i)?;
                } else {
                    out[i] = v;
                }
            }
            Ok(out.into())
        }

        "int8" | "int16" | "int32" => {
            let data: Vec<i32> = array
                .retrieve_array_subset::<Vec<i32>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = OwnedIntegerSexp::new(n)?;
            for (i, &v) in data.iter().enumerate() {
                if v == i32::MIN {
                    out.set_na(i)?;
                } else {
                    out[i] = v;
                }
            }
            Ok(out.into())
        }

        "int64" => {
            let data: Vec<i64> = array
                .retrieve_array_subset::<Vec<i64>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = OwnedRealSexp::new(n)?;
            for (i, &v) in data.iter().enumerate() {
                out[i] = v as f64;
            }
            Ok(out.into())
        }

        "uint8" | "uint16" => {
            let data: Vec<u16> = array
                .retrieve_array_subset::<Vec<u16>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = OwnedIntegerSexp::new(n)?;
            for (i, &v) in data.iter().enumerate() {
                out[i] = v as i32;
            }
            Ok(out.into())
        }

        "uint32" | "uint64" => {
            let data: Vec<u64> = array
                .retrieve_array_subset::<Vec<u64>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = OwnedRealSexp::new(n)?;
            for (i, &v) in data.iter().enumerate() {
                out[i] = v as f64;
            }
            Ok(out.into())
        }

        "bool" => {
            let data: Vec<u8> = array
                .retrieve_array_subset::<Vec<u8>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = OwnedLogicalSexp::new(n)?;
            for (i, &v) in data.iter().enumerate() {
                out.set_elt(i, v != 0)?;
            }
            Ok(out.into())
        }

        other => Err(savvy::Error::new(&format!(
            "dtype '{other}' is not yet supported by Rzarrs"
        ))),
    }
}
