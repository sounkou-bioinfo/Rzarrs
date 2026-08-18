mod nested;
mod vcf_schema;

#[cfg(feature = "async-altrep")]
mod altrep_async;

use savvy::savvy;
use savvy::{
    Complex64 as RComplex64, NotAvailableValue, NullSexp, OwnedComplexSexp, OwnedIntegerSexp,
    OwnedListSexp, OwnedLogicalSexp, OwnedRealSexp, OwnedStringSexp, TypedSexp,
};

use dashu_int::{IBig, UBig};
use half::{bf16, f16};
use num_complex::{Complex32, Complex64 as NumComplex64};

use std::path::PathBuf;
use std::sync::Arc;

use zarrs::array::Array;
use zarrs::array::ArraySubset;
use zarrs::array::data_type::{NumpyDateTime64DataType, NumpyTimeDelta64DataType, NumpyTimeUnit};
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
    inner: Arc<dyn ReadListStorage>,
    path: String,
}

/// @export
#[savvy]
impl ZarrStore {
    /// Open a local Zarr store at the given path.
    ///
    /// @param path Path to a `.zarr` directory or local `.zarr.zip`/`.zip` archive.
    /// @returns A `ZarrStore` object.
    /// @export
    fn open(path: &str) -> savvy::Result<Self> {
        let pb = PathBuf::from(path);
        if !pb.exists() {
            return Err(savvy::Error::new(&format!("path does not exist: {path}")));
        }
        let inner: Arc<dyn ReadListStorage> = if path.to_lowercase().ends_with(".zip") {
            open_local_zip_store(path)?
        } else {
            let store = FilesystemStore::new(pb)
                .map_err(|e| savvy::Error::new(&format!("cannot open store: {e}")))?;
            Arc::new(store)
        };
        Ok(Self {
            inner,
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
    storage: Arc<dyn ReadListStorage>,
    url: String,
}

fn parse_object_store_url(
    url: &str,
) -> Result<(Box<dyn object_store::ObjectStore>, object_store::path::Path), savvy::Error> {
    let parsed = url::Url::parse(url)
        .map_err(|e| savvy::Error::new(&format!("invalid URL '{url}': {e}")))?;
    object_store::parse_url_opts(&parsed, std::env::vars())
        .map_err(|e| savvy::Error::new(&format!("cannot open object store: {e}")))
}

fn prefix_object_store(
    store: Box<dyn object_store::ObjectStore>,
    path: object_store::path::Path,
) -> Box<dyn object_store::ObjectStore> {
    if path == object_store::path::Path::default() {
        store
    } else {
        Box::new(object_store::prefix::PrefixStore::new(store, path))
    }
}

fn sync_object_store(
    store: Box<dyn object_store::ObjectStore>,
) -> savvy::Result<
    Arc<
        AsyncToSyncStorageAdapter<
            AsyncObjectStore<Box<dyn object_store::ObjectStore>>,
            TokioBlockOn,
        >,
    >,
> {
    let runtime = make_tokio_runtime()?;
    let async_store = Arc::new(AsyncObjectStore::new(store));
    Ok(Arc::new(AsyncToSyncStorageAdapter::new(
        async_store,
        TokioBlockOn(runtime),
    )))
}

fn open_object_store_url(url: &str) -> savvy::Result<Arc<dyn ReadListStorage>> {
    let (store, path) = parse_object_store_url(url)?;
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
    {
        return open_object_store_zip_url(store, path, url);
    }

    let store = prefix_object_store(store, path);
    let sync_store = sync_object_store(store)?;
    Ok(sync_store)
}

#[cfg(feature = "zip")]
fn open_object_store_zip_url(
    store: Box<dyn object_store::ObjectStore>,
    path: object_store::path::Path,
    url: &str,
) -> savvy::Result<Arc<dyn ReadListStorage>> {
    let zip_name = path
        .filename()
        .ok_or_else(|| savvy::Error::new(&format!("URL does not name a zip object: {url}")))?;
    let zip_parent = path.parent().unwrap_or_default();
    let store = prefix_object_store(store, zip_parent);
    let sync_store = sync_object_store(store)?;
    let storage_key = zarrs::storage::StoreKey::try_from(zip_name)
        .map_err(|e| savvy::Error::new(&format!("invalid zip object key '{zip_name}': {e}")))?;
    let zip_store = zarrs_zip::ZipStorageAdapter::new(sync_store, storage_key)
        .map_err(|e| savvy::Error::new(&format!("cannot open zip object '{url}': {e}")))?;
    Ok(Arc::new(zip_store))
}

#[cfg(not(feature = "zip"))]
fn open_object_store_zip_url(
    _store: Box<dyn object_store::ObjectStore>,
    _path: object_store::path::Path,
    _url: &str,
) -> savvy::Result<Arc<dyn ReadListStorage>> {
    Err(savvy::Error::new(
        "remote .zarr.zip support is disabled; reinstall with Rust feature 'zip'",
    ))
}

/// @export
#[savvy]
impl ZarrObjectStore {
    /// Open an object-store Zarr backend from a URL.
    ///
    /// Supported URL schemes: `s3://`, `gs://`, `az://`, `https://`,
    /// `file:///`. URLs ending in the .zarr.zip or .zip suffix are opened as zip
    /// objects when zip support is enabled. Credentials are read
    /// from the process environment automatically — set the standard provider env vars
    /// (`AWS_ACCESS_KEY_ID` / `GOOGLE_APPLICATION_CREDENTIALS` /
    /// `AZURE_STORAGE_ACCOUNT` etc.) before calling this function.
    ///
    /// @param url Store URL, e.g. `"s3://my-bucket/path/to/store.zarr"`.
    /// @returns A `ZarrObjectStore` object.
    /// @export
    fn open(url: &str) -> savvy::Result<Self> {
        let storage = open_object_store_url(url)?;
        Ok(Self {
            storage,
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
        let storage: Arc<dyn ReadableStorageTraits> = store.storage.clone();
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

    /// Planned R materialization for this array's dtype.
    ///
    /// This does not read array data.  It reports whether the dtype can be
    /// mapped to base R exactly, needs an explicit cast policy, or requires a
    /// package-owned extension vector/list class.
    ///
    /// @returns A named list describing the dtype conversion policy.
    /// @export
    fn dtype_plan(&self) -> savvy::Result<savvy::Sexp> {
        dtype_plan_to_sexp(self.inner.data_type().to_string().as_str())
    }

    /// Codec names declared by this array, including nested sharding codecs.
    ///
    /// @returns A character vector of codec names in metadata traversal order.
    /// @export
    fn codecs(&self) -> savvy::Result<savvy::Sexp> {
        let value = serde_json::to_value(self.inner.metadata())
            .map_err(|e| savvy::Error::new(&e.to_string()))?;
        let mut codec_names = Vec::new();
        collect_codec_names_from_value(&value, &mut codec_names);
        Ok(string_vec_to_sexp(&codec_names)?.into())
    }

    /// Report whether this array's declared codecs are supported by Rzarrs.
    ///
    /// @returns A named list with `codec` and `supported` vectors.
    /// @export
    fn codec_capabilities(&self) -> savvy::Result<savvy::Sexp> {
        let value = serde_json::to_value(self.inner.metadata())
            .map_err(|e| savvy::Error::new(&e.to_string()))?;
        let mut codec_names = Vec::new();
        collect_codec_names_from_value(&value, &mut codec_names);
        codec_capabilities_to_sexp(&codec_names)
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

fn scalar_string(value: &str) -> savvy::Result<OwnedStringSexp> {
    let mut out = OwnedStringSexp::new(1)?;
    out.set_elt(0, value)?;
    Ok(out)
}

fn scalar_logical(value: bool) -> savvy::Result<OwnedLogicalSexp> {
    let mut out = OwnedLogicalSexp::new(1)?;
    out.set_elt(0, value)?;
    Ok(out)
}

fn dtype_plan_to_sexp(dtype_name: &str) -> savvy::Result<savvy::Sexp> {
    let canonical = dtype_name
        .split(" / ")
        .next()
        .unwrap_or(dtype_name)
        .to_string();
    let plan = nested::plan_dtype(&canonical, nested::Integer64Policy::Int64Class);

    let mut out = OwnedListSexp::new(9, true)?;
    out.set_name_and_value(0, "dtype", scalar_string(&plan.dtype_name)?)?;
    out.set_name_and_value(1, "r_type", scalar_string(&format!("{:?}", plan.r_type))?)?;
    out.set_name_and_value(
        2,
        "precision",
        scalar_string(&format!("{:?}", plan.precision))?,
    )?;
    out.set_name_and_value(3, "nullable", scalar_logical(plan.nullable)?)?;
    out.set_name_and_value(4, "nested", scalar_logical(plan.nested)?)?;
    out.set_name_and_value(5, "lossless", scalar_logical(plan.lossless)?)?;
    out.set_name_and_value(
        6,
        "requires_explicit_cast",
        scalar_logical(plan.requires_explicit_cast)?,
    )?;
    out.set_name_and_value(
        7,
        "extension_name",
        scalar_string(plan.extension_name.as_deref().unwrap_or(""))?,
    )?;
    out.set_name_and_value(
        8,
        "note",
        scalar_string(plan.note.as_deref().unwrap_or(""))?,
    )?;

    Ok(out.into())
}

fn supported_codec_names() -> &'static [&'static str] {
    &[
        "bytes",
        "endian",
        "gzip",
        "zstd",
        "crc32c",
        "sharding_indexed",
        "sharding-indexed",
        "transpose",
        #[cfg(feature = "blosc")]
        "blosc",
    ]
}

fn known_codec_names() -> &'static [&'static str] {
    &[
        "bytes",
        "endian",
        "gzip",
        "zstd",
        "crc32c",
        "sharding_indexed",
        "sharding-indexed",
        "transpose",
        "blosc",
    ]
}

fn codec_supported(name: &str) -> bool {
    supported_codec_names().iter().any(|codec| *codec == name)
}

fn collect_codec_names_from_value(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map.iter() {
                if key == "codecs" {
                    collect_codec_names_from_codec_array(child, out);
                } else if key == "configuration" {
                    collect_codec_names_from_value(child, out);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                collect_codec_names_from_value(child, out);
            }
        }
        _ => {}
    }
}

fn collect_codec_names_from_codec_array(value: &serde_json::Value, out: &mut Vec<String>) {
    if let serde_json::Value::Array(codecs) = value {
        for codec in codecs {
            if let serde_json::Value::Object(map) = codec {
                if let Some(name) = map.get("name").and_then(|name| name.as_str()) {
                    out.push(name.to_string());
                }
                if let Some(configuration) = map.get("configuration") {
                    collect_codec_names_from_value(configuration, out);
                }
            }
        }
    }
}

fn string_vec_to_sexp(values: &[String]) -> savvy::Result<OwnedStringSexp> {
    let mut out = OwnedStringSexp::new(values.len())?;
    for (i, value) in values.iter().enumerate() {
        out.set_elt(i, value)?;
    }
    Ok(out)
}

fn codec_capabilities_to_sexp(codec_names: &[String]) -> savvy::Result<savvy::Sexp> {
    let mut supported = OwnedLogicalSexp::new(codec_names.len())?;
    for (i, codec) in codec_names.iter().enumerate() {
        supported.set_elt(i, codec_supported(codec))?;
    }
    let mut out = OwnedListSexp::new(2, true)?;
    out.set_name_and_value(0, "codec", string_vec_to_sexp(codec_names)?)?;
    out.set_name_and_value(1, "supported", supported)?;
    Ok(out.into())
}

#[savvy]
fn rzarrs_codec_capabilities() -> savvy::Result<savvy::Sexp> {
    let codec_names = known_codec_names()
        .iter()
        .map(|codec| codec.to_string())
        .collect::<Vec<_>>();
    codec_capabilities_to_sexp(&codec_names)
}

#[savvy]
fn rzarrs_dtype_plan(dtype: &str) -> savvy::Result<savvy::Sexp> {
    dtype_plan_to_sexp(dtype)
}

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

fn u64_dim_to_i32(value: u64, label: &str) -> savvy::Result<i32> {
    if value > i32::MAX as u64 {
        return Err(savvy::Error::new(&format!(
            "{label} dimension length {value} exceeds R's i32 array-dim limit"
        )));
    }
    Ok(value as i32)
}

fn usize_len_to_i32(value: usize, label: &str) -> savvy::Result<i32> {
    if value > i32::MAX as usize {
        return Err(savvy::Error::new(&format!(
            "{label} length {value} exceeds R's i32 vector/dim limit"
        )));
    }
    Ok(value as i32)
}

fn shape_to_i32_dims(shape: &[u64]) -> savvy::Result<Vec<i32>> {
    shape
        .iter()
        .enumerate()
        .map(|(i, &d)| u64_dim_to_i32(d, &format!("dimension {}", i + 1)))
        .collect()
}

const F64_SAFE_INTEGER_MAX_U64: u64 = 9_007_199_254_740_992; // 2^53

fn ensure_u64_exact_as_r_double(value: u64, dtype: &str) -> savvy::Result<()> {
    if value > F64_SAFE_INTEGER_MAX_U64 {
        return Err(savvy::Error::new(&format!(
            "{dtype} value {value} cannot be represented exactly as an R double; use an explicit uint64/string/extension-vector policy"
        )));
    }
    Ok(())
}

fn scalar_real(value: f64) -> savvy::Result<OwnedRealSexp> {
    let mut out = OwnedRealSexp::new(1)?;
    out[0] = value;
    Ok(out)
}

fn i64_to_bitpattern_real(data: &[i64]) -> savvy::Result<OwnedRealSexp> {
    let mut out = OwnedRealSexp::new(data.len())?;
    for (dst, &value) in out.as_mut_slice().iter_mut().zip(data.iter()) {
        *dst = f64::from_bits(value as u64);
    }
    Ok(out)
}

fn u64_to_bitpattern_real(data: &[u64]) -> savvy::Result<OwnedRealSexp> {
    let mut out = OwnedRealSexp::new(data.len())?;
    for (dst, &value) in out.as_mut_slice().iter_mut().zip(data.iter()) {
        *dst = f64::from_bits(value);
    }
    Ok(out)
}

fn i64_to_rzarrs_int64(data: &[i64]) -> savvy::Result<OwnedRealSexp> {
    let mut out = i64_to_bitpattern_real(data)?;
    out.set_class(["Rzarrs_int64"])?;
    out.set_attrib("storage", scalar_string("i64-bitpattern")?.into())?;
    Ok(out)
}

fn u64_to_rzarrs_uint64(data: &[u64]) -> savvy::Result<OwnedRealSexp> {
    let mut out = u64_to_bitpattern_real(data)?;
    out.set_class(["Rzarrs_uint64"])?;
    out.set_attrib("storage", scalar_string("u64-bitpattern")?.into())?;
    Ok(out)
}

fn set_time64_attrs(
    out: &mut OwnedRealSexp,
    dtype_name: &str,
    unit: NumpyTimeUnit,
    scale_factor: u32,
) -> savvy::Result<()> {
    out.set_class(["Rzarrs_int64"])?;
    out.set_attrib("zarr_dtype", scalar_string(dtype_name)?.into())?;
    out.set_attrib("unit", scalar_string(&unit.to_string())?.into())?;
    out.set_attrib("scale_factor", scalar_real(f64::from(scale_factor))?.into())?;
    out.set_attrib("storage", scalar_string("i64-bitpattern")?.into())?;
    Ok(())
}

const F64_SAFE_INTEGER_MIN_I64: i64 = -9_007_199_254_740_992; // -2^53
const F64_SAFE_INTEGER_MAX_I64: i64 = 9_007_199_254_740_992; // 2^53

fn ensure_i64_exact_as_r_double(value: i64, dtype: &str) -> savvy::Result<()> {
    if !(F64_SAFE_INTEGER_MIN_I64..=F64_SAFE_INTEGER_MAX_I64).contains(&value) {
        return Err(savvy::Error::new(&format!(
            "{dtype} value {value} cannot be represented exactly as an R double"
        )));
    }
    Ok(())
}

fn has_class(x: &savvy::Sexp, class_name: &str) -> bool {
    x.get_class()
        .map(|classes| classes.iter().any(|class| *class == class_name))
        .unwrap_or(false)
}

fn string_attr_first(x: &savvy::Sexp, attr: &str) -> savvy::Result<Option<String>> {
    let Some(attr_value) = x.get_attrib(attr)? else {
        return Ok(None);
    };
    match attr_value.into_typed() {
        TypedSexp::String(v) => Ok(v.iter().next().map(|value| value.to_string())),
        _ => Ok(None),
    }
}

fn is_time64_sexp(x: &savvy::Sexp) -> savvy::Result<bool> {
    Ok(matches!(
        string_attr_first(x, "zarr_dtype")?.as_deref(),
        Some("numpy.datetime64") | Some("numpy.timedelta64")
    ))
}

fn dims_match_n(x_dims: Option<Vec<i32>>, y_dims: Option<Vec<i32>>, n: usize) -> Option<Vec<i32>> {
    if let Some(dims) = x_dims {
        if dims.iter().map(|&v| v as usize).product::<usize>() == n {
            return Some(dims);
        }
    }
    if let Some(dims) = y_dims {
        if dims.iter().map(|&v| v as usize).product::<usize>() == n {
            return Some(dims);
        }
    }
    None
}

fn check_recyclable(x_len: usize, y_len: usize, type_name: &str) -> savvy::Result<usize> {
    let n = x_len.max(y_len);
    if n == 0 {
        return Ok(0);
    }
    if x_len == 0 || y_len == 0 {
        return Err(savvy::Error::new(&format!(
            "cannot operate on zero-length and nonzero-length {type_name} vectors"
        )));
    }
    Ok(n)
}

fn check_numeric_i64(x: f64, label: &str) -> savvy::Result<i64> {
    if x.is_na() || !x.is_finite() {
        return Err(savvy::Error::new(&format!(
            "{label} cannot contain NA/NaN/Inf"
        )));
    }
    if x.fract() != 0.0 {
        return Err(savvy::Error::new(&format!(
            "{label} must contain whole-number values"
        )));
    }
    if x < F64_SAFE_INTEGER_MIN_I64 as f64 || x > F64_SAFE_INTEGER_MAX_I64 as f64 {
        return Err(savvy::Error::new(&format!(
            "{label} double value {x} is outside double's exact integer range; construct an Rzarrs_int64 vector explicitly"
        )));
    }
    Ok(x as i64)
}

fn check_numeric_u64(x: f64, label: &str) -> savvy::Result<u64> {
    if x.is_na() || !x.is_finite() {
        return Err(savvy::Error::new(&format!(
            "{label} cannot contain NA/NaN/Inf"
        )));
    }
    if x.fract() != 0.0 {
        return Err(savvy::Error::new(&format!(
            "{label} must contain whole-number values"
        )));
    }
    if x < 0.0 || x > F64_SAFE_INTEGER_MAX_U64 as f64 {
        return Err(savvy::Error::new(&format!(
            "{label} double value {x} is outside double's exact nonnegative integer range; construct an Rzarrs_uint64 vector explicitly"
        )));
    }
    Ok(x as u64)
}

fn collect_i64_operand(x: savvy::Sexp, label: &str) -> savvy::Result<(Vec<i64>, bool)> {
    if has_class(&x, "Rzarrs_uint64") {
        return Err(savvy::Error::new(&format!(
            "{label} is Rzarrs_uint64; use uint64 operations"
        )));
    }
    let is_class = has_class(&x, "Rzarrs_int64");
    let is_time = is_time64_sexp(&x)?;
    match x.into_typed() {
        TypedSexp::Real(v) if is_class => Ok((
            v.as_slice()
                .iter()
                .map(|src| src.to_bits() as i64)
                .collect(),
            is_time,
        )),
        TypedSexp::Integer(v) => {
            let mut out = Vec::with_capacity(v.len());
            for &value in v.iter() {
                if value.is_na() {
                    return Err(savvy::Error::new(&format!("{label} cannot contain NA")));
                }
                out.push(value as i64);
            }
            Ok((out, false))
        }
        TypedSexp::Real(v) => {
            let mut out = Vec::with_capacity(v.len());
            for &value in v.iter() {
                out.push(check_numeric_i64(value, label)?);
            }
            Ok((out, false))
        }
        _ => Err(savvy::Error::new(&format!(
            "{label} must be Rzarrs_int64, integer, or exactly representable double"
        ))),
    }
}

fn collect_u64_operand(x: savvy::Sexp, label: &str) -> savvy::Result<Vec<u64>> {
    if has_class(&x, "Rzarrs_int64") {
        return Err(savvy::Error::new(&format!(
            "{label} is Rzarrs_int64; use int64 operations"
        )));
    }
    let is_class = has_class(&x, "Rzarrs_uint64");
    match x.into_typed() {
        TypedSexp::Real(v) if is_class => {
            Ok(v.as_slice().iter().map(|src| src.to_bits()).collect())
        }
        TypedSexp::Integer(v) => {
            let mut out = Vec::with_capacity(v.len());
            for &value in v.iter() {
                if value.is_na() {
                    return Err(savvy::Error::new(&format!("{label} cannot contain NA")));
                }
                if value < 0 {
                    return Err(savvy::Error::new(&format!(
                        "{label} cannot contain negative values for uint64 operations"
                    )));
                }
                out.push(value as u64);
            }
            Ok(out)
        }
        TypedSexp::Real(v) => {
            let mut out = Vec::with_capacity(v.len());
            for &value in v.iter() {
                out.push(check_numeric_u64(value, label)?);
            }
            Ok(out)
        }
        _ => Err(savvy::Error::new(&format!(
            "{label} must be Rzarrs_uint64, nonnegative integer, or exactly representable nonnegative double"
        ))),
    }
}

fn compare_i64(a: i64, b: i64, op: &str) -> savvy::Result<bool> {
    match op {
        "==" => Ok(a == b),
        "!=" => Ok(a != b),
        "<" => Ok(a < b),
        "<=" => Ok(a <= b),
        ">" => Ok(a > b),
        ">=" => Ok(a >= b),
        _ => Err(savvy::Error::new(&format!(
            "unsupported int64 comparison '{op}'"
        ))),
    }
}

fn compare_u64(a: u64, b: u64, op: &str) -> savvy::Result<bool> {
    match op {
        "==" => Ok(a == b),
        "!=" => Ok(a != b),
        "<" => Ok(a < b),
        "<=" => Ok(a <= b),
        ">" => Ok(a > b),
        ">=" => Ok(a >= b),
        _ => Err(savvy::Error::new(&format!(
            "unsupported uint64 comparison '{op}'"
        ))),
    }
}

fn checked_i64_arithmetic(a: i64, b: i64, op: &str) -> savvy::Result<i64> {
    let result = match op {
        "+" => IBig::from(a) + IBig::from(b),
        "-" => IBig::from(a) - IBig::from(b),
        "*" => IBig::from(a) * IBig::from(b),
        _ => {
            return Err(savvy::Error::new(&format!(
                "operation '{op}' is not integer-preserving or not implemented for Rzarrs_int64"
            )));
        }
    };
    i64::try_from(&result).map_err(|_| {
        savvy::Error::new(&format!(
            "operation '{op}' overflows signed 64-bit range; explicit wider integer materialization is required"
        ))
    })
}

fn checked_u64_arithmetic(a: u64, b: u64, op: &str) -> savvy::Result<u64> {
    let result = match op {
        "+" => UBig::from(a) + UBig::from(b),
        "-" => {
            if a < b {
                return Err(savvy::Error::new(
                    "operation '-' would produce a negative value for Rzarrs_uint64",
                ));
            }
            UBig::from(a) - UBig::from(b)
        }
        "*" => UBig::from(a) * UBig::from(b),
        _ => {
            return Err(savvy::Error::new(&format!(
                "operation '{op}' is not integer-preserving or not implemented for Rzarrs_uint64"
            )));
        }
    };
    u64::try_from(&result).map_err(|_| {
        savvy::Error::new(&format!(
            "operation '{op}' overflows unsigned 64-bit range; explicit wider integer materialization is required"
        ))
    })
}

fn i64_bitpattern_to_strings(data: &[f64], time_na: bool) -> savvy::Result<OwnedStringSexp> {
    let mut out = OwnedStringSexp::new(data.len())?;
    for (i, &src) in data.iter().enumerate() {
        let value = src.to_bits() as i64;
        if time_na && value == i64::MIN {
            out.set_na(i)?;
        } else {
            out.set_elt(i, &IBig::from(value).to_string())?;
        }
    }
    Ok(out)
}

fn uint64_bitpattern_to_strings(data: &[f64]) -> savvy::Result<OwnedStringSexp> {
    let mut out = OwnedStringSexp::new(data.len())?;
    for (i, &src) in data.iter().enumerate() {
        out.set_elt(i, &UBig::from(src.to_bits()).to_string())?;
    }
    Ok(out)
}

fn i64_bitpattern_to_double(data: &[f64], time_na: bool) -> savvy::Result<OwnedRealSexp> {
    let mut out = OwnedRealSexp::new(data.len())?;
    for (i, &src) in data.iter().enumerate() {
        let value = src.to_bits() as i64;
        if time_na && value == i64::MIN {
            out.set_na(i)?;
        } else {
            ensure_i64_exact_as_r_double(value, "int64")?;
            out[i] = value as f64;
        }
    }
    Ok(out)
}

fn uint64_bitpattern_to_double(data: &[f64]) -> savvy::Result<OwnedRealSexp> {
    let mut out = OwnedRealSexp::new(data.len())?;
    for (i, &src) in data.iter().enumerate() {
        let value = src.to_bits();
        ensure_u64_exact_as_r_double(value, "uint64")?;
        out[i] = value as f64;
    }
    Ok(out)
}

#[savvy]
fn rzarrs_int64_values(x: savvy::Sexp) -> savvy::Result<savvy::Sexp> {
    let dims = x.get_dim().map(|d| d.to_vec());
    let time_na = is_time64_sexp(&x)?;
    let mut out = match x.into_typed() {
        TypedSexp::Real(v) => i64_bitpattern_to_strings(v.as_slice(), time_na)?,
        _ => return Err(savvy::Error::new("expected a Rzarrs_int64 vector")),
    };
    if let Some(dims) = dims {
        out.set_dim(&dims)?;
    }
    Ok(out.into())
}

#[savvy]
fn rzarrs_uint64_values(x: savvy::Sexp) -> savvy::Result<savvy::Sexp> {
    let dims = x.get_dim().map(|d| d.to_vec());
    let mut out = match x.into_typed() {
        TypedSexp::Real(v) => uint64_bitpattern_to_strings(v.as_slice())?,
        _ => return Err(savvy::Error::new("expected a Rzarrs_uint64 vector")),
    };
    if let Some(dims) = dims {
        out.set_dim(&dims)?;
    }
    Ok(out.into())
}

#[savvy]
fn rzarrs_int64_to_double(x: savvy::Sexp) -> savvy::Result<savvy::Sexp> {
    let dims = x.get_dim().map(|d| d.to_vec());
    let time_na = is_time64_sexp(&x)?;
    let mut out = match x.into_typed() {
        TypedSexp::Real(v) => i64_bitpattern_to_double(v.as_slice(), time_na)?,
        _ => return Err(savvy::Error::new("expected a Rzarrs_int64 vector")),
    };
    if let Some(dims) = dims {
        out.set_dim(&dims)?;
    }
    Ok(out.into())
}

#[savvy]
fn rzarrs_uint64_to_double(x: savvy::Sexp) -> savvy::Result<savvy::Sexp> {
    let dims = x.get_dim().map(|d| d.to_vec());
    let mut out = match x.into_typed() {
        TypedSexp::Real(v) => uint64_bitpattern_to_double(v.as_slice())?,
        _ => return Err(savvy::Error::new("expected a Rzarrs_uint64 vector")),
    };
    if let Some(dims) = dims {
        out.set_dim(&dims)?;
    }
    Ok(out.into())
}

#[savvy]
fn rzarrs_int64_is_na(x: savvy::Sexp) -> savvy::Result<savvy::Sexp> {
    let dims = x.get_dim().map(|d| d.to_vec());
    let time_na = is_time64_sexp(&x)?;
    let data = match x.into_typed() {
        TypedSexp::Real(v) => v
            .as_slice()
            .iter()
            .map(|src| src.to_bits() as i64)
            .collect::<Vec<_>>(),
        _ => return Err(savvy::Error::new("expected a Rzarrs_int64 vector")),
    };
    let mut out = OwnedLogicalSexp::new(data.len())?;
    for (i, value) in data.iter().enumerate() {
        out.set_elt(i, time_na && *value == i64::MIN)?;
    }
    if let Some(dims) = dims {
        out.set_dim(&dims)?;
    }
    Ok(out.into())
}

#[savvy]
fn rzarrs_uint64_is_na(x: savvy::Sexp) -> savvy::Result<savvy::Sexp> {
    let dims = x.get_dim().map(|d| d.to_vec());
    let len = match x.into_typed() {
        TypedSexp::Real(v) => v.len(),
        _ => return Err(savvy::Error::new("expected a Rzarrs_uint64 vector")),
    };
    let mut out = OwnedLogicalSexp::new(len)?;
    for i in 0..len {
        out.set_elt(i, false)?;
    }
    if let Some(dims) = dims {
        out.set_dim(&dims)?;
    }
    Ok(out.into())
}

#[savvy]
fn rzarrs_int64_op(x: savvy::Sexp, y: savvy::Sexp, op: &str) -> savvy::Result<savvy::Sexp> {
    let comparison = matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=");
    let x_dims = x.get_dim().map(|d| d.to_vec());
    let y_dims = y.get_dim().map(|d| d.to_vec());
    let (x_values, x_time) = collect_i64_operand(x, "left operand")?;
    let (y_values, y_time) = collect_i64_operand(y, "right operand")?;
    let n = check_recyclable(x_values.len(), y_values.len(), "int64")?;
    if n == 0 {
        return if comparison {
            Ok(OwnedLogicalSexp::new(0)?.into())
        } else {
            Ok(i64_to_rzarrs_int64(&[])?.into())
        };
    }

    if comparison {
        let mut out = OwnedLogicalSexp::new(n)?;
        for i in 0..n {
            let a = x_values[i % x_values.len()];
            let b = y_values[i % y_values.len()];
            if (x_time && a == i64::MIN) || (y_time && b == i64::MIN) {
                out.set_na(i)?;
            } else {
                out.set_elt(i, compare_i64(a, b, op)?)?;
            }
        }
        if let Some(dims) = dims_match_n(x_dims.clone(), y_dims.clone(), n) {
            out.set_dim(&dims)?;
        }
        return Ok(out.into());
    }

    if x_time || y_time {
        return Err(savvy::Error::new(
            "arithmetic on numpy datetime64/timedelta64 int64 payloads is not implemented; scale explicitly first",
        ));
    }

    let mut result = Vec::with_capacity(n);
    for i in 0..n {
        result.push(checked_i64_arithmetic(
            x_values[i % x_values.len()],
            y_values[i % y_values.len()],
            op,
        )?);
    }
    let mut out = i64_to_rzarrs_int64(&result)?;
    if let Some(dims) = dims_match_n(x_dims, y_dims, n) {
        out.set_dim(&dims)?;
    }
    Ok(out.into())
}

#[savvy]
fn rzarrs_uint64_op(x: savvy::Sexp, y: savvy::Sexp, op: &str) -> savvy::Result<savvy::Sexp> {
    let comparison = matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=");
    let x_dims = x.get_dim().map(|d| d.to_vec());
    let y_dims = y.get_dim().map(|d| d.to_vec());
    let x_values = collect_u64_operand(x, "left operand")?;
    let y_values = collect_u64_operand(y, "right operand")?;
    let n = check_recyclable(x_values.len(), y_values.len(), "uint64")?;
    if n == 0 {
        return if comparison {
            Ok(OwnedLogicalSexp::new(0)?.into())
        } else {
            Ok(u64_to_rzarrs_uint64(&[])?.into())
        };
    }

    if comparison {
        let mut out = OwnedLogicalSexp::new(n)?;
        for i in 0..n {
            out.set_elt(
                i,
                compare_u64(
                    x_values[i % x_values.len()],
                    y_values[i % y_values.len()],
                    op,
                )?,
            )?;
        }
        if let Some(dims) = dims_match_n(x_dims.clone(), y_dims.clone(), n) {
            out.set_dim(&dims)?;
        }
        return Ok(out.into());
    }

    let mut result = Vec::with_capacity(n);
    for i in 0..n {
        result.push(checked_u64_arithmetic(
            x_values[i % x_values.len()],
            y_values[i % y_values.len()],
            op,
        )?);
    }
    let mut out = u64_to_rzarrs_uint64(&result)?;
    if let Some(dims) = dims_match_n(x_dims, y_dims, n) {
        out.set_dim(&dims)?;
    }
    Ok(out.into())
}

#[savvy]
fn rzarrs_int64_summary(x: savvy::Sexp, op: &str, _na_rm: bool) -> savvy::Result<savvy::Sexp> {
    if is_time64_sexp(&x)? {
        return Err(savvy::Error::new(
            "Summary is not implemented for numpy datetime64/timedelta64 int64 payloads",
        ));
    }
    let (values, _) = collect_i64_operand(x, "x")?;
    match op {
        "sum" => {
            let mut acc = IBig::from(0);
            for value in values {
                acc += IBig::from(value);
            }
            let value = i64::try_from(&acc).map_err(|_| {
                savvy::Error::new(
                    "sum overflows signed 64-bit range; explicit wider integer materialization is required",
                )
            })?;
            Ok(i64_to_rzarrs_int64(&[value])?.into())
        }
        "prod" => {
            let mut acc = IBig::from(1);
            for value in values {
                acc *= IBig::from(value);
            }
            let value = i64::try_from(&acc).map_err(|_| {
                savvy::Error::new(
                    "prod overflows signed 64-bit range; explicit wider integer materialization is required",
                )
            })?;
            Ok(i64_to_rzarrs_int64(&[value])?.into())
        }
        "min" | "max" | "range" => {
            if values.is_empty() {
                return Err(savvy::Error::new(&format!(
                    "{op} is undefined for empty Rzarrs_int64 vectors"
                )));
            }
            let min = *values.iter().min().unwrap();
            let max = *values.iter().max().unwrap();
            match op {
                "min" => Ok(i64_to_rzarrs_int64(&[min])?.into()),
                "max" => Ok(i64_to_rzarrs_int64(&[max])?.into()),
                _ => Ok(i64_to_rzarrs_int64(&[min, max])?.into()),
            }
        }
        _ => Err(savvy::Error::new(&format!(
            "Summary operation '{op}' is not implemented for Rzarrs_int64"
        ))),
    }
}

#[savvy]
fn rzarrs_uint64_summary(x: savvy::Sexp, op: &str, _na_rm: bool) -> savvy::Result<savvy::Sexp> {
    let values = collect_u64_operand(x, "x")?;
    match op {
        "sum" => {
            let mut acc = UBig::from(0u8);
            for value in values {
                acc += UBig::from(value);
            }
            let value = u64::try_from(&acc).map_err(|_| {
                savvy::Error::new(
                    "sum overflows unsigned 64-bit range; explicit wider integer materialization is required",
                )
            })?;
            Ok(u64_to_rzarrs_uint64(&[value])?.into())
        }
        "prod" => {
            let mut acc = UBig::from(1u8);
            for value in values {
                acc *= UBig::from(value);
            }
            let value = u64::try_from(&acc).map_err(|_| {
                savvy::Error::new(
                    "prod overflows unsigned 64-bit range; explicit wider integer materialization is required",
                )
            })?;
            Ok(u64_to_rzarrs_uint64(&[value])?.into())
        }
        "min" | "max" | "range" => {
            if values.is_empty() {
                return Err(savvy::Error::new(&format!(
                    "{op} is undefined for empty Rzarrs_uint64 vectors"
                )));
            }
            let min = *values.iter().min().unwrap();
            let max = *values.iter().max().unwrap();
            match op {
                "min" => Ok(u64_to_rzarrs_uint64(&[min])?.into()),
                "max" => Ok(u64_to_rzarrs_uint64(&[max])?.into()),
                _ => Ok(u64_to_rzarrs_uint64(&[min, max])?.into()),
            }
        }
        _ => Err(savvy::Error::new(&format!(
            "Summary operation '{op}' is not implemented for Rzarrs_uint64"
        ))),
    }
}

#[savvy]
fn rzarrs_int64_math(x: savvy::Sexp, op: &str) -> savvy::Result<savvy::Sexp> {
    if is_time64_sexp(&x)? {
        return Err(savvy::Error::new(
            "Math is not implemented for numpy datetime64/timedelta64 int64 payloads",
        ));
    }
    let (values, _) = collect_i64_operand(x, "x")?;
    match op {
        "abs" => {
            let mut out = Vec::with_capacity(values.len());
            for value in values {
                let abs = if value < 0 {
                    -IBig::from(value)
                } else {
                    IBig::from(value)
                };
                out.push(i64::try_from(&abs).map_err(|_| {
                    savvy::Error::new("abs overflows signed 64-bit range for -9223372036854775808")
                })?);
            }
            Ok(i64_to_rzarrs_int64(&out)?.into())
        }
        "sign" => {
            let mut out = OwnedIntegerSexp::new(values.len())?;
            for (i, value) in values.iter().enumerate() {
                out[i] = if *value > 0 {
                    1
                } else if *value < 0 {
                    -1
                } else {
                    0
                };
            }
            Ok(out.into())
        }
        _ => Err(savvy::Error::new(&format!(
            "Math operation '{op}' is not integer-preserving or not implemented for Rzarrs_int64"
        ))),
    }
}

#[savvy]
fn rzarrs_uint64_math(x: savvy::Sexp, op: &str) -> savvy::Result<savvy::Sexp> {
    let values = collect_u64_operand(x, "x")?;
    match op {
        "abs" => Ok(u64_to_rzarrs_uint64(&values)?.into()),
        "sign" => {
            let mut out = OwnedIntegerSexp::new(values.len())?;
            for (i, value) in values.iter().enumerate() {
                out[i] = if *value == 0 { 0 } else { 1 };
            }
            Ok(out.into())
        }
        _ => Err(savvy::Error::new(&format!(
            "Math operation '{op}' is not integer-preserving or not implemented for Rzarrs_uint64"
        ))),
    }
}

fn retrieve_numpy_datetime64(
    array: &Array<dyn ReadableStorageTraits>,
    subset: &ArraySubset,
    dims: &[i32],
) -> savvy::Result<savvy::Sexp> {
    let dt = array
        .data_type()
        .downcast_ref::<NumpyDateTime64DataType>()
        .ok_or_else(|| savvy::Error::new("internal error: expected numpy.datetime64 dtype"))?;
    let data: Vec<i64> = array
        .retrieve_array_subset::<Vec<i64>>(subset)
        .map_err(|e| savvy::Error::new(&e.to_string()))?;
    let mut out = unsafe { OwnedRealSexp::new_without_init(data.len())? };
    map_c_to_r_order(&data, dims, out.as_mut_slice(), |&value| {
        f64::from_bits(value as u64)
    });
    set_time64_attrs(&mut out, "numpy.datetime64", dt.unit, dt.scale_factor.get())?;
    Ok(out.into())
}

fn retrieve_numpy_timedelta64(
    array: &Array<dyn ReadableStorageTraits>,
    subset: &ArraySubset,
    dims: &[i32],
) -> savvy::Result<savvy::Sexp> {
    let dt = array
        .data_type()
        .downcast_ref::<NumpyTimeDelta64DataType>()
        .ok_or_else(|| savvy::Error::new("internal error: expected numpy.timedelta64 dtype"))?;
    let data: Vec<i64> = array
        .retrieve_array_subset::<Vec<i64>>(subset)
        .map_err(|e| savvy::Error::new(&e.to_string()))?;
    let mut out = unsafe { OwnedRealSexp::new_without_init(data.len())? };
    map_c_to_r_order(&data, dims, out.as_mut_slice(), |&value| {
        f64::from_bits(value as u64)
    });
    set_time64_attrs(
        &mut out,
        "numpy.timedelta64",
        dt.unit,
        dt.scale_factor.get(),
    )?;
    Ok(out.into())
}

fn retrieve_typed(
    array: &Array<dyn ReadableStorageTraits>,
    subset: &ArraySubset,
    dtype: &str,
    dims: &[i32],
) -> savvy::Result<savvy::Sexp> {
    let n: usize = subset.num_elements() as usize;

    match dtype {
        "float16" | "f16" => {
            let data: Vec<f16> = array
                .retrieve_array_subset::<Vec<f16>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = unsafe { OwnedRealSexp::new_without_init(n)? };
            map_c_to_r_order(&data, dims, out.as_mut_slice(), |&value| f64::from(value));
            Ok(out.into())
        }

        "bfloat16" | "bf16" | "b16" => {
            let data: Vec<bf16> = array
                .retrieve_array_subset::<Vec<bf16>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = unsafe { OwnedRealSexp::new_without_init(n)? };
            map_c_to_r_order(&data, dims, out.as_mut_slice(), |&value| f64::from(value));
            Ok(out.into())
        }

        "float32" => {
            let data: Vec<f32> = array
                .retrieve_array_subset::<Vec<f32>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = unsafe { OwnedRealSexp::new_without_init(n)? };
            map_c_to_r_order(&data, dims, out.as_mut_slice(), |&value| value as f64);
            Ok(out.into())
        }

        "float64" => {
            let data: Vec<f64> = array
                .retrieve_array_subset::<Vec<f64>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = unsafe { OwnedRealSexp::new_without_init(n)? };
            map_c_to_r_order(&data, dims, out.as_mut_slice(), |&value| value);
            Ok(out.into())
        }

        "int8" => {
            let data: Vec<i8> = array
                .retrieve_array_subset::<Vec<i8>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = unsafe { OwnedIntegerSexp::new_without_init(n)? };
            map_c_to_r_order(&data, dims, out.as_mut_slice(), |&value| value as i32);
            Ok(out.into())
        }

        "int16" => {
            let data: Vec<i16> = array
                .retrieve_array_subset::<Vec<i16>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = unsafe { OwnedIntegerSexp::new_without_init(n)? };
            map_c_to_r_order(&data, dims, out.as_mut_slice(), |&value| value as i32);
            Ok(out.into())
        }

        "int32" => {
            let data: Vec<i32> = array
                .retrieve_array_subset::<Vec<i32>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = unsafe { OwnedIntegerSexp::new_without_init(n)? };
            // R's NA_INTEGER is the i32::MIN bit pattern, so the permutation can
            // copy all values directly into the final R allocation.
            copy_c_to_r_order(&data, dims, out.as_mut_slice());
            Ok(out.into())
        }

        "int64" => {
            let data: Vec<i64> = array
                .retrieve_array_subset::<Vec<i64>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = unsafe { OwnedRealSexp::new_without_init(n)? };
            map_c_to_r_order(&data, dims, out.as_mut_slice(), |&value| {
                f64::from_bits(value as u64)
            });
            out.set_class(["Rzarrs_int64"])?;
            out.set_attrib("storage", scalar_string("i64-bitpattern")?.into())?;
            Ok(out.into())
        }

        "uint8" => {
            let data: Vec<u8> = array
                .retrieve_array_subset::<Vec<u8>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = unsafe { OwnedIntegerSexp::new_without_init(n)? };
            map_c_to_r_order(&data, dims, out.as_mut_slice(), |&value| value as i32);
            Ok(out.into())
        }

        "uint16" => {
            let data: Vec<u16> = array
                .retrieve_array_subset::<Vec<u16>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = unsafe { OwnedIntegerSexp::new_without_init(n)? };
            map_c_to_r_order(&data, dims, out.as_mut_slice(), |&value| value as i32);
            Ok(out.into())
        }

        "uint32" => {
            let data: Vec<u32> = array
                .retrieve_array_subset::<Vec<u32>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = unsafe { OwnedRealSexp::new_without_init(n)? };
            map_c_to_r_order(&data, dims, out.as_mut_slice(), |&value| value as f64);
            Ok(out.into())
        }

        "uint64" => {
            let data: Vec<u64> = array
                .retrieve_array_subset::<Vec<u64>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = unsafe { OwnedRealSexp::new_without_init(n)? };
            map_c_to_r_order(&data, dims, out.as_mut_slice(), |&value| {
                f64::from_bits(value)
            });
            out.set_class(["Rzarrs_uint64"])?;
            out.set_attrib("storage", scalar_string("u64-bitpattern")?.into())?;
            Ok(out.into())
        }

        "complex64" => {
            let data: Vec<Complex32> = array
                .retrieve_array_subset::<Vec<Complex32>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = unsafe { OwnedComplexSexp::new_without_init(n)? };
            map_c_to_r_order(&data, dims, out.as_mut_slice(), |value| RComplex64 {
                re: f64::from(value.re),
                im: f64::from(value.im),
            });
            out.set_class(["Rzarrs_complex64", "complex"])?;
            Ok(out.into())
        }

        "complex128" => {
            let data: Vec<NumComplex64> = array
                .retrieve_array_subset::<Vec<NumComplex64>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = unsafe { OwnedComplexSexp::new_without_init(n)? };
            map_c_to_r_order(&data, dims, out.as_mut_slice(), |value| RComplex64 {
                re: value.re,
                im: value.im,
            });
            out.set_class(["Rzarrs_complex128", "complex"])?;
            Ok(out.into())
        }

        "bool" => {
            let data: Vec<bool> = array
                .retrieve_array_subset::<Vec<bool>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = unsafe { OwnedLogicalSexp::new_without_init(n)? };
            try_for_each_c_to_r_order(&data, dims, |index, &value| out.set_elt(index, value))?;
            Ok(out.into())
        }

        "string" => {
            let data: Vec<String> = array
                .retrieve_array_subset::<Vec<String>>(subset)
                .map_err(|e| savvy::Error::new(&e.to_string()))?;
            let mut out = OwnedStringSexp::new(n)?;
            try_for_each_c_to_r_order(&data, dims, |index, value| {
                out.set_elt(index, value.as_str())
            })?;
            Ok(out.into())
        }

        "numpy.datetime64" => retrieve_numpy_datetime64(array, subset, dims),

        "numpy.timedelta64" => retrieve_numpy_timedelta64(array, subset, dims),

        other => {
            let plan = nested::plan_dtype(other, nested::Integer64Policy::Int64Class);
            Err(savvy::Error::new(&format!(
                "dtype '{other}' is not yet materialized by Rzarrs; planned r_type={:?}, precision={:?}, requires_explicit_cast={}, note={}",
                plan.r_type,
                plan.precision,
                plan.requires_explicit_cast,
                plan.note.as_deref().unwrap_or("none")
            )))
        }
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

/// High-level VCF Zarr reader
///
/// `ZarrVcf$open(x)` opens a VCF Zarr store (spec versions 0.1–0.4) and
/// returns an instance with methods to access variant, sample, and genotype data.
/// Genotypes use the VCF Zarr integer encoding: allele indexes are 0-based,
/// `-1` is a missing allele (`.` in VCF), and `-2` is the array fill sentinel.
///
/// @section Methods:
/// \describe{
///   \item{`$open(x)`}{Open a VCF Zarr store from a path, URL, `ZarrStore`, or `ZarrObjectStore`.}
///   \item{`$version()`}{VCF Zarr spec version string.}
///   \item{`$n_variants()`, `$n_samples()`}{Number of variants and samples.}
///   \item{`$samples()`, `$contigs()`, `$filters()`}{Character vectors of sample IDs, contig names, filter IDs.}
///   \item{`$fields()`}{Available array names.}
///   \item{`$variant_position()`, `$variant_contig()`, `$variant_allele()`}{Per-variant data.}
///   \item{`$genotypes(variants, samples)`}{3-D array (variants × samples × ploidy) of integer genotypes using VCF Zarr `-1`/`-2` sentinel values.}
///   \item{`$call_genotype_phased(variants, samples)`}{Boolean phased matrix.}
///   \item{`$variant(name)`, `$call(name)`}{Generic accessor for `variant_<name>` / `call_<name>` arrays.}
/// }
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

const VCF_KNOWN_ARRAYS: &[&str] = &[
    "variant_contig",
    "variant_position",
    "variant_allele",
    "call_genotype",
    "call_genotype_phased",
    "sample_id",
    "contig_id",
    "filter_id",
    "filter_description",
];

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

    let mut array_names: Vec<String> = match group.children(false) {
        Ok(children) => children
            .iter()
            .filter(|n| matches!(n.metadata(), NodeMetadata::Array(_)))
            .map(|n| path_to_name(n.path().as_str()).to_string())
            .collect(),
        // Some backends (notably plain HTTP without directory listing) can read
        // known Zarr keys but cannot list group children.  VCF Zarr has a small
        // standard field set, so probe those arrays and keep the reader usable
        // on any readable storage backend.
        Err(_) => Vec::new(),
    };
    let readable_storage: Arc<dyn ReadableStorageTraits> = store.clone();
    for name in VCF_KNOWN_ARRAYS {
        if !array_names.iter().any(|field| field == name)
            && Array::open(readable_storage.clone(), format!("/{name}").as_str()).is_ok()
        {
            array_names.push((*name).to_string());
        }
    }

    // n_variants from variant_position array shape
    let n_variants = if array_names.contains(&"variant_position".to_string()) {
        let storage: Arc<dyn ReadableStorageTraits> = store.clone();
        if let Ok(arr) = Array::open(storage, "/variant_position") {
            let shape = arr.shape();
            if !shape.is_empty() {
                u64_dim_to_i32(shape[0], "variant")?
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
    let n_samples = usize_len_to_i32(samples.len(), "sample")?;

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

fn try_for_each_c_to_r_order<T, E, F>(data: &[T], dims: &[i32], mut write: F) -> Result<(), E>
where
    F: FnMut(usize, &T) -> Result<(), E>,
{
    let ndim = dims.len();
    if ndim <= 1 {
        for (index, value) in data.iter().enumerate() {
            write(index, value)?;
        }
        return Ok(());
    }

    let dims: Vec<usize> = dims.iter().map(|&dim| dim as usize).collect();
    if ndim == 2 {
        let (rows, cols) = (dims[0], dims[1]);
        const BLOCK: usize = 32;
        for row_start in (0..rows).step_by(BLOCK) {
            for col_start in (0..cols).step_by(BLOCK) {
                let row_end = (row_start + BLOCK).min(rows);
                let col_end = (col_start + BLOCK).min(cols);
                for row in row_start..row_end {
                    let source_offset = row * cols;
                    for col in col_start..col_end {
                        write(row + col * rows, &data[source_offset + col])?;
                    }
                }
            }
        }
        return Ok(());
    }

    let mut r_strides = vec![1usize; ndim];
    for axis in 1..ndim {
        r_strides[axis] = r_strides[axis - 1] * dims[axis - 1];
    }

    let mut coords = vec![0usize; ndim];
    let mut r_pos = 0usize;
    for value in data {
        write(r_pos, value)?;
        for axis in (0..ndim).rev() {
            coords[axis] += 1;
            r_pos += r_strides[axis];
            if coords[axis] < dims[axis] {
                break;
            }
            coords[axis] = 0;
            r_pos -= dims[axis] * r_strides[axis];
        }
    }
    Ok(())
}

fn map_c_to_r_order<T, U, F>(data: &[T], dims: &[i32], result: &mut [U], mut map: F)
where
    F: FnMut(&T) -> U,
{
    let outcome = try_for_each_c_to_r_order(data, dims, |index, value| {
        result[index] = map(value);
        Ok::<(), std::convert::Infallible>(())
    });
    match outcome {
        Ok(()) => {}
        Err(error) => match error {},
    }
}

fn copy_c_to_r_order<T: Clone>(data: &[T], dims: &[i32], result: &mut [T]) {
    map_c_to_r_order(data, dims, result, Clone::clone);
}

#[cfg(test)]
fn c_to_r_order<T: Clone>(data: &[T], dims: &[i32]) -> Vec<T> {
    if data.is_empty() {
        return Vec::new();
    }
    let mut result = vec![data[0].clone(); data.len()];
    copy_c_to_r_order(data, dims, &mut result);
    result
}

#[cfg(feature = "zip")]
fn open_local_zip_store(path: &str) -> savvy::Result<Arc<dyn ReadListStorage>> {
    let path = std::path::Path::new(path);
    let key = path
        .file_name()
        .and_then(|x| x.to_str())
        .ok_or_else(|| savvy::Error::new("invalid .zarr.zip path"))?;

    let root = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let fs_root = zarrs::filesystem::FilesystemStore::new(root).map_err(|e| {
        savvy::Error::new(&format!(
            "cannot open filesystem root for zip '{path:?}': {e}"
        ))
    })?;
    let storage_key = zarrs::storage::StoreKey::try_from(key)
        .map_err(|e| savvy::Error::new(&format!("invalid zip store key '{key}': {e}")))?;

    let zip_store = zarrs_zip::ZipStorageAdapter::new(Arc::new(fs_root), storage_key)
        .map_err(|e| savvy::Error::new(&format!("cannot open zip store '{key}': {e}")))?;

    Ok(Arc::new(zip_store))
}

#[cfg(not(feature = "zip"))]
fn open_local_zip_store(_path: &str) -> savvy::Result<Arc<dyn ReadListStorage>> {
    Err(savvy::Error::new(
        "local .zarr.zip support is disabled; reinstall with Rust feature 'zip'",
    ))
}

/// @export
#[savvy]
impl ZarrVcf {
    /// Open a VCF Zarr store from a local path or URL.
    ///
    /// Automatically detects local `.zip` files and `.zip` URL objects and reads
    /// them via the `zarrs_zip` adapter directly. URL schemes (`http://`,
    /// `s3://`, `gs://`, `az://`, `file://`) are supported by the object-store
    /// backend.
    ///
    /// @param path Path to a `.zarr` directory, a `.zip` file, or a URL.
    /// @returns A `ZarrVcf` object.
    /// @export
    fn open(path: &str) -> savvy::Result<Self> {
        if looks_like_url(path) {
            return Self::open_object_store(path);
        }

        let store: Arc<dyn ReadListStorage> = if path.to_lowercase().ends_with(".zip") {
            open_local_zip_store(path)?
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
    /// `.zarr.zip` / `.zip` URLs are opened as zip objects when the `zip` Rust
    /// feature is enabled.
    ///
    /// @param url Store URL, e.g. `"s3://my-bucket/path/to/store.zarr"`.
    /// @returns A `ZarrVcf` object.
    /// @export
    fn open_object_store(url: &str) -> savvy::Result<Self> {
        init_vcf_from_store(open_object_store_url(url)?)
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
        self.retrieve_array_by_name("variant_allele")
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
        let nv = u64_dim_to_i32(shape[0], "variant")?;
        let ns = u64_dim_to_i32(shape[1], "sample")?;
        let ploidy = u64_dim_to_i32(shape[2], "ploidy")?;

        let var_idx = parse_indices(variants.unwrap_or(NullSexp.into()), nv)?;
        let sam_idx = parse_indices(samples.unwrap_or(NullSexp.into()), ns)?;
        let dims = [
            usize_len_to_i32(var_idx.len(), "selected variant")?,
            usize_len_to_i32(sam_idx.len(), "selected sample")?,
            ploidy,
        ];

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
        let nv = u64_dim_to_i32(shape[0], "variant")?;
        let ns = u64_dim_to_i32(shape[1], "sample")?;

        let var_idx = parse_indices(variants.unwrap_or(NullSexp.into()), nv)?;
        let sam_idx = parse_indices(samples.unwrap_or(NullSexp.into()), ns)?;
        let dims = [
            usize_len_to_i32(var_idx.len(), "selected variant")?,
            usize_len_to_i32(sam_idx.len(), "selected sample")?,
        ];

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
                if x < 1.0 || x > max_val as f64 {
                    return Err(savvy::Error::new(&format!(
                        "index {} out of range [1, {}]",
                        x, max_val
                    )));
                }
                let xi = x as i32;
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
    let mut out = unsafe { OwnedIntegerSexp::new_without_init(data.len())? };
    copy_c_to_r_order(data, dims, out.as_mut_slice());
    if dims.len() > 1 {
        out.set_dim(dims)?;
    }
    Ok(out.into())
}

fn fill_logical_array(data: &[bool], dims: &[i32]) -> savvy::Result<savvy::Sexp> {
    let mut out = unsafe { OwnedLogicalSexp::new_without_init(data.len())? };
    try_for_each_c_to_r_order(data, dims, |index, &value| out.set_elt(index, value))?;
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

        let dims: Vec<i32> = shape_to_i32_dims(&shape)?;
        let mut out = retrieve_typed(&array, &subset, &dtype, &dims)?;
        if dims.len() > 1 {
            out.set_dim(&dims)?;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::{c_to_r_order, map_c_to_r_order};

    #[test]
    fn c_to_r_order_roundtrip_1d_identity() {
        let data = vec![10i32, 20, 30, 40];
        let dims = vec![4];
        let reordered = c_to_r_order(&data, &dims);
        assert_eq!(reordered, vec![10, 20, 30, 40]);
    }

    #[test]
    fn c_to_r_order_2x3_example() {
        let data = vec![1, 2, 3, 4, 5, 6];
        let dims = vec![2, 3];
        let reordered = c_to_r_order(&data, &dims);
        // Expected C->R mapping for dimensions (2,3):
        // C-order index layout (0,1,2,3,4,5) maps to R positions [0,2,4,1,3,5].
        assert_eq!(reordered, vec![1, 4, 2, 5, 3, 6]);
    }

    #[test]
    fn c_to_r_order_2x2x2_example() {
        let data: Vec<i32> = (1..=8).collect();
        let dims = vec![2, 2, 2];
        let reordered = c_to_r_order(&data, &dims);
        assert_eq!(reordered, vec![1, 5, 3, 7, 2, 6, 4, 8]);
    }

    #[test]
    fn c_to_r_order_matches_reference_for_bounded_shapes() {
        assert_eq!(c_to_r_order(&[7], &[]), vec![7]);

        for rank in 1..=5 {
            for shape_code in 0..5usize.pow(rank as u32) {
                let mut code = shape_code;
                let mut dims = Vec::with_capacity(rank);
                for _ in 0..rank {
                    dims.push((code % 5) as i32);
                    code /= 5;
                }

                let n = dims.iter().map(|&dim| dim as usize).product();
                let data: Vec<usize> = (0..n).collect();
                let actual = c_to_r_order(&data, &dims);
                let mut expected = vec![0; n];

                let mut c_strides = vec![1usize; rank];
                for axis in (0..rank.saturating_sub(1)).rev() {
                    c_strides[axis] = c_strides[axis + 1] * dims[axis + 1] as usize;
                }
                let mut r_strides = vec![1usize; rank];
                for axis in 1..rank {
                    r_strides[axis] = r_strides[axis - 1] * dims[axis - 1] as usize;
                }

                for (linear, &value) in data.iter().enumerate() {
                    let mut remaining = linear;
                    let mut r_pos = 0;
                    for axis in 0..rank {
                        let coordinate = remaining / c_strides[axis];
                        remaining %= c_strides[axis];
                        r_pos += coordinate * r_strides[axis];
                    }
                    expected[r_pos] = value;
                }

                assert_eq!(actual, expected, "shape {dims:?}");

                let mut mapped = vec![0u64; n];
                map_c_to_r_order(&data, &dims, &mut mapped, |&value| value as u64 * 3 + 1);
                let expected_mapped: Vec<u64> =
                    expected.iter().map(|&value| value as u64 * 3 + 1).collect();
                assert_eq!(mapped, expected_mapped, "mapped shape {dims:?}");
            }
        }
    }
}
