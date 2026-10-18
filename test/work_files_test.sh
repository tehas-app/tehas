#!/usr/bin/env bash
# Tests for libexec/work: the text files an issue links, and an issue that
# links what the cycle is not given.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/work_lib.sh
. "$(dirname "$0")/work_lib.sh" || exit 1
stub_github

# --- An issue that links what the cycle is not given -------------------------
# With both settings off tehas work reads nothing of the issue. What the
# dispatcher's approval record lists is enough to say the gap, and it is a file
# this host already has.
images_off() {
  rm -rf "$fx" "$T"/calls/* "$T/home/tehas/work" "$T/home/tehas/logs" \
    "$T/home/tehas/state/cycles/7.manifest.json" "$T/home/tehas/state/cycles/7.approved.json"
  mkdir -p "$fx/net"
  slice_good
  reads_image 1
  T_ENV=("${env_ok[@]}")
  t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=0 TEHAS_WORK_FILES=0
}
gapline() { cat "$T"/home/tehas/logs/work-7-*.jsonl.err 2>/dev/null; }
kept_manifests() { find "$T/home/tehas/state/cycles" -name '7.manifest.json*' 2>/dev/null | wc -l; }

t_case "with nothing given, the links of the approval record are said and kept, and GitHub is not asked"
images_off
record_is alice "![shot]($A/1111) [spec.md]($F/31/spec.md)" "$A/1111" "$F/31/spec.md"
issue_is "![shot]($A/1111) [spec.md]($F/31/spec.md)" NONE
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(t_calls gh)$(t_calls curl)" ""
t_eq "$(find "$imgs" -maxdepth 0 2>/dev/null | wc -l)" 0
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$mounts_off" | LC_ALL=C sort)"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"
t_contains "$t_out" "tehas work: the approval record of issue 7 (approved 2026-10-14T00:00:00Z by alice) lists 2 attachments and this instance gives a cycle none"
t_eq "$(gapline)" "tehas work: the approval record of issue 7 (approved 2026-10-14T00:00:00Z by alice) lists 2 attachments and this instance gives a cycle none (TEHAS_WORK_IMAGES=0, TEHAS_WORK_FILES=0): no directory is mounted and PI_ENSEMBLE_EXTRA_IMAGE_DIRS is not set"
t_eq "$(jq -c '{images, approval}' "$kept")" '{"images":[],"approval":{"at":"2026-10-14T00:00:00Z","by":"alice","how":"label"}}'
t_eq "$(jq -c '[.leftOut[] | [.link, .source]]' "$kept")" "[[\"$A/1111\",\"body\"],[\"$F/31/spec.md\",\"body\"]]"
t_contains "$(jq -r '.leftOut[0].reason' "$kept")" "no images and no files"
t_eq "$(kept_manifests)" 1

t_case "with nothing given an instance line for the images variable is not passed on, as the line says"
images_off
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=0 TEHAS_WORK_FILES=0 PI_ENSEMBLE_EXTRA_IMAGE_DIRS=/elsewhere
record_is alice "![shot]($A/1111)" "$A/1111"
issue_is "![shot]($A/1111)" NONE
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(after -e | grep -c '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" 0
t_lacks "$(after -e)" "/elsewhere"
t_lacks "$(cat "$T"/calls/docker.args.*)" "/elsewhere"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"
t_contains "$(gapline)" "PI_ENSEMBLE_EXTRA_IMAGE_DIRS is not set"

t_case "one link is said as one attachment"
images_off
record_is alice "![shot]($A/1111)" "$A/1111"
issue_is "![shot]($A/1111)" NONE
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "lists 1 attachment and this instance gives a cycle none"
t_eq "$(jq '.leftOut | length' "$kept")" 1

t_case "the links named are the approved text's: what the body gained since is not counted, and GitHub is not asked"
images_off
record_is alice "![shot]($A/1111)" "$A/1111"
issue_is "![shot]($A/1111) ![added since]($A/2222)" NONE
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(gapline)" "tehas work: the approval record of issue 7 (approved 2026-10-14T00:00:00Z by alice) lists 1 attachment and this instance gives a cycle none (TEHAS_WORK_IMAGES=0, TEHAS_WORK_FILES=0): no directory is mounted and PI_ENSEMBLE_EXTRA_IMAGE_DIRS is not set"
t_lacks "$t_out" "the text of issue 7 links"
t_lacks "$(gapline)" "2222"
t_eq "$(jq -c '[.leftOut[].link]' "$kept")" "[\"$A/1111\"]"
t_eq "$(t_calls gh)$(t_calls curl)" ""

t_case "the manifest lists at most 100 links and says how many more the record lists"
images_off
links=() body=""
for i in $(seq 1 130); do
  links+=("$A/$i")
  body+="![x$i]($A/$i) "
done
record_is alice "$body" "${links[@]}"
issue_is "$body" NONE
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "lists 130 attachments and this instance gives a cycle none"
t_contains "$t_out" "its manifest lists the first 100 of them"
t_eq "$(jq '.leftOut | length' "$kept")" 100
t_eq "$(jq '.leftOutMore' "$kept")" 30
t_eq "$(jq -r '.leftOut[99].link' "$kept")" "$A/100"
t_lacks "$(jq -r '.leftOut[].link' "$kept")" "$A/101"
t_eq "$([ "$(wc -c <"$kept")" -lt 262144 ] && echo under)" "under"
t_contains "$(gapline)" "its manifest lists the first 100 of them"
t_lacks "$(gapline)" "could not write"

t_case "a manifest path that is a link is not written through, and the cycle starts"
images_off
record_is alice "![shot]($A/1111)" "$A/1111"
issue_is "![shot]($A/1111)" NONE
mkdir -p "$(dirname "$kept")"
printf '{"images": [], "leftOut": []}' >"$T/elsewhere"
ln -s "$T/elsewhere" "$kept"
t_run "$work 7"
t_eq "$t_rc" 2
t_lacks "$t_out" "gives a cycle none"
t_eq "$(jq -c . "$T/elsewhere")" '{"images":[],"leftOut":[]}'
t_eq "$(find "$kept.new" 2>/dev/null | wc -l)" 0

t_case "an issue never dispatched has no record, so nothing is said and nothing is written"
images_off
issue_is "![shot]($A/1111)" OWNER
t_run "$work 7"
t_eq "$t_rc" 2
t_lacks "$t_out" "gives a cycle none"
t_eq "$(gapline)" ""
t_eq "$(kept_manifests)" 0
t_eq "$(t_calls gh)$(t_calls curl)" ""

t_case "an issue that links nothing is given nothing and says nothing"
images_off
record_is alice "Words only."
issue_is "Words only." NONE
t_run "$work 7"
t_eq "$t_rc" 2
t_lacks "$t_out" "gives a cycle none"
t_eq "$(kept_manifests)" 0

t_case "a record that is not of the shape tehas work reads says so once, and the cycle starts"
images_off
mkdir -p "$(dirname "$record")"
echo '{"at": 1, "links": ["x"]}' >"$record"
issue_is "![shot]($A/1111)" NONE
t_run "$work 7"
t_eq "$t_rc" 2
t_lacks "$t_out" "gives a cycle none"
t_contains "$t_out" "the approval record of issue 7, or the manifest kept for it, could not be read"
t_eq "$(gapline)" "tehas work: the approval record of issue 7, or the manifest kept for it, could not be read: nothing is said about what this cycle was not given"
t_eq "$(kept_manifests)" 0

t_case "an approval that is not one line is not read, so the line cannot name the wrong approval"
images_off
mkdir -p "$(dirname "$record")"
jq -n '{at: "2026-10-14T00:00:00Z\nby someone", by: "alice", how: "label",
  links: ["'"$A"'/1111"]}' >"$record"
issue_is "![shot]($A/1111)" NONE
t_run "$work 7"
t_eq "$t_rc" 2
t_lacks "$t_out" "gives a cycle none"
t_contains "$t_out" "could not be read"
t_eq "$(kept_manifests)" 0

t_case "a link of the record that is not of GitHub's storage is not counted"
images_off
record_is alice "see https://example.com/x.png" "https://example.com/x.png"
issue_is "see https://example.com/x.png" NONE
t_run "$work 7"
t_eq "$t_rc" 2
t_lacks "$t_out" "gives a cycle none"
t_eq "$(kept_manifests)" 0

t_case "a record written while the instance took links from comments keeps them, each with its source"
images_off
mkdir -p "$(dirname "$record")"
jq -n '{at: "2026-10-14T00:00:00Z", by: "alice", how: "label", links: ["'"$A"'/1111"],
  comment_links: [{link: "'"$A"'/2222", source: "comment:51", by: "alice"},
    {link: "https://example.com/y.png", source: "comment:52", by: "alice"}]}' >"$record"
issue_is "![shot]($A/1111)" NONE
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "lists 2 attachments"
t_eq "$(jq -c '[.leftOut[] | [.link, .source]]' "$kept")" "[[\"$A/1111\",\"body\"],[\"$A/2222\",\"comment:51\"]]"

t_case "the manifest of a cycle given nothing does not hold a later cycle that is given the image"
images_off
record_is alice "![shot]($A/1111)" "$A/1111"
issue_is "![shot]($A/1111)" NONE
t_run "$work 7"
t_eq "$t_rc" 2
cp "$kept" "$T/gap.manifest"
images_on
net "$A/1111" "$png"
issue_is "![shot]($A/1111)" NONE
record_is alice "![shot]($A/1111)" "$A/1111"
cp "$T/gap.manifest" "$kept"
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(manifest '.images | map(.link)')" "[\"$A/1111\"]"

t_case "the manifest of a cycle given nothing keeps what an earlier cycle of the same approval recorded"
images_on
net "$A/1111" "$png"
record_is alice "![shot]($A/1111)" "$A/1111"
issue_is "![shot]($A/1111)" NONE
t_run "$work 7"
t_eq "$t_rc" 2
sum=$(sha256sum "$fx/net/$(printf %s "$A/1111" | tr -c 'A-Za-z0-9' _)" | cut -d' ' -f1)
cp "$kept" "$T/give1.manifest"
images_off
cp "$T/give1.manifest" "$kept"
record_is alice "![shot]($A/1111)" "$A/1111"
issue_is "![shot]($A/1111)" NONE
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "lists 1 attachment and this instance gives a cycle none"
t_eq "$(jq -c '[.images[] | [.file, .link, .sha256]]' "$kept")" "[[\"image-1.png\",\"$A/1111\",\"$sum\"]]"
t_eq "$(jq -c '[.leftOut[].link]' "$kept")" "[\"$A/1111\"]"
cp "$kept" "$T/gap.manifest"
# The bytes an earlier cycle fetched are still held against this approval: a
# cycle given the image again is stopped when the link serves other bytes.
images_on
net "$A/1111" "$gif"
record_is alice "![shot]($A/1111)" "$A/1111"
issue_is "![shot]($A/1111)" NONE
cp "$T/gap.manifest" "$kept"
t_run "$work 7"
t_eq "$t_rc" 4
t_contains "$t_out" "$A/1111 gave other bytes than when this approval was first fetched"

# --- The issue's text files ----------------------------------------------------
files_on() {
  images_on "$@"
  t_instance "${T_MINIMAL[@]}" TEHAS_WORK_FILES=1 "$@"
}
files() { jq -c "[.files[] | [.file, .link, .name, .type, .bytes]]" "$imgs/manifest.json" 2>&1; }
left() { jq -c "[.leftOut[] | [.link, .reason]]" "$imgs/manifest.json" 2>&1; }

t_case "a files setting that is not 0 or 1 starts nothing and asks nobody"
files_on
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_FILES=yes
t_run "$work 7"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)$(t_calls curl)$(t_calls docker)" ""

t_case "with files alone the cycle gets the one directory, read-only, and the one variable, as with images"
files_on
net "$F/31/spec.md" '# Spec\nDo it.\n'
issue_is "[spec.md]($F/31/spec.md)"
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$mounts_off" "$imgs:$inside.images:ro" | LC_ALL=C sort)"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" PI_ENSEMBLE_EXTRA_IMAGE_DIRS | LC_ALL=C sort)"
t_eq "$(after -e | grep '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" "PI_ENSEMBLE_EXTRA_IMAGE_DIRS=$inside.images"
t_eq "$(cycle_args | unknown_options "$image" \
  "--name --cgroup-parent --memory --memory-swap --memory-reservation --cpus --pids-limit --cpu-shares --cap-drop --cap-add --security-opt --cidfile -w -v -e" "--rm -i --init")" ""
t_eq "$(preflight_args | awk 'prev == "-v" { print } { prev = $0 }')" "$ws:$inside"

t_case "with files alone the comment says nothing of images"
t_contains "$(t_calls gh)" "Tehas fetched 1 file from the issue text for this cycle"
t_lacks "$(t_calls gh)" "image from the issue text"
t_lacks "$(t_calls gh)" "sandbox image does not say"

t_case "text files are taken by their name, under names of tehas's own, listed with their name, type, size and hash"
files_on
net "$F/31/spec.md" '# Spec\n'
net "$F/32/Data.CSV" 'a,b\n1,2\n'
net "$F/33/fix.json" '{"a": 1}'
net "$F/34/ci.yml" 'on: push\n'
net "$F/35/page.html" '<p>hi</p>'
net "$F/36/run.log" 'ok\n'
net "$F/37/all.zip" 'PK'
net "$A/1111" "$png"
net "$F/38/shot.png" "$png"
issue_is "[spec.md]($F/31/spec.md) [Data.CSV]($F/32/Data.CSV) [fix.json]($F/33/fix.json) [ci.yml]($F/34/ci.yml)
[page.html]($F/35/page.html) [run.log]($F/36/run.log) [all.zip]($F/37/all.zip) ![s]($A/1111) [shot.png]($F/38/shot.png)"
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(cd "$imgs" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" \
  "./file-1.md ./file-2.csv ./file-3.json ./file-4.yml ./file-5.html ./file-6.log ./manifest.json "
t_eq "$(files)" "[[\"file-1.md\",\"$F/31/spec.md\",\"spec.md\",\"text/markdown\",7],[\"file-2.csv\",\"$F/32/Data.CSV\",\"Data.CSV\",\"text/csv\",8],[\"file-3.json\",\"$F/33/fix.json\",\"fix.json\",\"application/json\",8],[\"file-4.yml\",\"$F/34/ci.yml\",\"ci.yml\",\"application/yaml\",9],[\"file-5.html\",\"$F/35/page.html\",\"page.html\",\"text/html\",9],[\"file-6.log\",\"$F/36/run.log\",\"run.log\",\"text/plain\",3]]"
t_eq "$(manifest '.files[0].sha256')" "\"$(printf '# Spec\n' | sha256sum | cut -d' ' -f1)\""
t_eq "$(manifest '.images')" "[]"

t_case "with images off an image and another type are left out and not fetched"
other="Not a md, txt, csv, json, yaml, log, html, css or js file. This instance gives a cycle no images."
t_eq "$(left)" "[[\"$F/37/all.zip\",\"$other\"],[\"$A/1111\",\"$other\"],[\"$F/38/shot.png\",\"$other\"]]"
t_lacks "$(t_calls curl)" "all.zip"
t_lacks "$(t_calls curl)" "$A/1111"
t_lacks "$(t_calls curl)" "shot.png"

t_case "the comment says the files were fetched and are not shown yet, with each hash"
t_contains "$(t_calls gh)" "--body Tehas fetched 6 files from the issue text for this cycle and lists them in its manifest. The pi-rukas in the sandbox image does not read them yet: its agents are not shown them."
t_contains "$(t_calls gh)" "- $F/31/spec.md: file-1.md, sha256 $(printf '# Spec\n' | sha256sum | cut -d' ' -f1)"
t_lacks "$(t_calls gh)" "Tehas gave this cycle"

t_case "bytes that are not UTF-8 text, or hold a NUL, are left out with the reason"
files_on
net "$F/31/bad.txt" 'caf\xe9\n'
net "$F/32/nul.txt" 'a\x00b'
net "$F/33/ok.txt" 'caf\xc3\xa9\n'
issue_is "[bad.txt]($F/31/bad.txt) [nul.txt]($F/32/nul.txt) [ok.txt]($F/33/ok.txt)"
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(left)" "[[\"$F/31/bad.txt\",\"Not valid UTF-8 text, or it holds a NUL byte.\"],[\"$F/32/nul.txt\",\"Not valid UTF-8 text, or it holds a NUL byte.\"]]"
t_eq "$(cd "$imgs" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./file-1.txt ./manifest.json "

t_case "a file may take 1 MB and all files 4 MB: each fetch is held to what is left"
files_on
body=""
for i in 1 2 3 4 5; do
  head -c 900000 /dev/zero | tr '\0' a >"$fx/net/$(printf %s "$F/4$i/f$i.txt" | tr -c 'A-Za-z0-9' _)"
  body="$body [f$i.txt]($F/4$i/f$i.txt)"
done
head -c 1100000 /dev/zero | tr '\0' a >"$fx/net/$(printf %s "$F/49/big.txt" | tr -c 'A-Za-z0-9' _)"
issue_is "[big.txt]($F/49/big.txt)$body"
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(manifest '.files | map(.name)')" '["f1.txt","f2.txt","f3.txt","f4.txt"]'
t_eq "$(left)" "[[\"$F/49/big.txt\",\"Over the 1 MB limit of a file.\"],[\"$F/45/f5.txt\",\"Over the 4 MB limit of an issue's files.\"]]"
t_eq "$(t_calls curl | grep -o -e '--max-filesize [0-9]*' | tr '\n' ' ')" \
  "--max-filesize 1048576 --max-filesize 1048576 --max-filesize 1048576 --max-filesize 1048576 --max-filesize 1048576 --max-filesize 594304 "

t_case "at most sixteen files: the seventeenth is left out and not fetched"
files_on
body=""
for i in $(seq 1 17); do
  net "$F/5$i/n$i.txt" 'x'
  body="$body [n$i.txt]($F/5$i/n$i.txt)"
done
issue_is "$body"
t_run "$work 7"
t_eq "$(manifest '[(.files | length), (.leftOut | map([.link, .reason]))]')" "[16,[[\"$F/517/n17.txt\",\"More than 16 files.\"]]]"
t_lacks "$(t_calls curl)" "$F/517/n17.txt"

t_case "an outsider's issue is given no file without an approval, and nothing is fetched"
files_on
net "$F/31/spec.md" '# Spec\n'
issue_is "[spec.md]($F/31/spec.md)" NONE
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(t_calls curl)" ""
t_eq "$(manifest '.files')" "[]"

t_case "a file that cannot be fetched starts no cycle: exit 4, and the comment names files"
files_on
issue_is "[spec.md]($F/31/spec.md)"
t_run "$work 7"
t_eq "$t_rc" 4
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
t_contains "$(t_calls gh)" "Tehas started no cycle: $F/31/spec.md could not be fetched. This instance gives a cycle the files an issue links"

t_case "with images and files both on there is still one directory and one variable, and both kinds are in it"
files_on TEHAS_WORK_IMAGES=1
net "$F/31/spec.md" '# Spec\n'
net "$A/1111" "$png"
reads_image 1
issue_is "[spec.md]($F/31/spec.md) ![s]($A/1111)"
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(cd "$imgs" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./file-1.md ./image-1.png ./manifest.json "
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$mounts_off" "$imgs:$inside.images:ro" | LC_ALL=C sort)"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" PI_ENSEMBLE_EXTRA_IMAGE_DIRS | LC_ALL=C sort)"
t_contains "$(t_calls gh)" "--body Tehas gave this cycle 1 image from the issue text. Tehas fetched 1 file from the issue text"

t_case "a file of the same approval that gives other bytes stops the start"
record_is alice "[spec.md]($F/31/spec.md) ![s]($A/1111)" "$F/31/spec.md"
rm -f "$kept"
t_run "$work 7"
t_eq "$t_rc" 2
net "$F/31/spec.md" '# Other\n'
t_run "$work 7"
t_eq "$t_rc" 4
t_contains "$t_out" "$F/31/spec.md gave other bytes than when this approval was first fetched"

t_case "a cycle the push check stopped leaves no copy of the index beside its workspace"
images_on
issue_is "Words only."
rm -rf "$hostindex"
mkdir -p "$hostindex"
echo index >"$hostindex/widget.db"
T_ENV=(PUSH_RC=1 "${env_ok[@]}")
t_run "$work 7"
t_eq "$t_rc" 3
t_contains "$t_out" "cannot push to origin"
t_eq "$(find "$ws" -maxdepth 0 2>/dev/null | wc -l)" 1
t_eq "$(find "$state" -maxdepth 0 2>/dev/null | wc -l)" 0
T_ENV=("${env_ok[@]}")

t_done
