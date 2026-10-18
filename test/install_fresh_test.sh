#!/usr/bin/env bash
# Tests for install.sh: what a fresh install puts, the update lock, and
# what --check and --check --tests report on a matching install.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

t_case "a fresh install: exactly these files"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(core_files)" "$(sorted "$files")"
# The two pause commands are installed beside the other commands, ready to run.
t_eq "$([ -x "$th/libexec/pause" ] && [ -x "$th/libexec/resume" ] && echo both)" "both"
# Every script in libexec/ is installed: a command bin/tehas knows has its script.
missing=""
for f in "$T_ROOT"/libexec/*; do [ -x "$th/libexec/${f##*/}" ] || missing="$missing ${f##*/}"; done
for f in "$T_ROOT"/lib/*.sh; do [ -f "$th/lib/${f##*/}" ] || missing="$missing ${f##*/}"; done
t_eq "$missing" ""
# The follow-ups these copies call for are asked for by id and recorded as owed.
t_eq "$(printf '%s\n' "$t_out" | sed -n 's/^next: //p')" "$(next_steps)"
t_eq "$(cat "$th/state/update-pending")" "$(next_steps)"
# Every file in lib/ is installed: the commands source them from there.
t_eq "$(cd "$T_ROOT" && git ls-files lib | while read -r f; do [ -f "$th/$f" ] || echo "$f"; done)" ""
# install.sh asks systemd for nothing: a copied unit file is not in effect until
# the step that restarts the units has reloaded it.
t_eq "$(t_calls systemctl)" ""
# It makes the previews' network where there is none, and asks nothing else.
t_eq "$(t_calls docker)" "$net_inspect"
t_lacks "$t_out" "tehas-previews"
t_eq "$([ -d "$th/preview" ] && echo there)" "there"
# A host gets its tools from bootstrap.sh before its first install.
t_lacks "$t_out" "bootstrap.sh user"

t_case "another update holding the lock stops a real run before it installs anything"
fresh
instance
mkdir -p "$th/state"
lock=$th/state/update.lock
t_run "exec 8>>'$lock'; flock -n 8 || exit 9; '$install'"
t_eq "$t_rc" 1
t_contains "$t_out" "tehas: another update is running ($lock); nothing was installed"
t_eq "$(core_files)" ""
t_eq "$(t_calls cargo)" ""

t_case "the lock handed over by the update holding it is taken"
t_run "exec 8>>'$lock'; flock -n 8 || exit 9; TEHAS_UPDATE_LOCK_FD=8 '$install'"
t_eq "$t_rc" 0
t_eq "$(core_files)" "$(sorted "$files")"

t_case "a descriptor on the lock file that does not hold the lock is not taken for it"
fresh
instance
mkdir -p "$th/state"
t_run "( exec 6>>'$lock'; flock -n 6 && : >'$T/held' && exec sleep 20 ) &
while [ ! -e '$T/held' ]; do sleep 0.1; done
exec 8>>'$lock'; TEHAS_UPDATE_LOCK_FD=8 '$install'; rc=\$?; kill \$!; wait; exit \$rc"
rm -f "$T/held"
t_eq "$t_rc" 1
t_contains "$t_out" "another update is running"
t_eq "$(core_files)" ""

t_case "--check runs while an update holds the lock"
t_run "exec 8>>'$lock'; flock -n 8 || exit 9; '$install' --check"
t_lacks "$t_out" "another update is running"
t_contains "$t_out" "Installed:    DRIFT"

t_case "--check makes no network"
fresh
instance
t_run "$install"
rm -f "$T/calls/docker"
t_run "$install --check"
t_eq "$(t_calls docker)" ""

t_case "the manual and the front page are installed"
t_eq "$(cd "$th/docs" && find . -type f ! -name version.txt | wc -l)" "$(find "$T_ROOT/host/docs" -type f | wc -l)"
t_eq "$(cd "$th/fun" && find . -type f ! -name version.txt | wc -l)" "$(find "$T_ROOT/host/fun" -type f | wc -l)"

t_case "the instance's model list is what Pi gets"
t_eq "$(cat "$home/.pi/agent/models.json")" "$(cat "$TEHAS_INSTANCE_DIR/models.json")"

t_case "nothing installed names a file that is absent"
missing=""
for unit in "$home"/.config/systemd/user/*; do
  while read -r ref; do
    # The path unit watches for a file a cycle leaves and a poll removes.
    case "$ref" in */logs/* | */state/dispatch.again) ref=$(dirname "$ref") ;; esac
    [ -e "$ref" ] || missing="$missing $ref"
  done < <(grep -o '%h/tehas/[A-Za-z0-9_./-]*' "$unit" | sed "s|%h|$home|")
done
for file in docs.compose.yaml router.compose.yaml; do
  for ref in $(grep -o '"\.\.\?/[A-Za-z0-9_./-]*:' "$th/edge/$file" | tr -d '":'); do
    [ -e "$th/edge/$ref" ] || missing="$missing $ref"
  done
done
t_eq "$missing" ""
t_case "a second run changes nothing"
t_run "$install"
t_contains "$t_out" "tehas is up to date"

t_case "--check on a matching install reports five lines and exits 0"
# The follow-ups of the install above are taken as done, which is what running
# tehas update does to the record. A host that still owes one is another case.
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 0
t_eq "$t_out" "Instance:     $T/instance OK
Dependencies: not checked (running inside the tests)
Slice:        not checked (running inside the tests)
Installed:    OK
Tests:        not run (./install.sh --check --tests runs them)"

t_case "--check --tests runs the suite and reports it"
# A stand-in for the suite: these tests never start the real one, which would
# run them again.
printf '#!/usr/bin/env bash\n: >"%s/suite-ran"\nexit "$(cat "%s/suite-rc")"\n' "$T" "$T" >"$T/bin/suite"
chmod +x "$T/bin/suite"
echo 0 >"$T/suite-rc"
t_env TEHAS_TEST_SUITE="$T/bin/suite"
rm -f "$T/suite-ran"
t_run "$install --check"
t_eq "$t_rc" 0
t_eq "$([ -e "$T/suite-ran" ] && echo ran)" ""
t_run "$install --check --tests"
t_eq "$t_rc" 0
t_eq "$([ -e "$T/suite-ran" ] && echo ran)" ran
t_contains "$t_out" "Tests:        OK"

t_case "--check --tests with a failing suite says FAILED and exits 1"
echo 1 >"$T/suite-rc"
t_run "$install --check --tests"
t_eq "$t_rc" 1
t_contains "$t_out" "Tests:        FAILED"

t_case "--check --tests inside the tests with no stand-in runs nothing"
t_env
rm -f "$T/suite-ran"
t_run "$install --check --tests"
t_eq "$t_rc" 0
t_contains "$t_out" "Tests:        not checked (running inside the tests)"
t_eq "$([ -e "$T/suite-ran" ] && echo ran)" ""


t_done
