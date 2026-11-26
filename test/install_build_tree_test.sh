#!/usr/bin/env bash
# Tests for install.sh's build, split from install_build_test.sh: a tool
# directory another account can write, the footers' version, a checkout
# without the console's source, and a --check console that cannot read the tree.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

# The tool directories come first on PATH and build the console every cycle
# runs, so one another account can write is refused before a build.
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
cp -R "$fixture/install.sh" "$fixture/lib" "$fixture/host" "$fixture/bin" \
  "$fixture/libexec" "$tagged/"
t_console_source "$tagged" "$fixture"
t_run "$tagged/install.sh"
t_eq "$t_rc" 0
# Not a repository of its own: no version, and the pages show none.
t_eq "$(find "$th/docs" "$th/fun" -name version.txt | wc -l)" 0
commit() { git -C "$tagged" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m "$1"; }
# The version is the console's (console/ops/src/update/tree.rs): it reaches
# the pages.
git init -q -b main "$tagged"
commit first
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
# The word before the version is lower case, like pi-rukas on the dashboard.
t_contains "$(cat "$fixture/host/docs/_layout.html")" '{{ if and (not $isFrag) (fileExists "/version.txt") }}<p class="credit">tehas {{ readFile "/version.txt" | trim | html }}</p>'
# The dashboard shows one versions line, from the factory's health
# (host/fun/versions.js); a rukas-version.txt an older build left stays.
page=$fixture/host/docs/dashboard.frag
t_lacks "$(cat "$page")" 'version.txt'
t_contains "$(cat "$page")" '<script src="/fun/versions.js"></script>'
echo v0.14.29 >"$th/fun/rukas-version.txt"
t_run "$tagged/install.sh --check"
t_lacks "$t_out" "rukas-version.txt"

t_case "a checkout without the console's source installs nothing and exits 1"
fresh
instance
bare=$T/bare
rm -rf "$bare"
mkdir -p "$bare"
cp -R "$fixture/install.sh" "$fixture/lib" "$fixture/host" "$fixture/bin" \
  "$fixture/libexec" "$bare/"
t_tree_commit "$bare"
t_run "$bare/install.sh"
t_eq "$t_rc" 1
t_contains "$t_out" "$bare/console is missing from this checkout"
t_eq "$(host_files | wc -l)" 0

# --check builds nothing, so the console it finds can lack a verb of this tree.
t_case "a --check whose console cannot read the tree still reports, and says a real run builds the one it needs"
fresh
instance
t_run "$install"
old=$T/old-console
printf '#!/usr/bin/env bash\n[ "$1 $2" != "update tree" ] || exit 64\nexec %q "$@"\n' "$T_CONSOLE" >"$old"
chmod 755 "$old"
t_run "TEHAS_SETTINGS_CMD='$old' $install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Store:        REFUSED (the console at $old cannot read the commit of "
t_contains "$t_out" "(status 64); a real run of install.sh builds the one this tree needs"
t_run "TEHAS_SETTINGS_CMD='$old' $install"
t_eq "$t_rc" 1
t_contains "$t_out" "could not read the commit of $fixture (status 64); nothing was installed"

t_done
