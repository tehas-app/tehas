#!/usr/bin/env bash
# Tests for host/sandbox/build.sh, against stub git and docker: it builds on
# upstream's image for the commit versions.env follows, only when
# that image's --capabilities answer has the contract tehas work needs, labels
# the image with that commit and with whether its pi-rukas reads issue images,
# and with --if-moved or --check looks at the label of the current image first.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

home=$T/home/tehas
mkdir -p "$home/sandbox" "$home/lib"
cp "$T_ROOT/host/sandbox/build.sh" "$home/sandbox/"
cp "$T_ROOT/lib/headless.sh" "$T_ROOT/lib/rukas.sh" "$home/lib/"
cp "$T_ROOT/host/versions.env" "$home/"
sed -i 's#^PI_RUKAS_REPO=.*#PI_RUKAS_REPO=https://github.com/Up-Owner/pi-rukas.git#' "$home/versions.env"
build=$home/sandbox/build.sh
fx=$T/fx
mkdir -p "$fx"

head=$(printf 'a%.0s' {1..40})
old=$(printf 'b%.0s' {1..40})
uid=$(id -u)

# git lists the newest release tag at the commit in $fx/head, and an older
# one, and lists nothing when the file is missing; asked for a branch it
# answers the same commit as its head. docker
# answers the label in $fx/label, a pull fails when $fx/pull-fails exists, and
# asking the base for its capabilities prints $fx/caps.json and exits with
# $fx/ask-rc, 0 when it is missing. A build given the file named in
# $fx/build-fails - the instance's own, by its path - fails.
t_stub git '[ -e "$T/fx/head" ] || exit 2
case "$*" in
  *--tags*) printf "%s\trefs/tags/v0.12.9\n%s\trefs/tags/v0.12.80\n" "$(printf "c%.0s" {1..40})" "$(cat "$T/fx/head")" ;;
  *) printf "%s\trefs/heads/main\n" "$(cat "$T/fx/head")" ;;
esac'
t_stub docker 'case "$1" in
  image) [ -e "$T/fx/label" ] || exit 1; cat "$T/fx/label" ;;
  pull) [ ! -e "$T/fx/pull-fails" ] || exit 1 ;;
  run) cat "$T/fx/caps.json" 2>/dev/null; exit "$(cat "$T/fx/ask-rc" 2>/dev/null || echo 0)" ;;
  build) found=0
    for a in "$@"; do [ "$a" = "$(cat "$T/fx/build-fails" 2>/dev/null)" ] && found=1; done
    [ "$found" = 0 ] || exit 1 ;;
esac'
caps=$T_ROOT/test/fixtures/headless-c5ddc0a/capabilities.json
reset() {
  rm -f "$T"/calls/* "$fx"/*
  echo "$head" >"$fx/head"
  cp "$caps" "$fx/caps.json"
}
# The instance directory build.sh looks in for its own sandbox.Dockerfile. Made
# once, below, because t_run carries TEHAS_INSTANCE_DIR over only when it is
# already set in this file's environment.
t_env_instance() {
  export TEHAS_INSTANCE_DIR=$T/instance
  mkdir -p "$TEHAS_INSTANCE_DIR"
}

t_case "a build takes upstream's image for the newest release and labels it with the commit"
reset
t_run "$build"
t_eq "$t_rc" 0
t_contains "$(t_calls git)" "ls-remote --tags https://github.com/Up-Owner/pi-rukas.git"
t_contains "$(t_calls docker)" "pull -q ghcr.io/up-owner/pi-rukas:sha-aaaaaaaaaaaa"
t_contains "$(t_calls docker)" "--build-arg PI_RUKAS_IMAGE=ghcr.io/up-owner/pi-rukas:sha-aaaaaaaaaaaa --build-arg PI_RUKAS_COMMIT=$head"
t_contains "$(t_calls docker)" "tag tehas/pi-rukas:"
t_contains "$t_out" "with pi-rukas $head (release v0.12.80), issue images: 1, also tagged tehas/pi-rukas:uid$uid"
# A plain build does not ask what the current image is on.
t_lacks "$(t_calls docker)" "image inspect"

t_case "the base is asked for its capabilities, and whether it reads issue images is its answer"
t_contains "$(t_calls docker)" "run --rm --init ghcr.io/up-owner/pi-rukas:sha-aaaaaaaaaaaa /opt/pi-rukas/bin/pi-rukas-headless --capabilities"
t_contains "$(t_calls docker)" "--build-arg TEHAS_RUKAS_IMAGES=1"
t_contains "$t_out" "issue images: 1"
reset
jq 'del(.inputs[] | select(.name == "PI_ENSEMBLE_ISSUE_IMAGES"))' "$caps" >"$fx/caps.json"
t_run "$build"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "--build-arg TEHAS_RUKAS_IMAGES=0"
reset
jq '(.inputs[] | select(.name == "PI_ENSEMBLE_ISSUE_IMAGES") | .manifestVersion) = 2' "$caps" >"$fx/caps.json"
t_run "$build"
t_contains "$(t_calls docker)" "--build-arg TEHAS_RUKAS_IMAGES=0"

t_case "a base on result schema 5 passes the check, and one on an older schema is not built on"
reset
t_run "$build"
t_eq "$t_rc" 0
t_lacks "$t_out" "does not support"
for old in headless-0.12.80 headless-1ca933e headless-316134e; do
  reset
  cp "$T_ROOT/test/fixtures/$old/capabilities.json" "$fx/caps.json"
  t_run "$build"
  t_eq "$t_rc" 1
  t_contains "$t_out" "result schema version $(jq .contracts.resultSchema.version "$fx/caps.json"), not 5"
  t_lacks "$(t_calls docker)" "build --build-arg"
done

t_case "a base whose contract tehas work cannot use is not built on, and each gap is named"
reset
jq '.contracts.resultSchema.version = 6 | .verbs.work.flags -= ["--timeout"]' "$caps" >"$fx/caps.json"
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "does not support what tehas work needs, not building"
t_contains "$t_out" "result schema version 6, not 5"
t_contains "$t_out" "the flag --timeout"
t_lacks "$(t_calls docker)" "build --build-arg"

t_case "a base with no answer, or one that is not JSON, is not built on"
reset
rm -f "$fx/caps.json"
echo 2 >"$fx/ask-rc"
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "gave no --capabilities answer (exit 2)"
t_lacks "$(t_calls docker)" "build --build-arg"
reset
echo "pi-rukas-headless: unknown verb" >"$fx/caps.json"
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "is not a JSON object"
t_lacks "$(t_calls docker)" "build --build-arg"

t_case "a base that cannot be run stops the build and says why"
reset
echo 125 >"$fx/ask-rc"
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "could not be run to be asked (docker said 125)"
t_lacks "$(t_calls docker)" "build --build-arg"

t_case "without lib/headless.sh or lib/rukas.sh beside it nothing is asked or built"
for f in headless.sh rukas.sh; do
  reset
  mv "$home/lib/$f" "$T/$f"
  t_run "$build"
  t_eq "$t_rc" 1
  t_contains "$t_out" "no lib/headless.sh and lib/rukas.sh"
  t_eq "$(t_calls git)$(t_calls docker)" ""
  mv "$T/$f" "$home/lib/$f"
done

t_case "with --if-moved an image already on the commit followed is left alone"
reset
echo "$head" >"$fx/label"
t_run "$build --if-moved"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas/pi-rukas:uid$uid is on $head, release v0.12.80"
t_lacks "$(t_calls docker)" "build"

t_case "with --if-moved an image on another commit is rebuilt"
reset
echo "$old" >"$fx/label"
t_run "$build --if-moved"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "PI_RUKAS_COMMIT=$head"

t_case "--check says the commit moved and builds nothing"
reset
echo "$old" >"$fx/label"
t_run "$build --check"
t_eq "$t_rc" 1
t_contains "$t_out" "is on $old, release v0.12.80 is $head"
t_eq "$(t_calls docker | grep -c -e '^build' -e '^pull')" 0

t_case "--check of an image with no label says so"
reset
t_run "$build --check"
t_eq "$t_rc" 1
t_contains "$t_out" "is on no known commit"

t_case "the build passes the code index pin from versions.env"
reset
t_run "$build"
t_eq "$t_rc" 0
cbm=$(sed -n 's/^CBM_VERSION=//p' "$home/versions.env")
cbm_sha=$(sed -n 's/^CBM_INSTALLER_SHA256=//p' "$home/versions.env")
t_contains "$(t_calls docker)" "--build-arg CBM_VERSION=$cbm"
t_contains "$(t_calls docker)" "--build-arg CBM_INSTALLER_SHA256=$cbm_sha"

t_case "without a commit to follow nothing is pulled or built"
reset
rm -f "$fx/head"
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "no pi-rukas commit for PI_RUKAS_FOLLOW=release in https://github.com/Up-Owner/pi-rukas.git"
t_eq "$(t_calls docker)" ""

t_case "branch:<name> builds on the head of that branch"
reset
sed -i 's/^PI_RUKAS_FOLLOW=.*/PI_RUKAS_FOLLOW=branch:main/' "$home/versions.env"
t_run "$build"
t_eq "$t_rc" 0
t_contains "$(t_calls git)" "ls-remote https://github.com/Up-Owner/pi-rukas.git refs/heads/main"
t_contains "$t_out" "with pi-rukas $head (the head of main)"
sed -i 's/^PI_RUKAS_FOLLOW=.*/PI_RUKAS_FOLLOW=release/' "$home/versions.env"

t_case "a PI_RUKAS_FOLLOW that is neither release nor branch:<name> builds nothing"
reset
sed -i 's/^PI_RUKAS_FOLLOW=.*/PI_RUKAS_FOLLOW=main/' "$home/versions.env"
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "no pi-rukas commit for PI_RUKAS_FOLLOW=main"
t_eq "$(t_calls git)$(t_calls docker)" ""
sed -i 's/^PI_RUKAS_FOLLOW=.*/PI_RUKAS_FOLLOW=release/' "$home/versions.env"

t_case "a commit whose image is not published yet builds nothing"
reset
: >"$fx/pull-fails"
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "no image ghcr.io/up-owner/pi-rukas:sha-aaaaaaaaaaaa yet"
t_lacks "$(t_calls docker)" "build"

t_case "an argument it does not know is refused"
reset
t_run "$build --pin"
t_eq "$t_rc" 64
t_eq "$(t_calls git)$(t_calls docker)" ""

t_case "the Dockerfile takes its base from the build, not from a pin"
t_contains "$(cat "$T_ROOT/host/sandbox/Dockerfile")" 'FROM ${PI_RUKAS_IMAGE}'
t_lacks "$(cat "$T_ROOT/host/versions.env")" "PI_RUKAS_COMMIT"
t_contains "$(cat "$T_ROOT/host/versions.env")" "PI_RUKAS_FOLLOW=release"

# The instance's own sandbox.Dockerfile, when it has one, is built on the image
# above and takes the tag cycles run. TEHAS_INSTANCE_DIR names the directory; the
# cases that do not want a second build remove the file, and build.sh passes the
# path of the file it builds as its last two arguments.
t_case "without an instance sandbox.Dockerfile there is one build and the uid tag is that image"
reset
rm -f "$T/instance/sandbox.Dockerfile"
t_run "$build"
t_eq "$t_rc" 0
t_eq "$(t_calls docker | grep -c -e '^build ')" 1
t_contains "$(t_calls docker)" "tag tehas/pi-rukas:"
t_lacks "$(t_calls docker)" "TEHAS_BASE"

t_case "an instance sandbox.Dockerfile is built on that image and takes the tag cycles run"
reset
t_env_instance
instance_file=$TEHAS_INSTANCE_DIR/sandbox.Dockerfile
printf 'ARG TEHAS_BASE\nFROM $TEHAS_BASE\n' >"$instance_file"
rm -f "$T/instance/instance.env"
t_run "$build"
t_eq "$t_rc" 0
# Two builds, the tehas image first and the instance's on top of it. The second
# names the image the first made as its base, so the file itself names no tag,
# and the result is what carries the uid tag.
t_eq "$(t_calls docker | grep -c -e '^build ')" 2
t_eq "$(t_calls docker | sed -n "s|^build .* $instance_file $TEHAS_INSTANCE_DIR\$|instance|p")" "instance"
t_eq "$(t_calls docker | sed -n "s|^build .* $TEHAS_INSTANCE_DIR\$|tehas|p")" "tehas"
second=$(t_calls docker | grep -e "^build -t ")
t_contains "$second" "--build-arg=TEHAS_BASE=tehas/pi-rukas:"
t_contains "$second" "-f $instance_file"
product=$(sed -n 's/^build -t \([^ ]*\)-product .*/\1/p' <<<"$second")
t_eq "$(t_calls docker | grep -e '^tag tehas/pi-rukas:[0-9]\{6\}-[0-9]\{4\}-product ')" "tag $product-product tehas/pi-rukas:uid$uid"
t_lacks "$(cat "$instance_file")" "tehas/pi-rukas"
t_contains "$t_out" "from $instance_file, also tagged tehas/pi-rukas:uid$uid"

t_case "an instance file that does not build fails the build and says which file"
reset
printf 'ARG TEHAS_BASE\nFROM $TEHAS_BASE\nRUN false\n' >"$T/instance/sandbox.Dockerfile"
printf '%s\n' "$T/instance/sandbox.Dockerfile" >"$fx/build-fails"
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "$T/instance/sandbox.Dockerfile did not build"
# The tehas image and its uid tag are there as they were; only the layer that
# did not build is missing.
t_eq "$(t_calls docker | grep -c -e '^tag ')" 1
rm -f "$T/instance/sandbox.Dockerfile" "$fx/build-fails"

t_case "an instance directory with no sandbox.Dockerfile in it is no second build"
reset
# TEHAS_INSTANCE_DIR is set for every case in this file, so the directory being
# there with nothing to add has to be the build above and nothing more.
printf 'TEHAS_REPO=acme/widget\n' >"$T/instance/instance.env"
t_run "$build"
t_eq "$t_rc" 0
t_eq "$(t_calls docker | grep -c -e '^build ')" 1
t_lacks "$(t_calls docker)" "TEHAS_BASE"
t_contains "$t_out" "also tagged tehas/pi-rukas:uid$uid"

t_done
