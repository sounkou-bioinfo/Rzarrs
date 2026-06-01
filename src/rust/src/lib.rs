mod nested;
mod vcf_schema;
mod zip_extract;

#[cfg(feature = "async-altrep")]
mod altrep_async;

use savvy::savvy;
use savvy::{
    NotAvailableValue, NullSexp, OwnedIntegerSexp, OwnedListSexp, OwnedLogicalSexp, OwnedRealSexp,
    OwnedStringSexp, TypedSexp,
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
use zarrs_object_store::{AsyncObjectStore, object_store};

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

#[derive(Clone)]
struct TokioBlockOn(Arc<tokio::runtime::Runtime>);

impl AsyncToSyncBlockOn for TokioBlockOn {
    fn block_on<F: core::future::Future>(&self, future: F) -> F::Output {
        self.0.block_on(future)
    }
}

/// Build a Tokio runtime owned by the storage adapter.  Do not leak runtimes:
/// every object-store-backed handle owns an `Arc<Runtime>` through `TokioBlockOn`.
fn make_tokio_runtime() -> savvy::Result<Arc<tokio::runtime::Runtime>> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map(Arc::new)
        .map_err(|e| savvy::Error::new(&format!("cannot build tokio runtime: {e}")))
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
        let runtime = make_tokio_runtime()?;
        let async_store = Arc::new(AsyncObjectStore::new(store));
        let sync_store = Arc::new(AsyncToSyncStorageAdapter::new(
            async_store,
            TokioBlockOn(runtime),
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

    /// Retrieve array data using R indexing semantics.
    ///
    /// `starts` and `ends` are 1-based, inclusive R coordinates.  `NULL, NULL`
    /// reads the full array.  The returned vector is already laid out in R
    /// column-major order; R code must not call `rev()`, `dim<-`, or `aperm()`
    /// to repair the result.
    ///
    /// @param starts 1-based inclusive start indices, one per dimension, or `NULL`.
    /// @param ends 1-based inclusive end indices, one per dimension, or `NULL`.
    /// @returns An atomic R vector; arrays with `ndim > 1` have a `dim` attribute.
    /// @export
    fn retrieve(&self, starts: savvy::Sexp, ends: savvy::Sexp) -> savvy::Result<savvy::Sexp> {
        let shape = self.inner.shape();
        let ranges = build_r_ranges(starts, ends, shape)?;
        let dims = ranges_to_i32_dims(&ranges)?;
        let subset = ArraySubset::new_with_ranges(&ranges);
        let raw = self.inner.data_type().to_string();
        let dtype = raw.split(" / ").next().unwrap_or(&raw).to_string();
        let mut out = retrieve_typed(&self.inner, &subset, &dtype, &dims)?;
        if dims.len() > 1 {
            out.set_dim(&dims)?;
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn coerce_to_i64_vec(s: savvy::Sexp, label: &str) -> savvy::Result<Vec<i64>> {
    match s.into_typed() {
        TypedSexp::Integer(v) => {
            let mut out = Vec::with_capacity(v.len());
            for &x in v.iter() {
                if x.is_na() {
                    return Err(savvy::Error::new(&format!("{label} cannot contain NA")));
                }
                out.push(x as i64);
            }
            Ok(out)
        }
        TypedSexp::Real(v) => {
            let mut out = Vec::with_capacity(v.len());
            for &x in v.iter() {
                if x.is_na() || !x.is_finite() {
                    return Err(savvy::Error::new(&format!(
                        "{label} cannot contain NA/NaN/Inf"
                    )));
                }
                if x.fract() != 0.0 {
                    return Err(savvy::Error::new(&format!(
                        "{label} must contain whole-number indices; got {x}"
                    )));
                }
                if x < i64::MIN as f64 || x > i64::MAX as f64 {
                    return Err(savvy::Error::new(&format!(
                        "{label} index is outside the i64 range: {x}"
                    )));
                }
                out.push(x as i64);
            }
            Ok(out)
        }
        _ => Err(savvy::Error::new(&format!(
            "{label} must be an integer or double vector, or NULL"
        ))),
    }
}

fn build_r_ranges(
    starts: savvy::Sexp,
    ends: savvy::Sexp,
    shape: &[u64],
) -> savvy::Result<Vec<std::ops::Range<u64>>> {
    if starts.is_null() && ends.is_null() {
        return Ok(shape.iter().map(|&d| 0..d).collect());
    }
    if starts.is_null() || ends.is_null() {
        return Err(savvy::Error::new(
            "starts and ends must both be NULL or both be index vectors",
        ));
    }

    let ndim = shape.len();
    let sv = coerce_to_i64_vec(starts, "starts")?;
    let ev = coerce_to_i64_vec(ends, "ends")?;
    if sv.len() != ndim || ev.len() != ndim {
        return Err(savvy::Error::new(&format!(
            "starts and ends must each have length ndim ({ndim})"
        )));
    }

    let mut ranges = Vec::with_capacity(ndim);
    for i in 0..ndim {
        let start1 = sv[i];
        let end1 = ev[i];
        if start1 < 1 {
            return Err(savvy::Error::new(&format!(
                "starts[{}] must be >= 1; got {}",
                i + 1,
                start1
            )));
        }
        if end1 < start1 {
            return Err(savvy::Error::new(&format!(
                "ends[{}] must be >= starts[{}]; got {} < {}",
                i + 1,
                i + 1,
                end1,
                start1
            )));
        }
        if end1 as u64 > shape[i] {
            return Err(savvy::Error::new(&format!(
                "ends[{}] out of range: got {}, dimension length is {}",
                i + 1,
                end1,
                shape[i]
            )));
        }
        ranges.push((start1 as u64 - 1)..(end1 as u64));
    }
    Ok(ranges)
}

fn ranges_to_i32_dims(ranges: &[std::ops::Range<u64>]) -> savvy::Result<Vec<i32>> {
    let mut dims = Vec::with_capacity(ranges.len());
    for (i, r) in ranges.iter().enumerate() {
        let len = r.end - r.start;
        if len > i32::MAX as u64 {
            return Err(savvy::Error::new(&format!(
                "selected dimension {} has length {}; R array dims are limited to i32",
                i + 1,
                len
            )));
        }
        dims.push(len as i32);
    }
    Ok(dims)
}

fn maybe_c_to_r_order<T: Clone>(data: Vec<T>, dims: &[i32]) -> Vec<T> {
    if dims.len() > 1 {
        c_to_r_order(&data, dims)
    } else {
        data
    }
}

fn retrieve_typed(
    array: &Array<dyn ReadableStorageTraits>,
    subset: &ArraySubset,
    dtype: &str,
    dims: &[i32],
) -> savvy::Result<savvy::Sexp> {
    let n: usize = subset.num_elements() as usize;

    match dtype {
        "float32" => {
            let data: Vec<f32> = array
                .retrieve_array_subset::<Vec<f32>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let data = maybe_c_to_r_order(data, dims);
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
            let data = maybe_c_to_r_order(data, dims);
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
            let data = maybe_c_to_r_order(data, dims);
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
            let data = maybe_c_to_r_order(data, dims);
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
            let data = maybe_c_to_r_order(data, dims);
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
            let data = maybe_c_to_r_order(data, dims);
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
            let data = maybe_c_to_r_order(data, dims);
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
            let data = maybe_c_to_r_order(data, dims);
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
            let data = maybe_c_to_r_order(data, dims);
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
            let data = maybe_c_to_r_order(data, dims);
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
            let data = maybe_c_to_r_order(data, dims);
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
            let data = maybe_c_to_r_order(data, dims);
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

// ---------------------------------------------------------------------------
// ZarrVcf — VCF Zarr reader
// ---------------------------------------------------------------------------

/// A high-level reader for VCF Zarr stores (spec versions 0.1–0.4).
///
/// @export
#[savvy]
pub struct ZarrVcf {
    store: Arc<dyn ReadListStorage>,
    vcf_version: String,
    array_names: Vec<String>,
    n_variants: i32,
    n_samples: i32,
    samples: Vec<String>,
    contigs: Vec<String>,
    filters: Vec<String>,
}

fn looks_like_url(path: &str) -> bool {
    path.starts_with("http://")
        || path.starts_with("https://")
        || path.starts_with("s3://")
        || path.starts_with("gs://")
        || path.starts_with("az://")
        || path.starts_with("file://")
}

fn path_to_name(path: &str) -> &str {
    path.strip_prefix('/').unwrap_or(path)
}

fn init_vcf_from_store(store: Arc<dyn ReadListStorage>) -> savvy::Result<ZarrVcf> {
    let group = Group::open(store.clone(), "/")
        .map_err(|e| savvy::Error::new(&format!("cannot open root group: {e}")))?;

    let attrs = group.attributes();
    let vcf_version = attrs
        .get("vcf_zarr_version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.1")
        .to_string();

    let children = group
        .children(false)
        .map_err(|e| savvy::Error::new(&format!("cannot list children: {e}")))?;

    let array_names: Vec<String> = children
        .iter()
        .filter(|n| matches!(n.metadata(), NodeMetadata::Array(_)))
        .map(|n| path_to_name(n.path().as_str()).to_string())
        .collect();

    // n_variants from variant_position array shape
    let n_variants = if array_names.contains(&"variant_position".to_string()) {
        let storage: Arc<dyn ReadableStorageTraits> = store.clone();
        if let Ok(arr) = Array::open(storage, "/variant_position") {
            let shape = arr.shape();
            if !shape.is_empty() {
                shape[0] as i32
            } else {
                0
            }
        } else {
            0
        }
    } else {
        0
    };

    // samples
    let samples = if array_names.contains(&"sample_id".to_string()) {
        let storage: Arc<dyn ReadableStorageTraits> = store.clone();
        if let Ok(arr) = Array::open(storage, "/sample_id") {
            let subset = ArraySubset::new_with_ranges(&[0..arr.shape()[0]]);
            let data: Vec<String> = arr
                .retrieve_array_subset::<Vec<String>>(&subset)
                .unwrap_or_default();
            data
        } else {
            vec![]
        }
    } else {
        attrs
            .get("sample_id")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    };
    let n_samples = samples.len() as i32;

    // contigs
    let contigs = if array_names.contains(&"contig_id".to_string()) {
        let storage: Arc<dyn ReadableStorageTraits> = store.clone();
        if let Ok(arr) = Array::open(storage, "/contig_id") {
            let subset = ArraySubset::new_with_ranges(&[0..arr.shape()[0]]);
            arr.retrieve_array_subset::<Vec<String>>(&subset)
                .unwrap_or_default()
        } else {
            vec![]
        }
    } else {
        attrs
            .get("contigs")
            .or_else(|| attrs.get("contig_id"))
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .map(|v| {
                        if let Some(s) = v.as_str() {
                            s.to_string()
                        } else if let Some(obj) = v.as_object() {
                            obj.get("id")
                                .and_then(|id| id.as_str())
                                .unwrap_or("")
                                .to_string()
                        } else {
                            String::new()
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    // filters — v0.1 uses attrs$filters as [{id, description}, ...]
    let filters = if array_names.contains(&"filter_id".to_string()) {
        let storage: Arc<dyn ReadableStorageTraits> = store.clone();
        if let Ok(arr) = Array::open(storage, "/filter_id") {
            let subset = ArraySubset::new_with_ranges(&[0..arr.shape()[0]]);
            arr.retrieve_array_subset::<Vec<String>>(&subset)
                .unwrap_or_default()
        } else {
            vec![]
        }
    } else {
        // Try attrs$filter_id (v0.2 style without array) or attrs$filters (v0.1 style)
        attrs
            .get("filter_id")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .or_else(|| {
                attrs.get("filters").and_then(|v| v.as_array()).map(|a| {
                    a.iter()
                        .map(|v| {
                            v.as_object()
                                .and_then(|o| o.get("id"))
                                .and_then(|id| id.as_str())
                                .unwrap_or("")
                                .to_string()
                        })
                        .collect()
                })
            })
            .unwrap_or_default()
    };

    Ok(ZarrVcf {
        store,
        vcf_version,
        array_names,
        n_variants,
        n_samples,
        samples,
        contigs,
        filters,
    })
}

fn c_to_r_order<T: Clone>(data: &[T], dims: &[i32]) -> Vec<T> {
    let ndim = dims.len();
    if ndim <= 1 {
        return data.to_vec();
    }

    let dims_u64: Vec<u64> = dims.iter().map(|&d| d as u64).collect();

    let mut c_strides = vec![1u64; ndim];
    for i in (0..ndim - 1).rev() {
        c_strides[i] = c_strides[i + 1] * dims_u64[i + 1];
    }

    let mut r_strides = vec![1u64; ndim];
    for i in 1..ndim {
        r_strides[i] = r_strides[i - 1] * dims_u64[i - 1];
    }

    let n = data.len();
    if n == 0 {
        return Vec::new();
    }
    let mut result = vec![data[0].clone(); n];
    for linear in 0..n {
        let mut remaining = linear as u64;
        let mut coords = vec![0u64; ndim];
        for i in 0..ndim {
            coords[i] = remaining / c_strides[i];
            remaining %= c_strides[i];
        }
        let r_pos: usize = coords
            .iter()
            .zip(&r_strides)
            .map(|(&c, &s)| c * s)
            .sum::<u64>() as usize;
        result[r_pos] = data[linear].clone();
    }
    result
}

/// @export
#[savvy]
impl ZarrVcf {
    /// Open a VCF Zarr store from a local path or URL.
    ///
    /// Automatically detects `.zip` files (extracted to a temp directory)
    /// and URL schemes (`http://`, `s3://`, `gs://`, `az://`, `file://`).
    ///
    /// @param path Path to a `.zarr` directory, a `.zip` file, or a URL.
    /// @returns A `ZarrVcf` object.
    /// @export
    fn open(path: &str) -> savvy::Result<Self> {
        if looks_like_url(path) {
            return Self::open_object_store(path);
        }

        let store: Arc<dyn ReadListStorage> = if path.to_lowercase().ends_with(".zip") {
            let entries = zip_extract::read_zip_entries(path)
                .map_err(|e| savvy::Error::new(&format!("cannot read zip '{path}': {e}")))?;
            let mem_store = zarrs::storage::store::MemoryStore::new();
            for (name, data) in &entries {
                let key = zarrs::storage::StoreKey::new(name.as_str())
                    .map_err(|e| savvy::Error::new(&format!("invalid store key '{name}': {e}")))?;
                use zarrs::storage::WritableStorageTraits;
                mem_store
                    .set(&key, zarrs::storage::Bytes::copy_from_slice(data))
                    .map_err(|e| savvy::Error::new(&format!("cannot set key '{name}': {e}")))?;
            }
            Arc::new(mem_store)
        } else {
            let pb = std::path::PathBuf::from(path);
            if !pb.exists() {
                return Err(savvy::Error::new(&format!("path does not exist: {path}")));
            }
            let fs_store =
                FilesystemStore::new(pb).map_err(|e| savvy::Error::new(&e.to_string()))?;
            Arc::new(fs_store)
        };

        init_vcf_from_store(store)
    }

    /// Open a VCF Zarr store from an object-store URL (S3, GCS, Azure, HTTP/HTTPS).
    ///
    /// Credentials are discovered from the process environment variables automatically.
    ///
    /// @param url Store URL, e.g. `"s3://my-bucket/path/to/store.zarr"`.
    /// @returns A `ZarrVcf` object.
    /// @export
    fn open_object_store(url: &str) -> savvy::Result<Self> {
        let parsed = url::Url::parse(url)
            .map_err(|e| savvy::Error::new(&format!("invalid URL '{url}': {e}")))?;
        let (store, path) = object_store::parse_url_opts(&parsed, std::env::vars())
            .map_err(|e| savvy::Error::new(&format!("cannot open object store: {e}")))?;
        let store: Box<dyn object_store::ObjectStore> =
            if path == object_store::path::Path::default() {
                store
            } else {
                Box::new(object_store::prefix::PrefixStore::new(store, path))
            };
        let runtime = make_tokio_runtime()?;
        let async_store = Arc::new(AsyncObjectStore::new(store));
        let sync_store: Arc<dyn ReadListStorage> = Arc::new(AsyncToSyncStorageAdapter::new(
            async_store,
            TokioBlockOn(runtime),
        ));
        init_vcf_from_store(sync_store)
    }

    /// Open a VCF Zarr store from a `ZarrStore` (local filesystem).
    ///
    /// @param store A `ZarrStore` object.
    /// @returns A `ZarrVcf` object.
    /// @export
    fn open_store(store: &ZarrStore) -> savvy::Result<Self> {
        let storage: Arc<dyn ReadListStorage> = store.inner.clone();
        init_vcf_from_store(storage)
    }

    /// VCF Zarr spec version string.
    ///
    /// @returns A character scalar.
    /// @export
    fn version(&self) -> savvy::Result<savvy::Sexp> {
        let mut out = OwnedStringSexp::new(1)?;
        out.set_elt(0, &self.vcf_version)?;
        Ok(out.into())
    }

    /// Number of variants.
    ///
    /// @returns An integer scalar.
    /// @export
    fn n_variants(&self) -> savvy::Result<savvy::Sexp> {
        Ok(savvy::Sexp::try_from(self.n_variants)?)
    }

    /// Number of samples.
    ///
    /// @returns An integer scalar.
    /// @export
    fn n_samples(&self) -> savvy::Result<savvy::Sexp> {
        Ok(savvy::Sexp::try_from(self.n_samples)?)
    }

    /// Sample IDs.
    ///
    /// @returns A character vector.
    /// @export
    fn samples(&self) -> savvy::Result<savvy::Sexp> {
        let n = self.samples.len();
        let mut out = OwnedStringSexp::new(n)?;
        for (i, s) in self.samples.iter().enumerate() {
            out.set_elt(i, s)?;
        }
        Ok(out.into())
    }

    /// Contig (chromosome) names.
    ///
    /// @returns A character vector.
    /// @export
    fn contigs(&self) -> savvy::Result<savvy::Sexp> {
        let n = self.contigs.len();
        let mut out = OwnedStringSexp::new(n)?;
        for (i, s) in self.contigs.iter().enumerate() {
            out.set_elt(i, s)?;
        }
        Ok(out.into())
    }

    /// Filter IDs.
    ///
    /// @returns A character vector.
    /// @export
    fn filters(&self) -> savvy::Result<savvy::Sexp> {
        let n = self.filters.len();
        let mut out = OwnedStringSexp::new(n)?;
        for (i, s) in self.filters.iter().enumerate() {
            out.set_elt(i, s)?;
        }
        Ok(out.into())
    }

    /// Available array names in this VCF Zarr store.
    ///
    /// @returns A character vector.
    /// @export
    fn fields(&self) -> savvy::Result<savvy::Sexp> {
        let n = self.array_names.len();
        let mut out = OwnedStringSexp::new(n)?;
        for (i, s) in self.array_names.iter().enumerate() {
            out.set_elt(i, s)?;
        }
        Ok(out.into())
    }

    /// Variant positions (1-based genomic coordinates).
    ///
    /// @returns An integer vector.
    /// @export
    fn variant_position(&self) -> savvy::Result<savvy::Sexp> {
        self.retrieve_int_array("variant_position")
    }

    /// Variant contig names (resolved from contig index).
    ///
    /// @returns A character vector.
    /// @export
    fn variant_contig(&self) -> savvy::Result<savvy::Sexp> {
        let idx = self.retrieve_int_array_inner("variant_contig")?;
        let n = idx.len();
        let mut out = OwnedStringSexp::new(n)?;
        for (i, &j) in idx.iter().enumerate() {
            if j < 0 || j as usize >= self.contigs.len() {
                out.set_na(i)?;
            } else {
                out.set_elt(i, &self.contigs[j as usize])?;
            }
        }
        Ok(out.into())
    }

    /// Variant alleles (REF/ALT pairs).
    ///
    /// @returns A character vector with `dim` attribute `[n_variants, n_alleles]`.
    /// @export
    fn variant_allele(&self) -> savvy::Result<savvy::Sexp> {
        let storage: Arc<dyn ReadableStorageTraits> = self.store.clone();
        let array = Array::open(storage, "/variant_allele")
            .map_err(|e| savvy::Error::new(&format!("cannot open array variant_allele: {e}")))?;
        let shape = array.shape();
        let subset = ArraySubset::new_with_ranges(&shape.iter().map(|&d| 0..d).collect::<Vec<_>>());
        let data: Vec<String> = array
            .retrieve_array_subset::<Vec<String>>(&subset)
            .map_err(|e| savvy::Error::new(&e.to_string()))?;

        let dims: Vec<i32> = shape.iter().map(|&d| d as i32).collect();
        let reordered = c_to_r_order(&data, &dims);
        let mut out = OwnedStringSexp::new(reordered.len())?;
        for (i, s) in reordered.iter().enumerate() {
            out.set_elt(i, s)?;
        }
        out.set_dim(&dims)?;
        Ok(out.into())
    }

    /// Genotype array (variants × samples × ploidy).
    ///
    /// @param variants Optional 1-based variant indices to subset, or `NULL`.
    /// @param samples Optional 1-based sample indices to subset, or `NULL`.
    /// @returns A 3-dimensional integer array.
    /// @export
    fn genotypes(
        &self,
        variants: Option<savvy::Sexp>,
        samples: Option<savvy::Sexp>,
    ) -> savvy::Result<savvy::Sexp> {
        let storage: Arc<dyn ReadableStorageTraits> = self.store.clone();
        let array = Array::open(storage, "/call_genotype")
            .map_err(|e| savvy::Error::new(&format!("cannot open array call_genotype: {e}")))?;
        let shape = array.shape();
        if shape.len() != 3 {
            return Err(savvy::Error::new(
                "call_genotype must have dimensions variant x sample x ploidy",
            ));
        }
        let nv = shape[0] as i32;
        let ns = shape[1] as i32;
        let ploidy = shape[2] as i32;

        let var_idx = parse_indices(variants.unwrap_or(NullSexp.into()), nv)?;
        let sam_idx = parse_indices(samples.unwrap_or(NullSexp.into()), ns)?;
        let dims = [var_idx.len() as i32, sam_idx.len() as i32, ploidy];

        let variant_runs = contiguous_index_runs(&var_idx, "variants")?;
        let sample_runs = contiguous_index_runs(&sam_idx, "samples")?;
        read_i32_3d_runs(&array, &variant_runs, &sample_runs, shape[2], &dims)
    }

    /// Boolean phased genotype matrix (variants × samples).
    ///
    /// @param variants Optional 1-based variant indices to subset, or `NULL`.
    /// @param samples Optional 1-based sample indices to subset, or `NULL`.
    /// @returns A 2-dimensional logical matrix.
    /// @export
    fn call_genotype_phased(
        &self,
        variants: Option<savvy::Sexp>,
        samples: Option<savvy::Sexp>,
    ) -> savvy::Result<savvy::Sexp> {
        let storage: Arc<dyn ReadableStorageTraits> = self.store.clone();
        let array = Array::open(storage, "/call_genotype_phased").map_err(|e| {
            savvy::Error::new(&format!("cannot open array call_genotype_phased: {e}"))
        })?;
        let shape = array.shape();
        if shape.len() != 2 {
            return Err(savvy::Error::new(
                "call_genotype_phased must have dimensions variant x sample",
            ));
        }
        let nv = shape[0] as i32;
        let ns = shape[1] as i32;

        let var_idx = parse_indices(variants.unwrap_or(NullSexp.into()), nv)?;
        let sam_idx = parse_indices(samples.unwrap_or(NullSexp.into()), ns)?;
        let dims = [var_idx.len() as i32, sam_idx.len() as i32];

        let variant_runs = contiguous_index_runs(&var_idx, "variants")?;
        let sample_runs = contiguous_index_runs(&sam_idx, "samples")?;
        read_bool_2d_runs(&array, &variant_runs, &sample_runs, &dims)
    }

    /// Generic accessor for `variant_<name>` arrays.
    ///
    /// @param name Array name suffix (e.g. `"position"` reads `variant_position`).
    /// @returns A vector with appropriate dtype and dims.
    /// @export
    fn variant(&self, name: &str) -> savvy::Result<savvy::Sexp> {
        let array_name = format!("variant_{}", name);
        self.retrieve_array_by_name(&array_name)
    }

    /// Generic accessor for `call_<name>` arrays.
    ///
    /// @param name Array name suffix (e.g. `"genotype"` reads `call_genotype`).
    /// @returns A vector with appropriate dtype and dims.
    /// @export
    fn call(&self, name: &str) -> savvy::Result<savvy::Sexp> {
        let array_name = format!("call_{}", name);
        self.retrieve_array_by_name(&array_name)
    }
}

// ---------------------------------------------------------------------------
// ZarrVcf internal helpers
// ---------------------------------------------------------------------------

fn parse_indices(sexp: savvy::Sexp, max_val: i32) -> savvy::Result<Vec<usize>> {
    if sexp.is_null() {
        if max_val <= 0 {
            return Err(savvy::Error::new("max_val must be positive"));
        }
        return Ok((0..max_val as usize).collect());
    }
    match sexp.into_typed() {
        TypedSexp::Integer(v) => {
            let mut indices = Vec::with_capacity(v.len());
            for &x in v.iter() {
                if x.is_na() {
                    return Err(savvy::Error::new("NA values are not allowed in indices"));
                }
                if x < 1 || x > max_val {
                    return Err(savvy::Error::new(&format!(
                        "index {} out of range [1, {}]",
                        x, max_val
                    )));
                }
                indices.push((x - 1) as usize);
            }
            Ok(indices)
        }
        TypedSexp::Real(v) => {
            let mut indices = Vec::with_capacity(v.len());
            for &x in v.iter() {
                if x.is_na() || !x.is_finite() {
                    return Err(savvy::Error::new(
                        "NA/NaN/Inf values are not allowed in indices",
                    ));
                }
                if x.fract() != 0.0 {
                    return Err(savvy::Error::new(&format!(
                        "indices must be whole numbers; got {x}"
                    )));
                }
                let xi = x as i32;
                if xi < 1 || xi > max_val {
                    return Err(savvy::Error::new(&format!(
                        "index {} out of range [1, {}]",
                        xi, max_val
                    )));
                }
                indices.push((xi - 1) as usize);
            }
            Ok(indices)
        }
        _ => Err(savvy::Error::new(
            "indices must be an integer or numeric vector, or NULL",
        )),
    }
}

#[derive(Debug, Clone)]
struct IndexRun {
    out_start: usize,
    range: std::ops::Range<u64>,
}

fn contiguous_index_runs(indices: &[usize], label: &str) -> savvy::Result<Vec<IndexRun>> {
    if indices.is_empty() {
        return Ok(Vec::new());
    }

    for pair in indices.windows(2) {
        if pair[1] <= pair[0] {
            return Err(savvy::Error::new(&format!(
                "{label} must be strictly increasing; unsorted or duplicated selections are rejected"
            )));
        }
    }

    let mut runs = Vec::new();
    let mut out_start = 0usize;
    let mut run_start = indices[0];
    let mut prev = indices[0];

    for (out_pos, &idx) in indices.iter().enumerate().skip(1) {
        if idx == prev + 1 {
            prev = idx;
        } else {
            runs.push(IndexRun {
                out_start,
                range: run_start as u64..prev as u64 + 1,
            });
            out_start = out_pos;
            run_start = idx;
            prev = idx;
        }
    }

    runs.push(IndexRun {
        out_start,
        range: run_start as u64..prev as u64 + 1,
    });
    Ok(runs)
}

fn read_i32_3d_runs(
    array: &Array<dyn ReadableStorageTraits>,
    variant_runs: &[IndexRun],
    sample_runs: &[IndexRun],
    ploidy: u64,
    dims: &[i32; 3],
) -> savvy::Result<savvy::Sexp> {
    let nv_out = dims[0] as usize;
    let ns_out = dims[1] as usize;
    let ploidy_out = dims[2] as usize;
    let mut result = vec![0_i32; nv_out * ns_out * ploidy_out];

    for vr in variant_runs {
        let v_len = (vr.range.end - vr.range.start) as usize;
        for sr in sample_runs {
            let s_len = (sr.range.end - sr.range.start) as usize;
            let subset =
                ArraySubset::new_with_ranges(&[vr.range.clone(), sr.range.clone(), 0..ploidy]);
            let data: Vec<i32> = array
                .retrieve_array_subset::<Vec<i32>>(&subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;

            for local_v in 0..v_len {
                let out_v = vr.out_start + local_v;
                for local_s in 0..s_len {
                    let out_s = sr.out_start + local_s;
                    for p in 0..ploidy_out {
                        let src = (local_v * s_len + local_s) * ploidy_out + p;
                        let dst = (out_v * ns_out + out_s) * ploidy_out + p;
                        result[dst] = data[src];
                    }
                }
            }
        }
    }

    fill_i32_array(&result, dims)
}

fn read_bool_2d_runs(
    array: &Array<dyn ReadableStorageTraits>,
    variant_runs: &[IndexRun],
    sample_runs: &[IndexRun],
    dims: &[i32; 2],
) -> savvy::Result<savvy::Sexp> {
    let nv_out = dims[0] as usize;
    let ns_out = dims[1] as usize;
    let mut result = vec![false; nv_out * ns_out];

    for vr in variant_runs {
        let v_len = (vr.range.end - vr.range.start) as usize;
        for sr in sample_runs {
            let s_len = (sr.range.end - sr.range.start) as usize;
            let subset = ArraySubset::new_with_ranges(&[vr.range.clone(), sr.range.clone()]);
            let data: Vec<bool> = array
                .retrieve_array_subset::<Vec<bool>>(&subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;

            for local_v in 0..v_len {
                let out_v = vr.out_start + local_v;
                for local_s in 0..s_len {
                    let out_s = sr.out_start + local_s;
                    let src = local_v * s_len + local_s;
                    let dst = out_v * ns_out + out_s;
                    result[dst] = data[src];
                }
            }
        }
    }

    fill_logical_array(&result, dims)
}

fn fill_i32_array(data: &[i32], dims: &[i32]) -> savvy::Result<savvy::Sexp> {
    let reordered = maybe_c_to_r_order(data.to_vec(), dims);
    let mut out = OwnedIntegerSexp::new(reordered.len())?;
    for (i, &v) in reordered.iter().enumerate() {
        if v == i32::MIN {
            out.set_na(i)?;
        } else {
            out[i] = v;
        }
    }
    if dims.len() > 1 {
        out.set_dim(dims)?;
    }
    Ok(out.into())
}

fn fill_logical_array(data: &[bool], dims: &[i32]) -> savvy::Result<savvy::Sexp> {
    let reordered = maybe_c_to_r_order(data.to_vec(), dims);
    let mut out = OwnedLogicalSexp::new(reordered.len())?;
    for (i, &v) in reordered.iter().enumerate() {
        out.set_elt(i, v)?;
    }
    if dims.len() > 1 {
        out.set_dim(dims)?;
    }
    Ok(out.into())
}

impl ZarrVcf {
    fn retrieve_int_array_inner(&self, name: &str) -> savvy::Result<Vec<i32>> {
        let storage: Arc<dyn ReadableStorageTraits> = self.store.clone();
        let path = format!("/{name}");
        let array = Array::open(storage, &path)
            .map_err(|e| savvy::Error::new(&format!("cannot open array '{name}': {e}")))?;
        let shape = array.shape();
        let subset = ArraySubset::new_with_ranges(&shape.iter().map(|&d| 0..d).collect::<Vec<_>>());
        array
            .retrieve_array_subset::<Vec<i32>>(&subset)
            .map_err(|e| savvy::Error::new(&e.to_string()))
    }

    fn retrieve_int_array(&self, name: &str) -> savvy::Result<savvy::Sexp> {
        let data = self.retrieve_int_array_inner(name)?;
        let n = data.len();
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

    fn retrieve_array_by_name(&self, name: &str) -> savvy::Result<savvy::Sexp> {
        if !self.array_names.contains(&name.to_string()) {
            return Err(savvy::Error::new(&format!(
                "array '{}' not found in this VCF Zarr store",
                name
            )));
        }

        let storage: Arc<dyn ReadableStorageTraits> = self.store.clone();
        let path = format!("/{name}");
        let array = Array::open(storage, &path)
            .map_err(|e| savvy::Error::new(&format!("cannot open array '{name}': {e}")))?;
        let shape = array.shape();
        let subset = ArraySubset::new_with_ranges(&shape.iter().map(|&d| 0..d).collect::<Vec<_>>());
        let dtype = array.data_type().to_string();
        let dtype = dtype.split(" / ").next().unwrap_or(&dtype).to_string();

        let dims: Vec<i32> = shape.iter().map(|&d| d as i32).collect();
        let mut out = retrieve_typed(&array, &subset, &dtype, &dims)?;
        if dims.len() > 1 {
            out.set_dim(&dims)?;
        }
        Ok(out)
    }
}
