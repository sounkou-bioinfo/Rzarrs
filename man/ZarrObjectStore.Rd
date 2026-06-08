\name{ZarrObjectStore}
\alias{ZarrObjectStore}
\title{Object-store Zarr backend (S3, GCS, Azure, HTTP/HTTPS)}
\description{
  A handle to a Zarr store backed by any URL scheme supported by the
  \href{https://crates.io/crates/object_store}{object_store} crate:
  \code{s3://}, \code{gs://}, \code{az://}, \code{https://}, or
  \code{file:///}.
  URLs ending in ile{.zarr.zip} or ile{.zip} are opened as zip objects
  when the Rust ile{zip} feature is enabled.

  Credentials are read automatically from the standard environment
  variables for each provider (the same ones used by the AWS CLI,
  \code{gsutil}, \code{azcopy}, etc.).  No credentials are ever passed
  as R objects.
}
\section{Constructor}{
  \describe{
    \item{\code{ZarrObjectStore$open(url)}}{
      Open an object-store backend.

      \describe{
        \item{\code{url}}{Character scalar.  Store URL,
          e.g. \code{"s3://my-bucket/path/to/store.zarr"},
          \code{"s3://my-bucket/path/to/store.zarr.zip"}, or
          \code{"https://example.com/store.zarr"}.}
      }
      Returns a \code{ZarrObjectStore} object.
    }
  }
}
\section{Methods}{
  \describe{
    \item{\code{$url()}}{Return the URL passed to \code{open()} as a character scalar.}
  }
}
\section{Credentials}{
  Set the relevant environment variables \emph{before} calling \code{open()}:
  \describe{
    \item{AWS S3}{\env{AWS_ACCESS_KEY_ID}, \env{AWS_SECRET_ACCESS_KEY},
      \env{AWS_REGION}, \env{AWS_ENDPOINT_URL}}
    \item{Google GCS}{\env{GOOGLE_APPLICATION_CREDENTIALS}}
    \item{Azure Blob}{\env{AZURE_STORAGE_ACCOUNT},
      \env{AZURE_STORAGE_ACCESS_KEY}}
  }
}
\seealso{
  \code{\link{ZarrStore}}, \code{\link{ZarrArray}}
}
\examples{
\dontrun{
store <- ZarrObjectStore$open("s3://my-bucket/my.zarr")
arr   <- ZarrArray$open_object_store(store, "/")
arr$shape()
}
}
