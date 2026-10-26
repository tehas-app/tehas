#!/usr/bin/env bash
# Tests for install.sh: the slice, drift reported and repaired, a changed
# versions.env, and the console built once and again.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

# The cases below start from a host that is installed and owes nothing.
fresh
instance
t_run "$install"
rm -f "$th/state/update-pending"

# The slice is root's to install (sudo ./bootstrap.sh slice). install.sh only
# compares the checkout's file with the one under /etc and says what it found.
# Every container runs in the slice, so a host without it, or with another
# version of it, has something to do: the check exits 1 until it is done.
t_case "--check names a missing slice and what installs it, and exits 1"
mkdir -p "$T/etc/systemd/system"
T_ENV=(T_IN_TEST=1 T_SLICE=1 BOOTSTRAP_ETC="$T/etc")
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Slice:        MISSING (sudo ./bootstrap.sh slice)"
t_contains "$t_out" "Installed:    OK"

t_case "--check says OK for the slice file this checkout ships, and exits 0"
cp "$T_ROOT/host/slice/tehas.slice" "$T/etc/systemd/system/tehas.slice"
t_run "$install --check"
t_eq "$t_rc" 0
t_contains "$t_out" "Slice:        OK"

t_case "--check shows the last tehas check-product, which does not change its status"
t_run "$install --check"
t_lacks "$t_out" "Product:"
printf '%s\n' '{"v":1,"at":"2026-10-21T09:00:00Z","ok":false,"failures":["no AGENTS.md"],"sandbox":null}' >"$th/state/check-product.json"
t_run "$install --check"
t_eq "$t_rc" 0
t_contains "$t_out" "Product:      1 TO FIX (tehas check-product, 2026-10-21T09:00:00Z)
  no AGENTS.md"
printf '%s\n' '{"v":1,"at":"2026-10-21T09:00:00Z","ok":true,"failures":[],"sandbox":null}' >"$th/state/check-product.json"
t_run "$install --check"
t_contains "$t_out" "Product:      OK (tehas check-product"
rm -f "$th/state/check-product.json"

t_case "--check names a slice file that differs, and exits 1"
echo "MemoryMax=10%" >>"$T/etc/systemd/system/tehas.slice"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Slice:        DIFFERS (sudo ./bootstrap.sh slice)"
t_contains "$t_out" "Installed:    OK"

t_case "a real run notes a slice that differs or is missing, and installs no slice"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "note: $T/etc/systemd/system/tehas.slice differs from host/slice/tehas.slice in this checkout (sudo ./bootstrap.sh slice)"
rm "$T/etc/systemd/system/tehas.slice"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "note: $T/etc/systemd/system/tehas.slice is missing (sudo ./bootstrap.sh slice)"
t_eq "$(find "$T/etc" -type f | wc -l)" 0
t_eq "$(installed_files | grep -c tehas.slice)" 0

t_case "a real run says nothing of a slice that is in place"
cp "$T_ROOT/host/slice/tehas.slice" "$T/etc/systemd/system/tehas.slice"
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$t_out" "note:"
t_lacks "$t_out" "tehas.slice"
rm -rf "${T:?}/etc"
T_ENV=(T_IN_TEST=1)

t_case "--check reports drift, changes nothing and exits 1"
echo "# edited on the host" >>"$th/libexec/dispatch"
before=$(installed_files | xargs -I{} sh -c "cd '$home' && cksum '{}'")
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Installed:    DRIFT (1 files)"
t_contains "$t_out" "differs: $th/libexec/dispatch"
t_eq "$(installed_files | xargs -I{} sh -c "cd '$home' && cksum '{}'")" "$before"

t_case "--check reports a command whose mode is wrong, changes nothing and exits 1"
# Only the mode is wrong here, so the content edit of the case before this one
# is put back from the checkout first. The edit is restored below, where the
# cases that need it come again.
cp "$T_ROOT/libexec/dispatch" "$th/libexec/dispatch"
chmod 644 "$th/libexec/dispatch"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Installed:    DRIFT (1 files)"
t_contains "$t_out" "mode: $th/libexec/dispatch is 644, wants 755"
t_eq "$(stat -c %a "$th/libexec/dispatch")" 644

t_case "a real run sets the mode of a command whose content already matches"
# The content is already the checkout's, so the only thing owed is the mode: no
# copy is owed either, so no .prev appears beside it.
before=$(cksum <"$th/libexec/dispatch")
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(stat -c %a "$th/libexec/dispatch")" 755
t_eq "$(cksum <"$th/libexec/dispatch")" "$before"
t_eq "$([ -e "$th/libexec/dispatch.prev" ] && echo there)" ""
t_run "$install --check"
t_contains "$t_out" "Installed:    OK"
t_eq "$t_rc" 0

t_case "a file installed with the mode the list says is not reported"
t_eq "$(stat -c %a "$th/bin/tehas")" 755
t_eq "$(stat -c %a "$th/env.sh")" 644
t_run "$install --check"
t_eq "$t_rc" 0
t_lacks "$t_out" "mode:"

t_case "a unit file with the wrong mode is drift, and a real run puts it right"
unit=$T/home/.config/systemd/user/tehas-dispatch.service
before=$(cksum <"$unit")
chmod 600 "$unit"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "mode: $unit is 600, wants 644"
t_run "$install"
t_eq "$(stat -c %a "$unit")" 644
t_eq "$(cksum <"$unit")" "$before"

# The content edit the mode cases below displaced is put back for the cases
# after this block, which install dispatch and expect a copy to be owed.
echo "# edited on the host" >>"$th/libexec/dispatch"

t_case "a copy that fails halfway leaves the installed file as it was"
before=$(cksum <"$th/libexec/dispatch")
# An install that writes part of the file where it was told to, then fails.
t_stub install 'head -c 100 "$3" >"$4"; exit 1'
t_run "$install"
rm -f "$T/bin/install"
t_eq "$t_rc" 1
t_contains "$t_out" "tehas: could not install $th/libexec/dispatch"
t_eq "$(cksum <"$th/libexec/dispatch")" "$before"
t_eq "$(find "$th/libexec" -name '*.new' | wc -l)" 0

t_case "a real run repairs drift and keeps the old file as .prev"
t_run "$install"
t_contains "$t_out" "installed: $th/libexec/dispatch"
t_eq "$(cmp -s "$th/libexec/dispatch" "$T_ROOT/libexec/dispatch" && echo same)" "same"
t_contains "$(cat "$th/libexec/dispatch.prev")" "# edited on the host"

t_case "a changed versions.env asks for bootstrap.sh user on a later line"
cp "$th/versions.env" "$T/versions.env.kept"
echo "# an older pin" >>"$th/versions.env"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "later: $T_ROOT/bootstrap.sh user   (versions.env changed"
t_lacks "$t_out" "next: bootstrap"
t_run "$install"
t_lacks "$t_out" "bootstrap.sh user"
cmp -s "$th/versions.env" "$T/versions.env.kept" || t_bad "versions.env was not put back"

t_case "the console, analytics, the MCP server, the store, the attachment checker and the dispatcher's shadow are built once each, from the lock file, and installed as programs"
t_eq "$(t_calls cargo)" "build --release --locked --offline --quiet --manifest-path $T_ROOT/console/Cargo.toml --target-dir $T/target -p console
build --release --locked --offline --quiet --manifest-path $T_ROOT/console/Cargo.toml --target-dir $T/target -p analytics
build --release --locked --offline --quiet --manifest-path $T_ROOT/console/Cargo.toml --target-dir $T/target --target x86_64-unknown-linux-musl -p tehas-mcp
build --release --locked --offline --quiet --manifest-path $T_ROOT/console/Cargo.toml --target-dir $T/target -p store
build --release --locked --offline --quiet --manifest-path $T_ROOT/console/Cargo.toml --target-dir $T/target -p attach
build --release --locked --offline --quiet --manifest-path $T_ROOT/console/Cargo.toml --target-dir $T/target -p tehas-dispatch"
t_eq "$(stat -c %a "$th/libexec/tehas-mcp")" 755
t_eq "$(stat -c %a "$th/libexec/tehas-dispatch")" 755
t_eq "$(stat -c %a "$th/libexec/attach")" 755
t_eq "$(stat -c %a "$th/libexec/store")" 755
t_eq "$(stat -c %a "$th/libexec/console")" 755
t_eq "$(cat "$th/libexec/console")" "a console built from source"
t_eq "$(stat -c %a "$th/libexec/analytics")" 755
t_eq "$(cat "$th/libexec/analytics")" "an analytics built from source"

t_case "a console built from other source is drift, and --check builds nothing"
sed -i 's/^[0-9a-f]*/0000/' "$th/libexec/console.stamp"
rm -f "$T/calls/cargo"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Installed:    DRIFT (1 files)"
t_contains "$t_out" "differs: $th/libexec/console"
t_eq "$(t_calls cargo)" ""

t_case "a real run builds it again"
t_env CONSOLE_BUILD="new source"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "installed: $th/libexec/console"
t_eq "$(cat "$th/libexec/console")" "a console built from new source"
t_eq "$(cat "$th/libexec/console.prev")" "a console built from source"
t_eq "$(t_calls cargo)" "build --release --locked --offline --quiet --manifest-path $T_ROOT/console/Cargo.toml --target-dir $T/target -p console"
t_eq "$(cat "$th/libexec/analytics")" "an analytics built from source"
t_run "$install --check"
t_contains "$t_out" "Installed:    OK"

t_case "a console changed on the host is drift though its stamp is as it was"
echo "# another program" >"$th/libexec/console"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "differs: $th/libexec/console"
t_run "$install"
t_contains "$t_out" "installed: $th/libexec/console"
t_eq "$(cat "$th/libexec/console")" "a console built from new source"
t_eq "$(cat "$th/libexec/console.prev")" "# another program"
t_run "$install --check"
t_contains "$t_out" "Installed:    OK"
t_env

t_done
