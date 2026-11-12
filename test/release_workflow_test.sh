#!/usr/bin/env bash
# Tests for the first step of .github/workflows/release.yml: the tagged commit
# is on main and the account that pushed the tag is an admin of the repository.
# The step's own script runs here against scratch repositories, with curl
# stubbed for the GitHub API. Nothing here reaches the network.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

workflow=$T_ROOT/.github/workflows/release.yml
name='the tag is a commit on main, pushed by an admin of the repository'

t_case "the check is the first step after the checkout, which fetches all of history"
steps=$(grep -E '^      - (name|uses):' "$workflow")
t_eq "$(sed -n 2p <<<"$steps")" "      - name: $name"
t_contains "$(sed -n '/- uses: actions\/checkout/,/- name:/p' "$workflow")" 'fetch-depth: 0'

# The step's run script, as the runner gets it: from `run: |` to the next step,
# with the step's indent taken off.
awk -v n="      - name: $name" '$0 == n {s = 1; next}
  s && /^      - (name|uses):/ {exit}
  s == 2 {print substr($0, 11)}
  s && /^        run: \|$/ {s = 2}' "$workflow" >"$T/guard.sh"
t_contains "$(cat "$T/guard.sh")" 'git merge-base --is-ancestor HEAD refs/remotes/origin/main'

# origin with main and a side branch, and a clone of it as the checkout makes it.
git init -q -b main "$T/origin"
g() { git -C "$1" -c user.name=t -c user.email=t@example.invalid "${@:2}"; }
g "$T/origin" commit -q --allow-empty -m one
g "$T/origin" commit -q --allow-empty -m two
g "$T/origin" switch -q -c side
g "$T/origin" commit -q --allow-empty -m three
g "$T/origin" switch -q main
git clone -q "$T/origin" "$T/checkout"
main=$(git -C "$T/origin" rev-parse main)
side=$(git -C "$T/origin" rev-parse side)

# guard SHA ACTOR: the step on the checkout at SHA, as ACTOR pushed it, with
# the shell options of the workflow's bash. The calls of curl are the run's own.
guard() {
  rm -f "$T/calls/curl"
  git -C "$T/checkout" checkout -q "$1"
  t_run "cd '$T/checkout' && TAG=v1.2.3 ACTOR='$2' GH_TOKEN=x REPO=acme/widget bash -eo pipefail '$T/guard.sh'"
}

t_case "a commit on main pushed by an admin passes"
t_stub curl 'echo "{\"permission\":\"admin\"}"'
guard "$main" alice
t_eq "$t_rc" 0
t_contains "$(t_calls curl)" 'https://api.github.com/repos/acme/widget/collaborators/alice/permission'

t_case "an older commit of main passes"
t_stub curl 'echo "{\"permission\":\"admin\"}"'
guard "$main~1" alice
t_eq "$t_rc" 0

t_case "a commit that is not on main stops the release before the API is asked"
t_stub curl 'echo "{\"permission\":\"admin\"}"'
guard "$side" alice
t_eq "$t_rc" 1
t_contains "$t_out" "v1.2.3 is not on main: $side"
t_eq "$(t_calls curl)" ""

t_case "a tag pushed by a bot stops the release"
t_stub curl 'echo "{\"permission\":\"admin\"}"'
guard "$main" 'tehas-app-bot[bot]'
t_eq "$t_rc" 1
t_contains "$t_out" "not by a person"
t_eq "$(t_calls curl)" ""

t_case "a tag pushed by an account with write access stops the release"
t_stub curl 'echo "{\"permission\":\"write\"}"'
guard "$main" bob
t_eq "$t_rc" 1
t_contains "$t_out" "bob, who is not an admin of acme/widget"

t_case "an API that does not answer stops the release"
t_stub curl 'exit 22'
guard "$main" alice
t_eq "$t_rc" 1
t_contains "$t_out" "who is not an admin"

t_done
