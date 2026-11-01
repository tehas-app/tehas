# shellcheck shell=bash
# What tehasbot protolab and tehasbot preview both do with a numbered set
# under $TEHAS_HOME: a ticket's pages, a pull request's preview. Sourced, not
# run. These need TEHAS_HOME and call nothing but the shell's own tools and
# flock. The console has the same four in console/src/edge.rs, beside the
# previews' network.
#
#   edge_lock <name> [<seconds>]   one run at a time, on the descriptor bash
#                                  chose, named in edge_lock_fd
#   edge_put <file> <command...>   a file that is whole or not there
#   edge_place <new> <place>       a directory that is whole or the old one
#   edge_numbers <directory>       the numbered directories in it

# edge_lock <name> [<seconds to wait>]: take $TEHAS_HOME/state/<name>.lock
# until this process ends. Returns 1 when another process holds it.
# The descriptor is the one bash chose, and edge_lock_fd names it: a fixed
# number is a number every other command here would have to guess at and step
# on. A command that can outlive the run has to be given {edge_lock_fd}>&-,
# unquoted: a quoted one is an argument, not a redirection, and the descriptor
# the child inherited then holds the lock after the run has ended. Only a
# successful edge_lock sets edge_lock_fd, so a caller that never called one
# cannot write it (bash: ambiguous redirect).
edge_lock() {
  mkdir -p "$TEHAS_HOME/state" || return 1
  exec {edge_lock_fd}>"$TEHAS_HOME/state/$1.lock" || return 1
  if [ -n "${2:-}" ]; then flock -w "$2" "$edge_lock_fd"; else flock -n "$edge_lock_fd"; fi
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
