library(Rzarrs)

int64_mock <- function(chunks_le, attrs = list()) {
  chunks <- if (.Platform$endian == "little") chunks_le else lapply(chunks_le, rev)
  raw <- as.raw(unlist(chunks, use.names = FALSE))
  x <- structure(
    readBin(raw, "double", n = length(chunks_le), size = 8L, endian = .Platform$endian),
    class = "Rzarrs_int64",
    storage = "i64-bitpattern"
  )
  for (name in names(attrs)) attr(x, name) <- attrs[[name]]
  x
}

zero <- int64_mock(list(c(0, 0, 0, 0, 0, 0, 0, 0)))
one <- int64_mock(list(c(1, 0, 0, 0, 0, 0, 0, 0)))
two <- int64_mock(list(c(2, 0, 0, 0, 0, 0, 0, 0)))
minus_one <- int64_mock(list(c(255, 255, 255, 255, 255, 255, 255, 255)))
max_i64 <- int64_mock(list(c(255, 255, 255, 255, 255, 255, 255, 127)))
min_i64 <- int64_mock(list(c(0, 0, 0, 0, 0, 0, 0, 128)))

x <- int64_mock(list(
  c(0, 0, 0, 0, 0, 0, 0, 0),
  c(1, 0, 0, 0, 0, 0, 0, 0),
  c(255, 255, 255, 255, 255, 255, 255, 255),
  c(255, 255, 255, 255, 255, 255, 255, 127)
))

expect_equal(as.character(x), c("0", "1", "-1", "9223372036854775807"))
expect_equal(x > zero, c(FALSE, TRUE, FALSE, TRUE))
expect_equal(x == x, rep(TRUE, 4L))
m <- structure(
  c(unclass(zero), unclass(one), unclass(minus_one)),
  class = "Rzarrs_int64",
  storage = "i64-bitpattern",
  dim = c(3L, 1L)
)
expect_equal(
  as.double(m),
  matrix(c(0, 1, -1), nrow = 3L)
)
expect_error(as.double(max_i64), "cannot be represented exactly")
expect_equal(as.character(one + one), "2")
expect_equal(as.character(one + 1L), "2")
expect_equal(as.character(two - one), "1")
expect_equal(as.character(two * minus_one), "-2")
expect_equal(as.character(min(x)), "-1")
expect_equal(as.character(max(x)), "9223372036854775807")
expect_equal(as.character(range(x)), c("-1", "9223372036854775807"))
expect_equal(as.character(sum(one, two, minus_one)), "2")
expect_equal(as.character(prod(two, minus_one)), "-2")
expect_equal(sign(x), c(0L, 1L, -1L, 1L))
expect_equal(as.character(abs(minus_one)), "1")
expect_error(max_i64 + one, "overflows signed 64-bit")
expect_error(abs(min_i64), "abs overflows")
expect_error(sqrt(one), "not integer-preserving")
