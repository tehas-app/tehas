#!/usr/bin/env bash
# Tests for install.sh: the arguments it takes. Anything but none, --check,
# --check --tests or --lab-build is refused with 64 before anything is built
# or written, so a typo of --check never installs.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

t_case "an argument install.sh does not take is refused with 64 and installs nothing"
for args in "--check--tests" "--chek" "--help" "--tests" "--tests --check" "--check --check" "--check --tests x" "--lab-build --check" "x"; do
  fresh
  instance
  t_run "$install $args"
  t_eq "$t_rc" 64
  t_contains "$t_out" "usage:"
  t_eq "$(ls -A "$home")" ""
  t_eq "$(cat "$T"/calls/* 2>/dev/null)" ""
done

t_done
