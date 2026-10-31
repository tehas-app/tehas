#!/usr/bin/env bash
# Tests for libexec/mcp: the settings it refuses, the container it runs the MCP
# server in, and the keys it makes.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

mcp="$T_ROOT/libexec/mcp"
th=$T/home/tehas
key1=abcdefghijklmnopqrstuvwxyz012345
key2=ABCDEFGHIJKLMNOPQRSTUVWXYZ-_6789
digest=sha256:$(printf 'a%.0s' {1..64})
t_env() { T_ENV=("$@"); }

reset() {
  rm -rf "$th" "$T/calls"
  mkdir -p "$th/libexec" "$th/live" "$th/docs" "$T/calls"
  echo "binary" >"$th/libexec/tehas-mcp"
  chmod 755 "$th/libexec/tehas-mcp"
  printf 'CADDY_VERSION=2.8.4\nCADDY_DIGEST=%s\n' "$digest" >"$th/versions.env"
  echo "v0.7.30+abc1234" >"$th/docs/version.txt"
  # The network the server runs on exists, and lets nothing out.
  t_stub docker 'case "$*" in "network inspect"*) echo false ;; esac
[ "$1" != run ] || printf "%s\n%s\n" "${TEHAS_MCP_KEYS:-none}" "${TEHAS_MCP_DASHBOARD_KEYS:-none}" >"$T/keys.seen"'
  t_instance "${T_MINIMAL[@]}"
  t_env
}

t_case "off or unset, it runs nothing and removes a container left running"
for setting in "" TEHAS_MCP=0; do
  reset
  [ -z "$setting" ] || t_env "$setting" TEHAS_MCP_KEYS="$key1"
  t_run "$mcp serve"
  t_eq "$t_rc" 0
  t_eq "$(t_calls docker)" "rm -f tehas-mcp"
done

# Both settings go through the one check of their shape, since a key of either
# reaches the same program: at least one key stands in one list or the other,
# and every key of the right shape. The name of a setting is said and never the
# value of a key. That the server reads the two lists apart, refuses a host with
# no key at all, and refuses a key of both lists whose scope would be a guess,
# are the server's own rules and are tested of the binary.
#
# One run per row of: full list, dashboard list, the status, a word the line
# must hold, and a key of neither that the line must not hold. The keys reach
# the container by name, from the environment, so the two files the stub writes
# are read as the values it was given.
t_case "the keys of both lists are checked, and both reach the container by name"
# The row's last field is the two values the container was given, which is also
# how the run is said to have started at all: a run of no keys never got there.
check() { # <full> <dashboard> <rc> <a word the line must hold> <the two values>
  reset
  t_env TEHAS_MCP=1 "TEHAS_MCP_KEYS=$1" "TEHAS_MCP_DASHBOARD_KEYS=$2"
  t_run "$mcp serve"
  t_eq "$t_rc" "$3"
  [ -z "$4" ] || t_contains "$t_out" "$4"
  [ -z "$5" ] || t_eq "$(cat "$T/keys.seen" 2>/dev/null)" "$5"
  [ -z "$1" ] || t_lacks "$t_out" "$1"
  [ -z "$2" ] || t_lacks "$t_out" "$2"
}
check "$key1" "" 0 "" "$key1
none"
check "" "$key1" 0 "" "none
$key1"
check "$key1" "$key2" 0 "" "$key1
$key2"
check "" "" 78 "TEHAS_MCP_DASHBOARD_KEYS" ""
check "   " "   " 78 "TEHAS_MCP_KEYS" ""
check "${key1:0:31}" "" 78 "TEHAS_MCP_KEYS" ""
check "$key1" "${key2:0:31}+" 78 "TEHAS_MCP_DASHBOARD_KEYS" ""
check "$key1" "$key1" 0 "" "$key1
$key1"
check "$key1 $key2" "$key2" 0 "" "$key1 $key2
$key2"
check "$key1 $key2" "$key1" 0 "" "$key1 $key2
$key1"
reset
t_env TEHAS_MCP=1 TEHAS_MCP_KEYS="$key1 $key2" TEHAS_MCP_DASHBOARD_KEYS="$key2"
t_run "$mcp serve"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "$key2"
# The run line of a server that started names both settings and holds no key of
# either, which the rows above say of the checks that refused to start.
reset
t_env TEHAS_MCP=1 TEHAS_MCP_KEYS="$key1" TEHAS_MCP_DASHBOARD_KEYS="$key2"
t_run "$mcp serve"
t_contains "$(t_calls docker)" "-e TEHAS_MCP_KEYS -e TEHAS_MCP_DASHBOARD_KEYS --entrypoint"
t_lacks "$(t_calls docker)" "$key1"
t_lacks "$(t_calls docker)" "$key2"

t_case "a value of TEHAS_MCP other than 0 or 1 is 78"
reset
t_env TEHAS_MCP=yes TEHAS_MCP_KEYS="$key1"
t_run "$mcp serve"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_MCP"
t_eq "$(t_calls docker)" ""

t_case "keys in the instance directory are refused: it is in a repository"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_MCP_KEYS="$key1"
t_env TEHAS_MCP=1
t_run "$mcp serve"
t_eq "$t_rc" 78
t_contains "$t_out" ".config/tehas/env"
t_eq "$(t_calls docker)" ""

# The container is a security boundary: these are all the arguments it gets.
# A change here widens or narrows what a caller on the internet can reach.
t_case "on, it runs the binary in a locked-down container with live/, mcp/ and logs/ read-only and nothing else"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_HOST_ADDR=192.0.2.9 PI_ENSEMBLE_HOST_ALIASES=gpu1:192.0.2.10
mkdir -p "$T/home/.pi/agent"
echo '{"providers":{"test-provider":{"baseUrl":"http://models.lan:8086/v1","apiKey":"unused"}}}' \
  >"$T/home/.pi/agent/models.json"
printf '[user]\n\tname = Test Bot\n\temail = bot@example.com\n' >"$T/home/.gitconfig"
t_env TEHAS_MCP=1 TEHAS_MCP_KEYS="$key1	$key2"
t_run "$mcp serve"
t_eq "$t_rc" 0
t_eq "$(t_calls docker)" "network inspect --format {{index .Options \"com.docker.network.bridge.enable_ip_masquerade\"}} tehas-mcp
rm -f tehas-mcp
run --rm --init --name tehas-mcp --user $(id -u):$(id -g) --network tehas-mcp --read-only --cap-drop ALL --security-opt no-new-privileges:true --memory 256m --pids-limit 64 -p 192.0.2.9:8082:8082 -v $th/live:/live:ro -v $th/mcp:/sessions:ro -v $th/logs:/logs:ro -v $th/libexec/tehas-mcp:/tehas-mcp:ro -e TEHAS_MCP_KEYS -e TEHAS_MCP_DASHBOARD_KEYS --entrypoint /tehas-mcp caddy@$digest --live /live --sessions /sessions --logs /logs --listen 0.0.0.0:8082 --server-version v0.7.30+abc1234 --private gpu1 --private 192.0.2.10 --private models.lan --private bot@example.com"
t_lacks "$(t_calls docker)" "$key1"
t_lacks "$(t_calls docker)" "$key2"
rm -rf "$T/home/.pi" "$T/home/.gitconfig"

t_case "the private names are those the console is given: none under 4 characters, none without settings"
reset
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=gb:192.0.2.10,,v6host:2001:db8::1
t_env TEHAS_MCP=1 TEHAS_MCP_KEYS="$key1"
t_run "$mcp serve"
t_eq "$t_rc" 0
t_eq "$(t_calls docker | grep -e '^run ' | grep -o -e '--server-version .*')" \
  "--server-version v0.7.30+abc1234 --private 192.0.2.10 --private v6host --private 2001:db8::1"
reset
t_env TEHAS_MCP=1 TEHAS_MCP_KEYS="$key1"
t_run "$mcp serve"
t_lacks "$(t_calls docker)" "--private"

t_case "it makes the snapshot's directory private before the container runs, so Docker never makes it"
reset
t_stub docker 'case "$*" in "network inspect"*) echo false ;; esac
[ "$1" != run ] || stat -c %a "$TEHAS_HOME/mcp" >"$T/mode.seen"'
t_env TEHAS_MCP=1 TEHAS_MCP_KEYS="$key1"
t_run "$mcp serve"
t_eq "$t_rc" 0
t_eq "$(cat "$T/mode.seen" 2>/dev/null)" 700
t_eq "$(stat -c %a "$th/mcp")" 700
reset
mkdir -m 755 "$th/mcp"
t_env TEHAS_MCP=1 TEHAS_MCP_KEYS="$key1"
t_run "$mcp serve"
t_eq "$(stat -c %a "$th/mcp")" 700

t_case "without its network it makes one that has no way out"
reset
t_stub docker 'case "$*" in "network inspect"*) exit 1 ;; esac'
t_env TEHAS_MCP=1 TEHAS_MCP_KEYS="$key1"
t_run "$mcp serve"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "network create --driver bridge -o com.docker.network.bridge.enable_ip_masquerade=false tehas-mcp"
t_contains "$(t_calls docker)" "--network tehas-mcp"

t_case "a network of that name that lets the container out is refused"
reset
t_stub docker 'case "$*" in "network inspect"*) echo "" ;; esac'
t_env TEHAS_MCP=1 TEHAS_MCP_KEYS="$key1"
t_run "$mcp serve"
t_eq "$t_rc" 1
t_contains "$t_out" "tehas-mcp"
t_lacks "$(t_calls docker)" "run "

t_case "without a host address it binds the loopback address"
reset
t_env TEHAS_MCP=1 TEHAS_MCP_KEYS="$key1"
t_run "$mcp serve"
t_contains "$(t_calls docker)" " -p 127.0.0.1:8082:8082 "

t_case "a missing binary or digest is 1, and runs nothing"
reset
rm "$th/libexec/tehas-mcp"
t_env TEHAS_MCP=1 TEHAS_MCP_KEYS="$key1"
t_run "$mcp serve"
t_eq "$t_rc" 1
t_contains "$t_out" "install.sh"
t_lacks "$(t_calls docker)" "run "
reset
echo "CADDY_DIGEST=latest" >"$th/versions.env"
t_env TEHAS_MCP=1 TEHAS_MCP_KEYS="$key1"
t_run "$mcp serve"
t_eq "$t_rc" 1
t_lacks "$(t_calls docker)" "run "

t_case "a key is 43 characters of the URL-safe alphabet, new each time, and nothing is written"
reset
sleep 0.01
touch "$T/before-key"
t_run "$mcp key"
t_eq "$t_rc" 0
first=$t_out
[[ $first =~ ^[A-Za-z0-9_-]{43}$ ]] || t_bad "not a key: $first"
t_run "$mcp key"
[ "$t_out" != "$first" ] || t_bad "the same key twice"
t_eq "$(find "$th" -newer "$T/before-key" -type f | wc -l)" 0

t_case "an unknown argument is 64"
reset
t_run "$mcp nope"
t_eq "$t_rc" 64

t_case "the manual leaves /mcp to the server: no page there, and no link to it"
docs=$T_ROOT/host/docs
[ ! -e "$docs/mcp.md" ] || t_bad "host/docs/mcp.md would be served at /mcp"
[ -f "$docs/mcp-server.md" ] || t_bad "host/docs/mcp-server.md is missing"
t_eq "$(grep -rlE '\]\(/mcp\)|href="/mcp"|/mcp\.md\)' "$docs" | wc -l)" 0

t_case "the manual keeps the server's deadline at 10 seconds and names the flag it is given by"
# libexec/mcp gives the binary no --io-timeout, so the only deadline a host ever
# waits is the default the manual states.
grep -q "must arrive whole within 10 seconds" "$docs/mcp-server.md" \
  || t_bad "the manual no longer gives the 10 second deadline"
grep -q -- "--io-timeout" "$docs/mcp-server.md" \
  || t_bad "the manual does not name --io-timeout"

t_case "the flag is the binary's, not the container's: the run line does not hold it"
reset
t_env TEHAS_MCP=1 TEHAS_MCP_KEYS="$key1"
t_run "$mcp serve"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "--io-timeout"

t_done
