### Changes

- `tehas stop` on a pull request's number, or a number with no issue, says `#12 is not an issue; tehas stops only issues` and exits 1, where it said the labels could not be read. tehas reads such a number as no issue now: 1.x read a pull request's number as an issue's.
