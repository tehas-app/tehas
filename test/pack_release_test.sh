#!/usr/bin/env bash
# The tests of scripts/pack-release.sh: that the tarball holds what install.sh
# reads, that VERSION, COMMIT and the two pin lines say what the arguments said, that
# the .sha256 checks with sha256sum -c, that a tree holding a link out of itself
# is refused, and what a bad argument costs. The console is a stub file in every
# case: the pack script builds nothing and needs no cargo. The tree it packs is
# this checkout unless a case names another, so the case that checks what a tree
# missing one of those files costs builds a partial one in scratch.
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
# it, so a case says only what it changes. It gives the console stub, and a
# caller that names its own --console overrides that.
t_pack_reset() { rm -rf "$dist"; }
t_pack() {
  local root=${TEHAS_PACK_ROOT:-$T_ROOT} args=${*:-} cwd=${TEHAS_PACK_CWD:-$T_ROOT}
  [[ $args == *--console* ]] || args="--console '$stub' $args"
  # TEHAS_PACK_CWD names a tree to run from and leaves TEHAS_PACK_ROOT unset,
  # so the script falls back to its working directory: the default --help and
  # the manual show. t_run runs from elsewhere, hence the cd of its own, given
  # the path to the script so it does not depend on the directory it starts in.
  if [ -n "${TEHAS_PACK_CWD:-}" ]; then
    t_run "cd '$cwd' && \"$pack\" v9.9.9 '$dist' $args"
  else
    t_run "TEHAS_PACK_ROOT='$root' \"$pack\" v9.9.9 '$dist' $args"
  fi
}
# A tree with one real file in each directory the pack copies, the files a tehas
# tree has to have before it is packed at all, and the top-level files taken from
# the script's own lists, so a fifth required input needs no change here.
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
  cp "$T_ROOT/libexec/work" "$tree/libexec/"
  cp "$T_ROOT/lib/config.sh" "$tree/lib/"
  cp "$T_ROOT/host/sandbox/build.sh" "$tree/host/sandbox/"
  mkdir -p "$tree/instances"
  cp -R "$T_ROOT/instances/example" "$tree/instances/"
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

# Two statements in one list would read as a guard that holds, so they are
# apart: what the pack script never copies at all, and what the tree it copies
# carries that must not come along.
t_case "what the pack script does not copy is not in the tarball"
t_pack_reset
t_pack
t_eq "$t_rc" 0
list=$(tar -tzf "$dist/$tar_gz" | LC_ALL=C sort)
# These sit under the repository root, which is not among the directories
# lib/release.sh names, so no copy could bring them: named to say so.
for absent in console/ preview/ docs/; do
  case $list in
    *"tehas-v9.9.9/$absent"*) t_bad "$absent is in the tarball" ;;
  esac
done
# The example instance is packed, and no instance of a running factory with it.
t_contains "$list" "tehas-v9.9.9/instances/example/instance.env"
t_eq "$(grep '^tehas-v9.9.9/instances/[^/]*/' <<<"$list" | grep -v '^tehas-v9.9.9/instances/example/')" ""

# The git record is the one thing beside those directories that the copy could
# have carried with the tree, so the tree is made to look like a checkout: a
# worktree holds a .git file and a tag checked out elsewhere holds a directory.
t_case "the tree's git record does not reach the tarball"
t_pack_reset
make_tree
rm -f "$tree/.git"
mkdir -p "$tree/.git/objects" "$tree/.github/workflows" "$tree/.pi"
printf 'ref: refs/heads/main\n' >"$tree/.git/HEAD"
TEHAS_PACK_ROOT=$tree t_pack
t_eq "$t_rc" 0
list=$(tar -tzf "$dist/$tar_gz" | LC_ALL=C sort)
for absent in .git/ .git .github/ .pi/; do
  case $list in
    *"tehas-v9.9.9/$absent"*) t_bad "$absent is in the tarball" ;;
  esac
done

t_case "the console an adopter gets is the one --console named, and is executable"
t_pack_reset
printf 'the console of this release\n' >"$build/other"
chmod 755 "$build/other"
t_pack --console "$build/other"
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

t_case "the analytics program and the MCP server are packed when given, executable"
t_pack_reset
printf 'analytics of this release\n' >"$build/analytics"
printf 'mcp of this release\n' >"$build/tehas-mcp"
chmod 755 "$build/analytics" "$build/tehas-mcp"
t_pack --analytics "'$build/analytics'" --mcp "'$build/tehas-mcp'"
t_eq "$t_rc" 0
rm -rf "$T/tehas-v9.9.9"
tar -xzf "$dist/$tar_gz" -C "$T"
t_eq "$(cat "$T/tehas-v9.9.9/libexec/analytics")" "analytics of this release"
t_eq "$(cat "$T/tehas-v9.9.9/libexec/tehas-mcp")" "mcp of this release"
t_eq "$(stat -c '%a' "$T/tehas-v9.9.9/libexec/analytics" "$T/tehas-v9.9.9/libexec/tehas-mcp" | tr '\n' ' ')" "755 755 "

t_case "an --analytics or --mcp that is not a program is a bad argument, and writes nothing"
t_pack_reset
chmod 644 "$build/analytics"
t_pack --analytics "'$build/analytics'"
t_eq "$t_rc" 64
t_contains "$t_out" "not a program the pack can run"
t_pack --mcp "'$build/missing'"
t_eq "$t_rc" 64
t_run "\"$pack\" v9.9.9 '$dist' --console '$stub' --mcp"
t_eq "$t_rc" 64
t_contains "$t_out" "--mcp needs a path"
t_eq "$(find "$dist" -maxdepth 1 -type f 2>/dev/null | wc -l)" 0

t_case "the store is packed when given, executable, and --commit is written to COMMIT"
t_pack_reset
printf 'store of this release\n' >"$build/store"
chmod 755 "$build/store"
t_pack --store "'$build/store'" --commit "${commit^^}"
t_eq "$t_rc" 0
rm -rf "$T/tehas-v9.9.9"
tar -xzf "$dist/$tar_gz" -C "$T"
t_eq "$(cat "$T/tehas-v9.9.9/libexec/store")" "store of this release"
t_eq "$(stat -c '%a' "$T/tehas-v9.9.9/libexec/store")" 755
# Lower case, the shape install.sh accepts.
t_eq "$(cat "$T/tehas-v9.9.9/COMMIT")" "$commit"
t_pack_reset
t_pack
t_eq "$(tar -tzf "$dist/$tar_gz" | grep -c '^tehas-v9.9.9/COMMIT$')" 0

t_case "a --store that is not a program or a --commit of another shape writes nothing"
t_pack_reset
chmod 644 "$build/store"
t_pack --store "'$build/store'"
t_eq "$t_rc" 64
t_contains "$t_out" "not a program the pack can run"
t_pack --commit deadbeef
t_eq "$t_rc" 1
t_contains "$t_out" "--commit must be 40 hex digits"
t_run "\"$pack\" v9.9.9 '$dist' --console '$stub' --commit"
t_eq "$t_rc" 64
t_contains "$t_out" "--commit needs a commit"
t_eq "$(find "$dist" -maxdepth 1 -type f 2>/dev/null | wc -l)" 0

t_case "the tarball holds CHANGELOG.md, the checkout's own"
t_pack_reset
t_pack
t_eq "$t_rc" 0
t_eq "$(tar -xzf "$dist/$tar_gz" -O tehas-v9.9.9/CHANGELOG.md)" "$(cat "$T_ROOT/CHANGELOG.md")"

notes=$T_ROOT/scripts/release-notes.sh
changelog=$T/CHANGELOG.md
printf '%s\n' '# Changelog' '' '## Unreleased' '' '### Adopter action' '' 'None.' '' \
  '## v1.2.3 (2026-10-21)' '' '### Changes' '' '- v1.2.30 is not this' '' \
  '### Adopter action' '' 'Set TEHAS_X.' '' '## v1.2.30 (2026-10-22)' '' 'later' >"$changelog"

t_case "release-notes prints the tag's section without its heading or the next one"
t_run "'$notes' v1.2.3 '$changelog'"
t_eq "$t_rc" 0
t_eq "$t_out" "$(printf '%s\n' '### Changes' '' '- v1.2.30 is not this' '' '### Adopter action' '' 'Set TEHAS_X.')"

t_case "release-notes matches the version exactly, and reads Unreleased and standard input"
t_run "'$notes' v1.2.30 '$changelog'"
t_eq "$t_out" later
t_run "'$notes' Unreleased - <'$changelog'"
t_eq "$t_rc" 0
t_eq "$t_out" "$(printf '%s\n' '### Adopter action' '' 'None.')"

t_case "release-notes exits 1 for a tag with no section, and 64 for a bad argument"
t_run "'$notes' v1.2.4 '$changelog'"
t_eq "$t_rc" 1
t_contains "$t_out" "no section for v1.2.4"
t_run "'$notes' v1.2 '$changelog'"
t_eq "$t_rc" 64
t_run "'$notes'"
t_eq "$t_rc" 64
t_run "'$notes' v1.2.3 '$T/no-such-file'"
t_eq "$t_rc" 1

t_case "the checkout's CHANGELOG.md has an Unreleased section with an Adopter action"
t_run "'$notes' Unreleased"
t_eq "$t_rc" 0
t_contains "$t_out" "### Adopter action"

t_case "VERSION holds the tag"
t_pack_reset
t_pack
t_eq "$t_rc" 0
t_eq "$(tar -xzf "$dist/$tar_gz" -O tehas-v9.9.9/VERSION)" "v9.9.9"

t_case "the tree of the working directory is the tree it packs"
# The default a person gets with no TEHAS_PACK_ROOT, and the one --help and the
# manual show: the working directory. TEHAS_PACK_CWD says so to t_pack, which
# otherwise names a tree and would cover the default with a variable. The tree
# holds a file no other tree here has, and this checkout's own libexec/ holds
# what a build left beside it, so the list says which tree was read.
t_pack_reset
make_tree
printf 'a file of this tree\n' >"$tree/lib/only-here.sh"
TEHAS_PACK_CWD=$tree t_pack
t_eq "$t_rc" 0
t_eq "$(head -n 1 <<<"$t_out")" "$dist/$tar_gz"
packed=$(tar -tzf "$dist/$tar_gz" | sed 's|^tehas-v9.9.9/||' | grep -v '/$' |
  LC_ALL=C sort)
case $packed in
  *"lib/only-here.sh"*) ;;
  *) t_bad "lib/only-here.sh of the working directory is not in the tarball" ;;
esac
# A build's console left in a tree's libexec/ is not what an adopter gets: this
# checkout holds one, the scratch tree does not, and the pack takes the
# --console build whichever tree it reads.
t_eq "$(grep -c '^libexec/console$' <<<"$packed")" 1
t_eq "$(tar -xzf "$dist/$tar_gz" -O tehas-v9.9.9/libexec/console)" \
  "console built by the workflow"

t_case "a tree whose list names nothing is refused, and writes nothing"
# Two empty lists pass every existence check below them, so a manifest whose
# functions print nothing has to be a refusal rather than a release of nothing.
t_pack_reset
make_tree
printf '%s\n' '# shellcheck shell=bash' \
  'tehas_release_dirs() { :; }' \
  'tehas_release_plain() { :; }' >"$tree/lib/release.sh"
TEHAS_PACK_ROOT=$tree t_pack
t_eq "$t_rc" 1
t_contains "$t_out" "names nothing to pack"
t_eq "$(find "$dist" -maxdepth 1 -type f 2>/dev/null | wc -l)" 0

t_case "a tree holding a symbolic link is refused, and names the link"
# The guard a published release rests on: cp keeps a link and tar writes what it
# points at, so a link out of the tree is a file from outside it in the tarball.
t_pack_reset
make_tree
ln -s /etc "$tree/host/escape"
TEHAS_PACK_ROOT=$tree t_pack
t_eq "$t_rc" 1
t_contains "$t_out" "symbolic link"
t_contains "$t_out" "host/escape"
t_eq "$(find "$dist" -maxdepth 1 -type f 2>/dev/null | wc -l)" 0
rm "$tree/host/escape"
# With the link gone the same tree packs, and holds no link of its own.
TEHAS_PACK_ROOT=$tree t_pack
t_eq "$t_rc" 0
t_eq "$(tar -tzf "$dist/$tar_gz" | grep -c '^tehas-v9.9.9/host/escape$')" 0

# What lib/release.sh promises: every file install.sh reads from the tree is
# among the packed ones, so a release cannot be missing one install.sh wants.
# Read from install.sh itself rather than copied here, which is the drift this
# checks for.
t_case "every source install.sh reads is covered by the list the pack uses"
# Read from install.sh rather than copied into this file: the drift between what
# install.sh reads and what lib/release.sh names is what a broken release is,
# and a second hand-written list could not see it.
cases=0
check_sources() { # <lines of source paths>: each has to be covered by the lists
  local src top d found
  while IFS= read -r src; do
    [ -n "$src" ] || continue
    cases=$((cases + 1))
    case $src in
      /*)
        # An absolute source is a file of the instance, not of the tree.
        continue
        ;;
      \$*)
        # A source written as a variable names the instance's own file, which
        # install.sh reads from outside the tree by design.
        continue
        ;;
      */*)
        # A table entry is read from under host/ unless its first directory is
        # one of the four the pack copies whole, which is how install.sh's
        # source_of reads it. So a name under host/ is covered by the host/ that
        # is packed; anything else has to name one of the four.
        top=${src%%/*}
        found=0
        for d in "${want_dirs[@]}"; do
          [ "$top" = "$d" ] && found=1
        done
        if [ "$found" = 0 ] && [ ! -d "$T_ROOT/host/$top" ]; then
          t_bad "$src: install.sh reads it from under host/ and host/$top is no part of the tree"
        fi
        ;;
      *)
        # install.sh looks for a bare name under host/ when the tree's own top
        # level does not hold it; either way one of the two lists has to name it
        # or the file has to be in the host/ that is packed whole.
        printf '%s\n' "${want_plain[@]}" | grep -qx "host/$src" ||
          printf '%s\n' "${want_plain[@]}" | grep -qx "$src" ||
          [ -f "$T_ROOT/host/$src" ] ||
          t_bad "$src: install.sh reads it and neither list of lib/release.sh has it"
        ;;
    esac
  done
}
# The table, and the two loops that build entries from a list of names.
sources_of() {
  sed -n '/^files=(/,/^)/p' "$T_ROOT/install.sh" |
    sed -n 's/^[[:space:]]*"\([^"|]*\)|.*/\1/p'
  sed -n '/^for f in tehas tehasbot; do$/p' "$T_ROOT/install.sh" |
    sed 's/^for f in //;s/; do$//' | tr ' ' '\n' | sed 's|^|bin/|'
  sed -n '/^for f in analytics-cmd api cycle;/,/; do$/p' "$T_ROOT/install.sh" |
    sed 's/^for f in //;s/; do$//' | tr ' ' '\n' | sed '/^$/d;s|^|libexec/|'
}
check_sources < <(sources_of)
# The check itself: a run that read no source would say nothing.
[ "$cases" -ge 40 ] || t_bad "only $cases sources of install.sh were read"

t_case "an install.sh that reads what no release packs is named by the check"
# The one case that shows the check above can fail: a table entry whose first
# directory is neither one of the four the pack copies nor a directory of host/,
# in a copy of install.sh kept out of the tree. t_bad counts the failure and
# writes to standard error, so the case reads the tally it moved and then puts
# the tally back: a guard that cannot fail is not a guard.
broken=$T/install-broken.sh
sed '/^files=(/a\  "codegen/thing.rs|$TEHAS_HOME/libexec/thing.rs|755"' \
  "$T_ROOT/install.sh" >"$broken"
cases=0
failed_before=$t_failed
check_sources < <(sed -n '/^files=(/,/^)/p' "$broken" |
  sed -n 's/^[[:space:]]*"\([^"|]*\)|.*/\1/p')
[ "$t_failed" -gt "$failed_before" ] ||
  t_bad "an install.sh that reads codegen/thing.rs passed the check"
# Exactly one source of the copy was refused: the added one.
t_eq "$t_failed" "$((failed_before + 1))"
# The added line is one of the sources read, so the count moved: the check is
# not walking an empty list.
[ "$cases" -ge 2 ] || t_bad "only $cases sources of the broken install.sh were read"
t_failed=$failed_before

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

t_case "a pin given in upper case is a pin, and is shipped in lower case"
# git and a registry both hand back upper case, and the file the release ships
# holds one shape: the value is lower-cased on the way in, not refused.
t_pack_reset
t_pack --rukas-commit "$(tr 'a-f' 'A-F' <<<"$commit")" \
  --rukas-digest "sha256:$(printf 'A%.0s' {1..64})"
t_eq "$t_rc" 0
packed=$(tar -xzf "$dist/$tar_gz" -O tehas-v9.9.9/host/versions.env)
t_contains "$packed" "PI_RUKAS_FOLLOW=commit:$commit"
t_eq "$(tail -n 1 <<<"$packed")" "PI_RUKAS_IMAGE_DIGEST=$digest"

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

t_case "a tree that is not a tehas tree is a bad argument, named by its path"
t_pack_reset
TEHAS_PACK_ROOT=$build/inside t_pack
t_eq "$t_rc" 64
t_contains "$t_out" "not a tehas tree to pack: $build/inside"
t_eq "$(find "$dist" -maxdepth 1 -type f 2>/dev/null | wc -l)" 0
# The one file that answers to tehas besides install.sh is enough to say so:
# a tree that holds install.sh and none of the rest is refused the same way.
not_tehas=$T/not-tehas
rm -rf "$not_tehas"
mkdir -p "$not_tehas"
cp "$T_ROOT/install.sh" "$not_tehas/"
TEHAS_PACK_ROOT=$not_tehas t_pack
t_eq "$t_rc" 64
t_contains "$t_out" "not a tehas tree to pack: $not_tehas"

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
  "AGENTS.md CHANGELOG.md LICENSE NOTICE VERSION bin bootstrap.sh host install.sh instances lib libexec "

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
t_pack --console "$build/second"
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
# The arguments themselves, before their placeholders are filled: a manual that
# dropped --rukas-digest or renamed a placeholder would still run otherwise, and
# the workflow is written from exactly these names.
for want in '--console <path>' '--rukas-commit <40 hex>' '--rukas-digest <64 hex>'; do
  case $args in
    *"$want"*) ;;
    *) t_bad "the manual's command does not name $want" ;;
  esac
done
args=${args#*'<tag>'}
args=${args//'<out dir>'/"'$dist'"}
args=${args//'<path>'/"'$stub'"}
args=${args//'<40 hex>'/"$commit"}
args=${args//'<64 hex>'/"$digest"}
t_pack_reset
t_run "TEHAS_PACK_ROOT='$T_ROOT' \"$pack\" v9.9.9 $args"
t_eq "$t_rc" 0
t_eq "$(head -n 1 <<<"$t_out")" "$dist/$tar_gz"
# The values the manual's placeholders were filled with are the ones the tarball
# carries, so the command above is the workflow's and not merely a working one.
packed=$(tar -xzf "$dist/$tar_gz" -O tehas-v9.9.9/host/versions.env)
t_eq "$(tail -n 2 <<<"$packed")" \
  "$(printf 'PI_RUKAS_FOLLOW=commit:%s\nPI_RUKAS_IMAGE_DIGEST=%s' "$commit" "$digest")"

t_case "the statuses the manual says the workflow branches on are the two it gives"
# Both halves of the manual's sentence about exit statuses, from the manual's own
# command: only one pin named is a bad argument, a value of the wrong shape is a
# failure, and neither wrote a tarball for the next run to be refused against.
bad=$T/manual-args
sed -n "${start},$((end - 1))p" "$manual" | tr -d '\\\n' | tr -s ' ' >"$bad"
half=$(sed 's/ --rukas-digest <64 hex>//' "$bad")
half=${half#*'<tag>'}
half=${half//'<out dir>'/"'$dist'"}
half=${half//'<path>'/"'$stub'"}
half=${half//'<40 hex>'/"$commit"}
t_pack_reset
t_run "TEHAS_PACK_ROOT='$T_ROOT' \"$pack\" v9.9.9 $half"
t_eq "$t_rc" 64
t_contains "$t_out" "name both pins or neither"
wrong=${half//"$commit"/deadbeef}
t_pack_reset
t_run "TEHAS_PACK_ROOT='$T_ROOT' \"$pack\" v9.9.9 $wrong"
t_eq "$t_rc" 1
t_contains "$t_out" "40 hex digits"
t_eq "$(find "$dist" -maxdepth 1 -type f 2>/dev/null | wc -l)" 0
# The two names the manual says the workflow uploads are the two the run wrote.
t_pack_reset
t_run "TEHAS_PACK_ROOT='$T_ROOT' \"$pack\" v9.9.9 $args"
t_eq "$t_rc" 0
t_eq "$(find "$dist" -maxdepth 1 -type f -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "$tar_gz $tar_gz.sha256 "

t_done
