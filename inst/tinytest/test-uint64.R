library(Rzarrs)

uint64_mock <- function(chunks_le) {
  chunks <- if (.Platform$endian == "little") chunks_le else lapply(chunks_le, rev)
  raw <- as.raw(unlist(chunks, use.names = FALSE))
  structure(
    readBin(raw, "double", n = length(chunks_le), size = 8L, endian = .Platform$endian),
    class = "Rzarrs_uint64",
    storage = "u64-bitpattern"
  )
}

u <- uint64_mock(list(
  c(0, 0, 0, 0, 0, 0, 0, 0),                         # 0
  c(1, 0, 0, 0, 0, 0, 0, 0),                         # 1
  c(255, 255, 255, 255, 255, 255, 255, 255),         # 2^64 - 1
  c(0, 0, 0, 0, 0, 0, 0, 128)                        # 2^63
))
one <- uint64_mock(list(c(1, 0, 0, 0, 0, 0, 0, 0)))
two <- uint64_mock(list(c(2, 0, 0, 0, 0, 0, 0, 0)))

m <- structure(c(unclass(u[1]), unclass(u[2])), class = "Rzarrs_uint64", storage = "u64-bitpattern", dim = c(1L, 2L))

expect_equal(as.character(u), c("0", "1", "18446744073709551615", "9223372036854775808"))
expect_equal(as.double(m), matrix(c(0, 1), ncol = 2L))
expect_equal(u > one, c(FALSE, FALSE, TRUE, TRUE))
expect_equal(u == u, rep(TRUE, 4L))
expect_equal(as.character(one + one), "2")
expect_equal(as.character(one + 1L), "2")
expect_equal(as.character(two - one), "1")
expect_equal(as.character(two * two), "4")
expect_equal(as.character(min(u)), "0")
expect_equal(as.character(max(u)), "18446744073709551615")
expect_equal(as.character(range(u)), c("0", "18446744073709551615"))
expect_equal(as.character(sum(one, two)), "3")
expect_equal(as.character(prod(two, two)), "4")
expect_equal(sign(u), c(0L, 1L, 1L, 1L))
expect_equal(as.character(abs(u)), as.character(u))
expect_error(as.double(u), "cannot be represented exactly")
expect_error(u + one, "overflows unsigned 64-bit")
expect_error(one - two, "negative value")
expect_error(sqrt(one), "not integer-preserving")
