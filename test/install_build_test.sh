#!/usr/bin/env bash
# Tests for install.sh's console build: no secret, no install from a failed build
# or no cargo, and cargo in ~/.cargo/bin. One of four with install_build_tree,
# _stamp and _shared; failures and timeouts are in install_build_failure_test.sh.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

t_case "the build is given no variable named like a secret"
fresh
instance
t_stub cargo 'env >>"$T/cargo.env"
dir=$CARGO_TARGET_DIR/release
[[ " $* " != *" --target "* ]] || dir=$CARGO_TARGET_DIR/x86_64-unknown-linux-musl/release
mkdir -p "$dir" && echo built >"$dir/${*: -1}"
# install.sh runs the store, so the store has to be a program.
[ "${*: -1}" != store ] || { cp "$T/store-stand-in" "$dir/store"; chmod 755 "$dir/store"; }'
t_env GH_TOKEN=hidden-token DEPLOY_SECRET=hidden-secret DB_PASSWORD=hidden-word SOME_API_KEY=hidden-key \
  TEHAS_MODEL_KEY_MAIN=hidden-model DB_PASSWD=hidden-passwd cloud_credential=hidden-cred PLAIN=kept
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

# bootstrap.sh installs Rust with --no-modify-path, and a non-login run has no
# env.sh.
t_case "cargo in ~/.cargo/bin is found when PATH does not name that directory"
fresh
instance
mkdir -p "$home/.cargo/bin"
mv "$T/bin/cargo" "$home/.cargo/bin/cargo"
t_env PATH="$T/bin:/usr/bin:/bin"
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$t_out" "cargo is not installed"
t_eq "$(t_calls cargo | wc -l)" 7
t_eq "$(cat "$th/libexec/console")" "a console built from source"
t_eq "$(cat "$th/libexec/analytics")" "an analytics built from source"
t_eq "$(cat "$th/libexec/tehas-mcp")" "a tehas-mcp built from source"
t_contains "$(cat "$th/libexec/store")" "a store built from source"
t_stub_cargo
t_env

t_done
