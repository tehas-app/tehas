#!/usr/bin/env bash
# Tests for host/sandbox/build.sh, split from sandbox_build_test.sh: the smoke
# run of the image it made, and the instance layer on it, in $T/instance.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/sandbox_build_lib.sh
. "$(dirname "$0")/sandbox_build_lib.sh" || exit 1
t_env_instance

# A case that builds an instance layer calls fx_ids: docker's answers for the
# layers of both images are files under fx, which reset() clears. layers-platform
# is the chain of the image the build made, layers-product that of the layer on
# it, and layers-cached one that starts otherwise, as a build Docker served from
# its cache: a case picks it by copying it over layers-product.
fx_ids() {
  printf 'sha256:aaaaaaaa\nsha256:bbbbbbbb\n' >"$fx/layers-platform"
  printf 'sha256:aaaaaaaa\nsha256:bbbbbbbb\nsha256:cccccccc\n' >"$fx/layers-product"
  printf 'sha256:aaaaaaaa\nsha256:dddddddd\nsha256:cccccccc\n' >"$fx/layers-cached"
  printf 'layers-platform\n' >"$fx/which-layers"
  printf 'layers-product\n' >"$fx/which-product"
  # The stub, written once in sandbox_build_lib.sh, reads which file the two
  # inspects answer, so a stub of a case's own cannot lose them.
}

t_case "a build smoke-runs the image it made, in one offline work turn, before the tag moves"
reset
t_run "$build"
t_eq "$t_rc" 0
# The run is asked of bash in the image this build made, with no network, and
# it is asked before the tag moves.
t_contains "$(t_calls docker)" "run --pull=never --rm --init --network none --memory 2g --pids-limit 512 --entrypoint bash tehas/pi-rukas:"
t_contains "$(t_calls docker)" "timeout 20 /opt/pi-rukas/bin/pi-rukas-headless work 1 --json 2>&1 | sed "
t_contains "$t_out" "smoke-ran tehas/pi-rukas:"
smoke_at=$(t_calls docker | grep -n -m1 '^run --pull=never --rm --init --network none ' | cut -d: -f1)
tag_at=$(t_calls docker | grep -n -m1 '^tag ' | cut -d: -f1)
t_eq "$([ -n "$smoke_at" ] && [ -n "$tag_at" ] && [ "$smoke_at" -lt "$tag_at" ] && echo before)" before

t_case "a smoke run that prints step-started moves the uid tag"
reset
cp "$T_ROOT/test/fixtures/smoke-started.json" "$fx/smoke"
t_run "$build"
t_eq "$t_rc" 0
t_contains "$t_out" "smoke-ran tehas/pi-rukas:"
# The tag names the image the build made, as it did before the check was there:
# a run that started a step costs the tag nothing.
t_eq "$(t_calls docker | awk '$1 == "tag" && $3 ~ /^tehas\/pi-rukas:uid/ { print $2 }')" \
  "$(sandbox_builds | sed -n 's/^build .* -t \([^ ]*\) .*/\1/p' | head -n 1)"

t_case "a smoke run that prints the crash document leaves the uid tag where it was and ends 1"
reset
smoke_crash
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "failed its smoke run (crashed)"
t_contains "$t_out" "tehas/pi-rukas:uid$uid is left as it was"
# The image built, and no tag of the run moved onto it.
t_contains "$(t_calls docker)" "build --build-arg"
t_eq "$(t_calls docker | grep -c -e '^tag ')" 0

t_case "a smoke run that prints nothing leaves the uid tag where it was and ends 1"
reset
smoke_silence
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "failed its smoke run (silent)"
t_contains "$(t_calls docker)" "build --build-arg"
t_eq "$(t_calls docker | grep -c -e '^tag ')" 0

t_case "a smoke run whose container starts and answers nothing is refused, not waited on"
reset
# The shape of a container that was started and printed no line at all, which is
# what the runner of pi-rukas v0.14.30 to v0.14.40 left every work run with.
: >"$fx/smoke"
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "failed its smoke run (silent)"
t_eq "$(t_calls docker | grep -c -e '^tag ')" 0

t_case "a smoke run runs vipune as this uid on one CPU of a cpuset, after the work turn"
reset
t_run "$build"
t_eq "$t_rc" 0
vipune_run=$(t_calls docker | grep -m1 -e ' --cpuset-cpus ')
t_contains "$vipune_run" "--user $uid -e HOME=/home/vscode --entrypoint bash tehas/pi-rukas:"
t_contains "$vipune_run" "vipune search "
t_lacks "$vipune_run" "ORT_INTRA_OP_NUM_THREADS"
work_at=$(t_calls docker | grep -n -m1 'pi-rukas-headless work 1' | cut -d: -f1)
vipune_at=$(t_calls docker | grep -n -m1 -e ' --cpuset-cpus ' | cut -d: -f1)
t_eq "$([ -n "$work_at" ] && [ -n "$vipune_at" ] && [ "$work_at" -lt "$vipune_at" ] && echo after)" after

t_case "a smoke run whose vipune writes to stderr under a cpuset leaves the uid tag where it was and ends 1"
reset
# What onnxruntime printed on every vipune call of a cycle while no thread
# count was set: one line per thread it could not pin.
printf '%s\n' 'pthread_setaffinity_np failed for thread: 12, index: 3, mask: {4, }, error code: 22 error msg: Invalid argument.' quiet >"$fx/vipune"
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "failed its smoke run (noisy)"
t_contains "$t_out" "pthread_setaffinity_np failed"
t_contains "$t_out" "tehas/pi-rukas:uid$uid is left as it was"
t_eq "$(t_calls docker | grep -c -e '^tag ')" 0

# The CPUs this test may run on, as the build reads its own: dockerd's answer
# below names CPUs from this list, so the retry finds one to take on any host.
cpus=$(sed -n 's/^Cpus_allowed_list:[[:space:]]*//p' /proc/self/status)
low=${cpus%%[-,]*}
refused() {
  printf 'docker: Error response from daemon: Requested CPUs are not available - requested 999, available: %s\n\nRun %s for more information\n' "$1" "'docker run --help'" >"$fx/vipune.first"
}

t_case "a smoke run whose vipune CPU dockerd refuses runs it again on a CPU dockerd takes"
reset
refused "$low"
t_run "$build"
t_eq "$t_rc" 0
t_contains "$t_out" "the vipune check runs on CPU $low"
t_eq "$(t_calls docker | grep -c -e ' --cpuset-cpus ')" 2
t_eq "$(t_calls docker | grep -e ' --cpuset-cpus ' | tail -n 1 | sed -n 's/.* --cpuset-cpus \([0-9]*\) .*/\1/p')" "$low"
t_eq "$(t_calls docker | grep -c -e '^tag ')" 1

t_case "a smoke run whose CPUs dockerd takes none of says to restart docker, leaves the tag and ends 1"
reset
# A CPU past any list a host has: dockerd's start saw none of this process's.
refused 4095
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "docker takes none of the CPUs this process may run on; restart docker"
t_eq "$(t_calls docker | grep -c -e ' --cpuset-cpus ')" 1
t_contains "$t_out" "tehas/pi-rukas:uid$uid is left as it was"
t_eq "$(t_calls docker | grep -c -e '^tag ')" 0

t_case "the instance layer is smoke-run too, and a refusal there leaves the tag on the image under it"
reset
printf 'ARG TEHAS_BASE\nFROM $TEHAS_BASE\n' >"$T/instance/sandbox.Dockerfile"
fx_ids
# The platform image starts a step and the layer above it prints the crash
# document: the tag moves once, onto the image the host can run, and the second
# move is refused. The two runs are answered apart by the image each is in.
printf '%s\n' '{"schemaVersion":5,"error":{"code":"crash"}}' >"$fx/smoke-product"
t_run "$build"
t_eq "$t_rc" 1
t_contains "$t_out" "smoke-ran tehas/pi-rukas:"
t_contains "$t_out" "-product failed its smoke run (crashed)"
# The tag already moved onto the platform image, and the line says so.
t_contains "$t_out" "tehas/pi-rukas:uid$uid names tehas/pi-rukas:"
t_contains "$t_out" "without the layer of $T/instance/sandbox.Dockerfile"
t_lacks "$t_out" "is left as it was"
# One tag moved: the platform image's. The layer that was refused takes nothing.
t_eq "$(t_calls docker | grep -c -e '^tag ')" 1
uidtag=$(t_calls docker | awk '$1 == "tag" && $3 ~ /^tehas\/pi-rukas:uid/ { print $2 }')
t_lacks "$uidtag" "-product"
rm -f "$T/instance/sandbox.Dockerfile"

# The instance layer, when the host has one: built on the image above, checked
# against it, and the only thing allowed to move the tag cycles run. The cases
# that do not want a second build remove the file.
t_case "without an instance sandbox.Dockerfile there is one build and the uid tag is that image"
reset
rm -f "$T/instance/sandbox.Dockerfile"
t_run "$build"
t_eq "$t_rc" 0
t_eq "$(sandbox_builds | grep -c .)" 1
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
t_eq "$(sandbox_builds | grep -c .)" 2
t_eq "$(t_calls docker | sed -n "s|^build .* $instance_file $TEHAS_INSTANCE_DIR\$|instance|p")" "instance"
t_eq "$(t_calls docker | sed -n "s|^build .* $TEHAS_INSTANCE_DIR\$|tehas|p")" "tehas"
second=$(sandbox_builds | grep -e "^build -t ")
t_contains "$second" "--build-arg=TEHAS_BASE=tehas/pi-rukas:"
t_contains "$second" "-f $instance_file"
product=$(sed -n 's/^build -t \([^ ]*\)-product .*/\1/p' <<<"$second")
# The tag cycles run moves once, onto the layer rather than the image under it:
# the last tag of the run names the -product image.
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
t_contains "$t_out" "tehas/pi-rukas:uid$uid names tehas/pi-rukas:"
t_lacks "$t_out" "is left as it was"
# The tag cycles run is left on this run's platform image, not the layer that
# failed to build.
uidtag=$(t_calls docker | awk '$1 == "tag" && $3 ~ /^tehas\/pi-rukas:uid/ { print $2 }')
t_eq "$uidtag" "$(sandbox_builds | sed -n 's/^build .* -t \([^ ]*\) .*/\1/p' | head -n 1)"
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
t_contains "$t_out" "without the layer of $T/instance/sandbox.Dockerfile"
t_lacks "$(t_calls docker | grep -e '^tag ')" "-product "
rm -f "$T/instance/sandbox.Dockerfile" "$fx/product-missing"

t_case "the layers are asked one id per line, so a layer on this run's image passes the check (#1653)"
reset
printf 'ARG TEHAS_BASE\nFROM $TEHAS_BASE\n' >"$T/instance/sandbox.Dockerfile"
fx_ids
# The shape seen on Docker 29: a base of many layers and one layer on top. The
# stub prints {{.RootFS.Layers}} as Docker does, one bracketed line, so a check
# that asked for that field would read one id from each and refuse the layer.
for n in $(seq 1 52); do printf 'sha256:%064d\n' "$n"; done >"$fx/layers-platform"
{ cat "$fx/layers-platform"; printf 'sha256:%064d\n' 53; } >"$fx/layers-product"
t_run "$build"
t_eq "$t_rc" 0
t_lacks "$t_out" "was not built on"
t_eq "$(t_calls docker | grep -c -F 'image inspect --format {{range .RootFS.Layers}}{{println .}}{{end}} ')" 2
t_contains "$(t_calls docker | grep -e '^tag ')" "-product "
rm -f "$T/instance/sandbox.Dockerfile"

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

t_case "an instance layer that is already the newest image is not kept past the two builds"
reset
# A host that updates on a schedule must not gather a -product tag per build:
# the dated sweep counts and removes the instance's tag with the date tags.
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

t_case "a dated tag a container was made from is kept, and the count says so"
reset
# The same sweep with a container in the way: the image the unit ran stays,
# held, not counted, and the line says what kept it.
fx_ids
printf '%s\n' "uid$uid sha256:1" "261009-1000 sha256:1" "261009-1000-product sha256:2" \
  "261010-1000 sha256:3" "261011-1000 sha256:4" >"$fx/images"
printf '%s\n' "uid$uid sha256:1" >"$fx/containers"
t_run "$build"
t_eq "$t_rc" 0
t_contains "$t_out" "kept tehas/pi-rukas:261009-1000: a container or a uid tag uses it"

t_case "an instance directory with no sandbox.Dockerfile in it is no second build"
reset
# TEHAS_INSTANCE_DIR is set for every case in this file, so the directory being
# there with nothing to add has to be the build above and nothing more.
printf 'TEHAS_GITHUB_APP_ID=1\nTEHAS_REPO=acme/widget\n' >"$T/instance/instance.env"
t_run "$build"
t_eq "$t_rc" 0
t_eq "$(sandbox_builds | grep -c .)" 1
t_lacks "$(t_calls docker)" "TEHAS_BASE"
t_contains "$t_out" "also tagged tehas/pi-rukas:uid$uid"

t_case "the instance directory from the pointer file is used, and a build that saw no instance at all says the file it looked in"
reset
# A login shell that never sourced the settings has no TEHAS_INSTANCE_DIR:
# build.sh resolves the directory itself, and says so.
mkdir -p "$T/home/.config/tehas"
printf 'ARG TEHAS_BASE\nFROM $TEHAS_BASE\n' >"$T/home/.config/tehas/sandbox.Dockerfile"
fx_ids
# shellcheck disable=SC2034  # t_run reads T_ENV, which names what it carries over
T_ENV=(T_ROOT="$T_ROOT")
unset TEHAS_INSTANCE_DIR
t_run "$build"
t_eq "$t_rc" 0
t_eq "$(sandbox_builds | grep -c .)" 2
t_contains "$t_out" "$T/home/.config/tehas/sandbox.Dockerfile"
rm -f "$T/home/.config/tehas/sandbox.Dockerfile"

t_case "with no instance anywhere the build says the file it looked for and builds once"
reset
# shellcheck disable=SC2034  # t_run reads T_ENV, which names what it carries over
T_ENV=(T_ROOT="$T_ROOT")
unset TEHAS_INSTANCE_DIR
t_run "$build"
t_eq "$t_rc" 0
t_eq "$(sandbox_builds | grep -c .)" 1
t_lacks "$(t_calls docker)" "TEHAS_BASE"
# An image on the commit followed is still behind when the instance's layer is
# not the one it was built from: its tehas.layer label is the sum console
# sandbox layer gives of the instance's sandbox.Dockerfile and what it copies.
t_env_instance
layer_file=$TEHAS_INSTANCE_DIR/sandbox.Dockerfile

t_case "with --if-moved an image on the commit followed and on the instance's layer is left alone"
reset
fx_ids
echo "$head" >"$fx/label"
printf 'ARG TEHAS_BASE\nFROM $TEHAS_BASE\n' >"$layer_file"
"$home/libexec/console" sandbox layer "$layer_file" >"$fx/layer-label"
t_run "$build --if-moved"
t_eq "$t_rc" 0
t_contains "$t_out" "is on $head, release v0.15.80"
t_lacks "$(t_calls docker)" "build"

t_case "with --if-moved a changed sandbox.Dockerfile is rebuilt and the layer labelled with its sum (#2496)"
reset
fx_ids
echo "$head" >"$fx/label"
printf 'ARG TEHAS_BASE\nFROM $TEHAS_BASE\n' >"$layer_file"
"$home/libexec/console" sandbox layer "$layer_file" >"$fx/layer-label"
printf 'ARG TEHAS_BASE\nFROM $TEHAS_BASE\nRUN true\n' >"$layer_file"
sum=$("$home/libexec/console" sandbox layer "$layer_file")
t_run "$build --if-moved"
t_eq "$t_rc" 0
t_contains "$t_out" "its instance layer is $(cat "$fx/layer-label"), sandbox.Dockerfile is $sum"
t_contains "$(sandbox_builds | grep -e '^build -t ')" "--label=tehas.layer=$sum"
t_contains "$(t_calls docker | grep -e '^tag ')" "-product "

t_case "with --if-moved an image with no layer label is rebuilt when the instance has a layer"
reset
fx_ids
echo "$head" >"$fx/label"
printf 'ARG TEHAS_BASE\nFROM $TEHAS_BASE\n' >"$layer_file"
t_run "$build --if-moved"
t_eq "$t_rc" 0
t_contains "$t_out" "its instance layer is none"
t_eq "$(sandbox_builds | grep -c .)" 2

t_case "with --if-moved an image built with a layer the instance no longer has is rebuilt"
reset
echo "$head" >"$fx/label"
echo "sha256:$(printf '1%.0s' {1..64})" >"$fx/layer-label"
rm -f "$layer_file"
t_run "$build --if-moved"
t_eq "$t_rc" 0
t_contains "$t_out" "sandbox.Dockerfile is none"
t_eq "$(sandbox_builds | grep -c .)" 1

t_case "--check says the instance's layer changed and builds nothing"
reset
echo "$head" >"$fx/label"
printf 'ARG TEHAS_BASE\nFROM $TEHAS_BASE\n' >"$layer_file"
t_run "$build --check"
t_eq "$t_rc" 3
t_contains "$t_out" "its instance layer is none"
t_eq "$(sandbox_builds | grep -c .)" 0
rm -f "$layer_file"

t_done
