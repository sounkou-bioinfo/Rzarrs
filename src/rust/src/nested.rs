#![allow(dead_code)]

//! Mapping plan for non-primitive, optional, and extension dtypes.
//!
//! `zarrs` can expose v3 dtype extensions.  Rzarrs must not reject unknown
//! extension dtypes at open time.  Plan first; materialize only when the caller
//! requests data and has chosen a compatible representation.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Integer64Policy {
    String,
    DoubleLossy,
    Bit64Class,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingPolicy {
    RawSentinel,
    RNa,
    Masked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RMaterialization {
    Logical,
    Integer,
    Real,
    String,
    Raw,
    List,
    DataFrame,
    CompressedList,
    ExtensionHandle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DtypePlan {
    pub r_type: RMaterialization,
    pub nullable: bool,
    pub nested: bool,
    pub warning: Option<String>,
}

pub fn plan_dtype(dtype_name: &str, int64_policy: Integer64Policy) -> DtypePlan {
    match dtype_name {
        "bool" => primitive(RMaterialization::Logical),
        "int8" | "int16" | "int32" | "uint8" | "uint16" => primitive(RMaterialization::Integer),
        "float32" | "float64" | "complex64" | "complex128" => primitive(RMaterialization::Real),
        "string" | "utf8" | "vlen-utf8" => primitive(RMaterialization::String),
        "bytes" | "raw" => primitive(RMaterialization::Raw),
        "int64" | "uint32" | "uint64" => match int64_policy {
            Integer64Policy::String => DtypePlan {
                r_type: RMaterialization::String,
                nullable: false,
                nested: false,
                warning: None,
            },
            Integer64Policy::DoubleLossy => DtypePlan {
                r_type: RMaterialization::Real,
                nullable: false,
                nested: false,
                warning: Some("integer values above 2^53 may lose precision".to_string()),
            },
            Integer64Policy::Bit64Class => DtypePlan {
                r_type: RMaterialization::Real,
                nullable: false,
                nested: false,
                warning: Some("set class 'integer64' on the R double vector".to_string()),
            },
        },
        name if name.starts_with("optional[") => DtypePlan {
            r_type: RMaterialization::List,
            nullable: true,
            nested: true,
            warning: Some("optional dtype requires an explicit missing policy".to_string()),
        },
        name if name.starts_with("list[") || name.starts_with("varlen[") => DtypePlan {
            r_type: RMaterialization::CompressedList,
            nullable: false,
            nested: true,
            warning: None,
        },
        name if name.starts_with("struct{") => DtypePlan {
            r_type: RMaterialization::DataFrame,
            nullable: false,
            nested: true,
            warning: None,
        },
        _ => DtypePlan {
            r_type: RMaterialization::ExtensionHandle,
            nullable: false,
            nested: true,
            warning: Some(format!("unsupported extension dtype: {dtype_name}")),
        },
    }
}

fn primitive(r_type: RMaterialization) -> DtypePlan {
    DtypePlan {
        r_type,
        nullable: false,
        nested: false,
        warning: None,
    }
}
