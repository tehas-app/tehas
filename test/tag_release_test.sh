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
