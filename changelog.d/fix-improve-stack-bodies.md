### Changes

- The improvement loop's analyst is given the body of an issue only when the
  issue carries `tehas:improvement` or is of the improvement type. Of any other
  issue it is given the title and labels, and the Cycles lines of its body are
  no longer read.
