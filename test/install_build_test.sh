#!/usr/bin/env bash
# Tests for install.sh: a console build that is given no secret or fails,
# a host without cargo, cargo found in ~/.cargo/bin when PATH does not name it,
# a tool directory another account can write, the footers' version, a checkout
# without the console's source, and the analytics program built beside the
# console. Builds that fail or time out are in install_build_failure_test.sh.
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

# bootstrap.sh installs Rust with --no-modify-path, so cargo is only where it
# was put. A run that is not a login shell has no env.sh yet to name it.
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

# The tool directories are first on PATH for a real run, and what is built from
# them is installed as the console every cycle runs. One another account can
# write is refused before a build.
t_case "a tool directory another account can write stops the run before the build"
fresh
instance
mkdir -p "$home/.cargo/bin"
mv "$T/bin/cargo" "$home/.cargo/bin/cargo"
chmod 777 "$home/.cargo/bin"
t_env PATH="$T/bin:/usr/bin:/bin"
t_run "$install"
t_eq "$t_rc" 1
t_contains "$t_out" "$home/.cargo/bin is writable by everyone (chmod o-w)"
t_contains "$t_out" "nothing was installed"
t_eq "$(find "$th" -type f ! -path "$th/state/*" 2>/dev/null | wc -l)" 0
chmod 755 "$home/.cargo/bin"
t_run "$install"
t_eq "$t_rc" 0
t_stub_cargo
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
# The word before the version is lower case, like pi-rukas on the front pages.
t_contains "$(cat "$T_ROOT/host/docs/_layout.html")" 'tehas {{ readFile "/version.txt" | trim | html }}'
# The front pages read their versions from the factory's health instead
# (host/fun/versions.js). A rukas-version.txt an older build left is the
# host's: an install neither removes it nor names it stale.
for page in "$T_ROOT/host/fun/index.html" "$T_ROOT/host/fun/business.html"; do
  t_lacks "$(cat "$page")" 'version.txt'
  t_contains "$(cat "$page")" '<script src="/fun/versions.js"></script>'
done
echo v0.14.29 >"$th/fun/rukas-version.txt"
t_run "$tagged/install.sh --check"
t_lacks "$t_out" "rukas-version.txt"

t_case "a checkout without the console's source installs nothing and exits 1"
fresh
instance
bare=$T/bare
rm -rf "$bare"
mkdir -p "$bare"
cp -R "$T_ROOT/install.sh" "$T_ROOT/lib" "$T_ROOT/host" "$T_ROOT/bin" "$T_ROOT/libexec" "$bare/"
t_tree_commit "$bare"
t_run "$bare/install.sh"
t_eq "$t_rc" 1
t_contains "$t_out" "$bare/console is missing from this checkout"
t_eq "$(host_files | wc -l)" 0

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

t_case "a change to the attachment checker alone builds it alone, and its tests build nothing"
rm -f "$T/calls/cargo"
echo "// changed" >>"$part/console/attach/src/main.rs"
t_run "$part/install.sh --check"
t_contains "$t_out" "differs: $th/libexec/attach"
t_lacks "$t_out" "differs: $th/libexec/console"
t_run "$part/install.sh"
t_eq "$t_rc" 0
t_eq "$(t_calls cargo)" "build --release --locked --offline --quiet --manifest-path $part/console/Cargo.toml --target-dir $T/target -p attach"
t_eq "$(stat -c %a "$th/libexec/attach")" 755
rm -f "$T/calls/cargo"
echo "// changed" >>"$part/console/attach/src/tests.rs"
t_run "$part/install.sh"
t_eq "$(t_calls cargo)" ""

# The refresh text the dispatcher embeds is a file of its own package, so a
# change to it alone makes the dispatcher due: an install must not keep one that
# prints the old help.
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
