#!/usr/bin/env bash
# Tests for libexec/update: what an update is asked to do, and the
# follow-ups it owes. The rest of the command, the seam against the real
# install.sh and the pi-rukas follow are in test/update_test.sh.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck disable=SC2329  # the push_* helpers are defined in test/update_test.sh
# shellcheck disable=SC2154  # t_out is set by test/lib.sh, which the part sources first
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/update_lib.sh
. "$(dirname "$0")/update_lib.sh" || exit 1
t_case "--check reports follow-ups an earlier run left undone, and starts none"
instance
platform 0 'echo "Instance:     $TEHAS_INSTANCE_DIR OK"
echo "Installed:    OK"'
mkdir -p "$th/state"
printf 'edge --force-recreate\n' >"$owed_file"
before=$(head_sha)
t_run "$update --check"
t_eq "$t_rc" 1
t_contains "$t_out" "follow-up(s) an earlier run did not finish"
t_contains "$t_out" "next: edge --force-recreate"
t_contains "$t_out" "Installed:    OK"
t_eq "$(head_sha)" "$before"
t_eq "$(t_calls docker)" ""
# Nothing was started and nothing was taken back off the list.
t_eq "$(owed_steps)" "edge --force-recreate"

t_case "a checkout on a branch that is not the remote's default says so"
instance
platform 0 'echo "installed: nothing"'
push_branch other
git -C "$checkout" fetch -q origin
git -C "$checkout" symbolic-ref refs/remotes/origin/HEAD refs/remotes/origin/main
git -C "$checkout" checkout -q other
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "checkout: $checkout is on branch other; the default branch of origin is main"
t_contains "$t_out" "checkout: other is up to date at $(head_sha)"
t_eq "$(git -C "$checkout" symbolic-ref --quiet --short HEAD)" "other"
t_eq "$(t_calls install)" "install "

t_case "a dirty checkout changes nothing and runs no install.sh"
instance
platform 0 "$ASK"
before=$(state)
echo "half-finished work" >"$checkout/libexec/new-thing"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "has local changes; nothing was changed"
t_contains "$t_out" "libexec/new-thing"
t_eq "$(state)" "$before"
t_eq "$(t_calls install)" ""
t_eq "$(t_calls build)" ""

t_case "a held platform is not fetched, merged or installed, and the hold is named"
instance
platform 1 "$ASK"
before=$(state)
mkdir -p "$th/state"
: >"$th/state/update-hold"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "$th/state/update-hold exists"
t_eq "$(state)" "$before"
t_eq "$(t_calls timeout)" ""
t_eq "$(t_calls install)" ""
t_eq "$(t_calls build)" ""

t_case "--check on a held platform says so and still reports"
t_run "$update --check"
t_eq "$t_rc" 1
t_contains "$t_out" "an update will not move the checkout until it is removed"
t_contains "$t_out" "checkout: would fast-forward"
t_eq "$(state | cut -d' ' -f2)" "$(cut -d' ' -f2 <<<"$before")"
rm -f "$th/state/update-hold"

t_case "a checkout with commits of its own is not fast-forwarded"
instance
platform 0 "$ASK"
echo local >"$checkout/notes"
git -C "$checkout" add -A
git -C "$checkout" commit -qm "local work"
push_one "remote work"
before=$(head_sha)
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "cannot fast-forward to origin/main"
t_eq "$(head_sha)" "$before"
t_eq "$(t_calls install)" ""

t_case "a remote that cannot be reached stops before install.sh"
instance
platform 0 'echo "installed: nothing"'
git -C "$checkout" remote set-url origin "$T/no-such-place"
before=$(head_sha)
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "could not fetch from origin; nothing was changed"
t_eq "$(head_sha)" "$before"
t_eq "$(t_calls install)" ""

t_case "a checkout on a branch with no commit yet says what is missing"
instance
platform 0 'echo "installed: nothing"'
git -C "$checkout" checkout -q --orphan brand-new
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "on branch brand-new with no commit yet; nothing was changed"
t_lacks "$t_out" "cannot fast-forward"
t_eq "$(git -C "$checkout" symbolic-ref --quiet --short HEAD)" "brand-new"
t_eq "$(t_calls install)" ""

t_case "a detached checkout is refused rather than pulled into"
instance
platform 0 'echo "installed: nothing"'
git -C "$checkout" checkout -q --detach HEAD
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "not on a branch"
t_eq "$(t_calls install)" ""

t_case "a failed install stops before the follow-ups"
instance
platform 2 'echo "installed: libexec/dispatch"
echo "tehas: /x is missing from this checkout" >&2
exit 1'
before=$(head_sha)
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "install: install.sh exited 1; no follow-up was run"
# The pull happened: the checkout moved, and nothing after install.sh ran.
[ "$(head_sha)" = "$before" ] && t_bad "the checkout did not move to origin/main"
t_eq "$(t_calls install)" "install "
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""

t_case "invalid instance settings found by install.sh are passed through as 78"
instance
platform 0 'echo "tehas: models.json does not list model x" >&2
exit 78'
t_run "$update"
t_eq "$t_rc" 78
t_contains "$t_out" "install: install.sh exited 78"
t_eq "$(t_calls build)" ""

t_case "settings the pulled checkout adds are answered before anything is installed"
instance
platform 1 "$ASK"
push_settings_change
t_run "$update"
t_eq "$t_rc" 78
t_contains "$t_out" "the checkout this update pulled asks for settings this instance does not have"
t_contains "$t_out" "TEHAS_NEW_SETTING is not set"
t_eq "$(t_calls install)" ""
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""

t_case "a checkout whose config.sh predates the settings helpers defers to install.sh"
instance
platform 1 'echo "installed: nothing"'
push_old_config
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "does not define tehas_load; install.sh will check the settings itself"
t_lacks "$t_out" "asks for settings this instance does not have"
t_eq "$(t_calls install)" "install "

t_case "a pulled checkout has no console beside it, so install.sh checks the settings"
instance
platform 1 'echo "installed: nothing"'
push_settings_change
mkdir -p "$th/libexec"
cp "$TEHAS_SETTINGS_CMD" "$th/libexec/console"
t_run "unset TEHAS_SETTINGS_CMD; $update"
t_contains "$t_out" "this checkout has no console beside it; install.sh will check the settings itself"
t_lacks "$t_out" "asks for settings this instance does not have"
t_eq "$(t_calls install)" "install "

t_case "an update from before the console read settings leaves a pulled checkout to install.sh"
instance
platform 1 'echo "installed: nothing"'
push_one "remote work"
old=$T/old-install
rm -rf "$old"
mkdir -p "$old/libexec" "$th/libexec"
sed '/\[ ! -x "$1\/libexec\/console" \]/,+3d' "$update" >"$old/libexec/update"
chmod +x "$old/libexec/update"
t_lacks "$(cat "$old/libexec/update")" "has no console beside it"
cp -R "$T_ROOT/lib" "$old/lib"
cp "$TEHAS_SETTINGS_CMD" "$old/libexec/console"
printf '#!%s\necho "--since is required" >&2\nexit 2\n' "$T_BASH" >"$th/libexec/console"
chmod +x "$th/libexec/console"
t_run "unset TEHAS_SETTINGS_CMD; $old/libexec/update"
t_eq "$t_rc" 0
t_contains "$t_out" "does not define tehas_load; install.sh will check the settings itself"
t_lacks "$t_out" "asks for settings this instance does not have"
t_eq "$(t_calls install)" "install "

t_case "--check reports what would change and leaves the checkout as it was"
instance
platform 2 'echo "Instance:     $TEHAS_INSTANCE_DIR OK"
echo "Installed:    DRIFT (1 files)"
echo "  differs: '"$th"'/libexec/dispatch"
exit 1'
before=$(head_sha)
t_run "$update --check"
t_eq "$t_rc" 1
t_contains "$t_out" "checkout: would fast-forward"
t_contains "$t_out" "DRIFT (1 files)"
t_lacks "$t_out" "installed:"
# install.sh --check answers 1 when it found drift. That is its report, not a
# failed step: in check mode nothing was meant to run.
t_lacks "$t_out" "no follow-up was run"
t_contains "$t_out" "install: install.sh --check found something to update"
t_lacks "$t_out" "does not fix"
# Nothing moved: no merge, no file changed, nothing started.
t_eq "$(head_sha)" "$before"
t_eq "$(git -C "$checkout" status --porcelain)" ""
t_eq "$(t_calls install)" "install --check"
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""

t_case "--check exits 1 when only the checkout is behind"
instance
platform 2 'echo "Instance:     $TEHAS_INSTANCE_DIR OK"
echo "Installed:    OK"'
before=$(head_sha)
t_run "$update --check"
t_eq "$t_rc" 1
t_contains "$t_out" "checkout: would fast-forward $before"
t_contains "$t_out" "Installed:    OK"
t_eq "$(head_sha)" "$before"
t_eq "$(t_calls install)" "install --check"

t_case "--check names what an update does not fix apart from something to update"
for finding in "Dependencies: NOT OK" "Slice:        MISSING (sudo ./bootstrap.sh slice)" \
  "Login:        NOT CURRENT" "Tests:        FAILED"; do
  instance
  platform 0 'echo "Instance:     $TEHAS_INSTANCE_DIR OK"
echo "'"$finding"'"
echo "Installed:    OK"
exit 1'
  t_run "$update --check"
  t_eq "$t_rc" 1
  t_contains "$t_out" "$finding"
  t_contains "$t_out" "install: install.sh --check found something tehas update does not fix"
  t_lacks "$t_out" "found something to update"
done

t_case "--check on a host that is up to date reports it and exits 0"
instance
platform 0 'echo "Instance:     $TEHAS_INSTANCE_DIR OK"
echo "Installed:    OK"'
before=$(head_sha)
t_run "$update --check"
t_eq "$t_rc" 0
t_contains "$t_out" "checkout: main is up to date at $(head_sha)"
t_eq "$(head_sha)" "$before"
t_eq "$(t_calls install)" "install --check"

t_case "a follow-up that names no step this command knows is said out loud, not run"
instance
platform 0 'echo "next: rm -rf something"
echo "next: docker compose -f /somewhere/else.compose.yaml up -d"'
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: 2 step line(s) name no step this command knows"
t_contains "$t_out" "next: rm -rf something"
t_eq "$(t_calls docker)" ""
t_eq "$(t_calls build)" ""

t_case "a step line whose names are not unit names never reaches systemctl"
instance
platform 0 'echo "next: units --help tehas-dispatch.timer"'
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: 1 step line(s) name no step this command knows"
t_contains "$t_out" "next: units --help tehas-dispatch.timer"
t_eq "$(t_calls systemctl)" ""

t_case "a units step that names no unit is not run"
instance
platform 0 'echo "next: units"'
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: 1 step line(s) name no step this command knows"
t_contains "$t_out" "next: units"
t_eq "$(t_calls systemctl)" ""

t_case "a step asking for an argument it was not given is not run"
instance
platform 0 'echo "next: docs restart"'
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: 1 step line(s) name no step this command knows"
t_eq "$(t_calls docker)" ""

# The flag is part of the step, not decoration. docker compose up -d without
# --force-recreate leaves a running container serving the config it loaded at its
# start - so a bare docs would renew nothing, be counted as done, and leave the run
# green with a changed Caddyfile unapplied.
t_case "a compose step without its flag is not run, and stays owed"
instance
platform 0 'echo "next: docs"'
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: 1 step line(s) name no step this command knows"
t_contains "$t_out" "next: docs"
t_eq "$(t_calls docker)" ""
t_eq "$(owed_steps)" "docs"

t_case "install.sh's diagnostics are not read as follow-ups"
instance
platform 0 'echo "installed: libexec/dispatch"
echo "next: sandbox" >&2
echo "tehas: a step on this host did not run" >&2'
t_run "$update"
t_eq "$t_rc" 0
# stderr stays stderr, and is not parsed for steps: no build ran, and no WARN
# claims install.sh asked for one.
t_contains "$t_out" "tehas: a step on this host did not run"
t_contains "$t_out" "next: sandbox"
t_lacks "$t_out" "WARN:"
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""

t_case "a status of install.sh this command does not know is reported as 1"
instance
platform 0 'echo "bash: Permission denied" >&2
exit 126'
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "install: install.sh exited 126; no follow-up was run"
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""

t_case "a step recorded under the Protolab's old id renews the edge router"
instance
platform 1 'echo "installed: nothing"'
mkdir -p "$th/state"
printf 'protolab --force-recreate\n' >"$owed_file"
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "edge: updating the container"
t_contains "$(t_calls docker)" "compose -f $th/edge/router.compose.yaml up -d --force-recreate"
t_eq "$(owed_steps)" ""

t_case "a compose file that is not installed stops the update"
instance
ask='echo "next: sandbox"
echo "next: edge --force-recreate"'
platform 1 "$ask"
rm "$th/edge/router.compose.yaml"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "edge: $th/edge/router.compose.yaml is not installed, left alone"
t_lacks "$t_out" "edge: updating the container"
t_eq "$(t_calls docker)" ""
# The steps run in the order lib/update-steps.sh lists them: the build that came
# before the edge router ran, and the failure stopped the run after it.
t_eq "$(t_calls build)" "build"
t_eq "$(owed_steps)" "edge --force-recreate"

t_case "a sandbox build that is not installed stops the update"
instance
platform 0 'echo "next: sandbox"'
chmod -x "$th/sandbox/build.sh"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "sandbox: $th/sandbox/build.sh is not installed"
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""

t_case "a branch origin does not have stops before install.sh"
instance
platform 0 'echo "installed: nothing"'
git -C "$checkout" checkout -q -b local-only
before=$(head_sha)
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "checkout: origin has no branch local-only in $checkout; nothing was changed"
t_eq "$(git -C "$checkout" symbolic-ref --quiet --short HEAD)" "local-only"
t_eq "$(head_sha)" "$before"
t_eq "$(t_calls install)" ""

# The record is the memory an unfinished run leaves behind, so it is read as what
# it says or not at all. A file this account cannot open is not an answer of
# "nothing owed": the next write would replace steps somebody else recorded with
# what this run happened to know.
t_case "a follow-up record that cannot be read stops the update before anything runs"
instance
platform 1 "$ASK"
before=$(state)
mkdir -p "$th/state"
printf 'sandbox\ndocs --force-recreate\n' >"$owed_file"
chmod 000 "$owed_file"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "cannot read the follow-up record $owed_file"
t_contains "$t_out" "nothing was run"
# Nothing ran: no merge, no install.sh, no step.
t_eq "$(state)" "$before"
t_eq "$(t_calls install)" ""
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""
# The steps that were owed are still there.
chmod 644 "$owed_file"
t_eq "$(owed_steps)" "sandbox
docs --force-recreate"

t_case "a record that turned unreadable after install.sh stops the follow-ups"
instance
platform 1 'echo "next: sandbox"
chmod 000 "$TEHAS_HOME/state/update-pending"'
mkdir -p "$th/state"
printf 'docs --force-recreate\n' >"$owed_file"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "cannot be read; no follow-up was run"
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""
chmod 644 "$owed_file"
t_eq "$(owed_steps)" "docs --force-recreate"

t_case "a record that cannot be written stops the run before any step"
instance
platform 1 'echo "next: sandbox"'
mkdir -p "$th/state"
# The lock is there from an earlier run; only the record cannot be written.
: >"$th/state/update.lock"
chmod 500 "$th/state"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "could not make a temporary file beside $owed_file"
t_eq "$(t_calls build)" ""
chmod 755 "$th/state"

t_case "a state directory where the lock cannot be made stops the run before git"
instance
platform 1 'echo "next: sandbox"'
before=$(state)
mkdir -p "$th/state"
chmod 500 "$th/state"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "tehas update: cannot open $th/state/update.lock; nothing was changed"
t_eq "$(state)" "$before"
t_eq "$(t_calls install)" ""
chmod 755 "$th/state"

t_case "a failed run leaves the record, and nothing else, in state/"
instance
platform 1 "$ASK"
docker_no_answer
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "2 of 4 follow-up(s) are not done"
# Each rewrite goes through a temporary file moved over the record, so a run that
# died mid-write leaves the steps already owed there rather than half of a new list
# - and a run that finished leaves no stray file. The lock is kept: removing it
# would let a run that opened it a moment before lock a file no longer there.
t_eq "$(ls -A "$th/state")" "update-pending
update.lock"
t_eq "$(owed_steps)" "docs --force-recreate
edge --force-recreate"

t_done
