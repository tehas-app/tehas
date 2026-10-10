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
# sandbox files, the docs server, the Protolab and the manual, plus the
# instance's own models.json and, if it has one, settings.json.
#
# It copies files. It does not start the dispatcher or the docs server, rebuild the sandbox image or touch secrets (~/.config/tehas/env); the
# follow-up steps are printed when their inputs changed. A replaced file is
# kept next to the new one as <name>.prev.
#
# Two things it does besides copying, on a real run only: the product checkout's
# origin/HEAD is refreshed, so the commands know the default branch without
# asking the network; and each follow-up a copied file calls for is recorded in
# $TEHAS_HOME/state/update-pending before that file is put, because this script's
# report of what changed is one-shot - the next run compares again and finds the
# file matching - so a run that dies partway would otherwise leave the files it
# did copy with no follow-up owed.
#
# The follow-ups are named by id (units, sandbox, docs, protolab) in
# lib/update-steps.sh, which is the one place that names them: this script prints
# and records them, tehas-update runs them and clears the record. It asks systemd
# for nothing itself - a copied unit file is not in effect until systemd has
# reloaded it, and that reload belongs to the step that restarts the units, so no
# command can report a changed unit as applied when it was not.
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
# as is; anything else is under host/, except bin/ and lib/ at the top.
agent=$HOME/.pi/agent
units=$HOME/.config/systemd/user

files=(
  "env.sh|$TEHAS_HOME/env.sh|644"
  "lib/config.sh|$TEHAS_HOME/lib/config.sh|644"
  "lib/update-steps.sh|$TEHAS_HOME/lib/update-steps.sh|644"
  "versions.env|$TEHAS_HOME/versions.env|644"
  "sandbox/Dockerfile|$TEHAS_HOME/sandbox/Dockerfile|644"
  "sandbox/build.sh|$TEHAS_HOME/sandbox/build.sh|755"
  "edge/docs.Caddyfile|$TEHAS_HOME/edge/docs.Caddyfile|644"
  "edge/docs.compose.yaml|$TEHAS_HOME/edge/docs.compose.yaml|644"
  "edge/protolab.Caddyfile|$TEHAS_HOME/edge/protolab.Caddyfile|644"
  "edge/protolab.compose.yaml|$TEHAS_HOME/edge/protolab.compose.yaml|644"
  "systemd/tehas-dispatch.service|$units/tehas-dispatch.service|644"
  "systemd/tehas-dispatch.timer|$units/tehas-dispatch.timer|644"
  "systemd/tehas-energy.service|$units/tehas-energy.service|644"
  "systemd/tehas-protolab.service|$units/tehas-protolab.service|644"
  "systemd/tehas-protolab.timer|$units/tehas-protolab.timer|644"
  "pi/mcp.json|$agent/mcp.json|644"
  "$instance/models.json|$agent/models.json|644"
)
for f in tehas-console.ts tehas-dispatch tehas-energy tehas-health tehas-protolab tehas-status tehas-update tehas-work tehas-test; do
  files+=("bin/$f|$TEHAS_HOME/bin/$f|755")
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
    bin/* | lib/*) printf '%s\n' "$root/$1" ;;
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
  # a record of what the copies so far asked for. tehas-update finishes the
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

  # A follow-up an earlier tehas-update was asked for and never finished is
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
  elif out=$("$root/bin/tehas-test" 2>&1); then
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
mkdir -p "$TEHAS_HOME/logs" "$TEHAS_HOME/state" "$TEHAS_HOME/tmp" "$TEHAS_HOME/live" "$TEHAS_HOME/protolab"

# The one place the default branch is looked up over the network.
if [ -z "${TEHAS_BRANCH:-}" ] && [ -d "$TEHAS_WORK_REPO/.git" ]; then
  git -C "$TEHAS_WORK_REPO" remote set-head origin -a >/dev/null 2>&1 ||
    echo "WARN: could not read the default branch of $TEHAS_WORK_REPO from its remote; set TEHAS_BRANCH if the commands ask for it"
fi

if [ "${#changed[@]}" = 0 ]; then
  echo "tehas is up to date"
  exit 0
fi
# The follow-ups, by the ids lib/update-steps.sh defines: one line per step these
# changed files call for, in the order they run. They are already in the record of
# steps owed - recorded file by file above - and tehas-update is what runs them:
# it reloads the unit files before restarting any unit, and a step that cannot be
# performed leaves the run red and the step owed. A person who ran install.sh by
# hand reads the same lines, and the record stays until tehas-update clears it.
while IFS= read -r step; do echo "next: $step"; done < <(tehas_steps_for "${changed[@]}")
exit 0
