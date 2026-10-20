#!/usr/bin/env bash
# Tests for scripts/pack-release.sh: that the tarball holds what install.sh
# reads, that VERSION and the two pin lines say what the arguments said, that
# the .sha256 checks with sha256sum -c, and what a bad argument costs. The
# console is a stub file in every case: the pack script builds nothing and
# needs no cargo. The tree it packs is this checkout unless a case names
# another, so the case that checks what a tree missing one of those files costs
# builds a partial one in scratch.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

pack=$T_ROOT/scripts/pack-release.sh
manifest=$T_ROOT/lib/release.sh
dist=$T/dist
build=$T/build
tree=$T/tree
# The pin values a release is given, and the shape one of the two checks.
commit=0123456789abcdef0123456789abcdef01234567
digest=sha256:$(printf 'a%.0s' {1..64})
tar_gz=tehas-v9.9.9-linux-x86_64.tar.gz
# A console of one name and content, made once; a case that needs a particular
# content makes its own.
stub=$build/console
# The packed file list, read from the one place it is written down.
mapfile -t want_dirs < <(. "$manifest" && tehas_release_dirs)
mapfile -t want_plain < <(. "$manifest" && tehas_release_plain)

# A console is needed by almost every run, and the out dir is written fresh
# each time, so both are set up before the cases start.
mkdir -p "$build"
printf 'console built by the workflow\n' >"$stub"
chmod 755 "$stub"

# t_pack_reset empties the out dir; t_pack runs the script for v9.9.9 against
# this checkout, or against the tree TEHAS_PACK_ROOT names when the caller sets
# it, so a case says only what it changes.
t_pack_reset() { rm -rf "$dist"; }
t_pack() {
  local root=${TEHAS_PACK_ROOT:-$T_ROOT}
  t_run "TEHAS_PACK_ROOT='$root' \"$pack\" v9.9.9 '$dist' --console '$stub' $*"
}
# A tree with one real file in each directory the pack copies, and the top-level
# files taken from the script's own lists, so a fifth required input needs no
# change here.
make_tree() {
  rm -rf "$tree"
  mkdir -p "$tree/bin" "$tree/libexec" "$tree/lib" "$tree/host/sandbox"
  for f in "${want_plain[@]}"; do
    [ "$f" = install.sh ] && continue
    cp "$T_ROOT/$f" "$tree/$f"
  done
  cp "$T_ROOT/install.sh" "$tree/"
  cp "$T_ROOT/lib/release.sh" "$tree/lib/"
  cp "$T_ROOT/host/versions.env" "$tree/host/"
  cp "$T_ROOT/bin/tehas" "$tree/bin/"
  cp "$T_ROOT/libexec/dispatch" "$tree/libexec/"
  cp "$T_ROOT/lib/config.sh" "$tree/lib/"
  cp "$T_ROOT/host/sandbox/build.sh" "$tree/host/sandbox/"
}

t_case "a pack writes the tarball, its .sha256 and nothing else, and says both"
t_pack_reset
t_pack
t_eq "$t_rc" 0
t_eq "$(head -n 1 <<<"$t_out")" "$dist/$tar_gz"
hash=$(sha256sum <"$dist/$tar_gz" | awk '{print $1}')
t_contains "$t_out" "$hash"
t_eq "$(find "$dist" -maxdepth 1 -type f | wc -l)" 2

t_case "the tarball holds what install.sh reads, in one top directory"
t_pack_reset
t_pack
t_eq "$t_rc" 0
list=$(tar -tzf "$dist/$tar_gz" | LC_ALL=C sort)
# Every file of the two lists, and one real file of each directory: a release
# missing any of them stops on the adopter's host, so the list checked here is
# the one the script read rather than a copy of it kept in this file.
for f in "${want_plain[@]}"; do
  case $list in
    *"tehas-v9.9.9/$f"*) ;;
    *) t_bad "tehas-v9.9.9/$f is not in the tarball" ;;
  esac
done
for d in "${want_dirs[@]}"; do
  for f in "$T_ROOT/$d"/*; do
    [ -f "$f" ] || continue
    case $list in
      *"tehas-v9.9.9/${f#"$T_ROOT/"}"*) ;;
      *) t_bad "tehas-v9.9.9/${f#"$T_ROOT/"} is not in the tarball" ;;
    esac
  done
done
# Named by hand: the files an adopter's install and its host depend on.
for want in tehas-v9.9.9/bin/tehas tehas-v9.9.9/bin/tehasbot tehas-v9.9.9/libexec/dispatch \
  tehas-v9.9.9/libexec/work tehas-v9.9.9/libexec/console tehas-v9.9.9/lib/config.sh \
  tehas-v9.9.9/VERSION tehas-v9.9.9/host/versions.env \
  tehas-v9.9.9/host/sandbox/build.sh tehas-v9.9.9/host/docs/tools.md \
  tehas-v9.9.9/host/release/minisign.pub; do
  case $list in
    *"$want"*) ;;
    *) t_bad "$want is not in the tarball" ;;
  esac
done
# One top directory, so unpacking leaves tehas-v9.9.9/ and nothing beside it.
t_eq "$(cut -d/ -f1 <<<"$list" | LC_ALL=C sort -u | tr '\n' ' ')" "tehas-v9.9.9 "
# What the pack does not copy stays out. It copies only the directories
# lib/release.sh names, so a top-level directory of the tree cannot reach the
# tarball. The tree's git record is the one thing that would: a worktree carries
# a .git file rather than a directory, and a tag checked out elsewhere has
# neither, so the tree of this case is made to look like a checkout first.
git -C "$T_ROOT" rev-parse --git-common-dir >/dev/null || t_skip "no git to look like"
rm -rf "$tree"
mkdir -p "$tree"
printf 'gitdir: /somewhere/else\n' >"$tree/.git"
list=$(tar -tzf "$dist/$tar_gz" | LC_ALL=C sort)
for absent in .git/ .github/ .pi/; do
  case $list in
    *"tehas-v9.9.9/$absent"*) t_bad "$absent is in the tarball" ;;
  esac
done

t_case "the console an adopter gets is the one --console named, and is executable"
t_pack_reset
printf 'the console of this release\n' >"$build/other"
chmod 755 "$build/other"
t_run "TEHAS_PACK_ROOT='$T_ROOT' \"$pack\" v9.9.9 '$dist' --console '$build/other'"
t_eq "$t_rc" 0
t_eq "$(tar -xzf "$dist/$tar_gz" -O tehas-v9.9.9/libexec/console)" "the console of this release"
rm -rf "$T/unpacked"
tar -xzf "$dist/$tar_gz" -C "$T"
for f in libexec/console bin/tehas host/sandbox/build.sh; do
  # install.sh compares an installed file's mode, so what is packed executable
  # has to stay executable through the tarball.
  [ -x "$T/tehas-v9.9.9/$f" ] || t_bad "$f is not executable in the tarball"
done
t_eq "$(stat -c '%a' "$T/tehas-v9.9.9/libexec/console")" 755

t_case "VERSION holds the tag"
t_pack_reset
t_pack
t_eq "$t_rc" 0
t_eq "$(tar -xzf "$dist/$tar_gz" -O tehas-v9.9.9/VERSION)" "v9.9.9"

t_case "without pins, versions.env is the checkout's own"
t_pack_reset
t_pack
t_eq "$t_rc" 0
t_eq "$(tar -xzf "$dist/$tar_gz" -O tehas-v9.9.9/host/versions.env)" \
  "$(cat "$T_ROOT/host/versions.env")"

t_case "the pins are the last lines, and what the release does not pin is kept"
t_pack_reset
t_pack --rukas-commit "$commit" --rukas-digest "$digest"
t_eq "$t_rc" 0
packed=$(tar -xzf "$dist/$tar_gz" -O tehas-v9.9.9/host/versions.env)
t_contains "$packed" "PI_RUKAS_FOLLOW=commit:$commit"
t_contains "$packed" "PI_RUKAS_IMAGE_DIGEST=$digest"
# Every version the release does not pin is still in the file it ships with.
t_contains "$packed" "NODE_VERSION="
t_contains "$packed" "PI_RUKAS_REPO="
t_eq "$(grep -c '^PI_RUKAS_FOLLOW=' <<<"$packed")" 1
t_eq "$(grep -c '^PI_RUKAS_IMAGE_DIGEST=' <<<"$packed")" 1
t_eq "$(tail -n 2 <<<"$packed")" \
  "$(printf 'PI_RUKAS_FOLLOW=commit:%s\nPI_RUKAS_IMAGE_DIGEST=%s' "$commit" "$digest")"

t_case "a key the tree already carries is pinned once, not twice"
# What the rewrite is for: two lines for one key leave the value a host reads up
# to which lands last. This tree's file ends without a newline, which the pin
# must not be glued to.
t_pack_reset
make_tree
printf 'PI_RUKAS_IMAGE_DIGEST=sha256:%s\n' "$(printf 'b%.0s' {1..64})" >>"$tree/host/versions.env"
printf 'LAST_LINE_WITHOUT_A_NEWLINE=x' >>"$tree/host/versions.env"
TEHAS_PACK_ROOT=$tree t_pack --rukas-commit "$commit" --rukas-digest "$digest"
t_eq "$t_rc" 0
packed=$(tar -xzf "$dist/$tar_gz" -O tehas-v9.9.9/host/versions.env)
t_eq "$(grep -c '^PI_RUKAS_IMAGE_DIGEST=' <<<"$packed")" 1
t_contains "$packed" "PI_RUKAS_IMAGE_DIGEST=$digest"
t_eq "$(grep -c '^LAST_LINE_WITHOUT_A_NEWLINE=x$' <<<"$packed")" 1
t_eq "$(tail -n 1 <<<"$packed")" "PI_RUKAS_IMAGE_DIGEST=$digest"

t_case "the tag must be vX.Y.Z, and the arguments it needs are named"
t_pack_reset
t_run "\"$pack\" release-v9.9.9 '$dist' --console '$stub'"
t_eq "$t_rc" 64
t_contains "$t_out" "must be vX.Y.Z"
t_run "'$pack' --console '$stub'"
t_eq "$t_rc" 64
t_contains "$t_out" "name the tag"
t_run "\"$pack\" v9.9.9"
t_eq "$t_rc" 64
t_contains "$t_out" "name the output directory"

t_case "the console is needed, and must be a file the pack can run"
t_pack_reset
t_run "\"$pack\" v9.9.9 '$dist'"
t_eq "$t_rc" 64
t_contains "$t_out" "--console names the console"
t_run "\"$pack\" v9.9.9 '$dist' --console"
t_eq "$t_rc" 64
t_contains "$t_out" "--console needs a path"
mkdir -p "$build/inside"
t_run "\"$pack\" v9.9.9 '$dist' --console '$build/inside'"
t_eq "$t_rc" 64
t_contains "$t_out" "no console at"
printf 'not a binary\n' >"$build/plain"
chmod 644 "$build/plain"
t_run "\"$pack\" v9.9.9 '$dist' --console '$build/plain'"
t_eq "$t_rc" 64
t_contains "$t_out" "not executable"

t_case "half a pin is a bad argument, a pin of the wrong shape is a failure"
t_pack_reset
t_pack --rukas-commit "$commit"
t_eq "$t_rc" 64
t_contains "$t_out" "name both pins or neither"
t_pack --rukas-digest "$digest"
t_eq "$t_rc" 64
t_contains "$t_out" "name both pins or neither"
t_pack --rukas-commit deadbeef --rukas-digest "$digest"
t_eq "$t_rc" 1
t_contains "$t_out" "40 hex digits"
t_pack --rukas-commit "$commit" --rukas-digest sha256:abc
t_eq "$t_rc" 1
t_contains "$t_out" "64 hex digits"
# A run that was refused wrote nothing at all.
t_eq "$(find "$dist" -maxdepth 1 -type f 2>/dev/null | wc -l)" 0

t_case "an argument it does not know, or a third one, is a bad argument"
t_pack_reset
t_pack --consolex yes
t_eq "$t_rc" 64
t_contains "$t_out" "unknown argument"
t_run "\"$pack\" v8.8.8 v9.9.9 '$dist' --console '$stub'"
t_eq "$t_rc" 64
t_contains "$t_out" "too many arguments"
t_run "\"$pack\" --help"
t_eq "$t_rc" 0
t_contains "$t_out" "Usage: scripts/pack-release.sh"

t_case "a tree that is not a tree is a bad argument, named by its path"
t_pack_reset
TEHAS_PACK_ROOT=$build/inside t_pack
t_eq "$t_rc" 64
t_contains "$t_out" "no install.sh in the tree to pack: $build/inside"
t_eq "$(find "$dist" -maxdepth 1 -type f 2>/dev/null | wc -l)" 0

t_case "a tree missing what install.sh reads is refused, and writes nothing"
t_pack_reset
make_tree
# The one thing that tree does not have is named, with the tree it was read from.
rm -f "$tree/bootstrap.sh"
TEHAS_PACK_ROOT=$tree t_pack
t_eq "$t_rc" 1
t_contains "$t_out" "bootstrap.sh is missing from $tree"
t_eq "$(find "$dist" -maxdepth 1 -type f 2>/dev/null | wc -l)" 0

t_case "a tree without the list to pack is refused, and names itself"
t_pack_reset
make_tree
rm -f "$tree/lib/release.sh"
TEHAS_PACK_ROOT=$tree t_pack
t_eq "$t_rc" 1
t_contains "$t_out" "lib/release.sh is missing from $tree"

t_case "that same tree with the file it was missing packs"
make_tree
cp "$T_ROOT/bootstrap.sh" "$tree/bootstrap.sh"
TEHAS_PACK_ROOT=$tree t_pack
t_eq "$t_rc" 0
t_eq "$(tar -tzf "$dist/$tar_gz" | cut -d/ -f2 | grep -v '^$' | LC_ALL=C sort -u | tr '\n' ' ')" \
  "AGENTS.md LICENSE NOTICE VERSION bin bootstrap.sh host install.sh lib libexec "

t_case "the tree's own build of the console is refused, by path and by content"
t_pack_reset
make_tree
printf 'a console built in the tree\n' >"$tree/libexec/console"
chmod 755 "$tree/libexec/console"
# The tree's own file is the tree's build whoever names it, and so is a copy of
# it byte for byte: both are refused before anything is copied.
TEHAS_PACK_ROOT=$tree t_pack --console "$tree/libexec/console"
t_eq "$t_rc" 64
t_contains "$t_out" "is the tree's own build of it"
cp "$tree/libexec/console" "$build/same"
TEHAS_PACK_ROOT=$tree t_pack --console "$build/same"
t_eq "$t_rc" 64
t_contains "$t_out" "is the tree's own build of it"
# And the tree's console is not what an adopter gets when another is given.
t_pack_reset
TEHAS_PACK_ROOT=$tree t_pack --console "$stub"
t_eq "$t_rc" 0
t_eq "$(tar -xzf "$dist/$tar_gz" -O tehas-v9.9.9/libexec/console)" \
  "console built by the workflow"

t_case "the .sha256 is in sha256sum's format, checks, and fails on a changed tarball"
t_pack_reset
t_pack
t_eq "$t_rc" 0
# The hash of the file on disk, not of what the run printed: the line has to
# name the tarball as the file the adopter holds, so a tarball that differs
# from what was printed is found here rather than by comparing two hashes.
hash=$(sha256sum <"$dist/$tar_gz" | awk '{print $1}')
t_eq "$(cat "$dist/$tar_gz.sha256")" "$hash  $tar_gz"
# The name in the line is relative to the directory the hash is taken in, so
# the check that reads it runs from that directory and nothing else.
t_run "cd '$dist' && sha256sum -c '$tar_gz.sha256'"
t_eq "$t_rc" 0
t_contains "$t_out" ": OK"
printf 'tampered\n' >>"$dist/$tar_gz"
t_run "cd '$dist' && sha256sum -c '$tar_gz.sha256'"
t_eq "$t_rc" 1
t_contains "$t_out" "FAILED"

t_case "a tarball already packed for the tag is not overwritten"
t_pack_reset
t_pack
t_eq "$t_rc" 0
before=$(sha256sum <"$dist/$tar_gz" | awk '{print $1}')
printf 'a different console\n' >"$build/second"
chmod 755 "$build/second"
t_run "TEHAS_PACK_ROOT='$T_ROOT' \"$pack\" v9.9.9 '$dist' --console '$build/second'"
t_eq "$t_rc" 1
t_contains "$t_out" "already exists"
t_eq "$(sha256sum <"$dist/$tar_gz" | awk '{print $1}')" "$before"

t_case "an out dir that cannot be made is a bad argument"
t_run "TEHAS_PACK_ROOT='$T_ROOT' \"$pack\" v9.9.9 '$build/plain/nested' --console '$stub'"
t_eq "$t_rc" 64
t_contains "$t_out" "cannot make"

t_case "the names the release workflow is written against are the ones this writes"
# The workflow is a person's file and is not in the gate, so the names the two
# halves share are pinned from the manual's side: the fenced block it shows for
# the workflow's own command is run with only its placeholders filled, and it
# writes the file that block names. The block is broken across its lines with a
# backslash, so the backslashes and the newlines go and runs of spaces come to
# one. The environment prefix and the command's own name are taken off as far as
# the first placeholder, so the test does not depend on how the manual quotes
# either of them.
manual=$T_ROOT/host/docs/tools.md
t_eq "$(grep -c 'tehas-<tag>-linux-x86_64.tar.gz' "$manual")" 1
start=$(grep -n "^TEHAS_PACK_ROOT='<checkout>' scripts/pack-release.sh <tag>" "$manual" | cut -d: -f1)
end=$(awk -v n="$start" 'NR > n && /^```$/ { print NR; exit }' "$manual")
args=$(sed -n "${start},$((end - 1))p" "$manual" | tr -d '\\\n' | tr -s ' ')
[ -n "$args" ] || t_bad "the manual names no pack command to run"
args=${args#*'<tag>'}
args=${args//'<out dir>'/"'$dist'"}
args=${args//'<path>'/"'$stub'"}
args=${args//'<40 hex>'/"$commit"}
args=${args//'<64 hex>'/"$digest"}
t_pack_reset
t_run "TEHAS_PACK_ROOT='$T_ROOT' \"$pack\" v9.9.9 $args"
t_eq "$t_rc" 0
t_eq "$(head -n 1 <<<"$t_out")" "$dist/$tar_gz"

t_done
