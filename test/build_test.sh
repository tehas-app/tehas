#!/usr/bin/env bash
# Tests for host/sandbox/build.sh: the old date tags it removes after a build,
# and that what the Dockerfile reads comes from versions.env. A stub docker
# keeps the image's tags in $T/images, "<tag> <id>" per line, as
# `docker image ls` lists them, and the containers in $T/containers,
# "<container id> <image id>" per line.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

build="$T_ROOT/host/sandbox/build.sh"
uid=$(id -u)
# The head of pi-rukas's branch, so the build asks no network.
t_stub git 'printf "%s\trefs/heads/main\n" aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
t_stub docker 'case "$1" in
  build)
    while [ "$#" -gt 0 ]; do [ "$1" = -t ] && echo "${2#*:} sha256:new" >>"$T/images"; shift; done ;;
  tag) echo "${3#*:} sha256:new" >>"$T/images" ;;
  ps) [ -z "${PS_RC:-}" ] || exit "$PS_RC"; cut -d" " -f1 "$T/containers" ;;
  inspect) shift 3; for c in "$@"; do awk -v c="$c" "\$1 == c { print \$2 }" "$T/containers"; done ;;
  image) awk "!seen[\$1]++" "$T/images" ;;
  rmi) [ "$2" != "${RMI_FAILS:-}" ] || exit 1 ;;
esac'
# Six builds before this one, the oldest first, each its own image. The last
# also carried the uid tag, which the build moves to the new image.
setup() {
  : >"$T/containers"
  printf '%s\n' "uid$uid sha256:new" "uid$uid sha256:6" "261009-1000 sha256:1" "261010-1000 sha256:2" \
    "261011-1000 sha256:3" "261012-1000 sha256:4" "261013-1000 sha256:5" "261014-1000 sha256:6" >"$T/images"
  rm -f "$T"/calls/*
}
removed() { t_calls docker | awk '$1 == "rmi" { print $2 }' | LC_ALL=C sort | tr '\n' ' '; }

t_case "after a build the newest three date tags stay and older ones are removed"
setup
t_run "$build"
t_eq "$t_rc" 0
t_eq "$(removed)" "tehas/pi-rukas:261009-1000 tehas/pi-rukas:261010-1000 tehas/pi-rukas:261011-1000 tehas/pi-rukas:261012-1000 "
t_contains "$t_out" "removed tehas/pi-rukas:261009-1000"
t_lacks "$(t_calls docker)" "rmi tehas/pi-rukas:uid"

t_case "a tag whose image a container was made from is kept"
setup
printf '%s\n' "c1 sha256:2" "c2 sha256:new" >"$T/containers"
t_run "$build"
t_eq "$t_rc" 0
t_eq "$(removed)" "tehas/pi-rukas:261009-1000 tehas/pi-rukas:261011-1000 tehas/pi-rukas:261012-1000 "
t_contains "$t_out" "kept tehas/pi-rukas:261010-1000: a container or a uid tag uses it"

t_case "a tag whose image another uid tag names is kept"
setup
echo "uid4242 sha256:1" >>"$T/images"
t_run "$build"
t_eq "$t_rc" 0
t_eq "$(removed)" "tehas/pi-rukas:261010-1000 tehas/pi-rukas:261011-1000 tehas/pi-rukas:261012-1000 "

t_case "a tag that cannot be removed is said, and the build still counts"
setup
T_ENV=(RMI_FAILS=tehas/pi-rukas:261010-1000)
t_run "$build"
t_eq "$t_rc" 0
t_contains "$t_out" "warning: could not remove tehas/pi-rukas:261010-1000"
t_contains "$t_out" "removed tehas/pi-rukas:261009-1000"
T_ENV=()

t_case "a docker that does not list its containers removes nothing"
setup
T_ENV=(PS_RC=1)
t_run "$build"
t_eq "$t_rc" 0
t_eq "$(removed)" ""
t_contains "$t_out" "no old tehas/pi-rukas tag was removed"
T_ENV=()

t_case "three builds or fewer remove nothing"
setup
printf '%s\n' "uid$uid sha256:2" "261010-1000 sha256:1" "261011-1000 sha256:2" >"$T/images"
t_run "$build"
t_eq "$t_rc" 0
t_eq "$(removed)" ""

t_case "every argument the Dockerfile reads is passed from versions.env"
while read -r arg; do
  case $arg in PI_RUKAS_IMAGE | PI_RUKAS_COMMIT | TEHAS_UID | TEHAS_RUKAS_IMAGES) continue ;; esac
  grep -q "^$arg=" "$T_ROOT/host/versions.env" || t_bad "$arg is not in versions.env"
  grep -qF -- "--build-arg $arg=\"\$$arg\"" "$build" || t_bad "build.sh does not pass $arg"
done < <(sed -n 's/^ARG \([A-Z0-9_]*\).*/\1/p' "$T_ROOT/host/sandbox/Dockerfile")
t_eq "$(grep -c '^ARG ' "$T_ROOT/host/sandbox/Dockerfile")" "$(grep -c -- '--build-arg ' "$build")"

t_done
