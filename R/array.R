## Hand-written R layer on top of the savvy-generated wrappers.
##
## Two responsibilities:
##   1. C-order → Fortran-order: zarrs returns data in row-major (C) order;
##      R arrays are column-major (Fortran). We reverse dims and aperm so
##      that arr[i, j, ...] gives the expected element.
##   2. 1-based inclusive indexing: R users count from 1.  `starts` and `ends`
##      passed to $retrieve() are 1-based and inclusive; we convert to
##      0-based exclusive before forwarding to Rust.

## Override the savvy-generated closure factory so every new ZarrArray object
## gets the patched retrieve method (000-wrappers.R is loaded first by name,
## so this definition wins).
`ZarrArray_retrieve` <- function(self) {
  function(starts = NULL, ends = NULL) {

    ## --- index conversion ---------------------------------------------------
    if (!is.null(starts)) {
      starts <- as.integer(starts) - 1L          # 1-based → 0-based
      ends   <- as.integer(ends)                  # 1-based inclusive = 0-based exclusive
    }

    ## --- raw retrieval (C-order, dim set in C-order) ------------------------
    out <- .Call(savvy_ZarrArray_retrieve__impl, self, starts, ends)

    ## --- C-order → Fortran-order --------------------------------------------
    d <- dim(out)
    if (!is.null(d) && length(d) > 1L) {
      ## Set dim in reversed order then permute axes so R indexing is natural.
      dim(out) <- rev(d)
      out <- aperm(out, rev(seq_along(d)))
    }

    out
  }
}
