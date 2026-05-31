## tools/make_vcf_fixtures.R
## Generate minimal VCF Zarr V3 test fixtures for spec versions 0.2, 0.3, 0.4.
##
## Data layout (same across all versions):
##   5 variants (chr1: pos 100,200,300 + chr2: pos 50,150)
##   3 samples  (S1, S2, S3)
##   2 contigs  (chr1, chr2)
##   2 filters  (PASS, q30)
##   diploid (ploidy = 2)
##
## Integer encoding:  -1 = missing,  -2 = fill  (VCF Zarr spec)
## All arrays use Zarr V3, chunk = whole array, little-endian bytes codec.
##
## Run: Rscript tools/make_vcf_fixtures.R  (from package root)

set.seed(42)

# ---------------------------------------------------------------------------
# helpers
# ---------------------------------------------------------------------------

write_text <- function(path, text) {
  dir.create(dirname(path), recursive = TRUE, showWarnings = FALSE)
  writeLines(text, path)
}

# Write a single-chunk Zarr V3 integer array (int32, C-order).
# `data` is a plain R integer vector; dims is C-order (outermost first).
write_int32_array <- function(base, name, data, dims) {
  arr_dir <- file.path(base, name)
  dir.create(file.path(arr_dir, "c"), recursive = TRUE, showWarnings = FALSE)

  n_dims <- length(dims)
  shape_str  <- paste(dims, collapse = ", ")
  chunk_str  <- paste(dims, collapse = ", ")

  write_text(file.path(arr_dir, "zarr.json"), sprintf('{
  "zarr_format": 3,
  "node_type": "array",
  "shape": [%s],
  "data_type": "int32",
  "chunk_grid": {"name": "regular", "configuration": {"chunk_shape": [%s]}},
  "chunk_key_encoding": {"name": "default", "configuration": {"separator": "/"}},
  "fill_value": -2,
  "codecs": [{"name": "bytes", "configuration": {"endian": "little"}}]
}', shape_str, chunk_str))

  # Single chunk key: c/0 (1D), c/0/0 (2D), c/0/0/0 (3D)
  chunk_key <- file.path(arr_dir, "c", paste(rep("0", n_dims), collapse = "/"))
  dir.create(dirname(chunk_key), recursive = TRUE, showWarnings = FALSE)
  writeBin(as.integer(data), chunk_key, size = 4L, endian = "little")
}

# Write a single-chunk Zarr V3 bool array (uint8 0/1, C-order).
write_bool_array <- function(base, name, data, dims) {
  arr_dir <- file.path(base, name)
  dir.create(file.path(arr_dir, "c"), recursive = TRUE, showWarnings = FALSE)

  n_dims <- length(dims)
  shape_str <- paste(dims, collapse = ", ")
  chunk_str <- paste(dims, collapse = ", ")

  write_text(file.path(arr_dir, "zarr.json"), sprintf('{
  "zarr_format": 3,
  "node_type": "array",
  "shape": [%s],
  "data_type": "bool",
  "chunk_grid": {"name": "regular", "configuration": {"chunk_shape": [%s]}},
  "chunk_key_encoding": {"name": "default", "configuration": {"separator": "/"}},
  "fill_value": false,
  "codecs": [{"name": "bytes", "configuration": {"endian": "little"}}]
}', shape_str, chunk_str))

  chunk_key <- file.path(arr_dir, "c", paste(rep("0", n_dims), collapse = "/"))
  dir.create(dirname(chunk_key), recursive = TRUE, showWarnings = FALSE)
  writeBin(as.raw(as.integer(as.logical(data))), chunk_key)
}

# Write a single-chunk Zarr V3 string array (vlen-utf8 / vlen-bytes codec).
# Format: [u32le: num_elements][u32le: offset_1]...[u32le: offset_n+1][bytes...]
# zarrs vlen-bytes chunk format: concatenated bytes with offsets prefix.
write_string_array <- function(base, name, strings, dims) {
  arr_dir <- file.path(base, name)
  dir.create(file.path(arr_dir, "c"), recursive = TRUE, showWarnings = FALSE)

  n_dims <- length(dims)
  shape_str <- paste(dims, collapse = ", ")
  chunk_str <- paste(dims, collapse = ", ")

  write_text(file.path(arr_dir, "zarr.json"), sprintf('{
  "zarr_format": 3,
  "node_type": "array",
  "shape": [%s],
  "data_type": "string",
  "chunk_grid": {"name": "regular", "configuration": {"chunk_shape": [%s]}},
  "chunk_key_encoding": {"name": "default", "configuration": {"separator": "/"}},
  "fill_value": "",
  "codecs": [{"name": "vlen-bytes"}]
}', shape_str, chunk_str))

  # vlen_bytes chunk format used by zarrs (wraps vlen-v2, interleaved):
  #   [u32le: n_elements]
  #   [u32le: len_0][bytes_0]  (for each element)
  #   [u32le: len_1][bytes_1]
  #   ...
  n  <- length(strings)
  bytes_list <- lapply(strings, charToRaw)

  con <- rawConnection(raw(0), "wb")
  writeBin(as.integer(n), con, size = 4L, endian = "little")
  for (b in bytes_list) {
    writeBin(as.integer(length(b)), con, size = 4L, endian = "little")
    if (length(b) > 0) writeBin(b, con)
  }
  chunk_bytes <- rawConnectionValue(con)
  close(con)

  chunk_key <- file.path(arr_dir, "c", paste(rep("0", n_dims), collapse = "/"))
  dir.create(dirname(chunk_key), recursive = TRUE, showWarnings = FALSE)
  writeBin(chunk_bytes, chunk_key)
}

write_group <- function(path, attrs_json = "{}") {
  dir.create(path, recursive = TRUE, showWarnings = FALSE)
  write_text(file.path(path, "zarr.json"), sprintf('{
  "zarr_format": 3,
  "node_type": "group",
  "attributes": %s
}', attrs_json))
}

# ---------------------------------------------------------------------------
# shared data
# ---------------------------------------------------------------------------

n_variants <- 5L
n_samples  <- 3L
n_alleles  <- 2L  # REF + ALT
ploidy     <- 2L

sample_ids  <- c("S1", "S2", "S3")
contig_ids  <- c("chr1", "chr2")
filter_ids  <- c("PASS", "q30")
filter_desc <- c("All filters passed", "Quality below 30")

# variant_contig: 0-indexed contig index (chr1=0, chr2=1)
variant_contig   <- c(0L, 0L, 0L, 1L, 1L)
variant_position <- c(100L, 200L, 300L, 50L, 150L)

# variant_allele: 5 variants x 2 alleles (C-order row-major)
variant_allele <- c(
  "A", "T",    # var1
  "C", "G",    # var2
  "G", "A",    # var3
  "T", "C",    # var4
  "A", "G"     # var5
)

# call_genotype: variants x samples x ploidy  (C-order)
# shape [5, 3, 2]
gt_mat <- matrix(c(
  0L,0L, 0L,1L, 1L,1L,  # var1: 0/0, 0/1, 1/1
  0L,1L, 1L,1L, 0L,0L,  # var2
  1L,0L, 0L,0L, 1L,0L,  # var3
 -1L,-1L, 0L,1L, 1L,1L, # var4: missing for S1
  0L,0L, 0L,0L, 0L,1L   # var5
), nrow = n_variants, byrow = TRUE)
# gt_mat rows = variants, cols = sample*ploidy (C-order within row)
call_genotype <- as.vector(t(gt_mat))  # flatten row-major

# call_genotype_phased: variants x samples (C-order)
phased_mat <- matrix(c(
  FALSE, TRUE,  FALSE,
  TRUE,  FALSE, TRUE,
  FALSE, FALSE, FALSE,
  FALSE, TRUE,  TRUE,
  TRUE,  TRUE,  FALSE
), nrow = n_variants, byrow = TRUE)
call_genotype_phased <- as.vector(t(phased_mat))

# ---------------------------------------------------------------------------
# v0.2 — adds contig_id, filter_id, sample_id arrays; no filter_description
# ---------------------------------------------------------------------------

make_v02 <- function(base) {
  attrs <- '{"vcf_zarr_version": "0.2"}'
  write_group(base, attrs)

  write_int32_array(base, "variant_contig",   variant_contig,   c(n_variants))
  write_int32_array(base, "variant_position", variant_position, c(n_variants))
  write_string_array(base, "variant_allele",  variant_allele,   c(n_variants, n_alleles))
  write_int32_array(base, "call_genotype",    call_genotype,    c(n_variants, n_samples, ploidy))
  write_bool_array(base, "call_genotype_phased", call_genotype_phased, c(n_variants, n_samples))

  write_string_array(base, "sample_id",  sample_ids,  c(n_samples))
  write_string_array(base, "contig_id",  contig_ids,  c(length(contig_ids)))
  write_string_array(base, "filter_id",  filter_ids,  c(length(filter_ids)))
}

# ---------------------------------------------------------------------------
# v0.3 — adds filter_description array
# ---------------------------------------------------------------------------

make_v03 <- function(base) {
  make_v02(base)  # reuse everything
  # overwrite zarr.json with v0.3 version marker
  write_group(base, '{"vcf_zarr_version": "0.3"}')
  write_string_array(base, "filter_description", filter_desc, c(length(filter_desc)))
}

# ---------------------------------------------------------------------------
# v0.4 — uses vcf_meta_information attribute (richer group metadata)
# ---------------------------------------------------------------------------

make_v04 <- function(base) {
  make_v03(base)
  meta_info <- '{
    "vcf_zarr_version": "0.4",
    "vcf_meta_information": {
      "fileformat": "VCFv4.3",
      "FILTER": [
        {"ID": "PASS",  "Description": "All filters passed"},
        {"ID": "q30",   "Description": "Quality below 30"}
      ],
      "contig": [
        {"ID": "chr1", "length": 1000},
        {"ID": "chr2", "length": 500}
      ]
    }
  }'
  write_group(base, meta_info)
}

# ---------------------------------------------------------------------------
# write all versions
# ---------------------------------------------------------------------------

out_root <- file.path("inst", "testdata", "vcf_zarr")

for (ver in c("v0.2", "v0.3", "v0.4")) {
  path <- file.path(out_root, ver)
  if (dir.exists(path)) unlink(path, recursive = TRUE)
  switch(ver,
    "v0.2" = make_v02(path),
    "v0.3" = make_v03(path),
    "v0.4" = make_v04(path)
  )
  cat("wrote", path, "\n")
}

cat("Done.\n")
