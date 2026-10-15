# shellcheck shell=bash
# The improvement loop's shared parts: the switch, the record a cycle leaves
# for the loop, what a cycle ran with, and how a whole log is packed for
# upload. Sourced by libexec/improve, by tehas work and by the script that
# ends a cycle.
#
#   tehas_improve_on       is the loop switched on for this instance?
#   tehas_improve_dir      print where records wait
#   tehas_improve_record <issue> <started> <rc> <status> <step> [<pr> [<kills> [<image> [<rukas> [<cap> [<reason>]]]]]]
#                          write the record of a cycle that has ended. <kills>
#                          is how many processes in its container were killed
#                          for memory, oomKills in cycle.json; anything but a
#                          number counts as 0. <image> and <rukas> are the id
#                          of the image the container started from and the
#                          pi-rukas commit of its label, image and rukas.
#                          <cap> and <reason> are what ended the cycle, cap
#                          and capReason
#   tehas_improve_settings <file> <concurrent> <docker options>...
#                          write what a cycle ran with, for its record: the
#                          PI_ENSEMBLE_* variables the -e options give the
#                          container, the instance's TEHAS_* settings that
#                          shape a cycle, and how many cycles were running
#   tehas_improve_secret <file>
#                          does the file hold something shaped like a token?
#   tehas_improve_pack <file> <dir> <name> <limit>
#                          gzip <file> into <dir> as <name>.gz, or in numbered
#                          pieces when that would be over <limit> bytes
#
# Source lib/config.sh and call tehas_load first.
#
# A record is a directory, window/<issue>-<stamp>, and the only thing the loop
# reads of a cycle. Its cycle.json also holds what tehas work left in
# state/cycles/<issue>.settings, when this cycle wrote it, and the provider
# retry line of the preflight log. With the issue's attachments given, it also holds what
# tehas work kept of them: manifest.json, images.md and retry-context.md, each
# when the cycle had it, at most 256 KB. Never an image or a file. It is built beside the window and renamed into it, so a
# run never sees half of one and cycles that end together do not collide.

tehas_improve_on() { [ -n "${TEHAS_IMPROVE_REPO:-}" ]; }

tehas_improve_dir() { printf '%s\n' "$TEHAS_HOME/tmp/improve"; }

# The stream as a model can read it. A line that is not JSON is dropped. The
# id of a status event differs every time, so it goes, and uniq then keeps a
# status repeated in a row once.
# shellcheck disable=SC2016  # a jq program: the variables are jq's
_tehas_improve_condense='
  fromjson? | select(type == "object" and .type != "message_update")
  | if .type == "extension_ui_request" then del(.id) else . end
  | walk(if type == "string" and length > 2000 then .[0:2000] + " [cut]" else . end)'

tehas_improve_record() { # <issue> <started> <rc> <status> <step> [<pr> [<kills> [<image> [<rukas> [<cap> [<reason>]]]]]]
  tehas_improve_on || return 0
  local issue=$1 started=$2 rc=$3 status=$4 step=$5 pr=${6:-} kills=${7:-0} image=${8:-} rukas=${9:-} base raw rec tmp f from
  local cap reason settings retry
  cap=$(tr -cd 'A-Za-z0-9_.:-' <<<"${10:-}" | head -c 80)
  reason=$(tr -cd 'A-Za-z0-9 _.,:;/()=+-' <<<"${11:-}" | head -c 200)
  [[ $kills =~ ^(0|[1-9][0-9]{0,8})$ ]] || kills=0
  [[ $image =~ ^sha256:[0-9a-f]{64}$ ]] || image=""
  base=$(tehas_improve_dir)
  raw=$(find "$TEHAS_HOME/logs" -maxdepth 1 -type f -name "work-$issue-*.jsonl" \
    -newermt "@$((started - 5))" 2>/dev/null | sort | tail -n 1)
  if [ -z "$raw" ]; then
    tehas_err "improve: no log of issue $issue since the cycle began, no record written"
    return 1
  fi
  rec=$(basename "$raw" .jsonl)
  rec=${rec#work-}
  mkdir -p "$base/window" || return 1
  tmp=$(mktemp -d "$base/.new.XXXXXX") || return 1
  # The pi-rukas this cycle ran with: the image's, as its container started,
  # else as tehasbot cycle noted it at the start. What a fix in it is checked
  # against.
  [[ $rukas =~ ^[0-9a-f]{7,40}$ ]] || rukas=$(head -c 64 "$TEHAS_HOME/state/cycles/$issue.rukas" 2>/dev/null | head -n 1)
  [[ $rukas =~ ^[0-9a-f]{7,40}$ ]] || rukas=""
  # What it ran with: only this cycle's file, and only an object of the shape
  # tehas_improve_settings writes.
  settings=null
  from=$TEHAS_HOME/state/cycles/$issue.settings
  if [ -f "$from" ] && [ ! -L "$from" ] && [ "$(stat -c %s "$from" 2>/dev/null || echo 0)" -le 65536 ] &&
    [ -n "$(find "$from" -maxdepth 0 -newermt "@$((started - 5))" 2>/dev/null)" ]; then
    settings=$(jq -c 'if type == "object" and (.container | type == "object") and (.host | type == "object")
      and ([.container[], .host[]] | all(type == "string"))
      and (.concurrentCycles == null or (.concurrentCycles | type == "number"))
      then . else null end' "$from" 2>/dev/null) || settings=null
    [ -n "$settings" ] || settings=null
  fi
  retry=$(grep -a -m 1 '^provider retry: ' "$raw.preflight" 2>/dev/null | tr -cd 'A-Za-z0-9 _.,:;/()=+-' | head -c 200)
  if ! jq -n --argjson issue "$issue" --argjson started "$started" --argjson rc "$rc" \
    --argjson ended "$(date +%s)" --arg status "$status" --arg step "$step" --arg pr "$pr" \
    --arg log "$(basename "$raw")" --argjson kills "$kills" --arg rukas "$rukas" --arg image "$image" \
    --arg cap "$cap" --arg reason "$reason" --argjson settings "$settings" --arg retry "$retry" '
      {issue: $issue, started: $started, seconds: ($ended - $started), status: $status,
       step: $step, rc: $rc, pr: (if $pr == "" then null else $pr end), log: $log,
       oomKills: $kills, rukas: (if $rukas == "" then null else $rukas end),
       image: (if $image == "" then null else $image end),
       cap: (if $cap == "" then null else $cap end), capReason: (if $reason == "" then null else $reason end),
       settings: (if $settings == null then null else {container: $settings.container, host: $settings.host} end),
       concurrentCycles: (if $settings == null then null else $settings.concurrentCycles end),
       providerRetry: (if $retry == "" then null else $retry end)}' \
    >"$tmp/cycle.json"; then
    rm -rf "$tmp"
    return 1
  fi
  jq -c -R "$_tehas_improve_condense" "$raw" 2>/dev/null | uniq >"$tmp/stream.jsonl"
  cp "$TEHAS_HOME/live/console-$issue.txt" "$tmp/console.txt" 2>/dev/null || : >"$tmp/console.txt"
  cp "$raw.err" "$tmp/err.txt" 2>/dev/null || : >"$tmp/err.txt"
  cp "$raw.preflight" "$tmp/preflight.txt" 2>/dev/null || : >"$tmp/preflight.txt"
  # What the cycle was given of the issue's attachments, as tehas work kept it:
  # the manifest, the model's descriptions and a retry's pull request text.
  # Never the files themselves. Only this cycle's, only plain files, and none
  # over the bound.
  for f in manifest.json images.md retry-context.md; do
    from=$TEHAS_HOME/state/cycles/$issue.$f
    [ -f "$from" ] && [ ! -L "$from" ] || continue
    [ -n "$(find "$from" -maxdepth 0 -newermt "@$((started - 5))" 2>/dev/null)" ] || continue
    [ "$(stat -c %s "$from" 2>/dev/null || echo 0)" -le 262144 ] || continue
    cp "$from" "$tmp/$f" 2>/dev/null || rm -f "$tmp/$f"
  done
  rm -rf "${base:?}/window/$rec"
  if ! { mkdir -p "$base/window" && mv "$tmp" "$base/window/$rec"; }; then
    rm -rf "$tmp"
    return 1
  fi
}

# The TEHAS_* settings a record carries: those that shape a cycle. Not every
# one: the analyst's words are filed in public, and a host's addresses, its
# domain and the analytics settings are none of its business.
_tehas_improve_host='^TEHAS_(WORK_(TIMEOUT|STALL|CPUS|MEMORY|IMAGES|FILES|COMMENTS|DESCRIBE)|CONCURRENCY|TICKET_CONCURRENCY|MAX_AWAITING|CYCLE_RESERVE|VERIFY_TIMEOUT|PREFLIGHT_WAIT|STOP_WAIT|CONSOLE_WAIT|BRANCH)$'

# Only a value of a safe shape goes in: a number, a boolean, or a short word
# of [A-Za-z0-9_.,:=-]. Everything else is [withheld]: any path, URL or
# address, anything with an @, an IP address, a name with a domain in it
# (a dot before a letter) or a port, a long run of letters and digits that
# may be a token, and any value whose name says it may be a
# secret or a host.
_tehas_improve_value() { # <name> <value>
  local v=$2
  case "$1" in
    *KEY* | *TOKEN* | *SECRET* | *PASS* | *PWD* | *COOKIE* | *AUTH* | *CRED* | *BEARER* | *PRIVATE* | \
      *HOST* | *URL* | *ADDR* | *ENDPOINT* | *DOMAIN* | *PROXY*)
      echo "[withheld]"
      return
      ;;
  esac
  if [[ $v =~ ^[A-Za-z0-9_.,:=-]{0,64}$ ]] && ! [[ $v =~ ^[0-9]{1,3}(\.[0-9]{1,3}){3}(:[0-9]+)?$ ]] &&
    ! [[ $v =~ [A-Za-z0-9]{16,} ]] && ! [[ $v =~ (^|[^A-Za-z0-9])eyJ ]] &&
    ! [[ $v =~ [A-Za-z0-9-]\.[A-Za-z] ]] && ! [[ $v =~ :[0-9]+$ ]] &&
    ! tehas_improve_secret /dev/stdin <<<"$v"; then
    printf '%s' "$v"
  else
    echo "[withheld]"
  fi
}

tehas_improve_settings() { # <file> <concurrent> <docker options>...
  local out=$1 n=$2 a next=0 name value
  local -a pairs=() host=()
  shift 2
  [[ $n =~ ^(0|[1-9][0-9]{0,4})$ ]] || n=null
  for a in "$@"; do
    if [ "$next" = 1 ]; then
      next=0
      name=${a%%=*}
      [[ $name =~ ^PI_ENSEMBLE_[A-Z0-9_]+$ ]] || continue
      if [ "$name" = "$a" ]; then value=${!name-}; else value=${a#*=}; fi
      pairs+=("$name" "$(_tehas_improve_value "$name" "$value")")
    elif [ "$a" = -e ]; then
      next=1
    fi
  done
  while IFS= read -r name; do
    host+=("$name" "$(_tehas_improve_value "$name" "${!name-}")")
  done < <(compgen -v | grep -E "$_tehas_improve_host" | sort)
  jq -n --argjson n "$n" --argjson split "${#pairs[@]}" '
    def obj: [range(0; length; 2) as $i | {key: .[$i], value: .[$i + 1]}] | from_entries;
    {container: ($ARGS.positional[0:$split] | obj), host: ($ARGS.positional[$split:] | obj),
     concurrentCycles: $n}' --args "${pairs[@]}" "${host[@]}" >"$out.new" && mv "$out.new" "$out" && return 0
  rm -f "$out.new"
  return 1
}

# Logs should hold no secret. This is for the day one does: a log that matches
# is not uploaded.
tehas_improve_secret() { # <file>
  grep -E -q -e 'gh[pousr]_[A-Za-z0-9]{36,}' -e 'github_pat_[A-Za-z0-9_]{22,}' \
    -e '(^|[^A-Za-z0-9])sk-[A-Za-z0-9_-]{20,}' -e 'xox[abprs]-[A-Za-z0-9-]{10,}' -e 'AKIA[0-9A-Z]{16}' \
    -e '-----BEGIN [A-Z ]*PRIVATE KEY-----' "$1"
}

# Pieces are split at line boundaries and compressed one by one, so each opens
# on its own. The number of pieces doubles until every piece fits.
tehas_improve_pack() { # <file> <dir> <name> <limit>
  local file=$1 dir=$2 name=$3 limit=$4 n lines parts i f big
  mkdir -p "$dir" || return 1
  gzip -9 -c "$file" >"$dir/$name.gz" || return 1
  n=$(stat -c %s "$dir/$name.gz")
  [ "$n" -gt "$limit" ] || return 0
  rm -f "$dir/$name.gz"
  n=$((n / limit + 1))
  lines=$(wc -l <"$file")
  parts=$dir/.parts
  while [ "$lines" -ge 2 ]; do
    [ "$n" -le "$lines" ] || n=$lines
    rm -rf "$parts" "$dir/$name".*.gz
    mkdir -p "$parts" || return 1
    split -n "l/$n" -a 5 -d "$file" "$parts/p." || break
    i=0
    big=0
    for f in "$parts"/p.*; do
      [ -s "$f" ] || continue
      i=$((i + 1))
      gzip -9 -c "$f" >"$dir/$name.$i.gz" || big=1
      [ "$(stat -c %s "$dir/$name.$i.gz")" -le "$limit" ] || big=1
    done
    rm -rf "$parts"
    [ "$big" = 1 ] || return 0
    [ "$n" -lt "$lines" ] || break
    n=$((n * 2))
  done
  rm -rf "$parts" "$dir/$name".*.gz
  return 1
}
