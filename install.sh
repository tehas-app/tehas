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
# Two things it does besides copying, on a real run only. The product
# checkout's origin/HEAD is refreshed, so the commands know the default branch
# without asking the network. And the code index a cycle's agents search is
# built with tehas-index, once: when there is no index yet and both the
# sandbox image and the product checkout are there. Until then the step is
# printed as a follow-up. A failed indexing does not fail the install.
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

# --- 1. Instance settings ---------------------------------------------------
# Every finding is one line. Any finding stops a real run before it changes
# anything.
instance_findings() {
  local dir file models product_model mode owner group
  tehas_load 2>&1 || return 0
  dir=$TEHAS_INSTANCE_DIR
  (tehas_require TEHAS_REPO TEHAS_CONTROL_ISSUE TEHAS_ALLOWED_LABELERS \
    PI_ENSEMBLE_SUBAGENT_MODEL PI_ENSEMBLE_SUBAGENT_PROVIDER 2>&1) || return 0
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
for f in tehas-console.ts tehas-dispatch tehas-energy tehas-health tehas-index tehas-protolab tehas-status tehas-work; do
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
  mkdir -p "$(dirname "$to")"
  [ -f "$to" ] && cp -p "$to" "$to.prev"
  install -m "$mode" "$from" "$to"
  echo "installed: $to"
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
mkdir -p "$TEHAS_HOME/logs" "$TEHAS_HOME/state" "$TEHAS_HOME/tmp" "$TEHAS_HOME/live" "$TEHAS_HOME/protolab"

# The one place the default branch is looked up over the network.
if [ -z "${TEHAS_BRANCH:-}" ] && [ -d "$TEHAS_WORK_REPO/.git" ]; then
  git -C "$TEHAS_WORK_REPO" remote set-head origin -a >/dev/null 2>&1 ||
    echo "WARN: could not read the default branch of $TEHAS_WORK_REPO from its remote; set TEHAS_BRANCH if the commands ask for it"
fi

# The code index, once. Called last, so its line follows the image build's.
first_index() {
  [ -f "$TEHAS_HOME/state/index.json" ] && return 0
  if [ -d "$TEHAS_WORK_REPO/.git" ] && docker image inspect "$PI_ENSEMBLE_IMAGE" >/dev/null 2>&1; then
    "$TEHAS_HOME/bin/tehas-index" && return 0
    echo "WARN: the code index was not built"
  fi
  echo "next: $TEHAS_HOME/bin/tehas-index   (needs the sandbox image and the product checkout)"
}

if [ "${#changed[@]}" = 0 ]; then
  echo "tehas is up to date"
  first_index
  exit 0
fi
list=" ${changed[*]} "
case "$list" in *" systemd/"*)
  systemctl --user daemon-reload || true
  echo "next: systemctl --user enable --now tehas-dispatch.timer tehas-energy.service tehas-protolab.timer" ;;
esac
case "$list" in *" sandbox/"* | *" versions.env "*) echo "next: $TEHAS_HOME/sandbox/build.sh" ;; esac
# The compose file takes the bind address from the environment env.sh sets.
case "$list" in *" edge/docs."*) echo "next: . $TEHAS_HOME/env.sh && docker compose -f $TEHAS_HOME/edge/docs.compose.yaml up -d" ;; esac
case "$list" in *" edge/protolab."*) echo "next: . $TEHAS_HOME/env.sh && docker compose -f $TEHAS_HOME/edge/protolab.compose.yaml up -d --force-recreate" ;; esac
first_index
exit 0
