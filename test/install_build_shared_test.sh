#!/usr/bin/env bash
# Tests for install.sh's build, split from install_build_test.sh: a change to
# a shared library builds every program that takes it, from a home installed
# from a checkout of its own.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1
installed_checkout

# The attachment checker takes the library too, as its manifest says.
for shared in src/mask.rs src/time.rs Cargo.toml; do
  t_case "a change to the library the seven share builds all seven ($shared)"
  echo "# changed" >>"$part/console/core/$shared"
  rm -f "$T/calls/cargo"
  t_run "$part/install.sh --check"
  t_eq "$t_rc" 1
  for prog in console analytics tehas-mcp store attach tehas-dispatch tehas-cycle; do
    t_contains "$t_out" "differs: $th/libexec/$prog"
  done
  t_run "$part/install.sh"
  t_eq "$t_rc" 0
  t_eq "$(t_calls cargo | wc -l)" 7
  for prog in console analytics tehas-mcp store attach tehas-dispatch tehas-cycle; do
    t_contains "$(t_calls cargo)" "-p $prog"
  done
done

# The console, dispatcher, cycle supervisor and MCP server (through the GitHub
# adapter) take the attachment library; the analytics and store programs do not.
t_case "a change to the attachment checker builds it and the four that take it, and its tests build nothing"
rm -f "$T/calls/cargo"
echo "// changed" >>"$part/console/attach/src/main.rs"
t_run "$part/install.sh --check"
for prog in attach console tehas-dispatch tehas-cycle tehas-mcp; do
  t_contains "$t_out" "differs: $th/libexec/$prog"
done
for prog in analytics store; do
  t_lacks "$t_out" "differs: $th/libexec/$prog"
done
t_run "$part/install.sh"
t_eq "$t_rc" 0
t_eq "$(t_calls cargo | wc -l)" 5
for prog in attach console tehas-dispatch tehas-cycle tehas-mcp; do
  t_contains "$(t_calls cargo)" "-p $prog"
done
t_eq "$(stat -c %a "$th/libexec/attach")" 755
rm -f "$T/calls/cargo"
echo "// changed" >>"$part/console/attach/src/tests.rs"
t_run "$part/install.sh"
t_eq "$(t_calls cargo)" ""

# The refresh text the dispatcher embeds is a file of its package: a change to
# it alone makes the dispatcher due, so none prints the old help.
t_case "a change to the refresh text the dispatcher embeds builds the dispatcher alone"
rm -f "$T/calls/cargo"
echo " changed" >>"$part/console/dispatch/src/usage/refresh.txt"
t_run "$part/install.sh --check"
t_eq "$t_rc" 1
t_contains "$t_out" "differs: $th/libexec/tehas-dispatch"
t_lacks "$t_out" "differs: $th/libexec/console"
t_run "$part/install.sh"
t_eq "$t_rc" 0
t_eq "$(t_calls cargo)" "build --release --locked --offline --quiet --manifest-path $part/console/Cargo.toml --target-dir $T/target -p tehas-dispatch"

t_done
