#!/usr/bin/env bash
# Tests for libexec/dispatch: issues that link images, and how holds are
# logged.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

mesh_curl

# The model as models.json declares it, with the input it takes.
declares() { # <input list as JSON>
  jq -n --argjson input "$1" '{providers: {"test-provider": {baseUrl: "http://mesh.test/v1",
    models: [{id: "other", input: ["text", "image"]}, {id: "test-model", input: $input}]}}}' >"$T/home/.pi/agent/models.json"
}

t_case "without images for a cycle the model is not asked to read one"
reset
mesh '{"data":[{"id":"test-model"}]}'
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_lacks "$(t_calls curl)" "chat/completions"

# An instance that gives a cycle images, and a model that offers what
# models.json declares for it.
imaging() { # <input list as JSON>
  reset
  t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1
  mesh '{"data":[{"id":"test-model"}]}'
  declares "$1"
  rm -f "$fx/reads-images"
}
# An issue whose text links an attachment in GitHub's storage: both the answer
# of the list the poll reads and the one of the text it approves, from which
# the start's links come.
pictured() { # <issue>
  jq -nc --argjson n "$1" '{number: $n, state: "open",
    body: "It looks like this:\n\n![shot](https://github.com/user-attachments/assets/0f3e-41)"}' >"$fx/work-$1.json"
  jq -nc '{data: {repository: {issue: {lastEditedAt: null, editor: null,
    body: "It looks like this:\n\n![shot](https://github.com/user-attachments/assets/0f3e-41)"}}}}' >"$fx/edit-$1.json"
}
# An issue whose text says a file goes there and links no attachment: the words
# an issue is left with when the file was never attached.
worded() { # <issue> [<the words of the gap>]
  jq -nc --argjson n "$1" --arg w "${2:-SCREENSHOT GOES HERE}" '{number: $n, state: "open",
    body: ("The screenshot is the place; this text deliberately does not say it. \($w).")}' >"$fx/work-$1.json"
}
# The same words with the file attached as well.
worded_pictured() { # <issue> [<the words of the gap>]
  jq -nc --argjson n "$1" --arg w "${2:-SCREENSHOT GOES HERE}" '{number: $n, state: "open",
    body: ("\($w):\n\n![shot](https://github.com/user-attachments/assets/0f3e-41)")}' >"$fx/work-$1.json"
}
image_asks() { t_calls curl | grep -c 'chat/completions'; }

t_case "with images for a cycle, an issue that links one starts when the model declares image input and answers one"
imaging '["text","image"]'
pictured 7
: >"$fx/reads-images"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(jq -r .why "$dispatch_json")" ""
t_eq "$(jq -c '[.model, .max_tokens, (.messages[0].content | map(.type))]' "$T/calls/curl-body")" '["test-model",1,["text","image_url"]]'
t_eq "$(jq -r '.messages[0].content[1].image_url.url' "$T/calls/curl-body" | cut -d, -f2 | base64 -d | head -c 8 | od -An -tx1 | tr -d ' \n')" "89504e470d0a1a0a"

t_case "an issue that links no image starts while the model cannot read one, and the model is not asked"
for input in '["text"]' '["text","image"]'; do
  imaging "$input"
  t_run "$dispatch"
  t_eq "$(t_calls fake-start)" "7"
  t_eq "$(image_asks)" 0
  t_eq "$(jq -r .why "$dispatch_json")" ""
  t_lacks "$(t_calls gh)" "images are off"
done

t_case "an issue that links an image waits while the model does not declare image input, and it is not asked"
for input in '["text"]' 'null'; do
  imaging "$input"
  pictured 7
  : >"$fx/reads-images"
  t_run "$dispatch"
  t_eq "$t_rc" 0
  t_eq "$(t_calls fake-start)" ""
  t_lacks "$(t_calls gh)" "issue edit"
  t_contains "$t_out" "hold #7: its text links an attachment and TEHAS_WORK_IMAGES is 1, but test-model does not declare image input in models.json"
  t_eq "$(image_asks)" 0
  t_eq "$(jq -r .why "$dispatch_json")" ""
done

t_case "an issue that links an image waits while the model does not answer an image"
imaging '["text","image"]'
pictured 7
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_contains "$t_out" "hold #7: its text links an attachment and TEHAS_WORK_IMAGES is 1, but test-model does not answer an image"
t_lacks "$t_out" "mesh.test"

t_case "an issue without images behind one that waits for them starts"
imaging '["text"]'
ready 7 8
pictured 7
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "8"
t_contains "$t_out" "hold #7:"

t_case "the model is asked to read an image once a poll"
imaging '["text","image"]'
slots 3 TEHAS_WORK_IMAGES=1
ready 7 8 9
pictured 7
pictured 8
t_run "$dispatch"
t_eq "$(image_asks)" 1
t_eq "$(t_calls fake-start)" "9"
t_contains "$t_out" "hold #8:"

t_case "with every slot taken no image request is sent"
imaging '["text","image"]'
slots 1 TEHAS_WORK_IMAGES=1
pictured 7
: >"$fx/reads-images"
hold 9
t_run "$dispatch"
t_eq "$(image_asks)" 0
t_eq "$(t_calls fake-start)" ""
t_eq "$(jq -r .why "$dispatch_json")" count
release

t_case "a retry of an issue that links an image waits for a model that reads one, and the tick stays"
parked
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1
mesh '{"data":[{"id":"test-model"}]}'
declares '["text"]'
comments 7 "$(park_body x ' ')" alice
pictured 7
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X PATCH"
t_contains "$t_out" "hold #7: its text links an attachment"

t_case "a planning session is not held by the image part of the check"
imaging '["text"]'
echo '[]' >"$fx/ready.json"
echo '[{"number":7,"labels":[{"name":"tehas:plan"}]}]' >"$fx/plans.json"
echo '[{"event":"labeled","label":{"name":"tehas:plan"},"actor":{"login":"alice"}}]' >"$fx/events-7.json"
pictured 7
t_run "$dispatch"
t_eq "$(t_calls fake-plan | wc -l)" 1
t_eq "$(image_asks)" 0

t_case "an issue whose text cannot be read for its links is not started"
imaging '["text","image"]'
echo '{"number":7,"state":"open","body":42}' >"$fx/work-7.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "ERROR: the text of #7 could not be read for its links"

# --- A file the text promises and no attachment answers for -------------------
# The hold asks nothing of GitHub, nothing of the model and nothing of the
# sandbox image, so it is the same poll on every instance: the setting off
# included, because then the cycle is given no file at all.

# An instance that gives a cycle nothing of what an issue attaches.
offline() {
  reset
  t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=0 TEHAS_WORK_FILES=0
  mesh '{"data":[{"id":"test-model"}]}'
}

t_case "an issue whose text says a screenshot goes there and links no attachment starts no cycle, and nothing on it changes"
for imaging_on in 1 0; do
  imaging '["text","image"]'
  [ "$imaging_on" = 1 ] || t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=0 TEHAS_WORK_FILES=0
  worded 7
  : >"$fx/reads-images"
  t_run "$dispatch"
  t_eq "$t_rc" 0
  t_eq "$(t_calls fake-start)" ""
  t_lacks "$(t_calls gh)" "issue edit"
  t_lacks "$(t_calls gh)" "issue comment"
  t_contains "$t_out" "hold #7: its text says SCREENSHOT GOES HERE and links no attachment, so the cycle would be given none"
  t_eq "$(jq -r .why "$dispatch_json")" ""
done

t_case "the same issue with the file attached starts"
imaging '["text","image"]'
worded_pictured 7
: >"$fx/reads-images"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "hold #7:"
t_eq "$(jq -r .why "$dispatch_json")" ""

t_case "an issue with no placeholder and no images starts"
imaging '["text","image"]'
: >"$fx/reads-images"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "hold #7:"

t_case "the hold needs neither images for a cycle nor a model that reads one"
offline
worded 7
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(image_asks)" 0
t_contains "$t_out" "hold #7: its text says SCREENSHOT GOES HERE and links no attachment"

for words in 'SCREENSHOT_GOES_HERE' 'the image goes here' 'IMAGE GOES HERE' 'the mockup goes here'; do
  t_case "the words '$words' of a body that links nothing hold the issue"
  offline
  worded 7 "$words"
  t_run "$dispatch"
  t_eq "$(t_calls fake-start)" ""
  t_contains "$t_out" "hold #7: its text says"
done

for words in 'the value goes here in the code' 'goes here' 'WORDS GOES HERE' 'the plan goes here, then the build' 'Nothing is missing.'; do
  t_case "'$words' is prose, not a missing file, and the issue starts"
  offline
  worded 7 "$words"
  t_run "$dispatch"
  t_eq "$(t_calls fake-start)" "7"
  t_lacks "$t_out" "hold #7:"
done

t_case "a retry held on its image holds that issue, not the poll: the next ready issue starts"
parked
slots 3 TEHAS_WORK_IMAGES=1
mesh '{"data":[{"id":"test-model"}]}'
declares '["text"]'
comments 7 "$(park_body x ' ')" alice
pictured 7
ready 8
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "8"
t_contains "$t_out" "hold #7: its text links an attachment"

# --- What each retry box carries -------------------------------------------
#
# A clean start carries what people wrote on the pull request after the park;
# the issue alone carries the issue and nothing else. The links a start names
# are what tehas work fetches, so an image linked only from a comment reaches
# the cycle by the clean start alone, and one the body links reaches both.

# An instance that reads comments, with a model that reads the images it is
# given. It is the first helper a case calls: it resets the fixtures, so the
# ones that name the pull request and the park are written after it.
commented() {
  imaging '["text","image"]'
  : >"$fx/reads-images"
}
# What a parked issue with a pull request of its own leaves for the poll: the
# bot's pull request of the cycle, one comment an allowed account wrote on it
# after the park, no other comment of the issue, an issue whose body links an
# image, and the instance that reads comments and takes images. Every helper
# of these files ends in t_instance, so the settings are written last.
reviewed() { # [<ticked box: clean | alone | old> [<login of the reviewer, alice by default>]]
  echo '[{"number":30,"headRefName":"feature/issue-7-search","body":""}]' >"$fx/prlist.json"
  icomment 71 "${2:-alice}" 2026-01-01T00:20:00Z 2026-01-01T00:20:00Z \
    "The button goes here: ![shot]($A/7171)" | jq -s . >"$fx/icomments-30.json"
  echo '[]' >"$fx/icomments-7.json"
  case "$1" in
    clean) comments 7 "$(park_body x ' ' ' ')" alice ;;
    alone) comments 7 "$(park_body ' ' x ' ')" alice ;;
    *) comments 7 "$(park_body x ' ')" alice ;;
  esac
  pictured 7
  t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1 TEHAS_WORK_COMMENTS=1
}

# A=https://github.com/user-attachments/assets is the storage the images of
# these cases link.
A=https://github.com/user-attachments/assets

# The park comment these cases tick is the one a cycle posts now, with all
# three boxes: a tick on either retry box starts the cycle under the same
# rules, and only the record says which of the two it was. The two starts are
# authorized alike and differ only in what that record names, which is what
# tehas work fetches for the cycle. A comment with only the two boxes a cycle
# posted before is decided here too.

# The comments of the issue itself are the park's question, so the REST list
# the poll reads for a clean start is named and empty: only the pull request's
# comments go into the record.
t_case "a start by the clean-start box carries the comment an allowed account wrote after the park"
commented
parked
reviewed clean
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "pr list --repo acme/widget --author tehas-bot --state all"
t_eq "$(at_mark '.comment_links')" "$(sorted "[{\"link\":\"$A/7171\",\"source\":\"pr-comment:71\",\"by\":\"alice\"}]")"
t_eq "$(at_mark '.retry_context | length')" 1
t_contains "$(at_mark '.retry_context[0].text')" "The button goes here"

t_case "a start by the issue-alone box names no comment link and no retry context"
commented
parked
reviewed alone
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_eq "$(at_mark .how)" 'retry-tick'
t_lacks "$(t_calls gh)" "pr list"
t_eq "$(at_mark '[has("comment_links"), has("retry_context")]')" '[false,false]'

t_case "both retries give the cycle the image the issue's own body links"
for ticked in clean alone; do
  commented
  parked
  reviewed "$ticked"
  t_run "$dispatch"
  t_eq "$(t_calls fake-start)" "7"
  t_eq "$(at_mark '.links')" "$(sorted "[\"$A/0f3e-41\"]")"
done

t_case "an issue-alone tick whose pull request cannot be listed still starts: nothing of it is read"
commented
parked
reviewed alone
: >"$fx/pr-fails"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "ERROR"

t_case "a clean-start tick whose pull request cannot be listed starts nothing, as it did"
commented
parked
reviewed clean
: >"$fx/pr-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "ERROR: the comments on the pull request of #7 could not be read"

t_case "a park of the old two boxes still gives the clean start its pull request's comments"
commented
parked
reviewed old
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "pr list --repo acme/widget --author tehas-bot --state all"
t_eq "$(at_mark '.comment_links')" "$(sorted "[{\"link\":\"$A/7171\",\"source\":\"pr-comment:71\",\"by\":\"alice\"}]")"
t_eq "$(at_mark '.retry_context | length')" 1

t_case "under the issue-alone box a start reads neither the issue's comments nor the pull request"
commented
parked
reviewed alone
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1 TEHAS_WORK_COMMENTS=0
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_lacks "$(t_calls gh)" "pr list"
t_eq "$(at_mark '[has("comment_links"), has("retry_context")]')" '[false,false]'

t_case "an issue that links an attachment waits for a model that reads one, under either retry box"
for ticked in clean alone; do
  # The shape of the case that holds a tick of an issue that links a file, for
  # both boxes: the hold comes before either start, so it asks for no pull
  # request and changes no label.
  parked
  t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1
  mesh '{"data":[{"id":"test-model"}]}'
  declares '["text"]'
  case "$ticked" in
    clean) comments 7 "$(park_body x ' ' ' ')" alice ;;
    *) comments 7 "$(park_body ' ' x ' ')" alice ;;
  esac
  pictured 7
  t_run "$dispatch"
  t_eq "$(t_calls fake-start)" ""
  t_lacks "$(t_calls gh)" "issue edit"
  t_lacks "$(t_calls gh)" "api -X PATCH"
  t_contains "$t_out" "hold #7: its text links an attachment"
done
t_case "an issue that links an attachment says nothing of a file it promises"
imaging '["text","image"]'
worded_pictured 7
: >"$fx/reads-images"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "its text says"

t_case "one issue held by the placeholder does not hold the next one back"
offline
slots 3 TEHAS_WORK_IMAGES=0 TEHAS_WORK_FILES=0
ready 7 8
worded 7
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "8"
t_contains "$t_out" "hold #7:"
t_lacks "$t_out" "hold #8:"

t_case "a retry of an issue whose text promises a file waits, and the tick stays"
parked
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1
mesh '{"data":[{"id":"test-model"}]}'
declares '["text","image"]'
comments 7 "$(park_body x ' ')" alice
worded 7
: >"$fx/reads-images"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X PATCH"
t_contains "$t_out" "hold #7: its text says SCREENSHOT GOES HERE"

t_case "a refresh of an issue whose text promises a file starts nothing"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1 TEHAS_REFRESH_CONFLICTS=1
mesh '{"data":[{"id":"test-model"}]}'
declares '["text","image"]'
: >"$fx/reads-images"
# The image's pi-rukas takes --continue-pr, so a refresh is asked for at all.
jq -nc '{verbs: {work: {flags: ["--json", "--timeout", "--restart", "--continue-pr"]}}, features: {continuePr: true}}' >"$fx/caps.json"
jq -n '{data: {repository: {issue: {lastEditedAt: null, editor: null, body: "The screenshot is the place. SCREENSHOT GOES HERE."}}}}' >"$fx/edit-7.json"
mkdir -p "$T/home/tehas/state/refresh" "$T/home/tehas/state/cycles"
jq -nc '{at: "2026-10-17T09:00:00Z", pr: 50,
  head_sha: "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1",
  base_sha: "b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1"}' >"$T/home/tehas/state/refresh/7.json"
past 7 alice label 2026-01-01T00:00:00Z
queue 7
echo '[]' >"$fx/ready.json"
echo '[{"number":7,"labels":[{"name":"tehas:awaiting-merge"}]}]' >"$fx/awaiting.json"
jq -nc '{number: 7, state: "open", labels: [{name: "tehas:awaiting-merge"}]}' >"$fx/work-7.json"
head_sha=$(printf 'a%.0s' {1..40})
base_sha=$(printf 'c%.0s' {1..40})
jq -n --arg head "$head_sha" --arg base "$base_sha" '{data: {repository: {pullRequest: {state: "OPEN", number: 50,
  url: "https://github.com/acme/widget/pull/50", isDraft: false, isCrossRepository: false,
  headRefOid: $head, baseRefName: "main", baseRefOid: $base, mergeable: "CONFLICTING",
  author: {login: "tehas-bot"}, headRepository: {nameWithOwner: "acme/widget"}, labels: {nodes: []},
  closingIssuesReferences: {totalCount: 1, nodes: [{number: 7, state: "OPEN",
    labels: {nodes: [{name: "tehas:awaiting-merge"}]},
    closedByPullRequestsReferences: {nodes: [{number: 50, state: "OPEN"}]}}]}}}}}' >"$fx/pull-50.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "--add-label tehas:running"
t_lacks "$(t_calls gh)" "--add-label tehas:refreshing"

t_case "a planning turn is not held by the promised file"
imaging '["text"]'
echo '[]' >"$fx/ready.json"
echo '[{"number":7,"labels":[{"name":"tehas:plan"}]}]' >"$fx/plans.json"
echo '[{"event":"labeled","label":{"name":"tehas:plan"},"actor":{"login":"alice"}}]' >"$fx/events-7.json"
worded 7
: >"$fx/reads-images"
t_run "$dispatch"
t_eq "$(t_calls fake-plan | wc -l)" 1
t_lacks "$t_out" "hold #7:"

t_case "a dry run says the hold and changes nothing"
offline
worded 7
t_run "$dispatch --dry-run"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_contains "$t_out" "hold #7: its text says SCREENSHOT GOES HERE and links no attachment"

# What the sandbox image says about its own pi-rukas, as its build writes it:
# <commit>|<reads>.
img_commit=$(printf 'c%.0s' {1..40})
image_says() { # <1, 0, unknown, or none when the image carries no label>
  rm -f "$fx/image.label"
  [ "$1" = none ] || printf '%s\n' "$img_commit|$1" >"$fx/image.label"
}

t_case "an issue that links an image waits while the sandbox image says its pi-rukas does not read them, and the model is not asked"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1
mesh '{"data":[{"id":"test-model"}]}'
declares '["text","image"]'
: >"$fx/reads-images"
pictured 7
image_says 0
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(image_asks)" 0
t_contains "$t_out" "hold #7: its text links an attachment and TEHAS_WORK_IMAGES is 1, but the pi-rukas in the sandbox image cccccccccccc does not read them"
t_eq "$(jq -r .why "$dispatch_json")" ""
image_says none

t_case "an issue that links no image starts while the sandbox image does not read them"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1
mesh '{"data":[{"id":"test-model"}]}'
declares '["text","image"]'
: >"$fx/reads-images"
image_says 0
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
image_says none

t_case "an image that reads them is held for nothing, and the model is still asked"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1
mesh '{"data":[{"id":"test-model"}]}'
declares '["text","image"]'
: >"$fx/reads-images"
pictured 7
image_says 1
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(jq -r .why "$dispatch_json")" ""
t_eq "$(image_asks)" 1
image_says none

t_case "an image that says nothing is not held for it, whatever its pi-rukas really reads"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1
mesh '{"data":[{"id":"test-model"}]}'
declares '["text","image"]'
: >"$fx/reads-images"
pictured 7
image_says unknown
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(jq -r .why "$dispatch_json")" ""
image_says none
rm -f "$fx/reads-images"

t_case "no resources is logged once, with nothing in line as well, and released when the endpoint is back"
reset
echo '[]' >"$fx/ready.json"
mesh
t_run "$dispatch"
t_eq "$(grep -c 'holding: no resources' <<<"$t_out")" 1
t_eq "$(jq -r .why "$dispatch_json")" resources
t_run "$dispatch"
t_lacks "$t_out" "holding:"
mesh '{"data":[{"id":"test-model"}]}'
t_run "$dispatch"
t_eq "$(grep -c 'released: cycles may start again' <<<"$t_out")" 1
t_eq "$(jq -r .why "$dispatch_json")" ""

t_case "no resources holds a planning session too, and its label stays"
reset
mesh
echo '[]' >"$fx/ready.json"
echo '[{"number":7,"labels":[{"name":"tehas:plan"}]}]' >"$fx/plans.json"
echo '[{"event":"labeled","label":{"name":"tehas:plan"},"actor":{"login":"alice"}}]' >"$fx/events-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-plan)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "a dry run says no resources and writes nothing"
reset
mesh
t_run "$dispatch --dry-run"
t_eq "$(grep -c 'holding: no resources: the model endpoint does not answer' <<<"$t_out")" 1
t_eq "$([ -e "$dispatch_json" ] && echo written)" ""
t_eq "$(t_calls fake-start)" ""

t_case "a missing slice is said before no resources"
reset
mesh
no_slice
t_run "$dispatch"
t_eq "$(jq -r .why "$dispatch_json")" slice
rm -rf "$T/home/.pi"

t_case "a poll with room and no hold before it logs nothing about the slice"
reset
t_run "$dispatch"
t_lacks "$t_out" "holding:"
t_lacks "$t_out" "released"
t_eq "$(jq -c '[.why, .count, .room]' "$dispatch_json")" '["",0,4]'

t_case "a hold whose numbers change between polls is logged once while nothing waits"
reset
echo '[]' >"$fx/ready.json"
other $((19 * gib + gib / 2))
t_run "$dispatch"
t_contains "$t_out" "holding: tehas.slice has no room"
other $((19 * gib + gib * 3 / 4))
t_run "$dispatch"
t_lacks "$t_out" "holding:"
t_eq "$(jq -r .committed "$dispatch_json")" "$((19 * gib + gib * 3 / 4))"

t_case "while an issue waits, the hold is logged on every poll"
reset
other $((19 * gib + gib / 2))
t_run "$dispatch"
t_contains "$t_out" "holding: tehas.slice has no room"
t_run "$dispatch"
t_eq "$(grep -c 'holding' <<<"$t_out")" 1

t_case "a hold whose reason changes is logged again"
slice_state 12.00
other $((19 * gib + gib / 2))
t_run "$dispatch"
t_contains "$t_out" "holding: tehas.slice is under memory pressure"
t_eq "$(jq -r .why "$dispatch_json")" pressure
no_slice
t_run "$dispatch"
t_contains "$t_out" "holding: no slice file on this host"
t_eq "$(jq -c '[.why, .room, .committed, .max]' "$dispatch_json")" '["slice",0,0,0]'

t_case "a full set of slots is a hold like any other: logged once, then released"
reset
hold 9
t_run "$dispatch"
t_eq "$(grep -c 'holding: 1 cycles are running (limit 1)' <<<"$t_out")" 1
t_eq "$(jq -c '[.why, .count]' "$dispatch_json")" '["count",1]'
t_run "$dispatch"
t_lacks "$t_out" "holding:"
release
t_run "$dispatch"
t_contains "$t_out" "released: cycles may start again"
t_eq "$(t_calls fake-start)" "7"

t_case "a dispatch.json that cannot be read does not stop the poll"
reset
mkdir -p "$T/home/tehas/live"
echo '{"time":' >"$dispatch_json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_eq "$(jq -r .why "$dispatch_json")" ""

t_case "a slice probe that hangs holds within its bound"
reset
: >"$fx/slice-hangs"
# The probe hangs for 120 seconds and TEHAS_SLICE_WAIT is 1. A loaded runner
# makes the poll slower, but not by a minute: a poll under 60 seconds was cut
# short by the bound, not by luck.
t_stub systemctl "${systemctl_stub/sleep 20/sleep 120}"
began=$SECONDS
T_ENV+=(TEHAS_SLICE_WAIT=1)
t_run "$dispatch"
unset 'T_ENV[-1]'
t_eq "$([ $((SECONDS - began)) -lt 60 ] && echo in time)" "in time"
t_eq "$t_rc" 0
t_contains "$t_out" "holding: systemctl gave no answer"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_stub systemctl "$systemctl_stub"

t_done
