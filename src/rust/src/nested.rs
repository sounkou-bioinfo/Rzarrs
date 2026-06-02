#![allow(dead_code)]

//! Dtype planning for primitive, nested, and extension/plugin dtypes.
//!
//! This module is intentionally separate from materialization. Opening arrays
//! must be permissive: unknown dtype extensions should not prevent metadata
//! inspection. Reading data must be strict: Rzarrs must not silently downcast,
//! lose precision, or flatten nested values without an explicit policy.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Integer64Policy {
    /// Return exact decimal strings.
    String,
    /// Return R doubles. Values outside +/- 2^53 must error at read time.
    DoubleLossy,
    /// Return a package-owned signed 64-bit vector class.
    Int64Class,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FloatExtensionPolicy {
    /// Reject unless a dedicated vector class exists.
    Reject,
    /// Promote to R double. Exact for float16/bfloat16, lossy for float128.
    Double,
    /// Return exact string representation.
    String,
    /// Return a package-owned extension vector class retaining raw payloads.
    ExtensionVector,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingPolicy {
    RawSentinel,
    RNa,
    Masked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrecisionClass {
    NativeExact,
    PromotedExact,
    PromotedLossy,
    ExactString,
    RequiresExtensionClass,
    Nested,
    UnknownExtension,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RMaterialization {
    Logical,
    Integer,
    Real,
    Complex,
    String,
    Raw,
    List,
    DataFrame,
    CompressedList,
    ExtensionVector,
    ExtensionHandle,
    DateTime,
    TimeDelta,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DtypePlan {
    pub dtype_name: String,
    pub r_type: RMaterialization,
    pub precision: PrecisionClass,
    pub nullable: bool,
    pub nested: bool,
    pub lossless: bool,
    pub requires_explicit_cast: bool,
    pub extension_name: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DtypePolicy {
    pub int64: Integer64Policy,
    pub low_precision_float: FloatExtensionPolicy,
    pub high_precision_float: FloatExtensionPolicy,
    pub unknown_extension_as_handle: bool,
}

impl Default for DtypePolicy {
    fn default() -> Self {
        Self {
            int64: Integer64Policy::String,
            low_precision_float: FloatExtensionPolicy::Double,
            high_precision_float: FloatExtensionPolicy::Double,
            unknown_extension_as_handle: true,
        }
    }
}

pub fn plan_dtype(dtype_name: &str, int64_policy: Integer64Policy) -> DtypePlan {
    let mut policy = DtypePolicy::default();
    policy.int64 = int64_policy;
    plan_dtype_with_policy(dtype_name, &policy)
}

pub fn plan_dtype_with_policy(dtype_name: &str, policy: &DtypePolicy) -> DtypePlan {
    let dtype_name = canonical_dtype_name(dtype_name);

    match dtype_name.as_str() {
        "bool" => primitive(&dtype_name, RMaterialization::Logical),

        "int8" | "int16" | "int32" | "uint8" | "uint16" => {
            primitive(&dtype_name, RMaterialization::Integer)
        }

        // R integer cannot represent all uint32 values. R double can represent
        // every uint32 value exactly because 2^32 < 2^53.
        "uint32" => DtypePlan {
            dtype_name,
            r_type: RMaterialization::Real,
            precision: PrecisionClass::PromotedExact,
            nullable: false,
            nested: false,
            lossless: true,
            requires_explicit_cast: false,
            extension_name: None,
            note: Some("uint32 is promoted to R double exactly".to_string()),
        },

        "int64" => plan_int64_like(&dtype_name, &policy.int64),

        "uint64" => DtypePlan {
            dtype_name,
            r_type: RMaterialization::ExtensionVector,
            precision: PrecisionClass::RequiresExtensionClass,
            nullable: false,
            nested: false,
            lossless: true,
            requires_explicit_cast: false,
            extension_name: Some("Rzarrs_uint64".to_string()),
            note: Some("uint64 materializes as exact Rzarrs_uint64 with lossless formatting, exact checked double coercion, comparisons, and checked integer arithmetic".to_string()),
        },

        // Every f32 value is exactly representable in f64.
        "float32" => DtypePlan {
            dtype_name,
            r_type: RMaterialization::Real,
            precision: PrecisionClass::PromotedExact,
            nullable: false,
            nested: false,
            lossless: true,
            requires_explicit_cast: false,
            extension_name: None,
            note: None,
        },

        "float64" => primitive(&dtype_name, RMaterialization::Real),

        // Lower precision float extensions can be promoted exactly to f64, but
        // they are extension types from the perspective of Zarr/Rzarrs. Keep
        // that fact visible so callers can request a native vector class later.
        "float16" | "f16" | "bfloat16" | "bf16" | "b16" => {
            plan_float_extension(&dtype_name, &policy.low_precision_float, true)
        }

        // Higher precision floats cannot be represented exactly by base R.
        "float128" | "f128" | "decimal128" | "decimal256" => {
            plan_float_extension(&dtype_name, &policy.high_precision_float, false)
        }

        // Base R has complex vectors with f64 real/imag parts.
        "complex64" => DtypePlan {
            dtype_name,
            r_type: RMaterialization::Complex,
            precision: PrecisionClass::PromotedExact,
            nullable: false,
            nested: false,
            lossless: true,
            requires_explicit_cast: false,
            extension_name: None,
            note: Some("complex64 should materialize as R complex".to_string()),
        },

        "complex128" => DtypePlan {
            dtype_name,
            r_type: RMaterialization::Complex,
            precision: PrecisionClass::NativeExact,
            nullable: false,
            nested: false,
            lossless: true,
            requires_explicit_cast: false,
            extension_name: None,
            note: Some("complex128 should materialize as R complex".to_string()),
        },

        "string" | "utf8" | "vlen-utf8" => primitive(&dtype_name, RMaterialization::String),

        "bytes" | "raw" | "vlen-bytes" => primitive(&dtype_name, RMaterialization::Raw),

        name if name.starts_with("optional[") => DtypePlan {
            dtype_name: name.to_string(),
            r_type: RMaterialization::List,
            precision: PrecisionClass::Nested,
            nullable: true,
            nested: true,
            lossless: true,
            requires_explicit_cast: true,
            extension_name: None,
            note: Some("optional dtype requires explicit missing-policy selection".to_string()),
        },

        name if name.starts_with("list[") || name.starts_with("varlen[") => DtypePlan {
            dtype_name: name.to_string(),
            r_type: RMaterialization::CompressedList,
            precision: PrecisionClass::Nested,
            nullable: false,
            nested: true,
            lossless: true,
            requires_explicit_cast: true,
            extension_name: None,
            note: Some(
                "list/varlen dtype should use ALTREP list or compressed-list materialization".to_string(),
            ),
        },

        name if name.starts_with("struct{") || name.starts_with("struct[") => DtypePlan {
            dtype_name: name.to_string(),
            r_type: RMaterialization::DataFrame,
            precision: PrecisionClass::Nested,
            nullable: false,
            nested: true,
            lossless: true,
            requires_explicit_cast: true,
            extension_name: None,
            note: Some(
                "struct dtype should materialize as a field list/DataFrame; do not flatten silently".to_string(),
            ),
        },

        "numpy.datetime64" => DtypePlan {
            dtype_name,
            r_type: RMaterialization::DateTime,
            precision: PrecisionClass::RequiresExtensionClass,
            nullable: true,
            nested: false,
            lossless: true,
            requires_explicit_cast: false,
            extension_name: Some("Rzarrs_int64".to_string()),
            note: Some(
                "numpy.datetime64 materializes as exact Rzarrs_int64 values with R attributes zarr_dtype, unit, and scale_factor; scale explicitly before POSIXct coercion".to_string(),
            ),
        },

        "numpy.timedelta64" => DtypePlan {
            dtype_name,
            r_type: RMaterialization::TimeDelta,
            precision: PrecisionClass::RequiresExtensionClass,
            nullable: true,
            nested: false,
            lossless: true,
            requires_explicit_cast: false,
            extension_name: Some("Rzarrs_int64".to_string()),
            note: Some(
                "numpy.timedelta64 materializes as exact Rzarrs_int64 values with R attributes zarr_dtype, unit, and scale_factor; scale explicitly before seconds coercion".to_string(),
            ),
        },

        name if name.contains("datetime") || name.contains("timestamp") => DtypePlan {
            dtype_name: name.to_string(),
            r_type: RMaterialization::DateTime,
            precision: PrecisionClass::RequiresExtensionClass,
            nullable: true,
            nested: false,
            lossless: true,
            requires_explicit_cast: true,
            extension_name: Some("Rzarrs_time".to_string()),
            note: Some("time-like extension dtype requires a registered unit/timezone materializer".to_string()),
        },

        name if name.contains("timedelta") || name.contains("duration") => DtypePlan {
            dtype_name: name.to_string(),
            r_type: RMaterialization::TimeDelta,
            precision: PrecisionClass::RequiresExtensionClass,
            nullable: true,
            nested: false,
            lossless: true,
            requires_explicit_cast: true,
            extension_name: Some("Rzarrs_duration".to_string()),
            note: Some("duration-like extension dtype requires a registered unit materializer".to_string()),
        },

        _ => DtypePlan {
            dtype_name: dtype_name.to_string(),
            r_type: if policy.unknown_extension_as_handle {
                RMaterialization::ExtensionHandle
            } else {
                RMaterialization::ExtensionVector
            },
            precision: PrecisionClass::UnknownExtension,
            nullable: false,
            nested: true,
            lossless: true,
            requires_explicit_cast: true,
            extension_name: Some(dtype_name.clone()),
            note: Some(format!(
                "unsupported extension dtype '{dtype_name}'; open metadata is allowed, but read requires a registered extension materializer"
            )),
        },
    }
}

fn canonical_dtype_name(dtype_name: &str) -> String {
    dtype_name
        .split(" / ")
        .next()
        .unwrap_or(dtype_name)
        .trim()
        .to_ascii_lowercase()
}

fn primitive(dtype_name: &str, r_type: RMaterialization) -> DtypePlan {
    DtypePlan {
        dtype_name: dtype_name.to_string(),
        r_type,
        precision: PrecisionClass::NativeExact,
        nullable: false,
        nested: false,
        lossless: true,
        requires_explicit_cast: false,
        extension_name: None,
        note: None,
    }
}

fn plan_int64_like(dtype_name: &str, policy: &Integer64Policy) -> DtypePlan {
    match policy {
        Integer64Policy::String => DtypePlan {
            dtype_name: dtype_name.to_string(),
            r_type: RMaterialization::String,
            precision: PrecisionClass::ExactString,
            nullable: false,
            nested: false,
            lossless: true,
            requires_explicit_cast: true,
            extension_name: None,
            note: Some(
                "int64 default materialization is exact string or package-owned extension class; silent double is not used by default".to_string(),
            ),
        },
        Integer64Policy::DoubleLossy => DtypePlan {
            dtype_name: dtype_name.to_string(),
            r_type: RMaterialization::Real,
            precision: PrecisionClass::PromotedLossy,
            nullable: false,
            nested: false,
            lossless: false,
            requires_explicit_cast: true,
            extension_name: None,
            note: Some(
                "integer values above 2^53 cannot be represented exactly as R double".to_string(),
            ),
        },
        Integer64Policy::Int64Class => DtypePlan {
            dtype_name: dtype_name.to_string(),
            r_type: RMaterialization::ExtensionVector,
            precision: PrecisionClass::RequiresExtensionClass,
            nullable: false,
            nested: false,
            lossless: true,
            requires_explicit_cast: false,
            extension_name: Some("Rzarrs_int64".to_string()),
            note: Some("int64 materializes as exact Rzarrs_int64 with lossless formatting, exact checked double coercion, comparisons, and checked integer arithmetic".to_string()),
        },
    }
}

fn plan_float_extension(
    dtype_name: &str,
    policy: &FloatExtensionPolicy,
    f64_exact: bool,
) -> DtypePlan {
    match policy {
        FloatExtensionPolicy::Reject => DtypePlan {
            dtype_name: dtype_name.to_string(),
            r_type: RMaterialization::ExtensionHandle,
            precision: PrecisionClass::RequiresExtensionClass,
            nullable: false,
            nested: false,
            lossless: true,
            requires_explicit_cast: true,
            extension_name: Some(dtype_name.to_string()),
            note: Some(format!(
                "{dtype_name} requires an explicit materialization policy"
            )),
        },
        FloatExtensionPolicy::Double => DtypePlan {
            dtype_name: dtype_name.to_string(),
            r_type: RMaterialization::Real,
            precision: if f64_exact {
                PrecisionClass::PromotedExact
            } else {
                PrecisionClass::PromotedLossy
            },
            nullable: false,
            nested: false,
            lossless: f64_exact,
            requires_explicit_cast: false,
            extension_name: None,
            note: if f64_exact {
                Some(format!("{dtype_name} is promoted to R double exactly"))
            } else {
                Some(format!(
                    "{dtype_name} materializes as lossy R double; exact payload preservation requires an explicit extension-vector policy"
                ))
            },
        },
        FloatExtensionPolicy::String => DtypePlan {
            dtype_name: dtype_name.to_string(),
            r_type: RMaterialization::String,
            precision: PrecisionClass::ExactString,
            nullable: false,
            nested: false,
            lossless: true,
            requires_explicit_cast: true,
            extension_name: None,
            note: Some(format!("{dtype_name} materializes as exact string")),
        },
        FloatExtensionPolicy::ExtensionVector => DtypePlan {
            dtype_name: dtype_name.to_string(),
            r_type: RMaterialization::ExtensionVector,
            precision: PrecisionClass::RequiresExtensionClass,
            nullable: false,
            nested: false,
            lossless: true,
            requires_explicit_cast: true,
            extension_name: Some(format!("Rzarrs_{}", dtype_name.replace('-', "_"))),
            note: Some(format!(
                "{dtype_name} should use a package-owned vector class"
            )),
        },
    }
}
