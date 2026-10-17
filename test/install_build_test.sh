#!/usr/bin/env bash
# Tests for install.sh: a console build that is given no secret or fails,
# a host without cargo, the footers' version, and a checkout without the
# console's source.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh"

t_case "the build is given no variable named like a secret"
fresh
instance
t_stub cargo 'env >"$T/cargo.env"
mkdir -p "$CARGO_TARGET_DIR/release" && echo built >"$CARGO_TARGET_DIR/release/console"'
t_env GH_TOKEN=hidden-token DEPLOY_SECRET=hidden-secret DB_PASSWORD=hidden-word SOME_API_KEY=hidden-key PLAIN=kept
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$(cat "$T/cargo.env")" "hidden-"
t_contains "$(cat "$T/cargo.env")" "PLAIN=kept"
t_stub_cargo
t_env

for failure in CARGO_RC=101 NO_CONSOLE=1; do
  t_case "a console that does not build installs nothing and exits 1 ($failure)"
  fresh
  instance
  # What an earlier build left is not taken for this one's.
  mkdir -p "$T/target/release"
  echo "an earlier build" >"$T/target/release/console"
  t_env "$failure"
  t_run "$install"
  t_eq "$t_rc" 1
  t_contains "$t_out" "the console did not build"
  t_eq "$(host_files | wc -l)" 0
done
t_env

t_case "a host without cargo is told so, and nothing is installed"
fresh
instance
t_env CARGO_RC=127
t_run "$install"
t_eq "$t_rc" 1
t_contains "$t_out" "cargo is not installed"
t_lacks "$t_out" "the console did not build"
t_eq "$(host_files | wc -l)" 0
t_env

t_case "the footers are given the checkout's newest tag and its commit"
fresh
instance
tagged=$T/tagged
rm -rf "$tagged"
mkdir -p "$tagged"
cp -R "$T_ROOT/install.sh" "$T_ROOT/lib" "$T_ROOT/host" "$T_ROOT/bin" "$T_ROOT/libexec" "$tagged/"
t_console_source "$tagged"
t_run "$tagged/install.sh"
t_eq "$t_rc" 0
# Not a repository of its own: no version, and the pages show none.
t_eq "$(find "$th/docs" "$th/fun" -name version.txt | wc -l)" 0
commit() { git -C "$tagged" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m "$1"; }
git init -q -b main "$tagged"
commit first
t_run "$tagged/install.sh"
t_eq "$(cat "$th/docs/version.txt")" "$(git -C "$tagged" rev-parse --short=7 HEAD)"
git -C "$tagged" tag v9.8.7
commit second
t_run "$tagged/install.sh"
t_eq "$t_rc" 0
t_eq "$(cat "$th/docs/version.txt")" "v9.8.7+$(git -C "$tagged" rev-parse --short=7 HEAD)"
t_eq "$(cat "$th/fun/version.txt")" "$(cat "$th/docs/version.txt")"
t_contains "$t_out" "version: v9.8.7+"
# The file is the host's own: not stale, and no drift.
t_run "$tagged/install.sh --check"
t_contains "$t_out" "Installed:    OK"
t_lacks "$t_out" "version.txt"
for page in "$T_ROOT/host/docs/_layout.html" "$T_ROOT/host/fun/index.html"; do
  t_contains "$(cat "$page")" 'readFile "/version.txt"'
done

t_case "a checkout without the console's source installs nothing and exits 1"
fresh
instance
bare=$T/bare
rm -rf "$bare"
mkdir -p "$bare"
cp -R "$T_ROOT/install.sh" "$T_ROOT/lib" "$T_ROOT/host" "$T_ROOT/bin" "$T_ROOT/libexec" "$bare/"
t_run "$bare/install.sh"
t_eq "$t_rc" 1
t_contains "$t_out" "$bare/console is missing from this checkout"
t_eq "$(host_files | wc -l)" 0


t_done
