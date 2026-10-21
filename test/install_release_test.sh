#!/usr/bin/env bash
# Tests for install.sh run from a tree unpacked from a release: it has no
# console/ to build from, so the programs the release carries are installed as
# they are, cargo is never asked, and a program the release does not carry
# leaves the installed one alone. The tree is packed by scripts/pack-release.sh
# from this checkout, with stub programs.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

build=$T/build
mkdir -p "$build"
for p in console analytics mcp; do
  printf '%s of the release\n' "$p" >"$build/$p"
  chmod 755 "$build/$p"
done
pack_tree() { # [pack arguments]
  rm -rf "$T/dist" "$T/rel"
  TEHAS_PACK_ROOT=$T_ROOT "$T_ROOT/scripts/pack-release.sh" v9.9.9 "$T/dist" --console "$build/console" "$@" >/dev/null || return 1
  mkdir -p "$T/rel"
  tar -xzf "$T/dist/tehas-v9.9.9-linux-x86_64.tar.gz" -C "$T/rel"
}
tree=$T/rel/tehas-v9.9.9

t_case "a release's programs are installed as they are, and cargo is not asked"
pack_tree --analytics "$build/analytics" --mcp "$build/mcp"
fresh
instance
t_run "'$tree/install.sh'"
t_eq "$t_rc" 0
t_eq "$(cat "$th/libexec/console")" "console of the release"
t_eq "$(cat "$th/libexec/analytics")" "analytics of the release"
t_eq "$(cat "$th/libexec/tehas-mcp")" "mcp of the release"
t_eq "$(cat "$th/libexec/console.stamp")" "release $(sha256sum <"$build/console" | cut -d' ' -f1)"
t_eq "$(t_calls cargo)" ""
t_eq "$(cat "$th/docs/version.txt")" v9.9.9

t_case "a second run from the same release finds the programs installed"
t_run "'$tree/install.sh' --check"
t_lacks "$t_out" "differs: $th/libexec/console"
t_lacks "$t_out" "missing: $th/libexec/analytics"

t_case "a release without the analytics program keeps the installed one"
pack_tree
t_run "'$tree/install.sh'"
t_eq "$t_rc" 0
t_contains "$t_out" "this release carries no libexec/analytics; the one installed stays"
t_eq "$(cat "$th/libexec/analytics")" "analytics of the release"

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
