# shellcheck shell=bash
# What the tests of host/sandbox/build.sh share: a scratch home with the build,
# the console and tehas-cycle, and stub git and docker that answer from files
# under $fx. Source this from a sandbox_build_*_test.sh file after test/lib.sh.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck disable=SC2034  # the variables here are read by the test files
home=$T/home/tehas
mkdir -p "$home/sandbox" "$home/lib"
cp "$T_ROOT/host/sandbox/build.sh" "$home/sandbox/"
# The build reads the recipes beside itself: without them it stops and asks
# docker nothing.
cp "$T_ROOT/host/sandbox/Dockerfile" "$T_ROOT/host/sandbox/base.Dockerfile" "$home/sandbox/"
cp "$T_ROOT/lib/config.sh" "$home/lib/"
cp "$T_ROOT/host/versions.env" "$home/"
# The console names the commit followed and its image, and tells the sweep of
# old dated tags what is held.
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_console || t_bad "no console: $t_console_why"
mkdir -p "$home/libexec"
cp "$T_CONSOLE" "$home/libexec/console"
# What the base image says is read by tehas-cycle.
# shellcheck disable=SC2154  # t_cycle_why is set by t_cycle (test/lib.sh)
t_cycle || t_bad "no tehas-cycle: $t_cycle_why"
cp "$T_CYCLE" "$home/libexec/tehas-cycle"
sed -i 's#^PI_RUKAS_REPO=.*#PI_RUKAS_REPO=https://github.com/Up-Owner/pi-rukas.git#' "$home/versions.env"
build=$home/sandbox/build.sh
fx=$T/fx
mkdir -p "$fx"

head=$(printf 'a%.0s' {1..40})
old=$(printf 'b%.0s' {1..40})
uid=$(id -u)
# The base this build makes for the commit in $fx/head.
base=tehas/pi-rukas-base:sha-aaaaaaaaaaaa-uid$uid

# Every case's smoke run starts a step, so the uid tag moves; smoke_crash and
# smoke_silence below write the two refusals over it.
cp "$T_ROOT/test/fixtures/smoke-started.json" "$fx/smoke"

# git lists the newest release tag at the commit in $fx/head, and an older one,
# and nothing when the file is missing; asked for a branch it answers the same
# commit. In the clone it holds the commit unless $fx/no-commit exists; a fetch
# of the tags brings it when $fx/fetch-brings exists, one by id when
# $fx/fetch-id-brings does, and archive prints a stand-in tree. docker answers
# the label in $fx/label, a pull fails when $fx/pull-fails exists, and the
# base's capabilities print $fx/caps.json and exit with $fx/ask-rc, 0 when it is
# missing. The smoke run before the uid tag moves prints the whole of $fx/smoke,
# nothing when it is missing, and $fx/smoke-product for the instance's layer. A
# build given the file named in $fx/build-fails fails. fx_ids answers the
# inspects of an instance layer's check.
t_stub git 'if [ "$1" = -C ]; then
  case "$3" in
    cat-file) [ ! -e "$T/fx/no-commit" ] ;;
    fetch) case "$*" in
        *--tags*) [ ! -e "$T/fx/fetch-brings" ] || rm -f "$T/fx/no-commit" ;;
        *) [ ! -e "$T/fx/fetch-id-brings" ] || rm -f "$T/fx/no-commit" ;;
      esac ;;
    archive) echo tree ;;
  esac
  exit
fi
[ -e "$T/fx/head" ] || exit 2
case "$*" in
  *--tags*) printf "%s\trefs/tags/v0.15.9\n%s\trefs/tags/v0.15.80\n" "$(printf "c%.0s" {1..40})" "$(cat "$T/fx/head")" ;;
  *) printf "%s\trefs/heads/main\n" "$(cat "$T/fx/head")" ;;
esac'
t_stub docker 'case "$1" in
  image)
    case "$2" in
      ls) [ ! -s "$T/fx/images" ] || cat "$T/fx/images"; exit 0 ;;
      inspect)
        # The label says whether an image is on the commit followed; the layers,
        # from the file named in fx/which-layers or fx/which-product for the
        # instance layer, check that layer against the image under it.
        # fx/ask-fails answers no inspect, as for an image that is not there.
        [ ! -e "$T/fx/ask-fails" ] || exit 1
        # The tehas.layer label, the sum of the instance layer the image was
        # built from, is fx/layer-label.
        for a in "$@"; do case "$a" in *tehas.layer*)
          [ ! -e "$T/fx/layer-label" ] || cat "$T/fx/layer-label"
          exit 0 ;; esac; done
        for a in "$@"; do case "$a" in *tehas.rukas*)
          [ -e "$T/fx/label" ] || exit 1
          cat "$T/fx/label"
          exit 0 ;; esac; done
        which=$(cat "$T/fx/which-layers" 2>/dev/null)
        for a in "$@"; do case "$a" in *-product) which=$(cat "$T/fx/which-product" 2>/dev/null) ;; esac; done
        [ -n "$which" ] && [ -s "$T/fx/$which" ] || exit 1
        # Answered as Docker does: {{.RootFS.Layers}} prints the ids on one
        # bracketed line, and a range that prints each id ends with an empty line.
        fmt=""; prev=""; for a in "$@"; do [ "$prev" != --format ] || fmt=$a; prev=$a; done
        case "$fmt" in
          "{{range .RootFS.Layers}}{{println .}}{{end}}") cat "$T/fx/$which"; echo ;;
          "{{.RootFS.Layers}}") echo "[$(paste -sd " " "$T/fx/$which")]" ;;
          *) exit 1 ;;
        esac
        exit 0 ;;
    esac
    # "docker image" on its own, which no command of build.sh asks for.
    [ -e "$T/fx/label" ] || exit 1; cat "$T/fx/label" ;;
  pull) [ ! -e "$T/fx/pull-fails" ] || exit 1 ;;
  run)
    # The vipune check of a smoke run is the run with a cpuset. It answers
    # $fx/vipune when there is one and the quiet line otherwise. $fx/vipune.first
    # answers the first such run alone, as dockerd refuses a CPU, and exits 125.
    if [[ $* == "run --pull=never --rm --init --network none "*" --cpuset-cpus "*" --entrypoint bash "* ]]; then
      if [ -e "$T/fx/vipune.first" ]; then
        cat "$T/fx/vipune.first"
        rm -f "$T/fx/vipune.first"
        exit 125
      fi
      if [ -e "$T/fx/vipune" ]; then cat "$T/fx/vipune"; else echo quiet; fi
      exit 0
    fi
    # A smoke run asked of bash: the layer above the image answers from a file of
    # its own, so one case can let the image start a step and refuse the layer.
    if [[ $* == "run --pull=never --rm --init --network none "*" --entrypoint bash "*"pi-rukas-headless work 1 --json"* ]]; then
      answer=$T/fx/smoke
      if [[ $* == *"-product "* && -e $T/fx/smoke-product ]]; then
        answer=$T/fx/smoke-product
      fi
      [ ! -s "$answer" ] || cat "$answer"
      exit "$(cat "$T/fx/smoke-rc" 2>/dev/null || echo 0)"
    fi
    cat "$T/fx/caps.json" 2>/dev/null
    exit "$(cat "$T/fx/ask-rc" 2>/dev/null || echo 0)" ;;
  build) found=0
    for a in "$@"; do [ "$a" = "$(cat "$T/fx/build-fails" 2>/dev/null)" ] && found=1; done
    # The base is built from the tree on standard input.
    if [ "$2" = -f ] && [ "$3" = base.Dockerfile ]; then
      cat >"$T/fx/base-context"
      [ ! -e "$T/fx/base-fails" ] || exit 1
    fi
    [ "$found" = 0 ] || exit 1 ;;
  ps) # "docker ps", which asks which containers exist: a case that wants none
    # names them by touching fx/no-containers, and this docker then answers no id.
    [ ! -e "$T/fx/no-containers" ] || exit 0 ;;
  inspect)
    # "docker inspect", which asks what those containers were made from.
    [ ! -e "$T/fx/no-containers" ] || exit 0 ;;
esac'
caps=$T_ROOT/test/fixtures/headless-c5ddc0a/capabilities.json
reset() {
  rm -f "$T"/calls/* "$fx"/*
  echo "$head" >"$fx/head"
  cp "$caps" "$fx/caps.json"
  # Each case starts with the checkout's versions.env and no instance files.
  cp "$T_ROOT/host/versions.env" "$home/versions.env"
  sed -i 's#^PI_RUKAS_REPO=.*#PI_RUKAS_REPO=https://github.com/Up-Owner/pi-rukas.git#' "$home/versions.env"
  rm -f "$T/instance/instance.env" "$T/instance/sandbox.Dockerfile"
  # The default smoke run again, which the rm above took.
  cp "$T_ROOT/test/fixtures/smoke-started.json" "$fx/smoke"
}

# The line a runner that died before its first step prints, and the same turn
# giving no answer at all: the two refusals of a smoke run.
smoke_crash() { printf '%s\n' '{"schemaVersion":5,"issues":[],"error":{"code":"crash","message":"the runner process exited 0 without a result"}}' >"$fx/smoke"; }
smoke_silence() { rm -f "$fx/smoke"; }
# The docker builds of the sandbox image and the instance's layer, not the base.
sandbox_builds() { t_calls docker | grep -e '^build ' | grep -v -e '^build -f base.Dockerfile '; }
t_env_instance() {
  export TEHAS_INSTANCE_DIR=$T/instance
  mkdir -p "$TEHAS_INSTANCE_DIR"
}
