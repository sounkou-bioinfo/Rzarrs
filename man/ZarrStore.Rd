\name{ZarrStore}
\alias{ZarrStore}
\title{Local Zarr filesystem store}
\description{
  A handle to a Zarr store rooted at a local directory.
  Use \code{ZarrStore$open(path)} to create one, then pass it to
  \code{\link{ZarrArray}}\code{$open(store, path)} to open individual arrays.
}
\section{Constructor}{
  \describe{
    \item{\code{ZarrStore$open(path)}}{
      Open a local Zarr store.

      \describe{
        \item{\code{path}}{Character scalar. Path to the \file{.zarr} directory.}
      }
      Returns a \code{ZarrStore} object.
    }
  }
}
\section{Methods}{
  \describe{
    \item{\code{$path()}}{Return the store root path as a character scalar.}
  }
}
\seealso{
  \code{\link{ZarrObjectStore}}, \code{\link{ZarrArray}}
}
\examples{
zarr_path <- system.file("testdata", "int32.zarr", package = "Rzarrs")
store <- ZarrStore$open(zarr_path)
store$path()
}
