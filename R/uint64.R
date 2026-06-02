## Package-owned fixed-width 64-bit numeric vectors.
##
## Storage is a REALSXP carrying raw 64-bit bit patterns, with all user-facing
## numeric work delegated back to Rust so operations stay exact and checked.

rzarrs_recycle_warning <- function(e1, e2) {
  lx <- length(e1)
  ly <- length(e2)
  if (lx != ly && lx != 1L && ly != 1L && lx != 0L && ly != 0L) {
    warning("longer object length is not a multiple of shorter object length", call. = FALSE)
  }
}

rzarrs_concat_fixed64 <- function(args, class, storage) {
  if (!all(vapply(args, inherits, logical(1), what = class))) {
    stop("Summary for Rzarrs 64-bit vectors requires all arguments to have the same class", call. = FALSE)
  }
  structure(
    unlist(lapply(args, unclass), use.names = FALSE),
    class = class,
    storage = storage
  )
}

rzarrs_print_fixed64 <- function(x, label, ...) {
  dims <- dim(x)
  extent <- if (is.null(dims)) length(x) else paste(dims, collapse = " x ")
  cat("<", label, "[", extent, "]>\n", sep = "")
  print(format(x, ...), quote = FALSE)
  invisible(x)
}

#' @export
as.character.Rzarrs_int64 <- function(x, ...) {
  rzarrs_int64_values(x)
}

#' @export
as.double.Rzarrs_int64 <- function(x, ...) {
  rzarrs_int64_to_double(x)
}

#' @export
is.na.Rzarrs_int64 <- function(x) {
  rzarrs_int64_is_na(x)
}

#' @export
format.Rzarrs_int64 <- function(x, ...) {
  as.character(x)
}

#' @export
print.Rzarrs_int64 <- function(x, ...) {
  rzarrs_print_fixed64(x, "Rzarrs_int64", ...)
}

#' @export
Ops.Rzarrs_int64 <- function(e1, e2) {
  rzarrs_recycle_warning(e1, e2)
  rzarrs_int64_op(e1, e2, .Generic)
}

#' @export
Math.Rzarrs_int64 <- function(x, ...) {
  rzarrs_int64_math(x, .Generic)
}

#' @export
Summary.Rzarrs_int64 <- function(..., na.rm = FALSE) {
  args <- list(...)
  if (any(vapply(args, function(x) !is.null(attr(x, "zarr_dtype")), logical(1)))) {
    stop("Summary is not implemented for numpy datetime64/timedelta64 int64 payloads", call. = FALSE)
  }
  x <- rzarrs_concat_fixed64(args, "Rzarrs_int64", "i64-bitpattern")
  rzarrs_int64_summary(x, .Generic, na.rm)
}

#' @export
chooseOpsMethod.Rzarrs_int64 <- function(x, y, mx, my, cl, reverse) {
  TRUE
}

#' @export
as.character.Rzarrs_uint64 <- function(x, ...) {
  rzarrs_uint64_values(x)
}

#' @export
as.double.Rzarrs_uint64 <- function(x, ...) {
  rzarrs_uint64_to_double(x)
}

#' @export
is.na.Rzarrs_uint64 <- function(x) {
  rzarrs_uint64_is_na(x)
}

#' @export
format.Rzarrs_uint64 <- function(x, ...) {
  as.character(x)
}

#' @export
print.Rzarrs_uint64 <- function(x, ...) {
  rzarrs_print_fixed64(x, "Rzarrs_uint64", ...)
}

#' @export
Ops.Rzarrs_uint64 <- function(e1, e2) {
  rzarrs_recycle_warning(e1, e2)
  rzarrs_uint64_op(e1, e2, .Generic)
}

#' @export
Math.Rzarrs_uint64 <- function(x, ...) {
  rzarrs_uint64_math(x, .Generic)
}

#' @export
Summary.Rzarrs_uint64 <- function(..., na.rm = FALSE) {
  args <- list(...)
  x <- rzarrs_concat_fixed64(args, "Rzarrs_uint64", "u64-bitpattern")
  rzarrs_uint64_summary(x, .Generic, na.rm)
}

#' @export
chooseOpsMethod.Rzarrs_uint64 <- function(x, y, mx, my, cl, reverse) {
  TRUE
}
