# shellcheck shell=bash
# Files attached to an issue, for tehas work and tehas plan: the way
# to the program $TEHAS_HOME/libexec/attach (console/attach). It holds which
# links count, who may attach, how much an issue is given and what a file is,
# and it fetches. Here are the settings it is passed and what a caller reads.
# The approved word stays with the callers: it is who applied the label (rule 5).
#
#   tehas_issue_images <issue> <dir> [approved] [record] [issue]
#                                     remake <dir> from the issue's text, with
#                                     a manifest.json; uses TEHAS_REPO and
#                                     GH_TOKEN. Returns 1, with the reason in
#                                     $image_why, when the issue's images
#                                     cannot be given and nothing may start.
#                                     "approved" says an allowed account
#                                     approved the text with its label: the
#                                     images are then taken whoever opened the
#                                     issue. Without it, only when an owner,
#                                     member or collaborator opened it. With
#                                     the dispatcher's approval record, the
#                                     links are the record's, and only while
#                                     the body has the hash it approved (see
#                                     tehas-cycle start-check). With <issue>, the
#                                     answer the caller already read for the
#                                     issue, it is not read a second time
#   tehas_answer_images <dir> <answer file> <comment id>
#                                     add to a planning session's <dir> what
#                                     the reply in <answer file> links, source
#                                     "answer:<id>"; the new files are listed
#                                     in $answer_new, what was left out in
#                                     $answer_left. Returns 1, with
#                                     $image_why, and <dir> as it was, when a
#                                     link could not be fetched
#   tehas_record_is_file <path>     a plain file at <path>, not a link and not
#                                     a directory: what counts as the record
#   tehas_issue_images_changed <kept manifest> <dir>
#                                     returns 1, with the reason in $image_why,
#                                     when both are of the same approval and a
#                                     link gave other bytes this time
#   tehas_issue_images_left_out <issue> <file> [record]
#                                     a cycle given no images and no files: a
#                                     manifest at <file> that gives none and
#                                     leaves out the links the approval record
#                                     lists, with the reason, named with that
#                                     approval, and the line that says the gap,
#                                     in $image_gap. It caps what it lists and
#                                     counts the rest in leftOutMore, so the
#                                     file stays small enough for the
#                                     improvement loop to copy. It fetches
#                                     nothing and asks GitHub nothing. Returns
#                                     1 when there is nothing to say: no
#                                     record, an issue that links nothing, a
#                                     link in place of the file. Returns 2 when
#                                     a record is there but could not be read,
#                                     or its manifest could not be written
#   tehas_approval_record           print a jq definition, `record_ok`: the
#                                     shape of the dispatcher's approval record,
#                                     so its readers cannot drift apart
#   tehas_issue_images_said <dir> <what> <reads>
#                                     the comment that says what <what> (a
#                                     cycle, a planning session) was given and
#                                     whether it was shown: <reads> is 1 when
#                                     the pi-rukas in the sandbox image reads
#                                     the manifest, 0 when it says it does not,
#                                     anything else when the image does not say
#                                     (tehas_rukas_reads_images); empty when the
#                                     issue links no image and nothing was left
#                                     out
#
# Sourcing it does nothing.

# The dispatcher's approval of a start, as tehas_approval_view (lib/store.sh)
# writes it from the state store for the readers here. at, by and how are asserted by shape, not only by
# type: they are printed to an operator and published in a cycle's record, and
# a value with a newline in it moves the fields of a manifest read back by
# line.
tehas_approval_record() {
  # shellcheck disable=SC2016  # a jq program
  printf '%s\n' 'def record_ok: type == "object" and (.links | type) == "array"
    and (.at | type) == "string" and (.at | test("^[0-9T:.Z-]+\\z"))
    and (.by | type) == "string" and (.by | test("^[A-Za-z0-9-]+\\z"))
    and (.how | test("^(label|retry-tick|outage-retry|fixer|refresh)\\z"))
    and (.comment_links == null or (.comment_links | type) == "array");'
}

# tehas_record_is_file <path>: what counts as the dispatcher's approval record,
# said once. Every reader of an approval's view asks the same question, and a
# link or a directory at that path is read as no record.
tehas_record_is_file() {
  [ -f "$1" ] && [ ! -L "$1" ]
}

# With no libexec/attach nothing can be checked, so a text that links a file
# stops and one that links nothing goes on. The reason may be put on the issue,
# so it names no host path.
tehas_no_attach="the issue's files could not be checked (the attachment checker is not installed)"
_tehas_has_attach() {
  [ -n "${TEHAS_HOME:-}" ] && [ -f "$TEHAS_HOME/libexec/attach" ] && [ -x "$TEHAS_HOME/libexec/attach" ]
}

# Wider than LINK_RULE of console/attach/src/given.rs, so no link it takes is
# missed. The text may be JSON, which may escape a slash.
_tehas_may_link() { # <text>
  case ${1//\\/} in
    *https://github.com/*/assets/* | *https://github.com/*/files/* | *https://user-images.githubusercontent.com/*) return 0 ;;
  esac
  return 1
}

# tehas_attach <verb> [args...]: run libexec/attach, within 60 s. Its answer is
# in $tehas_attach_out after a 0.
tehas_attach_out=""
tehas_attach() { # <verb> [args...]
  local verb=$1 bin=${TEHAS_HOME:-}/libexec/attach out
  tehas_attach_out=""
  _tehas_has_attach || return 1
  out=$(timeout -k 2 60 "$bin" "$@" </dev/null 2>&1) || return 1
  [[ $out != *$'\n'* ]] || return 1
  jq -e --arg v "$verb" '.api == 1 and .ok == true and .verb == $v' <<<"$out" >/dev/null 2>&1 || return 1
  tehas_attach_out=$out
}

# _tehas_take <verb> [args...]: the verbs of the program that fetch, with the
# issue on standard input. No limit of its own: curl has one. curl's own words go
# to the caller's log. $image_why is why nothing may start, and a program that
# gives no answer is a reason too: nothing was checked.
tehas_take_out=""
_tehas_take() { # <verb> [args...]
  local verb=$1 bin=${TEHAS_HOME:-}/libexec/attach rc
  tehas_take_out=""
  tehas_take_out=$(GH_TOKEN=${GH_TOKEN:-} "$bin" "$@" --attach "$bin" --images "${TEHAS_WORK_IMAGES:-1}" \
    --files "${TEHAS_WORK_FILES:-0}" --zips "${TEHAS_WORK_ZIPS:-0}")
  rc=$?
  if [ "$rc" != 0 ] || ! image_why=$(jq -er --arg v "$verb" \
    'select(.api == 1 and .ok == true and .verb == $v) | .why // ""' <<<"$tehas_take_out" 2>/dev/null); then
    image_why="the issue's files could not be checked (attach: $verb exited $rc)"
    return 1
  fi
  [ -z "$image_why" ]
}

# tehas_issue_images with no program: <dir> with a manifest that gives nothing,
# when there is nothing to give. A record's links and the words of a retry are
# what a start with one is given, else the links of the issue's text.
_tehas_gives_nothing() { # <issue> <dir> [record] [issue]
  local data=${4:-} approval=null files=""
  if [ -n "${3:-}" ]; then
    image_why=$tehas_no_attach
    approval=$(jq -ec 'select((.links // []) + (.comment_links // []) + (.retry_context // []) == []) | {at, by, how}' \
      "$3" 2>/dev/null) || return 1
  else
    image_why="the issue could not be read for its images"
    [ -n "$data" ] || { data=$(gh api "repos/$TEHAS_REPO/issues/$1" 2>/dev/null) || return 1; }
    jq -e 'type == "object" and (.body == null or (.body | type) == "string")' <<<"$data" >/dev/null 2>&1 || return 1
    image_why=$tehas_no_attach
    ! _tehas_may_link "$data" || return 1
  fi
  [ "${TEHAS_WORK_FILES:-0}" != 1 ] || files=$'  "files": [],\n'
  image_why="the directory for the issue's images could not be made"
  rm -rf "$2" && mkdir -p "$2" && chmod 755 "$2" || return 1
  printf '{\n  "images": [],\n%s  "leftOut": [],\n  "issue": %s,\n  "approval": %s,\n  "leftOutMore": 0\n}\n' \
    "$files" "$1" "$approval" >"$2/manifest.json" && chmod 644 "$2/manifest.json"
}

# The links as the approval saw them. tehas-cycle start-check held the words of
# this answer to that approval before this was called.
tehas_issue_images() { # <issue> <dir> [approved] [record] [issue]
  _tehas_has_attach || { _tehas_gives_nothing "$1" "$2" "${4:-}" "${5:-}"; return; }
  _tehas_take issue "$1" "$2" --approved "${3:-}" --record "${4:-}" --repo "${TEHAS_REPO:-}" \
    --comments "${TEHAS_WORK_COMMENTS:-0}" --allowed "${TEHAS_ALLOWED_LABELERS:-}" \
    --millwright "${TEHAS_MILLWRIGHT_LOGIN:-}" <<<"${5:-}"
}

# shellcheck disable=SC2034  # the caller reads what this sets
tehas_answer_images() { # <dir> <answer file> <comment id>
  local reply
  answer_new=""
  answer_left=""
  if ! _tehas_has_attach; then
    image_why=$tehas_no_attach
    reply=$(cat -- "$2" 2>/dev/null) || return 1
    ! _tehas_may_link "$reply"
    return
  fi
  _tehas_take answer "$1" "$2" "$3" </dev/null || return 1
  answer_new=$(jq -r .new <<<"$tehas_take_out")
  answer_left=$(jq -r .left <<<"$tehas_take_out")
}

# What a cycle was given, said by the program: console/attach/src/given.rs.

# Returns 1, with the reason in $image_why, when a link gave other bytes than
# when this approval was first fetched. A program that cannot answer has not
# held the link to its bytes, so the start stops; the reason names no host path,
# because it may be put on the issue.
# shellcheck disable=SC2034  # the caller reads what this sets
tehas_issue_images_changed() { # <kept manifest> <dir>
  local link
  [ -f "$1" ] && [ ! -L "$1" ] || return 0
  # Nothing given has no bytes to hold to the kept manifest.
  if ! _tehas_has_attach && jq -e '.images + (.files // []) == []' "$2/manifest.json" >/dev/null 2>&1; then
    return 0
  fi
  tehas_attach changed "$1" "$2/manifest.json" || {
    image_why="the kept manifest could not be compared"
    return 1
  }
  link=$(jq -r '.link // empty' <<<"$tehas_attach_out")
  [ -z "$link" ] || { image_why="$link gave other bytes than when this approval was first fetched"; return 1; }
}

# The line that says the gap is left in $image_gap. Returns 1 when there is
# nothing to say, and 2 when a record is there but could not be read, or its
# manifest could not be written.
tehas_issue_images_left_out() { # <issue> <file> [record]
  image_gap=""
  tehas_record_is_file "${3:-}" || return 1
  tehas_attach left-out "$1" "$2" "$3" --images "${TEHAS_WORK_IMAGES:-0}" --files "${TEHAS_WORK_FILES:-0}" || return 2
  image_gap=$(jq -r '.gap // empty' <<<"$tehas_attach_out")
  [ -n "$image_gap" ] || return 1
}

tehas_issue_images_said() { # <dir> <what> <reads>
  tehas_attach said "$1" "$2" --reads "${3:-}" --build "${TEHAS_HOME:-$HOME/tehas}/sandbox/build.sh" || return 1
  jq -r '.text // empty' <<<"$tehas_attach_out"
}
