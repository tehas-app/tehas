#!/usr/bin/env bash
# Tests for the runner seam of tehas-cycle cycle: --runner job names a --work
# program that waits on a job run elsewhere. The report reads the cycle's notes,
# run log and exit status as it does for tehas work, and nothing of a workspace
# or an image on this host.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/cycle_lib.sh
. "$(dirname "$0")/cycle_lib.sh" || exit 1

# tehas-cycle cycle as the console execs it, with the settings a cycle reads
# given by hand. fake-work stands for the job waiter: it writes the notes and
# exits as the runner did, and makes no workspace.
supervisor="$T/home/tehas/libexec/tehas-cycle"
given="TEHAS_REPO=acme/widget TEHAS_WORK_DIR=$T/home/tehas/work/widget TEHAS_CYCLE_RETRY_WAIT=0"
words="--work fake-work --status-cmd fake-status --console fake-console"
# A workspace of an earlier in-house cycle of the issue, marked, with its
# images, and an image on record that docker would date.
planted() {
  mkdir -p "$workdir" "$workdir.images"
  : >"$T/home/tehas/work/.tehas-workspaces"
  echo 2026-10-29T14:44:16Z >"$fx/image.label"
  image_id=sha256:$(printf 'e%.0s' {1..64})
  echo '"$T/store-bin" cycle-image 7 --attempt "$TEHAS_CYCLE_ATTEMPT" --image '"$image_id"' >/dev/null' >"$fx/work.sh"
}

t_case "a job runner with outsource off is 78 before the lock, the store, GitHub or the waiter"
for off in "" "TEHAS_OUTSOURCE=0"; do
  reset
  t_run "$given $off $supervisor cycle 7 --runner job $words"
  t_eq "$t_rc" 78
  t_contains "$t_out" "--runner job needs TEHAS_OUTSOURCE=1"
  t_eq "$(t_calls gh)$(t_calls fake-work)$(t_calls docker)" ""
  t_eq "$(find "$state" -name '7.*' 2>/dev/null | wc -l)" 0
  t_eq "$(t_row cycle-show 7 -- '.cycle | tostring')" null
done

t_case "a runner that is neither work nor job is 64 and runs nothing"
reset
t_run "$given TEHAS_OUTSOURCE=1 $supervisor cycle 7 --runner remote $words"
t_eq "$t_rc" 64
t_eq "$(t_calls gh)$(t_calls fake-work)" ""

t_case "a job cycle is reported from its notes, and no workspace or image on this host is read or removed"
# Its pull request was merged while the runner was stopped: the comment says
# where the workspace is.
reset
planted
ended running handoff feature/x
merged_pr
echo 'exit 2' >>"$fx/work.sh"
t_run "$given TEHAS_OUTSOURCE=1 $supervisor cycle 7 --runner job $words"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-work)" "7"
t_eq "$(t_calls docker)" ""
t_eq "$(find "$workdir" "$workdir.images" -maxdepth 0 | wc -l)" 2
t_eq "$(t_calls gh | grep '^issue edit')" "issue edit 7 --repo acme/widget --remove-label tehas:running"
t_contains "$(t_calls gh)" "This cycle left no workspace on this host"
t_lacks "$(t_calls gh)" "The workspace is kept"
t_lacks "$t_out" "WARN"
t_eq "$(t_row history-show 7 -- '.rows[0] | [.status, .step] | @csv')" '"merged","handoff"'
t_eq "$(t_row cycle-show 7 -- '.cycle.ended_at != null')" true

t_case "the same cycle from tehas work reads the image and clears the workspace, as before"
reset
planted
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$given $supervisor cycle 7 --runner work $words"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "image inspect"
t_eq "$(find "$workdir.images" -maxdepth 0 2>/dev/null | wc -l)" 0
t_eq "$(t_row history-show 7 -- '.rows[0] | [.rc, .status, .step] | @csv')" '10,"handoff","commit-pr"'

t_done
