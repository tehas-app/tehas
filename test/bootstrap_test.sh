#!/usr/bin/env bash
# Tests for bootstrap.sh, against stub tools that print a version: what
# `check` reports, that `user` skips what is present, and the platform guard.
# No download and no install happens here.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

bootstrap="$T_ROOT/bootstrap.sh"
# shellcheck source=host/versions.env
. "$T_ROOT/host/versions.env"

printf 'ID=ubuntu\nVERSION_ID="26.04"\n' >"$T/os-ubuntu"
printf 'ID=debian\nVERSION_ID="13"\n' >"$T/os-debian"
# Only the stubs and the base system: tools installed for the person running
# the tests must not be found.
T_PATH=$T/bin:/usr/bin:/bin
T_ENV=(BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")

# Every tool at its expected version.
all_present() {
  t_stub node "echo v$NODE_VERSION"
  t_stub gh "echo 'gh version $GH_VERSION (2026-07-31)'"
  t_stub bun "echo $BUN_VERSION"
  t_stub pi "echo $PI_VERSION"
  t_stub ctx7 "echo $CTX7_VERSION"
  t_stub parallel-cli "echo 'parallel-cli, version $PARALLEL_CLI_VERSION'"
  t_stub rustc "echo 'rustc $RUST_VERSION (abc 2026-09-28)'"
  t_stub vipune "echo 'vipune $VIPUNE_VERSION'"
  t_stub oo "echo 'oo $OO_VERSION'"
  t_stub codebase-memory-mcp "echo 'codebase-memory-mcp $CBM_VERSION'"
  t_stub git "case \"\$*\" in *rev-parse*) echo $PI_RUKAS_COMMIT ;; *) echo 'git version 2.53.0' ;; esac"
  t_stub docker 'echo "Docker version 29.1.3"'
  t_stub loginctl 'echo yes'
  t_stub curl 'echo "curl 8.0.0"'
  t_stub uname 'echo x86_64'
  for tool in jq openssl flock shellcheck unzip; do t_stub "$tool" 'echo "1.2.3"'; done
  mkdir -p "$T/home/.pi/agent/extensions"
  ln -sfn "$T" "$T/home/.pi/agent/extensions/pi-rukas"
}

t_case "check passes when every tool is at its version"
all_present
t_run "$bootstrap check"
t_eq "$t_rc" 0
t_lacks "$t_out" "MISSING"
t_lacks "$t_out" "DIFFERS"
t_contains "$t_out" "node"
t_contains "$t_out" "$PI_RUKAS_COMMIT"

t_case "check changes nothing"
before=$(find "$T/home" | sort)
t_run "$bootstrap check"
t_eq "$(find "$T/home" | sort)" "$before"

t_case "check reports a tool at another version and exits 1"
t_stub gh 'echo "gh version 2.0.0 (2022-01-01)"'
t_run "$bootstrap check"
t_eq "$t_rc" 1
t_contains "$(grep '^gh ' <<<"$t_out")" "2.0.0"
t_contains "$(grep '^gh ' <<<"$t_out")" "$GH_VERSION"
t_contains "$(grep '^gh ' <<<"$t_out")" "DIFFERS"

t_case "check reports a missing tool and exits 1"
all_present
rm "$T/bin/vipune"
t_run "$bootstrap check"
t_eq "$t_rc" 1
t_contains "$(grep '^vipune ' <<<"$t_out")" "MISSING"

t_case "pi-rukas at another commit is reported"
all_present
t_stub git 'case "$*" in *rev-parse*) echo 0000000000000000000000000000000000000000 ;; *) echo "git version 2.53.0" ;; esac'
t_run "$bootstrap check"
t_eq "$t_rc" 1
t_contains "$(grep '^pi-rukas ' <<<"$t_out")" "DIFFERS"

t_case "pi-rukas cloned but never installed counts as missing"
all_present
rm "$T/home/.pi/agent/extensions/pi-rukas"
t_run "$bootstrap check"
t_contains "$(grep '^pi-rukas ' <<<"$t_out")" "MISSING"

t_case "docker that needs sudo is reported"
all_present
t_stub docker 'case "$1" in info) exit 1 ;; *) echo "Docker version 29.1.3" ;; esac'
t_run "$bootstrap check"
t_eq "$t_rc" 1
t_contains "$(grep '^docker without sudo' <<<"$t_out")" "MISSING"

t_case "lingering switched off is reported"
all_present
t_stub loginctl 'echo no'
t_run "$bootstrap check"
t_eq "$t_rc" 1
t_contains "$(grep '^lingering' <<<"$t_out")" "MISSING"

t_case "user with everything present downloads and installs nothing"
all_present
t_stub npm
t_stub cargo
t_stub tar
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_eq "$t_rc" 0
t_contains "$t_out" "bootstrap: user part done"
t_lacks "$t_out" "installing"
t_eq "$(t_calls curl)" ""
t_eq "$(t_calls npm)" ""
t_eq "$(t_calls cargo)" ""

t_case "user stops when a just-installed tool is still at the wrong version"
t_stub ctx7 'echo 0.0.1'
t_run "$bootstrap user"
t_eq "$t_rc" 1
t_contains "$t_out" "installing ctx7 $CTX7_VERSION (replacing 0.0.1)"
t_contains "$(t_calls npm)" "ctx7@$CTX7_VERSION"
t_contains "$t_out" "ctx7 is 0.0.1 after installing, expected $CTX7_VERSION"

t_case "an existing pi-rukas clone is fetched from the repository in versions.env"
all_present
t_stub git 'case "$*" in *rev-parse*) echo 0000000000000000000000000000000000000000 ;; *) echo "git version 2.53.0" ;; esac'
mkdir -p "$T/home/tehas/repos/pi-rukas/.git"
printf '#!/bin/sh\n' >"$T/home/tehas/repos/pi-rukas/install.sh"
chmod +x "$T/home/tehas/repos/pi-rukas/install.sh"
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_contains "$(t_calls git)" "remote set-url origin $PI_RUKAS_REPO"

t_case "pi-rukas's installer is not given the sandbox image's name"
# It would pull that name, fail, and build upstream's image over the tag.
printf '#!/bin/sh\necho "image=${PI_ENSEMBLE_IMAGE-unset}" >"$T/installer-env"\n' >"$T/home/tehas/repos/pi-rukas/install.sh"
T_ENV+=(PI_ENSEMBLE_IMAGE=tehas/pi-rukas:uid1000)
t_run "$bootstrap user"
T_ENV=(BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")
t_eq "$(cat "$T/installer-env")" "image=unset"
rm -rf "$T/home/tehas/repos/pi-rukas"

t_case "a download that does not match its checksum is refused"
all_present
t_stub gh 'echo "gh version 1.0.0"'
t_stub curl 'while [ $# -gt 0 ]; do [ "$1" = -o ] && { echo tampered >"$2"; exit 0; }; shift; done'
t_run "$bootstrap user"
t_eq "$t_rc" 1
t_contains "$t_out" "checksum mismatch for gh.tar.gz"
t_eq "$([ -e "$T/home/tehas/bin/gh" ] && echo installed)" ""
t_stub curl 'echo "curl 8.0.0"'

t_case "pool writes one limit around all cycles"
t_stub nproc 'echo 8'
t_stub id 'case "$*" in -u) echo 0 ;; *) exit 1 ;; esac'
t_stub systemctl
t_stub docker 'case "$1" in info) echo systemd ;; esac'
T_ENV+=(BOOTSTRAP_ETC="$T/etc")
t_run "$bootstrap pool 512m 2.75"
t_eq "$t_rc" 0
t_eq "$(grep -v '^#' "$T/etc/systemd/system/tehas-cycles.slice")" "[Unit]
Description=The containers of tehas cycles, together

[Slice]
MemoryMax=512M
CPUQuota=275%"
t_eq "$(t_calls systemctl)" "daemon-reload
start tehas-cycles.slice"

t_case "pool without values takes half the host's memory and its CPUs minus one"
half=$(($(awk '/^MemTotal:/ { print int($2 / 1024) }' /proc/meminfo) / 2))
t_run "$bootstrap pool"
t_eq "$t_rc" 0
t_contains "$(cat "$T/etc/systemd/system/tehas-cycles.slice")" "MemoryMax=${half}M
CPUQuota=700%"

t_case "pool refuses a value that is no amount of memory, and more than the host has"
rm -rf "${T:?}/etc" "$T/calls/systemctl"
for bad in lots 0 4m 100000g "512m many" "512m 0.5" "512m 9"; do
  t_run "$bootstrap pool $bad"
  t_eq "$t_rc" 1
done
t_eq "$([ -e "$T/etc" ] && echo written)" ""
t_eq "$(t_calls systemctl)" ""

t_case "pool refuses a host where Docker does not place containers by systemd"
t_stub docker 'case "$1" in info) echo cgroupfs ;; esac'
t_run "$bootstrap pool 512m"
t_eq "$t_rc" 1
t_contains "$t_out" "systemd cgroup driver"
t_eq "$([ -e "$T/etc" ] && echo written)" ""

t_case "pool must run as root"
t_stub id 'case "$*" in -u) echo 1000 ;; *) exit 1 ;; esac'
t_run "$bootstrap pool 512m"
t_eq "$t_rc" 1
t_contains "$t_out" "sudo ./bootstrap.sh pool"
t_eq "$([ -e "$T/etc" ] && echo written)" ""
rm -f "$T/bin/id" "$T/bin/systemctl" "$T/bin/nproc"
t_stub docker 'echo "Docker version 29.1.3"'
T_ENV=(BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")

t_case "user refuses an unsupported system before touching anything"
all_present
rm -rf "$T/home/tehas"
T_ENV=(BOOTSTRAP_OS_RELEASE="$T/os-debian" PATH="$T_PATH")
t_run "$bootstrap user"
t_eq "$t_rc" 1
t_contains "$t_out" "this host is debian 13 on x86_64; bootstrap.sh supports Ubuntu 26.04 on x86_64 only"
t_eq "$([ -e "$T/home/tehas" ] && echo created)" ""

t_case "another architecture is refused"
T_ENV=(BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")
t_stub uname 'echo aarch64'
t_run "$bootstrap user"
t_eq "$t_rc" 1
t_contains "$t_out" "on aarch64"
t_stub uname 'echo x86_64'

t_case "root refuses to run without root"
t_run "$bootstrap root someone"
t_eq "$t_rc" 1
t_contains "$t_out" "must run as root"

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
slice=$etc/systemd/system/tehas.slice
slice_stubs() { # <uid the caller has> [<what docker info answers>] [<systemd-oomd's state>]
  t_stub id "case \"\$*\" in -u) echo $1 ;; boss) ;; *) exit 1 ;; esac"
  t_stub docker "case \"\$1\" in info) echo '${2-systemd 2}' ;; *) echo 'Docker version 29.1.3' ;; esac"
  t_stub systemctl 'case "$*" in
  "show tehas.slice -p FragmentPath -p MemoryMax")
    if [ -e "$T/slice-loaded" ]; then printf "MemoryMax=21990232555\nFragmentPath=/etc/systemd/system/tehas.slice\n"
    else printf "MemoryMax=infinity\nFragmentPath=\n"; fi ;;
  "start tehas.slice") : >"$T/slice-loaded" ;;
  "is-active systemd-oomd") echo '"${3-inactive}"'; [ '"${3-inactive}"' = active ] ;;
esac'
}
fresh_slice() { rm -rf "$etc" "$T"/calls/* "$T/slice-loaded"; mkdir -p "$etc/systemd/system"; }
T_ENV=(BOOTSTRAP_ETC="$etc" BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")

t_case "slice installs the shipped file, reloads and starts"
fresh_slice
slice_stubs 0
t_run "$bootstrap slice"
t_eq "$t_rc" 0
t_eq "$(cmp -s "$slice" "$shipped" && echo same)" same
t_eq "$(stat -c %a "$slice")" 644
t_eq "$(t_calls systemctl | grep -v -e '^show ' -e '^is-active ')" "daemon-reload
start tehas.slice"
t_contains "$t_out" "bootstrap: slice part done"
t_contains "$t_out" "21990232555"

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
t_eq "$(t_calls systemctl)" "show tehas.slice -p FragmentPath -p MemoryMax"
t_eq "$(stat -c %Y "$slice")" "$before"

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

t_case "root installs the slice too"
fresh_slice
slice_stubs 0
t_stub uname 'echo x86_64'
for tool in apt-get usermod loginctl; do t_stub "$tool"; done
t_run "$bootstrap root boss"
t_eq "$t_rc" 0
t_eq "$(cmp -s "$slice" "$shipped" && echo same)" same
t_contains "$(t_calls systemctl)" "start tehas.slice"
t_contains "$t_out" "bootstrap: slice part done"
t_contains "$t_out" "bootstrap: root part done"

t_case "root with a cgroupfs driver finishes, writes no slice and says what to run"
fresh_slice
slice_stubs 0 "cgroupfs 2"
t_run "$bootstrap root boss"
t_eq "$t_rc" 0
t_eq "$([ -e "$slice" ] && echo there)" ""
t_contains "$t_out" "run sudo ./bootstrap.sh slice once Docker uses the systemd driver"
t_contains "$t_out" "bootstrap: root part done"
rm -f "$T/bin/id" "$T/bin/apt-get" "$T/bin/usermod" "$T/bin/systemctl"

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
t_eq "$(find "$etc" -type f | wc -l)" 1
T_ENV=(BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")

t_case "no arguments prints the usage"
t_run "$bootstrap"
t_eq "$t_rc" 64
t_contains "$t_out" "bootstrap.sh check"
t_contains "$t_out" "bootstrap.sh slice"

t_done
