#!/usr/bin/env bash
# Tests for bootstrap.sh's parts run as root: dev, slice, clock and root, against
# stubs and a scratch /etc.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/bootstrap_lib.sh
. "$(dirname "$0")/bootstrap_lib.sh" || exit 1

# --- dev: let other accounts start a session ---------------------------------
# Stubs for everything that would change the system. The files go to scratch
# directories through the same two hooks the real paths have.
etc=$T/etc
ubin=$T/usr-local-bin
mkdir -p "$etc/sudoers.d" "$ubin" "$T/boss"
dev_stubs() { # <uid the caller has>
  t_stub id "case \"\$*\" in -u) echo $1 ;; boss | alice | bob) ;; *) exit 1 ;; esac"
  t_stub getent 'case "$*" in "passwd boss") echo "boss:x:1001:1001::$T/boss:/bin/bash" ;; "group tehas") [ -e "$T/group-made" ] ;; *) exit 2 ;; esac'
  t_stub groupadd ': >"$T/group-made"'
  t_stub usermod
  t_stub visudo 'case "$1" in -cf) grep -q "^%tehas " "$2" ;; *) exit 1 ;; esac'
  t_stub sudo
}
dev_stubs 0
T_ENV=(BOOTSTRAP_ETC="$etc" BOOTSTRAP_BIN="$ubin" PATH="$T_PATH")
rule=$etc/sudoers.d/tehas-dev
written() { find "$etc" "$ubin" -type f | wc -l; }

t_case "dev lets the group run one script as the tehas user, and nothing else"
rm -f "$T"/calls/*
t_run "$bootstrap dev boss alice bob"
t_eq "$t_rc" 0
t_eq "$(grep -v -e '^#' -e '^Defaults' "$rule")" "%tehas ALL=(boss) NOPASSWD: $T/boss/tehas/libexec/dev-login"
t_eq "$(grep -c '' "$rule")" "$(grep -c -e '^#' -e '^Defaults!' -e '^%tehas ' "$rule")"
t_eq "$(grep '^Defaults' "$rule")" "Defaults!$T/boss/tehas/libexec/dev-login env_keep += \"COLORTERM\""
t_eq "$(stat -c %a "$rule")" 440
t_lacks "$(cat "$rule")" "ALL=(ALL"
t_lacks "$(cat "$rule")" "(root)"

t_case "dev names the tehas user for the command, and installs the command for everyone"
t_eq "$(cat "$etc/tehas/manager")" boss
t_eq "$(cmp -s "$ubin/tehas" "$T_ROOT/host/login/tehas" && echo same)" same
t_eq "$(stat -c %a "$ubin/tehas")" 755

t_case "dev makes the group once and adds the accounts named"
t_eq "$(t_calls groupadd)" "tehas"
t_eq "$(t_calls usermod)" "-aG tehas alice
-aG tehas bob"
rm -f "$T"/calls/*
t_run "$bootstrap dev boss"
t_eq "$t_rc" 0
t_eq "$(t_calls groupadd)" ""
t_eq "$(t_calls usermod)" ""

t_case "dev never puts an account in the docker group"
t_lacks "$(t_calls usermod)" "docker"

fresh_dev() { rm -rf "$etc" "$ubin" "$T"/calls/* "$T/group-made"; mkdir -p "$etc/sudoers.d" "$ubin"; }

t_case "dev changes nothing when an account does not exist"
fresh_dev
t_run "$bootstrap dev boss alice mallory"
t_eq "$t_rc" 1
t_contains "$t_out" "no such user: mallory"
t_eq "$(written)" 0
t_eq "$(t_calls usermod)$(t_calls groupadd)" ""

t_case "dev refuses the tehas user as one of the accounts, and names that are not plain"
for args in "boss boss" "boss 'a b'" "'bo ss'" "boss ALL" ""; do
  t_run "$bootstrap dev $args"
  [ "$t_rc" != 0 ] || t_bad "accepted: $args"
done
t_eq "$(written)" 0
t_eq "$(t_calls usermod)$(t_calls groupadd)" ""

t_case "dev installs no rule that visudo does not accept"
t_stub visudo 'exit 1'
t_run "$bootstrap dev boss alice"
t_eq "$t_rc" 1
t_contains "$t_out" "visudo"
t_eq "$([ -e "$rule" ] && echo there)" ""
t_eq "$([ -e "$ubin/tehas" ] && echo there)" ""
t_eq "$(t_calls usermod)" ""

t_case "dev refuses to run without root"
dev_stubs 1000
t_run "$bootstrap dev boss alice"
t_eq "$t_rc" 1
t_contains "$t_out" "must run as root"
t_eq "$(written)" 0
rm -f "$T/bin/id" "$T/bin/getent" "$T/bin/sudo" "$T/bin/visudo" "$T/bin/usermod" "$T/bin/groupadd"
T_ENV=(BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")

# --- slice: the one system unit every sandbox container runs under -----------
# systemctl answers as a host does: an unloaded slice has no file and no
# limit, and it is loaded from the file once it has been started.
shipped=$T_ROOT/host/slice/tehas.slice
rule_shipped=$T_ROOT/host/slice/96-tehas-memory-online.rules
reload_shipped=$T_ROOT/host/slice/tehas-slice-reload.service
slice=$etc/systemd/system/tehas.slice
rule=$etc/udev/rules.d/96-tehas-memory-online.rules
reload=$etc/systemd/system/tehas-slice-reload.service
slice_stubs() { # <uid the caller has> [<what docker info answers>] [<systemd-oomd's state>]
  # Names the stubs of this section make, so the clock section can take them
  # away again: a systemctl that only knows about chrony cannot answer the root
  # part's questions about the slice, and the root cases come after both.
  slice_tools="id docker systemctl "
  t_stub id "case \"\$*\" in -u) echo $1 ;; boss) ;; *) exit 1 ;; esac"
  t_stub docker "case \"\$1\" in info) echo '${2-systemd 2}' ;; *) echo 'Docker version 29.1.3' ;; esac"
  # The step asks udevd to re-read its rules after writing the new one; the
  # stub records the call and answers like any other command here.
  t_stub udevadm
  # slice-stale: the file on disk is newer than what systemd read, as systemd
  # reports it, until a daemon-reload.
  t_stub systemctl 'case "$*" in
  "show tehas.slice -p FragmentPath -p MemoryMax -p NeedDaemonReload")
    if [ -e "$T/slice-loaded" ]; then printf "MemoryMax=21990232555\nFragmentPath=/etc/systemd/system/tehas.slice\n"
    else printf "MemoryMax=infinity\nFragmentPath=\n"; fi
    if [ -e "$T/slice-stale" ]; then echo NeedDaemonReload=yes; else echo NeedDaemonReload=no; fi ;;
  "daemon-reload") rm -f "$T/slice-stale" ;;
  "start tehas.slice") : >"$T/slice-loaded" ;;
  "is-active systemd-oomd") echo '"${3-inactive}"'; [ '"${3-inactive}"' = active ] ;;
esac'
}
fresh_slice() { rm -rf "$etc" "$T"/calls/* "$T/slice-loaded"; mkdir -p "$etc/systemd/system"; }
T_ENV=(BOOTSTRAP_ETC="$etc" BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")
slice_tools=

t_case "slice installs the shipped file, reloads and starts"
fresh_slice
slice_stubs 0
t_run "$bootstrap slice"
t_eq "$t_rc" 0
t_eq "$(cmp -s "$slice" "$shipped" && echo same)" same
t_eq "$(stat -c %a "$slice")" 644
t_eq "$(t_calls systemctl | grep -v -e '^show ' -e '^is-active ')" "daemon-reload
start tehas.slice"
# daemon-reload does not read /etc/udev/rules.d, so the step has to ask udevd
# for its own reload or the rule sits on disk until udevd next restarts.
t_eq "$(t_calls udevadm)" "control --reload-rules"
t_contains "$t_out" "bootstrap: slice part done"
t_contains "$t_out" "21990232555"

# systemd reads a percentage MemoryMax when it loads the unit, so memory added
# to a running host reaches the slice only through a reload: the rule asks
# systemd-udevd for one, and the oneshot does it.
t_case "slice installs the memory-online rule and the reload unit beside it"
fresh_slice
slice_stubs 0
t_run "$bootstrap slice"
t_eq "$t_rc" 0
t_eq "$(cmp -s "$rule" "$rule_shipped" && echo same)" same
t_eq "$(cmp -s "$reload" "$reload_shipped" && echo same)" same
t_eq "$(stat -c %a "$rule")$(stat -c %a "$reload")" "644644"
# The rule names the unit and nothing else does the reload: RUN+= is killed when
# it runs too long, and many blocks come online at once. One read of the file,
# comments away, for every assertion on it: a rule may name RUN+= in its prose
# and still hold no RUN+=, and this case claims the second thing.
rule_code=$(grep -v -e '^#' -e '^$' "$rule")
t_eq "$rule_code" 'SUBSYSTEM=="memory", ACTION=="online", TAG+="systemd", ENV{SYSTEMD_WANTS}+="tehas-slice-reload.service"'
t_lacks "$rule_code" 'ACTION=="offline"'
t_lacks "$rule_code" "RUN+="
# The unit udevd is told to start is the one that ships beside the rule: both
# names are read from the files, so a rename of one side alone fails here.
t_eq "$(sed -n 's/.*SYSTEMD_WANTS}+="\([^"]*\)".*/\1/p' <<<"$rule_code")" "$(basename "$reload")"
t_contains "$(cat "$reload")" "Type=oneshot"
t_contains "$(cat "$reload")" "ExecStart=/usr/bin/systemctl daemon-reload"
# The reload is bounded, as every unit of host/systemd bounds its run: a hung
# daemon-reload would swallow every memory-online event that came after it.
t_contains "$(cat "$reload")" "TimeoutStartSec="

t_case "the shipped slice has one memory limit, a CPU weight and nothing else"
t_eq "$(grep -v -e '^#' -e '^$' "$shipped")" "[Unit]
Description=The containers tehas starts, together
[Slice]
MemoryMax=75%
CPUWeight=100"

t_case "slice run twice with the unit loaded from the file changes nothing"
rm -f "$T"/calls/*
before=$(stat -c %Y "$slice")
t_run "$bootstrap slice"
t_eq "$t_rc" 0
t_contains "$t_out" "bootstrap: slice part already in place"
t_eq "$(t_calls systemctl)" "show tehas.slice -p FragmentPath -p MemoryMax -p NeedDaemonReload"
t_eq "$(stat -c %Y "$slice")" "$before"

t_case "slice run twice with a file systemd has not re-read reloads and starts"
# An update changed the file and the reload after it failed: the unit is still
# the old file's, with a limit, so the file alone says nothing.
rm -f "$T"/calls/*
: >"$T/slice-stale"
t_run "$bootstrap slice"
t_eq "$t_rc" 0
t_lacks "$t_out" "already in place"
t_eq "$(t_calls systemctl | grep -v -e '^show ' -e '^is-active ')" "daemon-reload
start tehas.slice"
t_contains "$t_out" "bootstrap: slice part done"
t_eq "$([ -e "$T/slice-stale" ] && echo stale)" ""

t_case "slice run twice with a file but a limitless unit reloads and starts"
# The file is there but systemd never read it, or holds a slice it made up
# before the file existed.
rm -f "$T"/calls/* "$T/slice-loaded"
t_run "$bootstrap slice"
t_eq "$t_rc" 0
t_lacks "$t_out" "already in place"
t_eq "$(t_calls systemctl | grep -v -e '^show ' -e '^is-active ')" "daemon-reload
start tehas.slice"
t_contains "$t_out" "bootstrap: slice part done"

t_case "slice stops when the unit has no limit after it was started"
fresh_slice
slice_stubs 0
t_stub systemctl 'case "$1" in show) printf "MemoryMax=infinity\nFragmentPath=\n" ;; esac'
slice_tools="systemctl "
t_run "$bootstrap slice"
t_eq "$t_rc" 1
t_contains "$t_out" "no memory limit"
t_lacks "$t_out" "slice part done"

t_case "slice with arguments is refused (64)"
fresh_slice
slice_stubs 0
t_run "$bootstrap slice boss"
t_eq "$t_rc" 64
t_contains "$t_out" "usage: sudo ./bootstrap.sh slice"
t_eq "$([ -e "$slice" ] && echo there)" ""
t_eq "$(t_calls systemctl)" ""

t_case "slice refuses a cgroupfs driver and writes nothing"
slice_stubs 0 "cgroupfs 2"
t_run "$bootstrap slice"
t_eq "$t_rc" 1
t_contains "$t_out" "cgroupfs cgroup driver on cgroup v2"
t_contains "$t_out" "/etc/docker/daemon.json"
t_contains "$t_out" '"exec-opts": ["native.cgroupdriver=systemd"]'
t_eq "$([ -e "$slice" ] && echo there)" ""
t_eq "$(t_calls systemctl)" ""

t_case "slice refuses cgroup v1 and writes nothing"
slice_stubs 0 "systemd 1"
t_run "$bootstrap slice"
t_eq "$t_rc" 1
t_contains "$t_out" "systemd cgroup driver on cgroup v1"
t_eq "$([ -e "$slice" ] && echo there)" ""
t_eq "$(t_calls systemctl)" ""

t_case "slice refuses a Docker that does not answer and writes nothing"
slice_stubs 0
t_stub docker 'exit 1'
t_run "$bootstrap slice"
t_eq "$t_rc" 1
t_contains "$t_out" "Docker gave no answer"
t_eq "$([ -e "$slice" ] && echo there)" ""
t_eq "$(t_calls systemctl)" ""

t_case "slice must run as root"
slice_stubs 1000
rm -f "$T"/calls/*
t_run "$bootstrap slice"
t_eq "$t_rc" 64
t_contains "$t_out" "sudo ./bootstrap.sh slice"
t_eq "$([ -e "$slice" ] && echo there)" ""
t_eq "$(t_calls systemctl)$(t_calls docker)" ""

t_case "slice says whether systemd-oomd is active and installs either way"
for oomd in active inactive; do
  fresh_slice
  slice_stubs 0 "systemd 2" "$oomd"
  t_run "$bootstrap slice"
  t_eq "$t_rc" 0
  t_contains "$t_out" "systemd-oomd: $oomd"
  t_eq "$(cmp -s "$slice" "$shipped" && echo same)" same
done
t_lacks "$t_out" "systemd-oomd: active"

# --- clock --------------------------------------------------------------------
# The clock part: qemu-guest-agent on a VM, the chrony drop-in under
# BOOTSTRAP_ETC. The stubs name what the host is (systemd-detect-virt), which
# time service systemd has active (systemctl is-active) and what the distribution
# offers (apt-get).
conf=$etc/chrony/conf.d/tehas.conf
# Where the part writes, and a decoy at a path the script cannot guess: a run
# that wrote outside BOOTSTRAP_ETC would leave its file there.
conf_other=$T/not-etc/chrony/conf.d/tehas.conf
mkdir -p "${conf_other%/*}"
# The three lines the part writes, and the one line a case that does not care
# about the comments asks for.
conf_body='# Written by bootstrap.sh clock. A restored VM starts with the clock
# of the moment it was saved; stepping is the only way back within seconds.
makestep 1 -1'
# Every stub this section makes is named in clock_tools and removed at its end:
# the root part's cases after it ask systemctl about the slice.
clock_tools=
clock_stubs() { # <uid> <what systemd-detect-virt --vm answers> [<the units that are active>...]
  # Who runs it, what the host is, and which chrony units systemd has active:
  # the probe ends 0 for a unit the case named and 3 for any other.
  local clock_tool clock_unit units
  # The third argument onwards as one string, safe under set -u.
  #   ''            the step cases: chrony active, Ubuntu's name for its unit
  #   none          no unit active: the part is asked twice and told no twice
  #   <one or more> the units of the host, by their own names
  case "${3-}" in
    '') units=chrony ;;
    none) units='' ;;
    *) units=${*:3} ;;
  esac
  for clock_tool in id systemd-detect-virt apt-get systemctl; do
    clock_tools+="$clock_tool "
  done
  t_stub id "case \"\$*\" in -u) echo $1 ;; boss) ;; *) exit 1 ;; esac"
  # "$2" quoted, and not $2: clock_stubs 0 '' means a host that is no VM, and the
  # words of an unquoted empty argument would leave $2 unbound.
  case "$2" in
    '') t_stub systemd-detect-virt 'exit 1' ;;
    *) t_stub systemd-detect-virt "echo '$2'" ;;
  esac
  # The probe is the case's, named by the words of $units.
  # shellcheck disable=SC2086 # the words of $units are the units of the case
  t_stub systemctl "case \"\$*\" in$(for clock_unit in $units; do printf '\n  "is-active --quiet %s") : ;;' "$clock_unit"; done)
  # The part's two other calls succeed; a case that wants one to fail stubs
  # systemctl again.
  'enable --now qemu-guest-agent' | 'restart chrony') : ;;
  *) exit 3 ;;
esac"
  t_stub apt-get
}
fresh_clock() { rm -rf "$etc/chrony" "$T"/calls/*; }
T_ENV=(BOOTSTRAP_ETC="$etc" BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")

t_case "clock on a VM installs and enables the guest agent and writes the drop-in"
fresh_clock
clock_stubs 0 kvm
rm -f "$T"/calls/*
t_run "$bootstrap clock"
t_eq "$t_rc" 0
t_eq "$(t_calls systemd-detect-virt)" "--vm"
t_eq "$(t_calls apt-get | tr '\n' '|')" "update -qq|install -y -qq qemu-guest-agent|"
t_eq "$(t_calls systemctl)" "is-active --quiet chrony
enable --now qemu-guest-agent
restart chrony"
t_eq "$(cat "$conf")" "$conf_body"
t_eq "$(stat -c %a "$conf")" 644
t_eq "$(stat -c %a "${conf%/*}")" 755
t_contains "$t_out" "qemu-guest-agent: enabled on kvm"
t_contains "$t_out" "makestep 1 -1, chrony restarted"

t_case "clock run again keeps the file it wrote as .prev and writes the same bytes"
rm -f "$T"/calls/*
t_run "$bootstrap clock"
t_eq "$t_rc" 0
t_eq "$(cmp -s "$conf" "$conf.prev" && echo same)" same
t_eq "$(cat "$conf")" "$conf_body"

# The modes of the file and its directory: what a daemon must be able to read.
t_case "clock keeps the drop-in of an earlier run as .prev, and its mode"
fresh_clock
clock_stubs 0 kvm
rm -f "$T"/calls/*
# The file of the earlier run is put in place by the case itself: two copies of
# the script in one line of t_run would not stand for two runs, because the part
# names its file from BOOTSTRAP_ETC when the script is loaded, so both would
# write one path. What is kept is a file of another host's making, in the shape
# chrony itself ships, and the part must not write over it.
install -d -m 755 "${conf%/*}"
printf 'makestep 1 3\n' >"$conf"
chmod 600 "$conf"
t_run "$bootstrap clock"
t_eq "$t_rc" 0
t_eq "$(cat "$conf")" "$conf_body"
t_eq "$(cat "$conf.prev")" "makestep 1 3"
# The mode of the file the part wrote, and of the one it only moved aside.
t_eq "$(stat -c %a "$conf")" 644
t_eq "$(stat -c %a "$conf.prev")" 600
t_eq "$(stat -c %a "${conf%/*}")" 755
t_eq "$(find "$etc/chrony" -type f | wc -l)" 2

t_case "clock on bare metal installs no agent and still steps the clock"
fresh_clock
clock_stubs 0 '' chrony
t_run "$bootstrap clock"
t_eq "$t_rc" 0
t_eq "$(t_calls systemd-detect-virt)" "--vm"
t_eq "$(t_calls apt-get)$(t_calls systemctl | grep -v -e '^restart chrony$' -e '^is-active --quiet chrony$')" ""
t_contains "$t_out" "qemu-guest-agent: none on bare metal"
t_eq "$(cat "$conf")" "$conf_body"
t_eq "$(find "$etc/chrony" -type f | wc -l)" 1

# Neither of the two units active is a host whose time service is not chrony:
# systemd answered both questions, so the part knows what it found, writes
# nothing of its own, and still ends 0.
t_case "clock writes no drop-in when no chrony unit is active, and says what it found"
for units in systemd-timesyncd ntpd; do
  fresh_clock
  # The name of the loop holds the units of one case, which are the words the
  # helper joins into the answer of the stub.
  # shellcheck disable=SC2086
  clock_stubs 0 kvm $units
  t_run "$bootstrap clock"
  t_eq "$t_rc" 0
  t_eq "$([ -e "$conf" ] && echo there)" ""
  # Nothing of the part's own left under /etc; tehas.slice stands there from the
  # slice cases above, as it does on a host that ran them.
  t_eq "$(find "$etc/chrony" 2>/dev/null | wc -l)" 0
  t_contains "$t_out" "neither chrony nor chronyd is active"
  t_contains "$t_out" "No drop-in was written"
  # Both units were asked, in this order, and neither answered active.
  t_eq "$(t_calls systemctl | grep -c '^is-active --quiet ')" 2
  # The agent is the hypervisor's way to set the clock and does not depend on
  # which time service the host runs.
  t_contains "$(t_calls apt-get)" "install -y -qq qemu-guest-agent"
  t_eq "$(t_calls systemctl | grep -v '^is-active --quiet ')" "enable --now qemu-guest-agent"
done

# The two files the part can own, and the modes of both: the earlier file is
# kept as .prev rather than written over, and a person who wants the host's own
# makestep back takes the drop-in away and reads the file that was kept.
t_case "clock keeps the file of an earlier run as .prev, in the same run of the script"
stat -c "%n %a" "${conf}.prev" "$conf" "${conf%/*}" | LC_ALL=C sort

# A host with no systemd-detect-virt to ask: the part takes that as bare metal
# and goes on, rather than stopping without a word about the clock. The probe is
# stubbed by every case above, because the host running the tests may well have
# the real program and would answer for the case otherwise; here it is taken
# away again.
t_case "clock on a host without the probe goes on as on bare metal"
for answer in none none; do
  fresh_clock
  # The two words of the loop are what systemd-detect-virt answers, read unquoted
  # so that the word 'none' reaches the helper as one argument of its own.
  # shellcheck disable=SC2086
  clock_stubs 0 $answer
  # t_unstub, and not rm: no systemd-detect-virt on the PATH at all is the host
  # this case is about, and the host running the tests may well have one. The run
  # is given the stubs and the planted base system only, so the host's own probe
  # cannot answer for a host that has none.
  t_unstub systemd-detect-virt
  t_run "$bootstrap clock"
  t_eq "$t_rc" 0
  t_contains "$t_out" "qemu-guest-agent: none on bare metal"
  t_eq "$(t_calls apt-get)" ""
  t_eq "$(tail -n 1 "$conf")" "makestep 1 -1"
done

# The other name of the unit, on the distributions whose package calls it
# chronyd: the part asks chrony first and, told no, asks chronyd. The helper's
# word 'none' is not meant here, since the host of this case does have one unit
# active; its name is given instead, and chrony is answered by neither.
t_case "clock accepts the chronyd name of the unit"
fresh_clock
clock_stubs 0 kvm chronyd
rm -f "$T"/calls/*
t_run "$bootstrap clock"
t_eq "$t_rc" 0
t_eq "$(t_calls systemctl | grep '^is-active --quiet ')" "is-active --quiet chrony
is-active --quiet chronyd"
t_eq "$(tail -n 1 "$conf")" "makestep 1 -1"

# systemctl that cannot be asked at all: no systemd, or no systemctl on the
# PATH. That is not a host without chrony, so the part does not say "done": it
# says what it could not do and ends 1, and writes nothing.
t_case "clock that cannot ask systemd fails instead of saying done"
fresh_clock
clock_stubs 0 kvm
t_stub systemctl 'exit 127'
t_run "$bootstrap clock"
t_eq "$t_rc" 1
t_contains "$t_out" "could not ask systemd whether chrony or chronyd is active"
t_lacks "$t_out" "clock part done"
t_eq "$([ -e "$conf" ] && echo there)" ""
t_eq "$(t_calls apt-get)" ""

t_case "clock must run as root, and changes nothing when it is not"
fresh_clock
clock_stubs 1000 kvm systemd-timesyncd
t_run "$bootstrap clock"
t_eq "$t_rc" 1
t_contains "$t_out" "sudo ./bootstrap.sh clock"
t_eq "$([ -e "$conf" ] && echo there)" ""
t_eq "$(t_calls apt-get)$(t_calls systemctl)" ""

t_case "clock with an argument is refused (64)"
fresh_clock
clock_stubs 0 kvm
t_run "$bootstrap clock boss"
t_eq "$t_rc" 64
t_contains "$t_out" "usage: sudo ./bootstrap.sh clock"
t_eq "$([ -e "$conf" ] && echo there)" ""
t_eq "$(t_calls apt-get)$(t_calls systemctl)" ""

t_case "clock on an unsupported host stops before it installs anything"
fresh_clock
clock_stubs 0 kvm
T_ENV=(BOOTSTRAP_ETC="$etc" BOOTSTRAP_OS_RELEASE="$T/os-debian" PATH="$T_PATH")
t_run "$bootstrap clock"
t_eq "$t_rc" 1
t_contains "$t_out" "supports Ubuntu 26.04 on x86_64 only"
t_eq "$([ -e "$conf" ] && echo there)" ""
t_eq "$(t_calls apt-get)$(t_calls systemd-detect-virt)" ""
T_ENV=(BOOTSTRAP_ETC="$etc" BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")

t_case "clock that cannot install the package stops before it enables anything"
fresh_clock
clock_stubs 0 kvm
t_stub apt-get 'case "$1" in install) exit 100 ;; esac'
clock_tools+="apt-get "
t_run "$bootstrap clock"
t_eq "$t_rc" 1
t_contains "$t_out" "could not install qemu-guest-agent"
t_eq "$([ -e "$conf" ] && echo there)" ""
t_eq "$(t_calls systemctl | grep -v '^is-active --quiet ')" ""

t_case "clock whose agent will not start still writes the drop-in and exits 1"
fresh_clock
clock_stubs 0 kvm
# The probe answered first by the helper's stub, then again by this one: a body
# that only names the agent leaves the probe and the restart to the default of
# the case, which is the one the case means for them.
t_stub systemctl 'case "$*" in
  "is-active --quiet chrony") : ;;
  "enable --now qemu-guest-agent") exit 1 ;;
  "restart chrony") : ;;
esac'
clock_tools+="systemctl "
t_run "$bootstrap clock"
t_eq "$t_rc" 1
t_contains "$t_out" "qemu-guest-agent: not enabled"
t_eq "$(grep -c makestep "$conf")" 1
t_contains "$(t_calls systemctl)" "restart chrony"

t_case "clock that wrote the drop-in but cannot restart chrony says so and exits 1"
fresh_clock
clock_stubs 0 kvm
t_stub systemctl 'case "$*" in
  "is-active --quiet chrony") : ;;
  "enable --now qemu-guest-agent") : ;;
  "restart chrony") exit 1 ;;
esac'
clock_tools+="systemctl "
t_run "$bootstrap clock"
t_eq "$t_rc" 1
t_contains "$t_out" "chrony did not restart"
t_eq "$(tail -n 1 "$conf")" "makestep 1 -1"

# The copy that keeps the old drop-in is a step of its own, and its failure
# stops the part: the manual says the file before is kept, so writing over it
# after a failed copy would leave the host's own makestep nowhere.
t_case "clock whose copy to .prev fails leaves the old drop-in and ends 1"
fresh_clock
clock_stubs 0 kvm
clock_tools+="cp "
# The old drop-in is put where the part will find it, and a copy of it kept out of
# the way, so the case can ask what the file holds afterwards. The copy is under a
# name of its own, because $T is the one name the body of a stub may read when the
# stub runs (test/lib.sh gives it to the run), and no other case of this file
# writes that path.
install -d -m 755 "${conf%/*}"
printf 'makestep 1 3\n' >"$T/cp.host"
chmod 600 "$T/cp.host"
cp -p "$T/cp.host" "$conf"
t_stub cp 'case "$1" in
  -p) echo "cp: cannot copy" >&2; exit 1 ;;
esac'
t_run "$bootstrap clock"
t_eq "$t_rc" 1
t_contains "$t_out" "could not keep $conf as $conf.prev"
t_contains "$t_out" "$conf was left as it is"
t_lacks "$t_out" "clock part done"
# The host's file, its bytes and its mode, unchanged; nothing kept, nothing new,
# and the part stopped before it asked chrony to restart.
t_eq "$(cmp -s "$conf" "$T/cp.host" && echo same)" same
t_eq "$(cat "$conf")" "makestep 1 3"
t_eq "$(stat -c %a "$conf")" 600
t_eq "$([ -e "$conf.prev" ] && echo there)" ""
t_eq "$(t_calls systemctl | grep -c 'restart chrony')" 0

t_case "clock with no drop-in yet does not ask for a copy that cannot fail"
fresh_clock
clock_stubs 0 kvm
# cp is taken away rather than stubbed to fail: with no file to keep, the part
# has no reason to call it, and a run that did would say so.
clock_tools+="cp "
rm -f "$T/bin/cp"
t_run "$bootstrap clock"
t_eq "$t_rc" 0
t_eq "$(tail -n 1 "$conf")" "makestep 1 -1"
t_eq "$([ -e "$conf.prev" ] && echo there)" ""

# BOOTSTRAP_ETC is read when the script is loaded, so a run of the whole script
# that does not name it would write under the real /etc. The clock part owns one
# file there and its directory; this case asks what it says when it cannot make
# them. t_run builds the run's environment with env -i, so the name has to be
# taken away inside the line the run reads, and not only from T_ENV.
t_case "clock on a host that gives no BOOTSTRAP_ETC says it cannot make the file"
fresh_clock
clock_stubs 0 '' chrony
# The run holds no command of the host but the planted ones: the part must reach
# /etc by its own reading of BOOTSTRAP_ETC, and not by whether the person running
# the tests happens to be able to write there, which a tool of the host outside
# the case would decide.
t_unstub
# The case asks a host with no /etc/chrony to say it cannot make the file. Where
# the directory is already there - a host that ran the clock part once, which this
# repository's own hosts do - install -d finds the file system, the part goes on
# to write, and the message asked for here is never said: the whole ask belongs to
# a host with no such directory. The jail above (that no host command answers for
# the file) is the part that does not depend on the host and runs everywhere; the
# rest belongs to a host that can be asked, so it is skipped, not failed, here.
if [ -d /etc/chrony ]; then
  t_skip "this host has /etc/chrony, so the part cannot be asked to say it cannot make the file"
else
  t_run "unset BOOTSTRAP_ETC; $bootstrap clock"
  t_eq "$t_rc" 1
  t_contains "$t_out" "cannot create directory '/etc/chrony"
  t_eq "$([ -e "$conf" ] && echo there)" ""
  t_eq "$([ -e "${conf}.prev" ] && echo there)" ""
  t_eq "$([ -e /etc/chrony/conf.d/tehas.conf ] && echo there)" ""
  t_eq "$([ -e /etc/chrony/conf.d/tehas.conf.prev ] && echo there)" ""
fi

t_case "clock writes under BOOTSTRAP_ETC and nowhere else"
fresh_clock
rm -f "$conf_other"
clock_stubs 0 '' chrony
t_run "$bootstrap clock"
t_eq "$t_rc" 0
t_eq "$(tail -n 1 "$conf")" "makestep 1 -1"
t_eq "$([ -e "$conf_other" ] && echo there)" ""

for clock_tool in $clock_tools $slice_tools; do rm -f "$T/bin/$clock_tool"; done
unset clock_tool clock_tools slice_tools
# BOOTSTRAP_ETC stays set for every later run of this file, as it was before
# this section: the slice file that the root and check cases write goes under
# the scratch /etc, and not the host's.
T_ENV=(BOOTSTRAP_ETC="$etc" BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")

# The root part's own stubs: apt-get, usermod and loginctl record, useradd
# makes the account, runuser runs nothing and records how it was asked.
root_stubs() {
  for tool in apt-get usermod loginctl; do t_stub "$tool"; done
  t_stub useradd ': >"$T/user-made"'
  t_stub id "case \"\$*\" in -u) echo 0 ;; boss) ;; carol) [ -e \"\$T/user-made\" ] ;; *) exit 1 ;; esac"
  t_stub runuser
  t_stub uname 'echo x86_64'
}

t_case "root installs the slice, then runs the user part as the user in the same run"
fresh_slice
slice_stubs 0
root_stubs
t_run "$bootstrap root boss"
t_eq "$t_rc" 0
t_eq "$(cmp -s "$slice" "$shipped" && echo same)" same
t_contains "$(t_calls systemctl)" "start tehas.slice"
t_contains "$t_out" "bootstrap: slice part done"
t_contains "$t_out" "bootstrap: root part done"
t_eq "$(t_calls runuser)" "-l boss -c $T_ROOT/bootstrap.sh user"
t_contains "$(t_calls apt-get)" "minisign"
t_eq "$(t_calls useradd)" ""
t_lacks "$t_out" "must log in again"

t_case "root with --root-only stops before the user part"
fresh_slice
slice_stubs 0
root_stubs
t_run "$bootstrap root boss --root-only"
t_eq "$t_rc" 0
t_contains "$t_out" "bootstrap: root part done"
t_eq "$(t_calls runuser)" ""
t_contains "$t_out" "./bootstrap.sh user"

t_case "root fails when the user part fails"
fresh_slice
slice_stubs 0
root_stubs
t_stub runuser 'exit 1'
t_run "$bootstrap root boss"
t_eq "$t_rc" 1
t_contains "$t_out" "the user part failed"

t_case "root makes a user that is missing"
fresh_slice
slice_stubs 0
root_stubs
t_run "$bootstrap root carol"
t_eq "$t_rc" 0
t_eq "$(t_calls useradd)" "-m -s /bin/bash carol"
t_contains "$t_out" "made the user carol"
t_contains "$(t_calls usermod)" "-aG docker carol"

t_case "root refuses a name that is not a plain account name and changes nothing"
fresh_slice
slice_stubs 0
root_stubs
t_run "$bootstrap root 'bo ss'"
t_eq "$t_rc" 1
t_contains "$t_out" "not a plain account name"
t_eq "$(t_calls apt-get)$(t_calls useradd)" ""

# A Docker on cgroupfs answers systemd once daemon.json names the systemd
# driver and it was restarted.
docker_driver_stub() {
  t_stub docker "case \"\$1\" in
info) if grep -q native.cgroupdriver=systemd \"$etc/docker/daemon.json\" 2>/dev/null && [ -e \"\$T/docker-restarted\" ]; then echo 'systemd 2'; else echo 'cgroupfs 2'; fi ;;
*) echo 'Docker version 29.1.3' ;;
esac"
  t_stub systemctl "case \"\$*\" in
  'restart docker') : >\"\$T/docker-restarted\" ;;
  'show tehas.slice -p FragmentPath -p MemoryMax -p NeedDaemonReload')
    if [ -e \"\$T/slice-loaded\" ]; then printf 'MemoryMax=21990232555\nFragmentPath=/etc/systemd/system/tehas.slice\nNeedDaemonReload=no\n'
    else printf 'MemoryMax=infinity\nFragmentPath=\nNeedDaemonReload=no\n'; fi ;;
  'start tehas.slice') : >\"\$T/slice-loaded\" ;;
esac"
}

t_case "root sets the systemd cgroup driver in a missing daemon.json, restarts Docker and installs the slice"
fresh_slice
rm -f "$T/docker-restarted"
slice_stubs 0
root_stubs
docker_driver_stub
t_run "$bootstrap root boss"
t_eq "$t_rc" 0
t_eq "$(jq -c . "$etc/docker/daemon.json")" '{"exec-opts":["native.cgroupdriver=systemd"]}'
t_contains "$(t_calls systemctl)" "restart docker"
t_eq "$(cmp -s "$slice" "$shipped" && echo same)" same
t_contains "$t_out" "bootstrap: root part done"

t_case "root merges the driver into a daemon.json that has other settings and keeps them"
fresh_slice
rm -f "$T/docker-restarted"
mkdir -p "$etc/docker"
printf '%s\n' '{"log-driver":"local","exec-opts":["native.cgroupdriver=cgroupfs","other=1"]}' >"$etc/docker/daemon.json"
slice_stubs 0
root_stubs
docker_driver_stub
t_run "$bootstrap root boss"
t_eq "$t_rc" 0
t_eq "$(jq -c . "$etc/docker/daemon.json")" '{"log-driver":"local","exec-opts":["other=1","native.cgroupdriver=systemd"]}'
t_eq "$(jq -c . "$etc/docker/daemon.json.prev")" '{"log-driver":"local","exec-opts":["native.cgroupdriver=cgroupfs","other=1"]}'

t_case "root leaves a daemon.json it cannot read as JSON, writes no slice and says so"
fresh_slice
rm -f "$T/docker-restarted"
mkdir -p "$etc/docker"
printf '{ not json\n' >"$etc/docker/daemon.json"
slice_stubs 0
root_stubs
docker_driver_stub
t_run "$bootstrap root boss"
t_eq "$t_rc" 1
t_eq "$(cat "$etc/docker/daemon.json")" "{ not json"
t_contains "$t_out" "is not JSON"
t_eq "$([ -e "$slice" ] && echo there)" ""
t_eq "$(t_calls runuser)" ""

t_case "root on cgroup v1 finishes, writes no slice and no daemon.json, and says what to run"
fresh_slice
slice_stubs 0 "systemd 1"
root_stubs
t_run "$bootstrap root boss"
t_eq "$t_rc" 0
t_eq "$([ -e "$slice" ] && echo there)" ""
t_eq "$([ -e "$etc/docker/daemon.json" ] && echo there)" ""
t_contains "$t_out" "run sudo ./bootstrap.sh slice once Docker uses the systemd driver"
t_contains "$t_out" "bootstrap: root part done"
rm -f "$T/bin/id" "$T/bin/apt-get" "$T/bin/usermod" "$T/bin/systemctl" "$T/bin/runuser" "$T/bin/useradd"

t_case "check reports the slice and does not fail on it"
all_present
fresh_slice
t_run "$bootstrap check"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas.slice: missing (sudo ./bootstrap.sh slice)"
install -m 644 "$shipped" "$slice"
t_run "$bootstrap check"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas.slice: in place"
echo "MemoryMax=10%" >>"$slice"
t_run "$bootstrap check"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas.slice: differs (sudo ./bootstrap.sh slice)"
t_lacks "$t_out" "tehas.slice: in place"
t_contains "$t_out" "96-tehas-memory-online.rules: missing (sudo ./bootstrap.sh slice)"
t_contains "$t_out" "tehas-slice-reload.service: missing (sudo ./bootstrap.sh slice)"
install -D -m 644 "$rule_shipped" "$rule"
install -D -m 644 "$reload_shipped" "$reload"
t_run "$bootstrap check"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas.slice: differs"
t_contains "$t_out" "96-tehas-memory-online.rules: in place"
t_contains "$t_out" "tehas-slice-reload.service: in place"
echo "SUBSYSTEM==\"memory\", ACTION==\"offline\"" >>"$rule"
t_run "$bootstrap check"
t_eq "$t_rc" 0
t_contains "$t_out" "96-tehas-memory-online.rules: differs (sudo ./bootstrap.sh slice)"
t_eq "$(find "$etc" -type f | wc -l)" 3
T_ENV=(BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")

# The write path has three installs and each can fail on its own (an unreadable
# source, a full or unwritable /etc). Two of the three on disk is not in place,
# and the next run has to repair it: a rule beside no unit is the failure this
# step exists to avoid.
t_case "slice says so when a shipped file cannot be read, and writes nothing"
fresh_slice
slice_stubs 0
# A shipped file the step cannot read is reported as the write that failed, and
# nothing is left on disk: the pre-flight test of -f cannot tell that file from
# one that is there and readable. On a copy of the tree: other files of the suite
# read the shipped one while this runs.
mkdir -p "$T/unreadable"
cp "$bootstrap" "$T/unreadable/"
cp -R "$T_ROOT/lib" "$T_ROOT/host" "$T/unreadable/"
chmod 000 "$T/unreadable/host/slice/tehas-slice-reload.service"
t_run "$T/unreadable/bootstrap.sh slice"
t_eq "$t_rc" 1
t_contains "$t_out" "could not write"
t_eq "$([ -e "$slice" ] && echo there)" ""
t_eq "$([ -e "$rule" ] && echo there)" ""
t_eq "$([ -e "$reload" ] && echo there)" ""
t_eq "$(t_calls systemctl | grep -v -e '^show ' -e '^is-active ')" ""

T_ENV=(BOOTSTRAP_ETC="$etc" BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")

# Each of the three installs can fail on its own (a full or unwritable target
# directory, EACCES). Two of the three on disk is not in place, and the next run
# has to repair it: a rule beside no unit is the failure this step exists to
# avoid. The case writes the unit by hand and stubs install to fail at the rule.
t_case "slice reports the file it could not write and the retry writes what is missing"
fresh_slice
slice_stubs 0
# The unit is written first, so a run that fails at the rule leaves the shipped
# unit beside no rule and no slice: what a guard on the slice alone would report
# as "already in place". The directory of the rule is made unwritable to bring
# that about, as EACCES or a full disk would.
mkdir -p "$etc/udev"
chmod 555 "$etc/udev"
t_run "$bootstrap slice"
t_eq "$t_rc" 1
t_contains "$t_out" "could not write $rule"
t_eq "$([ -e "$slice" ] && echo there)" ""
t_eq "$([ -e "$rule" ] && echo there)" ""
t_eq "$(t_calls systemctl | grep -v -e '^show ' -e '^is-active ')" ""
t_eq "$(t_calls udevadm)" ""
# The retry writes the three and reloads udevd once.
chmod 755 "$etc/udev"
t_run "$bootstrap slice"
t_eq "$t_rc" 0
t_eq "$(cmp -s "$rule" "$rule_shipped" && echo same)" same
t_eq "$(cmp -s "$slice" "$shipped" && echo same)" same
t_eq "$(t_calls udevadm)" "control --reload-rules"
t_contains "$t_out" "bootstrap: slice part done"

t_case "slice says so when udevd's rules could not be reloaded, and still installs"
T_ENV=(BOOTSTRAP_ETC="$etc" BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")
fresh_slice
slice_stubs 0
t_stub udevadm 'exit 1'
t_run "$bootstrap slice"
t_eq "$t_rc" 0
t_contains "$t_out" "udevadm control --reload-rules failed"
t_contains "$t_out" "bootstrap: slice part done"
t_eq "$(cmp -s "$rule" "$rule_shipped" && echo same)" same

t_case "no arguments prints the usage"
t_run "$bootstrap"
t_eq "$t_rc" 64
t_contains "$t_out" "bootstrap.sh check"
t_contains "$t_out" "bootstrap.sh slice"

t_done
