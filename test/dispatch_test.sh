#!/usr/bin/env bash
# Tests for bin/tehas-dispatch against a stub gh and a scratch checkout whose
# default branch is main.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

dispatch="$T_ROOT/bin/tehas-dispatch"
fx=$T/fx
mkdir -p "$fx"
checkout=$T/home/tehas/repos/widget

# gh answers from fixture files; every call is recorded by t_stub.
t_stub gh '
case "$1 $2" in
  "repo view") cat "$T/fx/repo.json" || exit 1 ;;
  "issue view") cat "$T/fx/control.json" || exit 1 ;;
  "issue list")
    case "$*" in
      *awaiting-merge*) cat "$T/fx/awaiting.json" || exit 1 ;;
      *tehas:ready*) cat "$T/fx/ready.json" || exit 1 ;;
      *) echo "[]" ;;
    esac ;;
  "api repos/acme/widget/issues/"*)
    n=${2#repos/acme/widget/issues/}
    cat "$T/fx/events-${n%/events}.json" || exit 1 ;;
  "pr list") cat "$T/fx/pr.txt" 2>/dev/null ;;
esac'
t_stub fake-work
t_stub fake-status

reset() {
  rm -f "$T"/calls/*
  echo '{"nameWithOwner":"acme/widget"}' >"$fx/repo.json"
  echo '{"labels":[]}' >"$fx/control.json"
  echo '[]' >"$fx/awaiting.json"
  echo '[{"number":7,"labels":[{"name":"tehas:ready"}]}]' >"$fx/ready.json"
  echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}}]' >"$fx/events-7.json"
  T_ENV=(TEHAS_WORK_CMD=fake-work TEHAS_STATUS_CMD=fake-status)
  t_instance "${T_MINIMAL[@]}"
}

# A product repository whose default branch is main, and the instance's clone.
upstream=$T/git/acme/widget
mkdir -p "$upstream"
git init -q -b main "$upstream"
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m first
git clone -q "$upstream" "$checkout"

t_case "a dry run picks the issue an allowed account labelled"
reset
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "would run a cycle for #7 (labelled by alice)"
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(t_calls fake-work)" ""

t_case "an issue labelled by another account is skipped"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: tehas:ready was applied by 'mallory', not an allowed account"
t_contains "$t_out" "nothing to do"

t_case "the most recent labelling decides, not an earlier one"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}},
       {"event":"unlabeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}},
       {"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7"

t_case "a login that only contains an allowed one is not allowed"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice2"}}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7"

t_case "a label event with no actor is not an allowed account"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":null}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: tehas:ready was applied by 'unknown'"
t_contains "$t_out" "nothing to do"

t_case "an issue with no label event at all is skipped"
reset
echo '[]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7"
t_contains "$t_out" "nothing to do"

for odd in "alice  bob" "alice " " alice" " "; do
  t_case "a labeller list with stray spaces ('$odd') is refused, not guessed at"
  reset
  printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ALLOWED_LABELERS='$odd'" >"$TEHAS_INSTANCE_DIR/instance.env"
  echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":null}]' >"$fx/events-7.json"
  t_run "$dispatch --dry-run"
  t_eq "$t_rc" 78
  t_lacks "$t_out" "would run a cycle"
  t_eq "$(t_calls gh)" ""
done

t_case "when the pause check cannot reach GitHub nothing is dispatched"
reset
rm "$fx/control.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: gh issue view failed"
t_eq "$(t_calls fake-work)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "when the awaiting-merge count cannot be read nothing is dispatched"
reset
rm "$fx/awaiting.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: gh issue list failed"
t_eq "$(t_calls fake-work)" ""

t_case "when the label events cannot be read the issue is not taken"
reset
rm "$fx/events-7.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-work)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "a checkout of another repository stops everything"
reset
git -C "$checkout" remote set-url origin "$T/git/acme/other"
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "is a checkout of"
t_contains "$t_out" "not of acme/widget"
t_eq "$(t_calls gh)" ""
git -C "$checkout" remote set-url origin "$upstream"

for url in "https://github.com/acme/widget.git" "https://github.com/acme/widget" "git@github.com:acme/widget.git" "ssh://git@github.com/acme/widget/"; do
  t_case "origin $url is recognised as acme/widget"
  reset
  git -C "$checkout" remote set-url origin "$url"
  t_run "$dispatch --dry-run"
  t_eq "$t_rc" 0
  t_contains "$t_out" "would run a cycle for #7"
done
t_case "origin ending in a longer name is not acme/widget"
reset
git -C "$checkout" remote set-url origin "https://github.com/notacme/widget.git"
t_run "$dispatch --dry-run"
t_eq "$t_rc" 78
git -C "$checkout" remote set-url origin "$upstream"

t_case "a missing checkout stops before any label changes, even with TEHAS_BRANCH set"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_BRANCH=main TEHAS_WORK_REPO="$T/nowhere"
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "no product checkout at $T/nowhere"
t_eq "$(t_calls gh)" ""
t_eq "$([ -e "$T/nowhere" ] && echo created)" ""

t_case "a renamed repository stops the poll and says its new name"
reset
echo '{"nameWithOwner":"acme/gadget"}' >"$fx/repo.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: acme/widget is now acme/gadget on GitHub: set TEHAS_REPO"
t_eq "$(t_calls fake-work)" ""
t_lacks "$(t_calls gh)" "issue list"

t_case "a repository that cannot be looked up stops the poll"
reset
rm "$fx/repo.json"
t_run "$dispatch --dry-run"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: gh repo view failed"
t_lacks "$t_out" "nothing to do"

t_case "the repository's name is compared without regard to case"
reset
echo '{"nameWithOwner":"Acme/Widget"}' >"$fx/repo.json"
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "would run a cycle for #7"

t_case "the pause label on the control issue stops dispatch"
reset
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "paused: issue #1 carries tehas:pause"

t_case "an invalid instance makes no call at all and exits 78"
reset
rm -rf "$T/home/tehas/state" "$T/home/tehas/logs"
t_instance TEHAS_CONTROL_ISSUE=1 TEHAS_ALLOWED_LABELERS=alice
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "tehas: TEHAS_REPO is not set"
t_eq "$(t_calls gh)" ""
t_eq "$(t_calls fake-work)" ""
t_eq "$(find "$T/home/tehas" -maxdepth 1 \( -name state -o -name logs \) | wc -l)" 0

t_case "an empty labeller list does not widen who may label"
reset
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ALLOWED_LABELERS=''" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$dispatch --dry-run"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)" ""

t_case "a real poll puts the checkout on main and fast-forwards it"
reset
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m second
git -C "$checkout" checkout -q -b feature/left-over
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(git -C "$checkout" branch --show-current)" "main"
t_eq "$(git -C "$checkout" rev-parse HEAD)" "$(git -C "$upstream" rev-parse main)"
t_lacks "$t_out" "WARN"
t_eq "$(t_calls fake-work)" "7"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready"

t_case "a cycle that ends without a pull request is handed to a human"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"

t_case "a cycle that ends with a pull request waits for a human and is reported on it"
reset
mkdir -p "$checkout/.pi/work-state"
echo '{"pipelineState":{"status":"handoff","currentStep":"commit-pr","branchName":"feature/x"}}' >"$checkout/.pi/work-state/7.json"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label tehas:awaiting-merge"
t_contains "$(t_calls gh)" "pr comment https://github.com/acme/widget/pull/9"
rm -rf "$checkout/.pi" "$fx/pr.txt"

t_case "TEHAS_BRANCH overrides the checkout's default branch"
reset
git -C "$checkout" branch -q release main
git -C "$upstream" branch -q release main
git -C "$checkout" fetch -q origin
t_instance "${T_MINIMAL[@]}" TEHAS_BRANCH=release
t_run "$dispatch"
t_eq "$(git -C "$checkout" branch --show-current)" "release"

t_case "a checkout without origin/HEAD stops before any label changes"
reset
git -C "$checkout" remote set-head origin -d
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "default branch"
t_lacks "$(t_calls gh)" "issue edit"
git -C "$checkout" remote set-head origin main

t_case "--init-labels needs only the repository"
reset
t_instance TEHAS_REPO=acme/widget
t_run "$dispatch --init-labels"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "label create tehas:ready --repo acme/widget"

# Arguments. A poll starts only from an argument this command knows, and the
# check runs before the instance settings are read.
t_case "--help prints the usage of the command and calls nothing"
reset
t_run "$dispatch --help 2>/dev/null"
t_eq "$t_rc" 0
t_contains "$t_out" "Usage:"
t_contains "$t_out" "tehas-dispatch --dry-run"
t_contains "$t_out" "tehas-dispatch --init-labels"
t_lacks "$t_out" "set -uo pipefail"
t_eq "$(t_calls gh)" ""
t_eq "$(t_calls fake-work)" ""

t_case "-h is the same as --help"
reset
t_run "$dispatch -h 2>/dev/null"
t_eq "$t_rc" 0
t_contains "$t_out" "Usage:"
t_eq "$(t_calls gh)" ""

t_case "--help writes to standard output and nothing to standard error"
reset
t_run "$dispatch --help 1>/dev/null"
t_eq "$t_rc" 0
t_eq "$t_out" ""

t_case "--help answers on a host with no instance settings yet"
reset
rm -rf "$T/home/tehas/state" "$T/home/tehas/logs"
t_instance
t_run "$dispatch --help"
t_eq "$t_rc" 0
t_contains "$t_out" "Usage:"
t_lacks "$t_out" "TEHAS_REPO is not set"
t_eq "$(t_calls gh)" ""
t_eq "$(find "$T/home/tehas" -maxdepth 1 \( -name state -o -name logs \) | wc -l)" 0

t_case "a mistyped argument starts no poll and creates nothing"
reset
rm -rf "$T/home/tehas/state" "$T/home/tehas/logs"
t_run "$dispatch --dryrun"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas-dispatch: unknown argument: --dryrun"
t_contains "$t_out" "Usage:"
t_lacks "$t_out" "would run a cycle"
t_eq "$(t_calls gh)" ""
t_eq "$(t_calls fake-work)" ""
t_eq "$(find "$T/home/tehas" -maxdepth 1 \( -name state -o -name logs \) | wc -l)" 0

t_case "an unknown argument is reported on standard error, never as a poll"
reset
t_run "$dispatch --dryrun 2>/dev/null"
t_eq "$t_rc" 64
t_lacks "$t_out" "unknown argument"
t_run "$dispatch --dryrun 1>/dev/null"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas-dispatch: unknown argument: --dryrun"
t_contains "$t_out" "Usage:"

t_case "an unknown argument is refused before the instance settings are read"
reset
rm -rf "$T/home/tehas/state" "$T/home/tehas/logs"
t_instance TEHAS_CONTROL_ISSUE=1 TEHAS_ALLOWED_LABELERS=alice
t_run "$dispatch --dryrun"
t_eq "$t_rc" 64
t_contains "$t_out" "unknown argument: --dryrun"
t_lacks "$t_out" "TEHAS_REPO is not set"
t_eq "$(t_calls gh)" ""

t_case "an unknown option is refused too, whatever it looks like"
reset
for arg in "-x" "--dry-run=1" "7" "--"; do
  t_run "$dispatch '$arg'"
  t_eq "$t_rc" 64
  t_contains "$t_out" "tehas-dispatch: unknown argument: $arg"
done
t_eq "$(t_calls gh)" ""

t_case "an empty argument is an argument, not a poll"
reset
t_run "$dispatch ''"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas-dispatch: unknown argument:"
t_eq "$(t_calls gh)" ""
t_eq "$(t_calls fake-work)" ""

t_case "two arguments are refused, not combined"
reset
rm -rf "$T/home/tehas/state" "$T/home/tehas/logs"
t_run "$dispatch --dry-run --init-labels"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas-dispatch: unknown argument: --init-labels"
t_lacks "$t_out" "label create"
t_eq "$(t_calls gh)" ""
t_eq "$(find "$T/home/tehas" -maxdepth 1 \( -name state -o -name logs \) | wc -l)" 0

t_case "the same argument twice is refused too"
reset
t_run "$dispatch --dry-run --dry-run"
t_eq "$t_rc" 64
t_contains "$t_out" "unknown argument: --dry-run"
t_eq "$(t_calls gh)" ""

t_case "--help with another argument is still an unknown invocation"
reset
t_run "$dispatch --help --dry-run"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas-dispatch: unknown argument: --dry-run"
t_eq "$(t_calls gh)" ""

t_case "no argument and --dry-run still behave as before"
reset
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "would run a cycle for #7"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls fake-work)" "7"

t_done
