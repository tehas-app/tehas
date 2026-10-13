# shellcheck shell=bash
# Files attached to an issue: which links are GitHub's attachment storage,
# which types are known, and how one file is fetched. Sourced by the two
# commands that fetch them: tehasbot protolab (a ticket's page) and tehas work
# (the images a cycle is given). It is in one place so they cannot drift apart.
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
# Sourcing it does nothing. Which of the known types a command takes, and how
# much, is the command's own rule.

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
