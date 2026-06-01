## Thin R compatibility layer on top of savvy-generated wrappers.
##
## All indexing, casting, dimension handling, and C-order -> R-order conversion
## are implemented in Rust.  This file only provides default NULL arguments so
## users can call `arr$retrieve()` instead of `arr$retrieve(NULL, NULL)`.

`ZarrArray_retrieve` <- function(self) {
  function(starts = NULL, ends = NULL) {
    .Call(savvy_ZarrArray_retrieve__impl, self, starts, ends)
  }
}
