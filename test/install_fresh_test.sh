#!/usr/bin/env bash
# Tests for install.sh: what a fresh install puts, the update lock, the units
# a first install enables and the install list. install_check_test.sh and
# install_first_test.sh hold the other cases of a fresh install.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1
real_console

t_case "--check on a home not made yet makes nothing, the marker neither"
fresh
instance
t_run "$install --check"
t_eq "$([ -e "$th" ] && echo made)" ""

t_case "a fresh install: exactly these files"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(install_list_report "$(sorted "$files")" "$(core_files)")" ""
# The console's loader is installed beside the other commands, ready to run, and
# the table of labels beside the libraries.
t_eq "$([ -x "$th/libexec/console" ] && cmp -s "$fixture/host/labels.tsv" "$th/lib/labels.tsv" && echo both)" "both"
# Every script in libexec/ is installed: a command bin/tehas knows has its script.
missing=""
for f in "$fixture"/libexec/*; do [ -x "$th/libexec/${f##*/}" ] || missing="$missing ${f##*/}"; done
for f in "$fixture"/lib/*.sh; do [ -f "$th/lib/${f##*/}" ] || missing="$missing ${f##*/}"; done
t_eq "$missing" ""
# The follow-ups these copies call for are asked for by id and recorded as owed.
t_eq "$(printf '%s\n' "$t_out" | sed -n 's/^next: //p')" "$(next_steps)"
t_eq "$(cat "$th/state/update-pending")" "$(next_steps)"
# Every file in lib/ is installed: the commands source them from there. The
# fixture has no git, so the names come from the checkout and the files from the
# fixture the install above read.
t_eq "$(git -C "$T_ROOT" ls-files lib | while read -r f; do [ -f "$fixture/$f" ] || echo "$f"; done)" ""
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
t_eq "$([ -d "$th/preview" ] && [ -d "$th/items" ] && echo there)" "there"
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
n=0; while [ ! -e '$T/held' ] && kill -0 \$! 2>/dev/null && [ \$((n += 1)) -le 300 ]; do sleep 0.1; done
exec 8>>'$lock'; TEHAS_UPDATE_LOCK_FD=8 '$install'; rc=\$?; kill \$!; wait; exit \$rc"
rm -f "$T/held"
t_eq "$t_rc" 1
t_fast 5000
t_contains "$t_out" "another update is running"
t_eq "$(core_files)" ""

# The other half of the wait: a run that finds the lock held for a moment gets
# in once it is let go. The wait is derived from the hold below, so the two
# cannot drift apart, and the output is read for a refusal. It runs on a home of
# its own, so the home below stays unfinished for the --check of the next case.
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
# and the install ran. The units go where $HOME puts them, outside a home; the
# state, the pages and the marker are each asked about by a case of their own.
t_eq "$(cd "$late" && find . -type f ! -path './state/*' ! -path './docs/*' ! -path './fun/*' ! -name '.tehas-home' | LC_ALL=C sort)" \
  "$(printf '%s\n' "${files[@]}" | grep -v -e '^\./\.bashrc$' -e '^\./\.config/' -e '^\./\.pi/' -e '^\./tehas/docs/' -e '^\./tehas/fun/' -e '^\./tehas/state/' -e "^\./tehas/$install_marker$" | sed 's|^\./tehas/|./|' | LC_ALL=C sort)"
t_eq "$([ -f "$late/.tehas-home" ] && echo there)" "there"

t_case "--check runs while an update holds the lock"
t_run "exec 8>>'$lock'; flock -n 8 || exit 9; '$install' --check"
t_lacks "$t_out" "another update is running"
t_contains "$t_out" "Installed:    DRIFT"

t_case "the optional units are enabled on a first install only when their setting is on"
fresh
instance TEHAS_DOMAIN=widget.test TEHAS_PREVIEWS=1
# Energy use left core in 2.0: a host file that still sets it enables nothing.
mkdir -p "$home/.config/tehas" && echo TEHAS_ENERGY=on >"$home/.config/tehas/env"
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(sed -n 2p "$T/calls/systemctl")" \
  "--user enable --now tehas-dispatch.timer tehas-dispatch.path tehas-preview.timer tehas-preview-sweep.timer"

t_case "a domain without previews enables no timer of its own"
fresh
instance TEHAS_DOMAIN=widget.test
t_run "$install"
t_eq "$(sed -n 2p "$T/calls/systemctl")" "--user enable --now tehas-dispatch.timer tehas-dispatch.path"

# The round trip of install_list_report (test/install_lib.sh): a home with one
# file too many, and a home that lacks a file the list promises.
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
rm -f "$th/libexec/stray-probe" "$th/libexec/setup"
t_eq "$(install_list_report "$(sorted "$files")" "$(core_files)")" \
  "missing installed file ./tehas/libexec/setup: install.sh does not copy it into the home"

# The page the host gets, not a copy of the checkout's, read with its line
# breaks turned into spaces so a reflowed paragraph still matches. The list file
# is the pin; the count guards against a page that came out empty.
pin=$(tr '\n' ' ' <"$th/docs/operating.md")
t_eq "$(printf '%s' "$pin" | grep -c 'test/install_lib\.sh')" "1"
t_contains "$pin" "must name \`install.sh\` and \`test/install_lib.sh\`"
t_contains "$pin" "a cycle that changes a file outside the fence parks"
# The rule of step 1 and the brief the planner is given must keep naming the
# same section and the same reason for it. Both are read from the home, as the
# page above is.
fence_section="Out of scope"
t_contains "$pin" "The \`$fence_section\` section of the issue's spec is part of that fence"
brief=$(tr '\n' ' ' <"$th/lib/issue-brief.md")
t_contains "$brief" 'the nearby files the cycle must not touch, by path'
# The page is one of the installed manual's own: nothing planted beside it.
t_eq "$(cd "$th/docs" && find . -type f ! -name version.txt | LC_ALL=C sort)" \
  "$(cd "$fixture/host/docs" && find . -type f | LC_ALL=C sort)"

t_case "a login that does not resolve stops the install with 78 before the first copy"
fresh
instance
t_gh_fail 1 1 "gh: Not Found (HTTP 404)"
t_run "$install"
t_eq "$t_rc" 78
t_contains "$t_out" "tehas: the login alice does not resolve to a GitHub account"
t_contains "$t_out" "tehas: nothing was installed"
t_eq "$([ -e "$th/bin/tehas" ] || [ -e "$th/state/account-ids" ] || echo none)" none

t_done
