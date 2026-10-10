#!/usr/bin/env bash
# Install tehas on this host from this checkout, for the instance whose
# settings are in the instance directory (see lib/config.sh and
# instances/example). Run as the tehas user:
#
#   ./install.sh            copy changed files into place
#   ./install.sh --check    change nothing; report settings, dependencies,
#                           installed files and tests, each on its own line
#
# What is installed: the commands and units (issue to pull request), the
# sandbox files, the docs server, the edge router, the Protolab and the manual,
# plus the instance's own models.json and, if it has one, settings.json.
#
# It copies files, and compiles one: the console of a running cycle is a Rust
# program in console/. A real run builds it with cargo in this checkout
# (console/target, which git ignores) and installs it as libexec/console.
# Beside it, console.stamp holds two checksums: of the source it was built
# from and of the program itself. It is built again only when the source has
# changed or the program is not the one that was installed, and the first
# build fetches its crates from the network. --check compares the checksums
# and builds nothing. A console that does not build stops a real run before
# it changes anything.
#
# It does not start the dispatcher or the docs server, rebuild the sandbox image or touch secrets (~/.config/tehas/env); the
# follow-up steps are printed when their inputs changed. A replaced file is
# kept next to the new one as <name>.prev.
#
# It also removes the commands of an earlier layout, one file for each command
# in ~/tehas/bin and the console script in ~/tehas/libexec, unless a cycle is
# running that still uses them.
#
# Three things it does besides copying, on a real run only. The product
# checkout's origin/HEAD is refreshed, so the commands know the default branch
# without asking the network. The code index a cycle's agents search is built
# with `tehas index`, once: when there is no index yet and both the sandbox
# image and the product checkout are there. A failed indexing does not fail the
# install. And each follow-up a copied file calls for is recorded in
# $TEHAS_HOME/state/update-pending before that file is put. This script's
# report of what changed is one-shot: the next run compares again and finds the
# file matching. So a run that dies partway would otherwise leave the files it
# did copy with no follow-up owed.
#
# The follow-ups are named by id (units, sandbox, docs, edge) in
# lib/update-steps.sh, which is the one place that names them. This script
# prints them as "next: <step>" and records them; `tehas update` runs them and
# clears the record. Only a step is printed as "next:". Something for a person
# to do later that is no step is printed as "later:".
#
# It asks systemd for nothing, with one exception. A copied unit file is not in
# effect until systemd has reloaded it, and that reload belongs to the step
# that restarts the units, so no command can report a changed unit as applied
# when it was not. The exception is the earlier layout: its commands are
# removed only after systemd has reloaded, because the units it still holds
# name them.
#
# Exit: 0 done or nothing to report, 1 --check found something or a step
# failed, 78 the instance settings are invalid (nothing was changed).
set -euo pipefail

root=$(cd "$(dirname "$0")" && pwd)
host=$root/host
check=0
[ "${1:-}" = "--check" ] && check=1

# shellcheck source=lib/config.sh
. "$root/lib/config.sh"
# shellcheck source=lib/update-steps.sh
. "$root/lib/update-steps.sh"

# --- 1. Instance settings ---------------------------------------------------
# Every finding is one line. Any finding stops a real run before it changes
# anything.
instance_findings() {
  local dir file models product_model mode owner group
  tehas_load 2>&1 || return 0
  dir=$TEHAS_INSTANCE_DIR
  (tehas_require_instance_settings 2>&1) || return 0
  models=$dir/models.json
  if [ ! -r "$models" ]; then
    echo "tehas: no models.json in $dir"
  elif ! jq -e . "$models" >/dev/null 2>&1; then
    echo "tehas: $models is not valid JSON"
  else
    jq -e --arg p "$PI_ENSEMBLE_SUBAGENT_PROVIDER" --arg m "$PI_ENSEMBLE_SUBAGENT_MODEL" \
      '.providers[$p].models // [] | any(.id == $m)' "$models" >/dev/null ||
      echo "tehas: models.json does not list model $PI_ENSEMBLE_SUBAGENT_MODEL under provider $PI_ENSEMBLE_SUBAGENT_PROVIDER"
    # The session's model is named by the product, not the instance.
    if [ -r "$TEHAS_WORK_REPO/.pi/settings.json" ]; then
      product_model=$(jq -r '"\(.defaultProvider // "")/\(.defaultModel // "")"' "$TEHAS_WORK_REPO/.pi/settings.json" 2>/dev/null || echo "/")
      if [ "$product_model" != "/" ]; then
        jq -e --arg p "${product_model%%/*}" --arg m "${product_model#*/}" \
          '.providers[$p].models // [] | any(.id == $m)' "$models" >/dev/null ||
          echo "tehas: models.json does not list the product's session model $product_model (from $TEHAS_WORK_REPO/.pi/settings.json)"
      fi
    fi
  fi
  # The settings are shell that every command runs: nobody else may write them.
  # A group-writable file is accepted only for the user's own primary group.
  for file in "$dir" "$dir/instance.env"; do
    # -L: a symlink is judged by the file it points at.
    mode=$(stat -L -c %a "$file")
    owner=$(stat -L -c %u "$file")
    group=$(stat -L -c %g "$file")
    if [ "$owner" != "$(id -u)" ]; then
      echo "tehas: $file is not owned by $(id -un)"
    elif [ $((8#$mode & 2)) != 0 ]; then
      echo "tehas: $file is writable by everyone (chmod o-w)"
    elif [ $((8#$mode & 16)) != 0 ] && [ "$group" != "$(id -g)" ]; then
      echo "tehas: $file is writable by another group (chmod g-w)"
    fi
  done
}

findings=$(instance_findings)
if [ -n "$findings" ]; then
  if [ "$check" = 1 ]; then
    echo "Instance:     $(tehas_instance_dir) INVALID"
    printf '%s\n' "$findings" | sed 's/^/  /'
    echo "Dependencies: not checked"
    echo "Installed:    not checked"
    echo "Tests:        not checked"
  else
    printf '%s\n' "$findings" >&2
    echo "tehas: nothing was installed" >&2
  fi
  exit 78
fi
tehas_load
instance=$TEHAS_INSTANCE_DIR

# --- 2. What belongs to this instance ---------------------------------------
# One line per file: source|destination|mode. A source starting with / is taken
# as is; anything else is under host/, except bin/, libexec/ and lib/ at the top.
agent=$HOME/.pi/agent
units=$HOME/.config/systemd/user

files=(
  "env.sh|$TEHAS_HOME/env.sh|644"
  "lib/config.sh|$TEHAS_HOME/lib/config.sh|644"
  "lib/update-steps.sh|$TEHAS_HOME/lib/update-steps.sh|644"
  "lib/edge.sh|$TEHAS_HOME/lib/edge.sh|644"
  "lib/sandbox.sh|$TEHAS_HOME/lib/sandbox.sh|644"
  "versions.env|$TEHAS_HOME/versions.env|644"
  "sandbox/Dockerfile|$TEHAS_HOME/sandbox/Dockerfile|644"
  "sandbox/build.sh|$TEHAS_HOME/sandbox/build.sh|755"
  "edge/docs.Caddyfile|$TEHAS_HOME/edge/docs.Caddyfile|644"
  "edge/docs.compose.yaml|$TEHAS_HOME/edge/docs.compose.yaml|644"
  "edge/router.Caddyfile|$TEHAS_HOME/edge/router.Caddyfile|644"
  "edge/router.compose.yaml|$TEHAS_HOME/edge/router.compose.yaml|644"
  "systemd/tehas-dispatch.service|$units/tehas-dispatch.service|644"
  "systemd/tehas-dispatch.timer|$units/tehas-dispatch.timer|644"
  "systemd/tehas-energy.service|$units/tehas-energy.service|644"
  "systemd/tehas-protolab.service|$units/tehas-protolab.service|644"
  "systemd/tehas-protolab.timer|$units/tehas-protolab.timer|644"
  "systemd/tehas-preview.service|$units/tehas-preview.service|644"
  "systemd/tehas-preview.timer|$units/tehas-preview.timer|644"
  "systemd/tehas-preview-sweep.service|$units/tehas-preview-sweep.service|644"
  "systemd/tehas-preview-sweep.timer|$units/tehas-preview-sweep.timer|644"
  "pi/mcp.json|$agent/mcp.json|644"
  "$instance/models.json|$agent/models.json|644"
)
# The two commands on PATH, and the scripts behind them.
for f in tehas tehasbot; do
  files+=("bin/$f|$TEHAS_HOME/bin/$f|755")
done
for f in dev dispatch energy health index labels preview protolab status summary update work; do
  files+=("libexec/$f|$TEHAS_HOME/libexec/$f|755")
done
# Pi's own settings: the instance may bring its own (default provider and
# model for interactive use on the host), otherwise the generic ones.
if [ -r "$instance/settings.json" ]; then
  files+=("$instance/settings.json|$agent/settings.json|644")
else
  files+=("pi/settings.json|$agent/settings.json|644")
fi

changed=()
report=()
note() { if [ "$check" = 1 ]; then report+=("$1"); else echo "$1"; fi; }

source_of() {
  case "$1" in
    /*) printf '%s\n' "$1" ;;
    bin/* | libexec/* | lib/*) printf '%s\n' "$root/$1" ;;
    *) printf '%s\n' "$host/$1" ;;
  esac
}

# put <entry>: install one file if it differs.
put() {
  local src to mode from
  IFS='|' read -r src to mode <<<"$1"
  from=$(source_of "$src")
  [ -f "$from" ] || { echo "tehas: $from is missing from this checkout" >&2; exit 1; }
  if [ -f "$to" ] && cmp -s "$from" "$to"; then return 0; fi
  changed+=("$src")
  if [ "$check" = 1 ]; then
    if [ -f "$to" ]; then note "differs: $to"; else note "missing: $to"; fi
    return 0
  fi
  # Owed before it is copied, not after: a copy that fails halfway leaves behind
  # a record of what the copies so far asked for. tehas update finishes the
  # record and clears it; install.sh --check reports it while any of it is left.
  record_step "$src"
  mkdir -p "$(dirname "$to")"
  [ -f "$to" ] && cp -p "$to" "$to.prev"
  install -m "$mode" "$from" "$to"
  echo "installed: $to"
}

# The step this source calls for, added to the record of steps owed. One call per
# copied file, so the record is never behind what the host has already been given.
record_step() { # one source path from the file table
  local step steps=()
  while IFS= read -r step; do steps+=("$step"); done < <(tehas_steps_for "$1")
  if [ "${#steps[@]}" != 0 ]; then tehas_record_steps "${steps[@]}" || exit 1; fi
}

# --- 3. Install ---------------------------------------------------------------
# The console first: a build that fails must leave the host as it was. What is
# installed is judged without building: by the checksum of every file the
# build reads, and by the checksum of the program that was installed.
console_to=$TEHAS_HOME/libexec/console
[ -d "$root/console" ] || { echo "tehas: $root/console is missing from this checkout" >&2; exit 1; }
console_source=$(cd "$root/console" && find . -type f ! -path './target/*' ! -path './tests/*' -print0 |
  LC_ALL=C sort -z | xargs -0 sha256sum | sha256sum | cut -d' ' -f1) ||
  { echo "tehas: could not read the console's source in $root/console" >&2; exit 1; }
console_sum() { sha256sum <"$1" | cut -d' ' -f1; }
if [ ! -x "$console_to" ] || [ "$(cat "$console_to.stamp" 2>/dev/null)" != "$console_source $(console_sum "$console_to")" ]; then
  if [ "$check" = 1 ]; then
    changed+=("libexec/console")
    if [ -f "$console_to" ]; then note "differs: $console_to"; else note "missing: $console_to"; fi
  else
    # The target directory is named, so the program is where this looks for
    # it whatever cargo's own configuration says, and the old one is removed
    # first, so what is installed is what this build made. The build scripts
    # of the crates run here: they are not given the bot's token.
    target=${CARGO_TARGET_DIR:-$root/console/target}
    built=$target/release/console
    rm -f "$built"
    if ! out=$(env -u GH_TOKEN cargo build --release --locked --quiet \
      --manifest-path "$root/console/Cargo.toml" --target-dir "$target" 2>&1) || [ ! -f "$built" ]; then
      printf '%s\n' "$out" | tail -n 20 >&2
      echo "tehas: the console did not build (cargo build --release --locked in $root/console); nothing was installed" >&2
      exit 1
    fi
    put "$built|$console_to|755"
    echo "$console_source $(console_sum "$console_to")" >"$console_to.stamp"
  fi
fi

for e in "${files[@]}"; do put "$e"; done

# The operating manual and the front page are directories; pages come and go.
sync_dir() { # name: host/<name>/* -> $TEHAS_HOME/<name>/
  local name=$1 from to
  for from in "$host/$name"/*; do
    put "$name/$(basename "$from")|$TEHAS_HOME/$name/$(basename "$from")|644"
  done
  [ -d "$TEHAS_HOME/$name" ] || return 0
  for to in "$TEHAS_HOME/$name"/*; do
    [ -e "$to" ] || continue
    case "$to" in *.prev) continue ;; esac
    [ -e "$host/$name/$(basename "$to")" ] && continue
    changed+=("$name/$(basename "$to")")
    if [ "$check" = 1 ]; then note "stale: $to"; else rm -f "$to" "$to.prev" && echo "removed: $to"; fi
  done
}
sync_dir docs
sync_dir fun

# The commands of before bin/tehas, one file each in bin/, and the console of
# before it was a compiled program. And the Protolab's server files of before the
# edge router. Found here, removed in step 5 once systemd has the new units.
old=()
for f in bin/tehas-console.ts bin/tehas-dispatch bin/tehas-energy bin/tehas-health bin/tehas-index \
  bin/tehas-protolab bin/tehas-status bin/tehas-test bin/tehas-work libexec/console.ts \
  edge/protolab.Caddyfile edge/protolab.compose.yaml; do
  to=$TEHAS_HOME/$f
  [ -e "$to" ] || continue
  old+=("$to")
  changed+=("$f")
  # A running sampler is the old file calling itself; only a restart moves it
  # on, and the units step is what restarts it.
  if [ "$check" = 1 ]; then note "stale: $to"; else record_step "$f"; fi
done

# --- 4. Report (--check) -----------------------------------------------------
if [ "$check" = 1 ]; then
  rc=0
  echo "Instance:     $instance OK"

  if [ -n "${T_IN_TEST:-}" ]; then
    echo "Dependencies: not checked (running inside the tests)"
  elif [ -x "$root/bootstrap.sh" ]; then
    if out=$("$root/bootstrap.sh" check 2>&1); then
      echo "Dependencies: OK"
    else
      echo "Dependencies: NOT OK"
      printf '%s\n' "$out" | sed 's/^/  /'
      rc=1
    fi
  else
    echo "Dependencies: not checked (no bootstrap.sh in this checkout)"
  fi

  if [ "${#changed[@]}" = 0 ]; then
    echo "Installed:    OK"
  else
    echo "Installed:    DRIFT (${#changed[@]} files)"
    printf '%s\n' "${report[@]}" | sed 's/^/  /'
    rc=1
  fi

  # A follow-up an earlier tehas update was asked for and never finished is
  # still owed. Files matching is not enough to call a host current. A record
  # this account cannot open is not an answer of "nothing owed" either: it holds
  # steps, and this cannot say how many.
  owed=$(tehas_pending_file)
  if ! owed_lines=$(tehas_read_steps); then
    echo "Follow-ups:   NOT DONE (the record $owed cannot be read)"
    rc=1
  elif [ -n "$owed_lines" ]; then
    echo "Follow-ups:   NOT DONE ($(printf '%s\n' "$owed_lines" | wc -l) from an earlier run, see $owed)"
    rc=1
  fi

  if [ -n "${T_IN_TEST:-}" ]; then
    echo "Tests:        not checked (running inside the tests)"
  elif out=$("$root/scripts/test.sh" 2>&1); then
    echo "Tests:        OK"
  else
    echo "Tests:        FAILED"
    printf '%s\n' "$out" | tail -n 30 | sed 's/^/  /'
    rc=1
  fi
  exit "$rc"
fi

# --- 5. After a real run ------------------------------------------------------
# The servers mount live/ and protolab/: they have to be there, and ours.
mkdir -p "$TEHAS_HOME/logs" "$TEHAS_HOME/state" "$TEHAS_HOME/tmp" "$TEHAS_HOME/live" "$TEHAS_HOME/protolab" \
  "$TEHAS_HOME/preview"

# The previews' network: internal, so a preview reaches nothing. The router's
# compose file names it as external and `tehasbot preview` puts containers on
# it, so it is made here, whichever of the two starts first. A host with no
# Docker yet gets it from `tehasbot preview`.
if command -v docker >/dev/null 2>&1; then
  docker network inspect tehas-previews >/dev/null 2>&1 ||
    docker network create --internal tehas-previews >/dev/null 2>&1 ||
    note "The network tehas-previews could not be made; the edge router will not start without it."
fi

# The one place the default branch is looked up over the network.
if [ -z "${TEHAS_BRANCH:-}" ] && [ -d "$TEHAS_WORK_REPO/.git" ]; then
  git -C "$TEHAS_WORK_REPO" remote set-head origin -a >/dev/null 2>&1 ||
    echo "WARN: could not read the default branch of $TEHAS_WORK_REPO from its remote; set TEHAS_BRANCH if the commands ask for it"
fi

# The code index, once. Called last, so its line follows the image build's.
first_index() {
  [ -f "$TEHAS_HOME/state/index.json" ] && return 0
  if [ -d "$TEHAS_WORK_REPO/.git" ] && docker image inspect "$PI_ENSEMBLE_IMAGE" >/dev/null 2>&1; then
    "$TEHAS_HOME/bin/tehas" index && return 0
    echo "WARN: the code index was not built"
  fi
  echo "later: $TEHAS_HOME/bin/tehas index   (needs the sandbox image and the product checkout)"
}

if [ "${#changed[@]}" = 0 ]; then
  echo "tehas is up to date"
  first_index
  exit 0
fi
# The follow-ups, by the ids lib/update-steps.sh defines: one line per step these
# changed files call for, in the order they run. They are already in the record of
# steps owed, recorded file by file above, and `tehas update` is what runs them:
# it reloads the unit files before restarting any unit, and a step that cannot be
# performed leaves the run red and the step owed. A person who ran install.sh by
# hand reads the same lines, and the record stays until `tehas update` clears it.
while IFS= read -r step; do echo "next: $step"; done < <(tehas_steps_for "${changed[@]}")

# The old commands go while this holds the dispatcher's lock. A cycle that
# began before this install was started by the old dispatcher, which calls
# them by name until it ends: it holds the lock, so they stay. A poll that
# starts meanwhile finds the lock taken and ends before it has run anything.
# The units systemd holds name them too, so systemd reloads first, and only
# silence next to exit 0 says it did. Either way the files are left and a
# later run of this script removes them: they are still there to be found, so
# nothing is recorded. A file that cannot be removed fails the install.
retire_later=""
if [ "${#old[@]}" -gt 0 ]; then
  if ! out=$(systemctl --user daemon-reload 2>&1) || [ -n "$out" ]; then
    retire_later="once systemctl --user daemon-reload works   (systemd still holds the units that name the old commands and server files in $TEHAS_HOME, so they were left)"
  else
    exec 9>>"$TEHAS_HOME/state/dispatch.lock"
    if flock -n 9; then
      for to in "${old[@]}"; do
        rm -f "$to" "$to.prev"
        echo "removed: $to"
      done
    else
      retire_later="when the running cycle has ended   (the dispatcher's lock is held, so the old commands and server files in $TEHAS_HOME were left)"
    fi
    exec 9>&-
  fi
fi
[ -z "$retire_later" ] || echo "later: ./install.sh again $retire_later"
first_index
exit 0
