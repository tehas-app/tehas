# shellcheck shell=bash
# Unpack a zip attached to a ticket, for tehasbot protolab. The zip comes from
# GitHub and anyone who may attach to the repository wrote it, so nothing in it
# is trusted: not its names, not its file types, not the sizes it claims.
#
#   tehas_unzip_into <zip> <dest dir> <most bytes> <most files>
#       print one JSON line per file written,
#         {"name":"<path in dest>","kind":"app|part|image|video","bytes":N}
#       and one per entry left out,
#         {"skip":"<name in the zip>","reason":"..."}
#       exit 0, also when entries were left out; exit 2 when the zip cannot
#       be read
#
# Every entry goes through these rules, in this order:
#   0. at most 200 entries are read; a line of the listing that is not an
#      entry is left out
#   1. only regular files, and entries with no Unix file type, which zip
#      libraries write for plain files; a directory is passed over, a link or
#      any other kind is left out, and so is an encrypted file
#   2. no path that starts with /, has a .. part, has a backslash, or is more
#      than six levels deep
#   3. each part of the path cleaned as protolab cleans a name; a part that
#      ends up empty leaves the entry out
#   4. only the types protolab hosts, and md
#   5. at most <most files> files
#   6. at most <most bytes> in all, counted on what unzip writes and not on
#      what the zip says; after the first file over it, nothing more is
#      unpacked and the rest are left out for the same reason
#   7. nothing written through a link: not when <dest dir> is one, nor when a
#      part of the entry's path that already stands under it is one
# Files are written by this function alone, with unzip -p into a pipe, so
# nothing outside <dest dir> is touched and no link is ever made, whatever
# type an entry claims. Mode 644.
#
# Sourcing it does nothing. It needs lib/attachments.sh, zipinfo and unzip.

# More entries than this and the rest of the zip is not read.
tehas_unzip_entries=200

_tehas_unzip_skip() { jq -n -c --arg n "${1:0:200}" --arg r "$2" '{skip: $n, reason: $r}'; }

tehas_unzip_into() {
  local zip=$1 dest=$2 left=$3 files=$4 listing line mode tx name rel part clean
  local kind ext depth size n=0 pattern full=0 link probe
  local -a parts st
  local re='^([^ ]+) +[^ ]+ +[^ ]+ +[0-9]+ +([^ ]+) +[0-9]+ +[^ ]+ +[^ ]+ +[0-9]+:[0-9]+ (.*)$'
  # zipinfo's own lines before and after the entries.
  local frame='^(Archive: |Zip file size: |[0-9]+ files?, )'

  listing=$(zipinfo -l "$zip" 2>/dev/null) || return 2
  mkdir -p "$dest" || return 2

  while IFS= read -r line; do
    [ -n "$line" ] || continue
    [[ $line =~ $frame ]] && continue
    n=$((n + 1))
    if [ "$n" -gt "$tehas_unzip_entries" ]; then
      _tehas_unzip_skip "..." "More than $tehas_unzip_entries entries in the zip."
      break
    fi
    if ! [[ $line =~ $re ]]; then
      _tehas_unzip_skip "$line" "Not an entry the lab can read."
      continue
    fi
    mode=${BASH_REMATCH[1]} tx=${BASH_REMATCH[2]} name=${BASH_REMATCH[3]}
    case "$mode" in
      d*) continue ;;
      -* | \?*) ;;
      *) _tehas_unzip_skip "$name" "A link or a special file."; continue ;;
    esac
    # zipinfo writes an encrypted entry's text/binary mark in capitals.
    case "$tx" in [A-Z]*) _tehas_unzip_skip "$name" "A locked file."; continue ;; esac

    IFS=/ read -r -a parts <<<"$name"
    depth=${#parts[@]}
    case "$name" in
      /* | *\\*) _tehas_unzip_skip "$name" "A path outside the archive."; continue ;;
    esac
    case "/$name/" in
      */../*) _tehas_unzip_skip "$name" "A path outside the archive."; continue ;;
    esac
    if [ "$depth" -gt 6 ]; then _tehas_unzip_skip "$name" "A path outside the archive."; continue; fi

    rel=""
    for part in "${parts[@]}"; do
      clean=$(printf '%s' "$part" | tr -c 'A-Za-z0-9._-' '-' | sed 's/^[.-]*//' | cut -c1-80)
      if [ -z "$clean" ]; then rel=""; break; fi
      rel=${rel:+$rel/}$clean
    done
    if [ -z "$rel" ]; then _tehas_unzip_skip "$name" "A name the lab cannot use."; continue; fi

    ext=""
    case "${rel##*/}" in *.*) ext=${rel##*.} ;; esac
    ext=${ext,,}
    if [ "$ext" = md ]; then
      kind=part
    else
      kind=$(tehas_attachment_kind "$ext") || { _tehas_unzip_skip "$name" "Not a type the lab hosts."; continue; }
    fi
    if [ "$files" -le 0 ]; then _tehas_unzip_skip "$name" "More than $4 files."; continue; fi
    if [ "$full" = 1 ]; then _tehas_unzip_skip "$name" "Over the 10 MB limit of a ticket."; continue; fi
    # Nothing is written through a link: the directory given is checked, and so is
    # every part of the path that already exists under it, before mkdir -p or the
    # write could follow one out of it.
    link=""
    [ ! -L "$dest" ] || link=1
    probe=$dest
    for part in ${rel//\// }; do
      probe=$probe/$part
      [ ! -L "$probe" ] || link=1
    done
    if [ -n "$link" ]; then _tehas_unzip_skip "$name" "A path outside the archive."; continue; fi
    if [ -e "$dest/$rel" ]; then _tehas_unzip_skip "$name" "A second file of that name."; continue; fi
    # A file of the zip may already stand where this one needs a folder.
    if [[ $rel == */* ]] && ! mkdir -p "$dest/${rel%/*}" 2>/dev/null; then
      _tehas_unzip_skip "$name" "A name the lab cannot use."
      continue
    fi

    # unzip reads a name as a pattern, and one starting with a dash as an
    # option: both are escaped so the entry is the one named.
    pattern=$(printf '%s' "$name" | sed 's/[][*?]/\\&/g; s/^-/\\-/')
    unzip -p "$zip" "$pattern" </dev/null 2>/dev/null | head -c $((left + 1)) >"$dest/$rel"
    st=("${PIPESTATUS[@]}")
    size=$(stat -c %s "$dest/$rel" 2>/dev/null) || size=0
    if [ "$size" -gt "$left" ]; then
      rm -f "$dest/$rel"
      full=1
      _tehas_unzip_skip "$name" "Over the 10 MB limit of a ticket."
      continue
    fi
    if [ "${st[0]}" != 0 ] || [ "${st[1]}" != 0 ]; then
      rm -f "$dest/$rel"
      _tehas_unzip_skip "$name" "Could not be unpacked."
      continue
    fi
    chmod 644 "$dest/$rel"
    left=$((left - size))
    files=$((files - 1))
    jq -n -c --arg n "$rel" --arg k "$kind" --argjson b "$size" '{name: $n, kind: $k, bytes: $b}'
  done <<<"$listing"
  return 0
}
