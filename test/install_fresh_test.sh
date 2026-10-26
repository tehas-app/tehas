#!/usr/bin/env bash
# Tests for install.sh: what a fresh install puts, the update lock, and
# what --check and --check --tests report on a matching install.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

t_case "--check on a home not made yet makes nothing, the marker neither"
fresh
instance
t_run "$install --check"
t_eq "$([ -e "$th" ] && echo made)" ""

# The names of a home that the list install.sh and this suite keep together in
# test/install_lib.sh does not name, and the names the list promises that the
# home lacks, one line each. A caller passes two newline separated sets of names
# in any order; each name is one line, with no trailing space or tab, and
# appears once. A side that names nothing is refused: an empty list is a mistake
# of the caller, and a report built from it would read as a clean home. The
# direction is part of each line, not of the argument order: `expected '', got
# './tehas/lib/x.sh'` reads like a stray artefact rather than like a file nobody
# added to the list, and a report that named the wrong half sent its reader to
# edit the wrong file.
install_list_report() { # <names the list promises> <names the home holds>
  local expected installed one
  if [ "$#" != 2 ]; then
    printf 'install_list_report wants two arguments, got %s\n' "$#" >&2
    return 2
  fi
  expected=$1
  installed=$2
  # A side that names nothing is refused before anything is compared: an empty
  # list is a mistake of the caller, and a report built from one would read as a
  # clean home. The arguments stand on their own here, so a name that is nothing
  # but blank is refused too rather than reaching comm as a file named nothing.
  if [ -z "${expected//[$' \t\r\n']}" ] || [ -z "${installed//[$' \t\r\n']}" ]; then
    echo 'install_list_report: a side names no file at all' >&2
    return 2
  fi
  # comm answers only when both sides share one order, and puts its complaint on
  # a stderr the harness discards: the assertion below would read it as an empty
  # report. So each side is sorted first, and the sorted sides are checked by the
  # very order comm will read them in.
  expected=$(printf '%s\n' "$expected" | LC_ALL=C sort)
  installed=$(printf '%s\n' "$installed" | LC_ALL=C sort)
  if ! LC_ALL=C sort -c <<<"$expected" ||
    ! LC_ALL=C sort -c <<<"$installed"; then
    echo 'install_list_report: a side does not sort' >&2
    return 2
  fi
  # One line per name, so comm sees one line per file and nothing else.
  expected=$expected$'\n'
  installed=$installed$'\n'
  while read -r one; do
    printf 'new installed file %s: add it to the files= list of install.sh and of test/install_lib.sh, or stop shipping it\n' "$one"
  done < <(comm -13 <(printf '%s' "$expected") <(printf '%s' "$installed"))
  while read -r one; do
    printf 'missing installed file %s: install.sh does not copy it into the home\n' "$one"
  done < <(comm -23 <(printf '%s' "$expected") <(printf '%s' "$installed"))
}

t_case "a fresh install: exactly these files"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(install_list_report "$(sorted "$files")" "$(core_files)")" ""
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
# A first install makes the labels and enables the core units, and asks systemd
# nothing else: a copied unit file is not in effect until the step that
# restarts the units has reloaded it.
t_eq "$(t_calls systemctl)" "--user daemon-reload
--user enable --now tehas-dispatch.timer tehas-dispatch.path"
t_contains "$(t_calls gh)" "label create tehas:ready --repo acme/widget"
t_contains "$t_out" "first install: labels made in acme/widget, enabled tehas-dispatch.timer tehas-dispatch.path"
t_eq "$([ -e "$th/state/first-install" ] && echo owed)" ""
# It makes the previews' network where there is none, and asks nothing else.
t_eq "$(t_calls docker)" "$net_inspect"
t_lacks "$t_out" "tehas-previews"
t_eq "$([ -d "$th/preview" ] && echo there)" "there"
# A host gets its tools from bootstrap.sh before its first install.
t_lacks "$t_out" "bootstrap.sh user"
# The run that makes the home leaves the mark that says it is tehas's.
t_eq "$([ -f "$th/$install_marker" ] && echo there)" "there"

t_case "another update holding the lock stops a real run before it installs anything"
fresh
instance
mkdir -p "$th/state"
lock=$th/state/update.lock
# The case below that asks for the same refusal is timed; this one does the
# steps that cost seconds before it gets to the lock, so it is not. The exact
# line is what pins the knob: the 0 in it is the value the case exported, so a
# wait install.sh made up for itself would name other seconds here.
t_run "$no_wait exec 8>>'$lock'; flock -n 8 || exit 9; '$install'"
t_eq "$t_rc" 1
t_contains "$t_out" "tehas: another update is running ($lock), still held after 0s; nothing was installed"
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
# Timed with the same bound as the case in test/update_test.sh: the refusal is
# meant to come at once and the default wait is 10 seconds, so a knob that does
# not reach install.sh is red here. no_wait is named beside t_run in test/lib.sh.
t_start
t_run "$no_wait ( exec 6>>'$lock'; flock -n 6 && : >'$T/held' && exec sleep 20 ) &
while [ ! -e '$T/held' ]; do sleep 0.1; done
exec 8>>'$lock'; TEHAS_UPDATE_LOCK_FD=8 '$install'; rc=\$?; kill \$!; wait; exit \$rc"
rm -f "$T/held"
t_eq "$t_rc" 1
t_fast 5000
t_contains "$t_out" "another update is running"
t_eq "$(core_files)" ""

# The other half of the wait: a run that follows one which stopped partway finds
# the lock held for a moment and gets in once it is let go. The wait is derived
# from the hold below, so the two cannot drift apart. A refusal exits 1 and names
# the lock with the seconds it waited, which is what the line after the run reads:
# a -n restored on either branch, or a wait the script never asked for, is a red
# case here rather than a green one that refused at once. Whether that number is
# the knob's value or the default of 10 is pinned elsewhere: install.sh echoes
# what lib/update-steps.sh's tehas_update_lock_wait answered, and the refusal case
# above pins the 0 it echoes for the same knob. It runs on a home of its own: an
# install that finished would leave the home below matching this checkout, and
# "--check on a matching install reports six lines and exits 0" after it would
# have nothing left to find.
t_case "an install that finds the lock held waits for it and installs when it is let go"
mkdir -p "$th/state"
lock=$th/state/update.lock
late=$T/home/late/tehas
# A second shorter than the copy of a fresh install, so the install is stopped at
# the lock rather than by the holder letting go first; no bound is put on the run,
# because the copy's own cost would have to fit inside one.
hold_s=1
wait_s=$((hold_s + 4))
( exec 6>>"$lock"; flock -n 6; sleep "$hold_s" ) & holder=$!
t_run "export TEHAS_UPDATE_LOCK_WAIT=$wait_s; TEHAS_HOME='$late' '$install'"
wait "$holder"
t_eq "$t_rc" 0
t_lacks "$t_out" "another update is running"
# The files of a fresh install, asked of the second home: the wait was taken,
# and the install ran. The units go where $HOME puts them, so they are not part
# of a home; the state directory, the manual and the front page, and the marker
# that says the home is tehas's, are each asked about by a case of their own.
# Compared against the expected list rather than the home above, which an earlier
# case has left with the <name>.prev files of the copies it replaced.
t_eq "$(cd "$late" && find . -type f ! -path './state/*' ! -path './docs/*' ! -path './fun/*' ! -name '.tehas-home' | LC_ALL=C sort)" \
  "$(printf '%s\n' "${files[@]}" | grep -v -e '^\./\.bashrc$' -e '^\./\.config/' -e '^\./\.pi/' -e '^\./tehas/docs/' -e '^\./tehas/fun/' -e '^\./tehas/state/' -e "^\./tehas/$install_marker$" | sed 's|^\./tehas/|./|' | LC_ALL=C sort)"
t_eq "$([ -f "$late/.tehas-home" ] && echo there)" "there"

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

t_case "the instance's model list is what Pi gets, with the keys it left out filled"
# By parts, so key order is no part of the answer.
t_eq "$(jq -c '[.providers[].models[].id]' "$home/.pi/agent/models.json")" \
  "$(jq -c '[.providers[].models[].id]' "$TEHAS_INSTANCE_DIR/models.json")"
t_eq "$(jq -c '[.providers[].models[] | .name == .id] | unique' "$home/.pi/agent/models.json")" '[true]'
t_eq "$(jq -c '[.providers[].models[] | .input, .reasoning] | unique' "$home/.pi/agent/models.json")" '[false,["text"]]'

t_case "nothing installed names a file that is absent"
missing=""
for unit in "$home"/.config/systemd/user/*; do
  while read -r ref; do
    # The path unit watches for a file a cycle leaves and a poll removes, and
    # the poll's unit keeps the poll's lines in a file of its own.
    case "$ref" in */logs/* | */state/dispatch.again | */state/dispatch.last) ref=$(dirname "$ref") ;; esac
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

t_case "--check on a matching install reports six lines and exits 0"
# The follow-ups of the install above are taken as done, which is what running
# tehas update does to the record. A host that still owes one is another case.
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 0
t_eq "$t_out" "Instance:     $T/instance OK
Dependencies: not checked (running inside the tests)
Slice:        not checked (running inside the tests)
Installed:    OK
Shell:        OK
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


t_case "a second install makes no labels and enables nothing"
rm -f "$T"/calls/*
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(t_calls gh)" ""
# Only the reset of a tehas unit systemd failed, which enables nothing.
t_eq "$(t_calls systemctl)" "--user is-failed tehas-dispatch.path
--user reset-failed tehas-*"

t_case "a host installed before needs no first install"
t_run "$install"
t_lacks "$t_out" "first install"

t_case "the optional units are enabled on a first install only when their setting is on"
fresh
instance TEHAS_IMPROVE_REPO=acme/improve TEHAS_DOMAIN=widget.test TEHAS_PREVIEWS=1
mkdir -p "$home/.config/tehas" && echo TEHAS_ENERGY=on >"$home/.config/tehas/env"
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(sed -n 2p "$T/calls/systemctl")" \
  "--user enable --now tehas-dispatch.timer tehas-dispatch.path tehas-improve.timer tehas-protolab.timer tehas-preview.timer tehas-preview-sweep.timer tehas-energy.service"

t_case "a domain without previews enables the Protolab and not the previews"
fresh
instance TEHAS_DOMAIN=widget.test
t_run "$install"
t_eq "$(sed -n 2p "$T/calls/systemctl")" "--user enable --now tehas-dispatch.timer tehas-dispatch.path tehas-protolab.timer"

t_case "labels that cannot be made leave the first install owed, and the next run does it"
fresh
instance
t_stub gh 'exit 1'
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "later: ./install.sh again   (the first install's labels or units are not done: tehas labels failed)"
t_eq "$([ -e "$th/state/first-install" ] && echo owed)" "owed"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "First install: NOT DONE (labels and core units; ./install.sh does them)"
t_stub gh
rm -f "$T"/calls/*
t_run "$install"
t_contains "$t_out" "first install: labels made in acme/widget"
t_eq "$([ -e "$th/state/first-install" ] && echo owed)" ""
t_contains "$(t_calls systemctl)" "--user enable --now tehas-dispatch.timer tehas-dispatch.path"

# The round trip of the failure line above: a home with one file too many is
# reported by name and given the advice, and a file the list promises that the
# home lacks is reported too, rather than comparing as an exact match.
t_case "the install list and the home are compared in both directions"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
# A file no install list names, in a directory install.sh owns and takes whole:
# the name cannot be on the list without the list being changed.
echo '# a file no install list names' >"$th/libexec/stray-probe"
t_eq "$(install_list_report "$(sorted "$files")" "$(core_files)")" \
  "new installed file ./tehas/libexec/stray-probe: add it to the files= list of install.sh and of test/install_lib.sh, or stop shipping it"
# The same report with the stray named by a list of its own: the file is no
# longer extra, and the report says nothing.
t_eq "$(install_list_report "$(sorted "$files" ./tehas/libexec/stray-probe)" "$(core_files)")" ""
# The other half: a name the list promises and the home does not hold.
rm -f "$th/libexec/stray-probe" "$th/libexec/work"
t_eq "$(install_list_report "$(sorted "$files")" "$(core_files)")" \
  "missing installed file ./tehas/libexec/work: install.sh does not copy it into the home"

# The page the host gets, not a copy of the checkout's: a page planted beside
# the installed ones, or one that has gone stale, is red here. Each sentence is
# read with the line breaks turned into spaces, so reflowing the paragraph is no
# reason for this case to fail. The suite's list file is what the sentence exists
# to keep, so it is the pin; the count guards it against matching nothing on a
# page that came out empty or truncated, and the sentence beside it against
# keeping the rule it stands in.
pin=$(tr '\n' ' ' <"$th/docs/operating.md")
t_eq "$(printf '%s' "$pin" | grep -c 'test/install_lib\.sh')" "1"
t_contains "$pin" "must name \`install.sh\` and \`test/install_lib.sh\`"
t_contains "$pin" "a cycle that changes a file outside the fence parks"
# The fence rule of step 1 and the refine brief's "Out of scope" line must say
# the same thing: the section is a fence, not a note.
t_contains "$pin" "is part of that fence"
t_contains "$(cat "$T_ROOT/lib/refine-brief.md")" "Out of scope"
# The page is one of the installed manual's own: nothing planted beside it.
t_eq "$(cd "$th/docs" && find . -type f ! -name version.txt | LC_ALL=C sort)" \
  "$(cd "$T_ROOT/host/docs" && find . -type f | LC_ALL=C sort)"

t_done
