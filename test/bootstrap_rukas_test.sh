#!/usr/bin/env bash
# Tests for bootstrap.sh's pi-rukas clone and the sandbox image that follows
# it, the tools fetched by pin, the platform guard, and a tag upstream moved.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/bootstrap_lib.sh
. "$(dirname "$0")/bootstrap_lib.sh" || exit 1

# The stub git of the pi-rukas cases: it answers for the address in
# PI_RUKAS_REPO with $rukas_head as the commit of the newest release tag, and
# with $rukas_stub_behind as the commit the clone is at, so a run of `user` has
# something to move it to.
behind_head() {
  t_stub git "case \"\$*\" in
  *ls-remote*) printf '%s\trefs/tags/v0.15.81\n' $rukas_head ;;
  *rev-parse*) echo $rukas_stub_behind ;;
  *) echo 'git version 2.53.0' ;;
esac"
}
rukas_stub_behind=$(printf '0%.0s' {1..40})

t_case "an existing pi-rukas clone is fetched from the repository in versions.env"
all_present
behind_head
mkdir -p "$T/home/tehas/repos/pi-rukas/.git"
printf '#!/bin/sh\n' >"$T/home/tehas/repos/pi-rukas/install.sh"
chmod +x "$T/home/tehas/repos/pi-rukas/install.sh"
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_contains "$(t_calls git)" "remote set-url origin $PI_RUKAS_REPO"

t_case "pi-rukas behind its newest release is moved to it"
t_contains "$(t_calls git)" "checkout -q $rukas_head"

# The clone is only a copy of upstream, so its fetch takes the tags by force,
# which is what lets a release tag upstream re-points be taken at all: without
# --force git refuses it as one that would clobber a tag the clone has, and the
# run stopped right there and said nothing, because the fetch asked for quiet.
t_case "the pi-rukas fetch forces the tags it takes"
t_contains "$(t_calls git)" "fetch --tags --force origin"

# A fetch that cannot be made says so, with git's own words: this is the run
# that used to leave nothing behind after the line naming the commit it wanted.
t_case "a pi-rukas fetch that fails names the step and git's reason"
all_present
behind_head
t_stub git "case \"\$*\" in
  *fetch*) echo 'fatal: the remote did not answer' >&2; exit 128 ;;
  *ls-remote*) printf '%s\trefs/tags/v0.15.81\n' $rukas_head ;;
  *rev-parse*) echo $rukas_stub_behind ;;
  *) echo 'git version 2.53.0' ;;
esac"
mkdir -p "$T/home/tehas/repos/pi-rukas/.git"
printf '#!/bin/sh\n' >"$T/home/tehas/repos/pi-rukas/install.sh"
chmod +x "$T/home/tehas/repos/pi-rukas/install.sh"
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_eq "$t_rc" 1
t_contains "$t_out" "git fetch --tags --force failed for the pi-rukas clone"
t_contains "$t_out" "the remote did not answer"
rm -rf "$T/home/tehas/repos"
behind_head

# A release pins a commit, and upstream may re-point the tag it came from: then
# no tag or branch holds it and the fetch of the tags does not bring it. The
# clone holds it here once a fetch has named it by its id.
rukas_lost() {
  t_stub git "case \"\$*\" in
  *cat-file*) [ -e \"\$T/fetched-by-id\" ] ;;
  *'fetch -q origin $rukas_head'*) [ -z \"\${BY_ID_FAILS:-}\" ] || { echo 'fatal: not our ref' >&2; exit 128; }; : >\"\$T/fetched-by-id\" ;;
  *ls-remote*) printf '%s\trefs/tags/v0.15.81\n' $rukas_head ;;
  *checkout*) : >\"\$T/checked-out\" ;;
  *rev-parse*) if [ -e \"\$T/checked-out\" ]; then echo $rukas_head; else echo $rukas_stub_behind; fi ;;
  *) echo 'git version 2.53.0' ;;
esac"
  mkdir -p "$T/home/tehas/repos/pi-rukas/.git"
  printf '#!/bin/sh\n' >"$T/home/tehas/repos/pi-rukas/install.sh"
  chmod +x "$T/home/tehas/repos/pi-rukas/install.sh"
  rm -f "$T"/calls/* "$T/fetched-by-id" "$T/checked-out"
}

t_case "a pinned commit on no tag or branch upstream is fetched by its id"
all_present
rukas_lost
t_run "$bootstrap user"
t_eq "$t_rc" 0
t_contains "$(t_calls git)" "fetch -q origin $rukas_head"
t_contains "$(t_calls git)" "checkout -q $rukas_head"

t_case "a pinned commit a fetch by id does not bring stops user before the checkout"
all_present
rukas_lost
T_ENV+=(BY_ID_FAILS=1)
t_run "$bootstrap user"
T_ENV=(BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")
t_eq "$t_rc" 1
t_contains "$t_out" "bootstrap: the pi-rukas clone in $T/home/tehas/repos/pi-rukas does not hold $rukas_head, and a fetch of its tags and of the commit by id did not bring it: fatal: not our ref"
t_lacks "$(t_calls git)" "checkout"
rm -rf "$T/home/tehas/repos" "$T/fetched-by-id" "$T/checked-out"
behind_head

t_case "check reports a pi-rukas behind its newest release"
all_present
behind_head
mkdir -p "$T/home/tehas/repos/pi-rukas/.git"
printf '#!/bin/sh\n' >"$T/home/tehas/repos/pi-rukas/install.sh"
chmod +x "$T/home/tehas/repos/pi-rukas/install.sh"
t_run "$bootstrap check"
t_eq "$t_rc" 1
t_contains "$t_out" "$rukas_head"
t_contains "$t_out" "DIFFERS"

t_case "without a commit to follow the user part stops before pi-rukas"
t_stub git 'case "$*" in *ls-remote*) exit 2 ;; *rev-parse*) echo 0000000000000000000000000000000000000000 ;; *) echo "git version 2.53.0" ;; esac'
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_eq "$t_rc" 1
t_contains "$t_out" "no pi-rukas commit for PI_RUKAS_FOLLOW=$PI_RUKAS_FOLLOW in $PI_RUKAS_REPO"
t_lacks "$(t_calls git)" "fetch"
behind_head

t_case "with no console yet the user part leaves pi-rukas for after install.sh"
all_present
behind_head
rm "$T/home/tehas/libexec/console"
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_eq "$t_rc" 0
t_contains "$t_out" "pi-rukas waits for install.sh, then run ./bootstrap.sh user again"
t_contains "$t_out" "user part done"
t_lacks "$(t_calls git)" "ls-remote"
t_lacks "$(t_calls git)" "checkout"
t_run "$bootstrap check"
t_eq "$t_rc" 1
t_contains "$(grep '^pi-rukas ' <<<"$t_out")" "after install.sh: no console names the commit"
t_contains "$(grep '^pi-rukas ' <<<"$t_out")" "DIFFERS"

t_case "the console a release carries names the commit before any install"
all_present
rm "$T/home/tehas/libexec/console"
dir=$(pin_checkout)
mkdir -p "$dir/libexec"
cp "$T_CONSOLE" "$dir/libexec/console"
t_run "$dir/bootstrap.sh check"
t_contains "$(grep '^pi-rukas ' <<<"$t_out")" "$rukas_head"
t_lacks "$(grep '^pi-rukas ' <<<"$t_out")" "DIFFERS"
all_present
behind_head

t_case "pi-rukas's installer is not given the sandbox image's name"
# It would pull that name, fail, and build upstream's image over the tag.
printf '#!/bin/sh\necho "image=${PI_ENSEMBLE_IMAGE-unset}" >"$T/installer-env"\n' >"$T/home/tehas/repos/pi-rukas/install.sh"
T_ENV+=(PI_ENSEMBLE_IMAGE=tehas/pi-rukas:uid1000)
t_run "$bootstrap user"
T_ENV=(BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")
t_eq "$(cat "$T/installer-env")" "image=unset"
rm -rf "$T/home/tehas/repos/pi-rukas"

# The clone moved without the image is the case health calls behind: an
# installed host builds it then, and a first bootstrap, with no build.sh yet,
# leaves it to install.sh.
t_case "user moves the sandbox image after pi-rukas on an installed host"
all_present
mkdir -p "$T/home/tehas/sandbox"
printf '#!/bin/sh\necho "$*" >>"$T/build-calls"; exit "${BUILD_RC:-0}"\n' >"$T/home/tehas/sandbox/build.sh"
chmod +x "$T/home/tehas/sandbox/build.sh"
rm -f "$T/build-calls"
t_run "$bootstrap user"
t_eq "$t_rc" 0
t_eq "$(cat "$T/build-calls")" "--if-moved"
t_contains "$t_out" "bootstrap: user part done"

t_case "an installed home of another release leaves the build to install.sh"
# Its build.sh reads its own versions.env, and would refuse the tools this run
# just installed at this tree's versions.
rm -f "$T/build-calls"
sed 's/^VIPUNE_VERSION=.*/VIPUNE_VERSION=0.0.1/' "$T_ROOT/host/versions.env" >"$T/home/tehas/versions.env"
t_run "$bootstrap user"
t_eq "$t_rc" 0
t_eq "$(test -e "$T/build-calls" && echo there)" ""
t_contains "$t_out" "the installed tehas is another release; install.sh builds the sandbox image"
t_contains "$t_out" "bootstrap: user part done"

t_case "an installed home of this release builds as before"
rm -f "$T/build-calls"
cp "$T_ROOT/host/versions.env" "$T/home/tehas/versions.env"
t_run "$bootstrap user"
t_eq "$t_rc" 0
t_eq "$(cat "$T/build-calls")" "--if-moved"
rm -f "$T/home/tehas/versions.env"

t_case "an image that does not follow stops user and says how to try again"
rm -f "$T/build-calls"
T_ENV+=(BUILD_RC=1)
t_run "$bootstrap user"
T_ENV=(BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")
t_eq "$t_rc" 1
t_contains "$t_out" "the sandbox image did not follow pi-rukas to $rukas_head"
t_lacks "$t_out" "user part done"
rm -rf "$T/home/tehas/sandbox"

t_case "a first bootstrap, with no build.sh, builds nothing"
rm -f "$T/build-calls"
t_run "$bootstrap user"
t_eq "$t_rc" 0
t_eq "$(test -e "$T/build-calls" && echo there)" ""

# The sandbox build takes the instance's PI_RUKAS_FOLLOW over versions.env's,
# and the clone follows the same pi-rukas: otherwise every tehas update asks
# for a bootstrap.sh user that never moves the clone to the image's commit.
rukas_pin=$(printf '1%.0s' {1..40})
t_case "the instance's PI_RUKAS_FOLLOW wins over versions.env for the clone"
all_present
behind_head
t_instance "${T_MINIMAL[@]}" "PI_RUKAS_FOLLOW=commit:$rukas_pin"
mkdir -p "$T/home/tehas/repos/pi-rukas/.git"
printf '#!/bin/sh\n' >"$T/home/tehas/repos/pi-rukas/install.sh"
chmod +x "$T/home/tehas/repos/pi-rukas/install.sh"
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_contains "$t_out" "installing pi-rukas $rukas_pin"
t_contains "$(t_calls git)" "checkout -q $rukas_pin"
# A pinned commit asks upstream nothing.
t_lacks "$(t_calls git)" "ls-remote"
t_run "$bootstrap check"
t_contains "$(grep '^pi-rukas ' <<<"$t_out")" "$rukas_pin"

t_case "an instance that sets no PI_RUKAS_FOLLOW leaves the clone on versions.env's"
t_instance "${T_MINIMAL[@]}"
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_contains "$t_out" "installing pi-rukas $rukas_head"
t_contains "$(t_calls git)" "checkout -q $rukas_head"

t_case "an instance whose settings do not load stops user and check with 78 before anything is installed"
t_instance "${T_MINIMAL[@]}" "PI_RUKAS_FOLLOW=main"
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_eq "$t_rc" 78
t_contains "$t_out" "PI_RUKAS_FOLLOW is not valid"
t_lacks "$t_out" "is present"
t_eq "$(t_calls git)" ""
t_run "$bootstrap check"
t_eq "$t_rc" 78
t_lacks "$t_out" "TOOL"

t_case "with no console yet the instance is not loaded and pi-rukas waits for install.sh"
rm "$T/home/tehas/libexec/console"
t_run "$bootstrap user"
t_eq "$t_rc" 0
t_contains "$t_out" "pi-rukas waits for install.sh, then run ./bootstrap.sh user again"
t_no_instance
rm -rf "$T/home/tehas/repos/pi-rukas"
all_present
behind_head

# <tool>|<what --version says>|<the version in it>|<the pin>|<its download>|<the file>
for pin in "jq|jq-1.7.1|1.7.1|$JQ_VERSION|https://github.com/jqlang/jq/releases/download/jq-$JQ_VERSION/jq-linux-amd64|jq" \
  "gh|gh version 1.0.0|1.0.0|$GH_VERSION|https://github.com/cli/cli/releases/download/v$GH_VERSION/gh_${GH_VERSION}_linux_amd64.tar.gz|gh.tar.gz" \
  "tsc|Version 6.0.0|6.0.0|$TSC_VERSION|https://registry.npmjs.org/@typescript/typescript-linux-x64/-/typescript-linux-x64-$TSC_VERSION.tgz|tsc.tgz"; do
  IFS='|' read -r pin_tool says found want url file <<<"$pin"
  t_case "check reports a $pin_tool at another version, and user fetches the pinned one and refuses one that is not it"
  all_present
  t_stub "$pin_tool" "[ \"\${1:-}\" = --version ] && { echo '$says'; exit 0; }; exec $jq_real \"\$@\""
  t_run "$bootstrap check"
  t_eq "$t_rc" 1
  t_eq "$(grep "^$pin_tool " <<<"$t_out" | awk '{print $2, $3, $4}')" "$found $want DIFFERS"
  t_stub curl 'while [ $# -gt 0 ]; do [ "$1" = -o ] && { echo tampered >"$2"; exit 0; }; shift; done'
  t_run "$bootstrap user"
  t_eq "$t_rc" 1
  t_contains "$t_out" "bootstrap: installing $pin_tool $want (replacing $found)"
  t_contains "$(t_calls curl)" "$url"
  t_contains "$t_out" "checksum mismatch for $file"
  t_eq "$([ -e "$T/home/tehas/bin/$pin_tool" ] && echo installed)" ""
done

t_case "user unpacks the pinned tsc into tools and links it into bin, where it answers the pin"
all_present
t_stub tsc 'exit 1' && rm -f "$T/bin/tar"
mkdir -p "$T/pkg/package/lib" && printf '#!/bin/sh\necho "Version %s"\n' "$TSC_VERSION" >"$T/pkg/package/lib/tsc" && chmod 755 "$T/pkg/package/lib/tsc"
dir=$(pin_checkout) && tar -czf "$T/tsc.tgz" -C "$T/pkg" package
sed -i "s/^TSC_SHA256=.*/TSC_SHA256=$(sha256sum "$T/tsc.tgz" | cut -d ' ' -f 1)/" "$dir/host/versions.env"
t_stub curl "while [ \$# -gt 0 ]; do [ \"\$1\" = -o ] && { cp '$T/tsc.tgz' \"\$2\"; exit 0; }; shift; done"
t_run "$dir/bootstrap.sh user"
t_eq "$t_rc" 0
t_contains "$t_out" "bootstrap: installing tsc $TSC_VERSION"$'\n'
t_eq "$(readlink "$T/home/tehas/bin/tsc")" "$T/home/tehas/tools/tsc/lib/tsc"

t_case "the jq pin in this checkout is a version and a sum, and the image and CI take it"
t_eq "$(grep -cE '^JQ_VERSION=[0-9]+\.[0-9]+\.[0-9]+$' "$T_ROOT/host/versions.env")" 1
t_eq "$(grep -cE '^JQ_SHA256=[0-9a-f]{64}$' "$T_ROOT/host/versions.env")" 1
t_contains "$(cat "$T_ROOT/host/sandbox/Dockerfile")" 'echo "${JQ_SHA256}  /tmp/jq" | sha256sum -c -'
t_contains "$(cat "$T_ROOT/host/sandbox/Dockerfile")" 'if [ "$(jq --version)" != "jq-${JQ_VERSION}" ]'
t_contains "$(cat "$T_ROOT/.github/workflows/verify.yml")" 'echo "$JQ_SHA256  $RUNNER_TEMP/jq/jq" | sha256sum -c -'
t_contains "$(cat "$T_ROOT/.github/workflows/verify.yml")" 'echo "$RUNNER_TEMP/jq" >> "$GITHUB_PATH"'

# Without node on the job's PATH the gate skips the front page's tests, so CI
# takes the host's node pin the way it takes jq's. The step on its own, from
# its name to the next one, and before the gate runs.
t_case "CI puts the pinned node on the job's PATH before the gate runs"
t_eq "$(grep -cE '^NODE_VERSION=[0-9]+\.[0-9]+\.[0-9]+$' "$T_ROOT/host/versions.env")" 1
t_eq "$(grep -cE '^NODE_SHA256=[0-9a-f]{64}$' "$T_ROOT/host/versions.env")" 1
# The asset bootstrap.sh installs on the host is the one CI downloads.
t_contains "$(cat "$T_ROOT/bootstrap.sh")" 'https://nodejs.org/dist/v$NODE_VERSION/node-v$NODE_VERSION-linux-x64.tar.xz'
# Both workflows that run the gate: a pull request's, and the release gate's.
for workflow in "$T_ROOT/.github/workflows/verify.yml" "$T_ROOT/.github/workflows/release-gate.yml"; do
  node_step=$(awk '/- name: node at the pinned version/{s = 1; next}
    s && /- name:/{exit}
    s' "$workflow")
  t_contains "$node_step" 'https://nodejs.org/dist/v$NODE_VERSION/node-v$NODE_VERSION-linux-x64.tar.xz'
  t_contains "$node_step" 'curl -fsSL --retry 5 --retry-all-errors --retry-delay 5 -o "$RUNNER_TEMP/node.tar.xz"'
  t_contains "$node_step" 'echo "$NODE_SHA256  $RUNNER_TEMP/node.tar.xz" | sha256sum -c -'
  t_contains "$node_step" 'echo "$RUNNER_TEMP/node/bin" >> "$GITHUB_PATH"'
  t_contains "$node_step" 'test "$("$RUNNER_TEMP/node/bin/node" --version)" = "v$NODE_VERSION"'
  t_lacks "$node_step" 'continue-on-error'
  node_at=$(grep -n -- '- name: node at the pinned version' "$workflow" | cut -d : -f 1)
  gate_at=$(grep -n -- '- name: scripts/verify.sh' "$workflow" | cut -d : -f 1)
  t_eq "$([ -n "$node_at" ] && [ -n "$gate_at" ] && [ "$node_at" -lt "$gate_at" ] && echo "before ${workflow##*/}")" "before ${workflow##*/}"
done

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

# --- a tag upstream moved ---------------------------------------------------
# The pi-rukas cases above check what the script asks of git. These check what
# only git decides, on a real remote whose release tag upstream re-pointed after
# the host's clone was made: which fetch gets the clone onto the release.

# A name and a branch of their own, whatever the person running this has set.
# shellcheck disable=SC2329  # the three cases below call it
t_git() {
  git -c user.name=t -c user.email=t@example.com -c init.defaultBranch=main "$@"
}

# Under $T/rukas-moved: a bare remote, its upstream, and the host's clone made
# before upstream moved v0.15.80 to the next commit and tagged v0.15.81 on the
# one after. The clone holds v0.15.80 where upstream no longer has it.
# shellcheck disable=SC2329  # the three cases below call it
rukas_moved() {
  local r=$T/rukas-moved ref
  rm -rf "$r"
  mkdir -p "$r"
  t_git init --quiet --bare "$r/origin.git"
  t_git clone --quiet "$r/origin.git" "$r/up" 2>/dev/null
  printf 'first\n' >"$r/up/one"
  t_git -C "$r/up" add one
  t_git -C "$r/up" commit --quiet -m first
  t_git -C "$r/up" tag -a -m 'Release v0.15.80 - first' v0.15.80
  t_git -C "$r/up" push --quiet origin main v0.15.80
  t_git clone --quiet "$r/origin.git" "$r/host" 2>/dev/null
  t_git -C "$r/up" checkout --quiet -b release
  printf 'second\n' >"$r/up/two"
  t_git -C "$r/up" add two
  t_git -C "$r/up" commit --quiet -m second
  t_git -C "$r/up" tag --force -a -m 'Release v0.15.80 - moved' v0.15.80
  printf 'third\n' >"$r/up/three"
  t_git -C "$r/up" add three
  t_git -C "$r/up" commit --quiet -m third
  t_git -C "$r/up" tag v0.15.81
  t_git -C "$r/up" push --quiet --force origin release
  # The remote answers as upstream does now: its head, and the two tags as
  # upstream has them. The host's clone keeps what it read before the move.
  t_git -C "$r/origin.git" update-ref refs/heads/main "$(t_git -C "$r/up" rev-parse HEAD)"
  for ref in refs/tags/v0.15.80 refs/tags/v0.15.81; do
    t_git -C "$r/origin.git" fetch --quiet --force "$r/up" "+$ref:$ref"
  done
}

t_case "a clone of a remote whose tag moved holds the tag upstream moved"
rukas_moved
# The clone was made from the one commit the remote had then, and still has the
# tag where that left it, with no sign of the release that came after.
t_eq "$(t_git -C "$T/rukas-moved/host" rev-parse HEAD)" "$(t_git -C "$T/rukas-moved/up" rev-parse "HEAD~2^{commit}")"
t_eq "$(t_git -C "$T/rukas-moved/host" rev-parse "v0.15.80^{commit}")" "$(t_git -C "$T/rukas-moved/host" rev-parse HEAD)"
t_eq "$(t_git -C "$T/rukas-moved/up" rev-parse "v0.15.80^{commit}")" "$(t_git -C "$T/rukas-moved/up" rev-parse HEAD~1)"
t_lacks "$(t_git -C "$T/rukas-moved/host" tag --list)" v0.15.81

# The fetch the script used, the same arguments bar --force: git refuses the
# moved tag and exits 1, which is the failure that stopped the run.
t_case "a fetch without --force refuses the moved tag and fails"
rukas_moved
git -C "$T/rukas-moved/host" fetch -q --tags origin >"$T/fetch.out" 2>&1
t_eq "$?" 1
t_lacks "$(cat "$T/fetch.out")" "clobber"
git -C "$T/rukas-moved/host" fetch --tags origin >"$T/fetch.out" 2>&1
t_eq "$?" 1
t_contains "$(cat "$T/fetch.out")" "would clobber existing tag"

# What the script does now, on the same remote and the same clone: the moved tag
# is taken, the newer release is there, and the checkout gets onto it. This is
# the acceptance criterion: a clone whose tag upstream moved updates without a
# person fetching by hand.
t_case "the fetch the script makes takes the moved tag and the release on it"
rukas_moved
newest=$(t_git -C "$T/rukas-moved/up" rev-parse HEAD)
git -C "$T/rukas-moved/host" fetch --tags --force origin >"$T/fetch.out" 2>&1
t_eq "$?" 0
t_eq "$(t_git -C "$T/rukas-moved/host" rev-parse "v0.15.80^{commit}")" "$(t_git -C "$T/rukas-moved/up" rev-parse "v0.15.80^{commit}")"
t_eq "$(t_git -C "$T/rukas-moved/host" rev-parse "v0.15.81^{commit}")" "$newest"
git -C "$T/rukas-moved/host" checkout -q "$newest"
t_eq "$(t_git -C "$T/rukas-moved/host" rev-parse HEAD)" "$newest"

t_done
