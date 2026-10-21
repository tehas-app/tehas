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
cp "$T_ROOT/lib/headless.sh" "$T_ROOT/lib/rukas.sh" "$T_ROOT/lib/config.sh" "$home/lib/"
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
# $fx/build-fails - the instance's own, by its path - fails. A case that builds
# an instance layer calls fx_ids, which answers the inspects of that check.
t_stub git '[ -e "$T/fx/head" ] || exit 2
case "$*" in
  *--tags*) printf "%s\trefs/tags/v0.12.9\n%s\trefs/tags/v0.12.80\n" "$(printf "c%.0s" {1..40})" "$(cat "$T/fx/head")" ;;
  *) printf "%s\trefs/heads/main\n" "$(cat "$T/fx/head")" ;;
esac'
t_stub docker 'case "$1" in
  image)
    case "$2" in
      ls) [ ! -s "$T/fx/images" ] || cat "$T/fx/images"; exit 0 ;;
      inspect)
        # What the label of an image says, which is how a run tells whether the
        # image cycles run is already on the commit it follows, and the layers of
        # an image, which is how the layer of the instance is checked against the
        # image under it. The answer comes from the file named in fx/which-layers,
        # or fx/which-product for the layer of the instance.
        # fx/ask-fails is a docker that answers no inspect at all, the shape of
        # an image that is not there.
        [ ! -e "$T/fx/ask-fails" ] || exit 1
        for a in "$@"; do case "$a" in *tehas.rukas*)
          [ -e "$T/fx/label" ] || exit 1
          cat "$T/fx/label"
          exit 0 ;; esac; done
        which=$(cat "$T/fx/which-layers" 2>/dev/null)
        for a in "$@"; do case "$a" in *-product) which=$(cat "$T/fx/which-product" 2>/dev/null) ;; esac; done
        [ -n "$which" ] && [ -s "$T/fx/$which" ] || exit 1
        cat "$T/fx/$which"
        exit 0 ;;
    esac
    # "docker image" on its own, which no command of build.sh asks for.
    [ -e "$T/fx/label" ] || exit 1; cat "$T/fx/label" ;;
  pull) [ ! -e "$T/fx/pull-fails" ] || exit 1 ;;
  run) cat "$T/fx/caps.json" 2>/dev/null; exit "$(cat "$T/fx/ask-rc" 2>/dev/null || echo 0)" ;;
  build) found=0
    for a in "$@"; do [ "$a" = "$(cat "$T/fx/build-fails" 2>/dev/null)" ] && found=1; done
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

t_case "without lib/headless.sh, lib/rukas.sh or lib/config.sh beside it nothing is asked or built"
for f in headless.sh rukas.sh config.sh; do
  reset
  mv "$home/lib/$f" "$T/$f"
  t_run "$build"
  t_eq "$t_rc" 1
  t_contains "$t_out" "no lib/headless.sh, lib/rukas.sh and lib/config.sh"
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

t_case "a PI_RUKAS_FOLLOW that is neither release nor branch:<name> nor commit:<sha> builds nothing"
reset
sed -i 's/^PI_RUKAS_FOLLOW=.*/PI_RUKAS_FOLLOW=main/' "$home/versions.env"
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "no pi-rukas commit for PI_RUKAS_FOLLOW=main"
t_eq "$(t_calls git)$(t_calls docker)" ""
sed -i 's/^PI_RUKAS_FOLLOW=.*/PI_RUKAS_FOLLOW=release/' "$home/versions.env"

# The two pins a release tarball writes into its versions.env (pack-release.sh).
commit40=$(printf 'e%.0s' {1..40})
digest64=sha256:$(printf 'f%.0s' {1..64})
t_env_instance() {
  export TEHAS_INSTANCE_DIR=$T/instance
  mkdir -p "$TEHAS_INSTANCE_DIR"
}
pin_versions() { # <key> <value>
  printf '%s=%s\n' "$1" "$2" >>"$home/versions.env"
}

t_case "commit:<sha> builds that commit with no lookup upstream, on the image of its tag"
reset
pin_versions PI_RUKAS_FOLLOW "commit:$commit40"
t_run "$build"
t_eq "$t_rc" 0
# Nothing asked of git: the release named the commit, so asking again could only
# move it. The base is still the tag of those twelve characters.
t_eq "$(t_calls git)" ""
t_contains "$(t_calls docker)" "pull -q ghcr.io/up-owner/pi-rukas:sha-eeeeeeeeeeee"
t_contains "$(t_calls docker)" "--build-arg PI_RUKAS_COMMIT=$commit40"
t_contains "$t_out" "with pi-rukas $commit40 (pinned), issue images: 1"
sed -i '/^PI_RUKAS_FOLLOW=commit:/d' "$home/versions.env"

t_case "a digest in versions.env is the base, pulled by digest rather than by tag"
reset
pin_versions PI_RUKAS_IMAGE_DIGEST "$digest64"
t_run "$build"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "pull -q ghcr.io/up-owner/pi-rukas@$digest64"
t_contains "$(t_calls docker)" "run --rm --init ghcr.io/up-owner/pi-rukas@$digest64"
t_contains "$(t_calls docker)" "--build-arg PI_RUKAS_IMAGE=ghcr.io/up-owner/pi-rukas@$digest64"
t_lacks "$(t_calls docker)" "sha-aaaaaaaaaaaa"
sed -i '/^PI_RUKAS_IMAGE_DIGEST=/d' "$home/versions.env"

t_case "the commit and the digest of a release go together, the commit labelled and the base pinned"
reset
pin_versions PI_RUKAS_FOLLOW "commit:$commit40"
printf 'PI_RUKAS_IMAGE_DIGEST=%s\n' "$digest64" >>"$home/versions.env"
t_run "$build"
t_eq "$t_rc" 0
t_eq "$(t_calls git)" ""
t_contains "$(t_calls docker)" "pull -q ghcr.io/up-owner/pi-rukas@$digest64"
t_contains "$(t_calls docker)" "--build-arg PI_RUKAS_IMAGE=ghcr.io/up-owner/pi-rukas@$digest64 --build-arg PI_RUKAS_COMMIT=$commit40"
t_contains "$t_out" "with pi-rukas $commit40 (pinned)"
sed -i '/^PI_RUKAS_FOLLOW=commit:/d; /^PI_RUKAS_IMAGE_DIGEST=/d' "$home/versions.env"

t_case "a malformed digest stops the build with 78 and asks docker nothing"
# versions.env is sourced, so values with a space are quoted.
for bad in "\"sha256:$(printf 'f%.0s' {1..63})\"" "sha256:$(printf 'f%.0s' {1..65})" \
  "'sha512:$(printf 'f%.0s' {1..64})'" "$(printf 'f%.0s' {1..64})" "sha256:$(printf 'g%.0s' {1..64})" \
  "sha256:$(printf 'F%.0s' {1..64})" "'sha256:'" "'sha256: $(printf 'f%.0s' {1..64})'"; do
  reset
  pin_versions PI_RUKAS_IMAGE_DIGEST "$bad"
  t_run "$build"
  t_eq "$t_rc" 78
  t_contains "$t_out" "is not sha256: and 64 hex digits"
  t_eq "$(t_calls git)$(t_calls docker)" ""
done

t_case "no digest line leaves the base the tag it has always been"
reset
t_run "$build"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "pull -q ghcr.io/up-owner/pi-rukas:sha-aaaaaaaaaaaa"
t_lacks "$(t_calls docker)" "@sha256:"

t_case "the instance's PI_RUKAS_FOLLOW wins, and the release's digest goes with the release's commit"
reset
pin_versions PI_RUKAS_FOLLOW "commit:$(printf '1%.0s' {1..40})"
pin_versions PI_RUKAS_IMAGE_DIGEST "$digest64"
t_instance "${T_MINIMAL[@]}" "PI_RUKAS_FOLLOW=commit:$commit40"
t_run "$build"
t_eq "$t_rc" 0
t_eq "$(t_calls git)" ""
t_contains "$(t_calls docker)" "--build-arg PI_RUKAS_COMMIT=$commit40"
# The release's digest is the image of the release's commit, not of this one.
t_contains "$(t_calls docker)" "pull -q ghcr.io/up-owner/pi-rukas:sha-eeeeeeeeeeee"
t_lacks "$(t_calls docker)" "@sha256:"
t_contains "$t_out" "with pi-rukas $commit40 (pinned)"

t_case "an instance that follows another commit and names its digest builds on that digest"
reset
pin_versions PI_RUKAS_FOLLOW "commit:$(printf '1%.0s' {1..40})"
pin_versions PI_RUKAS_IMAGE_DIGEST "sha256:$(printf '2%.0s' {1..64})"
t_instance "${T_MINIMAL[@]}" "PI_RUKAS_FOLLOW=commit:$commit40" "PI_RUKAS_IMAGE_DIGEST=$digest64"
t_run "$build"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "pull -q ghcr.io/up-owner/pi-rukas@$digest64"
t_lacks "$(t_calls docker)" "sha256:222222"

t_case "an instance that sets no pin builds on the release's commit and digest"
reset
pin_versions PI_RUKAS_FOLLOW "commit:$commit40"
pin_versions PI_RUKAS_IMAGE_DIGEST "$digest64"
t_instance "${T_MINIMAL[@]}"
t_run "$build"
t_eq "$t_rc" 0
t_eq "$(t_calls git)" ""
t_contains "$(t_calls docker)" "pull -q ghcr.io/up-owner/pi-rukas@$digest64"
t_contains "$(t_calls docker)" "--build-arg PI_RUKAS_COMMIT=$commit40"

t_case "an instance whose PI_RUKAS_FOLLOW is not valid builds nothing, with 78"
for bad in main "commit:${commit40:0:39}" ""; do
  reset
  t_instance "${T_MINIMAL[@]}" "PI_RUKAS_FOLLOW=$bad"
  t_run "$build"
  t_eq "$t_rc" 78
  t_contains "$t_out" "PI_RUKAS_FOLLOW is not valid"
  t_eq "$(t_calls git)$(t_calls docker)" ""
done

t_case "an instance whose settings do not load (a token in instance.env) builds nothing, with 78"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_MODEL_KEY_MAIN=not-here
t_run "$build"
t_eq "$t_rc" 78
t_eq "$(t_calls git)$(t_calls docker)" ""

t_case "a malformed digest in the instance's file is 78 before docker is asked anything"
reset
t_instance "${T_MINIMAL[@]}" "PI_RUKAS_IMAGE_DIGEST=sha256:$(printf 'f%.0s' {1..63})"
t_run "$build"
t_eq "$t_rc" 78
t_contains "$t_out" "is not sha256: and 64 hex digits"
t_eq "$(t_calls git)$(t_calls docker)" ""

t_case "with --check a pinned commit is compared with the image like any other"
reset
pin_versions PI_RUKAS_FOLLOW "commit:$commit40"
echo "$old" >"$fx/label"
t_run "$build --check"
t_eq "$t_rc" 1
t_eq "$(t_calls git)" ""
t_contains "$t_out" "is on $old, pinned is $commit40"
t_eq "$(t_calls docker | grep -c -e '^build' -e '^pull')" 0
sed -i '/^PI_RUKAS_FOLLOW=commit:/d' "$home/versions.env"

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

# The instance layer, on top of the image above: built on it, checked against it,
# and the only thing allowed to move the tag cycles run. t_env_instance above names
# the directory the same way every command names it.
# The instance layer is compared with the image this build made, so a case that
# builds one calls fx_ids. It asks docker for the layers of both images, and the
# answers are files under fx, which reset() clears: layers-platform is the chain
# of the image the build made, layers-product the chain of the layer on top of it
# (a chain that starts with the same layers), and layers-cached one that does not,
# the shape of a build Docker served from its own cache - a case picks it by
# copying it over layers-product. fx/ask-fails is a docker that answers nothing.
fx_ids() {
  printf 'sha256:aaaaaaaa\nsha256:bbbbbbbb\n' >"$fx/layers-platform"
  printf 'sha256:aaaaaaaa\nsha256:bbbbbbbb\nsha256:cccccccc\n' >"$fx/layers-product"
  printf 'sha256:aaaaaaaa\nsha256:dddddddd\nsha256:cccccccc\n' >"$fx/layers-cached"
  printf 'layers-platform\n' >"$fx/which-layers"
  printf 'layers-product\n' >"$fx/which-product"
  # The one stub is written once, at the top of the file, so a case cannot lose
  # the answers it needs by calling this after another installed a stub of its
  # own; what a case chooses is which file the two inspects read.
}

# The instance layer, when the host has one: built on the image above, checked
# against it, and the only thing allowed to move the tag cycles run. The cases
# that do not want a second build remove the file.
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
fx_ids
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
# The tag cycles run moves once, and onto the layer rather than the image under
# it: the last tag of the run names the -product image, and the one before it the
# image that layer was built on, which the line above named for the host that has
# no layer of its own.
uidtag=$(t_calls docker | awk '$1 == "tag" && $3 ~ /^tehas\/pi-rukas:uid/ { print $2 }' | tail -n 1)
t_eq "$uidtag" "$product-product"
t_lacks "$(cat "$instance_file")" "tehas/pi-rukas"
t_contains "$t_out" "from $instance_file, also tagged tehas/pi-rukas:uid$uid"

t_case "an instance file that does not build fails the build, says which file, and leaves the tag cycles run on this run's image"
reset
printf 'ARG TEHAS_BASE\nFROM $TEHAS_BASE\nRUN false\n' >"$T/instance/sandbox.Dockerfile"
printf '%s\n' "$T/instance/sandbox.Dockerfile" >"$fx/build-fails"
fx_ids
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "$T/instance/sandbox.Dockerfile did not build"
# The tag cycles run is left on the image the host can run, the platform image
# of this run and not the layer that failed to build: the last tag of the run is
# that one, and no tag of the run names the layer at all.
uidtag=$(t_calls docker | awk '$1 == "tag" && $3 ~ /^tehas\/pi-rukas:uid/ { print $2 }')
t_eq "$uidtag" "$(t_calls docker | grep -e '^build ' | sed -n 's/^build .* -t \([^ ]*\) .*/\1/p' | head -n 1)"
t_lacks "$uidtag" "-product"
t_lacks "$(t_calls docker | grep -e '^tag ')" "-product "
rm -f "$T/instance/sandbox.Dockerfile" "$fx/build-fails"

t_case "a layer docker served from a cached base is refused, and the tag cycles run stays off it"
reset
printf 'ARG TEHAS_BASE\nFROM $TEHAS_BASE\n' >"$T/instance/sandbox.Dockerfile"
fx_ids
cp "$fx/layers-cached" "$fx/layers-product"
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "was not built on tehas/pi-rukas:"
t_lacks "$(t_calls docker | grep -e '^tag ')" "-product "
rm -f "$T/instance/sandbox.Dockerfile" "$fx/product-missing"

t_case "a docker that answers no layers for the instance image stops the build"
reset
printf 'ARG TEHAS_BASE\nFROM $TEHAS_BASE\n' >"$T/instance/sandbox.Dockerfile"
fx_ids
: >"$fx/ask-fails"
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "docker gave no layers"
t_lacks "$(t_calls docker | grep -e '^tag ')" "-product "
rm -f "$T/instance/sandbox.Dockerfile"

t_case "an instance layer that is already the newest image is not kept past the three dated tags"
reset
# A host that updates on a schedule must not accumulate one -product tag per
# build: the dated sweep names the instance's tag too, so it is counted with the
# date tags and removed with them. The docker here answers no containers, which
# is the shape that makes the sweep say what it would have removed.
printf 'ARG TEHAS_BASE\nFROM $TEHAS_BASE\n' >"$T/instance/sandbox.Dockerfile"
fx_ids
printf '%s\n' "uid$uid sha256:1" "261009-1000 sha256:1" "261009-1000-product sha256:2" \
  "261010-1000 sha256:3" "261011-1000 sha256:4" >"$fx/images"
# No container was made from any of these images, which is the only shape from
# which the sweep may remove one.
: >"$fx/no-containers"
t_run "$build"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "image ls --no-trunc --format {{.Tag}} {{.ID}} tehas/pi-rukas"
rm -f "$T/instance/sandbox.Dockerfile"

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

t_case "the instance directory from the pointer file is used, and a build that saw no instance at all says the file it looked in"
reset
# The shape of a login shell that never sourced the settings: no
# TEHAS_INSTANCE_DIR in the environment at all. build.sh resolves the directory
# itself, so the layer is still built, and nothing about it is silent.
mkdir -p "$T/home/.config/tehas"
printf 'ARG TEHAS_BASE\nFROM $TEHAS_BASE\n' >"$T/home/.config/tehas/sandbox.Dockerfile"
fx_ids
# shellcheck disable=SC2034  # t_run reads T_ENV, which names what it carries over
T_ENV=(T_ROOT="$T_ROOT")
unset TEHAS_INSTANCE_DIR
t_run "$build"
t_eq "$t_rc" 0
t_eq "$(t_calls docker | grep -c -e '^build ')" 2
t_contains "$t_out" "$T/home/.config/tehas/sandbox.Dockerfile"
rm -f "$T/home/.config/tehas/sandbox.Dockerfile"

t_case "with no instance anywhere the build says the file it looked for and builds once"
reset
# shellcheck disable=SC2034  # t_run reads T_ENV, which names what it carries over
T_ENV=(T_ROOT="$T_ROOT")
unset TEHAS_INSTANCE_DIR
t_run "$build"
t_eq "$t_rc" 0
t_eq "$(t_calls docker | grep -c -e '^build ')" 1
t_lacks "$(t_calls docker)" "TEHAS_BASE"
t_done
