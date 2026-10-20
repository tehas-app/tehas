#!/usr/bin/env bash
# Tests for scripts/pack-release.sh: that the tarball holds what install.sh
# reads, that VERSION and the two pin lines say what the arguments said, that
# the .sha256 checks with sha256sum -c, and what a bad argument costs. The
# console is a stub file in every case: the pack script builds nothing and
# needs no cargo. The tree it packs is this checkout, so the case that checks
# what a tree missing one of those files costs builds a partial one in scratch.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

pack=$T_ROOT/scripts/pack-release.sh
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

# A console is needed by almost every run, and the out dir is written fresh
# each time, so both are set up before the cases start.
mkdir -p "$build"
printf 'console built by the workflow\n' >"$stub"
chmod 755 "$stub"

t_case "a pack writes the tarball, its .sha256 and nothing else, and says both"
rm -rf "$dist"
t_run "\"$pack\" v9.9.9 '$dist' --console '$stub'"
t_eq "$t_rc" 0
t_eq "$(head -n 1 <<<"$t_out")" "$dist/$tar_gz"
hash=$(sha256sum <"$dist/$tar_gz" | awk '{print $1}')
t_contains "$t_out" "$hash"
t_eq "$(find "$dist" -maxdepth 1 -type f | wc -l)" 2
t_eq "$(find "$dist" -maxdepth 1 -type f -name '*-build.json' | wc -l)" 0

t_case "the tarball holds what install.sh reads, in one top directory"
rm -rf "$dist"
t_run "\"$pack\" v9.9.9 '$dist' --console '$stub'"
t_eq "$t_rc" 0
list=$(tar -tzf "$dist/$tar_gz" | LC_ALL=C sort)
for want in tehas-v9.9.9/install.sh tehas-v9.9.9/bootstrap.sh tehas-v9.9.9/VERSION \
  tehas-v9.9.9/LICENSE tehas-v9.9.9/NOTICE tehas-v9.9.9/AGENTS.md \
  tehas-v9.9.9/bin/tehas tehas-v9.9.9/bin/tehasbot tehas-v9.9.9/libexec/dispatch \
  tehas-v9.9.9/libexec/work tehas-v9.9.9/libexec/console \
  tehas-v9.9.9/lib/config.sh tehas-v9.9.9/host/versions.env \
  tehas-v9.9.9/host/sandbox/build.sh tehas-v9.9.9/host/docs/tools.md \
  tehas-v9.9.9/host/release/minisign.pub; do
  case $list in
    *"$want"*) ;;
    *) t_bad "$want is not in the tarball" ;;
  esac
done
# One top directory, so unpacking leaves tehas-v9.9.9/ and nothing beside it.
t_eq "$(cut -d/ -f1 <<<"$list" | LC_ALL=C sort -u | tr '\n' ' ')" "tehas-v9.9.9 "
# What an adopter has no toolchain for, and what a host does not install, stay
# out: the console's source, the instances of this repository's own factory, and
# the git record of whoever packed it.
for absent in console/ instances/ preview/ docs/ .git/ .github/ .pi/ tmp/; do
  case $list in
    *"tehas-v9.9.9/$absent"*) t_bad "$absent is in the tarball" ;;
  esac
done

t_case "the console an adopter gets is the one --console named, and is executable"
rm -rf "$dist"
printf 'the console of this release\n' >"$build/other"
chmod 755 "$build/other"
t_run "\"$pack\" v9.9.9 '$dist' --console '$build/other'"
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
rm -rf "$dist"
t_run "\"$pack\" v9.9.9 '$dist' --console '$stub'"
t_eq "$t_rc" 0
t_eq "$(tar -xzf "$dist/$tar_gz" -O tehas-v9.9.9/VERSION)" "v9.9.9"

t_case "without pins, versions.env is the checkout's own"
rm -rf "$dist"
t_run "\"$pack\" v9.9.9 '$dist' --console '$stub'"
t_eq "$t_rc" 0
t_eq "$(tar -xzf "$dist/$tar_gz" -O tehas-v9.9.9/host/versions.env)" \
  "$(cat "$T_ROOT/host/versions.env")"

t_case "the pins are the last lines, and what the release does not pin is kept"
rm -rf "$dist"
t_run "\"$pack\" v9.9.9 '$dist' --console '$stub' --rukas-commit $commit --rukas-digest $digest"
t_eq "$t_rc" 0
packed=$(tar -xzf "$dist/$tar_gz" -O tehas-v9.9.9/host/versions.env)
t_contains "$packed" "PI_RUKAS_FOLLOW=commit:$commit"
t_contains "$packed" "PI_RUKAS_IMAGE_DIGEST=$digest"
# Every version the release does not pin is still in the file it ships with.
t_contains "$packed" "NODE_VERSION="
t_contains "$packed" "PI_RUKAS_REPO="
t_eq "$(grep -c '^PI_RUKAS_FOLLOW=' <<<"$packed")" 1
t_eq "$(tail -n 2 <<<"$packed")" \
  "$(printf 'PI_RUKAS_FOLLOW=commit:%s\nPI_RUKAS_IMAGE_DIGEST=%s' "$commit" "$digest")"

t_case "the tag must be vX.Y.Z, and the arguments it needs are named"
rm -rf "$dist"
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
rm -rf "$dist"
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
rm -rf "$dist"
t_run "\"$pack\" v9.9.9 '$dist' --console '$stub' --rukas-commit $commit"
t_eq "$t_rc" 64
t_contains "$t_out" "name both pins or neither"
t_run "\"$pack\" v9.9.9 '$dist' --console '$stub' --rukas-digest $digest"
t_eq "$t_rc" 64
t_contains "$t_out" "name both pins or neither"
t_run "\"$pack\" v9.9.9 '$dist' --console '$stub' --rukas-commit deadbeef --rukas-digest $digest"
t_eq "$t_rc" 1
t_contains "$t_out" "40 hex digits"
t_run "\"$pack\" v9.9.9 '$dist' --console '$stub' --rukas-commit $commit --rukas-digest sha256:abc"
t_eq "$t_rc" 1
t_contains "$t_out" "64 hex digits"
# A run that was refused wrote nothing at all.
t_eq "$(find "$dist" -maxdepth 1 -type f 2>/dev/null | wc -l)" 0

t_case "an argument it does not know, or a third one, is a bad argument"
rm -rf "$dist"
t_run "\"$pack\" v9.9.9 '$dist' --console '$stub' --consolex yes"
t_eq "$t_rc" 64
t_contains "$t_out" "unknown argument"
t_run "\"$pack\" v8.8.8 v9.9.9 '$dist' --console '$stub'"
t_eq "$t_rc" 64
t_contains "$t_out" "too many arguments"
t_run "\"$pack\" --help"
t_eq "$t_rc" 0
t_contains "$t_out" "Usage: scripts/pack-release.sh"

t_case "a tree missing what install.sh reads is refused, and writes nothing"
rm -rf "$dist" "$tree"
mkdir -p "$tree/bin" "$tree/libexec" "$tree/lib" "$tree/host/sandbox" "$tree/scripts"
for f in bootstrap.sh LICENSE NOTICE AGENTS.md; do
  printf '%s\n' "$f" >"$tree/$f"
done
cp "$T_ROOT/install.sh" "$T_ROOT/host/versions.env" "$tree/"
cp "$T_ROOT/bin/tehas" "$tree/bin/"
cp "$T_ROOT/libexec/dispatch" "$tree/libexec/"
cp "$T_ROOT/lib/config.sh" "$tree/lib/"
cp "$T_ROOT/host/sandbox/build.sh" "$tree/host/sandbox/"
cp "$pack" "$tree/scripts/"
chmod +x "$tree/scripts/pack-release.sh"
# The one thing that tree does not have is named, with the tree it was read from.
rm -f "$tree/bootstrap.sh"
t_run "\"$tree/scripts/pack-release.sh\" v9.9.9 '$dist' --console '$stub'"
t_eq "$t_rc" 1
t_contains "$t_out" "bootstrap.sh is missing from $tree"
t_eq "$(find "$dist" -maxdepth 1 -type f 2>/dev/null | wc -l)" 0

t_case "that same tree with the file it was missing packs"
cp "$T_ROOT/bootstrap.sh" "$tree/"
t_run "\"$tree/scripts/pack-release.sh\" v9.9.9 '$dist' --console '$stub'"
t_eq "$t_rc" 0
t_eq "$(tar -tzf "$dist/$tar_gz" | cut -d/ -f2 | grep -v '^$' | LC_ALL=C sort -u | tr '\n' ' ')" \
  "AGENTS.md LICENSE NOTICE VERSION bin bootstrap.sh host install.sh lib libexec "

t_case "the .sha256 is in sha256sum's format, checks, and fails on a changed tarball"
rm -rf "$dist"
t_run "\"$pack\" v9.9.9 '$dist' --console '$stub'"
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
rm -rf "$dist"
t_run "\"$pack\" v9.9.9 '$dist' --console '$stub'"
t_eq "$t_rc" 0
before=$(sha256sum <"$dist/$tar_gz" | awk '{print $1}')
printf 'a different console\n' >"$build/second"
chmod 755 "$build/second"
t_run "\"$pack\" v9.9.9 '$dist' --console '$build/second'"
t_eq "$t_rc" 1
t_contains "$t_out" "already exists"
t_eq "$(sha256sum <"$dist/$tar_gz" | awk '{print $1}')" "$before"

t_case "an out dir that cannot be made is a bad argument"
t_run "\"$pack\" v9.9.9 '$build/plain/nested' --console '$stub'"
t_eq "$t_rc" 64
t_contains "$t_out" "cannot make"

t_done
