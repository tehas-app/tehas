# shellcheck shell=bash
# shellcheck disable=SC2016,SC2034  # stub bodies are expanded when the stub runs; the parts read the variables
# What the tests of libexec/plan share: test/plan_*_test.sh source this, and
# scripts/test.sh runs no *_lib.sh file. A stub docker records its arguments,
# keeps the first two lines it is sent and answers with a recorded event
# stream, so no model runs.
# The part has sourced test/lib.sh before this.
# The CPU quota and the ticket ceiling follow the host's CPUs: the same on
# every test machine.
t_stub nproc 'echo 16'

plan="$T_ROOT/libexec/plan"
fx=$T/fx
mkdir -p "$fx"
checkout=$T/home/tehas/repos/widget
ws=$T/home/tehas/work/widget
# The directory on the host behind it, for issue 7, and the turn's own state.
tree=$ws-plan-7
state=$tree.state
upstream=$T/git/acme/widget
mkdir -p "$upstream"
git init -q -b main "$upstream"
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m first
git clone -q "$upstream" "$checkout"

# The turn's workspace is gone when the turn is over, so the container notes
# what it was handed there.
t_stub docker 'if [ "$1" = info ]; then cat "$T/docker.info"; exit; fi
# tehas work asks the image what it supports; one case runs it.
case " $* " in *" --capabilities "*) cat "$T/caps.json" 2>/dev/null; exit ;; esac
printf "%s\n" "$@" >"$T/calls/docker.args.$$"
case " $* " in *" --mode "*)
  ws=$T/home/tehas/work/widget-plan-7
  cat "$ws/.tehas/brief.md" >"$T/calls/brief.seen" 2>/dev/null
  git -C "$ws" status --porcelain >"$T/calls/tree.status" 2>&1
  cp "$ws/.git/info/exclude" "$T/calls/exclude.seen" 2>/dev/null
  mkdir -p "$ws/.tehas/definition" && echo "{}" >"$ws/.tehas/definition/app.json"
  git -C "$ws" status --porcelain -uall >"$T/calls/tree.status.definition" 2>&1
  git -C "$ws" rev-parse HEAD >"$T/calls/head.seen" 2>&1
  ls -A "$ws/.git/pi-plan-cache" >"$T/calls/cache.seen" 2>/dev/null
  cat "$ws.images/manifest.json" >"$T/calls/manifest.seen" 2>/dev/null
  # What pi-rukas does before it reads its plan cache, and what filing needs.
  if [ -e "$T/fx/fetch" ]; then
    git -C "$ws" fetch -q origin --end-of-options main >"$T/calls/fetch.err" 2>&1
    git -C "$ws" rev-parse refs/remotes/origin/main >"$T/calls/origin.seen" 2>&1
    git -C "$ws" push -q origin HEAD:refs/heads/plan-push >"$T/calls/push.err" 2>&1
    echo "$?" >"$T/calls/push.rc"
  fi
  cat "$ws/.git/pi-plan-cache/"*.json >"$T/calls/cache.read" 2>/dev/null
  # What the agent leaves in the plan cache, when a case says.
  [ ! -e "$T/fx/cache" ] || . "$T/fx/cache"
  # The host directory the container has at /home/vscode/.pi/agent/sessions,
  # read from the mounts of this call: the state directory of the turn, made
  # anew for it, so what it holds was put there by tehas. The id is the
  # argument after --session, empty for a first turn. A later turn has the
  # file only when tehas carried the sessions of the last turn across.
  container=/home/vscode/.pi/agent/sessions
  mount=$(grep -F -e "$container" "$T/calls/docker.args.$$" | head -n 1)
  mount_at=:$container
  pstate=${mount%$mount_at}
  sid=$(grep -F -A 1 -x -- --session "$T/calls/docker.args.$$" |
    grep -v -F -x -- --session | head -n 1)
  # What the turn was given to reopen, then what its agent leaves there: the
  # text of the session, when a case says, as the agent would leave it.
  ls -A "$pstate" >"$T/calls/sessions.given" 2>/dev/null
  head -n 1 "$pstate/$sid.jsonl" >"$T/calls/sessions.first" 2>/dev/null
  cat "$pstate/nested/part.json" >"$T/calls/sessions.nested" 2>/dev/null
  # What the agent of this turn writes, when a case says the container wrote
  # anything at all: the file of the session, a directory beside it, or both
  # or neither. The state directory belongs to the turn alone, so the stub
  # makes it anew first: what it finds there was carried across by tehas.
  if [ -e "$T/fx/session.any" ]; then
    rm -rf "$pstate"
    mkdir -p "$pstate"
    sid=$(cat "$T/fx/session.id")
    [ ! -e "$T/fx/session.text" ] || cp "$T/fx/session.text" "$pstate/$sid.jsonl"
    if [ -e "$T/fx/session.part" ]; then
      mkdir -p "$pstate/nested"
      cp "$T/fx/session.part" "$pstate/nested/part.json"
    fi
    ls -A "$pstate" >"$T/calls/sessions.wrote" 2>/dev/null
    # A turn whose agent wrote nothing leaves an empty directory, which tehas
    # keeps as nothing: what an earlier turn left is then gone.
    keep_dir=$pstate
    ls -A "$pstate" | head -n 1 | grep -q . || rm -rf "$keep_dir"
  fi
  # Pi has a session only when its file is under the directory it was given,
  # so a later turn with nothing to reopen is told of one with no messages. A
  # case that wants the session itself gone, whatever the directory holds,
  # says so with $T/fx/no_session.
  messages=6
  if [ -n "$sid" ]; then
    [ -e "$T/fx/no_session" ] && messages=0
    [ -e "$pstate/$sid.jsonl" ] || messages=0
  fi
  IFS= read -r line
  # A later turn first asks what is in the session it opened.
  case "$line" in *"\"id\":\"opened\""*)
    printf "%s\n" "$line" >"$T/calls/docker.opened"
    if [ -e "$T/fx/opened" ]; then . "$T/fx/opened"
    else echo "{\"id\":\"opened\",\"type\":\"response\",\"command\":\"get_state\",\"success\":true,\"data\":{\"sessionId\":\"abc-123\",\"messageCount\":$messages}}"; fi
    IFS= read -r -t 5 line ;;
  esac
  printf "%s\n" "$line" >"$T/calls/docker.stdin"
  # The second line may never come: do not wait for it for ever.
  IFS= read -r -t 5 line; printf "%s\n" "$line" >"$T/calls/docker.stdin2"
  # A case that needs a container that lives for a while writes its part here.
  if [ -e "$T/fx/live" ]; then . "$T/fx/live"; else cat "$T/fx/turn.jsonl" 2>/dev/null; fi ;;
  *" image inspect "*) cat "$T/image.label" 2>/dev/null ;;
esac'
# The command looks at the log once a second. Here a look costs no time, so a
# container that lives for a second is looked at many times.
t_stub sleep
printf '[user]\n\tname = Test Bot\n\temail = bot@example.com\n' >"$T/home/.gitconfig"
# Pi's settings as install.sh leaves them on a host.
mkdir -p "$T/home/.pi/agent"
echo '{"retry":{"provider":{"timeoutMs":600000,"maxRetries":8,"maxRetryDelayMs":60000}}}' >"$T/home/.pi/agent/settings.json"
echo '{"providers":{}}' >"$T/home/.pi/agent/models.json"
# A host with the root step done: the slice loaded from its file with 8 GiB,
# so a container's ceiling is 2048m, Docker on the systemd driver and cgroup
# v2, and nothing in the slice.
t_stub systemctl 'cat "$T/slice.show"'
slice_good() {
  printf '%s\n' FragmentPath=/etc/systemd/system/tehas.slice ActiveState=active MemoryMax=8589934592 >"$T/slice.show"
  echo "systemd 2" >"$T/docker.info"
  slice_state 0.00
}
limits="--cgroup-parent --memory --memory-swap --memory-reservation --cpus --pids-limit --cpu-shares"
# The security options of lib/sandbox.sh, the same eight words a cycle is given
# (test/work_lib.sh holds the same pair for the cycle's tests).
security_opts="--cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true"
# The words after one option, read the way docker reads them.
security_of() { # <flag> (reads the arguments on standard input)
  awk -v flag="$1" 'prev == flag { print } { prev = $0 }'
}
# The same eight words, in order, as one line; empty when given another way.
security_line() {
  awk '
    { arg[NR] = $0 }
    END {
      want[1] = "--cap-drop"; want[2] = "ALL"
      want[3] = "--cap-add"; want[4] = "CHOWN"
      want[5] = "--cap-add"; want[6] = "FOWNER"
      want[7] = "--security-opt"; want[8] = "no-new-privileges:true"
      for (i = 1; i <= NR; i++) {
        if (seen >= 8) break
        if (arg[i] != want[seen + 1]) continue
        for (j = 1; j <= 8; j++) if (arg[i + j - 1] != want[j]) break
        if (j > 8) { seen = 8; start = i }
      }
      if (seen == 8) for (i = start; i < start + 8; i++) printf "%s ", arg[i]
    }'
}

turn_args() { grep -l -x -e '--mode' -e /opt/pi-rukas/bin/pi-rukas-headless "$T"/calls/docker.args.* 2>/dev/null | head -n 1 | xargs -r cat; }
after() { turn_args | awk -v flag="$1" 'prev == flag { print } { prev = $0 }'; }
image="tehas/pi-rukas:uid$(id -u)"
tail_of() { turn_args | sed -n "/^$(printf '%s' "$image" | sed 's/[\/.]/\\&/g')\$/,\$p" | tr '\n' ' '; }
# Every option that is not one this command is known to pass, up to the image
# name, read the way docker reads them. See test/work_lib.sh.
unknown_options() { # <image> <options that take a value> <options that take none>
  awk -v image="$1" -v valued=" $2 " -v plain=" $3 " '
    NR == 1 { next } # "run"
    skip { skip = 0; next }
    $0 == image { exit }
    index(valued, " " $0 " ") { skip = 1; next }
    index(plain, " " $0 " ") { next }
    /^--add-host=[A-Za-z0-9.-]+:[0-9a-fA-F.:]+$/ { next }
    { print }'
}
# What docker is given before the image, one argument per line, without the
# container's name and without the cid file, which only a cycle has.
options() {
  awk -v image="$image" '$0 == image { exit } $0 == "--cidfile" { skip = 1; next } skip { skip = 0; next }
    prev == "--name" { $0 = "<name>" } { print; prev = $0 }'
}

# A turn that asked a question with the tool, then settled.
asked() {
  cat >"$fx/turn.jsonl" <<'EOF'
{"type":"agent_start"}
{"type":"tool_execution_end","toolCallId":"c1","toolName":"question","result":{"content":[],"details":{"question":"Adopt my resolutions?","options":["Adopt all","Change some"],"answer":null,"deferred":true,"optionDetails":[{"label":"Adopt all","description":"Apply all five"},{"label":"Change some","description":""}]}},"isError":false}
{"type":"agent_settled"}
{"id":"state","type":"response","command":"get_state","success":true,"data":{"sessionId":"abc-123"}}
{"id":"text","type":"response","command":"get_last_assistant_text","success":true,"data":{"text":"Two ways.\nPick one."}}
EOF
}
# What the agent of a turn leaves in the sessions directory of its turn: the
# host directory behind /home/vscode/.pi/agent/sessions, which tehas keeps for
# the next turn. The case writes the text of the session; the stub copies it
# in under the id of the turn, as the agent would leave it. The second form
# leaves the file alone, as a container that wrote nothing does.
agent_leaves() { # <the id of the session> <what it holds>
  : >"$fx/session.any"
  printf '%s\n' "$2" >"$fx/session.text"
  printf '%s\n' '{"part":2}' >"$fx/session.part"
  printf '%s\n' "$1" >"$fx/session.id"
}
# A container whose agent wrote no session at all.
agent_leaves_nothing() {
  : >"$fx/session.any"
  rm -f "$fx/session.text" "$fx/session.part" "$fx/session.id"
}
reset() {
  rm -rf "$T"/calls/* "$T/out.json" "$fx/live" "$fx/cache" "$fx/fetch" "$fx/opened" \
    "$fx/session.any" "$fx/session.text" "$fx/session.part" "$fx/session.id" \
    "$fx/no_session" "$T/keep"
  asked
  t_instance "${T_MINIMAL[@]}"
  T_ENV=(GH_TOKEN=test-token "TEHAS_CGROUP_ROOT=$T_CGROUP")
  slice_good
  printf '# Faster search\n\nMake it fast.\n' >"$T/brief.md"
}

# The variables a work cycle is given to its container, one per line.
# The container part pins them for a plan turn, and the session part
# reads them again with the two a planning turn adds on top.
want="GH_TOKEN
GIT_CONFIG_COUNT
GIT_CONFIG_KEY_0
GIT_CONFIG_KEY_1
GIT_CONFIG_KEY_2
GIT_CONFIG_KEY_3
GIT_CONFIG_VALUE_0
GIT_CONFIG_VALUE_1
GIT_CONFIG_VALUE_2
GIT_CONFIG_VALUE_3
GIT_TERMINAL_PROMPT
CBM_CACHE_DIR
PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS
PI_ENSEMBLE_MAX_WORKSTREAMS
PI_ENSEMBLE_PLAN_CACHE_TTL_MS
PI_ENSEMBLE_SANDBOX_MODE
PI_ENSEMBLE_SUBAGENT_MODEL
PI_ENSEMBLE_SUBAGENT_PROVIDER
PI_ENSEMBLE_VERIFY_TIMEOUT_MS
PI_ENSEMBLE_WORKSPACE_ROOT"
