#![allow(dead_code)]

//! VCF Zarr schema compatibility layer.
//!
//! This module is intentionally independent of the current fixture-specific
//! `ZarrVcf` fields.  The reader should scan the store once, produce a
//! `VcfSchema`, and route all high-level accessors through that schema.

use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VcfZarrSpecVersion {
    V0_1,
    V0_2,
    V0_3,
    V0_4,
    Unknown(String),
}

impl VcfZarrSpecVersion {
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.unwrap_or("0.1") {
            "0.1" => Self::V0_1,
            "0.2" => Self::V0_2,
            "0.3" => Self::V0_3,
            "0.4" => Self::V0_4,
            other => Self::Unknown(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::V0_1 => "0.1",
            Self::V0_2 => "0.2",
            Self::V0_3 => "0.3",
            Self::V0_4 => "0.4",
            Self::Unknown(s) => s.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VcfFieldKind {
    Variant,
    Call,
    Sample,
    Contig,
    Filter,
    RegionIndex,
    MaskFor(String),
    FillFor(String),
    Unknown,
}

#[derive(Debug, Clone)]
pub struct VcfField {
    pub name: String,
    pub path: String,
    pub kind: VcfFieldKind,
    pub dimensions: Vec<String>,
    pub dtype: String,
    pub fill_value: Option<Value>,
    pub mask_path: Option<String>,
    pub fill_path: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct VcfMeta {
    pub source: Option<String>,
    pub vcf_header: Option<String>,
    pub vcf_meta_information: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VcfSchema {
    pub version: VcfZarrSpecVersion,
    pub fields: BTreeMap<String, VcfField>,
    pub meta: VcfMeta,
    pub warnings: Vec<String>,
}

impl VcfSchema {
    pub fn from_root_attrs(attrs: &serde_json::Map<String, Value>) -> Self {
        let version =
            VcfZarrSpecVersion::parse(attrs.get("vcf_zarr_version").and_then(Value::as_str));
        let meta = VcfMeta {
            source: attrs
                .get("source")
                .and_then(Value::as_str)
                .map(str::to_string),
            vcf_header: attrs
                .get("vcf_header")
                .and_then(Value::as_str)
                .map(str::to_string),
            vcf_meta_information: attrs
                .get("vcf_meta_information")
                .and_then(Value::as_str)
                .map(str::to_string),
        };
        Self {
            version,
            fields: BTreeMap::new(),
            meta,
            warnings: Vec::new(),
        }
    }

    pub fn classify_array_name(name: &str) -> VcfFieldKind {
        if let Some(base) = name.strip_suffix("_mask") {
            return VcfFieldKind::MaskFor(base.to_string());
        }
        if let Some(base) = name.strip_suffix("_fill") {
            return VcfFieldKind::FillFor(base.to_string());
        }
        if name == "region_index" {
            return VcfFieldKind::RegionIndex;
        }
        if name.starts_with("variant_") {
            return VcfFieldKind::Variant;
        }
        if name.starts_with("call_") {
            return VcfFieldKind::Call;
        }
        if name == "sample_id" {
            return VcfFieldKind::Sample;
        }
        if name == "contig_id" {
            return VcfFieldKind::Contig;
        }
        if name == "filter_id" || name == "filter_description" {
            return VcfFieldKind::Filter;
        }
        VcfFieldKind::Unknown
    }

    pub fn attach_masks_and_fills(&mut self) {
        let keys: Vec<String> = self.fields.keys().cloned().collect();
        for key in keys {
            let kind = self.fields.get(&key).map(|f| f.kind.clone());
            match kind {
                Some(VcfFieldKind::MaskFor(base)) => {
                    let mask_path = self.fields.get(&key).map(|f| f.path.clone());
                    if let (Some(base_field), Some(mask_path)) =
                        (self.fields.get_mut(&base), mask_path)
                    {
                        base_field.mask_path = Some(mask_path);
                    }
                }
                Some(VcfFieldKind::FillFor(base)) => {
                    let fill_path = self.fields.get(&key).map(|f| f.path.clone());
                    if let (Some(base_field), Some(fill_path)) =
                        (self.fields.get_mut(&base), fill_path)
                    {
                        base_field.fill_path = Some(fill_path);
                    }
                }
                _ => {}
            }
        }
    }

    pub fn require_core_reader_fields(&self) -> Result<(), String> {
        for name in [
            "variant_position",
            "variant_contig",
            "variant_allele",
            "call_genotype",
        ] {
            if !self.fields.contains_key(name) {
                return Err(format!("required VCF Zarr array is missing: {name}"));
            }
        }
        Ok(())
    }
}
