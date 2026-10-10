# shellcheck shell=bash
# What tehasbot protolab and tehasbot preview both do with a numbered set
# under $TEHAS_HOME: a ticket's pages, a pull request's preview. Sourced, not
# run. It needs TEHAS_HOME and calls nothing but the shell's own tools and
# flock.
#
#   edge_lock <name> [<seconds>]   one run at a time
#   edge_put <file> <command...>   a file that is whole or not there
#   edge_place <new> <place>       a directory that is whole or the old one
#   edge_numbers <directory>       the numbered directories in it

# edge_lock <name> [<seconds to wait>]: take $TEHAS_HOME/state/<name>.lock
# until this process ends. Returns 1 when another process holds it.
edge_lock() {
  mkdir -p "$TEHAS_HOME/state" || return 1
  exec 9>"$TEHAS_HOME/state/$1.lock" || return 1
  if [ -n "${2:-}" ]; then flock -w "$2" 9; else flock -n 9; fi
}

# edge_put <file> <command...>: the command's output becomes the file. A
# server may read the file at any moment, so it is written beside itself and
# renamed. A command that fails leaves the file as it was.
edge_put() {
  local file=$1
  shift
  if ! "$@" >"$file.new"; then
    rm -f "$file.new"
    return 1
  fi
  mv "$file.new" "$file"
}

# edge_place <new directory> <place>: the new one takes the place by rename.
# The old one is moved aside first and removed afterwards, so there is no
# moment with a half-copied directory.
edge_place() {
  local new=$1 place=$2 old
  old=$(dirname "$place")/.old
  rm -rf "$old"
  if [ -d "$place" ]; then mv "$place" "$old" || return 1; fi
  mv "$new" "$place" || return 1
  rm -rf "$old"
}

# edge_numbers <directory>: its subdirectories whose name is a number.
edge_numbers() {
  local dir n
  for dir in "$1"/*/; do
    [ -d "$dir" ] || continue
    n=$(basename "$dir")
    case "$n" in *[!0-9]* | "") continue ;; esac
    echo "$n"
  done
  return 0
}
