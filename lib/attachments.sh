# shellcheck shell=bash
# Files attached to an issue: which links are GitHub's attachment storage,
# which types are known, and how one file is fetched. Sourced by the commands
# that fetch them: tehasbot protolab (a ticket's page), and tehas work and
# tehas plan (the images an issue links). It is in one place so they cannot
# drift apart.
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
# the end of this file. Needs lib/labels.sh beside it: what an operator's
# reply may bring is decided by who wrote it, and that test is labels.sh's
# (AGENTS.md rule 5), not a copy of it.

# shellcheck source=lib/labels.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/labels.sh"

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

# The images an issue links, for a container that is given them: tehas work
# and tehas plan. The rules are written in libexec/work's header.
#
#   tehas_issue_images <issue> <dir>  remake <dir> from the issue's text, with
#                                     a manifest.json; uses TEHAS_REPO and
#                                     GH_TOKEN. Returns 1, with the reason in
#                                     $image_why, when the issue's images
#                                     cannot be given and nothing may start
#   tehas_reply_images <issue> <dir> <text> <login>
#                                     add to <dir> what an operator's reply
#                                     links, keeping the numbering and the
#                                     limits of what <dir> already holds, and
#                                     rewrite manifest.json. The links are
#                                     fetched only when <login> names an
#                                     account that may authorise work here;
#                                     every link of another account is left
#                                     out. A link already fetched, or one
#                                     already left out for a reason that still
#                                     stands (its type, the limits), is not
#                                     fetched again; one left out only because
#                                     of the account that wrote it is judged
#                                     again for a reply that may authorise
#                                     work. Uses GH_TOKEN and
#                                     TEHAS_ALLOWED_LABELERS. Returns 1, with
#                                     the reason in $image_why, when an image
#                                     it may fetch could not be fetched or the
#                                     record already there cannot be read; the
#                                     manifest is then what it was before.
#   tehas_issue_images_said <dir> <what>
#                                     the comment that says what <what> (a
#                                     cycle, a planning session) was given;
#                                     empty when the issue links no image
#   tehas_images_added <dir> <what> <source>
#                                     the comment that says what <what> was
#                                     given from <source> by the last call of
#                                     tehas_reply_images; empty when that call
#                                     added nothing

# What one container is given, and the reasons a link is left out. Named here
# because the issue's own images and a reply's obey the same rules.
tehas_image_max=8
tehas_image_limit=$((10 * 1024 * 1024))
tehas_image_other="Not a png, jpg, webp or gif image."
tehas_image_over="Over the 10 MB limit of an issue."
tehas_image_author="The issue was not opened by an owner, member or collaborator."
tehas_image_stranger="It was not written by an account that may authorise work here."

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

# One link, of an issue's text or of an operator's reply: fetch it, judge its
# bytes, and record it in <dir>/.taken. <count> and <used> are what the session
# has so far, because the limits are counted over a session and not over one
# call. <why> is the reason the link is not taken at all, empty when it may be.
# Sets image_bytes to the bytes taken and, when the link was left out,
# image_left to the reason. Returns 1 for the failures that stop everything,
# with the reason in $image_why.
# shellcheck disable=SC2034  # the caller reads what this sets
tehas_image_take() { # <link> <dir> <images so far> <bytes used so far> <why not taken>
  local ref=$1 dir=$2 count=$3 used=$4 why=$5 name ext size rc left slot
  image_bytes=0
  image_left=$why
  if [ -z "$image_left" ]; then
    # A file attached under its name is not fetched when the name says it is
    # no image. A pasted one has no name: its bytes say.
    case "$ref" in
      https://github.com/user-attachments/assets/* | https://github.com/*/assets/*) ;;
      *)
        name=${ref##*/}
        name=${name%%\?*}
        ext=""
        case "$name" in *.*) ext=${name##*.} ;; esac
        case "${ext,,}" in png | jpg | jpeg | webp | gif) ;; *) image_left=$tehas_image_other ;; esac
        ;;
    esac
  fi
  [ -z "$image_left" ] || return 0
  if [ "$count" -ge "$tehas_image_max" ]; then
    image_left="More than $tehas_image_max images."
    return 0
  fi
  left=$((tehas_image_limit - used))
  if [ "$left" -le 0 ]; then
    image_left=$tehas_image_over
    return 0
  fi
  rm -f "$dir/.part"
  tehas_attachment_fetch "$ref" "$dir/.part" "$left" "$GH_TOKEN" >/dev/null
  rc=$?
  case "$rc" in
    0) ;;
    63) image_left=$tehas_image_over; return 0 ;;
    *) image_why="$ref could not be fetched"; return 1 ;;
  esac
  size=$(stat -c %s "$dir/.part" 2>/dev/null) || size=0
  if [ "$size" -gt "$left" ]; then
    image_left=$tehas_image_over
    return 0
  fi
  ext=$(tehas_image_type "$dir/.part") || { image_left=$tehas_image_other; return 0; }
  slot=$((count + 1))
  if ! { mv "$dir/.part" "$dir/image-$slot.$ext" && chmod 644 "$dir/image-$slot.$ext"; }; then
    image_why="the images could not be written"
    return 1
  fi
  jq -n -c --arg file "image-$slot.$ext" --arg link "$ref" --arg type "image/${ext/jpg/jpeg}" --argjson bytes "$size" \
    '{file: $file, link: $link, type: $type, bytes: $bytes}' >>"$dir/.taken"
  image_bytes=$size
}

# shellcheck disable=SC2034  # the caller reads what this sets
tehas_issue_images() { # <issue> <dir>
  local issue=$1 images=$2 data who ref why used=0 count=0 taken
  image_why="the directory for the issue's images could not be made"
  { rm -rf "$images" && mkdir -p "$images"; } || return 1
  : >"$images/.taken"
  : >"$images/.left"
  image_why="the issue could not be read for its images"
  data=$(gh api "repos/$TEHAS_REPO/issues/$issue" 2>/dev/null) || return 1
  jq -e 'type == "object" and (.body == null or (.body | type) == "string")' <<<"$data" >/dev/null 2>&1 || return 1
  who=$(jq -r '.author_association // ""' <<<"$data")
  why=$tehas_image_author
  case "$who" in OWNER | MEMBER | COLLABORATOR) why="" ;; esac

  while IFS= read -r ref; do
    [ -n "$ref" ] || continue
    tehas_image_take "$ref" "$images" "$count" "$used" "$why" || return 1
    if [ -n "$image_left" ]; then
      jq -n -c --arg link "$ref" --arg reason "$image_left" '{link: $link, reason: $reason}' >>"$images/.left"
    else
      count=$((count + 1))
      used=$((used + image_bytes))
    fi
  done < <(jq -r "$(tehas_attachment_links)"'
    (.body // "") | links | reduce .[] as $x ([]; if any(.[]; . == $x) then . else . + [$x] end) | .[]' <<<"$data")

  image_why="the issue's images could not be written"
  taken=$(jq -s '.' "$images/.taken") || return 1
  jq -n --argjson issue "$issue" --argjson taken "$taken" --slurpfile left "$images/.left" \
    '{images: $taken, leftOut: $left} + {issue: $issue}' >"$images/manifest.json" || return 1
  rm -f "$images/.part" "$images/.taken" "$images/.left"
  { chmod 755 "$images" && chmod 644 "$images/manifest.json"; } || return 1
  image_why=""
}

# What an operator's reply adds to a session's images. The new files go in a
# directory of their own first, so that a reply image that cannot be fetched
# leaves the images of the earlier turns where they are; they and every link
# left out are then recorded beside the manifest as .added and .added.left,
# which is what tehas_images_added reads. The manifest says nothing about where
# an image came from.
# shellcheck disable=SC2034  # the caller reads what this sets
tehas_reply_images() { # <issue> <dir> <text> <login>
  local issue=$1 images=$2 text=$3 by=$4
  local stage=$images.new manifest=$images/manifest.json
  local ref why used count f allow
  image_why="the images already given to this session could not be read"
  if [ ! -e "$manifest" ]; then
    # A session that had no images yet: the reply is the first of them.
    { mkdir -p "$images" &&
      jq -n --argjson issue "$issue" '{issue: $issue, images: [], leftOut: []}' >"$manifest"; } ||
      { image_why="the directory for the session's images could not be made"; return 1; }
  fi
  jq -e --argjson issue "$issue" 'type == "object" and .issue == $issue and
    ((.images // []) | type == "array") and ((.leftOut // []) | type == "array")' \
    "$manifest" >/dev/null 2>&1 || return 1
  count=$(jq '(.images // []) | length' "$manifest") || return 1
  used=$(jq '[(.images // [])[].bytes // 0] | add // 0' "$manifest") || return 1

  image_why="the directory for the reply's images could not be made"
  { rm -rf "$stage" && mkdir -p "$stage"; } || return 1
  : >"$stage/.taken"
  : >"$stage/.left"
  allow=false
  why=$tehas_image_stranger
  if tehas_allowed_labeler "$by"; then { why=""; allow=true; }; fi

  # A link the session already has, or one it already left out for a reason
  # that still stands, is fetched once: quoting it again adds no file and no
  # line. A link left out only because of who wrote it is judged again, since
  # this reply may come from an account that may authorise work.
  while IFS= read -r ref; do
    [ -n "$ref" ] || continue
    tehas_image_take "$ref" "$stage" "$count" "$used" "$why" || { rm -rf "$stage"; return 1; }
    if [ -n "$image_left" ]; then
      jq -n -c --arg link "$ref" --arg reason "$image_left" '{link: $link, reason: $reason}' >>"$stage/.left"
    else
      count=$((count + 1))
      used=$((used + image_bytes))
    fi
  done < <(jq -r -n --slurpfile have "$manifest" --arg t "$text" --arg who "$tehas_image_stranger" \
    --argjson allowed "$allow" "$(tehas_attachment_links)"'
    ($have[0].images // []) as $got
    | ($have[0].leftOut // []) as $left
    | ($t // "") | links
    | reduce .[] as $x ([];
        if (any($got[]; .link == $x) or
            any($left[]; .link == $x and (.reason != $who or ($allowed | not))) or
            any(.[]; . == $x))
        then . else . + [$x] end)
    | .[]')

  image_why="the reply's images could not be written"
  jq -n --slurpfile have "$manifest" --slurpfile new "$stage/.taken" --slurpfile out "$stage/.left" \
    '{issue: $have[0].issue, images: (($have[0].images // []) + $new),
     leftOut: (($have[0].leftOut // []) + $out)}' >"$manifest.new" || { rm -rf "$stage"; return 1; }
  # The files before the record that names them, so a manifest never points at
  # a file that is not there; and the manifest last of all, so a turn that
  # reports a failure leaves the session's record of its images as it was, and
  # says nothing about images it did not give.
  for f in "$stage"/image-*; do
    [ -f "$f" ] || continue
    mv "$f" "$images/" || { rm -f "$manifest.new"; rm -rf "$stage"; return 1; }
  done
  mv "$stage/.taken" "$images/.added" || { rm -f "$manifest.new"; rm -rf "$stage"; return 1; }
  mv "$stage/.left" "$images/.added.left" ||
    { rm -f "$manifest.new" "$images/.added"; rm -rf "$stage"; return 1; }
  mv "$manifest.new" "$manifest" ||
    { rm -f "$manifest.new" "$images/.added" "$images/.added.left"; rm -rf "$stage"; return 1; }
  rm -rf "$stage"
  { chmod 755 "$images" && chmod 644 "$manifest" "$images/.added" "$images/.added.left"; } || return 1
  image_why=""
}

# The comment that says what a session or a cycle was given, and what was left
# out with the reason. The links are of the shape tehas_attachment_links takes
# and the reasons are written above, so nothing of the issue's own words is
# posted back. The sentence is one jq definition, read the same way by both.
#
# shellcheck disable=SC2016  # a jq program
tehas_images_said() {
  printf '%s\n' 'def said(what; from; images; left):
    (if (images | length) == 0 then "Tehas gave this \(what) no image from \(from)."
     else "Tehas gave this \(what) \(images | length) \(if (images | length) == 1 then "image" else "images" end) from \(from)." end)
    + (if (left | length) > 0 then "\n\nLeft out:\n" + (left | map("- \(.link): \(.reason)") | join("\n")) else "" end);'
}

tehas_issue_images_said() { # <dir> <what>
  local prog
  prog=$(tehas_images_said)
  jq -r "$prog"'
    select((.images | length) + (.leftOut | length) > 0)
    | said($what; "the issue text"; .images; .leftOut)' --arg what "$2" \
    "$1/manifest.json" 2>/dev/null
}

# The same words for one batch: what the last tehas_reply_images added.
tehas_images_added() { # <dir> <what> <source>
  local prog
  [ -e "$1/.added" ] && [ -e "$1/.added.left" ] || return 0
  prog=$(tehas_images_said)
  jq -r -n "$prog"'
    select(($taken | length) + ($left | length) > 0)
    | said($what; $from; $taken; $left)' --arg what "$2" --arg from "$3" \
    --slurpfile taken "$1/.added" --slurpfile left "$1/.added.left"
}
