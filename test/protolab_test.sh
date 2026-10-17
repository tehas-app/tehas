#!/usr/bin/env bash
# Tests for libexec/protolab against a stub gh and a stub curl: which tickets
# are at the prototype stage and whose label counts, which files of a ticket
# are taken, the 10 MB limit, what the page says, and that a ticket leaves the
# lab when it is closed or unlabelled, and how an attached zip is unpacked.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

protolab="$T_ROOT/libexec/protolab"
fx=$T/fx
site=$T/home/tehas/protolab
live=$T/home/tehas/live

# gh answers from fixture files.
t_stub gh '
case "$1 $2" in
  "auth token") ;;
  "api user")
    [ ! -e "$T/fx/user-fails" ] || exit 1
    cat "$T/fx/user.json" 2>/dev/null || echo "{\"login\":\"tehas-bot\"}" ;;
  "api repos/acme/widget/issues?"*) cat "$T/fx/issues.json" 2>/dev/null || exit 1 ;;
  "api repos/acme/widget/issues/"*)
    n=${2#repos/acme/widget/issues/}
    case "$n" in
      */events*) cat "$T/fx/events-${n%%/*}.json" 2>/dev/null || exit 1 ;;
      *) cat "$T/fx/comments-${n%%/*}.json" 2>/dev/null || exit 1 ;;
    esac ;;
esac'

# curl hands out $T/fx/net/<address, with _ for every other character>. A file
# <name>.type is the content type it prints, <name>.rc an exit status to fail
# with. What arrives on standard input (the header file) is kept per call.
t_stub curl '
out="" url="" max=0 header=0
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out=$2; shift ;;
    --max-filesize) max=$2; shift ;;
    -H) header=1; shift ;;
    https://*) url=$1 ;;
  esac
  shift
done
key=$(printf %s "$url" | tr -c "A-Za-z0-9" _)
[ "$header" = 1 ] && { printf "%s " "$url"; cat; } >>"$T/calls/curl-headers"
[ -f "$T/fx/net/$key.rc" ] && exit "$(cat "$T/fx/net/$key.rc")"
[ -f "$T/fx/net/$key" ] || exit 22
[ "$(stat -c %s "$T/fx/net/$key")" -le "$max" ] || exit 63
cp "$T/fx/net/$key" "$out"
cat "$T/fx/net/$key.type" 2>/dev/null
exit 0'

A=https://github.com/user-attachments/assets
F=https://github.com/user-attachments/files

# net <address> <content> [content type]: something GitHub would hand out.
net() {
  local key
  key=$(printf %s "$1" | tr -c 'A-Za-z0-9' _)
  printf '%s' "$2" >"$fx/net/$key"
  [ -z "${3:-}" ] || printf '%s' "$3" >"$fx/net/$key.type"
}
# net_file <address> <path>: the same, from a file, for content that is binary.
net_file() { cp "$2" "$fx/net/$(printf %s "$1" | tr -c 'A-Za-z0-9' _)"; }
# labeled <number> <account> [label] [time]: the issue's label events, one
# page. With no account the event names nobody. Call it after issue, which
# writes a labelling by the allowed account.
labeled() {
  jq -n -c --arg who "${2:-}" --arg l "${3:-tehas:prototype}" --arg at "${4:-2026-10-09T08:00:00Z}" '
    [{event: "labeled", label: {name: $l}, created_at: $at,
      actor: (if $who == "" then null else {login: $who} end)}]' >"$fx/events-$1.json"
}
# issue <number> <title> <body> [association] [comments] [updated]
# Every ticket is at the prototype stage at the hand of an allowed account,
# unless a case writes its own events.
issue() {
  jq -n -c --argjson n "$1" --arg title "$2" --arg body "$3" --arg who "${4:-OWNER}" \
    --argjson comments "${5:-0}" --arg updated "${6:-2026-10-09T08:00:00Z}" '
    {number: $n, title: $title, body: $body, html_url: "https://github.com/acme/widget/issues/\($n)",
     user: {login: "alice"}, author_association: $who, updated_at: $updated, comments: $comments}'
  labeled "$1" alice
}
issues() { printf '%s\n' "$@" | jq -s . >"$fx/issues.json"; }
reset() {
  rm -rf "$fx" "$T/home/tehas" "$T"/calls/*
  mkdir -p "$fx/net"
  T_ENV=(GH_TOKEN=sekret-token)
  t_instance "${T_MINIMAL[@]}"
}
files_of() { (cd "$site/$1/files" 2>/dev/null && find . -type f | LC_ALL=C sort | tr '\n' ' '); }

t_case "invalid settings: exit 78 before GitHub is asked"
reset
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$protolab"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)" ""

t_case "a ticket's page has its title, the link to GitHub and its files"
reset
net "$A/11111111-2222-3333-4444-555555555555" "PNGDATA" "image/png"
net "$F/21/proto.html" "<h1>proto</h1>"
net "$F/22/style.css" "h1{}"
net "$A/66666666-2222-3333-4444-555555555555" "MP4DATA" "video/mp4"
issues "$(issue 7 'Map <script>alert(1)</script> & pins' \
  "Look: ![shot]($A/11111111-2222-3333-4444-555555555555)
[proto.html]($F/21/proto.html) [style.css]($F/22/style.css) [all.tar]($F/23/all.tar)
and https://example.com/not-ours.png" OWNER 2)"
jq -n --arg video "$A/66666666-2222-3333-4444-555555555555" --arg other "$F/31/stranger.html" '[
  {author_association: "COLLABORATOR", body: $video},
  {author_association: "NONE", body: "[stranger.html](\($other))"}]' >"$fx/comments-7.json"
t_run "$protolab"
t_eq "$t_rc" 0
t_contains "$t_out" "#7: 4 files, 32 bytes, 1 left out"
t_eq "$(files_of 7)" "./image-1.png ./proto.html ./style.css ./video-2.mp4 "
page=$(cat "$site/7/index.html")
t_contains "$page" "Map &lt;script&gt;alert(1)&lt;/script&gt; &amp; pins"
t_lacks "$page" "<script"
t_contains "$page" 'href="https://github.com/acme/widget/issues/7"'
t_contains "$page" '<img src="files/image-1.png"'
t_contains "$page" '<video src="files/video-2.mp4"'
t_contains "$page" '<iframe src="files/proto.html"'
t_contains "$page" 'sandbox="allow-scripts allow-forms allow-modals allow-pointer-lock"'
t_contains "$page" "<span>style.css</span>"
t_contains "$page" "4 FILES, 32 B OF 10 MB"

t_case "a type the lab does not host and a link from outside are left out, and said"
t_contains "$page" "<span>all.tar</span><span class=\"asset-fact\">Not a type the lab hosts.</span>"
t_contains "$page" "1 link from outside the repository"
t_lacks "$(t_calls curl)" "all.tar"
t_lacks "$(t_calls curl)" "stranger.html"
t_lacks "$(t_calls curl)" "example.com"

t_case "the token goes to github.com on standard input, never on the command line"
t_lacks "$(t_calls curl)" "sekret-token"
t_contains "$(cat "$T/calls/curl-headers")" "$F/21/proto.html Authorization: Bearer sekret-token"

t_case "the list page and the dashboard's file name the ticket"
t_contains "$(cat "$site/index.html")" '<a class="row" href="/7/"><b>#7</b>'
t_contains "$(cat "$site/index.html")" "1 PROTOTYPE<"
t_eq "$(jq -c '.tickets | map([.number, .files, .bytes, .url])' "$live/protolab.json")" \
  '[[7,4,32,"https://github.com/acme/widget/issues/7"]]'
t_eq "$(jq -r '[.url, .limitBytes] | @tsv' "$live/protolab.json")" "	10485760"

t_case "nothing is left behind that is not a page or a file"
t_eq "$(cd "$site" && find . -mindepth 1 ! -path './7/files/*' | LC_ALL=C sort | tr '\n' ' ')" \
  "./7 ./7/.meta.json ./7/.stamp ./7/files ./7/index.html ./index.html "

t_case "an unchanged ticket is not fetched again"
rm -f "$T"/calls/*
t_run "$protolab"
t_eq "$t_rc" 0
t_eq "$t_out" ""
t_eq "$(t_calls curl)" ""
t_eq "$(t_calls gh | grep -c comments)" 0

t_case "a changed ticket is built again"
issues "$(issue 7 'Map and pins' "![shot]($A/11111111-2222-3333-4444-555555555555)" OWNER 0 2026-10-09T09:00:00Z)"
t_run "$protolab"
t_contains "$t_out" "#7: 1 files, 7 bytes, 0 left out"
t_eq "$(files_of 7)" "./image-1.png "
t_contains "$(cat "$site/7/index.html")" "Map and pins"

t_case "the public address of the lab is passed on to the dashboard"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_PROTOLAB_URL=https://lab.example
issues
t_run "$protolab"
t_eq "$(jq -r .url "$live/protolab.json")" "https://lab.example"
t_contains "$(cat "$site/index.html")" "Nothing in the lab."

t_case "text by someone outside the repository brings nothing in"
reset
net "$F/40/theirs.html" "<p>theirs</p>"
issues "$(issue 8 'From a stranger' "[theirs.html]($F/40/theirs.html)" NONE)"
t_run "$protolab"
t_eq "$(files_of 8)" ""
t_eq "$(t_calls curl)" ""
t_contains "$(cat "$site/8/index.html")" "No image, video or HTML file is attached"
t_contains "$(cat "$site/8/index.html")" "1 link from outside the repository"

t_case "a ticket whose prototype label came from an account outside the list gets no page"
reset
net "$F/91/idea.html" "<p>idea</p>"
issues "$(issue 20 'Idea the bot labelled' "[idea.html]($F/91/idea.html)")"
labeled 20 tehas-bot
t_run "$protolab"
t_contains "$t_out" "#20: tehas:prototype was applied by 'tehas-bot', not an allowed account"
t_eq "$([ -e "$site/20" ] && echo there)" ""
t_eq "$(t_calls curl)" ""
t_eq "$(jq -c '.tickets | map(.number)' "$live/protolab.json")" "[]"

t_case "a page goes when the label was applied by an account outside the list"
reset
issues "$(issue 21 'Kept then taken' 'no files')"
labeled 21 alice
t_run "$protolab"
t_contains "$t_out" "#21: 0 files, 0 bytes, 0 left out"
issues "$(issue 21 'Kept then taken' 'no files' OWNER 0 2026-10-09T11:00:00Z)"
labeled 21 mallory
t_run "$protolab"
t_contains "$t_out" "#21: tehas:prototype was applied by 'mallory', not an allowed account"
t_contains "$t_out" "#21: no longer at the prototype stage, removed"
t_eq "$([ -e "$site/21" ] && echo there)" ""

t_case "the most recent labelling decides, whatever order its events arrive in"
reset
issues "$(issue 22 'Taken after a label' '')" "$(issue 23 'Approved after a stranger' '')"
printf '%s\n' \
  '[{"event":"labeled","label":{"name":"tehas:prototype"},"actor":{"login":"mallory"},"created_at":"2026-10-02T00:00:00Z"}]' \
  '[{"event":"labeled","label":{"name":"tehas:prototype"},"actor":{"login":"alice"},"created_at":"2026-10-01T00:00:00Z"}]' \
  >"$fx/events-22.json"
printf '%s\n' \
  '[{"event":"labeled","label":{"name":"tehas:prototype"},"actor":{"login":"alice"},"created_at":"2026-10-02T00:00:00Z"}]' \
  '[{"event":"labeled","label":{"name":"tehas:prototype"},"actor":{"login":"mallory"},"created_at":"2026-10-01T00:00:00Z"}]' \
  >"$fx/events-23.json"
t_run "$protolab"
t_contains "$t_out" "#22: tehas:prototype was applied by 'mallory', not an allowed account"
t_contains "$t_out" "#23: 0 files, 0 bytes, 0 left out"
t_contains "$(t_calls gh)" "api repos/acme/widget/issues/22/events?per_page=100 --paginate"
t_eq "$(jq -c '.tickets | map(.number)' "$live/protolab.json")" "[23]"

t_case "a labelling that names no account, and one that is not there, are both no approval"
reset
issues "$(issue 24 'Anonymous' '')" "$(issue 25 'No events' '')"
labeled 24 ""
echo '[]' >"$fx/events-25.json"
t_run "$protolab"
t_contains "$t_out" "#24: tehas:prototype was applied by 'unknown', not an allowed account"
t_contains "$t_out" "#25: tehas:prototype was applied by 'unknown', not an allowed account"
t_eq "$(cd "$site" && find . -maxdepth 1 -type d ! -name . | LC_ALL=C sort | tr '\n' ' ')" ""

t_case "label events that cannot be read leave the ticket's page as it was"
reset
issues "$(issue 26 'Held' 'no files')"
labeled 26 alice
t_run "$protolab"
before=$(cd "$site/26" && find . -type f | LC_ALL=C sort | xargs cksum)
issues "$(issue 26 'Held' 'no files' OWNER 0 2026-10-09T12:00:00Z)"
rm -f "$fx/events-26.json"
t_run "$protolab"
t_contains "$t_out" "#26: its label events could not be read, nothing built or removed"
t_eq "$([ -e "$site/26" ] && echo there)" "there"
t_eq "$(cd "$site/26" && find . -type f | LC_ALL=C sort | xargs cksum)" "$before"
t_contains "$(cat "$site/index.html")" '<b>#26</b>'
t_contains "$(jq -c '.tickets | map(.number)' "$live/protolab.json")" "[26]"

t_case "events that are an answer of GitHub rather than a list of events are unreadable ones"
reset
issues "$(issue 27 'Held' 'no files')"
t_run "$protolab"
before=$(cd "$site/27" && find . -type f | LC_ALL=C sort | xargs cksum)
issues "$(issue 27 'Held' 'no files' OWNER 0 2026-10-09T13:00:00Z)" "$(issue 28 'Never built' 'no files')"
echo '{"message":"Bad credentials"}' >"$fx/events-27.json"
echo '{"event":"labeled","label":{"name":"tehas:prototype"},"actor":{"login":"alice"}}' >"$fx/events-28.json"
t_run "$protolab"
t_eq "$t_rc" 0
t_contains "$t_out" "#27: its label events could not be read, nothing built or removed"
t_contains "$t_out" "#28: its label events could not be read, nothing built or removed"
t_eq "$(cd "$site/27" && find . -type f | LC_ALL=C sort | xargs cksum)" "$before"
t_eq "$([ -e "$site/28" ] && echo there)" ""
t_eq "$(jq -c '.tickets | map(.number)' "$live/protolab.json")" "[27]"

t_case "an instance that lists the bot's own login publishes nothing"
reset
net "$F/93/idea.html" "<p>idea</p>"
issues "$(issue 30 'Labelled by the bot' "[idea.html]($F/93/idea.html)")"
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ALLOWED_LABELERS='alice tehas-bot'" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$protolab"
t_eq "$t_rc" 78
t_contains "$t_out" "the bot account tehas-bot is in TEHAS_ALLOWED_LABELERS"
t_contains "$(t_calls gh)" "api user"
t_eq "$(cd "$site" && find . -maxdepth 1 -type d ! -name . | LC_ALL=C sort | tr '\n' ' ')" ""
t_eq "$(t_calls curl)" ""

t_case "a pass that cannot ask GitHub for its own login builds nothing, even where the list names the bot"
reset
net "$F/94/idea.html" "<p>idea</p>"
issues "$(issue 31 'Labelled by an owner' "[idea.html]($F/94/idea.html)")"
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ALLOWED_LABELERS='alice tehas-bot'" >"$TEHAS_INSTANCE_DIR/instance.env"
: >"$fx/user-fails"
t_run "$protolab"
t_eq "$t_rc" 0
t_contains "$t_out" "the bot's own login could not be read, nothing built or removed"
t_eq "$(t_calls gh)" "api user"
t_eq "$(cd "$site" && find . -maxdepth 1 -type d ! -name . | LC_ALL=C sort | tr '\n' ' ')" ""
t_eq "$(t_calls curl)" ""

t_case "an own login GitHub answers without one leaves every page as it was"
reset
net "$F/95/idea.html" "<p>idea</p>"
issues "$(issue 32 'Kept' "[idea.html]($F/95/idea.html)")"
t_run "$protolab"
before=$(cd "$site/32" && find . -type f | LC_ALL=C sort | xargs cksum)
echo '{"message":"Bad credentials"}' >"$fx/user.json"
t_run "$protolab"
t_eq "$t_rc" 0
t_contains "$t_out" "the bot's own login could not be read, nothing built or removed"
t_eq "$(cd "$site/32" && find . -type f | LC_ALL=C sort | xargs cksum)" "$before"
t_eq "$(jq -c '.tickets | map(.number)' "$live/protolab.json")" "[32]"

t_case "10 megabytes a ticket: what does not fit is left out, what still fits is taken"
reset
for n in 1 2 3; do net "$F/5$n/part$n.html" ""; done
truncate -s 6M "$fx/net/$(printf %s "$F/51/part1.html" | tr -c 'A-Za-z0-9' _)"
truncate -s 6M "$fx/net/$(printf %s "$F/52/part2.html" | tr -c 'A-Za-z0-9' _)"
truncate -s 1M "$fx/net/$(printf %s "$F/53/part3.html" | tr -c 'A-Za-z0-9' _)"
issues "$(issue 9 'Big' "$F/51/part1.html $F/52/part2.html $F/53/part3.html")"
t_run "$protolab"
t_eq "$(files_of 9)" "./part1.html ./part3.html "
t_contains "$(cat "$site/9/index.html")" "<span>part2.html</span><span class=\"asset-fact\">Over the 10 MB limit of a ticket.</span>"
t_contains "$(cat "$site/9/index.html")" "2 FILES, 7 MB OF 10 MB"
t_eq "$(($(du -sb "$site/9/files" | cut -f1) <= 10 * 1024 * 1024 + 8192))" 1

t_case "a download that answers more than it promised is still held to the limit"
reset
net "$F/61/first.html" ""
net "$F/62/late.html" ""
truncate -s 9M "$fx/net/$(printf %s "$F/61/first.html" | tr -c 'A-Za-z0-9' _)"
truncate -s 2M "$fx/net/$(printf %s "$F/62/late.html" | tr -c 'A-Za-z0-9' _)"
t_stub curl '
while [ $# -gt 0 ]; do
  case "$1" in -o) out=$2; shift ;; https://*) url=$1 ;; esac
  shift
done
cp "$T/fx/net/$(printf %s "$url" | tr -c "A-Za-z0-9" _)" "$out"'
issues "$(issue 10 'Unbounded' "$F/61/first.html $F/62/late.html")"
t_run "$protolab"
t_eq "$(files_of 10)" "./first.html "
t_contains "$(cat "$site/10/index.html")" "<span>late.html</span><span class=\"asset-fact\">Over the 10 MB limit of a ticket.</span>"

t_case "a name cannot leave the ticket's directory, and two files may share a name"
reset
net "$F/71/..%2F..%2Fevil.html" "<p>evil</p>"
net "$F/72/same.html" "<p>one</p>"
net "$F/73/same.html" "<p>two</p>"
t_stub curl '
while [ $# -gt 0 ]; do
  case "$1" in -o) out=$2; shift ;; https://*) url=$1 ;; esac
  shift
done
cp "$T/fx/net/$(printf %s "$url" | tr -c "A-Za-z0-9" _)" "$out" 2>/dev/null || exit 22'
issues "$(issue 11 'Names' "$F/71/..%2F..%2Fevil.html $F/72/same.html $F/73/same.html")"
t_run "$protolab"
t_eq "$(files_of 11)" "./2-same.html ./2F..-2Fevil.html ./same.html "
t_eq "$(find "$T/home" -name '*evil*' | wc -l)" 1

t_case "a download that fails for now is tried again on the next run"
reset
net "$F/81/slow.html" "<p>slow</p>"
echo 28 >"$fx/net/$(printf %s "$F/81/slow.html" | tr -c 'A-Za-z0-9' _).rc"
t_stub curl '
while [ $# -gt 0 ]; do
  case "$1" in -o) out=$2; shift ;; https://*) url=$1 ;; esac
  shift
done
key=$(printf %s "$url" | tr -c "A-Za-z0-9" _)
[ -f "$T/fx/net/$key.rc" ] && exit "$(cat "$T/fx/net/$key.rc")"
cp "$T/fx/net/$key" "$out"'
issues "$(issue 12 'Slow' "$F/81/slow.html")"
t_run "$protolab"
t_contains "$(cat "$site/12/index.html")" "Could not be fetched. The lab tries again."
t_eq "$([ -e "$site/12/.stamp" ] && echo stamped)" ""
rm -f "$fx"/net/*.rc
t_run "$protolab"
t_eq "$(files_of 12)" "./slow.html "
t_eq "$([ -e "$site/12/.stamp" ] && echo stamped)" "stamped"

t_case "a ticket that is closed or unlabelled leaves the lab; a pull request never enters"
issues "$(issue 13 'Stays' '')" "$(issue 14 'A pull request' '' | jq -c '. + {pull_request: {}}')"
t_run "$protolab"
t_contains "$t_out" "#12: no longer at the prototype stage, removed"
t_eq "$(cd "$site" && find . -maxdepth 1 -type d ! -name . | LC_ALL=C sort | tr '\n' ' ')" "./13 "
t_eq "$(jq -c '.tickets | map(.number)' "$live/protolab.json")" "[13]"

t_case "GitHub down or answering oddly: everything is left as it is"
before=$(cd "$site" && find . -type f | LC_ALL=C sort | xargs cksum)
rm -f "$fx/issues.json"
t_run "$protolab"
t_eq "$t_rc" 0
echo '{"message":"Bad credentials"}' >"$fx/issues.json"
t_run "$protolab"
t_eq "$t_rc" 0
t_eq "$(cd "$site" && find . -type f | LC_ALL=C sort | xargs cksum)" "$before"

t_case "comments that cannot be read leave the ticket's page as it was"
issues "$(issue 13 'Stays' '' OWNER 3 2026-10-09T10:00:00Z)"
t_run "$protolab"
t_contains "$t_out" "#13: could not be read, left as it was"
t_eq "$(cd "$site" && find . -type f | LC_ALL=C sort | xargs cksum)" "$before"

t_case "without a domain a ticket is linked by path, as before"
reset
issues "$(issue 7 'Map' 'no files')"
t_run "$protolab"
t_contains "$(cat "$site/index.html")" 'href="/7/"'
t_contains "$(cat "$site/7/index.html")" '<a href="/">ALL PROTOTYPES</a>'
t_eq "$(jq -c '.tickets[0].address' "$live/protolab.json")" "null"

t_case "with a domain a ticket is linked by its own name, and its page links back to the list"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_DOMAIN=example.org
issues "$(issue 7 'Map' 'no files')"
t_run "$protolab"
t_contains "$(cat "$site/index.html")" 'href="https://lab-7.example.org/"'
t_lacks "$(cat "$site/index.html")" 'href="/7/"'
t_contains "$(cat "$site/7/index.html")" '<a href="https://lab.example.org/">ALL PROTOTYPES</a>'
t_eq "$(jq -r '.tickets[0].address' "$live/protolab.json")" "https://lab-7.example.org/"
# The jq programs put the name together themselves, because jq cannot call a
# shell function. It is the name tehas_edge_name makes.
t_run '. "$T_ROOT/lib/config.sh"; tehas_load && tehas_edge_name lab 7'
t_eq "$(jq -r '.tickets[0].address' "$live/protolab.json")" "https://$t_out/"
t_contains "$(cat "$site/index.html")" "href=\"https://$t_out/\""
t_eq "$(jq -r '.url' "$live/protolab.json")" "https://lab.example.org"

t_case "a domain that changes rebuilds a ticket that did not"
t_instance "${T_MINIMAL[@]}" TEHAS_DOMAIN=example.net
t_run "$protolab"
t_contains "$(cat "$site/7/index.html")" 'https://lab.example.net/'

zips=$T_ROOT/test/fixtures/protolab
# The page's line for one file left out.
left_out() { printf '<span>%s</span><span class="asset-fact">%s</span>' "$1" "$2"; }

t_case "a zip is unpacked: its HTML is framed and the files beside it are served"
reset
net_file "$F/1/good.zip" "$zips/good.zip"
issues "$(issue 40 'A zip' "[good.zip]($F/1/good.zip)")"
t_run "$protolab"
t_eq "$t_rc" 0
t_contains "$t_out" "#40: 4 files"
t_eq "$(files_of 40)" "./good/app.js ./good/data/sample.json ./good/index.html ./good/prototype.md "
page=$(cat "$site/40/index.html")
t_contains "$page" '<iframe src="files/good/index.html"'
uses=${page#*FILES THE HTML USES}
uses=${uses%%</ul>*}
t_contains "$uses" "<span>good/app.js</span>"
t_contains "$uses" 'href="files/good/data/sample.json"'
t_contains "$uses" "<span>good/prototype.md</span>"
t_contains "$page" "$(left_out good/notes.pdf 'Not a type the lab hosts.')"
t_eq "$(find "$site" -name '*.zip' -o -name '.part' | wc -l)" 0
t_eq "$(stat -c %a "$site/40/files/good/data/sample.json")" 644
t_eq "$(cat "$site/40/files/good/data/sample.json")" '{"name": "sample"}'

t_case "a zip cannot write outside its folder, and links in it are left out"
reset
net_file "$F/2/evil.zip" "$zips/evil.zip"
issues "$(issue 41 'Evil' "[evil.zip]($F/2/evil.zip)")"
t_run "$protolab"
t_eq "$t_rc" 0
t_eq "$(find "$T" \( -name escape.txt -o -name abs.txt -o -name link \) ! -path '*/files/evil/*' | wc -l)" 0
t_eq "$(files_of 41)" "./evil/ok.txt "
page=$(cat "$site/41/index.html")
t_contains "$page" "$(left_out evil/../escape.txt 'A path outside the archive.')"
t_contains "$page" "$(left_out evil//abs.txt 'A path outside the archive.')"
t_contains "$page" "$(left_out evil/link 'A link or a special file.')"

t_case "a zip is held to the ticket's 10 MB by what it unpacks to"
reset
net_file "$F/3/bomb.zip" "$zips/bomb.zip"
issues "$(issue 42 'Bomb' "[bomb.zip]($F/3/bomb.zip)")"
t_run "$protolab"
t_eq "$(find "$T/home" -name big.txt | wc -l)" 0
t_contains "$(cat "$site/42/index.html")" "$(left_out bomb/big.txt 'Over the 10 MB limit of a ticket.')"
t_eq "$(($(du -sb "$site/42" | cut -f1) < 1024 * 1024))" 1

t_case "a zip counts every file against the 40 a ticket"
reset
net_file "$F/4/many.zip" "$zips/many.zip"
issues "$(issue 43 'Many' "[many.zip]($F/4/many.zip)")"
t_run "$protolab"
t_eq "$(find "$site/43/files" -type f | wc -l)" 40
t_contains "$(cat "$site/43/index.html")" "40 FILES,"
t_eq "$(grep -o 'More than 40 files.' "$site/43/index.html" | wc -l)" 5
t_contains "$(cat "$site/43/index.html")" "$(left_out many/f45.txt 'More than 40 files.')"

t_case "a zip after other files has only what is left of the 40"
reset
net "$F/5/first.html" "<p>first</p>"
net_file "$F/6/many.zip" "$zips/many.zip"
issues "$(issue 44 'Many after one' "[first.html]($F/5/first.html) [many.zip]($F/6/many.zip)")"
t_run "$protolab"
t_eq "$(find "$site/44/files" -type f | wc -l)" 40
t_eq "$(grep -o 'More than 40 files.' "$site/44/index.html" | wc -l)" 6

t_case "a file that is not a zip, named .zip, is left out and said"
reset
net "$F/7/fake.zip" "not a zip at all"
issues "$(issue 45 'Fake' "[fake.zip]($F/7/fake.zip)")"
t_run "$protolab"
t_eq "$t_rc" 0
t_contains "$(cat "$site/45/index.html")" "$(left_out fake.zip 'Not a zip the lab can read.')"
t_eq "$(cd "$site/45" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" \
  "./.meta.json ./.stamp ./files ./index.html "

t_case "a name in a zip is taken literally, never as a pattern or an option"
reset
net_file "$F/8/odd.zip" "$zips/odd.zip"
issues "$(issue 46 'Odd' "[odd.zip]($F/8/odd.zip)")"
t_run "$protolab"
t_eq "$(files_of 46)" "./odd/1/2/3/4/5/ok.txt ./odd/a-1-.txt ./odd/a1.txt ./odd/q-.txt ./odd/qq.txt ./odd/st-r.txt ./odd/x.txt "
t_eq "$(cat "$site/46/files/odd/a-1-.txt")" "content of a[1].txt"
t_eq "$(cat "$site/46/files/odd/a1.txt")" "content of a1.txt"
t_eq "$(cat "$site/46/files/odd/q-.txt")" "content of q?.txt"
t_eq "$(cat "$site/46/files/odd/st-r.txt")" "content of st*r.txt"
t_eq "$(cat "$site/46/files/odd/x.txt")" "content of -x.txt"

t_case "a zip path with a backslash or more than six levels is left out"
page=$(cat "$site/46/index.html")
t_contains "$page" "$(left_out 'odd/back\slash.txt' 'A path outside the archive.')"
t_contains "$page" "$(left_out odd/1/2/3/4/5/6/deep.txt 'A path outside the archive.')"

t_case "a zip whose name is taken gets a folder of its own"
reset
net_file "$F/9/good.zip" "$zips/good.zip"
net_file "$F/10/good.zip" "$zips/good.zip"
issues "$(issue 47 'Twice' "[good.zip]($F/9/good.zip) [good.zip]($F/10/good.zip)")"
t_run "$protolab"
t_contains "$(cat "$site/47/index.html")" '<iframe src="files/good/index.html"'
t_contains "$(cat "$site/47/index.html")" '<iframe src="files/2-good/index.html"'

t_case "a zip entry with no Unix file type is taken as a file"
reset
net_file "$F/13/edge.zip" "$zips/edge.zip"
issues "$(issue 49 'Edges' "[edge.zip]($F/13/edge.zip)")"
t_run "$protolab"
t_eq "$t_rc" 0
t_eq "$(files_of 49)" "./edge/a-b.txt ./edge/d.txt ./edge/plain.txt "
t_eq "$(cat "$site/49/files/edge/plain.txt")" "plain"
page=$(cat "$site/49/index.html")

t_case "a locked file in a zip is left out"
t_contains "$page" "$(left_out edge/locked.txt 'A locked file.')"

t_case "two names in a zip that clean to one: the second is left out"
t_contains "$page" "$(left_out edge/a-b.txt 'A second file of that name.')"
t_eq "$(cat "$site/49/files/edge/a-b.txt")" "first"

t_case "a file of a zip standing where another needs a folder leaves that one out"
t_contains "$page" "$(left_out edge/d.txt/inner.txt 'A name the lab cannot use.')"

t_case "a damaged file in a zip is left out and nothing of it kept"
t_contains "$page" "$(left_out edge/bad.txt 'Could not be unpacked.')"
t_eq "$(find "$site/49" -name 'bad*' | wc -l)" 0

t_case "a zip is read only up to 200 entries"
reset
net_file "$F/14/entries.zip" "$zips/entries.zip"
issues "$(issue 50 'Entries' "[entries.zip]($F/14/entries.zip)")"
t_run "$protolab"
page=$(cat "$site/50/index.html")
t_eq "$(find "$site/50/files" -type f | wc -l)" 40
t_eq "$(grep -o 'More than 40 files.' <<<"$page" | wc -l)" 160
t_eq "$(grep -o 'More than 200 entries in the zip.' <<<"$page" | wc -l)" 1
t_lacks "$page" "e201.txt"

t_case "small files of a zip together are held to what is left, and unpacking stops there"
reset
real_unzip=$(command -v unzip)
t_stub unzip "exec $real_unzip \"\$@\""
net_file "$F/15/sizes.zip" "$zips/sizes.zip"
issues "$(issue 51 'Sizes' "[sizes.zip]($F/15/sizes.zip)")"
t_run "$protolab"
page=$(cat "$site/51/index.html")
t_eq "$(files_of 51)" "./sizes/a.txt ./sizes/b.txt "
t_contains "$page" "$(left_out sizes/c.txt 'Over the 10 MB limit of a ticket.')"
t_contains "$page" "$(left_out sizes/d.txt 'Over the 10 MB limit of a ticket.')"
t_eq "$(t_calls unzip | wc -l)" 3
rm -f "$T/bin/unzip"

t_case "a line of the zip's listing that is not an entry is left out and said"
reset
t_stub zipinfo 'printf "%s\n" "Archive:  x.zip" "Zip file size: 1 bytes, number of entries: 1" "this is not an entry" "1 file, 1 bytes uncompressed, 1 bytes compressed:  0.0%"'
net_file "$F/16/good.zip" "$zips/good.zip"
issues "$(issue 52 'Odd listing' "[good.zip]($F/16/good.zip)")"
t_run "$protolab"
t_contains "$(cat "$site/52/index.html")" "$(left_out 'good/this is not an entry' 'Not an entry the lab can read.')"
t_eq "$(files_of 52)" ""
rm -f "$T/bin/zipinfo"

t_case "a name with no slash keeps the same address as before"
reset
net "$F/11/solo.html" "<h1>solo</h1>"
net "$F/12/solo.css" "h1{}"
issues "$(issue 48 'Solo' "[solo.html]($F/11/solo.html) [solo.css]($F/12/solo.css)")"
t_run "$protolab"
t_contains "$(cat "$site/48/index.html")" '<iframe src="files/solo.html"'
t_contains "$(cat "$site/48/index.html")" 'href="files/solo.css"'

t_done
