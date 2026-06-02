#' Report Rzarrs codec capabilities
#'
#' With no argument, returns the codecs compiled into this Rzarrs build.  When
#' passed a `ZarrArray`, returns support status for the codecs declared by that
#' array, including nested sharding codecs.
#'
#' @param x Optional `ZarrArray`.
#' @return A named list with `codec` and `supported` vectors.
#' @export
codec_capabilities <- function(x = NULL) {
  if (is.null(x)) return(rzarrs_codec_capabilities())
  if (inherits(x, "ZarrArray")) return(x$codec_capabilities())
  stop("x must be NULL or a ZarrArray", call. = FALSE)
}

#' Plan R materialization for a Zarr dtype
#'
#' @param dtype Character scalar dtype name.
#' @return A named list describing the planned R materialization.
#' @export
dtype_plan <- function(dtype) {
  rzarrs_dtype_plan(dtype)
}
