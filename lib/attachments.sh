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

# The images an issue links, for a container that is given them: tehas work
# and tehas plan. The rules are written in libexec/work's header.
#
#   tehas_issue_images <issue> <dir> [approved]
#                                     remake <dir> from the issue's text, with
#                                     a manifest.json; uses TEHAS_REPO and
#                                     GH_TOKEN. Returns 1, with the reason in
#                                     $image_why, when the issue's images
#                                     cannot be given and nothing may start.
#                                     "approved" says an allowed account
#                                     approved the text with its label: the
#                                     images are then taken whoever opened the
#                                     issue. Without it, only when an owner,
#                                     member or collaborator opened it
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

# shellcheck disable=SC2034  # the caller reads what this sets
tehas_issue_images() { # <issue> <dir> [approved]
  local issue=$1 images=$2 approved=${3:-} data who ref name ext size rc left used=0 count=0
  local limit=$((10 * 1024 * 1024)) max=8
  local other="Not a png, jpg, webp or gif image." over="Over the 10 MB limit of an issue."
  image_why="the directory for the issue's images could not be made"
  { rm -rf "$images" && mkdir -p "$images"; } || return 1
  : >"$images/.taken"
  : >"$images/.left"
  leave() { jq -n -c --arg link "$1" --arg reason "$2" '{link: $link, reason: $reason}' >>"$images/.left"; }
  image_why="the issue could not be read for its images"
  data=$(gh api "repos/$TEHAS_REPO/issues/$issue" 2>/dev/null) || return 1
  jq -e 'type == "object" and (.body == null or (.body | type) == "string")' <<<"$data" >/dev/null 2>&1 || return 1
  who=$(jq -r '.author_association // ""' <<<"$data")

  while IFS= read -r ref; do
    [ -n "$ref" ] || continue
    case "$approved:$who" in
      approved:* | *:OWNER | *:MEMBER | *:COLLABORATOR) ;;
      *) leave "$ref" "The issue was not opened by an owner, member or collaborator, and its label was not applied by an allowed account."; continue ;;
    esac
    # A file attached under its name is not fetched when the name says it is
    # no image. A pasted one has no name: its bytes say.
    case "$ref" in
      https://github.com/user-attachments/assets/* | https://github.com/*/assets/*) ;;
      *)
        name=${ref##*/}
        name=${name%%\?*}
        ext=""
        case "$name" in *.*) ext=${name##*.} ;; esac
        case "${ext,,}" in png | jpg | jpeg | webp | gif) ;; *) leave "$ref" "$other"; continue ;; esac
        ;;
    esac
    if [ "$count" -ge "$max" ]; then leave "$ref" "More than $max images."; continue; fi
    left=$((limit - used))
    if [ "$left" -le 0 ]; then leave "$ref" "$over"; continue; fi

    rm -f "$images/.part"
    tehas_attachment_fetch "$ref" "$images/.part" "$left" "$GH_TOKEN" >/dev/null
    rc=$?
    case "$rc" in
      0) ;;
      63) leave "$ref" "$over"; continue ;;
      *) image_why="$ref could not be fetched"; return 1 ;;
    esac
    size=$(stat -c %s "$images/.part" 2>/dev/null) || size=0
    if [ "$size" -gt "$left" ]; then leave "$ref" "$over"; continue; fi
    ext=$(tehas_image_type "$images/.part") || { leave "$ref" "$other"; continue; }
    count=$((count + 1))
    used=$((used + size))
    if ! { mv "$images/.part" "$images/image-$count.$ext" && chmod 644 "$images/image-$count.$ext"; }; then
      image_why="the issue's images could not be written"
      return 1
    fi
    jq -n -c --arg file "image-$count.$ext" --arg link "$ref" --arg type "image/${ext/jpg/jpeg}" --argjson bytes "$size" \
      '{file: $file, link: $link, type: $type, bytes: $bytes}' >>"$images/.taken"
  done < <(jq -r "$(tehas_attachment_links)"'
    (.body // "") | links | reduce .[] as $x ([]; if any(.[]; . == $x) then . else . + [$x] end) | .[]' <<<"$data")

  image_why="the issue's images could not be written"
  jq -n --argjson issue "$issue" --slurpfile images "$images/.taken" --slurpfile left "$images/.left" \
    '{images: $images, leftOut: $left} + {issue: $issue}' >"$images/manifest.json" || return 1
  rm -f "$images/.part" "$images/.taken" "$images/.left"
  { chmod 755 "$images" && chmod 644 "$images/manifest.json"; } || return 1
  image_why=""
}

# The links are of the shape tehas_attachment_links takes and the reasons are
# written above, so nothing of the issue's own words is posted back. A comment
# does not say the images were given when the agents are not shown them.
tehas_issue_images_said() { # <dir> <what> [yes|no|unknown]
  jq -r --arg what "$2" --arg reads "${3:-yes}" 'select((.images | length) + (.leftOut | length) > 0)
    | "\(.images | length) \(if (.images | length) == 1 then "image" else "images" end) from the issue text" as $n
    | if $reads == "no" then "Tehas fetched \($n) for this \($what), but the pi-rukas in the sandbox image does not read them: its agents are not shown them."
      elif $reads == "yes" then "Tehas gave this \($what) \($n)."
      else "Tehas gave this \($what) \($n). Whether the pi-rukas in the sandbox image reads them could not be checked." end
      + (if (.leftOut | length) > 0 then "\n\nLeft out:\n" + (.leftOut | map("- \(.link): \(.reason)") | join("\n")) else "" end)' \
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
