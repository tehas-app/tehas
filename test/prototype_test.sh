#!/usr/bin/env bash
# Tests for tehas prototype file: a building feature that is 78 while off, and,
# turned on as a test build may, an issue opened and labelled tehas:prototype by
# the person's own gh, never with a token of the factory. gh is stubbed: it
# answers who is signed in from $T/fx/user.json, opens issue 42, and records
# whether any token name reached it.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

t_console || t_bad "$t_console_why"
T_ENV_ALWAYS=(TEHAS_CONSOLE="$T_CONSOLE")
cmd="$T_ROOT/bin/tehas prototype file"
th=$T/home/tehas
SUM_A=$(printf 'a%.0s' {1..64})
SUM_B=$(printf 'b%.0s' {1..64})
ATTACHED=https://github.com/user-attachments/files/7/proto.zip
SPEC=https://github.com/user-attachments/files/8/spec.md

gh_stub() {
  t_stub gh '
[ -z "${GH_TOKEN-}${GITHUB_TOKEN-}${TEHAS_GH_TOKEN-}" ] || echo "a token reached gh" >>"$T/calls/tokens"
case "$1 $2" in
  "api user") cat "$T/fx/user.json" ;;
  "api -X")
    case "$4" in
      repos/acme/widget/issues)
        [ ! -e "$T/fx/create-fails" ] || { echo "gh: Forbidden (HTTP 403)" >&2; exit 1; }
        typ=""
        case "$*" in *" -f type=Prototype"*) typ=", \"type\": {\"name\": \"Prototype\"}" ;; esac
        echo "{\"number\": 42, \"html_url\": \"https://github.com/acme/widget/issues/42\"$typ}" ;;
      repos/acme/widget/issues/42/labels)
        [ ! -e "$T/fx/label-fails" ] || { echo "gh: Forbidden (HTTP 403)" >&2; exit 1; }
        echo "[{\"name\": \"tehas:prototype\"}]" ;;
      *) exit 1 ;;
    esac ;;
  *) exit 1 ;;
esac'
}
user() { printf '{"login": "%s", "id": %s}\n' "$1" "${2:-$(t_uid "$1")}" >"$T/fx/user.json"; }

reset() {
  rm -rf "$th" "$T"/calls/* "$T/fx"
  mkdir -p "$T/fx"
  T_ENV=(TEHAS_TEST_FEATURES=prototype-file GH_TOKEN=ghs_app TEHAS_GH_TOKEN=ghs_poll)
  t_instance "${T_MINIMAL[@]}" TEHAS_PROTOTYPE_FILE=on "$@"
  t_ids "$th"
  gh_stub
  user alice
}

t_case "off, as a release has it, the command is 78 and calls nothing"
reset
t_instance "${T_MINIMAL[@]}"
T_ENV=()
t_run "$cmd --title T"
t_eq "$t_rc" 78
t_contains "$t_out" "prototype-file is off (TEHAS_PROTOTYPE_FILE) and still being built in this release"
t_eq "$(t_calls gh)" ""
t_run "$cmd --bogus"
t_eq "$t_rc" 78

t_case "switched on in a release, every command is 78 and names the feature"
reset
T_ENV=()
t_run "$cmd --title T"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_PROTOTYPE_FILE is on and prototype-file is still being built in this release"
t_eq "$(t_calls gh)" ""

t_case "the help text needs no settings"
t_run "$T_ROOT/bin/tehas help prototype"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas prototype file --title TEXT"

t_case "on in a test build: the issue is opened and labelled by the person's own gh"
reset
printf 'A page to try the new menu.\n' >"$T/text.md"
t_run "$cmd --title 'New menu' --body-file $T/text.md --zip $ATTACHED --zip-sha256 $SUM_A --spec $SPEC --spec-sha256 $SUM_B"
t_eq "$t_rc" 0
t_eq "$t_out" "filed #42 as alice with tehas:prototype: https://github.com/acme/widget/issues/42"
t_eq "$(t_calls gh | head -1)" "api user"
t_contains "$(t_calls gh)" "api -X POST repos/acme/widget/issues -f title=New menu -f body=A page to try the new menu.

Zip: $ATTACHED sha256:$SUM_A
Spec: $SPEC sha256:$SUM_B
"
t_eq "$(t_calls gh | tail -1)" "api -X POST repos/acme/widget/issues/42/labels -f labels[]=tehas:prototype"
t_eq "$(t_calls tokens)" ""

t_case "the text comes from standard input"
reset
t_run "printf 'From a pipe.\n' | $cmd --title T --body-file -"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "-f body=From a pipe.
"

t_case "a gh signed in as the bot is refused with 3 and nothing is filed"
reset
user 'tehas-bot[bot]'
t_run "$cmd --title T"
t_eq "$t_rc" 3
t_contains "$t_out" "gh is signed in as tehas-bot[bot], an app"
t_eq "$(t_calls gh)" "api user"

t_case "a login off the allowed list is refused with 3"
reset
user mallory
t_run "$cmd --title T"
t_eq "$t_rc" 3
t_contains "$t_out" "mallory is not in TEHAS_ALLOWED_LABELERS"
t_eq "$(t_calls gh)" "api user"

t_case "an allowed login with another user id is refused with 3"
reset
user alice 7
t_run "$cmd --title T"
t_eq "$t_rc" 3
t_contains "$t_out" "alice acted with the user id 7"
t_eq "$(t_calls gh)" "api user"

t_case "a gh that cannot say who is signed in is 1"
reset
rm -f "$T/fx/user.json"
t_run "$cmd --title T"
t_eq "$t_rc" 1
t_contains "$t_out" "your gh could not say who you are"

t_case "an issue opened but not labelled is named, to mark by hand"
reset
: >"$T/fx/label-fails"
t_run "$cmd --title T"
t_eq "$t_rc" 1
t_contains "$t_out" "#42 was opened (https://github.com/acme/widget/issues/42) but not marked"

t_case "an issue GitHub did not open is 1"
reset
: >"$T/fx/create-fails"
t_run "$cmd --title T"
t_eq "$t_rc" 1
t_contains "$t_out" "the issue was not opened"

t_case "with TEHAS_ISSUE_TYPES the issue is opened with the type and no label"
reset TEHAS_ISSUE_TYPES=prototype=Prototype
t_run "$cmd --title T"
t_eq "$t_rc" 0
t_eq "$t_out" "filed #42 as alice with the type Prototype: https://github.com/acme/widget/issues/42"
t_contains "$(t_calls gh)" "-f type=Prototype"
t_lacks "$(t_calls gh)" "labels"

t_case "a zip outside GitHub is 78 while protolab-zip-url is off"
reset TEHAS_PROTOLAB_ZIP_HOSTS=files.example.org
# Usable but off: while it is building, its hosts alone stop the load.
T_ENV=(TEHAS_TEST_FEATURES="prototype-file protolab-zip-url" GH_TOKEN=ghs_app TEHAS_GH_TOKEN=ghs_poll)
t_run "$cmd --title T --zip https://files.example.org/p.zip --zip-sha256 $SUM_A"
t_eq "$t_rc" 78
t_contains "$t_out" "a zip outside GitHub needs protolab-zip-url, which is off on this instance"
t_eq "$(t_calls gh)" ""

t_case "with protolab-zip-url on, a zip on a listed host is filed and one on another is 64"
reset TEHAS_PROTOLAB_ZIP_URL=on TEHAS_PROTOLAB_ZIP_HOSTS=files.example.org
T_ENV=(TEHAS_TEST_FEATURES="prototype-file protolab-zip-url")
t_run "$cmd --title T --zip https://files.example.org/p.zip --zip-sha256 $SUM_A"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "Zip: https://files.example.org/p.zip sha256:$SUM_A"
rm -f "$T/calls/gh"
t_run "$cmd --title T --zip https://other.example.org/p.zip --zip-sha256 $SUM_A"
t_eq "$t_rc" 64
t_eq "$(t_calls gh)" ""

t_case "bad words and a text that already has the line are 64 and call nothing"
reset
printf 'Spec: mine\n' >"$T/text.md"
t_run "$cmd --title T --body-file $T/text.md --spec $SPEC --spec-sha256 $SUM_B"
t_eq "$t_rc" 64
t_contains "$t_out" "the text has a \`Spec:\` line of its own"
t_run "$cmd --title T --zip $ATTACHED"
t_eq "$t_rc" 64
t_run "$cmd"
t_eq "$t_rc" 64
t_eq "$(t_calls gh)" ""

t_done
