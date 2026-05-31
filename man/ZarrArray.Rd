\name{ZarrArray}
\alias{ZarrArray}
\title{Zarr array handle}
\description{
  A handle to a single Zarr array within a store.  Arrays can be opened
  from a local store (\code{\link{ZarrStore}}) or an object-store backend
  (\code{\link{ZarrObjectStore}}).

  Indexing follows R conventions: indices are \strong{1-based and inclusive}
  on both ends.
}
\section{Constructors}{
  \describe{
    \item{\code{ZarrArray$open(store, path)}}{
      Open from a local \code{\link{ZarrStore}}.
      \describe{
        \item{\code{store}}{A \code{ZarrStore} object.}
        \item{\code{path}}{Character scalar. Array path within the store,
          e.g. \code{"/"} for the root array.}
      }
    }
    \item{\code{ZarrArray$open_object_store(store, path)}}{
      Open from a \code{\link{ZarrObjectStore}}.
      Same arguments as above but \code{store} is a \code{ZarrObjectStore}.
    }
  }
}
\section{Methods}{
  \describe{
    \item{\code{$ndim()}}{Number of dimensions (integer scalar).}
    \item{\code{$shape()}}{Array shape (integer vector, one element per dimension).}
    \item{\code{$chunk_shape()}}{Chunk shape (integer vector), or \code{NULL} for
      irregular chunk grids.}
    \item{\code{$dtype()}}{Data type name in Zarr V3 canonical form, e.g.
      \code{"float32"}, \code{"int32"}, \code{"bool"} (character scalar).}
    \item{\code{$dimension_names()}}{Dimension names (character vector), or
      \code{NULL} if none are stored.}
    \item{\code{$metadata_json()}}{Full array metadata as pretty-printed JSON
      (character scalar).}
    \item{\code{$retrieve(starts = NULL, ends = NULL)}}{
      Read array data into R.

      \describe{
        \item{\code{starts}}{Integer vector of \strong{1-based} start indices
          (one per dimension), or \code{NULL} to start at the first element.}
        \item{\code{ends}}{Integer vector of \strong{1-based inclusive} end
          indices (one per dimension), or \code{NULL} to include the last element.}
      }

      Returns a vector of the appropriate R type (\code{integer},
      \code{double}, or \code{logical}) with a \code{dim} attribute set to
      the shape of the requested region.  Data are returned in R (Fortran /
      column-major) order so that standard R array indexing works as expected.
    }
  }
}
\seealso{
  \code{\link{ZarrStore}}, \code{\link{ZarrObjectStore}}
}
\examples{
zarr_path <- system.file("testdata", "int32.zarr", package = "Rzarrs")
arr <- ZarrArray$open(ZarrStore$open(zarr_path), "/")

arr$ndim()        # 2
arr$shape()       # c(4L, 6L)
arr$dtype()       # "int32"

# Full array
full <- arr$retrieve()
dim(full)         # c(4L, 6L)
full[1, 6]        # 6

# Subset: rows 1-2, cols 1-3
sub <- arr$retrieve(c(1L, 1L), c(2L, 3L))
dim(sub)          # c(2L, 3L)
}
