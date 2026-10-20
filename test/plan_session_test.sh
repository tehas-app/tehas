#!/usr/bin/env bash
# Tests for libexec/plan: what a session carries from one turn to the
# next, the issue's images, and a reply's attachments.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/plan_lib.sh
. "$(dirname "$0")/plan_lib.sh" || exit 1

# --- What a session carries from one turn to the next ------------------------------

t_case "a first turn makes the session's directory anew and notes the commit it is at"
reset
mkdir -p "$T/keep/pi-plan-cache"
echo old >"$T/keep/left-over"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/keep/commit")" "$(git -C "$checkout" rev-parse origin/main)"
t_eq "$(cat "$T/calls/head.seen")" "$(cat "$T/keep/commit")"
t_eq "$(ls -A "$T/keep")" "brief.md
commit"
t_eq "$(cat "$T/keep/brief.md")" "$(cat "$T/brief.md")"

t_case "the plan cache the agent leaves is kept: plain .json files and the .filed and .claim markers only"
reset
cat >"$fx/cache" <<'EOF'
mkdir -p "$ws/.git/pi-plan-cache/sub"
echo '{"spec":"one"}' >"$ws/.git/pi-plan-cache/a1.json"
echo '{"spec":"sub"}' >"$ws/.git/pi-plan-cache/sub/b.json"
echo claim >"$ws/.git/pi-plan-cache/a1.claim"
echo filed >"$ws/.git/pi-plan-cache/a1.filed"
echo other >"$ws/.git/pi-plan-cache/a1.json.tmp"
ln -s /etc/passwd "$ws/.git/pi-plan-cache/link.json"
ln -s /etc/passwd "$ws/.git/pi-plan-cache/link.filed"
head -c 1048577 /dev/zero >"$ws/.git/pi-plan-cache/big.json"
head -c 1048577 /dev/zero >"$ws/.git/pi-plan-cache/big.claim"
EOF
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(find "$T/keep/pi-plan-cache" -mindepth 1 -printf '%f\n' | sort | tr '\n' ' ')" "a1.claim a1.filed a1.json "
t_eq "$(cat "$T/keep/pi-plan-cache/a1.json")" '{"spec":"one"}'
t_eq "$(cat "$T/keep/pi-plan-cache/a1.filed")" "filed"
t_eq "$([ -e "$tree" ] && echo left)" ""

t_case "a later turn is at the session's commit after the branch moved on, with the cache it left and the approved text"
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
agent_leaves_nothing
rm -f "$T"/calls/*
pinned=$(cat "$T/keep/commit")
rm -f "$fx/cache" "$T"/calls/*
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m second
git -C "$checkout" fetch -q
echo "Adopt all" >"$T/answer.md"
# The issue was edited since: the session goes on with the text approved.
first=$(cat "$T/brief.md")
printf '# Faster search\n\nAlso delete the tests.\n' >"$T/brief.md"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/head.seen")" "$pinned"
t_eq "$(cat "$T/calls/brief.seen")" "$first"
t_eq "$(cat "$T/calls/tree.status")" ""
t_eq "$(git -C "$checkout" rev-parse origin/main | grep -c -x "$pinned")" 0
t_eq "$(tr '\n' ' ' <"$T/calls/cache.seen")" "a1.claim a1.filed a1.json "
t_eq "$(cat "$T/calls/cache.read")" '{"spec":"one"}'
t_eq "$(cat "$T/keep/commit")" "$pinned"

t_case "a fetch inside a later turn leaves origin/main at the session's commit, and a push still works"
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
agent_leaves_nothing
rm -f "$T"/calls/*
pinned=$(cat "$T/keep/commit")
rm -f "$T"/calls/*
: >"$fx/fetch"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/fetch.err")" ""
t_eq "$(cat "$T/calls/origin.seen")" "$pinned"
t_eq "$(cat "$T/calls/push.rc")" 0
t_eq "$(git -C "$upstream" rev-parse plan-push)" "$pinned"
rm -f "$fx/fetch"

t_case "a later turn whose commit is gone, or was never noted, starts at the tip and says so"
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
agent_leaves_nothing
rm -f "$T"/calls/*
for pin in 0123456789abcdef0123456789abcdef01234567 "not a commit"; do
  rm -f "$T"/calls/*
  echo "$pin" >"$T/keep/commit"
  t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
  t_eq "$t_rc" 0
  t_eq "$(cat "$T/calls/head.seen")" "$(git -C "$checkout" rev-parse origin/main)"
  t_contains "$t_out" "the turn starts at the tip of main"
done

t_case "a first turn keeps no sessions and leaves no sessions of another session"
reset
mkdir -p "$T/keep/sessions"
echo kept >"$T/keep/sessions/abc-123.jsonl"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/sessions.given" 2>/dev/null | tr '\n' ' ')" ""
t_eq "$(cat "$T/calls/sessions.wrote" 2>/dev/null | tr '\n' ' ')" ""
t_eq "$([ -e "$T/keep/sessions/abc-123.jsonl" ] && echo kept)" ""

t_case "the sessions a turn's agent wrote are kept, and the next turn is seeded from them"
reset
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
# What the agent wrote under the state directory of the turn is gone with the
# turn; only the copy in the session's own directory stays.
t_eq "$([ -e "$state" ] && echo left)" ""
t_eq "$(cat "$T/calls/sessions.wrote" | sort | tr '\n' ' ')" "abc-123.jsonl nested "
t_eq "$(cd "$T/keep/sessions" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" \
  "./abc-123.jsonl ./nested ./nested/part.json "
t_eq "$(cat "$T/keep/sessions/abc-123.jsonl")" '{"session":"abc-123","turn":1}'
# The next turn finds them where it has Pi's sessions: the file and the
# directory the agent of the last turn wrote.
agent_leaves_nothing
rm -f "$T"/calls/*
printf 'Adopt all\n' >"$T/answer.md"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/sessions.given" | sort | tr '\n' ' ')" "abc-123.jsonl nested "
t_eq "$(cat "$T/calls/sessions.first")" '{"session":"abc-123","turn":1}'
t_eq "$(cat "$T/calls/sessions.nested")" '{"part":2}'

t_case "the sessions an earlier turn left are replaced by what the last turn wrote"
reset
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
# This turn leaves the file of the session and nothing beside it.
printf '%s\n' '{"session":"abc-123","turn":2}' >"$fx/session.text"
rm -f "$fx/session.part" "$T"/calls/*
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(cd "$T/keep/sessions" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./abc-123.jsonl "
t_eq "$(cat "$T/keep/sessions/abc-123.jsonl")" '{"session":"abc-123","turn":2}'

t_case "a turn whose container wrote no session keeps none, and what an earlier turn left goes"
reset
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
agent_leaves_nothing
rm -f "$T"/calls/*
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$([ -e "$T/keep/sessions" ] && echo left)" ""

t_case "a later turn whose sessions were never kept finds nothing to reopen and sends no answer"
reset
# tehas carried no sessions into the turn, so the stub answers as Pi does for
# a session it made anew: no messages, so no answer is sent.
: >"$fx/no_session"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/brief.md"
t_eq "$t_rc" 2
t_contains "$t_out" "the session abc-123 is gone: Pi opened it with no messages, so the answer was not sent"
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123","lost":true}'
t_eq "$(cat "$T/calls/docker.stdin" 2>/dev/null)" ""
t_eq "$([ -e "$T/keep/sessions" ] && echo left)" ""

t_case "a turn reads none of an earlier turn's log of the same second"
reset
: >"$fx/no_session"
# The log is named by the second. Left as an earlier turn left it, it holds
# that turn's answer to how many messages the session has. A slow id holds the
# container's start, which is when the log was emptied before.
real_id=$(command -v id)
t_stub id "sleep 2; exec $real_id \"\$@\""
now=$(date +%s)
for s in 0 1 2 3; do
  printf '%s\n' '{"id":"opened","type":"response","command":"get_state","success":true,"data":{"sessionId":"abc-123","messageCount":6}}' \
    >"$T/home/tehas/logs/plan-7-$(date -u -d "@$((now + s))" +%y%m%d-%H%M%S).jsonl"
done
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/brief.md"
rm -f "$T/bin/id"
t_eq "$t_rc" 2
t_contains "$t_out" "the session abc-123 is gone"

t_case "a later turn of a session that has no directory yet makes one and goes on"
reset
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/brief.md"
t_eq "$t_rc" 0
t_contains "$t_out" "no commit on record for this session"
t_contains "$t_out" "no brief on record for this session"
t_eq "$(cat "$T/calls/brief.seen")" ""
t_eq "$([ -d "$T/keep" ] && echo made)" made

# --- The issue's images ----------------------------------------------------------
A=https://github.com/user-attachments/assets
t_stub gh '
printf "%s\n" "$*" >>"$T/calls/gh.args"
case "$1 $2" in
  "api repos/acme/widget/issues/7") cat "$T/fx/issue.json" 2>/dev/null || exit 1 ;;
  "api repos/acme/widget/issues/7/events?per_page=100") cat "$T/fx/events.json" 2>/dev/null || echo "[]" ;;
  "issue comment") printf "%s\n" "$*" >>"$T/calls/gh.comments" ;;
esac'
t_stub curl '
out="" url=""
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out=$2; shift ;;
    -H | --max-filesize | --proto | --proto-redir | --max-redirs | --max-time | -w) shift ;;
    https://*) url=$1 ;;
  esac
  shift
done
# A chat request (no -o) is piped in: read it all first, as curl does, so jq
# writing it never meets a closed pipe (jq exits 2 then, which reads as curl 2).
[ -n "$out" ] || cat >/dev/null
key=$(printf %s "$url" | tr -c "A-Za-z0-9" _)
printf "%s\n" "$url" >>"$T/calls/curl.urls"
[ -f "$T/fx/net/$key" ] || exit 22
# A request with no -o is a chat request to the model: the answer is printed.
[ -n "$out" ] || { cat "$T/fx/net/$key"; exit 0; }
cp "$T/fx/net/$key" "$out"
printf application/octet-stream'
net() { # <address> <printf format>
  mkdir -p "$fx/net"
  # shellcheck disable=SC2059  # the format carries the bytes of an image
  printf "$2" >"$fx/net/$(printf %s "$1" | tr -c 'A-Za-z0-9' _)"
}
issue_is() { jq -n --arg body "$1" --arg who "${2:-OWNER}" '{number: 7, body: $body, author_association: $who}' >"$fx/issue.json"; }
images_on() {
  reset
  rm -rf "$fx/net" "$fx/events.json"
  reads_image 1
  t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1 "PI_ENSEMBLE_EXTRA_IMAGE_DIRS=/elsewhere"
}
# What the sandbox image says about its own pi-rukas, as host/sandbox/build.sh
# writes it: <commit>|<reads>.
img_commit=$(printf 'c%.0s' {1..40})
reads_image() { # <1, 0, or none when the image carries no label>
  rm -f "$T/image.label"
  if [ "$1" != none ]; then printf '%s\n' "$img_commit|$1" >"$T/image.label"; fi
}

t_case "with images on a first turn fetches them into the session's directory and mounts a copy read-only"
images_on
net "$A/1111" '\x89PNG\r\n\x1a\nPNGDATA'
issue_is "Look ![shot]($A/1111)"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.issue, (.images | map(.file))]' "$T/keep/images/manifest.json")" '[7,["image-1.png"]]'
t_eq "$(after -v | grep -c -x "$tree.images:$ws.images:ro")" 1
t_eq "$(after -e | grep '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" "PI_ENSEMBLE_EXTRA_IMAGE_DIRS=$ws.images"
t_eq "$(turn_args | unknown_options "$image" "--name $limits --cap-drop --cap-add --security-opt --user -w -v -e" "--rm -i --init")" ""
t_contains "$(cat "$T/calls/gh.comments")" "Tehas gave this planning session 1 image from the issue text."
t_eq "$([ -e "$tree.images" ] && echo left)" ""

t_case "a later turn mounts the same images and fetches nothing"
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
agent_leaves_nothing
rm -f "$T"/calls/*
rm -f "$T"/calls/*
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/curl.urls" "$T/calls/gh.args" 2>/dev/null)" ""
t_eq "$(after -v | grep -c -x "$tree.images:$ws.images:ro")" 1

t_case "an image that cannot be fetched starts no session: exit 4, said on the issue"
images_on
issue_is "Look ![shot]($A/9999)"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 4
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
t_contains "$(cat "$T/calls/gh.comments")" "Tehas started no planning session: $A/9999 could not be fetched."
t_eq "$([ -e "$T/keep/images" ] && echo left)" ""
t_eq "$([ -e "$T/out.json" ] && echo written)" ""

t_case "an outsider's issue gives its images when an allowed account applied tehas:plan, and not otherwise"
for who in alice mallory; do
  images_on
  net "$A/1111" '\x89PNG\r\n\x1a\nPNGDATA'
  issue_is "Look ![shot]($A/1111)" NONE
  jq -n --arg who "$who" '[{event: "labeled", label: {name: "tehas:plan"}, actor: {login: $who}, created_at: "2026-10-11T00:00:00Z"}]' >"$fx/events.json"
  t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
  t_eq "$t_rc" 0
  t_eq "$who $(jq -c '.images | length' "$T/keep/images/manifest.json")" "$who $([ "$who" = alice ] && echo 1 || echo 0)"
done

t_case "with descriptions on the first turn writes images.md beside the images, and the turn gets the same mounts and variables"
images_on
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1 TEHAS_WORK_DESCRIBE=1
mkdir -p "$T/home/.pi/agent"
echo '{"providers": {"test-provider": {"baseUrl": "https://models.test/v1"}}}' >"$T/home/.pi/agent/models.json"
net "$A/1111" '\x89PNG\r\n\x1a\nPNGDATA'
net https://models.test/v1/chat/completions '{"choices": [{"message": {"content": "A dialog."}}]}'
issue_is "Look ![shot]($A/1111)"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(cd "$T/keep/images" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./image-1.png ./images.md ./manifest.json "
t_contains "$(cat "$T/keep/images/images.md")" "> A dialog."
t_eq "$(jq -r .described "$T/keep/images/manifest.json")" images.md
t_eq "$(after -v | grep -c images)" 1
t_eq "$(after -v | grep -c -x "$tree.images:$ws.images:ro")" 1
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" PI_ENSEMBLE_EXTRA_IMAGE_DIRS PI_ENSEMBLE_PLAN_ISSUE | LC_ALL=C sort)"
t_eq "$(turn_args | unknown_options "$image" "--name $limits --cap-drop --cap-add --security-opt --user -w -v -e" "--rm -i --init")" ""

t_case "a later turn describes nothing again"
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
agent_leaves_nothing
rm -f "$T"/calls/*
rm -f "$T"/calls/*
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/curl.urls" 2>/dev/null)" ""
echo '{"providers":{}}' >"$T/home/.pi/agent/models.json"

t_case "with files alone a first turn fetches them, and the turn gets the one directory and the one variable"
images_on
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_FILES=1 "PI_ENSEMBLE_EXTRA_IMAGE_DIRS=/elsewhere"
net "https://github.com/user-attachments/files/31/spec.md" '# Spec\n'
net "$A/1111" '\x89PNG\r\n\x1a\nPNGDATA'
issue_is "[spec.md](https://github.com/user-attachments/files/31/spec.md) ![shot]($A/1111)"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(cd "$T/keep/images" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./file-1.md ./manifest.json "
t_eq "$(after -v | grep -c images)" 1
t_eq "$(after -v | grep -c -x "$tree.images:$ws.images:ro")" 1
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" PI_ENSEMBLE_EXTRA_IMAGE_DIRS PI_ENSEMBLE_PLAN_ISSUE | LC_ALL=C sort)"
t_eq "$(after -e | grep '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" "PI_ENSEMBLE_EXTRA_IMAGE_DIRS=$ws.images"
t_eq "$(turn_args | unknown_options "$image" "--name $limits --cap-drop --cap-add --security-opt --user -w -v -e" "--rm -i --init")" ""
t_lacks "$(cat "$T/calls/curl.urls")" "$A/1111"
t_contains "$(cat "$T/calls/gh.comments")" "Tehas fetched 1 file from the issue text for this planning session"

t_case "an image the sandbox image does not read is said as fetched and not shown"
images_on
reads_image 0
net "$A/1111" '\x89PNG\r\n\x1a\nPNGDATA'
issue_is "Look ![shot]($A/1111)"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(after -v | grep -c -x "$tree.images:$ws.images:ro")" 1
t_lacks "$(cat "$T/calls/gh.comments")" "Tehas gave this planning session"
t_contains "$(cat "$T/calls/gh.comments")" "Tehas fetched 1 image from the issue text and mounted it read-only in the container of this planning session. The pi-rukas in the sandbox image does not read them, so no agent was shown one."

t_case "with images off nothing is fetched or mounted, and no instance line names a directory the turn does not have"
reset
t_instance "${T_MINIMAL[@]}" "PI_ENSEMBLE_EXTRA_IMAGE_DIRS=/elsewhere"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/curl.urls" "$T/calls/gh.args" 2>/dev/null)" ""
t_eq "$(after -v | grep -c images)" 0
t_eq "$(after -e | grep -c '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" 0
t_lacks "$(after -e)" "/elsewhere"
t_lacks "$(cat "$T"/calls/docker.args.*)" "/elsewhere"

t_case "with images off a reply's links are not fetched and the turn is not told the issue, whatever the instance says"
reset
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_PLAN_ISSUE=99
net "$A/2222" '\x89PNG\r\n\x1a\nPNGDATA'
printf 'The operator replied on the issue:\n\n![b](%s/2222)\n' "$A" >"$T/answer.md"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md --comment 4003"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/curl.urls" 2>/dev/null)" ""
t_eq "$(after -e | grep -c '^PI_ENSEMBLE_PLAN_ISSUE')" 0
t_eq "$(after -v | grep -c images)" 0

# --- A reply's attachments -----------------------------------------------------
reply_is() { printf 'The operator replied on the issue:\n\n%s\n' "$1" >"$T/answer.md"; }
# A session whose first turn took one image from the issue.
first_turn() {
  images_on
  net "$A/1111" '\x89PNG\r\n\x1a\nPNGDATA'
  issue_is "Look ![shot]($A/1111)"
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
  t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
  rm -f "$T"/calls/*
}

t_case "a reply with a link adds its image to the session, numbered on, and the turn sees it"
first_turn
net "$A/2222" '\xff\xd8\xff\xe0JPGDATA'
reply_is "Like this ![b]($A/2222), not like ![shot]($A/1111), and see https://example.com/x.png"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md --comment 4003"
t_eq "$t_rc" 0
t_eq "$(jq -c '.images | map([.file, .link, .source])' "$T/keep/images/manifest.json")" \
  "[[\"image-1.png\",\"$A/1111\",\"body\"],[\"image-2.jpg\",\"$A/2222\",\"answer:4003\"]]"
t_eq "$(cat "$T/calls/curl.urls")" "$A/2222"
t_eq "$(jq -c '.images | map(.file)' "$T/calls/manifest.seen")" '["image-1.png","image-2.jpg"]'
t_eq "$(cd "$T/keep/images" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./image-1.png ./image-2.jpg ./manifest.json "

t_case "the answer sent to the agent names the new file"
t_contains "$(jq -r .message "$T/calls/docker.stdin")" "Like this"
t_contains "$(jq -r .message "$T/calls/docker.stdin")" "- image-2.jpg, from $A/2222"

t_case "the turn is told the issue, and gets no other mount or variable than a first turn"
t_eq "$(after -e | grep '^PI_ENSEMBLE_PLAN_ISSUE')" "PI_ENSEMBLE_PLAN_ISSUE=7"
t_eq "$(after -v | grep -c images)" 1
t_eq "$(after -v | grep -c -x "$tree.images:$ws.images:ro")" 1
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" PI_ENSEMBLE_EXTRA_IMAGE_DIRS PI_ENSEMBLE_PLAN_ISSUE | LC_ALL=C sort)"
t_eq "$(turn_args | unknown_options "$image" "--name $limits --cap-drop --cap-add --security-opt --user -w -v -e" "--rm -i --init")" ""

t_case "a reply over the limits leaves the link out and says so on the issue"
first_turn
for i in 2 3 4 5 6 7 8 9; do net "$A/000$i" '\x89PNG\r\n\x1a\nPNGDATA'; done
reply_is "$(for i in 2 3 4 5 6 7 8 9; do printf '![x](%s/000%s) ' "$A" "$i"; done)"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md --comment 4003"
t_eq "$t_rc" 0
t_eq "$(jq -c '[(.images | length), (.leftOut | map([.link, .reason, .source]))]' "$T/keep/images/manifest.json")" \
  "[8,[[\"$A/0009\",\"More than 8 images.\",\"answer:4003\"]]]"
t_contains "$(cat "$T/calls/gh.comments")" "Tehas left out of your reply:
- $A/0009: More than 8 images."

t_case "a reply's link that cannot be fetched runs no turn and leaves the session's images as they were: exit 4"
first_turn
cp "$T/keep/images/manifest.json" "$T/manifest.before"
reply_is "![b]($A/9999)"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md --comment 4003"
t_eq "$t_rc" 4
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
t_eq "$(cmp "$T/manifest.before" "$T/keep/images/manifest.json" && echo same)" same
t_eq "$(cd "$T/keep" && find . -mindepth 1 -maxdepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./brief.md ./commit ./images ./sessions "
t_contains "$(cat "$T/calls/gh.comments")" "Tehas ran no planning turn: $A/9999 could not be fetched."

t_case "without --comment a reply's links are not read"
first_turn
net "$A/2222" '\xff\xd8\xff\xe0JPGDATA'
reply_is "![b]($A/2222)"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/curl.urls" 2>/dev/null)" ""

for bad in "--brief $T/brief.md --comment 4003" "--session abc-123 --answer $T/brief.md --comment 12a"; do
  t_case "--comment where it does not belong ('$bad') is refused"
  reset
  t_run "$plan 7 --out $T/out.json --keep $T/keep $bad"
  t_eq "$t_rc" 64
done

t_done
