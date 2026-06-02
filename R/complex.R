## Complex storage helpers for Zarr native complex dtypes.

#' @export
as.double.Rzarrs_complex64 <- function(x, ...) {
  if (any(Im(x) != 0)) {
    warning("imaginary part is discarded by as.double() for Rzarrs_complex64", call. = FALSE)
  }
  out <- Re(x)
  dim(out) <- dim(x)
  class(out) <- NULL
  out
}

#' @export
Summary.Rzarrs_complex64 <- function(..., na.rm = FALSE) {
  if (.Generic %in% c("sum", "prod")) {
    return(NextMethod())
  }

  stop(
    "Summary operation '", .Generic, "' is not implemented for Rzarrs_complex64",
    call. = FALSE
  )
}

#' @export
as.double.Rzarrs_complex128 <- function(x, ...) {
  if (any(Im(x) != 0)) {
    warning("imaginary part is discarded by as.double() for Rzarrs_complex128", call. = FALSE)
  }
  out <- Re(x)
  dim(out) <- dim(x)
  class(out) <- NULL
  out
}

#' @export
Summary.Rzarrs_complex128 <- function(..., na.rm = FALSE) {
  if (.Generic %in% c("sum", "prod")) {
    return(NextMethod())
  }

  stop(
    "Summary operation '", .Generic, "' is not implemented for Rzarrs_complex128",
    call. = FALSE
  )
}
