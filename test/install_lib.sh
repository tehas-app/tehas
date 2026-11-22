# shellcheck shell=bash
# What the tests of install.sh share: the scratch host, the stubs for Docker,
# systemctl and cargo, a valid instance, and the files a fresh install puts.
# Source this from an install_*_test.sh file, after test/lib.sh.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck disable=SC2034  # the variables here are read by the test files

# install_fixture <checkout> <dir>: <checkout>'s tracked files as the working
# tree holds them, deleted ones left out, so an untracked file cannot change what
# an install sees. With no .git it names itself as a release tree does: COMMIT,
# and VERSION as <newest tag>+<short hash>, the hash alone with no tag.
install_fixture() { # <checkout> <dir>
  local from=$1 to=$2 kept=$T/ls-files deleted=$T/ls-deleted tag hash
  # A tree whose tracked files cannot be read fails here, not as a fixture of
  # part of them.
  rm -rf "$to"
  mkdir -p "$to"
  git -C "$from" ls-files -z >"$kept" || return 1
  git -C "$from" ls-files -z --deleted >"$deleted" || return 1
  (cd "$from" && comm -z -23 "$kept" "$deleted" |
    xargs -0 -r cp -P --parents -t "$to/") || return 1
  rm -f "$kept" "$deleted"
  git -C "$from" rev-parse HEAD >"$to/COMMIT"
  hash=$(git -C "$from" rev-parse --short=7 HEAD)
  tag=$(git -C "$from" describe --tags --abbrev=0 2>/dev/null) || tag=""
  printf '%s\n' "${tag:+$tag+}$hash" >"$to/VERSION"
}
# The tree every install here runs, built once in $T so no case writes into the
# checkout. A case that changes the tree copies it first.
fixture=$T/tree
install_fixture "$T_ROOT" "$fixture"
install="$fixture/install.sh"
home=$T/home
th=$home/tehas
# The two files of bootstrap.sh dev that install.sh reads, outside any home: in
# scratch, so no test touches the real /etc or /usr/local/bin.
dev_etc=$T/etc
dev_bin=$T/usr-local-bin
console_env=()
# shellcheck disable=SC2120  # the parts pass it variables, this file does not
t_env() { T_ENV=(BOOTSTRAP_ETC="$dev_etc" BOOTSTRAP_BIN="$dev_bin" T_IN_TEST=1 "${console_env[@]}" "$@"); }
t_env
# real_console: installs from here on hold the console of this checkout, not the
# stub's line of text. A file that pins what install.sh asks Docker needs it.
real_console() {
  # shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
  t_console || t_bad "no console: $t_console_why"
  console_env=(CONSOLE_BIN="$T_CONSOLE")
  t_env
}

# The previews' network as install.sh wants to find it, and how it asks.
net_good='case "$1 $2" in "version --format") echo 29.1.3 ;; "network inspect") echo "true bridge false isolated" ;; esac'
net_inspect='version --format {{.Server.Version}}
network inspect --format {{.Internal}} {{.Driver}} {{.EnableIPv6}} {{index .Options "com.docker.network.bridge.gateway_mode_ipv4"}} tehas-previews'
t_stub docker "$net_good"
t_stub systemctl
# A first install makes the labels in the product repository.
t_stub gh
t_stub_cargo
# What cargo stands in for is no program, and the labels and tehas api are verbs
# of the console: these run the console of this checkout.
# shellcheck disable=SC2154  # t_console_why is set by test/lib.sh
t_console || t_bad "$t_console_why"
T_APP_WORLD=0
T_ENV_ALWAYS+=(TEHAS_CONSOLE="$T_CONSOLE" GH_TOKEN=ghs_test)

# An instance directory with settings and a model list that matches them.
# shellcheck disable=SC2120  # some parts never pass it settings
instance() {
  t_instance "${T_MINIMAL[@]}" "$@"
  cat >"$TEHAS_INSTANCE_DIR/models.json" <<'EOF'
{"providers": {"test-provider": {"models": [{"id": "test-model"}, {"id": "session-model"}]}}}
EOF
}
# Everything install.sh put on the host, relative to the scratch home.
installed_files() { (cd "$home" && find . -type f ! -name '*.prev' | LC_ALL=C sort); }
# The host facts after an install as one JSON object, from `tehas api --json`
# in a clean environment, as the factory's user would run it.
# shellcheck disable=SC2154  # t_out is set by test/lib.sh, which the part sources first
api_json() { t_run "'$th/bin/tehas' api --json"; printf '%s\n' "$t_out"; }

fresh() {
  rm -rf "$home" "$T"/calls/*
  mkdir -p "$home"
}

files="./.bashrc
./.config/systemd/user/tehas-cycle@.service
./.config/systemd/user/tehas-clean.service
./.config/systemd/user/tehas-clean.timer
./.config/systemd/user/tehas-dispatch.path
./.config/systemd/user/tehas-dispatch.service
./.config/systemd/user/tehas-dispatch.timer
./.config/systemd/user/tehas-mcp-fetch.socket
./.config/systemd/user/tehas-mcp-fetch@.service
./.config/systemd/user/tehas-mcp.service
./.config/systemd/user/tehas-preview-sweep.service
./.config/systemd/user/tehas-preview-sweep.timer
./.config/systemd/user/tehas-preview.service
./.config/systemd/user/tehas-preview.timer
./.config/systemd/user/tehas-protolab.service
./.config/systemd/user/tehas-protolab.timer
./.config/systemd/user/tehas-top.service
./.pi/agent/mcp.json
./.pi/agent/models.json
./.pi/agent/settings.json
./tehas/bin/tehas
./tehas/bin/tehasbot
./tehas/edge/docs.Caddyfile
./tehas/edge/docs.compose.yaml
./tehas/edge/router.Caddyfile
./tehas/edge/router.compose.yaml
./tehas/env.sh
./tehas/lib/config.sh
./tehas/lib/github-app.sh
./tehas/lib/issue-brief.md
./tehas/lib/labels.tsv
./tehas/lib/paths.sh
./tehas/lib/refine-brief.md
./tehas/lib/release.list
./tehas/libexec/analytics
./tehas/libexec/analytics.stamp
./tehas/libexec/attach
./tehas/libexec/attach.stamp
./tehas/libexec/console
./tehas/libexec/console.stamp
./tehas/libexec/dev-login
./tehas/libexec/setup
./tehas/libexec/store
./tehas/libexec/store.stamp
./tehas/libexec/tehas-cycle
./tehas/libexec/tehas-cycle.stamp
./tehas/libexec/tehas-dispatch
./tehas/libexec/tehas-dispatch.stamp
./tehas/libexec/tehas-mcp
./tehas/libexec/tehas-mcp.stamp
./tehas/libexec/update
./tehas/npm/ctx7/package-lock.json
./tehas/npm/ctx7/package.json
./tehas/npm/parallel-web-cli/package-lock.json
./tehas/npm/parallel-web-cli/package.json
./tehas/npm/pi/package-lock.json
./tehas/npm/pi/package.json
./tehas/release/minisign.pub
./tehas/sandbox/Dockerfile
./tehas/sandbox/base.Dockerfile
./tehas/sandbox/build.sh
./tehas/sandbox/gh-shim
./tehas/sandbox/npm/playwright/package-lock.json
./tehas/sandbox/npm/playwright/package.json
./tehas/shim/gh
./tehas/versions.env"
# Left out of the exact comparison and checked on their own: the manual and the
# front page, whole directories, and what is the host's own, not install.sh's:
# state/ and the marker that says the home is tehas's, which nothing removes.
install_marker=.tehas-home
core_files() {
  installed_files | grep -v -e '^./tehas/docs/' -e '^./tehas/fun/' -e '^./tehas/state/' -e "^./tehas/$install_marker"
}
sorted() { printf '%s\n' "$@" | LC_ALL=C sort; }
# Every file under the scratch home but the update lock, which a real run takes
# before it builds or copies anything.
host_files() { find "$home" -type f ! -path "$th/state/update.lock"; }

# The follow-ups a fresh install of this repository asks for, taken from the one
# place that names them and the sources a fresh host has none of.
next_steps() {
  "$T_CONSOLE" update steps for systemd/tehas-cycle@.service systemd/tehas-clean.service systemd/tehas-clean.timer \
    systemd/tehas-dispatch.path systemd/tehas-dispatch.service systemd/tehas-dispatch.timer \
    systemd/tehas-preview.service \
    systemd/tehas-preview.timer systemd/tehas-preview-sweep.service systemd/tehas-preview-sweep.timer systemd/tehas-protolab.service systemd/tehas-protolab.timer systemd/tehas-top.service sandbox/Dockerfile sandbox/base.Dockerfile sandbox/build.sh \
    sandbox/npm/playwright/package.json sandbox/npm/playwright/package-lock.json \
    npm/pi/package.json npm/pi/package-lock.json npm/ctx7/package.json npm/ctx7/package-lock.json \
    npm/parallel-web-cli/package.json npm/parallel-web-cli/package-lock.json versions.env \
    edge/docs.Caddyfile edge/docs.compose.yaml edge/router.Caddyfile \
    edge/router.compose.yaml labels.tsv
}

# A checkout of its own, so a case can change the source.
checkout() {
  part=$T/part
  rm -rf "$part"
  mkdir -p "$part"
  cp -R "$fixture/install.sh" "$fixture/lib" "$fixture/host" "$fixture/bin" \
    "$fixture/libexec" "$part/"
  t_console_source "$part" "$fixture"
}
# A home installed from such a checkout, which owes no build.
installed_checkout() { fresh; instance; checkout; t_run "$part/install.sh"; }

# install_list_report: the names of a home that the files= list above does not
# name, and the names it promises that the home lacks, one line each, with the
# direction in each line. Each side is a newline separated set of names in any
# order; a side that names nothing is refused, as a mistake of the caller.
install_list_report() { # <names the list promises> <names the home holds>
  local expected installed one
  if [ "$#" != 2 ]; then
    printf 'install_list_report wants two arguments, got %s\n' "$#" >&2
    return 2
  fi
  expected=$1
  installed=$2
  # A name that is nothing but blank is refused too.
  if [ -z "${expected//[$' \t\r\n']}" ] || [ -z "${installed//[$' \t\r\n']}" ]; then
    echo 'install_list_report: a side names no file at all' >&2
    return 2
  fi
  # comm's complaint about order would be lost, read as an empty report.
  expected=$(printf '%s\n' "$expected" | LC_ALL=C sort)
  installed=$(printf '%s\n' "$installed" | LC_ALL=C sort)
  if ! LC_ALL=C sort -c <<<"$expected" ||
    ! LC_ALL=C sort -c <<<"$installed"; then
    echo 'install_list_report: a side does not sort' >&2
    return 2
  fi
  expected=$expected$'\n'
  installed=$installed$'\n'
  while read -r one; do
    printf 'new installed file %s: add it to the files= list of install.sh and of test/install_lib.sh, or stop shipping it\n' "$one"
  done < <(comm -13 <(printf '%s' "$expected") <(printf '%s' "$installed"))
  while read -r one; do
    printf 'missing installed file %s: install.sh does not copy it into the home\n' "$one"
  done < <(comm -23 <(printf '%s' "$expected") <(printf '%s' "$installed"))
}
