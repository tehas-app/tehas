#!/usr/bin/env bash
# Tests for lib/attachments.sh: which links count as attached files, the type
# table, and where the token goes when a file is fetched. A stub curl records
# its arguments and what it is given on standard input.
# shellcheck disable=SC2016  # stub bodies and snippets are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

lib=". $T_ROOT/lib/attachments.sh"
t_stub curl 'prev=""
for a in "$@"; do
  [ "$a" != "@-" ] || { printf "stdin: "; cat; } >>"$T/calls/curl-stdin"
  [ "$prev" != -o ] || [ ! -e "$T/png" ] || cp "$T/png" "$a"
  prev=$a
done
printf image/png
exit "${CURL_RC:-0}"'

t_case "sourcing it prints nothing and writes nothing"
before=$(find "$T" | LC_ALL=C sort)
t_run "$lib"
t_eq "$t_rc$t_out" "0"
t_eq "$(find "$T" | LC_ALL=C sort)" "$before"

t_case "only links to GitHub's attachment storage are taken, in the order written"
text='![a](https://github.com/user-attachments/assets/1111-aa) see https://example.com/x.png
[b.png](https://github.com/user-attachments/files/21/b.png) <img src="https://github.com/acme/widget/assets/42/cc-dd">
https://user-images.githubusercontent.com/9/shot.png https://github.com/acme/widget/blob/main/x.png
http://github.com/user-attachments/assets/plain https://github.com.evil.example/user-attachments/assets/2222'
t_run "$lib; jq -rn --arg t '$text' \"\$(tehas_attachment_links) \"'\$t | links[]'"
t_eq "$t_out" "https://github.com/user-attachments/assets/1111-aa
https://github.com/user-attachments/files/21/b.png
https://github.com/acme/widget/assets/42/cc-dd
https://user-images.githubusercontent.com/9/shot.png"

t_case "the kind of a file follows its extension, and an unknown one fails"
t_run "$lib; for e in png jpg jpeg gif webp svg avif mp4 webm mov html htm css js mjs json txt; do tehas_attachment_kind \$e; done | tr '\n' ' '"
t_eq "$t_out" "image image image image image image image video video video app app part part part part part "
t_run "$lib; tehas_attachment_kind zip"
t_eq "$t_rc$t_out" "1"
t_run "$lib; tehas_attachment_kind ''"
t_eq "$t_rc$t_out" "1"

t_case "a content type gives an extension for an image or a video only"
t_run "$lib; for c in image/png image/jpeg 'image/gif; charset=binary' image/webp image/svg+xml image/avif video/mp4 video/webm video/quicktime; do tehas_attachment_ext \"\$c\"; done | tr '\n' ' '"
t_eq "$t_out" "png jpg gif webp svg avif mp4 webm mov "
t_run "$lib; tehas_attachment_ext text/html"
t_eq "$t_rc$t_out" "1"

t_case "a fetch is https only, limited in size and redirects, and prints the content type"
t_run "$lib; tehas_attachment_fetch https://github.com/user-attachments/assets/1 $T/out 1234 sekret-token"
t_eq "$t_rc $t_out" "0 image/png"
t_eq "$(t_calls curl)" "-sSfL --proto =https --proto-redir =https --max-redirs 3 --max-time 120 --max-filesize 1234 -o $T/out -w %{content_type} -H @- https://github.com/user-attachments/assets/1"

t_case "the token goes to github.com on standard input, never on the command line"
t_eq "$(cat "$T/calls/curl-stdin")" "stdin: Authorization: Bearer sekret-token"
t_lacks "$(t_calls curl)" "sekret-token"

t_case "no other host is sent the token, and without one none is sent"
rm -f "$T"/calls/*
t_run "$lib; tehas_attachment_fetch https://user-images.githubusercontent.com/9/shot.png $T/out 10 sekret-token
tehas_attachment_fetch https://github.com/user-attachments/assets/1 $T/out 10
tehas_attachment_fetch https://github.com/user-attachments/assets/1 $T/out 10 ''"
t_eq "$(find "$T/calls" -name curl-stdin | wc -l)" 0
t_lacks "$(t_calls curl)" "-H"

t_case "the exit status is curl's"
T_ENV=(CURL_RC=63)
t_run "$lib; tehas_attachment_fetch https://github.com/user-attachments/assets/1 $T/out 10 tok"
t_eq "$t_rc" 63
T_ENV=()

# --- The images an issue links -----------------------------------------------
# gh answers for issue 7 from a file; curl hands out a png.
t_stub gh 'case "$*" in "api repos/acme/widget/issues/7") cat "$T/issue.json" ;; esac'
printf '\x89PNG\r\n\x1a\nPNGDATA' >"$T/png"
outsider() { jq -n --arg who "$1" '{number: 7, author_association: $who,
  body: "See ![shot](https://github.com/user-attachments/assets/1111)"}' >"$T/issue.json"; }
images="TEHAS_REPO=acme/widget GH_TOKEN=tok; $lib; tehas_issue_images 7 $T/images"
taken() { jq -c '[(.images | map(.file)), (.leftOut | map(.reason))]' "$T/images/manifest.json"; }

t_case "an issue opened by someone outside the repository is given no image without an approval"
outsider NONE
rm -f "$T"/calls/*
t_run "$images"
t_eq "$t_rc" 0
t_eq "$(taken)" '[[],["The issue was not opened by an owner, member or collaborator, and its label was not applied by an allowed account."]]'
t_eq "$(t_calls curl)" ""

t_case "an approved text gives its images whoever opened the issue"
t_run "$images approved"
t_eq "$t_rc" 0
t_eq "$(taken)" '[["image-1.png"],[]]'
t_contains "$(t_calls curl)" "https://github.com/user-attachments/assets/1111"

t_case "only the word approved approves"
t_run "$images yes"
t_eq "$(taken)" '[[],["The issue was not opened by an owner, member or collaborator, and its label was not applied by an allowed account."]]'

t_case "an owner's issue gives its images without an approval, as before"
outsider OWNER
t_run "$images"
t_eq "$(taken)" '[["image-1.png"],[]]'

t_case "the comment says the images were given only when the sandbox image reads them"
said=". $T_ROOT/lib/attachments.sh; tehas_issue_images_said $T/images cycle"
t_run "$said"
t_eq "$t_out" "Tehas gave this cycle 1 image from the issue text."
t_run "$said yes"
t_eq "$t_out" "Tehas gave this cycle 1 image from the issue text."
t_run "$said no"
t_eq "$t_out" "Tehas fetched 1 image from the issue text for this cycle, but the pi-rukas in the sandbox image does not read them: its agents are not shown them."
t_run "$said unknown"
t_eq "$t_out" "Tehas gave this cycle 1 image from the issue text. Whether the pi-rukas in the sandbox image reads them could not be checked."

# --- Whether the sandbox image reads them ------------------------------------
t_stub docker 'case "$1 $2" in "image inspect") cat "$T/image.id" ;; *) exit "${READS_RC:-0}" ;; esac'
reader="TEHAS_HOME=$T/home/tehas; $lib; tehas_images_read some/image"

t_case "an id that is not an image id is not run, and the answer is unknown"
rm -f "$T"/calls/*
echo "sha256:../../etc" >"$T/image.id"
t_run "$reader"
t_eq "$t_rc $t_out" "0 unknown"
t_eq "$(t_calls docker)" "image inspect --format {{.Id}} some/image"
t_eq "$(jq -c '[.image, .reads]' "$T/home/tehas/state/images-read.json")" '["some/image",null]'

t_case "the image's answer is kept under its id"
rm -f "$T"/calls/*
echo "sha256:$(printf '%064d' 1)" >"$T/image.id"
T_ENV=(READS_RC=1)
t_run "$reader"
t_eq "$t_out" no
t_eq "$(cat "$T/home/tehas/state/images-read/$(printf '%064d' 1)")" no
t_eq "$(jq -c .reads "$T/home/tehas/state/images-read.json")" false
T_ENV=()

t_done
