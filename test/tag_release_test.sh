#!/usr/bin/env bash
# Tests for scripts/tag-release.sh and scripts/tag-deploy.sh, against a bare
# repository as the remote and a stub ssh as the host: the scripts' glue, the
# release gate's runs on the speed lane (gh, a written floor) and the deploy.
# The polyfills, the pinned tools and the changelog fragments, which ask the
# code host, are Rust tests on the ports' fake in console/xtask/src/tag/tests.rs.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

origin=$T/origin.git
work=$T/work
g() { git -c user.name=t -c user.email=t@example.com -c init.defaultBranch=main "$@"; }
commit() { g -C "$T/up" commit --quiet --allow-empty -m "$1" && g -C "$T/up" push --quiet origin main; }
head_of() { git -C "$origin" rev-parse "${1:-main}^{commit}"; }

# The script execs this checkout's xtask, built once and named in TEHAS_XTASK.
t_xtask || t_bad "no xtask: $t_xtask_why"

printf '[user]\n\tname = t\n\temail = t@example.com\n' >"$T/home/.gitconfig"

# A remote tagged v0.1.0, a clone that runs the scripts, and a clone that adds
# commits. The remote's gate records that it ran and exits $T/verify_rc. Its
# CHANGELOG.md has a section for each release the cases tag; one still
# Unreleased gets a pull request first (the cases after `named` below).
fresh() {
  rm -rf "$origin" "$work" "$T/up" "$T/calls/"* "$T/verify_rc" "$T/verify_hook"
  g init --quiet --bare "$origin"
  g clone --quiet "$origin" "$T/up" 2>/dev/null
  mkdir -p "$T/up/scripts"
  printf '#!/usr/bin/env bash\necho ran >>"%s/calls/verify"\n[ ! -f "%s/verify_hook" ] || . "%s/verify_hook"\nexit "$(cat "%s/verify_rc" 2>/dev/null || echo 0)"\n' "$T" "$T" "$T" "$T" >"$T/up/scripts/verify.sh"
  chmod +x "$T/up/scripts/verify.sh"
  {
    printf '# Changelog\n\n## Unreleased\n\n### Changes\n\n- first\n\n### Adopter action\n\nNone.\n'
    for v in v1.0.1 v1.0.0 v0.2.0 v0.1.3 v0.1.2 v0.1.1; do
      printf '\n## %s (2026-01-01)\n\n### Changes\n\n- first\n\n### Adopter action\n\nNone.\n' "$v"
    done
  } >"$T/up/CHANGELOG.md"
  g -C "$T/up" add scripts/verify.sh CHANGELOG.md
  commit first
  g -C "$T/up" tag v0.1.0
  g -C "$T/up" push --quiet origin v0.1.0
  g clone --quiet "$origin" "$work"
  mkdir -p "$work/scripts"
  cp "$T_ROOT/scripts/tag-release.sh" "$T_ROOT/scripts/tag-deploy.sh" "$work/scripts/"
}
tags() { git -C "$origin" tag --list | sort -V | tr '\n' ' '; }

t_case "a head that carries the newest tag is left as it is"
fresh
g -C "$work" fetch --quiet --tags --force origin
t_run "$work/scripts/tag-release.sh --local-gate"
t_eq "$t_rc" 0
t_eq "$(tail -n 1 <<<"$t_out")" v0.1.0
t_contains "$t_out" "already v0.1.0"
t_eq "$(tags)" "v0.1.0 "

t_case "a new commit is tagged with the next patch and the tag is pushed"
commit second
t_run "$work/scripts/tag-release.sh --local-gate 2>/dev/null"
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
t_run "$work/scripts/tag-release.sh --local-gate"
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
t_run "$work/scripts/tag-release.sh --local-gate 2>/dev/null"
t_eq "$t_out" v0.1.3
t_eq "$(head_of v0.1.3)" "$(head_of)"

t_case "--minor and --major reset the parts below them"
commit fourth
t_run "$work/scripts/tag-release.sh --local-gate --minor 2>/dev/null"
t_eq "$t_out" v0.2.0
commit fifth
t_run "$work/scripts/tag-release.sh --local-gate --major 2>/dev/null"
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
t_run "$work/scripts/tag-release.sh --local-gate"
t_eq "$t_rc" 1
t_contains "$t_out" "v0.9.0 is not on main"
t_eq "$(tags)" "v0.1.0 v0.9.0 "

t_case "a refused push leaves no local tag"
fresh
commit second
printf '#!/bin/sh\nexit 1\n' >"$origin/hooks/pre-receive"
chmod +x "$origin/hooks/pre-receive"
t_run "$work/scripts/tag-release.sh --local-gate"
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

t_case "an Adopter action over no changes makes no tag and names the empty section"
changelog '# Changelog\n\n## Unreleased\n\n### Adopter action\n\nNone.\n\n## v0.1.0 (2026-01-01)\n\n### Changes\n\n- a\n\n### Adopter action\n\nNone.\n'
t_run "$work/scripts/tag-release.sh --no-verify"
t_eq "$t_rc" 1
t_contains "$t_out" "Unreleased section of CHANGELOG.md lists no changes"
t_eq "$(tags)" "v0.1.0 "

t_case "a Changes heading with nothing under it lists no changes"
changelog '# Changelog\n\n## Unreleased\n\n### Changes\n\n### Adopter action\n\nNone.\n'
t_run "$work/scripts/tag-release.sh --dry-run"
t_eq "$t_rc" 1
t_contains "$t_out" '"### Changes" heading with a line under it'
t_lacks "$t_out" "would tag"

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

# The release gate on the speed lane. The stub gh lists the runs of
# release-gate.yml from $T/runs.json; when $T/runs.next is there, the list
# after it is that file's (once a run planned in $T/runs.started is
# started), as a run moving on. `workflow run` puts
# $T/runs.started in place, as the run it starts. TEHAS_GATE_POLL=0: the
# looks follow each other at once, and the bound is what --gate-wait says.
# gate_run <id> <commit> <status> <conclusion>: one run, as gh gives it.
gate_run() {
  printf '{"databaseId":%s,"displayTitle":"release-gate %s","status":"%s","conclusion":"%s","url":"https://github.com/acme/widget/actions/runs/%s"}' "$1" "$2" "$3" "$4" "$1"
}
lane() {
  fresh
  commit second
  rm -f "$T/runs.json" "$T/runs.next" "$T/runs.started" "$T/calls/gh" "$T/calls/verify"
  echo '[]' >"$T/runs.json"
  t_stub gh 'case "$1 $2" in
  "run list")
    cat "$T/runs.json"
    if [ -e "$T/runs.next" ] && [ ! -e "$T/runs.started" ]; then mv "$T/runs.next" "$T/runs.json"; fi ;;
  "workflow run") [ ! -e "$T/runs.started" ] || mv "$T/runs.started" "$T/runs.json" ;;
esac'
  T_ENV=(TEHAS_GATE_POLL=0)
}

t_case "a green release gate on the commit tags it with no local gate and starts none"
lane
c=$(head_of)
echo "[$(gate_run 5 "$c" completed success)]" >"$T/runs.json"
t_run "$work/scripts/tag-release.sh --gate-wait 1"
t_eq "$t_rc" 0
t_eq "$(tail -n 1 <<<"$t_out")" v0.1.1
t_contains "$t_out" "the release gate passed on ${c:0:7}, https://github.com/acme/widget/actions/runs/5; no local gate"
t_eq "$(t_calls verify)" ""
t_contains "$(t_calls gh)" "run list --workflow release-gate.yml --event workflow_dispatch"
t_lacks "$(t_calls gh)" "workflow run"
t_eq "$(tags)" "v0.1.0 v0.1.1 "

t_case "a green gate on another commit does not count: one is started on this commit, and its pass tags"
lane
c=$(head_of)
other=$(head_of v0.1.0)
echo "[$(gate_run 5 "$other" completed success),$(gate_run 6 "$c" completed failure)]" >"$T/runs.json"
echo "[$(gate_run 7 "$c" in_progress '')]" >"$T/runs.started"
echo "[$(gate_run 7 "$c" completed success)]" >"$T/runs.next"
t_run "$work/scripts/tag-release.sh --gate-wait 1"
t_eq "$t_rc" 0
t_eq "$(tail -n 1 <<<"$t_out")" v0.1.1
t_contains "$(t_calls gh)" "workflow run release-gate.yml --ref main -f commit=$c -f proofs=true"
t_contains "$t_out" "waiting for the release gate: https://github.com/acme/widget/actions/runs/7"
t_contains "$t_out" "actions/runs/7; no local gate"
t_eq "$(t_calls verify)" ""

t_case "a release gate that fails tags nothing"
lane
c=$(head_of)
echo "[$(gate_run 7 "$c" completed failure)]" >"$T/runs.started"
t_run "$work/scripts/tag-release.sh --gate-wait 1"
t_eq "$t_rc" 1
t_contains "$t_out" "the release gate failed on ${c:0:7}, https://github.com/acme/widget/actions/runs/7; nothing was tagged"
t_eq "$(grep -c 'workflow run' "$T/calls/gh")" 1
t_eq "$(tags)" "v0.1.0 "

t_case "a release gate already running is waited for, and none is started"
lane
c=$(head_of)
echo "[$(gate_run 8 "$c" queued '')]" >"$T/runs.json"
echo "[$(gate_run 8 "$c" completed success)]" >"$T/runs.next"
t_run "$work/scripts/tag-release.sh --gate-wait 1"
t_eq "$t_rc" 0
t_eq "$(tail -n 1 <<<"$t_out")" v0.1.1
t_lacks "$(t_calls gh)" "workflow run"

t_case "no green release gate within the bound tags nothing and names the local fallback"
lane
t_run "$work/scripts/tag-release.sh --gate-wait 0"
t_eq "$t_rc" 1
t_contains "$t_out" "within 0 minutes; nothing was tagged"
t_contains "$t_out" "--local-gate runs the gate here instead"
t_eq "$(tags)" "v0.1.0 "

t_case "a gh that cannot start the release gate tags nothing"
lane
t_stub gh '[ "$1 $2" != "workflow run" ] || exit 1; echo "[]"'
t_run "$work/scripts/tag-release.sh --gate-wait 1"
t_eq "$t_rc" 1
t_contains "$t_out" "cannot start release-gate.yml"
t_eq "$(tags)" "v0.1.0 "

t_case "--local-gate runs the gate here and asks no release gate; --no-verify asks neither"
lane
t_run "$work/scripts/tag-release.sh --local-gate 2>/dev/null"
t_eq "$t_out" v0.1.1
t_eq "$(t_calls verify)" ran
t_eq "$(t_calls gh)" ""
commit third
t_run "$work/scripts/tag-release.sh --no-verify 2>/dev/null"
t_eq "$t_out" v0.1.2
t_eq "$(t_calls verify)" ran
t_eq "$(t_calls gh)" ""
T_ENV=()

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
