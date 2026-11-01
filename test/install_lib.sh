# shellcheck shell=bash
# What the tests of install.sh share: the scratch host, the stubs for Docker,
# systemctl and cargo, an instance that is valid, and the files a fresh
# install puts. Source this from an install_*_test.sh file.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck disable=SC2034  # the variables here are read by the test files
# The part has sourced test/lib.sh before this.
# The one place the follow-ups are named: install.sh prints what it defines here,
# and tehas update runs the same ids, so a case can pin both sides to it.
# shellcheck source=lib/update-steps.sh
. "$T_ROOT/lib/update-steps.sh"

install="$T_ROOT/install.sh"
home=$T/home
th=$home/tehas
# bootstrap.sh dev writes two files outside any home, and install.sh reads them to
# know whether this host lets other accounts start a session. Pointed at scratch
# in every run here, so no test reads or writes the real /etc or /usr/local/bin.
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
T_ENV_ALWAYS+=(TEHAS_CONSOLE="$T_CONSOLE")

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
# The host facts a program reads back after an install, as one JSON object: the
# same answer `tehas api --json` gives, from the files install.sh put there.
# Read in a clean environment, as the factory's user would run it.
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
./.config/systemd/user/tehas-energy.service
./.config/systemd/user/tehas-improve.service
./.config/systemd/user/tehas-improve.timer
./.config/systemd/user/tehas-mcp.service
./.config/systemd/user/tehas-preview-sweep.service
./.config/systemd/user/tehas-preview-sweep.timer
./.config/systemd/user/tehas-preview.service
./.config/systemd/user/tehas-preview.timer
./.config/systemd/user/tehas-protolab.service
./.config/systemd/user/tehas-protolab.timer
./.config/systemd/user/tehas-refresh.service
./.config/systemd/user/tehas-refresh.timer
./.config/systemd/user/tehas-review.service
./.config/systemd/user/tehas-review.timer
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
./tehas/lib/analytics.sh
./tehas/lib/attachments.sh
./tehas/lib/config.sh
./tehas/lib/cycles.sh
./tehas/lib/disk.sh
./tehas/lib/millwright.sh
./tehas/lib/github-app.sh
./tehas/lib/headless.sh
./tehas/lib/history-lines.jq
./tehas/lib/history-listing.jq
./tehas/lib/history-repeats.jq
./tehas/lib/improve-prompt.md
./tehas/lib/improve.sh
./tehas/lib/issue-brief.md
./tehas/lib/labels.sh
./tehas/lib/labels.tsv
./tehas/lib/park-fields.jq
./tehas/lib/limit.sh
./tehas/lib/models.sh
./tehas/lib/paths.sh
./tehas/lib/questions.sh
./tehas/lib/refine-brief.md
./tehas/lib/release.sh
./tehas/lib/rukas.sh
./tehas/lib/sandbox.sh
./tehas/lib/sizing.sh
./tehas/lib/status.sh
./tehas/lib/store.sh
./tehas/lib/slice.sh
./tehas/lib/update-steps.sh
./tehas/libexec/analytics
./tehas/libexec/analytics.stamp
./tehas/libexec/attach
./tehas/libexec/attach.stamp
./tehas/libexec/check-product
./tehas/libexec/clean
./tehas/libexec/console
./tehas/libexec/console.stamp
./tehas/libexec/cycle
./tehas/libexec/dev
./tehas/libexec/dev-login
./tehas/libexec/dispatch
./tehas/libexec/export
./tehas/libexec/github-token
./tehas/libexec/history
./tehas/libexec/improve
./tehas/libexec/index
./tehas/libexec/init-product
./tehas/libexec/plan
./tehas/libexec/preview
./tehas/libexec/protolab
./tehas/libexec/refresh
./tehas/libexec/review
./tehas/libexec/setup
./tehas/libexec/status
./tehas/libexec/store
./tehas/libexec/store.stamp
./tehas/libexec/tehas-cycle
./tehas/libexec/tehas-cycle.stamp
./tehas/libexec/tehas-dispatch
./tehas/libexec/tehas-dispatch.stamp
./tehas/libexec/tehas-mcp
./tehas/libexec/tehas-mcp.stamp
./tehas/libexec/update
./tehas/libexec/with-settings
./tehas/libexec/work
./tehas/release/minisign.pub
./tehas/sandbox/Dockerfile
./tehas/sandbox/build.sh
./tehas/sandbox/gh-shim
./tehas/sandbox/npm/playwright/package-lock.json
./tehas/sandbox/npm/playwright/package.json
./tehas/shim/gh
./tehas/versions.env"
# The manual and the front page are whole directories; leave them out of the
# exact comparison and check them separately. So is state/, which is what the host
# keeps for itself - the record of follow-ups owed among it - not a file install.sh
# puts.
# The marker that says the home is tehas's: the host's own, like state/, so it
# is left out of the exact comparison and checked by its own cases. Nothing
# removes it, so a host installed before the marker keeps it once a run gave it
# one.
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
  tehas_steps_for systemd/tehas-cycle@.service systemd/tehas-clean.service systemd/tehas-clean.timer \
    systemd/tehas-dispatch.path systemd/tehas-dispatch.service systemd/tehas-dispatch.timer \
    systemd/tehas-energy.service systemd/tehas-improve.service systemd/tehas-improve.timer \
    systemd/tehas-preview.service \
    systemd/tehas-preview.timer systemd/tehas-preview-sweep.service systemd/tehas-preview-sweep.timer systemd/tehas-protolab.service systemd/tehas-protolab.timer systemd/tehas-refresh.service systemd/tehas-refresh.timer systemd/tehas-review.service systemd/tehas-review.timer systemd/tehas-top.service sandbox/Dockerfile sandbox/build.sh \
    sandbox/npm/playwright/package.json sandbox/npm/playwright/package-lock.json versions.env \
    edge/docs.Caddyfile edge/docs.compose.yaml edge/router.Caddyfile \
    edge/router.compose.yaml labels.tsv
}
