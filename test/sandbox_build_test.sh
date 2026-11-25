#!/usr/bin/env bash
# Tests for host/sandbox/build.sh, against stub git and docker: it builds the
# base from the host's pi-rukas clone at the commit versions.env follows, then
# the sandbox image on it, only when that image's --capabilities answer has the
# contract tehas work needs, labels the image with that commit and with whether
# its pi-rukas reads issue images, and with --if-moved or --check looks at the
# label of the current image first. sandbox_build_smoke_test.sh holds the smoke
# run and the instance layer.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/sandbox_build_lib.sh
. "$(dirname "$0")/sandbox_build_lib.sh" || exit 1

t_case "a build makes the base from the clone at the newest release, builds on it and labels it with the commit"
reset
t_run "$build"
t_eq "$t_rc" 0
t_contains "$(t_calls git)" "ls-remote --tags https://github.com/Up-Owner/pi-rukas.git"
t_contains "$(t_calls git)" "-C $T/home/tehas/repos/pi-rukas archive --add-file=$home/sandbox/base.Dockerfile $head"
t_eq "$(cat "$fx/base-context")" "tree"
t_contains "$(t_calls docker)" "build -f base.Dockerfile -t $base --build-arg=TEHAS_UID=$uid --build-arg=GITHUB_SHA=$head"
t_contains "$(t_calls docker)" "--build-arg=OO_VERSION=$(sed -n 's/^OO_VERSION=//p' "$home/versions.env") -"
t_lacks "$(t_calls docker | sed 's/ --pull=never / /')" "pull"
t_contains "$(t_calls docker)" "--build-arg PI_RUKAS_IMAGE=$base --build-arg PI_RUKAS_COMMIT=$head"
t_contains "$(t_calls docker)" "tag tehas/pi-rukas:"
t_contains "$t_out" "with pi-rukas $head (release v0.15.80), issue images: 1, also tagged tehas/pi-rukas:uid$uid"
# A plain build does not ask what the current image is on.
t_lacks "$(t_calls docker)" "image inspect"
# A clone that already holds the commit is not fetched.
t_lacks "$(t_calls git)" "fetch"
# The bases of this uid but this build's lose their tags.
t_contains "$(t_calls docker)" "image ls --format {{.Tag}} tehas/pi-rukas-base"

t_case "the base is asked for its capabilities, and whether it reads issue images is its answer"
t_contains "$(t_calls docker)" "run --pull=never --rm --init $base /opt/pi-rukas/bin/pi-rukas-headless --capabilities"
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

t_case "the base is passed every argument its recipe declares, and the pins come from versions.env"
base_args=$(sed -n 's/^ARG \([A-Z0-9_]*\).*/\1/p' "$T_ROOT/host/sandbox/base.Dockerfile" | LC_ALL=C sort -u)
passed=$(grep -oE -- '--build-arg=[A-Z0-9_]+=' "$T_ROOT/host/sandbox/build.sh" | sed 's/^--build-arg=//; s/=$//' |
  grep -vx TEHAS_BASE | LC_ALL=C sort -u)
t_eq "$passed" "$base_args"
for arg in $base_args; do
  [ "$arg" = TEHAS_UID ] || [ "$arg" = GITHUB_SHA ] || grep -q "^$arg=" "$T_ROOT/host/versions.env" ||
    t_bad "$arg is not in versions.env"
done
# pi-rukas's own recipe is read for the floor of the pi version.
t_contains "$(cat "$T_ROOT/host/sandbox/base.Dockerfile")" "source=.devcontainer/Dockerfile"

t_case "each apt step of the base retries its fetches, and the whole step once after a pause"
recipe=$T_ROOT/host/sandbox/base.Dockerfile
t_eq "$(grep -c 'apt-get update' "$recipe")" 3
t_eq "$(grep -c 'apt-get update -o Acquire::Retries=5 ' "$recipe")" 3
t_eq "$(grep -c 'apt-get install' "$recipe")" 3
t_eq "$(grep -c 'apt-get install -y -o Acquire::Retries=5 --no-install-recommends' "$recipe")" 3
t_eq "$(grep -c 'RUN for try in 1 2; do' "$recipe")" 3
t_eq "$(grep -c '&& break; ' "$recipe")" 3
t_eq "$(grep -c 'rm -rf /var/lib/apt/lists/\*; sleep 30; ' "$recipe")" 3

t_case "the base is built from pinned inputs: images by digest, downloads checked before they run"
recipe=$T_ROOT/host/sandbox/base.Dockerfile
froms=$(grep -E '^FROM ' "$recipe")
t_eq "$(printf '%s\n' "$froms" | grep -cvE '^FROM [^ ]+@sha256:[0-9a-f]{64}( AS [a-z-]+)?$')" 0
t_eq "$(printf '%s\n' "$froms" | wc -l)" 2
# No download goes straight into a shell, and no tool is taken at its newest.
t_eq "$(grep -cE '\| *(ba)?sh\b|@latest|sh\.rustup\.rs|deb\.nodesource|bun\.sh/install' "$recipe")" 0
# Every file it fetches is checked against a pinned sum.
t_eq "$(grep -c -- ' -o /tmp/' "$recipe")" "$(grep -c '_SHA256}  /tmp/' "$recipe")"
for tree in pi ctx7 parallel-web-cli; do
  t_contains "$(cat "$recipe")" "COPY --from=npm $tree/package.json $tree/package-lock.json /opt/npm/$tree/"
done
t_contains "$(cat "$recipe")" 'pin pi @earendil-works/pi-coding-agent "$PI_VERSION" "$PI_LOCK_SHA256" --ignore-scripts'
reset
t_run "$build"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "--build-context=npm=$home/sandbox/../npm -"

t_case "tehas's node has a prefix of its own, and pi and the npm tools run it whatever node is first on PATH (#2607)"
recipe=$(cat "$T_ROOT/host/sandbox/base.Dockerfile")
t_contains "$recipe" 'tar -xJf /tmp/node.tar.xz -C /opt/tehas-node --strip-components=1 --no-same-owner'
t_lacks "$recipe" 'tar -xJf /tmp/node.tar.xz -C /usr/local '
t_contains "$recipe" 'for b in node npm npx; do ln -sf "/opt/tehas-node/bin/$b" "/usr/local/bin/$b"; done'
# A global npm install still lands in /usr/local, on PATH, as before the move.
t_contains "$recipe" "printf 'prefix=/usr/local\n' >/opt/tehas-node/etc/npmrc"
t_contains "$recipe" 'test "$(npm prefix -g)" = /usr/local'
# tehas's node starts a node script bin by its path, not a product's node.
t_contains "$recipe" 'exec /opt/tehas-node/bin/node %s "$@"'
t_lacks "$recipe" 'ln -sf "$PWD/node_modules/.bin/$bin" "/usr/local/bin/$bin"'
# The build proves it: a node first on PATH that only fails.
t_contains "$recipe" 'printf '"'"'#!/bin/sh\nexit 97\n'"'"' >/tmp/no-node/node'
t_contains "$recipe" 'PATH=/tmp/no-node:$PATH pi --version'

t_case "the recipe's links run a node script on tehas's node and link anything else (#2607)"
# The recipe's loop, run by sh against a scratch tree.
w=$T/links
mkdir -p "$w/tree/node_modules/js/bin" "$w/tree/node_modules/.bin" "$w/bin" "$w/tehas-node/bin" "$w/product"
printf '#!/usr/bin/env node\n' >"$w/tree/node_modules/js/bin/cli.js"
printf '\177ELF' >"$w/tree/node_modules/js/bin/native"
chmod 755 "$w/tree/node_modules/js/bin/cli.js" "$w/tree/node_modules/js/bin/native"
ln -s ../js/bin/cli.js "$w/tree/node_modules/.bin/js"
ln -s ../js/bin/native "$w/tree/node_modules/.bin/native"
printf '#!/bin/sh\necho tehas "$@"\n' >"$w/tehas-node/bin/node"
printf '#!/bin/sh\necho product "$@"\n' >"$w/product/node"
chmod 755 "$w/tehas-node/bin/node" "$w/product/node"
sed -n '/&& for bin in \$bins; do \\$/,/^ *done; \\$/p' "$T_ROOT/host/sandbox/base.Dockerfile" |
  sed 's/^ *&& //; s/ *\\$//; s#/opt/tehas-node#'"$w"'/tehas-node#g; s#/usr/local/bin#'"$w"'/bin#g' >"$w/loop.sh"
t_eq "$(grep -c . "$w/loop.sh")" 6
bins="js native" sh -c 'cd "$1" && . "$2"' sh "$w/tree" "$w/loop.sh" || t_bad "the loop failed"
t_eq "$(PATH=$w/product:$PATH "$w/bin/js" a b)" "tehas $w/tree/node_modules/js/bin/cli.js a b"
t_eq "$(readlink "$w/bin/native")" "$w/tree/node_modules/js/bin/native"

t_case "the example layer pins the product's node in its own prefix, ahead of tehas's on PATH (#2607)"
layer=$T_ROOT/instances/example/product-node.Dockerfile
t_eq "$(grep -vE '^(#|$)' "$layer" | head -n 2)" $'ARG TEHAS_BASE\nFROM $TEHAS_BASE'
layer=$(grep -v '^#' "$layer")
t_contains "$layer" 'echo "${PRODUCT_NODE_SHA256}  /tmp/node.tar.xz" | sha256sum -c -'
t_contains "$layer" '-C /opt/product-node --strip-components=1'
t_contains "$layer" 'ENV PATH=/opt/product-node/bin:$PATH'
t_lacks "$layer" '-C /usr/local'
t_lacks "$layer" '/opt/tehas-node'
# A version or a sum left empty fails the layer, and the tag stays on the image without it.
t_contains "$layer" 'test -n "$PRODUCT_NODE_VERSION" && test -n "$PRODUCT_NODE_SHA256"'

t_case "a clone without the commit fetches its tags, and one a fetch does not bring it to builds nothing"
reset
: >"$fx/no-commit"
: >"$fx/fetch-brings"
t_run "$build"
t_eq "$t_rc" 0
t_contains "$(t_calls git)" "-C $T/home/tehas/repos/pi-rukas fetch -q --tags --force origin"
t_lacks "$(t_calls git)" "fetch -q origin $head"
reset
: >"$fx/no-commit"
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "build: the pi-rukas clone in $T/home/tehas/repos/pi-rukas does not hold $head, and a fetch of its tags and of the commit by id did not bring it"
t_eq "$(t_calls docker)" ""

# A release's commit whose tag upstream re-pointed is still served by its id.
t_case "a commit on no tag or branch upstream is fetched by its id"
reset
: >"$fx/no-commit"
: >"$fx/fetch-id-brings"
t_run "$build"
t_eq "$t_rc" 0
t_contains "$(t_calls git)" "-C $T/home/tehas/repos/pi-rukas fetch -q origin $head"
t_contains "$(t_calls docker)" "-t $base"

t_case "a base that does not build stops the build before the base is asked anything"
reset
: >"$fx/base-fails"
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "the base $base did not build"
t_lacks "$(t_calls docker)" "run "
t_eq "$(sandbox_builds)" ""

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

t_case "without libexec/tehas-cycle, libexec/console or lib/config.sh beside it nothing is asked or built"
for f in libexec/tehas-cycle libexec/console lib/config.sh; do
  reset
  mv "$home/$f" "$T/held"
  t_run "$build"
  t_eq "$t_rc" 1
  t_contains "$t_out" "no libexec/tehas-cycle, libexec/console and lib/config.sh"
  t_eq "$(t_calls git)$(t_calls docker)" ""
  mv "$T/held" "$home/$f"
done

t_case "with --if-moved an image already on the commit followed is left alone"
reset
echo "$head" >"$fx/label"
t_run "$build --if-moved"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas/pi-rukas:uid$uid is on $head, release v0.15.80"
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
t_eq "$t_rc" 3
t_contains "$t_out" "is on $old, release v0.15.80 is $head"
t_eq "$(t_calls docker | grep -c -e '^build' -e '^pull')" 0

t_case "--check of an image with no label says so"
reset
t_run "$build --check"
t_eq "$t_rc" 3
t_contains "$t_out" "is on no known commit"

t_case "the build passes the code index pin from versions.env"
reset
t_run "$build"
t_eq "$t_rc" 0
cbm=$(sed -n 's/^CBM_VERSION=//p' "$home/versions.env")
cbm_sha=$(sed -n 's/^CBM_INSTALLER_SHA256=//p' "$home/versions.env")
t_contains "$(t_calls docker)" "--build-arg CBM_VERSION=$cbm"
t_contains "$(t_calls docker)" "--build-arg CBM_INSTALLER_SHA256=$cbm_sha"

t_case "the build passes the jq pin from versions.env"
reset
t_run "$build"
t_eq "$t_rc" 0
jqv=$(sed -n 's/^JQ_VERSION=//p' "$home/versions.env")
jq_sha=$(sed -n 's/^JQ_SHA256=//p' "$home/versions.env")
t_contains "$(t_calls docker)" "--build-arg JQ_VERSION=$jqv"
t_contains "$(t_calls docker)" "--build-arg JQ_SHA256=$jq_sha"

t_case "without a commit to follow nothing is pulled or built"
reset
rm -f "$fx/head"
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "no pi-rukas commit for PI_RUKAS_FOLLOW=release:0.15 in https://github.com/Up-Owner/pi-rukas.git"
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
pin_versions() { # <key> <value>
  printf '%s=%s\n' "$1" "$2" >>"$home/versions.env"
}

t_case "commit:<sha> builds that commit with no lookup upstream"
reset
pin_versions PI_RUKAS_FOLLOW "commit:$commit40"
t_run "$build"
t_eq "$t_rc" 0
# Nothing asked upstream, which could only move it: the clone, for that commit.
t_lacks "$(t_calls git)" "ls-remote"
t_contains "$(t_calls git)" "archive --add-file=$home/sandbox/base.Dockerfile $commit40"
t_contains "$(t_calls docker)" "-t tehas/pi-rukas-base:sha-eeeeeeeeeeee-uid$uid"
t_contains "$(t_calls docker)" "--build-arg PI_RUKAS_COMMIT=$commit40"
t_contains "$t_out" "with pi-rukas $commit40 (pinned), issue images: 1"
sed -i '/^PI_RUKAS_FOLLOW=commit:/d' "$home/versions.env"

t_case "a release's image digest is no base: the base is built here all the same"
reset
pin_versions PI_RUKAS_FOLLOW "commit:$commit40"
pin_versions PI_RUKAS_IMAGE_DIGEST "$digest64"
t_run "$build"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "@sha256:"
t_lacks "$(t_calls docker | sed 's/ --pull=never / /')" "pull"
t_contains "$(t_calls docker)" "--build-arg PI_RUKAS_IMAGE=tehas/pi-rukas-base:sha-eeeeeeeeeeee-uid$uid --build-arg PI_RUKAS_COMMIT=$commit40"
sed -i '/^PI_RUKAS_FOLLOW=commit:/d; /^PI_RUKAS_IMAGE_DIGEST=/d' "$home/versions.env"

t_case "the instance's PI_RUKAS_FOLLOW wins over the release's"
reset
pin_versions PI_RUKAS_FOLLOW "commit:$(printf '1%.0s' {1..40})"
t_instance "${T_MINIMAL[@]}" "PI_RUKAS_FOLLOW=commit:$commit40"
t_run "$build"
t_eq "$t_rc" 0
t_lacks "$(t_calls git)" "ls-remote"
t_contains "$(t_calls docker)" "--build-arg PI_RUKAS_COMMIT=$commit40"
t_contains "$t_out" "with pi-rukas $commit40 (pinned)"

t_case "an instance that sets no pin builds on the release's commit"
reset
pin_versions PI_RUKAS_FOLLOW "commit:$commit40"
t_instance "${T_MINIMAL[@]}"
t_run "$build"
t_eq "$t_rc" 0
t_lacks "$(t_calls git)" "ls-remote"
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

t_case "with --check a pinned commit is compared with the image like any other"
reset
pin_versions PI_RUKAS_FOLLOW "commit:$commit40"
echo "$old" >"$fx/label"
t_run "$build --check"
t_eq "$t_rc" 3
t_eq "$(t_calls git)" ""
t_contains "$t_out" "is on $old, pinned is $commit40"
t_eq "$(t_calls docker | grep -c -e '^build' -e '^pull')" 0
sed -i '/^PI_RUKAS_FOLLOW=commit:/d' "$home/versions.env"

t_case "an argument it does not know is refused"
reset
t_run "$build --pin"
t_eq "$t_rc" 64
t_eq "$(t_calls git)$(t_calls docker)" ""

t_case "the Dockerfile takes its base from the build, not from a pin"
t_contains "$(cat "$T_ROOT/host/sandbox/Dockerfile")" 'FROM ${PI_RUKAS_IMAGE}'
t_lacks "$(cat "$T_ROOT/host/versions.env")" "PI_RUKAS_COMMIT"
t_contains "$(cat "$T_ROOT/host/versions.env")" "PI_RUKAS_FOLLOW=release"

t_done
