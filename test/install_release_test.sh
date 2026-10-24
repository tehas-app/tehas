#!/usr/bin/env bash
# Tests for install.sh run from a tree unpacked from a release: it has no
# console/ to build from, so the programs the release carries are installed as
# they are, cargo is never asked, and a program the release does not carry
# leaves the installed one alone. The tree is packed by scripts/pack-release.sh
# from this checkout, with stub programs; the store's stub is a program, since
# install.sh runs it. Each is given the ELF header the pack reads from a
# program, which is what lets a stub be packed as one at all.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

build=$T/build
mkdir -p "$build"
# The header pack-release.sh reads, and no more of an ELF than that: the magic,
# 14 bytes of padding, then e_machine at offsets 18 and 19 - 0x3e for x86_64,
# little endian, the architecture this packs for by default and the one read
# from the file rather than from what the pack was told. The bytes of a program
# are the bytes of a release, so what the host installed is compared with the
# fixture by cmp: a command substitution would drop the nulls of the header.
elf_x86_64='\177ELF\000\000\000\000\000\000\000\000\000\000\000\000\000\000\076\000'
# A program of the machine this pack is not for: e_machine b700, which is what a
# refusal names beside the 3e00 the tarball's name promises.
elf_aarch64='\177ELF\000\000\000\000\000\000\000\000\000\000\000\000\000\000\267\000'
# A release's programs, as the pack is given them: the header, then what the
# host does with the file. The three that are only installed carry their text,
# which an assertion quotes back from the host's copy.
for p in console analytics mcp; do
  { printf '%b' "$elf_x86_64"; printf '%s of the release\n' "$p"; } >"$build/$p"
  chmod 755 "$build/$p"
done
# The store is the program install.sh runs, so its file has to answer to two
# readers at once: the pack, which asks the two bytes at offset 18 which machine
# the release is for, and the host, which runs the file it finds in libexec/. A
# file that opens with the magic is handed to the kernel first, and the kernel
# reads the whole file as an ELF and answers Exec format error - so a header in
# front of a script is a program no host can start. The two are kept apart the
# way a release keeps them apart in fact: the file the pack is handed is the
# header, and the file the tree and the host hold is the script t_stub writes,
# which records the call the case below reads back and answers as the store
# does. pack_tree puts the script into the tree it unpacks, since the pack
# copies into libexec/ whatever the pack was given.
{
  printf '%b' "$elf_x86_64"
  printf 'the store of the release\n'
} >"$build/store"
chmod 755 "$build/store"
t_stub store '
case $1 in
  init) echo "{\"api\":1,\"verb\":\"init\",\"ok\":true,\"schema\":1,\"created\":true}" ;;
  migrate) echo "{\"api\":1,\"verb\":\"migrate\",\"ok\":true,\"from\":1,\"to\":1,\"backup\":\"$T/tehas-1-0.db\"}" ;;
esac
'
# The script of PATH is the program the host runs. The one word the release's
# own copy of the store carries is put at its head, after the shebang, so the
# file the pack was handed and the file the host runs differ by the header alone
# and an assertion can compare the two with cmp.
{
  head -n 1 "$T/bin/store"
  printf '# store of the release\n'
  tail -n +2 "$T/bin/store"
} >"$T/bin/store.body"
mv "$T/bin/store.body" "$T/bin/store"
chmod 755 "$T/bin/store"
pack_tree() { # [pack arguments]
  rm -rf "$T/dist" "$T/rel"
  # No --arch, so the pack wants x86_64 programs: the header above is what makes
  # these stubs pass. A pack that was refused writes no tarball for the unpack
  # below to find, so its status is this function's; what a success prints names
  # the two files, which no case here reads.
  TEHAS_PACK_ROOT=$T_ROOT "$T_ROOT/scripts/pack-release.sh" v9.9.9 "$T/dist" --console "$build/console" \
    --commit "$T_COMMIT" "$@" >/dev/null || return 1
  mkdir -p "$T/rel"
  tar -xzf "$T/dist/tehas-v9.9.9-linux-x86_64.tar.gz" -C "$T/rel"
  # What the pack copied into the tree is the file it was handed for --store,
  # which is the header it reads and nothing else; the program the host runs is
  # the script of the same name, as a real release's is a program of one name.
  [ ! -e "$tree/libexec/store" ] || cp "$T/bin/store" "$tree/libexec/store"
}
tree=$T/rel/tehas-v9.9.9

t_case "a release's programs are installed as they are, and cargo is not asked"
pack_tree --analytics "$build/analytics" --mcp "$build/mcp" --store "$build/store"
fresh
instance
t_run "'$tree/install.sh'"
t_eq "$t_rc" 0
t_eq "$(cmp -s "$build/console" "$th/libexec/console" && echo same)" same
t_eq "$(cmp -s "$build/analytics" "$th/libexec/analytics" && echo same)" same
t_eq "$(cmp -s "$build/mcp" "$th/libexec/tehas-mcp" && echo same)" same
t_eq "$(cmp -s "$T/bin/store" "$th/libexec/store" && echo same)" same
# The store is run with what the release says it is: the tag, and the commit
# from its COMMIT file, and then imports what an older tehas left, which it
# judged before the copies.
t_eq "$(t_calls store)" "import --dry-run
init --version v9.9.9 --commit $T_COMMIT --instance instance --repo acme/widget
import"
t_eq "$(cat "$th/libexec/console.stamp")" "release $(sha256sum <"$build/console" | cut -d' ' -f1)"
t_eq "$(t_calls cargo)" ""
t_eq "$(cat "$th/docs/version.txt")" v9.9.9

t_case "a release whose program is built for the other machine is refused"
# The pack, not install.sh, is what reads the header: a tarball named for one
# architecture and filled with programs of the other is refused before it
# exists, so there is nothing here to unpack, install or run.
{ printf '%b' "$elf_aarch64"; printf 'console of the other machine\n'; } >"$build/wrong"
chmod 755 "$build/wrong"
rm -rf "$T/dist" "$T/rel"
t_run "TEHAS_PACK_ROOT='$T_ROOT' '$T_ROOT/scripts/pack-release.sh' v9.9.9 '$T/dist' \
  --console '$build/wrong' --commit '$T_COMMIT'"
t_eq "$t_rc" 1
t_contains "$t_out" "not a x86_64 program: $build/wrong"
t_contains "$t_out" "e_machine b700, want 3e00"
t_eq "$(find "$T/dist" -maxdepth 1 -type f 2>/dev/null | wc -l)" 0
# The same tree with the console the pack accepts packs, which is what the
# cases after this one run from: pack_tree's own status is the unpack's, so a
# pack that was refused has already been heard from above.
pack_tree --store "$build/store"

t_case "a second run from the same release finds the programs installed"
t_run "'$tree/install.sh' --check"
t_lacks "$t_out" "differs: $th/libexec/console"
t_lacks "$t_out" "missing: $th/libexec/analytics"

t_case "a release without the analytics program keeps the installed one"
pack_tree
t_run "'$tree/install.sh'"
t_eq "$t_rc" 0
t_contains "$t_out" "this release carries no libexec/analytics; the one installed stays"
t_eq "$(cmp -s "$build/analytics" "$th/libexec/analytics" && echo same)" same

t_case "a release with no store, on a host with none, installs nothing"
pack_tree
fresh
instance
t_run "'$tree/install.sh'"
t_eq "$t_rc" 1
t_contains "$t_out" "this release has no libexec/store and none is installed; nothing was installed"
t_eq "$(host_files | wc -l)" 0
t_run "'$tree/install.sh' --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Store:        REFUSED (this release has no libexec/store and none is installed; ./install.sh stops before it writes)"
t_lacks "$t_out" "$th/libexec/store"

t_case "a release with no COMMIT installs nothing"
pack_tree --store "$build/store"
rm -f "$tree/COMMIT"
fresh
instance
t_run "'$tree/install.sh'"
t_eq "$t_rc" 1
t_contains "$t_out" "the commit of $tree is not known"
t_eq "$(host_files | wc -l)" 0
t_run "'$tree/install.sh' --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Store:        REFUSED (the commit of $tree is not known"

t_case "a release with no console installs nothing"
pack_tree
rm -f "$tree/libexec/console"
fresh
instance
t_run "'$tree/install.sh'"
t_eq "$t_rc" 1
t_contains "$t_out" "this release has no libexec/console"
t_eq "$(host_files | wc -l)" 0

t_done
