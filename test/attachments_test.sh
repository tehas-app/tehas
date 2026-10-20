#!/usr/bin/env bash
# Tests for lib/attachments.sh: which links of an issue or an answer are the
# files a cycle is given, the table of kinds a fetched file is named by, where
# the token goes when a file is fetched, and the archives a cycle may be given.
# A stub curl records its arguments and what it is given on standard input, and
# hands out the bytes a case put at the address of a fetch, so a case says what
# one link of an issue is and every other link of it stays the png below.
# shellcheck disable=SC2016  # stub bodies and snippets are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

lib=". $T_ROOT/lib/attachments.sh"
fx=$T/fx
mkdir -p "$fx/net"
# The png a link with no bytes of its own is answered with, written here so the
# stub has it from the first fetch of the file, whoever its link is.
printf '\x89PNG\r\n\x1a\nPNGDATA' >"$T/png"

# The stub reads two names of its own, which the two helpers below write into it:
# the directory of the bytes kept by the address of a link, and the directory the
# calls are recorded in. The bytes of a link live at the address of it, so a case
# says what one link of an issue is by writing its bytes there, and every other
# link of it stays the png. Neither name carries a dollar of its own, so a helper
# writes it with sed, whose pattern this file expands.
# The stub, written by a here-document nothing expands. Its own shell has $T; the
# three names of the bytes it hands out are written into it by the two helpers
# below, the only ones to rewrite it. The line that records the arguments of a
# call is the one t_stub writes for every other stub, so its path is named here.
# callsat: the directory the stub records its calls in, written into it by
# the first of the two helpers below, which is the one that names a path.
# shellcheck disable=SC2034  # the name the stub's $callsat is rewritten from
calls_at=$T/calls
stub_calls() { sed -i "s|[$]callsat|$T/calls|g" "$T/bin/curl"; }
cat >"$T/bin/curl" <<'STUB'
#!/usr/bin/env bash
printf "%s\n" "$*" >>"$callsat/curl"
out="" prev="" addr="" cap=1
for a in "$@"; do
  [ "$a" != "@-" ] || { printf "stdin: "; cat; } >>"$callsat/curl-stdin"
  [ "$prev" != -o ] || out=$a
  [ "${a#https}" != "$a" ] && addr=$a
  [ "$prev" != --max-filesize ] || cap=$a
  prev=$a
done
# The bytes a fetch gets are those the cases put at the address of the fetch:
# a case says what a link of an issue is and every other link of it stays the png
# written below, which also stands for a link no case filled. An address with a
# .rc beside it is answered with that exit status, as a fetch that fails is
# answered on the host.
# By the name of the link, and by the room its own fetch was given: an
# archive is answered in full to the fetch that unpacks it and in part to the
# one that only looks, which is how the room of a fetch says which bytes to
# hand out. The cycle asks for one link more than once, of a kind it took it
# for first and of the kind its bytes turn out to be, and the answer of one
# fetch must not be that of another.
key=$(printf %s "${addr##*/}" | tr -c "A-Za-z0-9" _)
[ -f "$fx/net/$key.rc" ] && exit "$(cat "$fx/net/$key.rc")"
# The room of the fetch says which bytes of a link to hand out: at the 1 MB
# of one file, the file the link's name says it is; at more, what the link
# holds, which for an archive is the archive itself.
bytes=$fx/net/$key
[ "$cap" -le 1048576 ] && [ -f "$fx/net/${key}_file" ] && bytes=$fx/net/${key}_file
if [ -f "$bytes" ]; then
  cp "$bytes" "$out"
  # The bytes a host answers with those of a fixture: an image by its magic,
  # anything else with a type no extension of ours matches, which is how a
  # text file and an archive are answered on the host.
  case "$(od -An -N4 -tx1 <"$out" | tr -cd '0-9a-f')" in
    89504e47*) printf image/png ;;
    *) printf application/octet-stream ;;
  esac
  exit "${CURL_RC:-0}"
fi
# A link no case filled is the png, answered as a host answers a png.
[ -n "$out" ] && [ -e "$T/png" ] && cp "$T/png" "$out"
printf image/png
exit "${CURL_RC:-0}"
STUB
chmod +x "$T/bin/curl"
# stub_dir <directory>: where the bytes kept by the address of a link live. The
# path is one this file makes under $T, so a slash is the only separator it holds.
stub_dir() { sed -i "s|[$]fx/net|$1/net|g" "$T/bin/curl"; }
stub_calls
stub_dir "$fx"

# The three names of the bytes a fetch is answered with are written once, here,
# and by every case of its own before it runs a snippet: the directory of the
# bytes kept by the address of a link, and the two names of the archive and of
# the file of the case.


# stub_dir <directory>: where the bytes kept by the address of a link live. The
# path is one this file makes under $T, so a slash is the only separator it holds.

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


# --- An archive the issue links ----------------------------------------------
# A zip is taken by its first bytes, not its name. The fixtures are written by
# hand and committed under test/fixtures/work-zips, so these cases ask no zip
# tool, as the Protolab's do.
zips=$T_ROOT/test/fixtures/work-zips
anyzip=https://github.com/user-attachments/files/9/any.zip
bzip=https://github.com/user-attachments/files/8/b.zip
czip=https://github.com/user-attachments/files/7/c.zip
pnglink=https://github.com/user-attachments/assets/1111
evilzip=https://github.com/user-attachments/files/13/evil.zip
evil_body="[evil.zip]($evilzip)"
bigzip=https://github.com/user-attachments/files/11/big.zip
zipdir=$T/zipdir

# zip_case <dir> <body> [setting=VALUE]...: that issue, and its images in a
# clean shell, with the instance's settings and the case's own after them. The
# stub answers the .zip link with NET_ZIP's bytes and every other link with the
# png, so a case says what the archive is and nothing else about the issue.
zip_case() {
  local dir=$1 body=$2 nm key
  shift 2
  rm -rf "$dir"
  jq -n --arg b "$body" '{number: 7, author_association: "OWNER", body: $b}' >"$T/issue.json"
  # The instance's settings and the case's own: a case that names none of the
  # three kinds is one where the instance gives a cycle images, files and
  # archives alike, which is what most of these cases read.
  T_ENV=(TEHAS_REPO=acme/widget GH_TOKEN=tok "$@")
  local has_images=0 has_files=0 has_zips=0 setting
  for setting in "$@"; do
    case "$setting" in
      TEHAS_WORK_IMAGES=*) has_images=1 ;;
      TEHAS_WORK_FILES=*) has_files=1 ;;
      TEHAS_WORK_ZIPS=*) has_zips=1 ;;
    esac
  done
  [ "$has_images" = 1 ] || T_ENV+=(TEHAS_WORK_IMAGES=1)
  [ "$has_files" = 1 ] || T_ENV+=(TEHAS_WORK_FILES=1)
  [ "$has_zips" = 1 ] || T_ENV+=(TEHAS_WORK_ZIPS=1)
  # Nothing of a case is kept from one to the next: the bytes it put at the
  # address of a link, and the png that answers the links it left alone.
  rm -rf "$fx/net"
  mkdir -p "$fx/net"
  printf '\x89PNG\r\n\x1a\nPNGDATA' >"$T/png"
  # The png answers every link but those the case filled, so a case that says
  # the instance gives a cycle no image has one link left: the archive, and the
  # bytes of none of its entries are a png.
  for setting in "$@"; do
    case "$setting" in
      TEHAS_WORK_IMAGES=0) printf 'not a png at all\n' >"$T/png" ;;
    esac
  done
  # Every link of the body that ends in .zip is answered with the bytes of the
  # one archive the case named, so a case of three links named .zip is a case of
  # three archives of those bytes. Any other link is answered with the bytes the
  # case named for a file, or with the png when it named none. An archive bigger
  # than the 1 MB of one file is answered only at the room of the fetch that can
  # hold it, so the cycle reading one link twice - once as the kind its name
  # says and once as the kind its bytes turn out to be - is answered both times.
  local zip_bytes=${NET_ZIP:-} file_bytes=${NET_FILE:-} nm key
  # The bytes of a link answer that link alone: an archive under the name it is
  # linked by, and one text file of an archive under the name the issue links
  # it by. A link no case filled stays the png below. The names are read into a
  # variable first, as the loop's own process substitution is not open while
  # this shell runs the call below it.
  local links
  links=$(jq -r .body "$T/issue.json" | grep -o -e 'https://[^ )]*' || true)
  for nm in $links; do
    key=$(printf %s "${nm##*/}" | tr -c 'A-Za-z0-9' _)
    case "${nm##*/}" in
      *.zip)
        [ -n "$zip_bytes" ] || continue
        cp "$zip_bytes" "$fx/net/$key"
        # The cycle reads an archive twice: once for the file its name says,
        # at the 1 MB of one file, and once for the archive, at the 4 MB of an
        # issue's files. The bytes it gets are the same both times.
        [ -n "$file_bytes" ] && cp "$file_bytes" "$fx/net/${key}_file"
        ;;
      *) [ -n "$file_bytes" ] && cp "$file_bytes" "$fx/net/$key" ;;
    esac
  done
  t_run "$lib; tehas_issue_images 7 $dir approved || echo \"FAILED \$image_why\""
  T_ENV=()
  return 0
}
# The body of these cases: the one .zip and one png, and the three archives
# and one png of the case of the third one left out.
zip_body="[any.zip]($anyzip) ![shot]($pnglink)"
three_body="[a.zip]($anyzip) [b.zip]($bzip) [c.zip]($czip) ![shot]($pnglink)"
# The archive of five files of 1 MB, under a name of its own: the bytes of a
# link answer that link alone, so the archive is never answered with the
# bytes of a file of the issue, nor the other way about.
big_body="[big.zip]($bigzip)"
# The files of a directory, in order, as one line.
ls_imgs() { (cd "$1" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' '); }

t_case "a zip is known by its first bytes, and a file named .zip that is not one is not"
printf 'not a zip at all\n' >"$T/fake.zip"
for f in "$zips/good.zip" "$zips/big.zip" "$zips/evil.zip"; do
  t_run "$lib; tehas_zip_type '$f' && echo yes || echo no"
  t_eq "$f $t_out" "$f yes"
done
for f in "$T/fake.zip" "$T/png" "$T/no-such-file"; do
  t_run "$lib; tehas_zip_type '$f' && echo yes || echo no"
  t_eq "$f $t_out" "$f no"
done

t_case "the name of a file an archive did not give keeps the characters a taken name would, cut at 200 bytes"
t_run "$lib; tehas_path_clean 'a/../b.txt'"
t_eq "$t_out" "a/../b.txt"
t_run "$lib; tehas_path_clean 'img/x[1].txt'"
t_eq "$t_out" "img/x-1-.txt"
# 300 letters in 150 parts of 2 bytes: the whole keeps its first 200 bytes, and
# never its beginning cut short at the 80 of one part, which is what an IFS left
# over from a caller would give.
t_run "$lib; tehas_path_clean \"\$(printf 'a,%.0s' {1..150})\" | wc -c"
t_eq "$t_out" "200"
# Each part of a path is kept in full to the 200 bytes of the whole, so a
# manifest says the path an archive gave and not only its last name.
t_run "$lib; tehas_path_clean 'deep/down/inside/the/archive/name.txt'"
t_eq "$t_out" "deep/down/inside/the/archive/name.txt"
# A path of 1000 bytes keeps the 200 first of the cleaned one, so what is said
# of it is only ever the beginning of it.
t_run "$lib; tehas_path_clean \"\$(printf 'ab-%.0s' {1..500} | sed 's/.\$//')\" | wc -c"
t_eq "$t_out" "200"
# Each folder of a path keeps its own 80 bytes, so a deep one still reads.
t_run "$lib; tehas_path_clean \"\$(printf 'ab-%.0s' {1..20} | sed 's/.\$//')/name.txt\" | wc -c"
t_eq "$t_out" "68"

t_case "a cycle given an archive gets its image and its file, each saying the archive and the path"
NET_ZIP=$zips/good.zip zip_case "$zipdir" "$zip_body"
t_eq "$t_out" ""
t_eq "$(ls_imgs "$zipdir")" "./file-1.md ./image-1.png ./image-2.png ./manifest.json "
t_eq "$(jq -c '[.images[] | [.file, .zip, .path]]' "$zipdir/manifest.json")" \
  "[[\"image-1.png\",\"$anyzip\",\"img/b.png\"],[\"image-2.png\",null,null]]"
t_eq "$(jq -c '[.files[] | [.file, .name, .zip, .path, .type]]' "$zipdir/manifest.json")" \
  "[[\"file-1.md\",\"a.md\",\"$anyzip\",\"a.md\",\"text/markdown\"]]"
t_eq "$(jq -c '[.leftOut[] | [.link, .path, .reason]]' "$zipdir/manifest.json")" \
  "[[\"$anyzip\",\"c.svg\",\"Not a png, jpg, webp or gif image, nor a md, txt, csv, json, yaml, log, html, css or js file.\"]]"
t_eq "$(jq -r '.files[0].bytes' "$zipdir/manifest.json")" 4
t_eq "$(find "$zipdir" -name '.unzip-*' -o -name '.part' | wc -l)" 0

t_case "a file named .zip that is not one is left out with the reason, and nothing of it is kept"
NET_ZIP=$T/fake.zip zip_case "$zipdir" "$zip_body"
t_eq "$t_out" ""
t_eq "$(ls_imgs "$zipdir")" "./image-1.png ./manifest.json "
t_eq "$(jq -c '[.leftOut[] | [.link, .reason]]' "$zipdir/manifest.json")" \
  "[[\"$anyzip\",\"Not a zip archive.\"]]"

t_case "an entry that escapes the archive, or is a link, writes nothing and is left out by its cleaned name"
NET_ZIP=$zips/evil.zip zip_case "$zipdir" "$evil_body"
t_eq "$t_out" ""
t_eq "$(find "$T" \( -name 'escape.md' -o -name link \) ! -path '*/fixtures/*' | wc -l)" 0
t_eq "$(ls_imgs "$zipdir")" "./file-1.md ./manifest.json "
t_eq "$(jq -c '[.leftOut[] | [.path, .reason]]' "$zipdir/manifest.json")" \
  '[["../escape.md","A path outside the archive."],["link","A link or a special file."]]'

t_case "what of an archive runs over the 1 MB of one file is left out, the files before it kept"
NET_ZIP=$zips/big.zip NET_FILE=$T/ok.txt zip_case "$zipdir" "$big_body" TEHAS_WORK_IMAGES=0
t_eq "$t_out" ""
t_eq "$(jq -c '[.files[] | .file]' "$zipdir/manifest.json")" \
  '["file-1.txt","file-2.txt","file-3.txt","file-4.txt"]'
t_eq "$(jq -c '[.leftOut[] | [.path, .reason]]' "$zipdir/manifest.json")" \
  "[[\"e.txt\",\"Over the 1 MB limit of a file.\"]]"
t_eq "$(find "$zipdir" -name '.unzip-*' -o -name '.part' | wc -l)" 0

t_case "with the setting off a zip the issue links is left out as before, and never unpacked"
NET_ZIP=$zips/good.zip zip_case "$zipdir" "$zip_body" TEHAS_WORK_ZIPS=0
t_eq "$t_out" ""
t_eq "$(ls_imgs "$zipdir")" "./image-1.png ./manifest.json "
t_eq "$(jq -r '.leftOut | length' "$zipdir/manifest.json")" 1
t_contains "$(jq -r '.leftOut[0].reason' "$zipdir/manifest.json")" \
  "Not a png, jpg, webp or gif image, nor a md, txt, csv, json, yaml, log, html, css or js file."
t_eq "$(jq -c '.leftOut[0] | has("path")' "$zipdir/manifest.json")" "false"

t_case "the third archive of one issue is left out, and the two before it are taken"
NET_ZIP=$zips/good.zip zip_case "$zipdir" "$three_body"
t_eq "$t_out" ""
# Of the two archives that were taken, only the svg each holds is left out: the
# archive of the link that was not read at all has no file to name.
t_eq "$(jq -c '[.leftOut[] | select(.path == null) | [.link, .reason]]' "$zipdir/manifest.json")" \
  "[[\"$czip\",\"More than 2 archives.\"]]"
t_eq "$(jq -r '[.leftOut[] | select(.path) | .link] | unique | .[]' "$zipdir/manifest.json" | sort | tr '\n' ' ')" \
  "$(printf '%s\n' "$anyzip" "$bzip" | LC_ALL=C sort | tr '\n' ' ')"
t_eq "$(jq -c '[.files[] | .file]' "$zipdir/manifest.json")" '["file-1.md","file-2.md"]'
t_eq "$(jq -c '[.images[] | .file]' "$zipdir/manifest.json")" '["image-1.png","image-2.png","image-3.png"]'

t_done
