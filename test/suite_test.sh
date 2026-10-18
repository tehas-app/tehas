#!/usr/bin/env bash
# Tests for the test files themselves. A test file run where test/lib.sh is not
# beside it, a copy in /tmp say, has no scratch directory: $T is empty, and a
# path built on it, such as "$T/home", is a real one. Each file stops there
# instead. Checked by reading the files: running one without its library is the
# very thing this guards against.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

t_case "every test file stops when it cannot load its library"
for f in "$T_ROOT"/test/*_test.sh; do
  first=$(grep -m1 -E '^\. .*lib\.sh' "$f")
  t_eq "${f##*/}: $first" "${f##*/}: . \"\$(dirname \"\$0\")/lib.sh\" || exit 1"
done
t_done
