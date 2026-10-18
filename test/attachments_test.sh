#!/usr/bin/env bash
# Tests for lib/attachments.sh: which links count as attached files, the type
# table, and where the token goes when a file is fetched. A stub curl records
# its arguments and what it is given on standard input.
# shellcheck disable=SC2016  # stub bodies and snippets are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

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
t_run "$said 1"
t_eq "$(head -n 1 <<<"$t_out")" "Tehas gave this cycle 1 image from the issue text."
t_run "$said 0"
t_eq "$(head -n 1 <<<"$t_out")" "Tehas fetched 1 image from the issue text and mounted it read-only in the container of this cycle. The pi-rukas in the sandbox image does not read them, so no agent was shown one."
t_run "$said"
t_contains "$(head -n 1 <<<"$t_out")" "Tehas fetched 1 image from the issue text and mounted it read-only in the container of this cycle. The sandbox image does not say whether its pi-rukas reads them: rebuild it with"

pngsum=$(sha256sum "$T/png" | cut -d' ' -f1)
t_case "each image is recorded with its sha256, and the comment lists them"
t_eq "$(jq -r '.images[0].sha256' "$T/images/manifest.json")" "$pngsum"
t_run "$said 1"
t_eq "$t_out" "Tehas gave this cycle 1 image from the issue text.

- https://github.com/user-attachments/assets/1111: image-1.png, sha256 $pngsum"

t_case "without a record the manifest names no approval"
t_eq "$(jq -c .approval "$T/images/manifest.json")" null

# record <body>: an approval record of that body, with the one link.
record() {
  jq -n --arg sum "$(printf %s "$1" | sha256sum | cut -d' ' -f1)" \
    '{at: "2026-10-14T00:00:00Z", by: "alice", how: "label", body_sha256: $sum, links: ["https://github.com/user-attachments/assets/1111"]}' >"$T/record.json"
}
t_case "with a record whose hash is the body's, the record's links are fetched and the approval named"
outsider NONE
record "See ![shot](https://github.com/user-attachments/assets/1111)"
t_run "$images approved $T/record.json"
t_eq "$t_rc" 0
t_eq "$(taken)" '[["image-1.png"],[]]'
t_eq "$(jq -c .approval "$T/images/manifest.json")" '{"at":"2026-10-14T00:00:00Z","by":"alice","how":"label"}'

t_case "with a record of another body nothing is fetched, and the reason says the text changed"
rm -f "$T"/calls/*
record "See nothing"
t_run "$images approved $T/record.json; echo \"\$image_why\""
t_eq "$t_rc" 0
t_eq "$t_out" "the issue's text is not the text that was approved: it changed after the approval"
t_eq "$(t_calls curl)" ""

t_case "the same link of the same approval must give the same bytes"
record "See ![shot](https://github.com/user-attachments/assets/1111)"
t_run "$images approved $T/record.json"
cp "$T/images/manifest.json" "$T/kept.json"
changed="$lib; tehas_issue_images_changed $T/kept.json $T/images || { echo \"\$image_why\"; exit 1; }"
t_run "$changed"
t_eq "$t_rc$t_out" "0"
jq '.images[0].sha256 = "0000"' "$T/images/manifest.json" >"$T/kept.json"
t_run "$changed"
t_eq "$t_rc $t_out" "1 https://github.com/user-attachments/assets/1111 gave other bytes than when this approval was first fetched"
jq '.images[0].sha256 = "0000" | .approval.at = "2026-10-15T00:00:00Z"' "$T/images/manifest.json" >"$T/kept.json"
t_run "$changed"
t_eq "$t_rc$t_out" "0"
jq '.images[0].sha256 = "0000" | .approval = null' "$T/images/manifest.json" >"$T/kept.json"
t_run "$changed"
t_eq "$t_rc$t_out" "0"
rm -f "$T/kept.json"
t_run "$changed"
t_eq "$t_rc$t_out" "0"

t_case "the text types a cycle may be given, and no other"
t_run "$lib; for e in md txt log csv json yaml yml html css js; do tehas_text_type \$e; done | tr '\n' ' '"
t_eq "$t_out" "text/markdown text/plain text/plain text/csv application/json application/yaml application/yaml text/html text/css text/javascript "
for e in svg htm mjs pdf zip png ''; do
  t_run "$lib; tehas_text_type '$e'"
  t_eq "$e:$t_rc$t_out" "$e:1"
done

t_case "text is valid UTF-8 with no NUL byte"
printf 'caf\xc3\xa9\n' >"$T/ok.txt"
printf 'caf\xe9\n' >"$T/latin1.txt"
printf 'a\x00b' >"$T/nul.txt"
: >"$T/empty.txt"
t_run "$lib; for f in ok latin1 nul empty; do tehas_text_ok $T/\$f.txt && echo \$f; done"
t_eq "$t_out" "ok
empty"

t_case "the comment names images and files apart, and the hash of each"
mkdir -p "$T/both"
jq -n '{images: [{file: "image-1.png", link: "L1", sha256: "aa"}], files: [{file: "file-1.md", link: "L2", sha256: "bb"}, {file: "file-2.txt", link: "L3", sha256: "cc"}], leftOut: []}' >"$T/both/manifest.json"
t_run "$lib; tehas_issue_images_said $T/both cycle 0"
t_eq "$t_out" "Tehas fetched 1 image from the issue text and mounted it read-only in the container of this cycle. The pi-rukas in the sandbox image does not read them, so no agent was shown one. Tehas fetched 2 files from the issue text for this cycle and lists them in its manifest. The pi-rukas in the sandbox image does not read them yet: its agents are not shown them.

- L1: image-1.png, sha256 aa
- L2: file-1.md, sha256 bb
- L3: file-2.txt, sha256 cc"
jq '.images = []' "$T/both/manifest.json" >"$T/m" && mv "$T/m" "$T/both/manifest.json"
t_run "$lib; tehas_issue_images_said $T/both cycle"
t_eq "$(head -n 1 <<<"$t_out")" "Tehas fetched 2 files from the issue text for this cycle and lists them in its manifest. The pi-rukas in the sandbox image does not read them yet: its agents are not shown them."

t_case "a file of the same approval is held to its bytes as an image is"
jq '.approval = {at: "t", by: "alice", how: "label"}' "$T/both/manifest.json" >"$T/m" && mv "$T/m" "$T/both/manifest.json"
jq '.files[1].sha256 = "dd"' "$T/both/manifest.json" >"$T/kept.json"
t_run "$lib; tehas_issue_images_changed $T/kept.json $T/both || echo \"\$image_why\""
t_eq "$t_out" "L3 gave other bytes than when this approval was first fetched"

t_done
