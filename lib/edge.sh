# shellcheck shell=bash
# What tehasbot protolab and tehasbot preview both do with a numbered set
# under $TEHAS_HOME: a ticket's pages, a pull request's preview. Sourced, not
# run. The first four need TEHAS_HOME and call nothing but the shell's own
# tools and flock. The last three ask Docker about the previews' network, and
# are here so that tehasbot preview and install.sh decide by one rule.
#
#   edge_lock <name> [<seconds>]   one run at a time
#   edge_put <file> <command...>   a file that is whole or not there
#   edge_place <new> <place>       a directory that is whole or the old one
#   edge_numbers <directory>       the numbered directories in it
#   edge_docker_fault              why this Docker cannot keep the host out
#   edge_net_create <network>      make the previews' network
#   edge_net_fault <network>       why that network does not keep the host out

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

# --- The previews' network ---------------------------------------------------
# A preview runs a pull request's own code, so its network has to keep it from
# the host. What does that is checked, not a name: the network is internal
# (no way out), a bridge (the option below means nothing to another driver),
# without IPv6 (the option is about IPv4 only, and with IPv6 the host has an
# address of that kind on the bridge), and made with gateway_mode_ipv4
# "isolated" (the host has no IPv4 address on the bridge) by a Docker that
# knows that option. Docker before 27.1 does not know it. What is known: the
# bridge driver of Docker 29.1.3 was seen to accept an option it does not know
# and to show it when asked. That an older Docker takes this one and ignores
# it was not tried. The version is checked so that it does not matter.
edge_net_option=com.docker.network.bridge.gateway_mode_ipv4

# edge_docker_fault: prints why and returns 1 when the Docker server is older
# than 27.1 or does not say a version that can be read.
edge_docker_fault() {
  local v
  v=$(docker version --format '{{.Server.Version}}' 2>/dev/null) || v=""
  if ! [[ $v =~ ^([0-9]{1,4})\.([0-9]{1,4})([.+~-][0-9A-Za-z.+~-]*)?$ ]]; then
    echo "Docker's version could not be read"
    return 1
  fi
  if [ "$((10#${BASH_REMATCH[1]}))" -lt 27 ] ||
    { [ "$((10#${BASH_REMATCH[1]}))" -eq 27 ] && [ "$((10#${BASH_REMATCH[2]}))" -lt 1 ]; }; then
    echo "Docker $v is older than 27.1"
    return 1
  fi
}

# edge_net_create <network>: the one command that makes the network.
edge_net_create() {
  docker network create --internal --ipv6=false -o "$edge_net_option=isolated" "$1" >/dev/null 2>&1
}

# edge_net_fault <network>: says nothing and returns 0 when the network keeps
# the host out. Otherwise prints why and returns 1, or 2 when Docker shows no
# such network at all. It does not ask for Docker's version: a caller asks
# edge_docker_fault first.
edge_net_fault() {
  local said internal driver ipv6 mode
  if ! said=$(docker network inspect \
    --format "{{.Internal}} {{.Driver}} {{.EnableIPv6}} {{index .Options \"$edge_net_option\"}}" "$1" 2>/dev/null); then
    echo "it could not be read"
    return 2
  fi
  read -r internal driver ipv6 mode <<<"$said"
  if [ "$internal" != true ]; then
    echo "it is not internal"
  elif [ "$driver" != bridge ]; then
    echo "its driver is not bridge"
  elif [ "$ipv6" != false ]; then
    echo "it has IPv6"
  elif [ "$mode" != isolated ]; then
    echo "it is not isolated from the host"
  else
    return 0
  fi
  return 1
}
