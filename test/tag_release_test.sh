#!/usr/bin/env bash
# Tests for scripts/tag-release.sh and scripts/tag-deploy.sh, against a bare
# repository as the remote and a stub ssh as the host.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

origin=$T/origin.git
work=$T/work
g() { git -c user.name=t -c user.email=t@example.com -c init.defaultBranch=main "$@"; }
commit() { g -C "$T/up" commit --quiet --allow-empty -m "$1" && g -C "$T/up" push --quiet origin main; }
# The tag of the remote moved to another commit, as a person re-making a
# release does: the tag is rewritten and pushed over the one that is there.
# shellcheck disable=SC2329  # the case at the end of this group calls it
moved_tag() {
  g -C "$T/up" tag --force "$1" "$2" &&
    g -C "$T/up" push --quiet --force --atomic origin main "$1"
}
head_of() { git -C "$origin" rev-parse "${1:-main}^{commit}"; }

# The annotated tags the script makes need a name.
printf '[user]\n\tname = t\n\temail = t@example.com\n' >"$T/home/.gitconfig"

# A remote with one commit tagged v0.1.0, a clone that runs the scripts, and a
# second clone that adds commits to the remote. The remote's gate records that
# it ran and exits with the status in $T/verify_rc.
fresh() {
  rm -rf "$origin" "$work" "$T/up" "$T/calls/"* "$T/verify_rc"
  g init --quiet --bare "$origin"
  g clone --quiet "$origin" "$T/up" 2>/dev/null
  mkdir -p "$T/up/scripts"
  printf '#!/usr/bin/env bash\necho ran >>"%s/calls/verify"\nexit "$(cat "%s/verify_rc" 2>/dev/null || echo 0)"\n' "$T" "$T" >"$T/up/scripts/verify.sh"
  chmod +x "$T/up/scripts/verify.sh"
  printf '# Changelog\n\n## Unreleased\n\n### Changes\n\n- first\n\n### Adopter action\n\nNone.\n' >"$T/up/CHANGELOG.md"
  g -C "$T/up" add scripts/verify.sh CHANGELOG.md
  commit first
  g -C "$T/up" tag v0.1.0
  g -C "$T/up" push --quiet origin v0.1.0
  g clone --quiet "$origin" "$work"
  mkdir -p "$work/scripts"
  cp "$T_ROOT/scripts/tag-release.sh" "$T_ROOT/scripts/tag-deploy.sh" "$T_ROOT/scripts/release-notes.sh" "$work/scripts/"
}
tags() { git -C "$origin" tag --list | sort -V | tr '\n' ' '; }

t_case "a head that carries the newest tag is left as it is"
fresh
g -C "$work" fetch --quiet --tags --force origin
t_run "$work/scripts/tag-release.sh"
t_eq "$t_rc" 0
t_eq "$(tail -n 1 <<<"$t_out")" v0.1.0
t_contains "$t_out" "already v0.1.0"
t_eq "$(tags)" "v0.1.0 "

t_case "a new commit is tagged with the next patch and the tag is pushed"
commit second
t_run "$work/scripts/tag-release.sh 2>/dev/null"
t_eq "$t_rc" 0
t_eq "$t_out" v0.1.1
t_eq "$(tags)" "v0.1.0 v0.1.1 "
t_eq "$(head_of v0.1.1)" "$(head_of)"
t_eq "$(git -C "$origin" cat-file -t v0.1.1)" tag
t_eq "$(git -C "$origin" tag -l --format='%(contents:subject)' v0.1.1)" "Release v0.1.1 - second"
t_eq "$(t_calls verify)" ran
t_eq "$(git -C "$work" worktree list | wc -l)" 1

t_case "a failed gate makes no tag"
commit broken
echo 1 >"$T/verify_rc"
t_run "$work/scripts/tag-release.sh"
t_eq "$t_rc" 1
t_contains "$t_out" "the gate failed"
t_eq "$(tags)" "v0.1.0 v0.1.1 "
t_eq "$(git -C "$work" worktree list | wc -l)" 1

t_case "--no-verify skips the gate, --message names the release"
rm -f "$T/calls/verify"
t_run "$work/scripts/tag-release.sh --no-verify --message 'by hand' 2>/dev/null"
rm -f "$T/verify_rc"
t_eq "$t_out" v0.1.2
t_eq "$(t_calls verify)" ""
t_eq "$(git -C "$origin" tag -l --format='%(contents:subject)' v0.1.2)" "Release v0.1.2 - by hand"

t_case "it tags the remote's head, not the checkout's"
commit third
g -C "$work" checkout --quiet -b elsewhere v0.1.0
t_run "$work/scripts/tag-release.sh 2>/dev/null"
t_eq "$t_out" v0.1.3
t_eq "$(head_of v0.1.3)" "$(head_of)"

t_case "--minor and --major reset the parts below them"
commit fourth
t_run "$work/scripts/tag-release.sh --minor 2>/dev/null"
t_eq "$t_out" v0.2.0
commit fifth
t_run "$work/scripts/tag-release.sh --major 2>/dev/null"
t_eq "$t_out" v1.0.0

t_case "--dry-run names the tag and makes none"
commit sixth
t_run "$work/scripts/tag-release.sh --dry-run"
t_eq "$t_rc" 0
t_eq "$(tail -n 1 <<<"$t_out")" v1.0.1
t_contains "$t_out" "would tag main"
t_lacks "$(tags)" v1.0.1
t_eq "$(git -C "$work" tag --list v1.0.1)" ""

t_case "a newest tag that is not on the branch stops it"
fresh
g -C "$T/up" checkout --quiet -b side
g -C "$T/up" commit --quiet --allow-empty -m side
g -C "$T/up" tag v0.9.0
g -C "$T/up" push --quiet origin v0.9.0
g -C "$T/up" checkout --quiet main
commit second
t_run "$work/scripts/tag-release.sh"
t_eq "$t_rc" 1
t_contains "$t_out" "v0.9.0 is not on main"
t_eq "$(tags)" "v0.1.0 v0.9.0 "

t_case "a refused push leaves no local tag"
fresh
commit second
printf '#!/bin/sh\nexit 1\n' >"$origin/hooks/pre-receive"
chmod +x "$origin/hooks/pre-receive"
t_run "$work/scripts/tag-release.sh"
t_eq "$t_rc" 1
t_contains "$t_out" "pushing v0.1.1 to origin failed"
t_eq "$(git -C "$work" tag --list v0.1.1)" ""

# changelog <text>: the remote's CHANGELOG.md becomes <text>, in a new commit.
changelog() {
  printf '%b' "$1" >"$T/up/CHANGELOG.md"
  g -C "$T/up" add CHANGELOG.md
  g -C "$T/up" commit --quiet -m changelog && g -C "$T/up" push --quiet origin main
}

t_case "no Adopter action under Unreleased makes no tag and says what is missing"
fresh
changelog '# Changelog\n\n## Unreleased\n\n### Changes\n\n- a change\n'
t_run "$work/scripts/tag-release.sh --no-verify"
t_eq "$t_rc" 1
t_contains "$t_out" '"### Adopter action" heading'
t_eq "$(tags)" "v0.1.0 "

t_case "an Adopter action heading with nothing under it is not one"
changelog '# Changelog\n\n## Unreleased\n\n### Adopter action\n\n## v0.1.0 (2026-01-01)\n\n### Adopter action\n\nNone.\n'
t_run "$work/scripts/tag-release.sh --no-verify"
t_eq "$t_rc" 1
t_contains "$t_out" "Unreleased section of CHANGELOG.md needs"
t_eq "$(tags)" "v0.1.0 "

t_case "--dry-run checks CHANGELOG.md too"
t_run "$work/scripts/tag-release.sh --dry-run"
t_eq "$t_rc" 1
t_lacks "$t_out" "would tag"

t_case "a changelog with no section for the release makes no tag"
changelog '# Changelog\n\n## v0.1.0 (2026-01-01)\n\n### Adopter action\n\nNone.\n'
t_run "$work/scripts/tag-release.sh --no-verify"
t_eq "$t_rc" 1
t_contains "$t_out" "no section for v0.1.1 or Unreleased"
t_eq "$(tags)" "v0.1.0 "

t_case "a head with no CHANGELOG.md makes no tag"
g -C "$T/up" rm --quiet CHANGELOG.md
commit "no changelog"
t_run "$work/scripts/tag-release.sh --no-verify"
t_eq "$t_rc" 1
t_contains "$t_out" "has no CHANGELOG.md"
t_eq "$(tags)" "v0.1.0 "

t_case "the section already renamed to the release's tag counts, and is what tags"
changelog '# Changelog\n\n## Unreleased\n\n## v0.1.1 (2026-10-21)\n\n### Changes\n\n- a\n\n### Adopter action\n\nSet TEHAS_X.\n'
t_run "$work/scripts/tag-release.sh --no-verify 2>/dev/null"
t_eq "$t_rc" 0
t_eq "$t_out" v0.1.1
t_eq "$(tags)" "v0.1.0 v0.1.1 "

t_case "an unknown argument is refused before git runs"
t_run "$work/scripts/tag-release.sh --bogus"
t_eq "$t_rc" 64

# The tags of the clone that runs the script follow the remote: a tag the
# remote moved is one the script has to see in its own clone, since it reads the
# newest release from there. A fetch that is not forced refuses such a tag and
# exits 1, which used to be the whole of the report.
t_case "the fetch takes a tag the remote moved, and a refusal of it is named"
fresh
# The tag names a commit the clone has never had: an unforced fetch will not
# write it over the tag the clone holds.
printf 'new\n' >"$T/up/new"
g -C "$T/up" add new
g -C "$T/up" commit --quiet -m new
moved=$(g -C "$T/up" rev-parse HEAD)
g -C "$T/up" tag --force v0.1.0 "$moved"
g -C "$T/up" push --quiet --force origin v0.1.0
# Without --force git refuses the moved tag and says why: git exits 1, and what
# the scripts used to have to go on was their own line about the remote.
g -C "$work" fetch --tags origin 2>"$T/fetch.err"
t_eq "$?" 1
t_contains "$(cat "$T/fetch.err")" "would clobber existing tag"
t_eq "$(git -C "$work" rev-parse "v0.1.0^{commit}")" "$(git -C "$T/up" rev-parse HEAD~1)"
# With --force, which is what the scripts ask for now, the clone reads the
# release the remote has: the tag names the commit upstream moved it to, and
# the newest tag the script can see is the remote's, not the stale one.
g -C "$work" fetch --tags --force origin
t_eq "$?" 0
t_eq "$(git -C "$work" rev-parse "v0.1.0^{commit}")" "$moved"
t_eq "$(git -C "$work" tag --list 'v*' --sort=-v:refname | head -n 1)" v0.1.0

# A head that carries a polyfill of someFeature, with what scripts/polyfills.sh
# and the lookup of pi-rukas read. The stub gh says the upstream issue's state
# from $T/issue_state and the upstream pull request's from $T/pr_state (empty:
# not found), the open pull requests from $T/open_prs, and opens #9.
# The stub docker answers --capabilities with $T/caps.json.
mark='# polyfill'
polyfilled() {
  fresh
  mkdir -p "$T/up/lib" "$T/up/test" "$T/up/libexec" "$T/up/host"
  cp "$T_ROOT/scripts/polyfills.sh" "$T/up/scripts/"
  cp "$T_ROOT/lib/rukas.sh" "$T/up/lib/"
  printf 'PI_RUKAS_REPO=https://github.com/up-owner/pi-rukas.git\nPI_RUKAS_FOLLOW=release\n' >"$T/up/host/versions.env"
  printf '# shellcheck shell=bash\n# Upstream issue: up-owner/pi-rukas#77\n# Upstream PR: up-owner/pi-rukas#78\n' \
    >"$T/up/lib/polyfill-someFeature.sh"
  printf '#!/usr/bin/env bash\n' >"$T/up/test/polyfill_someFeature_test.sh"
  printf '#!/usr/bin/env bash\necho native\necho polyfill %s someFeature\n' "$mark" >"$T/up/libexec/run"
  g -C "$T/up" add -A
  commit "carry a polyfill"
  rm -f "$T/open_prs" "$T/calls/gh" "$T/calls/docker" "$T/calls/verify"
  echo OPEN >"$T/issue_state"
  echo OPEN >"$T/pr_state"
  echo '{"features":{}}' >"$T/caps.json"
  t_stub gh 'case "$1 $2" in
  "issue view") cat "$T/issue_state" ;;
  "pr view") cat "$T/pr_state" ;;
  "pr list") cat "$T/open_prs" 2>/dev/null || true ;;
  "pr create") echo https://github.com/acme/widget/pull/9 ;;
esac'
  t_stub docker '[ "$1" != run ] || cat "$T/caps.json"'
}

t_case "a polyfill whose feature pi-rukas still lacks is kept, and the head is tagged"
polyfilled
t_run "$work/scripts/tag-release.sh --rukas-caps '$T/caps.json'"
t_eq "$t_rc" 0
t_eq "$(tail -n 1 <<<"$t_out")" v0.1.1
t_contains "$(t_calls gh)" "issue view 77 -R up-owner/pi-rukas"
t_contains "$(t_calls gh)" "pr view 78 -R up-owner/pi-rukas"
t_lacks "$t_out" "is merged"
t_eq "$(t_calls verify)" ran
t_eq "$(t_calls docker)" ""

t_case "a polyfill whose upstream pull request is merged without the feature released is said, and kept"
polyfilled
echo CLOSED >"$T/issue_state"
echo MERGED >"$T/pr_state"
t_run "$work/scripts/tag-release.sh --rukas-caps '$T/caps.json'"
t_eq "$t_rc" 0
t_contains "$t_out" "up-owner/pi-rukas#78 is merged, but the pi-rukas of $T/caps.json does not set features.someFeature"

t_case "a polyfill whose upstream pull request was closed unmerged, or cannot be found, tags nothing"
for state in CLOSED ""; do
  polyfilled
  echo "$state" >"$T/pr_state"
  t_run "$work/scripts/tag-release.sh --rukas-caps '$T/caps.json'"
  t_eq "$t_rc" 1
  t_contains "$t_out" "stand in for no work upstream is doing; nothing was tagged"
  t_contains "$t_out" "someFeature: its upstream pull request up-owner/pi-rukas#78"
  t_eq "$(tags)" "v0.1.0 "
done
t_contains "$t_out" "cannot be found"

t_case "a polyfill whose upstream issue cannot be found tags nothing"
polyfilled
: >"$T/issue_state"
t_run "$work/scripts/tag-release.sh --rukas-caps '$T/caps.json'"
t_eq "$t_rc" 1
t_contains "$t_out" "someFeature: its upstream issue up-owner/pi-rukas#77 cannot be found"
t_eq "$(tags)" "v0.1.0 "

t_case "an expired polyfill is removed even when its upstream pull request is closed"
polyfilled
echo CLOSED >"$T/pr_state"
echo '{"features":{"someFeature":true}}' >"$T/caps.json"
t_run "$work/scripts/tag-release.sh --rukas-caps '$T/caps.json'"
t_eq "$t_rc" 2
t_contains "$t_out" "pull/9 removes them"

t_case "pi-rukas is asked through the image of the commit versions.env follows"
polyfilled
up_rukas=$T/up-rukas.git
g init --quiet --bare "$up_rukas"
rukas_commit=$(g -C "$T/up" rev-parse HEAD)
g -C "$T/up" push --quiet "$up_rukas" HEAD:refs/heads/main
g -C "$T/up" push --quiet "$up_rukas" HEAD:refs/tags/v0.14.0
git config --file "$T/home/.gitconfig" "url.$up_rukas.insteadOf" https://github.com/up-owner/pi-rukas.git
t_run "$work/scripts/tag-release.sh"
git config --file "$T/home/.gitconfig" --unset "url.$up_rukas.insteadOf"
t_eq "$t_rc" 0
t_eq "$(t_calls docker)" "pull -q ghcr.io/up-owner/pi-rukas:sha-${rukas_commit:0:12}
run --rm --init ghcr.io/up-owner/pi-rukas:sha-${rukas_commit:0:12} /opt/pi-rukas/bin/pi-rukas-headless --capabilities"

t_case "a pi-rukas that cannot be asked tags nothing"
polyfilled
sed -i 's/^PI_RUKAS_FOLLOW=.*/PI_RUKAS_FOLLOW=nowhere/' "$T/up/host/versions.env"
g -C "$T/up" commit --quiet -am "follow nowhere" && g -C "$T/up" push --quiet origin main
t_run "$work/scripts/tag-release.sh"
t_eq "$t_rc" 1
t_contains "$t_out" "whether the polyfills of"
t_contains "$t_out" "--rukas-caps <file>"
t_eq "$(tags)" "v0.1.0 "

t_case "a polyfill pi-rukas no longer needs is removed in a pull request, and nothing is tagged"
polyfilled
echo '{"features":{"someFeature":true}}' >"$T/caps.json"
t_run "$work/scripts/tag-release.sh --rukas-caps '$T/caps.json'"
t_eq "$t_rc" 2
t_contains "$t_out" "https://github.com/acme/widget/pull/9 removes them; merge it, then tag"
t_contains "$t_out" "removed from lib/polyfill-someFeature.sh"
t_eq "$(tags)" "v0.1.0 "
t_eq "$(t_calls verify)" ran
t_contains "$(t_calls gh)" "pr create --base main --head remove-polyfills-v0.1.1"
t_contains "$(t_calls gh)" "features.someFeature, upstream up-owner/pi-rukas#77 and up-owner/pi-rukas#78"
fixed=$(git -C "$origin" ls-tree -r --name-only remove-polyfills-v0.1.1)
t_lacks "$fixed" polyfill-someFeature
t_lacks "$fixed" polyfill_someFeature
t_eq "$(git -C "$origin" show remove-polyfills-v0.1.1:libexec/run)" "#!/usr/bin/env bash
echo native"
t_eq "$(git -C "$origin" log -1 --format=%s remove-polyfills-v0.1.1)" "Remove the polyfills pi-rukas no longer needs"
t_eq "$(git -C "$origin" rev-parse remove-polyfills-v0.1.1^)" "$(head_of)"
t_eq "$(git -C "$work" worktree list | wc -l)" 1
t_eq "$(git -C "$work" branch --list 'remove-polyfills-*')" ""

t_case "an open pull request that removes polyfills holds the tag and nothing is pushed again"
echo https://github.com/acme/widget/pull/9 >"$T/open_prs"
rm -f "$T/calls/gh"
t_run "$work/scripts/tag-release.sh --rukas-caps '$T/caps.json'"
t_eq "$t_rc" 2
t_contains "$t_out" "pull/9 removes polyfills; merge it, then tag"
t_lacks "$(t_calls gh)" "pr create"
t_eq "$(tags)" "v0.1.0 "

t_case "a gate that fails with the polyfills removed pushes nothing, and a branch of that name it did not make is kept"
polyfilled
git -C "$work" branch remove-polyfills-v0.1.1
echo '{"features":{"someFeature":true}}' >"$T/caps.json"
echo 1 >"$T/verify_rc"
t_run "$work/scripts/tag-release.sh --rukas-caps '$T/caps.json'"
rm -f "$T/verify_rc"
t_eq "$t_rc" 1
t_contains "$t_out" "the gate failed with the polyfills removed"
t_eq "$(git -C "$origin" branch --list 'remove-polyfills-*')" ""
t_lacks "$(t_calls gh)" "pr create"
t_contains "$(git -C "$work" branch --list 'remove-polyfills-*')" remove-polyfills-v0.1.1

# The host: the update's status from $T/ssh_rc, its checkout's commit from
# $T/host_head.
host() {
  t_stub ssh 'case "$*" in
  *rev-parse*) cat "'"$T"'/host_head" ;;
  *"test -r"*) [ ! -e "'"$T"'/no_env" ] ;;
  *) exit "$(cat "'"$T"'/ssh_rc" 2>/dev/null || echo 0)" ;;
esac'
  rm -f "$T/ssh_rc" "$T/no_env"
  head_of >"$T/host_head"
}

t_case "deploy refuses a branch head past the newest tag and calls no host"
fresh
host
commit second
t_run "$work/scripts/tag-deploy.sh tehas-tehas"
t_eq "$t_rc" 1
t_contains "$t_out" "past v0.1.0"
t_eq "$(t_calls ssh)" ""

t_case "deploy runs tehas update on the host and checks it is at the tag"
fresh
host
t_run "$work/scripts/tag-deploy.sh tehas-tehas"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas-tehas is at v0.1.0"
t_eq "$(t_calls ssh)" "tehas-tehas test -r ~/tehas/env.sh
tehas-tehas . ~/tehas/env.sh && tehas update
tehas-tehas git -C ~/tehas/platform rev-parse HEAD"

t_case "deploy fails when the host ends up at another commit"
fresh
host
echo 0000000000000000000000000000000000000000 >"$T/host_head"
t_run "$work/scripts/tag-deploy.sh tehas-tehas"
t_eq "$t_rc" 1
t_contains "$t_out" "not v0.1.0"

t_case "deploy passes on a failed update and does not check further"
fresh
host
echo 1 >"$T/ssh_rc"
t_run "$work/scripts/tag-deploy.sh tehas-tehas"
t_eq "$t_rc" 1
t_contains "$t_out" "exited 1"
t_eq "$(t_calls ssh | wc -l)" 2

t_case "deploy --check runs the update's check and nothing else"
fresh
host
t_run "$work/scripts/tag-deploy.sh --check tehas-tehas"
t_eq "$t_rc" 0
t_eq "$(t_calls ssh)" "tehas-tehas test -r ~/tehas/env.sh
tehas-tehas . ~/tehas/env.sh && tehas update --check"

t_case "deploy uses the ssh command TEHAS_SSH names"
fresh
host
T_ENV=(TEHAS_SSH="ssh -o BatchMode=yes")
t_run "$work/scripts/tag-deploy.sh --check tehas-tehas"
T_ENV=()
t_eq "$(t_calls ssh | tail -n 1)" "-o BatchMode=yes tehas-tehas . ~/tehas/env.sh && tehas update --check"

t_case "deploy stops with a clear line when the ssh user has no tehas, and updates nothing"
fresh
host
: >"$T/no_env"
t_run "$work/scripts/tag-deploy.sh tehas-tehas"
t_eq "$t_rc" 1
t_contains "$t_out" "tehas-tehas has no ~/tehas/env.sh for the ssh user"
t_eq "$(t_calls ssh)" "tehas-tehas test -r ~/tehas/env.sh"

t_case "deploy needs one destination"
t_run "$work/scripts/tag-deploy.sh"
t_eq "$t_rc" 64
t_run "$work/scripts/tag-deploy.sh a b"
t_eq "$t_rc" 64

t_done
