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

zv_store <- ZarrVcf$open(ZarrStore$open(base))
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
