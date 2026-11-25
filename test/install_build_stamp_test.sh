#!/usr/bin/env bash
# Tests for install.sh's build, split from install_build_test.sh: each program
# is built beside the console with its own stamp, and a change builds its own.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

t_case "analytics is built and installed beside the console, with its own stamp"
fresh
instance
checkout
t_run "$part/install.sh"
t_eq "$t_rc" 0
t_eq "$(cat "$th/libexec/analytics")" "an analytics built from source"
t_eq "$(stat -c %a "$th/libexec/analytics")" 755
t_eq "$(cut -d' ' -f2 "$th/libexec/analytics.stamp")" "$(sha256sum <"$th/libexec/analytics" | cut -d' ' -f1)"
[ "$(cut -d' ' -f1 "$th/libexec/analytics.stamp")" != "$(cut -d' ' -f1 "$th/libexec/console.stamp")" ] ||
  t_bad "analytics and the console have the same source checksum"
t_run "$part/install.sh --check"
t_contains "$t_out" "Installed:    OK"

t_case "a change to analytics alone builds analytics alone, and the console's stamp stays"
console_stamp=$(cat "$th/libexec/console.stamp")
echo "// changed" >>"$part/console/analytics/src/main.rs"
t_run "$part/install.sh --check"
t_eq "$t_rc" 1
t_contains "$t_out" "differs: $th/libexec/analytics"
t_lacks "$t_out" "differs: $th/libexec/console"
rm -f "$T/calls/cargo"
t_env CONSOLE_BUILD="new source"
t_run "$part/install.sh"
t_eq "$t_rc" 0
t_eq "$(t_calls cargo)" "build --release --locked --offline --quiet --manifest-path $part/console/Cargo.toml --target-dir $T/target -p analytics"
t_eq "$(cat "$th/libexec/analytics")" "an analytics built from new source"
t_eq "$(cat "$th/libexec/console")" "a console built from source"
t_eq "$(cat "$th/libexec/console.stamp")" "$console_stamp"
t_env

t_case "a change to the console alone builds the console alone"
echo "// changed" >>"$part/console/src/main.rs"
rm -f "$T/calls/cargo"
t_run "$part/install.sh"
t_eq "$(t_calls cargo)" "build --release --locked --offline --quiet --manifest-path $part/console/Cargo.toml --target-dir $T/target -p console"

t_case "the MCP server is built static and installed beside the console, with its own stamp"
rm -f "$T/calls/cargo"
echo "// changed" >>"$part/console/mcp/src/main.rs"
t_run "$part/install.sh --check"
t_contains "$t_out" "differs: $th/libexec/tehas-mcp"
t_lacks "$t_out" "differs: $th/libexec/console"
t_run "$part/install.sh"
t_eq "$t_rc" 0
t_eq "$(t_calls cargo)" "build --release --locked --offline --quiet --manifest-path $part/console/Cargo.toml --target-dir $T/target --target x86_64-unknown-linux-musl -p tehas-mcp"
t_eq "$(cat "$th/libexec/tehas-mcp")" "a tehas-mcp built from source"
t_eq "$(stat -c %a "$th/libexec/tehas-mcp")" 755
t_eq "$(cut -d' ' -f2 "$th/libexec/tehas-mcp.stamp")" "$(sha256sum <"$th/libexec/tehas-mcp" | cut -d' ' -f1)"

t_case "a change to the store builds the console too, which links it, and its tests build nothing"
rm -f "$T/calls/cargo"
echo "// changed" >>"$part/console/store/src/main.rs"
t_run "$part/install.sh --check"
t_contains "$t_out" "differs: $th/libexec/store"
t_contains "$t_out" "differs: $th/libexec/console"
t_run "$part/install.sh"
t_eq "$t_rc" 0
t_eq "$(t_calls cargo | wc -l)" 2
t_contains "$(t_calls cargo)" "build --release --locked --offline --quiet --manifest-path $part/console/Cargo.toml --target-dir $T/target -p store"
t_contains "$(t_calls cargo)" "-p console"
t_eq "$(stat -c %a "$th/libexec/store")" 755
t_eq "$(cut -d' ' -f2 "$th/libexec/store.stamp")" "$(sha256sum <"$th/libexec/store" | cut -d' ' -f1)"
rm -f "$T/calls/cargo"
echo "// changed" >>"$part/console/store/src/tests.rs"
t_run "$part/install.sh"
t_eq "$(t_calls cargo)" ""

t_case "a change to the cycle builds the console too, which links it, and its tests build nothing"
mkdir -p "$part/console/cycle/src"
echo "// changed" >>"$part/console/cycle/src/caps.rs"
rm -f "$T/calls/cargo"
t_run "$part/install.sh"
t_eq "$t_rc" 0
t_eq "$(t_calls cargo | wc -l)" 2
t_contains "$(t_calls cargo)" "-p console"
t_contains "$(t_calls cargo)" "-p tehas-cycle"
mkdir -p "$part/console/cycle/tests"
echo "// changed" >>"$part/console/cycle/tests/changed.rs"
rm -f "$T/calls/cargo"
t_run "$part/install.sh"
t_eq "$(t_calls cargo)" ""

t_done
