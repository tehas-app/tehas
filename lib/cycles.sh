# shellcheck shell=bash
# Which cycles are running, where a parked cycle's question is on record, what
# a number on GitHub is, and what the sandbox image says about itself. Sourced
# by the dispatcher, by tehasbot cycle, by tehas index, by install.sh, and by
# the two commands that give a cycle its issue's images.
#
# tehasbot cycle holds $TEHAS_PROJECT_HOME/state/cycles/<issue>.lock for as long as it
# lives. A lock nobody holds is a cycle that ended, however it ended, so a
# cycle that died is never counted.
#
#   tehas_cycle_running <issue>   0 if that issue's cycle holds its lock
#   tehas_cycle_lock_free <issue> 0 only when the poll can tell the lock is
#                                 free: no lock file, or one it can take. It is
#                                 not 0 when the probe could not tell.
#   tehas_cycles_running          the issues with a running cycle, one per line
#   tehas_cycle_note_files <ext>  the files state/cycles/<issue>.<ext>, one per
#                                 line, that name an issue and are plain files
#                                 and no link: the per-issue leftovers a command
#                                 may read or remove. A stray name, a directory
#                                 or a link is no issue's file and is not
#                                 listed, so nothing acts on it.
#   park_id <issue>               the path of the file that holds the id of
#                                 the park question this host posted
#   tehas_rukas_commit            the pi-rukas commit a cycle started now runs
#   tehas_names_issue             print a jq definition, `mine`: whether a pull
#                                 request of gh pr list names the issue $n
#   tehas_rukas_reads_images      whether that pi-rukas reads the issue image
#                                 manifest: 1, 0, or nothing when the image
#                                 does not say
#   tehas_index_state <commit>    the code index for that commit: current when
#                                 the host's index holds a database and its
#                                 record names the commit, failed when the
#                                 indexer last failed at it, due otherwise
#
# Needs TEHAS_PROJECT_HOME. Reads only: it takes each lock for an instant and lets go.
# The two tehas_rukas_ functions ask Docker about PI_ENSEMBLE_IMAGE, once per
# command.

tehas_cycle_running() {
  local lock=$TEHAS_PROJECT_HOME/state/cycles/$1.lock
  [ -e "$lock" ] && ! flock -n "$lock" true
}

tehas_cycle_lock_free() {
  local lock=$TEHAS_PROJECT_HOME/state/cycles/$1.lock
  [ -e "$lock" ] || return 0
  [ -f "$lock" ] && [ ! -L "$lock" ] || return 1
  flock -n "$lock" true
}

tehas_cycles_running() {
  local lock n
  for lock in "$TEHAS_PROJECT_HOME"/state/cycles/*.lock; do
    [ -e "$lock" ] || continue
    n=${lock##*/}
    n=${n%.lock}
    if tehas_cycle_running "$n"; then printf '%s\n' "$n"; fi
  done
  return 0
}

tehas_cycle_note_files() {
  local f n
  for f in "$TEHAS_PROJECT_HOME"/state/cycles/*."$1"; do
    [ -e "$f" ] || continue
    n=${f##*/}
    n=${n%."$1"}
    [[ $n =~ ^[1-9][0-9]*$ ]] && [ -f "$f" ] && [ ! -L "$f" ] || continue
    printf '%s\n' "$f"
  done
  return 0
}

# An agent in a container holds the bot's token: it can post a comment with a
# marker, edit the bot's comments and apply labels. It cannot write here, so
# this file is what makes a comment the factory's own question. One line, the
# comment's number. tehasbot cycle writes it, the dispatcher reads it.
park_id() { printf '%s/state/park-%s.id\n' "$TEHAS_PROJECT_HOME" "$1"; }

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
