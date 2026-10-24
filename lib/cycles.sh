# shellcheck shell=bash
# Which cycles are running, what a number on GitHub is, and what the sandbox
# image says about itself. Sourced by the dispatcher, by tehasbot cycle, by
# tehas index, by install.sh, and by the two commands that give a cycle its
# issue's images.
#
# tehasbot cycle holds $TEHAS_HOME/state/cycles/<issue>.lock for as long as it
# lives. A lock nobody holds is a cycle that ended, however it ended, so a
# cycle that died is never counted. Whether a cycle runs is the lock's to say,
# here; what a cycle is on record as (its attempt, when it started, its
# approval, its end and history, its park and the park's question) is the
# state store's (lib/store.sh).
#
#   tehas_cycle_running <issue>   0 if that issue's cycle holds its lock
#   tehas_cycle_lock_free <issue> 0 only when the poll can tell the lock is
#                                 free: no lock file, or one it can take. It is
#                                 not 0 when the probe could not tell.
#   tehas_cycles_running          the issues with a running cycle, one per line
#   tehas_rukas_commit            the pi-rukas commit a cycle started now runs
#   tehas_names_issue             print a jq definition, `mine`: whether a pull
#                                 request of gh pr list names the issue $n
#   tehas_branch_pattern <issue>  the one grammar for the branch pi-rukas makes
#                                 for that issue, as an extended regular
#                                 expression: ^feature/issue-<n>...$
#   tehas_rukas_reads_images      whether that pi-rukas reads the issue image
#                                 manifest: 1, 0, or nothing when the image
#                                 does not say
#   tehas_index_state <commit>    the code index for that commit: current when
#                                 the host's index holds a database and its
#                                 record names the commit, failed when the
#                                 indexer last failed at it, due otherwise
#
# Needs TEHAS_HOME. Reads only: it takes each lock for an instant and lets go.
# The two tehas_rukas_ functions ask Docker about PI_ENSEMBLE_IMAGE, once per
# command.

tehas_cycle_running() {
  local lock=$TEHAS_HOME/state/cycles/$1.lock
  [ -e "$lock" ] && ! flock -n "$lock" true
}

tehas_cycle_lock_free() {
  local lock=$TEHAS_HOME/state/cycles/$1.lock
  [ -e "$lock" ] || return 0
  [ -f "$lock" ] && [ ! -L "$lock" ] || return 1
  flock -n "$lock" true
}

tehas_cycles_running() {
  local lock n
  for lock in "$TEHAS_HOME"/state/cycles/*.lock; do
    [ -e "$lock" ] || continue
    n=${lock##*/}
    n=${n%.lock}
    if tehas_cycle_running "$n"; then printf '%s\n' "$n"; fi
  done
  return 0
}

# The files of a cycle's end, and what each one is proof of. tehas work writes
# them and tehasbot cycle reads and removes them, and a cycle that was
# killed outright leaves them for the next cycle of the issue: both commands
# clear them before they start. What each says is not interchangeable, so it is
# written here rather than only where the tests set the files up.
#
#   <issue>.runner_exit   0 when tehas work watched the container to its end,
#                         and the runner's own exit status for the comment.
#                         Its existence, not its content, is the proof: a file
#                         with no line in it still says the run was watched.
#   <issue>.status        the one word `killed`, and only when tehas work saw
#                         the container end with 137. The only account of the
#                         end a cycle has when the runner wrote no result: a
#                         payload read for its word, not a marker of anything.
#   <issue>.progress.json the summary tehas work made of the run's NDJSON. This
#                         is what says the run log of the issue is this cycle's
#                         and may be read for its steps and its branch: tehas
#                         work writes no other file after the container has
#                         ended, so a file no older than the cycle's own start
#                         was written by this cycle. An older one is a cycle
#                         that never reported, and its log is read for nothing.
#   <issue>.nostart       one line: the container's own exit status, plain
#                         digits, 1 to 999, no sign and no leading zero. A
#                         file that holds anything else is no note. Left only
#                         when the container was watched to its end and wrote
#                         neither a result nor one line of its stream, and only
#                         for a container nobody stopped: no step of the driver
#                         was ever seen. The one thing that says a cycle of
#                         this issue started no runner, as opposed to one that
#                         died part way through; tehasbot cycle makes tehas's
#                         cap runner-never-started of it, and reads no run log
#                         for the step of it. It is this cycle's own because
#                         tehas work removes it before the container starts.
#                         The count of memory kills is not in it: that is the
#                         account of <issue>.oom, as for every other end.

# The caps tehas sets for a stop of its own making, and no other command does.
# The one list: libexec/cycle both makes them and says who made them from it, so
# a cap named here is a cap whose comment reads "tehas stopped the cycle", and
# adding one to the list is the whole of adding it.
tehas_own_caps="wall-clock stall runner-never-started"

tehas_cycle_own_cap() { # <cap>: 0 when tehas itself set that cap
  local c
  for c in $tehas_own_caps; do [ "$c" = "$1" ] && return 0; done
  return 1
}

# The factory works on open issues only: a closed issue is never taken, and a
# pull request is never taken, open, merged or closed. What GitHub answers for
# a number decides it, not which labels that number carries.
#
#   tehas_work_kind <the answer for that number>
#     0  the answer is an open issue, which the factory may take
#     1  it is anything else, with the reason in $tehas_work_why: pr (a pull
#        request), closed, or unreadable (no answer, or one that is not the
#        answer for a single issue). A number is never taken on a guess.
tehas_work_why=""
tehas_work_kind() {
  local kind
  kind=$(jq -r 'if type == "object" and ((.pull_request // null) != null) then "pr"
                elif type == "object" and ((.state // "") | ascii_downcase) == "open" then "open"
                elif type == "object" and ((.state // "") | ascii_downcase) == "closed" then "closed"
                else "unreadable" end' <<<"${1:-}" 2>/dev/null) || kind=""
  case "$kind" in
    open) tehas_work_why=open ;;
    pr | closed) tehas_work_why=$kind ;;
    *) tehas_work_why=unreadable; return 1 ;;
  esac
  [ "$tehas_work_why" = open ]
}

# The sandbox image answers two questions about itself, in its labels, and one
# docker call answers both. tehas.rukas is the pi-rukas commit it was built
# with; tehas.rukas.images is whether that pi-rukas reads the issue image
# manifest, and host/sandbox/build.sh writes what it found when it builds.
# Asked once, because a command that asks one of them usually wants the other.
# The answer is kept for the rest of that command: every command that asks here
# asks, acts and exits, so nothing is carried into a later poll or across a
# rollback of the image while a command runs. A label the image does not carry,
# or a value that is not one of the expected ones, reads as nothing.
tehas_rukas_asked=""
tehas_rukas_commit_label=""
tehas_rukas_images_label=""
tehas_rukas_ask() {
  local line commit="" images=""
  if [ -n "$tehas_rukas_asked" ]; then return 0; fi
  tehas_rukas_asked=1
  line=$(docker image inspect --format '{{index .Config.Labels "tehas.rukas"}}|{{index .Config.Labels "tehas.rukas.images"}}' "${PI_ENSEMBLE_IMAGE:-}" 2>/dev/null) || line=""
  IFS='|' read -r commit images <<<"$line" || true
  [[ $commit =~ ^[0-9a-f]{7,40}$ ]] || commit=""
  case "$images" in 1 | 0) ;; *) images="" ;; esac
  tehas_rukas_commit_label=$commit
  tehas_rukas_images_label=$images
}

# The commit, also for an image put back for a rollback. Nothing when the
# image does not say.
tehas_rukas_commit() {
  tehas_rukas_ask
  printf '%s\n' "$tehas_rukas_commit_label"
}

# Whether the pi-rukas in the sandbox image reads the manifest tehas writes
# beside the images it mounts for a cycle. 1 reads them, 0 does not, nothing
# when the image does not say: an image built before the label existed, or not
# built by tehas at all. Whoever is asked whether a cycle saw a picture has to
# answer that it does not know.
tehas_rukas_reads_images() {
  tehas_rukas_ask
  printf '%s\n' "$tehas_rukas_images_label"
}

# The one grammar for a branch pi-rukas makes for an issue: feature/issue-<n>
# with an optional slug of up to 80 characters. tehas_headless_run (lib/
# headless.sh) asks whether a name read out of a cycle's run log is this
# issue's branch, and tehasbot cycle holds the same grammar before it asks
# GitHub about a branch; both take it from here so the two cannot drift.
tehas_branch_pattern() { # <issue>
  local n=$1
  [[ $n =~ ^[1-9][0-9]*$ ]] || return 1
  printf '%s\n' "^feature/issue-$n(-[A-Za-z0-9._-]{1,80})?$"
}

# A pull request names an issue when its head is the issue's branch as pi-rukas
# names it, feature/issue-<n>-..., or its body closes #<n>. tehasbot cycle and
# the dispatcher must agree on which pull request is a cycle's.
tehas_names_issue() {
  # shellcheck disable=SC2016  # a jq program
  printf '%s\n' 'def mine: ((.headRefName // "") | tostring | (. == "feature/issue-\($n)" or startswith("feature/issue-\($n)-")))
  or ((.body // "") | tostring | test("(?i)\\b(close[sd]?|fix(e[sd])?|resolve[sd]?):?\\s+#\($n)([^0-9]|$)"));'
}

tehas_index_state() {
  # The directory is tehas_index_dir of lib/sandbox.sh. Only a database counts
  # as index data, not the indexer's configuration.
  local dir=$TEHAS_HOME/state/index f
  if [ "$(jq -r '.commit // empty' "$TEHAS_HOME/state/index.json" 2>/dev/null)" = "$1" ]; then
    for f in "$dir"/*.db; do
      [ -f "$f" ] && [ "$f" != "$dir/_config.db" ] && { echo current; return 0; }
    done
  fi
  if [ "$(head -c 64 "$TEHAS_HOME/state/index.failed" 2>/dev/null)" = "$1" ]; then echo failed; else echo due; fi
}
