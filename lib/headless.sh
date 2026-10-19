# shellcheck shell=bash
# What tehas reads of pi-rukas's headless runner: its --capabilities answer,
# its NDJSON progress and its --json result. Only the contract the runner
# publishes is read here (docs/headless.md in pi-rukas), never its state file.
# Sourced by tehas work (libexec/work) and tehasbot cycle (libexec/cycle).
#
#   tehas_headless_unsupported <caps> <flags> <names> [features]
#       what the image's --capabilities answer <caps> does not support, one
#       line each: a contract version other than the one read here, a flag in
#       <flags> (the work arguments, space separated) that the work verb does
#       not take, a variable in <names> (space separated) that is no input a
#       host may set, a name in <features> that the answer's features do not
#       set to true. Prints nothing when all are supported. Returns 1 when
#       the answer is not an object.
#       A family such as PI_ENSEMBLE_MODEL_<ROLE> takes the members the
#       answer lists for it, and any name characters when it lists none.
#   tehas_headless_reads_images <caps>
#       1 when the answer lists PI_ENSEMBLE_ISSUE_IMAGES for the manifest
#       format tehas writes (manifestVersion 1), else 0.
#   tehas_headless_expired <caps> <workarounds>
#       the workarounds in <workarounds> (NAME@VERSION, space separated) that
#       the image's pi-rukas no longer needs: its version is VERSION or later.
#       One line each. A version that cannot be read expires them all, said
#       once: whether the fix is there is not known.
#   tehas_headless_run <ndjson> <issue>
#       two lines: the step of the last stable step-started line, and the step
#       right after commit-pr as the branch pi-rukas pushed, said only when it
#       is this issue's own branch name. A runner that died wrote no result, so
#       these are all tehas has of the step it was in and the branch it pushed.
#   tehas_headless_now <ndjson>
#       the step of the last stable step-started line and the message of the
#       last stable log line, as two lines.
#   tehas_headless_progress <ndjson> [<caps> <result> <issue>]
#       one JSON object: roles, the per-role usage; reason, why the cycle
#       stopped at its cap; failure, the result's failure kind, step, role and
#       cause, or null; from, where roles and failure were read; reviewHeld, a
#       stable cap-hit ended the review on its round cap and sent the cycle on
#       to ci, so its findings were left open. roles and failure come from the
#       result where the answer says it carries them (features.usageByRole,
#       features.failureReason), else from the progress: the stable
#       dispatch-completed lines, each counted once by its key, and for the
#       reason lines the runner marks unstable.
#   tehas_headless_outcome <result> <issue>
#       the issue's entry of a schema 4 result as one JSON object, its text
#       fields reduced to plain characters, its exitCode null unless a number.
#       handoffBody is the handoff's markdown as the runner rendered it, cut
#       at 30000 characters and not cleaned: it is for a comment, through
#       q_clean. Returns 1 when there is none.
#   tehas_headless_error_only <result>
#       whether <result> is a schema 4 result with no issue entry at all, as a
#       refusal that carries only an error may be.

# The contract versions this file and the console read. Lower case: tehas_load
# unsets every TEHAS_* name. Result schema 4 renamed pi-rukas's review-loop cap
# wall-clock to lens-review-timeout, so wall-clock is tehas's own: its time
# limit. A schema 3 result could mean either.
tehas_headless_exit_codes=1
tehas_headless_result=4
tehas_headless_ndjson_stable=1

tehas_headless_unsupported() { # <caps> <flags> <names> [features]
  jq -e 'type == "object"' >/dev/null 2>&1 <<<"$1" || return 1
  jq -r --arg flags "$2" --arg names "$3" --arg features "${4:-}" \
    --argjson ev "$tehas_headless_exit_codes" --argjson rv "$tehas_headless_result" \
    --argjson nv "$tehas_headless_ndjson_stable" '
    def words: split(" ") | map(select(length > 0));
    # A family such as PI_ENSEMBLE_MODEL_<ROLE> takes its listed members, or
    # any suffix of name characters when the build lists none.
    def family($m): ([$m[]? | strings | select(test("^[A-Z0-9_]+$"))]) as $ids
      | gsub("<[A-Z]+>"; if ($ids | length) > 0 then "(" + ($ids | join("|")) + ")" else "[A-Z0-9_]+" end)
      | "^" + . + "$";
    [.inputs[]? | objects | select(.scope == "cycle" or .scope == "sandbox")
     | .members as $m | .name | strings | select(test("^[A-Z0-9_<>]+$")) | family($m)] as $ok
    | [.verbs.work.flags[]? | strings] as $have
    | (if .contracts.exitCodes.version == $ev then empty
       else "exit codes version \(.contracts.exitCodes.version), not \($ev)" end),
      (if .contracts.resultSchema.version == $rv then empty
       else "result schema version \(.contracts.resultSchema.version), not \($rv)" end),
      (if .contracts.ndjson.stable.version == $nv then empty
       else "stable NDJSON version \(.contracts.ndjson.stable.version), not \($nv)" end),
      ($flags | words[] | select(startswith("--")) | select(. as $f | $have | index($f) | not) | "the flag \(.)"),
      ($names | words[] | select(. as $n | any($ok[]; . as $re | $n | test($re)) | not)
       | "the input \(.)"),
      (.features as $f | $features | words[] | select(($f | type) != "object" or $f[.] != true)
       | "the feature \(.)")' <<<"$1" 2>/dev/null | tr -cd 'A-Za-z0-9_ .,<>\n-'
}

tehas_headless_expired() { # <caps> <workarounds>
  local have entry name want
  [ -n "$2" ] || return 0
  have=$(jq -r '.version | strings' 2>/dev/null <<<"$1")
  if ! [[ $have =~ ^[0-9]{1,6}(\.[0-9]{1,6}){1,3}$ ]]; then
    printf '%s\n' "the image's pi-rukas version could not be read, so it is not known whether $2 are still needed"
    return 0
  fi
  for entry in $2; do
    name=${entry%@*} want=${entry##*@}
    [ "$(printf '%s\n%s\n' "$want" "$have" | sort -V | head -n 1)" = "$want" ] || continue
    printf '%s\n' "$name works around a pi-rukas defect fixed in $want, and the image's pi-rukas is $have"
  done
}

tehas_headless_reads_images() { # <caps>
  if [ "$(jq -r 'if type == "object" and any(.inputs[]? | objects;
    .name == "PI_ENSEMBLE_ISSUE_IMAGES" and .manifestVersion == 1) then 1 else 0 end' <<<"$1" 2>/dev/null)" = 1 ]; then
    echo 1
  else
    echo 0
  fi
}

# The stable lines of a progress stream, each a JSON object. A line that is
# not JSON, or that the runner marks unstable, is not one of them.
_tehas_headless_stable() { # <ndjson>
  jq -R -c 'fromjson? | objects | select(.v == 1 and (.unstable // false) == false)' "$1" 2>/dev/null
}

# A step name this platform accepts, and the one the step right after commit-pr
# must be: pi-rukas's branch words the branch it pushed, and a branch name holds
# a solidus and runs to 100 characters.
tehas_headless_step_chars='^[A-Za-z0-9_./:-]{1,100}$'

# What the step-started lines of a run say of it, in one pass over its log:
# first the last step the runner says it started, which is the step of a cycle
# whose result names none, and second the step right after commit-pr, the branch
# pi-rukas pushed. The branch is said only when it is this issue's own, as
# pi-rukas names a branch; any other name, and a run that never got to
# commit-pr, give an empty second line.
tehas_headless_run() { # <ndjson> <issue>
  tail -c 200000 "$1" 2>/dev/null | jq -R -r -s --argjson issue "$2" \
    --arg chars "$tehas_headless_step_chars" '
    [split("\n")[] | fromjson? | objects
     | select(.v == 1 and (.unstable // false) == false)
     | select(.kind == "step-started") | (.event.step | strings)
     | select(test($chars))] as $s
    | ([$s | to_entries[] | select(.value == "commit-pr") | .key] | last // -1) as $at
    | (($s | length) as $n | if $at < 0 or $at + 1 >= $n then ""
       else $s[$at + 1] end) as $after
    | ($s | last // ""),
      (if $after != "" and
          ($after | test("^feature/issue-" + ($issue | tostring) + "(-[A-Za-z0-9._-]{1,80})?$"))
       then $after else "" end)' 2>/dev/null | head -n 2
}

tehas_headless_now() { # <ndjson>
  tail -c 200000 "$1" 2>/dev/null | jq -R -r -s '
    [split("\n")[] | fromjson? | objects | select(.v == 1 and (.unstable // false) == false)] as $l
    | def line: gsub("[\u0000-\u001f\u007f\u2028\u2029]"; "");
      ([$l[] | select(.kind == "step-started") | .event.step | strings] | last // "" | line),
      ([$l[] | select(.kind == "log") | .message | strings] | last // ""
       | [split("\n")[] | select(length > 0 and (startswith("pi-rukas:driver-event") | not))] | first // "" | line)' \
    2>/dev/null | head -n 2
}

tehas_headless_progress() { # <ndjson> [<caps> <result> <issue>]
  local legacy result=null caps
  caps=$(jq -c 'objects' <<<"${2:-}" 2>/dev/null) || caps=""
  [ -n "$caps" ] || caps='{}'
  legacy=$(_tehas_headless_progress_lines "$1") || legacy=""
  [ -n "$legacy" ] || legacy='{"roles":[],"reason":"","reviewHeld":false}'
  if [ -n "${4:-}" ] && [ ! -L "${3:-}" ]; then
    result=$(jq -c --argjson issue "$4" --argjson rv "$tehas_headless_result" '
      select(type == "object" and .schemaVersion == $rv)
      | [.issues[]? | objects | select(.issue == $issue)] | first // null' "$3" 2>/dev/null) || result=null
    [ -n "$result" ] || result=null
  fi
  jq -n -c --argjson legacy "$legacy" --argjson r "$result" --argjson caps "$caps" '
    def n: if type == "number" and . >= 0 then . else 0 end;
    def plain: if type == "string" then gsub("[^A-Za-z0-9_.:-]"; "") else "" end;
    (($caps.features.usageByRole == true) and ($r != null)) as $by_role
    | (($caps.features.failureReason == true) and ($r != null)) as $by_failure
    | ($r.failure // null | objects // null) as $f
    | {roles: (if $by_role then
               [($r.usage | objects | .byRole | objects) // {} | to_entries[] | select(.value | type == "object")
                | (.key | plain) as $k
                | {role: (if $k == "" then "driver" else $k end), dispatches: (.value.dispatches | n),
                   turns: (.value.turns | n), input: (.value.input | n), output: (.value.output | n),
                   cacheRead: (.value.cacheRead | n)}]
             else $legacy.roles end),
       reason: (if $by_failure then
                ([$f.providerMessage?, $f.message?] | map(strings | select(length > 0)) | first // "")
              else $legacy.reason end),
       failure: (if $by_failure and $f != null then
                 {kind: ($f.kind | plain), step: ($f.step | plain), role: ($f.role | plain), cause: ($f.cause | plain)}
               else null end),
       reviewHeld: ($legacy.reviewHeld == true),
       from: {roles: (if $by_role then "result" else "progress" end),
              failure: (if $by_failure then "result" else "progress" end)}}' 2>/dev/null
}

# The per-role usage and the reason of a build whose result carries neither
# (v0.12.80): the stable dispatch-completed lines, and for the reason the
# lines the runner marks unstable.
# TODO(trail-openers/pi-rukas#1367): remove once no host runs an image
# without features.failureReason and features.usageByRole.
_tehas_headless_progress_lines() { # <ndjson>
  {
    _tehas_headless_stable "$1"
    jq -R -c 'fromjson? | objects | select(.v == 1 and .unstable == true)
      | select(.kind == "dispatch-failed" or .kind == "dispatch-failed-provider")' "$1" 2>/dev/null
    jq -R -c 'fromjson? | objects | select(.v == 1 and .kind == "cap-hit")' "$1" 2>/dev/null
  } | jq -s -c '
    def n: tonumber? // 0;
    def plain: tostring | gsub("[^A-Za-z0-9_.-]"; "");
    (map(select(.kind == "dispatch-completed" and (.unstable // false) == false))
     | unique_by(.key) | map(.event | objects)
     | map({role: ((.role // "" | plain) as $r | if $r == "" then "driver" else $r end),
            u: (.usage // {} | {turns: (.turns | n), input: (.input | n), output: (.output | n),
                               cacheRead: (.cacheRead | n)})})
     | group_by(.role)
     | map({role: .[0].role, dispatches: length, turns: (map(.u.turns) | add),
            input: (map(.u.input) | add), output: (map(.u.output) | add),
            cacheRead: (map(.u.cacheRead) | add)})) as $roles
    | ([.[] | select(.kind == "cap-hit") | .event.evidence?] | map(strings | select(length > 0)) | last) as $evidence
    | any(.[]; .kind == "cap-hit" and (.unstable // false) == false
          and .event.cap? == "round-cap" and .event.nextStep? == "ci") as $held
    | ([.[] | select(.unstable == true) | .event | (.providerMessage?, .errorTail?)]
       | map(strings | select(length > 0)) | last) as $failure
    | {roles: $roles, reason: ($evidence // $failure // ""), reviewHeld: $held}' 2>/dev/null
}

tehas_headless_error_only() { # <result>
  jq -e --argjson rv "$tehas_headless_result" \
    'type == "object" and .schemaVersion == $rv and ((.issues // []) | type == "array" and length == 0)' \
    "$1" >/dev/null 2>&1 || return 1
}

tehas_headless_outcome() { # <result> <issue>
  jq -e -c --argjson issue "$2" --argjson rv "$tehas_headless_result" '
    def plain: if type == "string" then gsub("[^A-Za-z0-9_.:/-]"; "") else "" end;
    def n: if type == "number" and . >= 0 then . else 0 end;
    select(type == "object" and .schemaVersion == $rv)
    | . as $r
    | [.issues[]? | objects | select(.issue == $issue)] | first
    | select(. != null)
    | {status: (.status | plain), cap: (.cap | plain), failedStep: (.failedStep | plain),
       lastStep: ([.steps[]? | .step? | plain] | last // ""),
       parkReason: (.parkReason | plain),
       handoffBody: (.handoff.body? | if type == "string" then .[0:30000] else "" end),
       branch: (.branch | plain), prUrl: (.prUrl | plain),
       resumable: (.resumable == true),
       exitCode: (if (.exitCode | type) == "number" then .exitCode else null end),
       usage: (.usage // {} | {input: (.input | n), output: (.output | n),
               cacheRead: (.cacheRead | n), turns: (.turns | n), dispatches: (.dispatches | n)}),
       buildCommit: ($r.buildCommit | plain), buildVersion: ($r.buildVersion | plain)}' "$1" 2>/dev/null
}
