use savvy::savvy;
use savvy::{
    NullSexp, OwnedIntegerSexp, OwnedListSexp, OwnedLogicalSexp, OwnedRealSexp, OwnedStringSexp,
    TypedSexp,
};

use std::path::PathBuf;
use std::sync::Arc;

use zarrs::array::Array;
use zarrs::array::ArraySubset;
use zarrs::filesystem::FilesystemStore;
use zarrs::group::Group;
use zarrs::node::NodeMetadata;
use zarrs::storage::storage_adapter::async_to_sync::{
    AsyncToSyncBlockOn, AsyncToSyncStorageAdapter,
};
use zarrs::storage::{ListableStorageTraits, ReadableStorageTraits};
use zarrs_object_store::AsyncObjectStore;

// ---------------------------------------------------------------------------
// Force-link codec plugins registered via `inventory`.
// When zarrs is compiled as a staticlib these registrations are dropped by the
// linker because nothing references them.  Touching each codec type ensures
// its `inventory::submit!` global constructor survives into the final .so.
// ---------------------------------------------------------------------------
// Combined readable+listable trait object
trait ReadListStorage: ReadableStorageTraits + ListableStorageTraits {}
impl<T: ReadableStorageTraits + ListableStorageTraits> ReadListStorage for T {}

// ---------------------------------------------------------------------------
// Tokio block_on adapter
// ---------------------------------------------------------------------------

struct TokioBlockOn(tokio::runtime::Handle);

impl AsyncToSyncBlockOn for TokioBlockOn {
    fn block_on<F: core::future::Future>(&self, future: F) -> F::Output {
        self.0.block_on(future)
    }
}

/// Build a one-shot tokio multi-thread runtime and return its handle.
/// The runtime is intentionally leaked so the handle stays valid for the
/// lifetime of the ZarrObjectStore / ZarrArray that holds it.
fn make_tokio_handle() -> savvy::Result<tokio::runtime::Handle> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| savvy::Error::new(&format!("cannot build tokio runtime: {e}")))?;
    let handle = rt.handle().clone();
    std::mem::forget(rt); // keep the runtime alive
    Ok(handle)
}

// ---------------------------------------------------------------------------
// ZarrStore  (local filesystem)
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
        let pb = PathBuf::from(path);
        if !pb.exists() {
            return Err(savvy::Error::new(&format!("path does not exist: {path}")));
        }
        let store = FilesystemStore::new(pb)
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
// ZarrObjectStore  (S3 / GCS / Azure / HTTP/HTTPS via object_store)
//
// Credentials are read from standard environment variables:
//   S3    – AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_REGION, AWS_ENDPOINT_URL
//   GCS   – GOOGLE_APPLICATION_CREDENTIALS  (or instance metadata)
//   Azure – AZURE_STORAGE_ACCOUNT, AZURE_STORAGE_ACCESS_KEY / AZURE_CLIENT_ID etc.
//
// URL schemes understood by object_store::parse_url_opts:
//   s3://bucket/prefix   gs://bucket/prefix   az://container/prefix
//   https://...          file:///path
// ---------------------------------------------------------------------------

/// A handle to an object-store Zarr backend (S3, GCS, Azure, …).
///
/// Credentials are discovered from the standard environment variables for each
/// provider — the same variables used by the AWS CLI, `gsutil`, `azcopy`, etc.
/// No credentials need to be passed to R; set them in the process environment
/// before calling `ZarrObjectStore$open()`.
///
/// @export
#[savvy]
pub struct ZarrObjectStore {
    storage: Arc<
        AsyncToSyncStorageAdapter<
            AsyncObjectStore<Box<dyn object_store::ObjectStore>>,
            TokioBlockOn,
        >,
    >,
    url: String,
}

/// @export
#[savvy]
impl ZarrObjectStore {
    /// Open an object-store Zarr backend from a URL.
    ///
    /// Supported URL schemes: `s3://`, `gs://`, `az://`, `https://`,
    /// `file:///`. Credentials are read from the process environment
    /// automatically — set the standard provider env vars
    /// (`AWS_ACCESS_KEY_ID` / `GOOGLE_APPLICATION_CREDENTIALS` /
    /// `AZURE_STORAGE_ACCOUNT` etc.) before calling this function.
    ///
    /// @param url Store URL, e.g. `"s3://my-bucket/path/to/store.zarr"`.
    /// @returns A `ZarrObjectStore` object.
    /// @export
    fn open(url: &str) -> savvy::Result<Self> {
        let parsed = url::Url::parse(url)
            .map_err(|e| savvy::Error::new(&format!("invalid URL '{url}': {e}")))?;
        // Pass all process env vars — object_store builders pick up whichever
        // ones are relevant for the selected backend (AWS_*, GOOGLE_*, AZURE_*…).
        let (store, path) = object_store::parse_url_opts(&parsed, std::env::vars())
            .map_err(|e| savvy::Error::new(&format!("cannot open object store: {e}")))?;
        // Wrap with a prefix so array paths are resolved relative to the URL's
        // path component rather than the storage root.
        let store: Box<dyn object_store::ObjectStore> =
            if path == object_store::path::Path::default() {
                store
            } else {
                Box::new(object_store::prefix::PrefixStore::new(store, path))
            };
        let handle = make_tokio_handle()?;
        let async_store = Arc::new(AsyncObjectStore::new(store));
        let sync_store = Arc::new(AsyncToSyncStorageAdapter::new(
            async_store,
            TokioBlockOn(handle),
        ));
        Ok(Self {
            storage: sync_store,
            url: url.to_string(),
        })
    }

    /// URL passed to `open()`.
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
// ZarrGroup
// ---------------------------------------------------------------------------

/// A handle to a Zarr group (root or sub-group) within a store.
///
/// Groups may contain arrays and/or sub-groups.  Use `$attributes()` to read
/// the group's JSON attributes as a native R list, and `$children()` to list
/// the immediate child nodes.
///
/// @export
#[savvy]
pub struct ZarrGroup {
    inner: Group<dyn ReadListStorage>,
}

/// @export
#[savvy]
impl ZarrGroup {
    /// Open a group from a local filesystem store.
    ///
    /// @param store A `ZarrStore` object.
    /// @param path Group path within the store, e.g. `"/"`.
    /// @returns A `ZarrGroup` object.
    /// @export
    fn open(store: &ZarrStore, path: &str) -> savvy::Result<Self> {
        let storage: Arc<dyn ReadListStorage> = store.inner.clone();
        let group = Group::open(storage, path)
            .map_err(|e| savvy::Error::new(&format!("cannot open group '{path}': {e}")))?;
        Ok(Self { inner: group })
    }

    /// Open a group from an object-store backend (S3, GCS, Azure, HTTP/HTTPS…).
    ///
    /// @param store A `ZarrObjectStore` object.
    /// @param path Group path within the store, e.g. `"/"`.
    /// @returns A `ZarrGroup` object.
    /// @export
    fn open_object_store(store: &ZarrObjectStore, path: &str) -> savvy::Result<Self> {
        let storage: Arc<dyn ReadListStorage> = store.storage.clone();
        let group = Group::open(storage, path)
            .map_err(|e| savvy::Error::new(&format!("cannot open group '{path}': {e}")))?;
        Ok(Self { inner: group })
    }

    /// Group attributes as a native R list.
    ///
    /// @returns A named list (may be empty).
    /// @export
    fn attributes(&self) -> savvy::Result<savvy::Sexp> {
        let map = self.inner.attributes();
        let mut out = OwnedListSexp::new(map.len(), true)?;
        for (i, (k, v)) in map.iter().enumerate() {
            out.set_name_and_value(i, k, json_to_sexp(v)?)?;
        }
        Ok(out.into())
    }

    /// Group attributes as a raw JSON string.
    ///
    /// @returns A character scalar.
    /// @export
    fn attributes_json(&self) -> savvy::Result<savvy::Sexp> {
        let json = serde_json::to_string_pretty(self.inner.attributes())
            .map_err(|e| savvy::Error::new(&e.to_string()))?;
        let mut out = OwnedStringSexp::new(1)?;
        out.set_elt(0, &json)?;
        Ok(out.into())
    }

    /// List child node paths and their types.
    ///
    /// Returns a data-frame-like named list with two character vectors:
    /// `$path` (absolute path string) and `$kind` (`"array"` or `"group"`).
    ///
    /// @param recursive If `TRUE`, descend into sub-groups.
    /// @returns A named list with elements `path` and `kind`.
    /// @export
    fn children(&self, recursive: bool) -> savvy::Result<savvy::Sexp> {
        let nodes = self
            .inner
            .children(recursive)
            .map_err(|e| savvy::Error::new(&format!("cannot list children: {e}")))?;

        let n = nodes.len();
        let mut paths = OwnedStringSexp::new(n)?;
        let mut kinds = OwnedStringSexp::new(n)?;

        for (i, node) in nodes.iter().enumerate() {
            paths.set_elt(i, node.path().as_str())?;
            let kind = match node.metadata() {
                NodeMetadata::Array(_) => "array",
                NodeMetadata::Group(_) => "group",
            };
            kinds.set_elt(i, kind)?;
        }

        let mut out = OwnedListSexp::new(2, true)?;
        out.set_name_and_value(0, "path", paths)?;
        out.set_name_and_value(1, "kind", kinds)?;
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

// Internal helper — open an array from an AsyncToSync-wrapped store.
fn open_via_adapter<S, B>(
    storage: Arc<AsyncToSyncStorageAdapter<S, B>>,
    path: &str,
) -> savvy::Result<ZarrArray>
where
    S: zarrs::storage::AsyncReadableStorageTraits
        + zarrs::storage::AsyncListableStorageTraits
        + 'static,
    B: AsyncToSyncBlockOn + 'static,
{
    let storage: Arc<dyn ReadableStorageTraits> = storage;
    let array = Array::open(storage, path)
        .map_err(|e| savvy::Error::new(&format!("cannot open array '{path}': {e}")))?;
    Ok(ZarrArray { inner: array })
}

/// @export
#[savvy]
impl ZarrArray {
    /// Open a Zarr array from a local filesystem store.
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

    /// Open a Zarr array from an object-store backend (S3, GCS, Azure, HTTP/HTTPS…).
    ///
    /// @param store A `ZarrObjectStore` object.
    /// @param path Array path within the store, e.g. `"/"`.
    /// @returns A `ZarrArray` object.
    /// @export
    fn open_object_store(store: &ZarrObjectStore, path: &str) -> savvy::Result<Self> {
        open_via_adapter(store.storage.clone(), path)
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

    /// Data type name (V3 canonical form, e.g. `"float32"`, `"int32"`, `"bool"`).
    ///
    /// @returns A character scalar.
    /// @export
    fn dtype(&self) -> savvy::Result<savvy::Sexp> {
        let raw = self.inner.data_type().to_string();
        // Display emits "v3name / v2name" for V2 arrays — keep only the V3 name.
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
    /// @returns A character scalar.
    /// @export
    fn metadata_json(&self) -> savvy::Result<savvy::Sexp> {
        let json = serde_json::to_string_pretty(self.inner.metadata())
            .map_err(|e| savvy::Error::new(&e.to_string()))?;
        let mut out = OwnedStringSexp::new(1)?;
        out.set_elt(0, &json)?;
        Ok(out.into())
    }

    /// Array metadata as a native R list (no external JSON package required).
    ///
    /// @returns A named list mirroring the Zarr array metadata.
    /// @export
    fn metadata(&self) -> savvy::Result<savvy::Sexp> {
        let v = serde_json::to_value(self.inner.metadata())
            .map_err(|e| savvy::Error::new(&e.to_string()))?;
        json_to_sexp(&v)
    }

    /// Retrieve array data as a flat vector with a `dim` attribute (C order).
    ///
    /// This is the low-level internal function. R users should call the
    /// high-level `$retrieve()` method defined in `R/array.R`, which accepts
    /// 1-based inclusive indices and handles C-to-Fortran axis reordering.
    ///
    /// @param starts 0-based start indices (one per dimension), or `NULL`.
    /// @param ends 0-based exclusive end indices (one per dimension), or `NULL`.
    /// @returns A vector with a `dim` attribute in C order.
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
        let raw = self.inner.data_type().to_string();
        let dtype = raw.split(" / ").next().unwrap_or(&raw).to_string();
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
        "float32" => {
            let data: Vec<f32> = array
                .retrieve_array_subset::<Vec<f32>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = OwnedRealSexp::new(n)?;
            for (i, &v) in data.iter().enumerate() {
                out[i] = v as f64;
            }
            Ok(out.into())
        }

        "float64" => {
            let data: Vec<f64> = array
                .retrieve_array_subset::<Vec<f64>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = OwnedRealSexp::new(n)?;
            for (i, &v) in data.iter().enumerate() {
                out[i] = v;
            }
            Ok(out.into())
        }

        "int8" => {
            let data: Vec<i8> = array
                .retrieve_array_subset::<Vec<i8>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = OwnedIntegerSexp::new(n)?;
            for (i, &v) in data.iter().enumerate() {
                out[i] = v as i32;
            }
            Ok(out.into())
        }

        "int16" => {
            let data: Vec<i16> = array
                .retrieve_array_subset::<Vec<i16>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = OwnedIntegerSexp::new(n)?;
            for (i, &v) in data.iter().enumerate() {
                out[i] = v as i32;
            }
            Ok(out.into())
        }

        "int32" => {
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

        "uint8" => {
            let data: Vec<u8> = array
                .retrieve_array_subset::<Vec<u8>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = OwnedIntegerSexp::new(n)?;
            for (i, &v) in data.iter().enumerate() {
                out[i] = v as i32;
            }
            Ok(out.into())
        }

        "uint16" => {
            let data: Vec<u16> = array
                .retrieve_array_subset::<Vec<u16>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = OwnedIntegerSexp::new(n)?;
            for (i, &v) in data.iter().enumerate() {
                out[i] = v as i32;
            }
            Ok(out.into())
        }

        "uint32" => {
            let data: Vec<u32> = array
                .retrieve_array_subset::<Vec<u32>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = OwnedRealSexp::new(n)?;
            for (i, &v) in data.iter().enumerate() {
                out[i] = v as f64;
            }
            Ok(out.into())
        }

        "uint64" => {
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
            let data: Vec<bool> = array
                .retrieve_array_subset::<Vec<bool>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = OwnedLogicalSexp::new(n)?;
            for (i, &v) in data.iter().enumerate() {
                out.set_elt(i, v)?;
            }
            Ok(out.into())
        }

        "string" => {
            let data: Vec<String> = array
                .retrieve_array_subset::<Vec<String>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = OwnedStringSexp::new(n)?;
            for (i, v) in data.iter().enumerate() {
                out.set_elt(i, v.as_str())?;
            }
            Ok(out.into())
        }

        other => Err(savvy::Error::new(&format!(
            "dtype '{other}' is not yet supported by Rzarrs"
        ))),
    }
}

// ---------------------------------------------------------------------------
// JSON → R (serde_json::Value → savvy::Sexp) — no R-side JSON dep needed
// ---------------------------------------------------------------------------

fn json_to_sexp(v: &serde_json::Value) -> savvy::Result<savvy::Sexp> {
    use serde_json::Value;
    match v {
        Value::Null => Ok(NullSexp.into()),

        Value::Bool(b) => {
            let mut out = OwnedLogicalSexp::new(1)?;
            out.set_elt(0, *b)?;
            Ok(out.into())
        }

        Value::Number(n) => {
            let mut out = OwnedRealSexp::new(1)?;
            out[0] = n.as_f64().unwrap_or(f64::NAN);
            Ok(out.into())
        }

        Value::String(s) => {
            let mut out = OwnedStringSexp::new(1)?;
            out.set_elt(0, s)?;
            Ok(out.into())
        }

        Value::Array(arr) => {
            // Homogeneous scalar arrays → atomic R vectors for convenience.
            if arr.iter().all(|x| x.is_f64() || x.is_i64() || x.is_u64()) && !arr.is_empty() {
                let mut out = OwnedRealSexp::new(arr.len())?;
                for (i, x) in arr.iter().enumerate() {
                    out[i] = x.as_f64().unwrap_or(f64::NAN);
                }
                return Ok(out.into());
            }
            if arr.iter().all(|x| x.is_string()) && !arr.is_empty() {
                let mut out = OwnedStringSexp::new(arr.len())?;
                for (i, x) in arr.iter().enumerate() {
                    out.set_elt(i, x.as_str().unwrap_or(""))?;
                }
                return Ok(out.into());
            }
            if arr.iter().all(|x| x.is_boolean()) && !arr.is_empty() {
                let mut out = OwnedLogicalSexp::new(arr.len())?;
                for (i, x) in arr.iter().enumerate() {
                    out.set_elt(i, x.as_bool().unwrap_or(false))?;
                }
                return Ok(out.into());
            }
            // Mixed / nested → unnamed list
            let mut out = OwnedListSexp::new(arr.len(), false)?;
            for (i, x) in arr.iter().enumerate() {
                out.set_value(i, json_to_sexp(x)?)?;
            }
            Ok(out.into())
        }

        Value::Object(map) => {
            let mut out = OwnedListSexp::new(map.len(), true)?;
            for (i, (k, val)) in map.iter().enumerate() {
                out.set_name_and_value(i, k, json_to_sexp(val)?)?;
            }
            Ok(out.into())
        }
    }
}
