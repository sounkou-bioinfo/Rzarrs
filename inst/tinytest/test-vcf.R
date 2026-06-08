## tests for ZarrVcf — VCF Zarr reader

# Helper: open a fixture by version
zv <- function(v) ZarrVcf$open(system.file("testdata", "vcf_zarr", v, package = "Rzarrs"))

# ---- ZarrGroup basics ----
base <- system.file("testdata", "vcf_zarr", "v0.2", package = "Rzarrs")
grp <- ZarrGroup$open(ZarrStore$open(base), "/")
expect_true(inherits(grp, "ZarrGroup"))

attrs <- grp$attributes()
expect_true(is.list(attrs))
expect_equal(attrs$vcf_zarr_version, "0.2")

ch <- grp$children(FALSE)
expect_true(is.list(ch))
expect_true("path" %in% names(ch))
expect_true("kind" %in% names(ch))
expect_true(all(ch$kind == "array"))  # all direct children are arrays
expect_true(length(ch$path) >= 8)

aj <- grp$attributes_json()
expect_true(is.character(aj))
expect_true(nchar(aj) > 0)

# ---- ZarrVcf$open dispatchers ----
zv_path <- ZarrVcf$open(base)
expect_true(inherits(zv_path, "ZarrVcf"))

zv_store <- ZarrVcf$open_store(ZarrStore$open(base))
expect_true(inherits(zv_store, "ZarrVcf"))

# ---- ZarrVcf$version ----
expect_equal(zv("v0.2")$version(), "0.2")
expect_equal(zv("v0.3")$version(), "0.3")
expect_equal(zv("v0.4")$version(), "0.4")

# ---- sample metadata ----
for (v in c("v0.2", "v0.3", "v0.4")) {
  z <- zv(v)
  expect_equal(z$n_samples(), 3L)
  expect_equal(z$samples(), c("S1", "S2", "S3"))
}

# ---- contigs and filters ----
for (v in c("v0.2", "v0.3", "v0.4")) {
  z <- zv(v)
  expect_equal(z$contigs(), c("chr1", "chr2"))
  expect_equal(z$filters(), c("PASS", "q30"))
}

# ---- filter_description (v0.3+) ----
for (v in c("v0.3", "v0.4")) {
  z <- zv(v)
  base <- system.file("testdata", "vcf_zarr", v, package = "Rzarrs")
  arr <- ZarrArray$open(ZarrStore$open(base), "/filter_description")
  f <- arr$retrieve()
  expect_equal(f, c("All filters passed", "Quality below 30"))
}

# ---- variant fields ----
for (v in c("v0.2", "v0.3", "v0.4")) {
  z <- zv(v)
  expect_equal(z$n_variants(), 5L)
  expect_equal(z$variant_position(), c(100L, 200L, 300L, 50L, 150L))
  expect_equal(z$variant_contig(), c("chr1", "chr1", "chr1", "chr2", "chr2"))
}

# ---- variant_allele (string 2D array) ----
z <- zv("v0.4")
alleles <- z$variant_allele()
expect_equal(length(alleles), 10L)
expect_equal(alleles[1], "A")

# ---- call_genotype (3D array, C-order → Fortran-order) ----
z <- zv("v0.2")
gt <- z$genotypes()
expect_equal(dim(gt), c(5L, 3L, 2L))  # variants x samples x ploidy
# S1, var1 = 0/0
expect_equal(gt[1, 1, ], c(0L, 0L))
# var4, S1 = ./.
expect_equal(gt[4, 1, ], c(-1L, -1L))

# ---- call_genotype subsetting ----
z <- zv("v0.4")
gt2 <- z$genotypes(variants = 1:3, samples = 1)
expect_equal(dim(gt2), c(3L, 1L, 2L))
expect_equal(gt2[1, 1, ], c(0L, 0L))

# ---- call_genotype_phased ----
ph <- z$call_genotype_phased()
expect_equal(dim(ph), c(5L, 3L))
expect_true(is.logical(ph))

# ---- variant() / call() generic accessors ----
z <- zv("v0.4")
expect_equal(z$variant("position"), z$variant_position())
# variant("contig") returns raw 0-indexed integer indices
expect_equal(z$variant("contig"), c(0L, 0L, 0L, 1L, 1L))

# ---- fields ----
z <- zv("v0.4")
flds <- z$fields()
expect_true("sample_id" %in% flds)
expect_true("call_genotype" %in% flds)
expect_true("variant_position" %in% flds)
expect_true("variant_contig" %in% flds)
expect_true("variant_allele" %in% flds)

# ---- v0.1 (group-attribute metadata) ----
z01 <- zv("v0.1")
expect_equal(z01$version(), "0.1")
expect_equal(z01$n_variants(), 5L)
expect_equal(z01$n_samples(), 3L)
expect_equal(z01$samples(), c("S1", "S2", "S3"))
expect_equal(z01$contigs(), c("chr1", "chr2"))
expect_equal(z01$filters(), c("PASS", "q30"))
expect_equal(z01$variant_position(), c(100L, 200L, 300L, 50L, 150L))
expect_equal(z01$variant_contig(), c("chr1", "chr1", "chr1", "chr2", "chr2"))

# ---- ZIP backend ----
zip_path <- system.file("testdata", "vcf_zarr", "v0.4.zarr.zip", package = "Rzarrs")
zz <- ZarrVcf$open(zip_path)
expect_true(inherits(zz, "ZarrVcf"))
expect_equal(zz$version(), "0.4")
expect_equal(zz$n_variants(), 5L)
expect_equal(zz$n_samples(), 3L)
expect_equal(zz$samples(), c("S1", "S2", "S3"))
expect_equal(zz$contigs(), c("chr1", "chr2"))
expect_equal(zz$variant_position(), c(100L, 200L, 300L, 50L, 150L))
expect_equal(zz$variant_contig(), c("chr1", "chr1", "chr1", "chr2", "chr2"))
phz <- zz$call_genotype_phased()
expect_equal(dim(phz), c(5L, 3L))
expect_true(is.logical(phz))
# No temp dir leakage
expect_false(any(grepl("rzarrs_", list.dirs(tempdir(), recursive = FALSE))))

zip_abs <- normalizePath(zip_path, winslash = "/", mustWork = TRUE)
zip_url <- paste0("file://", if (.Platform$OS.type == "windows") "/" else "", zip_abs)
zz_url <- ZarrVcf$open(zip_url)
expect_equal(zz_url$version(), "0.4")
expect_equal(zz_url$genotypes(variants = 1:2, samples = 1:2), zz$genotypes(variants = 1:2, samples = 1:2))
zip_obj <- ZarrObjectStore$open(zip_url)
zip_grp <- ZarrGroup$open_object_store(zip_obj, "/")
expect_equal(zip_grp$attributes()$vcf_zarr_version, "0.4")

# ---- VCF index validation is Rust-side and rejects silent truncation ----
z <- zv("v0.4")
expect_error(z$genotypes(variants = 1.5, samples = 1), "whole numbers")
expect_error(z$call_genotype_phased(variants = 1, samples = NA_integer_), "NA")
