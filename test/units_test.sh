#!/usr/bin/env bash
# Tests for the systemd units in host/systemd: what each one's ExecStart is,
# and which of them adds a sink of its own to a log its command already writes.
# Behaviour of the commands themselves belongs to the suite of that command.
# shellcheck disable=SC2016  # the unit's ExecStart is asserted as a literal
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

units=$T_ROOT/host/systemd

# A unit whose command is the writer of its own log adds no redirect: the
# redirect would write each line of a run into that file a second time. The
# journal holds what the run prints that is not in the log, and a refusal that
# happens before the log can be opened - malformed settings, another run
# holding the lock, a state directory that could not be made - reaches only
# `journalctl --user -u <unit>`.
self_writing=(clean refresh)

t_case "the cleaner's unit is the run with no sink of its own"
# The member the cleaner's own suite cannot see. Pinned as the whole line: an
# empty grep, a spaced-out `ExecStart =`, or a commented directive cannot leave
# the case to read as a pass.
# The first of its two steps; the - lets the disk reading after it run when the
# clean failed.
t_eq "$(sed -n 's/^ExecStart=//p' "$units/tehas-clean.service" | head -n 1)" \
  "-/bin/bash -c \". %h/tehas/env.sh && exec %h/tehas/bin/tehasbot clean\""

t_case "the cleaner's unit opens no file and sets no sink"
# Not just the ExecStart: StandardOutput=append:, StandardError=append: or a
# redirect in either step would give the run a second destination and write
# each line of it twice. Comments are left out, so the prose beside the
# directive can name the log without tripping the case, and no directive can
# hide in it.
no_comments=$(grep -v '^#' "$units/tehas-clean.service")
t_lacks "$no_comments" "logs/"
t_lacks "$no_comments" ">>"
t_lacks "$no_comments" "StandardOutput="
t_lacks "$no_comments" "StandardError="

t_case "the cleaner's unit publishes the disk reading after the clean, failed or not"
# The reading factory_disk serves, from the console directly. A second ExecStart
# and no post step: systemd runs neither after a step that failed, so the clean
# carries the - and the reading does not. Pinned as the whole line, and as the
# last of exactly two steps; the two cases above still read true of it.
t_eq "$(sed -n 's/^ExecStart=//p' "$units/tehas-clean.service" | sed -n 2p)" \
  "/bin/bash -c \". %h/tehas/env.sh && exec %h/tehas/libexec/console disk %h/tehas --publish\""
t_eq "$(grep -c '^ExecStart=' "$units/tehas-clean.service")" 2
t_eq "$(grep -c '^ExecStartPost=' "$units/tehas-clean.service")" 0

t_case "the units with no redirect are the commands that write their own log"
# The class read from the source as well as from the units, so a third
# self-writing command in console/src/run.rs has to be said here before the units
# are believed. Today: tehasbot clean opens logs/clean.log and tehasbot refresh
# opens logs/refresh.log. A command of this class prints each line it writes,
# so the unit for it adds no redirect: the redirect would write each line of a
# run into that file a second time.
t_eq "$(grep -o 'logs/[a-z]*\.log' "$T_ROOT/console/src/run.rs" | sed 's#logs/##; s#\.log##' | LC_ALL=C sort -u | tr '\n' ' ')" \
  "$(printf '%s\n' "${self_writing[@]}" | LC_ALL=C sort | tr '\n' ' ')"
for name in "${self_writing[@]}"; do
  unit=$units/tehas-$name.service
  [ -f "$unit" ] || { t_bad "$unit is not there"; continue; }
  # The cleaner's own step is its first, and carries the - of the case above.
  want=""; [ "$name" = clean ] && want=-
  t_eq "$(sed -n 's/^ExecStart=//p' "$unit" | head -n 1)" \
    "$want/bin/bash -c \". %h/tehas/env.sh && exec %h/tehas/bin/tehasbot $name\""
done

t_case "a cycle's unit starts the cycle from its own copy of the code"
# tehas-cycle run copies bin/, libexec/ and lib/ and execs tehasbot cycle from
# the copy, so an update while the cycle runs does not change its code (#989).
t_eq "$(sed -n 's/^ExecStart=//p' "$units/tehas-cycle@.service")" \
  "/bin/bash -c \". %h/tehas/env.sh && exec %h/tehas/libexec/tehas-cycle run %i >> %h/tehas/logs/dispatch.log 2>&1\""

t_case "the poll's unit removes the file that starts it before the poll runs"
# A poll that stops on its settings writes nothing, so the unit removes the
# file: left in place, tehas-dispatch.path would start poll after poll until
# systemd failed it at the service's start limit.
t_eq "$(sed -n 's/^ExecStartPre=//p' "$units/tehas-dispatch.service" | head -n 1)" \
  "-/bin/rm -f %h/tehas/state/dispatch.again"

t_case "the poll's unit appends the poll to its log, with no step before or after it but the file's"
t_eq "$(sed -n 's/^ExecStart=//p' "$units/tehas-dispatch.service")" \
  "/bin/bash -c \". %h/tehas/env.sh && %h/tehas/bin/tehasbot dispatch >> %h/tehas/logs/dispatch.log 2>&1\""
t_eq "$(grep -c '^ExecStartPre=' "$units/tehas-dispatch.service")" 1
t_eq "$(grep -c '^ExecStopPost=' "$units/tehas-dispatch.service")" 0
t_eq "$(sed -n 's/^PathExists=//p' "$units/tehas-dispatch.path")" "%h/tehas/state/dispatch.again"

t_case "every unit that redirects writes a log its command does not"
# The same property as the two cases above, for the units that do keep a
# redirect: each keeps one only while nothing it runs opens that file.
# Comments, prose and a line the shell itself does not write are not a writer,
# so the search takes the assignments where a command names its own log. A unit
# with no redirect is not a violation, so the sweep needs no list to consult.
missing=""
for unit in "$units"/*.service; do
  target=$(sed -n 's/^ExecStart=.* >> %h\/tehas\/logs\/\([a-z0-9.-]*\.log\) 2>&1"$/\1/p' "$unit")
  [ -n "$target" ] || continue
  if grep -En "logs/$target\"?\$|logfile=.*$target|logs/$target\"" "$T_ROOT"/libexec/* "$T_ROOT/console/src/run.rs" >/dev/null 2>&1; then
    missing="$missing ${unit##*/}:$target"
  fi
done
t_eq "$missing" ""

t_done
