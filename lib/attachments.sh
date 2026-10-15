# shellcheck shell=bash
# Files attached to an issue: which links are GitHub's attachment storage,
# which types are known, and how one file is fetched. Sourced by the commands
# that fetch them: tehasbot protolab (a ticket's page), and tehas work and
# tehas plan (the images and text files an issue links). It is in one place
# so they cannot drift apart.
#
#   tehas_attachment_links            print a jq definition, `links`: every
#                                     attachment link in a string, in the
#                                     order written
#   tehas_attachment_kind <extension> image, video, app or part; fails for a
#                                     type that is not known
#   tehas_attachment_ext <content type>
#                                     the extension of an image or a video;
#                                     fails for any other type
#   tehas_attachment_fetch <address> <file> <most bytes> [<token>]
#                                     download, print the content type
#
# Sourcing it does nothing. Which of the known types protolab takes, and how
# much, is its own rule; the images a container is given follow the rule at
# the end of this file.

# Only GitHub's own storage: nothing else is ever fetched for an issue. A
# function and not a variable: tehas_load drops every TEHAS_ variable.
tehas_attachment_links() {
  # shellcheck disable=SC2016  # a jq program
  printf '%s\n' 'def links: [scan("https://(?:github\\.com/user-attachments/(?:assets|files)/|github\\.com/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+/(?:assets|files)/[0-9]+/|user-images\\.githubusercontent\\.com/)[A-Za-z0-9._~%/?&=+-]+")];'
}

tehas_attachment_kind() {
  case "$1" in
    png | jpg | jpeg | gif | webp | svg | avif) echo image ;;
    mp4 | webm | mov) echo video ;;
    html | htm) echo app ;;
    css | js | mjs | json | txt) echo part ;;
    *) return 1 ;;
  esac
}

# An image or video pasted into an issue has no name, so its type is the one
# GitHub's storage answers with.
tehas_attachment_ext() {
  case "${1%%;*}" in
    image/png) echo png ;;
    image/jpeg) echo jpg ;;
    image/gif) echo gif ;;
    image/webp) echo webp ;;
    image/svg+xml) echo svg ;;
    image/avif) echo avif ;;
    video/mp4) echo mp4 ;;
    video/webm) echo webm ;;
    video/quicktime) echo mov ;;
    *) return 1 ;;
  esac
}

# The token goes to github.com only, and on standard input, not the command
# line; curl does not pass it on when GitHub redirects to its storage. The
# exit status is curl's: 63 is a file over <most bytes>, 22 one GitHub did not
# hand out.
tehas_attachment_fetch() { # <address> <file> <most bytes> [<token>]
  local args=(-sSfL --proto '=https' --proto-redir '=https' --max-redirs 3 --max-time 120
    --max-filesize "$3" -o "$2" -w '%{content_type}')
  case "$1" in
    https://github.com/*)
      if [ -n "${4:-}" ]; then
        curl "${args[@]}" -H @- "$1" <<<"Authorization: Bearer $4"
        return
      fi
      ;;
  esac
  curl "${args[@]}" "$1" </dev/null
}

# The images and text files an issue links, for a container that is given
# them: tehas work and tehas plan. The rules are written in libexec/work's
# header.
#
#   tehas_issue_images <issue> <dir> [approved] [record]
#                                     remake <dir> from the issue's text, with
#                                     a manifest.json; uses TEHAS_REPO and
#                                     GH_TOKEN. Returns 1, with the reason in
#                                     $image_why, when the issue's images
#                                     cannot be given and nothing may start.
#                                     "approved" says an allowed account
#                                     approved the text with its label: the
#                                     images are then taken whoever opened the
#                                     issue. Without it, only when an owner,
#                                     member or collaborator opened it. With
#                                     the dispatcher's approval record, the
#                                     links are the record's, and only while
#                                     the body has the hash it approved
#   tehas_issue_images_changed <kept manifest> <dir>
#                                     returns 1, with the reason in $image_why,
#                                     when both are of the same approval and a
#                                     link gave other bytes this time
#   tehas_issue_images_said <dir> <what> [yes|no|unknown]
#                                     the comment that says what <what> (a
#                                     cycle, a planning session) was given;
#                                     empty when the issue links no image. The
#                                     third is what tehas_images_read said
#                                     [yes]
#   tehas_images_read <image>         whether the pi-rukas in the sandbox
#                                     image reads a manifest of images: yes,
#                                     no or unknown, also written to
#                                     $TEHAS_HOME/state/images-read.json for
#                                     tehasbot health
#   tehas_images_read_forget          remove that file: the instance gives no
#                                     images

# tehas_image_type <file>: the type its first bytes say; fails for any other file.
tehas_image_type() {
  case "$(head -c 12 "$1" | od -An -v -tx1 | tr -d ' \n')" in
    89504e470d0a1a0a*) echo png ;;
    ffd8ff*) echo jpg ;;
    474946383761* | 474946383961*) echo gif ;;
    52494646????????57454250) echo webp ;;
    *) return 1 ;;
  esac
}

# tehas_body_sha256: the sha256 of an issue's body, given the issue as JSON on
# standard input. Carriage returns are dropped: the dispatcher reads the body
# from GraphQL and tehas work from REST, and only line ends may differ.
tehas_body_sha256() {
  local sum
  sum=$(jq -j '(.body // "") | gsub("\r"; "")' | sha256sum) || return 1
  sum=${sum%% *}
  [[ $sum =~ ^[0-9a-f]{64}$ ]] || return 1
  echo "$sum"
}

# tehas_text_type <extension>: the type of a text file a cycle may be given;
# fails for any other extension. Not svg: as an image it is code, and no step
# wants it as text yet.
tehas_text_type() {
  case "$1" in
    md) echo text/markdown ;;
    txt | log) echo text/plain ;;
    csv) echo text/csv ;;
    json) echo application/json ;;
    yaml | yml) echo application/yaml ;;
    html) echo text/html ;;
    css) echo text/css ;;
    js) echo text/javascript ;;
    *) return 1 ;;
  esac
}

# tehas_text_ok <file>: valid UTF-8 with no NUL byte.
tehas_text_ok() {
  [ "$(tr -d '\000' <"$1" | wc -c)" = "$(wc -c <"$1")" ] && iconv -f UTF-8 -t UTF-8 "$1" >/dev/null 2>&1
}

# What is taken follows the settings: images unless TEHAS_WORK_IMAGES is 0,
# text files when TEHAS_WORK_FILES is 1. Each kind has its own limits.
# shellcheck disable=SC2034  # the caller reads what this sets
tehas_issue_images() { # <issue> <dir> [approved] [record]
  local issue=$1 images=$2 approved=${3:-} record=${4:-} data who ref src by name ext kind type size rc left sum given=null
  local want_images=1 want_files=0 want_comments=0 used=0 count=0 fused=0 fcount=0
  [ "${TEHAS_WORK_IMAGES:-1}" != 0 ] || want_images=0
  [ "${TEHAS_WORK_FILES:-0}" != 1 ] || want_files=1
  [ "${TEHAS_WORK_COMMENTS:-0}" != 1 ] || want_comments=1
  local limit=$((10 * 1024 * 1024)) max=8 flimit=$((4 * 1024 * 1024)) fmax=16 fone=$((1024 * 1024))
  local other="Not a png, jpg, webp or gif image." over="Over the 10 MB limit of an issue."
  local fover="Over the 4 MB limit of an issue's files." fbig="Over the 1 MB limit of a file."
  local notext="Not valid UTF-8 text, or it holds a NUL byte."
  [ "$want_files" = 0 ] || other="Not a png, jpg, webp or gif image, nor a md, txt, csv, json, yaml, log, html, css or js file."
  [ "$want_images" = 1 ] || other="Not a md, txt, csv, json, yaml, log, html, css or js file. This instance gives a cycle no images."
  image_why="the directory for the issue's images could not be made"
  { rm -rf "$images" && mkdir -p "$images"; } || return 1
  : >"$images/.taken"
  : >"$images/.files"
  : >"$images/.left"
  leave() { jq -n -c --arg link "$1" --arg reason "$2" --arg source "$src" '{link: $link, reason: $reason, source: $source}' >>"$images/.left"; }
  image_why="the issue could not be read for its images"
  data=$(gh api "repos/$TEHAS_REPO/issues/$issue" 2>/dev/null) || return 1
  jq -e 'type == "object" and (.body == null or (.body | type) == "string")' <<<"$data" >/dev/null 2>&1 || return 1
  who=$(jq -r '.author_association // ""' <<<"$data")
  # The links as the approval saw them. A body changed since was not approved,
  # and no record of which links were approved is no ground to fetch any.
  if [ -n "$record" ]; then
    image_why="the approval record $record could not be read"
    jq -e 'type == "object" and (.body_sha256 | type) == "string" and (.links | type) == "array"
      and (.at | type) == "string" and (.by | type) == "string" and (.how | type) == "string"
      and (.comment_links == null or (.comment_links | type) == "array")' "$record" >/dev/null 2>&1 || return 1
    image_why="the issue's text could not be read for its hash"
    sum=$(tehas_body_sha256 <<<"$data") || return 1
    if [ "$sum" != "$(jq -r .body_sha256 "$record")" ]; then
      image_why="the issue's text is not the text that was approved: it changed after the approval"
      return 1
    fi
    given=$(jq -c '{at, by, how}' "$record") || return 1
  fi

  # One line per link: where it was written, the link, and for a comment who
  # wrote it. A tab parts them; none of the three can hold one.
  while IFS=$'\t' read -r src ref by; do
    [ -n "$ref" ] || continue
    if [ "$src" = body ]; then
      case "$approved:$who" in
        approved:* | *:OWNER | *:MEMBER | *:COLLABORATOR) ;;
        *) leave "$ref" "The issue was not opened by an owner, member or collaborator, and its label was not applied by an allowed account."; continue ;;
      esac
    # A comment is taken on its author's word and the approval's, both allowed
    # now: the record says who they were, not that they still may.
    elif [ "$approved" != approved ] || ! tehas_allowed_labeler "$by"; then
      leave "$ref" "The comment was not written by an allowed account, or the start was not approved by one."
      continue
    fi
    # A file attached under its name is taken by what the name says. A pasted
    # one has no name and is only ever an image: its bytes say which.
    name=""
    kind=image
    case "$ref" in
      https://github.com/user-attachments/assets/* | https://github.com/*/assets/*) ;;
      *)
        name=${ref##*/}
        name=${name%%\?*}
        ext=""
        case "$name" in *.*) ext=${name##*.} ;; esac
        ext=${ext,,}
        case "$ext" in
          png | jpg | jpeg | webp | gif) ;;
          *) kind=text ;;
        esac
        ;;
    esac
    if [ "$kind" = image ] && [ "$want_images" = 0 ]; then leave "$ref" "$other"; continue; fi
    if [ "$kind" = text ] && { [ "$want_files" = 0 ] || ! type=$(tehas_text_type "$ext"); }; then
      leave "$ref" "$other"
      continue
    fi
    if [ "$kind" = image ]; then
      if [ "$count" -ge "$max" ]; then leave "$ref" "More than $max images."; continue; fi
      left=$((limit - used))
      if [ "$left" -le 0 ]; then leave "$ref" "$over"; continue; fi
    else
      if [ "$fcount" -ge "$fmax" ]; then leave "$ref" "More than $fmax files."; continue; fi
      left=$((flimit - fused))
      if [ "$left" -le 0 ]; then leave "$ref" "$fover"; continue; fi
      [ "$left" -le "$fone" ] || left=$fone
    fi

    rm -f "$images/.part"
    tehas_attachment_fetch "$ref" "$images/.part" "$left" "$GH_TOKEN" >/dev/null
    rc=$?
    case "$rc:$kind" in
      0:*) ;;
      63:image) leave "$ref" "$over"; continue ;;
      63:text) if [ "$left" = "$fone" ]; then leave "$ref" "$fbig"; else leave "$ref" "$fover"; fi; continue ;;
      *) image_why="$ref could not be fetched"; return 1 ;;
    esac
    size=$(stat -c %s "$images/.part" 2>/dev/null) || size=0
    if [ "$size" -gt "$left" ]; then
      if [ "$kind" = image ]; then leave "$ref" "$over"; elif [ "$left" = "$fone" ]; then leave "$ref" "$fbig"; else leave "$ref" "$fover"; fi
      continue
    fi
    if [ "$kind" = image ]; then
      ext=$(tehas_image_type "$images/.part") || { leave "$ref" "$other"; continue; }
      count=$((count + 1))
      used=$((used + size))
      name=image-$count.$ext
      type=image/${ext/jpg/jpeg}
    else
      tehas_text_ok "$images/.part" || { leave "$ref" "$notext"; continue; }
      fcount=$((fcount + 1))
      fused=$((fused + size))
      name=file-$fcount.$ext
    fi
    if ! { mv "$images/.part" "$images/$name" && chmod 644 "$images/$name"; }; then
      image_why="the issue's images could not be written"
      return 1
    fi
    sum=$(sha256sum "$images/$name") || { image_why="the issue's images could not be read back"; return 1; }
    if [ "$kind" = image ]; then
      jq -n -c --arg file "$name" --arg link "$ref" --arg type "$type" --argjson bytes "$size" \
        --arg sum "${sum%% *}" --arg source "$src" \
        '{file: $file, link: $link, type: $type, bytes: $bytes, sha256: $sum, source: $source}' >>"$images/.taken"
    else
      # The name as the link has it: the link's characters only.
      jq -n -c --arg file "$name" --arg link "$ref" --arg name "${ref##*/}" --arg type "$type" --argjson bytes "$size" \
        --arg sum "${sum%% *}" --arg source "$src" \
        '{file: $file, link: $link, name: $name, type: $type, bytes: $bytes, sha256: $sum, source: $source}' >>"$images/.files"
    fi
  done < <(if [ -n "$record" ]; then
    # Only links of the shape this file takes, whatever the record holds: the
    # body's first, then the comments' in the order written. Comments only
    # where the instance takes them.
    jq -r --argjson comments "$want_comments" "$(tehas_attachment_links)"'
      (.links[] | strings | select(links == [.]) | "body\t\(.)\t"),
      (if $comments == 1 then (.comment_links // [])[]
         | select(type == "object" and (.link | type) == "string" and (.source | type) == "string"
             and (.by | type) == "string" and (.link | links == [.])
             and (.source | test("^(pr-)?comment:[0-9]+$")) and (.by | test("^[A-Za-z0-9-]+$")))
         | "\(.source)\t\(.link)\t\(.by)"
       else empty end)' "$record"
  else
    jq -r "$(tehas_attachment_links)"'(.body // "") | links | .[] | "body\t\(.)\t"' <<<"$data"
  fi | awk -F '\t' '!seen[$2]++')

  image_why="the issue's images could not be written"
  jq -n --argjson issue "$issue" --argjson given "$given" --argjson files "$want_files" \
    --slurpfile images "$images/.taken" --slurpfile taken "$images/.files" --slurpfile left "$images/.left" \
    '{images: $images} + (if $files == 1 then {files: $taken} else {} end) + {leftOut: $left, issue: $issue, approval: $given}' \
    >"$images/manifest.json" || return 1
  rm -f "$images/.part" "$images/.taken" "$images/.files" "$images/.left"
  { chmod 755 "$images" && chmod 644 "$images/manifest.json"; } || return 1
  image_why=""
}

# The same link of the same approval must give the same bytes: what was
# approved is what a retry of it fetches. Without an approval on either side
# there is nothing to hold them to.
# shellcheck disable=SC2034  # the caller reads what this sets
tehas_issue_images_changed() { # <kept manifest> <dir>
  local link
  [ -f "$1" ] && [ ! -L "$1" ] || return 0
  link=$(jq -r --slurpfile new "$2/manifest.json" '
    select(.approval != null and .approval == $new[0].approval)
    | [.images[]?, .files[]? | {(.link): .sha256}] | add // {} | . as $old
    | $new[0] | .images[]?, .files[]? | select($old[.link] != null and $old[.link] != .sha256) | .link' "$1" 2>/dev/null | head -n 1)
  [ -z "$link" ] || { image_why="$link gave other bytes than when this approval was first fetched"; return 1; }
}

# The links are of the shape tehas_attachment_links takes and the reasons are
# written above, so nothing of the issue's own words is posted back. A comment
# does not say the images were given when the agents are not shown them, and
# no pi-rukas reads the text files yet.
tehas_issue_images_said() { # <dir> <what> [yes|no|unknown]
  jq -r --arg what "$2" --arg reads "${3:-yes}" '(.files // []) as $files
    | select((.images | length) + ($files | length) + (.leftOut | length) > 0)
    | (if any(.images[], $files[]; (.source // "body") != "body") then "the issue and its comments" else "the issue text" end) as $from
    | def where: if (.source // "body") == "body" then "" else " (\(.source | sub(":"; " ")))" end;
      "\(.images | length) \(if (.images | length) == 1 then "image" else "images" end) from \($from)" as $n
    | "\($files | length) \(if ($files | length) == 1 then "file" else "files" end) from \($from)" as $f
    | ([(if (.images | length) > 0 or ($files | length) == 0 then
          if $reads == "no" then "Tehas fetched \($n) for this \($what), but the pi-rukas in the sandbox image does not read them: its agents are not shown them."
          elif $reads == "yes" then "Tehas gave this \($what) \($n)."
          else "Tehas gave this \($what) \($n). Whether the pi-rukas in the sandbox image reads them could not be checked." end
        else empty end),
       (if ($files | length) > 0 then
          "Tehas fetched \($f) for this \($what) and lists them in its manifest. The pi-rukas in the sandbox image does not read them yet: its agents are not shown them."
        else empty end)] | join(" "))
      + (if (.images | length) + ($files | length) > 0 then
          "\n\n" + (.images + $files | map("- \(.link)\(where): \(.file), sha256 \(.sha256)") | join("\n")) else "" end)
      + (if (.leftOut | length) > 0 then "\n\nLeft out:\n" + (.leftOut | map("- \(.link)\(where): \(.reason)") | join("\n")) else "" end)' \
    "$1/manifest.json" 2>/dev/null
}

# The reader is a file in the image, so the image answers without a session.
# An image id names fixed content, so the answer is kept per id. The check's
# container is given nothing: no mount, no variable, no network, and it pulls
# nothing. Like the push check it is a few seconds and not in the slice.
tehas_images_read() { # <image>
  local image=$1 id answer=unknown cache=$TEHAS_HOME/state/images-read
  local reader=/opt/pi-rukas/extension/src/work-driver-issue-images.ts
  id=$(timeout -k 5 60 docker image inspect --format '{{.Id}}' "$image" 2>/dev/null) || id=""
  if [[ $id =~ ^sha256:[0-9a-f]{64}$ ]]; then
    answer=$(cat "$cache/${id#sha256:}" 2>/dev/null) || answer=""
    case "$answer" in
      yes | no) ;;
      *)
        timeout -k 5 60 docker run --rm --pull never --network none --entrypoint sh "$id" \
          -c "test -f $reader" </dev/null >/dev/null 2>&1
        case $? in
          0) answer=yes ;;
          1) answer=no ;;
          *) answer=unknown ;;
        esac
        if [ "$answer" != unknown ] && mkdir -p "$cache"; then
          { echo "$answer" >"$cache/${id#sha256:}"; } 2>/dev/null || true
        fi
        ;;
    esac
  fi
  { mkdir -p "$TEHAS_HOME/state" &&
    jq -n --arg time "$(date -u +%FT%TZ)" --arg image "$image" --arg answer "$answer" \
      '{time: $time, image: $image, reads: (if $answer == "yes" then true elif $answer == "no" then false else null end)}' \
      >"$TEHAS_HOME/state/images-read.json.new" &&
    mv "$TEHAS_HOME/state/images-read.json.new" "$TEHAS_HOME/state/images-read.json"; } 2>/dev/null || true
  echo "$answer"
}

tehas_images_read_forget() { rm -f "$TEHAS_HOME/state/images-read.json"; }
