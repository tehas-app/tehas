# shellcheck shell=bash
# The one reader of the host's free disk space. Sourced by the dispatcher,
# which starts no cycle below TEHAS_CYCLE_MIN_FREE, and by the previews, which
# build none below TEHAS_PREVIEW_MIN_FREE. Never by lib/config.sh or a login
# shell: it calls df and docker, and loading settings must not.
#
#   tehas_free_kb <path> [<seconds>]   the free kilobytes of the file system of
#                                    <path> and of the one Docker keeps its
#                                    images on, whichever has less, as df's
#                                    1024-blocks. Returns 1 and prints nothing
#                                    when either figure cannot be read: no
#                                    figure is never the same as plenty, and a
#                                    command that starts work on a guess would
#                                    start it on a host whose df or Docker is
#                                    broken. The caller names the file system
#                                    it counts, so the two of them can be told
#                                    apart. Every outside call runs under
#                                    "timeout 10" (the seconds the caller
#                                    gives, or TEHAS_DISK_WAIT in the tests)
#                                    so a silent one cannot block a poll.
#   tehas_docker_dir                 the directory Docker keeps its images in,
#                                    as Docker itself names it. Empty when
#                                    Docker is not there at all, which is the
#                                    host of /var/lib/docker.

# A figure df prints for a real file system: a whole number that still fits a
# 64-bit one, so the caller's arithmetic cannot wrap it into another figure.
_tehas_disk_num() { [[ ${1:-} =~ ^(0|[1-9][0-9]{0,17})$ ]]; }

_tehas_disk_wait() { # [<seconds>]: the bound of this read, 10 on a host
  local n=${1:-${TEHAS_DISK_WAIT:-}}
  [[ $n =~ ^[1-9][0-9]?$ ]] || n=10
  printf '%s\n' "$n"
}

# One df, bounded, and its awk inside the bound: "timeout N df | awk" would
# leave the status of the pipeline to awk and the timeout to df's own part
# alone. The fourth column of the second line is what is wanted. The command is
# in single quotes on purpose: the $1 and $4 there are the inner sh's and
# awk's, not this function's.
# shellcheck disable=SC2016  # the df and awk program below are quoted for sh
_tehas_disk_kb() { # <path> <seconds>
  local figure
  figure=$(timeout "${2}" sh -c 'df -Pk "$1" 2>/dev/null | awk '\''NR == 2 {print $4}'\''' _ "$1") || return 1
  _tehas_disk_num "$figure" || return 1
  printf '%s\n' "$figure"
}

# Docker's own answer for where its images live. A call that is not answered in
# time is no answer at all, and then the default of a host without Docker is
# the file system to read: what Docker says is not guessed for it.
tehas_docker_dir() { # [<seconds>]
  local out
  out=$(timeout "$(_tehas_disk_wait "${1:-}")" docker info --format '{{.DockerRootDir}}' 2>/dev/null </dev/null) || return 0
  printf '%s\n' "$out"
}

tehas_free_kb() { # <path> [<seconds>]
  local wait_kb home_kb docker_dir docker_kb
  [ -n "${1:-}" ] || return 1
  wait_kb=$(_tehas_disk_wait "${2:-}")
  home_kb=$(_tehas_disk_kb "$1" "$wait_kb") || return 1
  docker_dir=$(tehas_docker_dir "$wait_kb") || return 1
  docker_kb=$(_tehas_disk_kb "${docker_dir:-/var/lib/docker}" "$wait_kb") || return 1
  printf '%s\n' "$((home_kb < docker_kb ? home_kb : docker_kb))"
}
