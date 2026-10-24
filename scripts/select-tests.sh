#!/usr/bin/env bash
# Name the checks a change needs, from the paths it touched. It is the
# selection behind `scripts/verify.sh --changed`, the gate a developer runs
# while working. The full gate stays what the driver and CI run.
#
#   scripts/select-tests.sh <path>...
#
# Each path is relative to the root of the checkout. One line per check:
#
#   full <why>    the whole gate is needed; it is then the only line
#   test <file>   a shell test file, test/*_test.sh
#   node <file>   a front page test file, test/*_test.mjs
#   cargo         the workspace's tests
#   omit <file>   a heavy file left to the full gate (see below)
#
# How a path maps:
#
#   - scripts/test.sh, scripts/verify.sh, this file and test/lib.sh: full.
#   - A test file: itself.
#   - console/: cargo, and the shell tests that name console/ or build the
#     store (t_store).
#   - A helper under test/ or a fixture under test/fixtures/<name>/: the test
#     files that name it.
#   - Any other path: the test files that name its file name as a word, or its
#     directory when that has a slash in it (host/docs). A helper that names it
#     brings in the test files that name the helper.
#   - Nothing names it: no check for docs/ and the top-level *.md files, which
#     are not installed, and full for anything else.
#
# The heavy integration files, test/install_*, test/update_*,
# test/pack_release_test.sh and test/bootstrap_test.sh, are left to the full
# gate unless the change touches what they test directly: the file itself, a
# helper it uses, or a path whose name starts its name (install.sh and
# host/release/install for install_*, libexec/update for update_*,
# scripts/pack-release.sh, bootstrap.sh). Reached only through a shared library
# such as lib/config.sh, they are omitted and named in an omit line.
#
# When unsure it asks for more, never less. A test that reaches a file by a
# name built at run time is missed here; the full gate the driver and CI run
# still has it.
#
# Exit: 0 the lines were printed, 64 no path was given.
set -uo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root" || exit 1
[ "$#" -gt 0 ] || {
  echo "usage: scripts/select-tests.sh <path>..." >&2
  exit 64
}

tests=() helpers=()
for f in test/*_test.sh test/*_test.mjs; do
  [ -f "$f" ] && tests+=("$f")
done
for f in test/*.sh; do
  case "$f" in test/lib.sh | *_test.sh) ;; *) [ -f "$f" ] && helpers+=("$f") ;; esac
done

picked=() cargo=0 direct=" "
pick() { picked+=("$@"); }
heavy() { # file
  case "${1##*/}" in install_*_test.sh | update_*_test.sh | update_test.sh | pack_release_test.sh | bootstrap_test.sh) return 0 ;; esac
  return 1
}
# The heavy files whose name starts with <path>'s, which it tests directly.
subject() { # path
  local stem=${1##*/} f
  stem=${stem%%.*}
  stem=${stem//-/_}
  for f in "${tests[@]}"; do
    case "${f##*/}" in "${stem}"_test.sh | "${stem}"_*_test.sh) direct+="$f " ;; esac
  done
}
full() {
  echo "full $1"
  exit 0
}

# The files of <file>... that name <path>. A command's bare name is a common
# word (install, update, work), so it counts only after its directory
# (libexec/update), after the command that runs it (tehas update), or as the
# start of a test file's name (update_release_test.sh). A file name with a dot
# in it is specific enough to count as a word on its own.
names() { # path file...
  local p=$1 base parent stem f
  shift
  [ "$#" -gt 0 ] || return 0
  base=${p##*/}
  stem=${base%%.*}
  stem=${stem//-/_}
  [[ $base != *.* ]] || grep -lwF -e "$base" -- "$@" 2>/dev/null
  if [ "$base" != "$p" ]; then
    parent=${p%/*}
    parent=${parent##*/}
    grep -lF -e "$parent/$base" -- "$@" 2>/dev/null
  fi
  case "$p" in
    libexec/* | bin/*)
      grep -lE -e "tehas(bot)? $base([^a-z_-]|$)" -e 'libexec/["$]' -- "$@" 2>/dev/null
      ;;
  esac
  for f in "$@"; do
    case "${f##*/}" in "${stem}"_test.sh | "${stem}"_*_test.sh | "${stem}"_test.mjs) echo "$f" ;; esac
  done
}

# The test files that name <path>, and those that name a helper that names it.
naming() { # path
  local h
  names "$1" "${tests[@]}"
  for h in $(names "$1" "${helpers[@]}"); do
    grep -lwF -e "${h##*/}" -- "${tests[@]}" 2>/dev/null
  done
}

for p in "$@"; do
  p=${p#./}
  case "$p" in
    scripts/test.sh | scripts/verify.sh | scripts/select-tests.sh | test/lib.sh)
      full "$p is part of the gate itself"
      ;;
    test/*_test.sh | test/*_test.mjs)
      [ ! -f "$p" ] || pick "$p"
      direct+="$p "
      continue
      ;;
    console/*)
      cargo=1
      mapfile -t hit < <(grep -lF -e "console/" -e "t_store" -- "${tests[@]}" 2>/dev/null)
      pick "${hit[@]}"
      continue
      ;;
    test/fixtures/*)
      name=${p#test/fixtures/}
      p=test/fixtures/${name%%/*}
      ;;
  esac
  subject "$p"
  mapfile -t hit < <(naming "$p")
  if [[ $p == test/*.sh ]]; then
    for f in "${hit[@]}"; do direct+="$f "; done
  fi
  dir=${p%/*}
  if [ "$dir" != "$p" ] && [[ $dir == */* ]]; then
    mapfile -t more < <(naming "$dir")
    hit+=("${more[@]}")
  fi
  if [ "${#hit[@]}" -gt 0 ]; then
    pick "${hit[@]}"
    continue
  fi
  [[ $p == docs/* || ($p == *.md && $p != */*) ]] && continue
  full "no test names $p"
done

[ "$cargo" = 0 ] || echo cargo
[ "${#picked[@]}" -eq 0 ] && exit 0
printf '%s\n' "${picked[@]}" | LC_ALL=C sort -u | while read -r f; do
  if heavy "$f" && [[ $direct != *" $f "* ]]; then
    echo "omit $f"
    continue
  fi
  case "$f" in
    *.mjs) echo "node $f" ;;
    *) echo "test $f" ;;
  esac
done
