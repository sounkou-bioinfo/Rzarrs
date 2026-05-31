## ZarrVcf — High-level VCF Zarr reader (spec v0.1–v0.4)

## Internal constructor — returns a ZarrVcf instance environment.
.zv_new <- function(store, grp, attrs, ver, arr_names) {
  e <- new.env(parent = emptyenv())
  e$.store   <- store
  e$.group   <- grp
  e$.attrs   <- attrs
  e$.version <- ver
  e$.arrays  <- arr_names

  # Wire instance methods
  e$version            <- function()           .zv_version(e)
  e$n_variants         <- function()           .zv_n_variants(e)
  e$n_samples          <- function()           .zv_n_samples(e)
  e$samples            <- function()           .zv_samples(e)
  e$contigs            <- function()           .zv_contigs(e)
  e$filters            <- function()           .zv_filters(e)
  e$fields             <- function()           .zv_fields(e)
  e$variant_position   <- function()           .zv_variant_position(e)
  e$variant_contig     <- function()           .zv_variant_contig(e)
  e$variant_allele     <- function()           .zv_variant_allele(e)
  e$genotypes          <- function(variants = NULL, samples = NULL)
                                                   .zv_genotypes(e, variants, samples)
  e$call_genotype_phased <- function(variants = NULL, samples = NULL)
                                                   .zv_call_genotype_phased(e, variants, samples)
  e$variant            <- function(name)       .zv_variant(e, name)
  e$call               <- function(name)       .zv_call(e, name)

  class(e) <- c("ZarrVcf", "Rzarrs::ZarrVcf")
  e
}

#' High-level VCF Zarr reader
#'
#' `ZarrVcf$open(x)` opens a VCF Zarr store and returns an instance with
#' methods to access variant, sample, and genotype data. Supports spec
#' versions 0.1 through 0.4.
#'
#' @param x Path (string), URL (string), `ZarrStore`, or `ZarrObjectStore`.
#' @return A `ZarrVcf` instance.
#' @export
ZarrVcf <- new.env(parent = emptyenv())
ZarrVcf$open <- function(x) {
  store <- if (inherits(x, "ZarrStore")) {
    x
  } else if (inherits(x, "ZarrObjectStore")) {
    x
  } else if (is.character(x)) {
    if (grepl("^https?://|^s3://|^gs://|^az://|^file://", x)) {
      ZarrObjectStore$open(x)
    } else {
      ZarrStore$open(x)
    }
  } else {
    stop("'x' must be a path, URL, ZarrStore, or ZarrObjectStore", call. = FALSE)
  }

  grp <- ZarrGroup$open(store, "/")
  attrs <- grp$attributes()
  ver <- attrs$vcf_zarr_version %||% "0.1"

  # Available array names (basename of each child array path)
  children <- grp$children(FALSE)
  arr_names <- basename(children$path[children$kind == "array"])

  .zv_new(store, grp, attrs, ver, arr_names)
}

# ---------------------------------------------------------------------------
# Instance method implementations
# ---------------------------------------------------------------------------

.zv_fields    <- function(e) e$.arrays
.zv_version   <- function(e) e$.version
.zv_n_variants <- function(e) length(.zv_retrieve(e, "variant_position"))

.zv_n_samples <- function(e) {
  if ("sample_id" %in% e$.arrays) {
    length(.zv_retrieve(e, "sample_id"))
  } else {
    length(e$.attrs$sample_id %||% character(0))
  }
}

.zv_samples <- function(e) {
  if ("sample_id" %in% e$.arrays) {
    .zv_retrieve(e, "sample_id")
  } else {
    e$.attrs$sample_id %||% character(0)
  }
}

.zv_contigs <- function(e) {
  if ("contig_id" %in% e$.arrays) {
    .zv_retrieve(e, "contig_id")
  } else {
    ctg <- e$.attrs$contigs %||% e$.attrs$contig_id %||% character(0)
    if (is.list(ctg)) vapply(ctg, `[[`, character(1), "id") else as.character(ctg)
  }
}

.zv_filters <- function(e) {
  if ("filter_id" %in% e$.arrays) {
    .zv_retrieve(e, "filter_id")
  } else {
    e$.attrs$filter_id %||% character(0)
  }
}

.zv_variant_position <- function(e) .zv_retrieve(e, "variant_position")
.zv_variant_allele   <- function(e) .zv_retrieve(e, "variant_allele")

.zv_variant_contig <- function(e) {
  idx <- .zv_retrieve(e, "variant_contig") + 1L
  e$contigs()[idx]
}

.zv_genotypes <- function(e, variants, samples) {
  gt <- .zv_retrieve(e, "call_genotype")
  if (!is.null(variants)) gt <- gt[variants, , , drop = FALSE]
  if (!is.null(samples))  gt <- gt[, samples, , drop = FALSE]
  gt
}

.zv_call_genotype_phased <- function(e, variants, samples) {
  ph <- .zv_retrieve(e, "call_genotype_phased")
  if (!is.null(variants)) ph <- ph[variants, , drop = FALSE]
  if (!is.null(samples))  ph <- ph[, samples, drop = FALSE]
  ph
}

.zv_variant <- function(e, name) {
  .zv_retrieve(e, paste0("variant_", name))
}

.zv_call <- function(e, name) {
  .zv_retrieve(e, paste0("call_", name))
}

# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------

.zv_retrieve <- function(e, name) {
  if (!name %in% e$.arrays) {
    stop("array '", name, "' not found in this VCF Zarr store", call. = FALSE)
  }
  ZarrArray$open(e$.store, paste0("/", name))$retrieve()
}

`%||%` <- function(a, b) if (is.null(a)) b else a
