#!/usr/bin/env bash
# Tests for libexec/work: the images an issue links, fetched for the cycle,
# and their descriptions.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/work_lib.sh
. "$(dirname "$0")/work_lib.sh" || exit 1
stub_github

t_case "with images off GitHub is not asked, nothing is fetched and no directory is made"
images_on
t_instance "${T_MINIMAL[@]}"
issue_is "![shot]($A/1111)"
net "$A/1111" "$png"
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(t_calls gh)$(t_calls curl)" ""
t_eq "$(find "$imgs" -maxdepth 0 2>/dev/null | wc -l)" 0
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$mounts_off" | LC_ALL=C sort)"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"

t_case "a setting that is not 0 or 1 starts nothing and asks nobody"
images_on
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=yes
t_run "$work 7"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)$(t_calls curl)$(t_calls docker)" ""
t_eq "$(find "$T/home/tehas/work" 2>/dev/null | wc -l)" 0

t_case "with images on the cycle gets one more mount, read-only, and one more variable"
images_on
net "$A/1111" "$png"
net "$F/21/side.JPG" "$jpg"
net "$A/2222" '<svg xmlns="http://www.w3.org/2000/svg"/>'
net "$F/24/drawing.svg" '<svg/>'
net "$A/3333" "$gif"
net "https://user-images.githubusercontent.com/9/old.webp" "$webp"
issue_is "Look: ![shot]($A/1111) and again ![shot]($A/1111)
[side.JPG]($F/21/side.JPG) [all.zip]($F/23/all.zip) [drawing.svg]($F/24/drawing.svg)
<img src=\"$A/2222\"> ![anim]($A/3333) https://user-images.githubusercontent.com/9/old.webp
and https://example.com/not-ours.png"
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$mounts_off" "$imgs:$inside.images:ro" | LC_ALL=C sort)"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" PI_ENSEMBLE_EXTRA_IMAGE_DIRS | LC_ALL=C sort)"
t_eq "$(after -e | grep '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" "PI_ENSEMBLE_EXTRA_IMAGE_DIRS=$inside.images"
t_eq "$(cycle_args | unknown_options "$image" \
  "--name --cgroup-parent --memory --memory-swap --memory-reservation --cpus --pids-limit --cpu-shares --cap-drop --cap-add --security-opt --tmpfs --user --cidfile -w -v -e" "--rm -i --init")" ""

t_case "the push check's container is given no image and not the variable"
t_eq "$(preflight_args | awk 'prev == "-v" { print } { prev = $0 }')" "$ws:$inside"
t_eq "$(preflight_args | awk 'prev == "-e" { print } { prev = $0 }' | sed 's/=.*//' | LC_ALL=C sort)" \
  "$(printf '%s\n' "$want" HOME | LC_ALL=C sort)"

t_case "nothing is made on the host at the images' path inside the container"
t_eq "$(find "$inside.images" -maxdepth 0 2>/dev/null | wc -l)" 0

t_case "png, jpg, gif and webp are taken in the order written, under names of tehas's own"
t_eq "$(cd "$imgs" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" \
  "./image-1.png ./image-2.jpg ./image-3.gif ./image-4.webp ./manifest.json "
t_eq "$(manifest '.issue')" 7
t_eq "$(manifest '.images | map([.file, .link, .type, .bytes])')" \
  "[[\"image-1.png\",\"$A/1111\",\"image/png\",15],[\"image-2.jpg\",\"$F/21/side.JPG\",\"image/jpeg\",11],[\"image-3.gif\",\"$A/3333\",\"image/gif\",13],[\"image-4.webp\",\"https://user-images.githubusercontent.com/9/old.webp\",\"image/webp\",16]]"
t_eq "$(cmp "$imgs/image-1.png" "$fx/net/$(printf %s "$A/1111" | tr -c 'A-Za-z0-9' _)" && echo same)" same

t_case "another type is left out and said, and one known by its name is not fetched"
t_eq "$(manifest '.leftOut | map([.link, .reason])')" \
  "[[\"$F/23/all.zip\",\"Not a png, jpg, webp or gif image.\"],[\"$F/24/drawing.svg\",\"Not a png, jpg, webp or gif image.\"],[\"$A/2222\",\"Not a png, jpg, webp or gif image.\"]]"
t_lacks "$(t_calls curl)" "all.zip"
t_lacks "$(t_calls curl)" "drawing.svg"
t_lacks "$(t_calls curl)" "example.com"

t_case "the token goes to github.com on standard input, never on the command line"
t_lacks "$(t_calls curl)" "test-token"
t_contains "$(cat "$T/calls/curl-headers")" "$A/1111 Authorization: Bearer test-token"
t_lacks "$(cat "$T/calls/curl-headers")" "githubusercontent"

t_case "the issue is told how many images the cycle was given and what was left out"
t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Tehas gave this cycle 4 images from the issue text."
t_contains "$(t_calls gh)" "- $F/23/all.zip: Not a png, jpg, webp or gif image."

t_case "the limit counts all images together: each fetch may take what is left"
t_eq "$(t_calls curl | grep -o -e '--max-filesize [0-9]*' | tr '\n' ' ')" \
  "--max-filesize 10485760 --max-filesize 10485745 --max-filesize 10485734 --max-filesize 10485734 --max-filesize 10485721 "

t_case "an image the sandbox image does not read is said as fetched and not shown, and still mounted"
images_on
reads_image 0
net "$A/1111" "$png"
issue_is "![shot]($A/1111)"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$(after -v)" "$imgs:$inside.images:ro"
t_eq "$(after -e | grep '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" "PI_ENSEMBLE_EXTRA_IMAGE_DIRS=$inside.images"
t_lacks "$(t_calls gh)" "Tehas gave this cycle"
t_contains "$(t_calls gh)" "Tehas fetched 1 image from the issue text and mounted it read-only in the container of this cycle. The pi-rukas in the sandbox image does not read them, so no agent was shown one."

# An image built before the label existed is not proof of anything, and tehas
# does not claim one way or the other.
t_case "a sandbox image that says nothing is not claimed to have shown the images"
images_on
reads_image none
net "$A/1111" "$png"
issue_is "![shot]($A/1111)"
t_run "$work 7"
t_eq "$t_rc" 2
t_lacks "$(t_calls gh)" "Tehas gave this cycle"
t_contains "$(t_calls gh)" "The sandbox image does not say whether its pi-rukas reads them: rebuild it with $T/home/tehas/sandbox/build.sh, which records the answer."

t_case "a sandbox image that reads them is said to have given them"
images_on
reads_image 1
net "$A/1111" "$png"
issue_is "![shot]($A/1111)"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Tehas gave this cycle 1 image from the issue text."

t_case "bytes that are not the image the name says are left out"
images_on
net "$F/21/shot.png" '<svg xmlns="http://www.w3.org/2000/svg"><script/></svg>'
net "$F/22/empty.png" ''
issue_is "[shot.png]($F/21/shot.png) [empty.png]($F/22/empty.png)"
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(manifest '[(.images | length), (.leftOut | map(.reason))]')" '[0,["Not a png, jpg, webp or gif image.","Not a png, jpg, webp or gif image."]]'
t_eq "$(cd "$imgs" && find . -mindepth 1 | tr '\n' ' ')" "./manifest.json "

t_case "an issue with no image still gets the mount, with an empty list, and no comment"
images_on
issue_is "Words only."
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(manifest '[.images, .leftOut]')" '[[],[]]'
t_contains "$(after -v)" "$imgs:$inside.images:ro"
t_lacks "$(t_calls gh)" "issue comment"

t_case "an issue by someone outside the repository is given no image, and nothing is fetched"
images_on
net "$A/1111" "$png"
issue_is "![shot]($A/1111)" NONE
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(t_calls curl)" ""
t_eq "$(manifest '[.images, (.leftOut | map([.link, .reason]))]')" \
  "[[],[[\"$A/1111\",\"The issue was not opened by an owner, member or collaborator, and its label was not applied by an allowed account.\"]]]"
t_contains "$(cycle_args | tr '\n' ' ')" "$runner work 7 --timeout 360 --json"


t_case "an outsider's issue that an allowed account labelled is given its images"
images_on
net "$A/1111" "$png"
issue_is "![shot]($A/1111)" NONE
labelled_by alice
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(manifest '[(.images | map(.link)), .leftOut]')" "[[\"$A/1111\"],[]]"
t_contains "$(after -v)" "$imgs:$inside.images:ro"
t_contains "$(t_calls gh)" "api repos/acme/widget/issues/7/events?per_page=100 --paginate"

t_case "one labelled last by an account outside the list is not, and nothing is fetched"
images_on
net "$A/1111" "$png"
issue_is "![shot]($A/1111)" NONE
labelled_by mallory
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(t_calls curl)" ""
t_eq "$(manifest '.images')" "[]"

t_case "events that cannot be read start no cycle: exit 4, said on the issue"
images_on
issue_is "![shot]($A/1111)" NONE
: >"$fx/events-fail"
t_run "$work 7"
t_eq "$t_rc" 4
t_contains "$t_out" "tehas work: the issue's label events could not be read for its images, not starting a cycle"
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
t_eq "$(t_calls curl)" ""
t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Tehas started no cycle: the issue's label events could not be read"

t_case "at most eight images: the ninth is left out and not fetched"
images_on
body=""
for i in 1 2 3 4 5 6 7 8 9; do
  net "$A/000$i" "$png"
  body="$body ![s]($A/000$i)"
done
issue_is "$body"
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(manifest '[(.images | length), (.leftOut | map([.link, .reason]))]')" "[8,[[\"$A/0009\",\"More than 8 images.\"]]]"
t_lacks "$(t_calls curl)" "$A/0009"

t_case "an image over what is left of the limit is left out, and the cycle starts"
images_on
net "$A/1111" "$png"
echo 63 >"$fx/net/$(printf %s "$A/1111" | tr -c 'A-Za-z0-9' _).rc"
issue_is "![big]($A/1111)"
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(manifest '.leftOut | map(.reason)')" '["Over the 10 MB limit of an issue."]'
t_contains "$(cycle_args | tr '\n' ' ')" "$runner work 7 --timeout 360 --json"

for rc in 22 6 28; do
  t_case "an image that cannot be fetched (curl $rc) starts no cycle: exit 4, said on the issue"
  images_on
  net "$A/1111" "$png"
  net "$A/2222" "$png"
  echo "$rc" >"$fx/net/$(printf %s "$A/2222" | tr -c 'A-Za-z0-9' _).rc"
  issue_is "![a]($A/1111) ![b]($A/2222)"
  t_run "$work 7"
  t_eq "$t_rc" 4
  t_contains "$t_out" "tehas work: $A/2222 could not be fetched, not starting a cycle"
  t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
  t_eq "$(find "$imgs" "$ws" -maxdepth 0 2>/dev/null | wc -l)" 0
  t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Tehas started no cycle: $A/2222 could not be fetched"
done

t_case "an issue GitHub does not hand out starts no cycle, and what the last cycle left stays where it is"
images_on
mkdir -p "$T/home/tehas/work" "$ws"
: >"$T/home/tehas/work/.tehas-workspaces"
echo unpushed >"$ws/kept.txt"
t_run "$work 7"
t_eq "$t_rc" 4
t_eq "$(cat "$ws/kept.txt")" unpushed
t_eq "$(find "$ws.before-retry" -maxdepth 0 2>/dev/null | wc -l)" 0
t_contains "$t_out" "tehas work: the issue could not be read for its images, not starting a cycle"
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
echo "not json" >"$fx/issue.json"
t_run "$work 7"
t_eq "$t_rc" 4
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0

t_case "a comment that cannot be posted does not stop the cycle"
images_on
net "$A/1111" "$png"
issue_is "![shot]($A/1111)"
: >"$fx/comment-fails"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$(cycle_args | tr '\n' ' ')" "$runner work 7 --timeout 360 --json"

t_case "the images are remade for every cycle: what an earlier one left is gone"
images_on
mkdir -p "$T/home/tehas/work" "$imgs"
: >"$T/home/tehas/work/.tehas-workspaces"
echo old >"$imgs/image-9.png"
issue_is "Words only."
t_run "$work 7"
t_eq "$(cd "$imgs" && find . -mindepth 1 | tr '\n' ' ')" "./manifest.json "

t_case "a path of the instance's own for the variable is replaced, not added to"
images_on PI_ENSEMBLE_EXTRA_IMAGE_DIRS=/elsewhere
issue_is "Words only."
t_run "$work 7"
t_eq "$(after -e | grep -c '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" 1
t_eq "$(after -e | grep '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" "PI_ENSEMBLE_EXTRA_IMAGE_DIRS=$inside.images"
t_lacks "$(cat "$T"/calls/docker.args.*)" "/elsewhere"

# --- The images described ------------------------------------------------------
chat=https://models.test/v1/chat/completions
describe_on() {
  images_on TEHAS_WORK_DESCRIBE=1
  mkdir -p "$T/home/.pi/agent"
  echo '{"providers": {"test-provider": {"baseUrl": "https://models.test/v1"}}}' >"$T/home/.pi/agent/models.json"
}

t_case "a description setting without images starts nothing: exit 78"
images_on
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_DESCRIBE=1
t_run "$work 7"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_WORK_DESCRIBE is 1 and TEHAS_WORK_IMAGES is not"
t_eq "$(t_calls gh)$(t_calls curl)$(t_calls docker)" ""

t_case "with descriptions on the directory holds exactly the images, the manifest and images.md"
describe_on
net "$A/1111" "$png"
net "$A/2222" "$jpg"
net "$chat" '{"choices": [{"message": {"content": "A chart with three bars."}}]}'
issue_is "![a]($A/1111) ![b]($A/2222)"
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(cd "$imgs" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./image-1.png ./image-2.jpg ./images.md ./manifest.json "
t_eq "$(grep -c '^> A chart with three bars.$' "$imgs/images.md")" 2
t_eq "$(manifest .described)" '"images.md"'
t_eq "$(cmp "$imgs/images.md" "$T/home/tehas/state/cycles/7.images.md" && echo same)" same
t_eq "$(jq -r .described "$kept")" images.md

t_case "with descriptions on the container gets the same mounts and variables as with images alone"
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$mounts_off" "$imgs:$inside.images:ro" | LC_ALL=C sort)"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" PI_ENSEMBLE_EXTRA_IMAGE_DIRS | LC_ALL=C sort)"
t_eq "$(cycle_args | unknown_options "$image" \
  "--name --cgroup-parent --memory --memory-swap --memory-reservation --cpus --pids-limit --cpu-shares --cap-drop --cap-add --security-opt --tmpfs --user --cidfile -w -v -e" "--rm -i --init")" ""

t_case "the model is asked once per image, with no token"
t_eq "$(t_calls curl | grep -c -e "$chat")" 2
t_lacks "$(grep -e "$chat" "$T/calls/curl-headers")" "Authorization"

t_case "a model that does not answer is said in the file, and the cycle starts"
describe_on
net "$A/1111" "$png"
issue_is "![a]($A/1111)"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$(cat "$imgs/images.md")" "> not described: the request to the model failed (curl 22)"
t_contains "$(cycle_args | tr '\n' ' ')" "$runner work 7 --timeout 360 --json"

t_case "with images alone nothing is described"
images_on
net "$A/1111" "$png"
issue_is "![a]($A/1111)"
t_run "$work 7"
t_eq "$(find "$imgs/images.md" 2>/dev/null | wc -l)" 0
t_lacks "$(t_calls curl)" "chat/completions"
t_eq "$(manifest .described)" null

t_done
