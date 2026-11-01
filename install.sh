#!/usr/bin/env bash
# Install tehas on this host from this checkout, for the instance whose
# settings are in the instance directory (see lib/config.sh and
# instances/example). Run as the tehas user:
#
#   ./install.sh            copy changed files into place
#   ./install.sh --check    change nothing; report settings, dependencies,
#                           the slice, installed files (content and mode) and
#                           the command the other accounts type when root's
#                           copy is stale,
#                           each on its own line
#   ./install.sh --check --tests
#                           the same, and run the shell tests too. They take
#                           most of a check's time, so tehas update --check,
#                           which has a time limit, leaves them out
#
# What is installed: the commands and units (issue to pull request), the
# sandbox files, the docs server, the edge router, the Protolab, the
# improvement loop and the manual, plus Pi's model list and settings, made from
# the instance's models.json and settings.json (see section 2).
#
# It copies files, and compiles seven: the console of a running cycle, the
# analytics program, the MCP server, the state store, the attachment checker,
# the dispatcher and the cycle's shadow, Rust programs in the Cargo
# workspace console/ (the other six in console/analytics, console/mcp,
# console/store, console/attach, console/dispatch and console/cycle). A real
# run builds each with cargo in this checkout (console/target, which git
# ignores) and installs them as libexec/console, libexec/analytics,
# libexec/tehas-mcp, libexec/store, libexec/attach, libexec/tehas-dispatch and
# libexec/tehas-cycle; the MCP server is
# built static (musl), because it runs in a container that has none of the
# host's libraries. Beside each, <name>.stamp holds two checksums: of the
# source it was built from and of the program itself. A program is built
# again only when its source has changed or it is not the one that was
# installed, and the build that fetches the crates from the network is the
# first one on a host that has never fetched. The console's source leaves out
# console/analytics, console/mcp, console/store, console/attach,
# console/dispatch and console/cycle, so a change there builds that program
# alone. All but the attachment checker use the library in console/core, so a
# change there builds those six. --check
# compares the checksums and builds nothing. A
# program that does not build stops a real run before it changes anything.
#
# A real run then gives the state store its database, state/tehas.db, under
# the update lock: `store init` when there is none, `store migrate --backup
# state/backup` otherwise, which copies the database into state/backup/ before
# it changes it. state/ is made and kept 0700. The store is told the version
# and commit of this tree, the instance and the repository: the commit is
# git's for a checkout and the COMMIT file's for a tree with no git, which is
# what a release carries. A tree with neither is refused before anything is
# written, and a store that fails stops the run with exit 1 and its own line.
#
# The records an older tehas kept as files under state/ are then imported
# (`store import`): made rows, each file moved to state/imported-<YYYYMMDD>/,
# nothing deleted. A real run holds state/dispatch.lock from before the first
# copy until after the import, so no poll starts anything in between; a poll
# that holds it longer than the update lock's wait stops the run (exit 1,
# nothing installed). Before the first copy the store judges the old files
# (`store import --dry-run`): while one is there, a running cycle or planning
# turn, or a file the import would hold, stops the run the same way. Each
# state/plan-<n>.json.keep moves to $TEHAS_WORK_DIR-plan-<n>.keep.
#
# It does not start the dispatcher or the docs server, rebuild the sandbox
# image or write secrets. It reads the model endpoints' tokens from
# ~/.config/tehas/env, to check the ones models.json names; the
# follow-up steps are printed when their inputs changed. A replaced file is
# kept next to the new one as <name>.prev. Each file is copied to <name>.new
# and renamed over the installed one.
#
# It also removes the commands of an earlier layout, one file for each command
# in ~/tehas/bin and the console script in ~/tehas/libexec, once systemd has
# reloaded the units that name them.
#
# Four things it does besides copying, on a real run only. The product
# checkout's origin/HEAD is refreshed, so the commands know the default branch
# without asking the network. The code index a cycle's agents search is built
# with `tehas index`, once: when there is no index yet and both the sandbox
# image and the product checkout are there. A failed indexing does not fail the
# install. The Docker network of the previews, tehas-previews, is made where
# Docker is installed, is 27.1 or later, and the network is not there. One
# that is there and does not keep the host out is left, and the run says that
# it has to be remade. And each follow-up a copied file
# calls for is recorded in $TEHAS_HOME/state/update-pending before that file is
# put. This script's report of what changed is one-shot: the next run compares
# again and finds the file matching. So a run that dies partway would otherwise
# leave the files it did copy with no follow-up owed.
#
# The follow-ups are named by id (units, sandbox, docs, edge) in
# lib/update-steps.sh, which is the one place that names them. This script
# prints them as "next: <step>" and records them; `tehas update` runs them and
# clears the record. Only a step is printed as "next:". Something for a person
# to do later that is no step is printed as "later:".
#
# It does not install the slice the sandbox containers are to run in: that is
# one system unit, and root's (sudo ./bootstrap.sh slice). It compares the
# file in this checkout, host/slice/tehas.slice, with the one under /etc and
# says what it found: a "Slice:" line in --check, a "note:" in a real run when
# the file is missing or differs. Every container runs in the slice, so a
# missing or differing file makes --check exit 1; a real run still installs.
#
# One file of a host is not this script's to install: /usr/local/bin/tehas, the
# command an account that is not the tehas user types, put there by root by
# `bootstrap.sh dev` from host/login/tehas. This script asks for nothing and
# installs nothing as root, so a difference between that file and this
# checkout's is reported, on a line that names the command to run. It is looked
# for only where that command left its marker (/etc/tehas/manager), through the
# same two paths it writes to (BOOTSTRAP_ETC, BOOTSTRAP_BIN), so the pair cannot
# end up looking in different places. A host that never ran it reports nothing.
#
# It asks systemd for nothing, with two exceptions. A copied unit file is not in
# effect until systemd has reloaded it, and that reload belongs to the step
# that restarts the units, so no command can report a changed unit as applied
# when it was not. The first exception is the earlier layout: its commands are
# removed only after systemd has reloaded, because the units it still holds
# name them. The second is a first install, into a home with no env.sh: it
# makes the labels (tehas labels) and enables the dispatch timer and path, and
# the optional units whose setting is on. Until both are done
# state/first-install says they are owed, --check reports it, and the next run
# tries again.
#
# A real run also puts the line that reads env.sh at the top of ~/.bashrc when
# the file does not read it already, and --check says whether it does.
#
# A real run holds $TEHAS_HOME/state/update.lock, the lock tehas update holds,
# and stops when another run has it.
#
# Before its first write it also asks whether $TEHAS_HOME is tehas's at all, by
# the marker .tehas-home it leaves there. A home that exists, is not empty and
# holds no marker is refused, and so is a path that is not absolute or that
# holds `..`: the loader takes TEHAS_HOME from the environment as it is, and
# this script removes what it takes for stale under it, so a wrong path would
# lose files that were never tehas's. A home that already has env.sh and
# libexec/dispatch is taken as one installed before the marker, and gets it on
# this run.
#
# Exit: 0 done or nothing to report, 1 --check found something, a step failed
# or another update is running, 78 the instance settings are invalid or
# TEHAS_HOME is not provably tehas's (nothing was changed).
set -euo pipefail

root=$(cd "$(dirname "$0")" && pwd)
host=$root/host
check=0
tests=0
[ "${1:-}" = "--check" ] && check=1
[ "$check" = 1 ] && [ "${2:-}" = "--tests" ] && tests=1
# The tests give a stand-in for the suite: a check inside the tests never runs
# the real one, which would run these tests again. Read before tehas_load,
# which drops it from the environment.
suite=${TEHAS_TEST_SUITE:-}
# Read before tehas_load, which drops it from the environment.
update_fd=${TEHAS_UPDATE_LOCK_FD:-}
# Read before tehas_load, which drops every TEHAS_* it does not know: the one
# rule for it is tehas_update_lock_wait, taken with lib/update-steps.sh below,
# where tehas update takes it too. Set here so the file is defined before that
# call reads it.
lock_wait=
# The model endpoints' tokens, from the file env.sh reads them from. The shell
# that runs this may not have them: a first install has no env.sh yet, and an
# update runs under the loader of the version before, which may drop them.
# The file is read as env.sh reads it, in a subshell, and only the
# TEHAS_MODEL_KEY_ variables are taken from it.
if [ -r "$HOME/.config/tehas/env" ]; then
  while IFS= read -r -d '' kv; do
    export "${kv?}"
  done < <(
    set -a
    # shellcheck disable=SC1091
    . "$HOME/.config/tehas/env" >/dev/null 2>&1
    for v in ${!TEHAS_MODEL_KEY_@}; do printf '%s=%s\0' "$v" "${!v}"; done
  )
fi

# shellcheck source=lib/config.sh
. "$root/lib/config.sh"
# shellcheck source=lib/update-steps.sh
. "$root/lib/update-steps.sh"
# The waits for the update lock and the dispatcher's, read before tehas_load
# below drops them.
lock_wait=$(tehas_update_lock_wait)
dispatch_wait=$(tehas_dispatch_lock_wait)
# shellcheck source=lib/edge.sh
. "$root/lib/edge.sh"
# shellcheck source=lib/cycles.sh
. "$root/lib/cycles.sh"
# shellcheck source=lib/models.sh
. "$root/lib/models.sh"
# A partial checkout is named as the missing file it is, as every other
# checkout file this script needs is named: sourcing it under set -e would stop
# the run with a bare bash error and a status this header does not document.
[ -f "$root/lib/paths.sh" ] || { echo "tehas: $root/lib/paths.sh is missing from this checkout" >&2; exit 1; }
# shellcheck source=lib/paths.sh
. "$root/lib/paths.sh"

# --- 1. Instance settings ---------------------------------------------------
# Every finding is one line. Any finding stops a real run before it changes
# anything.
instance_findings() {
  local dir file models product_model mode owner group provider model
  # The settings are shell that every command runs: nobody else may write them.
  # A group-writable file is accepted only for the user's own primary group.
  # Checked before tehas_load, which sources instance.env: a file another
  # account can write is refused, not run.
  dir=$(tehas_instance_dir)
  # Only a file that exists has permissions to check: a missing directory or a
  # missing instance.env is tehas_load's to name. A file that is there is
  # judged here, before anything is sourced. The instance's own
  # sandbox.Dockerfile is among them: its bytes are built into the image every
  # cycle runs, so they are as trusted as the platform's Dockerfile and only
  # the tehas user may write them.
  for file in "$dir" "$dir/instance.env" "$dir/sandbox.Dockerfile"; do
    [ -e "$file" ] || [ -L "$file" ] || continue
    # -L: a symlink is judged by the file it points at.
    mode=$(stat -L -c %a "$file" 2>/dev/null) || mode=0
    owner=$(stat -L -c %u "$file" 2>/dev/null) || owner=-1
    group=$(stat -L -c %g "$file" 2>/dev/null) || group=-1
    if [ "$owner" != "$(id -u)" ]; then
      echo "tehas: $file is not owned by $(id -un)"
      return 0
    elif [ $((8#$mode & 2)) != 0 ]; then
      echo "tehas: $file is writable by everyone (chmod o-w)"
      return 0
    elif [ $((8#$mode & 16)) != 0 ] && [ "$group" != "$(id -g)" ]; then
      echo "tehas: $file is writable by another group (chmod g-w)"
      return 0
    fi
  done
  tehas_load 2>&1 || return 0
  (tehas_require_instance_settings 2>&1) || return 0
  models=$dir/models.json
  if [ ! -r "$models" ]; then
    echo "tehas: no models.json in $dir"
  elif ! jq -e . "$models" >/dev/null 2>&1; then
    echo "tehas: $models is not valid JSON"
  else
    while read -r provider model; do
      jq -e --arg p "$provider" --arg m "$model" \
        '.providers[$p].models // [] | any(.id == $m)' "$models" >/dev/null ||
        echo "tehas: models.json does not list model $model under provider $provider"
    done < <(tehas_models)
    # A token is named, never written: models.json is in a repository.
    tehas_model_keys_ok "$models" 2>&1
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

# --- 1a. The tool PATH ------------------------------------------------------
# A run that is not a login shell has no env.sh yet - this script is what
# installs it - so the tool directories bootstrap.sh put in the home are named
# here, from the one list env.sh and bootstrap.sh use. Applied once, after the
# settings have loaded and before the build and the copies below, so every step
# that resolves a tool sees what a login shell would.
tehas_tool_paths

# --- 1b. The home is tehas's -------------------------------------------------
# The loader takes TEHAS_HOME from the environment as it is (AGENTS.md rule 3
# keeps loading free of side effects), and this script removes what it takes for
# stale under it: every file of $TEHAS_HOME/docs and $TEHAS_HOME/fun this
# checkout does not have, and the commands of an earlier layout. So a home that
# is not tehas's is refused before the lock, the build and the first copy, and
# --check says the same thing.
#
# The proof is the marker .tehas-home, which a run leaves there and nothing
# removes: an existing host has one as soon as it has been installed once. A run
# that makes the home, or finds the marker, goes on and leaves the marker there.
# A home with no marker is known another way, by what an install of tehas leaves
# and nothing else puts beside it: env.sh with libexec/dispatch, or the manual
# with the front page. Otherwise it is tehas's only when everything under it is
# one of the directories tehas makes - no docs/ or fun/ of its own, which is what
# this run would empty - or versions.env. Anything else is refused, as is a path
# that is not absolute or that names a parent.
home_marker=.tehas-home
# The top-level directories of a home tehas makes: the ones install.sh puts
# there, the ones bootstrap.sh user makes (tools/, repos/ and tmp/, the tools
# under tools/node and tools/npm/<name>), and .cargo, which bootstrap.sh leaves
# for cargo but does not make, and run, where each cycle keeps its copy of the
# code (tehas-cycle run). docs and fun are not among them: a home that has
# them without the marker is not known to be tehas's, and this run would remove
# the pages of someone else from them.
home_dirs=(bin libexec lib live logs mcp platform preview protolab repos sandbox
  state tmp tools work target .cargo versions current run)

home_refusal() { # the reason $TEHAS_HOME is not provably tehas's, or nothing
  local path=${TEHAS_HOME:-} entry part rest
  [ -n "$path" ] || { echo "TEHAS_HOME is not set"; return; }
  case "$path" in
    /*) ;;
    *) echo "TEHAS_HOME is not an absolute path: $path"; return ;;
  esac
  # A component, not a substring: a directory of someone else may be named
  # a..b, and only a component of `..` moves the path out of where it says it is.
  rest=${path#/}
  while [ -n "$rest" ]; do
    part=${rest%%/*}
    [ "$rest" = "$part" ] && rest= || rest=${rest#*/}
    [ "$part" != ".." ] || { echo "TEHAS_HOME names a parent directory: $path"; return; }
  done
  [ -d "$path" ] || return
  [ -e "$path/$home_marker" ] && return
  # An existing host has no marker yet, so the home of an earlier install has to
  # be known another way: env.sh beside libexec/dispatch, and the manual and the
  # front page beside the rest, are what an install of tehas leaves and nothing
  # else puts there.
  if [ -f "$path/env.sh" ] && [ -f "$path/libexec/dispatch" ]; then return; fi
  if [ -f "$path/docs/host.md" ] && [ -f "$path/fun/index.html" ]; then return; fi
  while IFS= read -r entry; do
    [ -n "$entry" ] || continue
    case " ${home_dirs[*]} " in *" $entry "*) continue ;; esac
    case "$entry" in versions.env) continue ;; esac
    echo "TEHAS_HOME is not empty and holds no $home_marker: $path"
    return
  done < <(ls -A "$path" 2>/dev/null)
}

if refusal=$(home_refusal) && [ -n "$refusal" ]; then
  if [ "$check" = 1 ]; then
    # The report's first line, before any of the others: nothing beyond it is
    # looked at, because a home this script cannot trust makes every answer
    # about it meaningless. `tehas update --check` reads this line by its prefix.
    echo "Home:         REFUSED"
    echo "  tehas: $refusal"
    echo "Dependencies: not checked"
    echo "Installed:    not checked"
    echo "Tests:        not checked"
    exit 1
  fi
  echo "tehas: $refusal" >&2
  echo "tehas: nothing was installed" >&2
  exit 78
fi

# --- 1c. The commit the store records -----------------------------------------
# The store's database records which commit of tehas made it: git's for a
# checkout, the COMMIT file's for a tree with no git of its own, which is what
# a release carries. Asked before the lock and the first write, so a tree with
# neither is refused while nothing has been written.
tree_commit() {
  if [ -z "$(git -C "$root" rev-parse --show-prefix 2>/dev/null || echo none)" ]; then
    git -C "$root" rev-parse HEAD 2>/dev/null
    return
  fi
  [ ! -f "$root/COMMIT" ] || head -n 1 "$root/COMMIT"
}
store_commit=$(tree_commit) || store_commit=""
store_refusal=""
if [[ ! $store_commit =~ ^[0-9a-f]{40}$ ]]; then
  store_refusal="the commit of $root is not known (no git commit, no COMMIT file with 40 hex digits), and the store records it"
  if [ "$check" != 1 ]; then
    echo "tehas: $store_refusal; nothing was installed" >&2
    exit 1
  fi
fi

# One update at a time. Two runs side by side would build the image twice and
# each rewrite the record of steps owed with its own view. tehas update holds the
# lock while it runs this script and names the descriptor it holds it on; any
# other run takes the lock itself. --check changes nothing and takes no lock.
#
# The lock is waited for, not refused at once: a run that stopped partway can
# leave it held a moment by a child that outlived it. Both branches wait, the
# handed-over descriptor included, so the two commands ask the same question of
# the same lock.
if [ "$check" != 1 ]; then
  update_lock=$TEHAS_HOME/state/update.lock
  # The state directory the lock is in is made here: a home with nothing in it
  # yet has none, and a run that has not been given the lock by tehas update
  # opens the file itself. It is the store's directory too, so it is 0700 from
  # here on: with WAL on, a readable state/ is a readable copy of the database.
  if ! mkdir -p "$TEHAS_HOME/state" || ! chmod 700 "$TEHAS_HOME/state"; then
    echo "tehas: could not make $TEHAS_HOME/state mode 0700; nothing was installed" >&2
    exit 1
  fi
  if [[ $update_fd =~ ^[0-9]+$ ]] && [ "/dev/fd/$update_fd" -ef "$update_lock" ]; then
    flock -w "$lock_wait" "$update_fd" || { echo "tehas: another update is running ($update_lock), still held after ${lock_wait}s; nothing was installed" >&2; exit 1; }
  else
    exec 7>>"$update_lock"
    flock -w "$lock_wait" 7 || { echo "tehas: another update is running ($update_lock), still held after ${lock_wait}s; nothing was installed" >&2; exit 1; }
  fi
fi

# --- 2. What belongs to this instance ---------------------------------------
# One line per file: source|destination|mode. A source starting with / is taken
# as is; anything else is under host/, except bin/, libexec/ and lib/ at the top.
agent=$HOME/.pi/agent
units=$HOME/.config/systemd/user

# Pi's settings and model list are made from the instance, not copied as they
# are. The session model is named once, by PI_ENSEMBLE_SUBAGENT_PROVIDER and
# PI_ENSEMBLE_SUBAGENT_MODEL; a settings.json that names its own keeps it. A
# model entry needs only its id; the keys Pi also wants get today's defaults,
# and a key the instance gives is kept. A product's .pi/settings.json, when it
# has one, still wins: Pi reads it over these.
made=$(mktemp -d) || { echo "tehas: could not make a scratch directory" >&2; exit 1; }
trap 'rm -rf "$made"' EXIT
pi_settings=$host/pi/settings.json
[ -r "$instance/settings.json" ] && pi_settings=$instance/settings.json
jq --arg p "$PI_ENSEMBLE_SUBAGENT_PROVIDER" --arg m "$PI_ENSEMBLE_SUBAGENT_MODEL" '
  .defaultProvider //= $p | .defaultModel //= $m
' "$pi_settings" >"$made/settings.json" ||
  { echo "tehas: $pi_settings is not valid JSON; nothing was installed" >&2; exit 78; }
jq '
  {supportsDeveloperRole: false, supportsReasoningEffort: false} as $compat |
  .providers[] |= (
    .compat //= $compat |
    .models[]? |= (
      .name //= .id | .input //= ["text"] | .reasoning //= false |
      .cost //= {input: 0, output: 0, cacheRead: 0, cacheWrite: 0}
    )
  )
' "$instance/models.json" >"$made/models.json" ||
  { echo "tehas: $instance/models.json is not valid JSON; nothing was installed" >&2; exit 78; }

files=(
  "env.sh|$TEHAS_HOME/env.sh|644"
  # gh as the GitHub App, first on PATH when the instance names one (env.sh).
  "shim/gh|$TEHAS_HOME/shim/gh|755"
  "lib/config.sh|$TEHAS_HOME/lib/config.sh|644"
  "lib/paths.sh|$TEHAS_HOME/lib/paths.sh|644"
  "lib/analytics.sh|$TEHAS_HOME/lib/analytics.sh|644"
  "lib/attachments.sh|$TEHAS_HOME/lib/attachments.sh|644"
  "lib/history-lines.jq|$TEHAS_HOME/lib/history-lines.jq|644"
  "lib/history-listing.jq|$TEHAS_HOME/lib/history-listing.jq|644"
  "lib/history-repeats.jq|$TEHAS_HOME/lib/history-repeats.jq|644"
  "lib/park-fields.jq|$TEHAS_HOME/lib/park-fields.jq|644"
  "lib/cycles.sh|$TEHAS_HOME/lib/cycles.sh|644"
  "lib/labels.sh|$TEHAS_HOME/lib/labels.sh|644"
  "lib/improve.sh|$TEHAS_HOME/lib/improve.sh|644"
  "lib/improve-prompt.md|$TEHAS_HOME/lib/improve-prompt.md|644"
  "lib/refine-brief.md|$TEHAS_HOME/lib/refine-brief.md|644"
  "lib/issue-brief.md|$TEHAS_HOME/lib/issue-brief.md|644"
  "lib/millwright.sh|$TEHAS_HOME/lib/millwright.sh|644"
  "lib/live.sh|$TEHAS_HOME/lib/live.sh|644"
  "lib/limit.sh|$TEHAS_HOME/lib/limit.sh|644"
  "lib/models.sh|$TEHAS_HOME/lib/models.sh|644"
  "lib/update-steps.sh|$TEHAS_HOME/lib/update-steps.sh|644"
  "lib/edge.sh|$TEHAS_HOME/lib/edge.sh|644"
  "lib/headless.sh|$TEHAS_HOME/lib/headless.sh|644"
  "lib/github-app.sh|$TEHAS_HOME/lib/github-app.sh|644"
  "lib/rukas.sh|$TEHAS_HOME/lib/rukas.sh|644"
  "lib/sandbox.sh|$TEHAS_HOME/lib/sandbox.sh|644"
  "lib/slice.sh|$TEHAS_HOME/lib/slice.sh|644"
  # The free disk of the host, read by the dispatcher's hold and the previews'.
  "lib/disk.sh|$TEHAS_HOME/lib/disk.sh|644"
  # The measure of an issue body, read by the dispatcher and the analytics.
  "lib/sizing.sh|$TEHAS_HOME/lib/sizing.sh|644"
  "lib/questions.sh|$TEHAS_HOME/lib/questions.sh|644"
  "lib/status.sh|$TEHAS_HOME/lib/status.sh|644"
  # How the commands run the state store, libexec/store.
  "lib/store.sh|$TEHAS_HOME/lib/store.sh|644"
  "lib/planning.sh|$TEHAS_HOME/lib/planning.sh|644"
  "lib/refresh.sh|$TEHAS_HOME/lib/refresh.sh|644"
  # The key a release signature is checked with. It is here so the copy an
  # adopter verifies against is the one the repository holds and not one that
  # came inside the download it is checking.
  "release/minisign.pub|$TEHAS_HOME/release/minisign.pub|644"
  # What a release packs, so the list is on a host as it is in a checkout.
  "lib/release.sh|$TEHAS_HOME/lib/release.sh|644"
  "lib/update-release.sh|$TEHAS_HOME/lib/update-release.sh|644"
  "versions.env|$TEHAS_HOME/versions.env|644"
  "sandbox/Dockerfile|$TEHAS_HOME/sandbox/Dockerfile|644"
  "sandbox/build.sh|$TEHAS_HOME/sandbox/build.sh|755"
  "sandbox/gh-shim|$TEHAS_HOME/sandbox/gh-shim|755"
  "sandbox/npm/playwright/package.json|$TEHAS_HOME/sandbox/npm/playwright/package.json|644"
  "sandbox/npm/playwright/package-lock.json|$TEHAS_HOME/sandbox/npm/playwright/package-lock.json|644"
  "edge/docs.Caddyfile|$TEHAS_HOME/edge/docs.Caddyfile|644"
  "edge/docs.compose.yaml|$TEHAS_HOME/edge/docs.compose.yaml|644"
  "edge/router.Caddyfile|$TEHAS_HOME/edge/router.Caddyfile|644"
  "edge/router.compose.yaml|$TEHAS_HOME/edge/router.compose.yaml|644"
  "systemd/tehas-cycle@.service|$units/tehas-cycle@.service|644"
  "systemd/tehas-clean.service|$units/tehas-clean.service|644"
  "systemd/tehas-clean.timer|$units/tehas-clean.timer|644"
  "systemd/tehas-dispatch.service|$units/tehas-dispatch.service|644"
  "systemd/tehas-dispatch.path|$units/tehas-dispatch.path|644"
  "systemd/tehas-dispatch.timer|$units/tehas-dispatch.timer|644"
  "systemd/tehas-energy.service|$units/tehas-energy.service|644"
  "systemd/tehas-improve.service|$units/tehas-improve.service|644"
  "systemd/tehas-improve.timer|$units/tehas-improve.timer|644"
  "systemd/tehas-mcp.service|$units/tehas-mcp.service|644"
  "systemd/tehas-protolab.service|$units/tehas-protolab.service|644"
  "systemd/tehas-protolab.timer|$units/tehas-protolab.timer|644"
  "systemd/tehas-preview.service|$units/tehas-preview.service|644"
  "systemd/tehas-preview.timer|$units/tehas-preview.timer|644"
  "systemd/tehas-preview-sweep.service|$units/tehas-preview-sweep.service|644"
  "systemd/tehas-preview-sweep.timer|$units/tehas-preview-sweep.timer|644"
  "systemd/tehas-refresh.service|$units/tehas-refresh.service|644"
  "systemd/tehas-refresh.timer|$units/tehas-refresh.timer|644"
  "systemd/tehas-review.service|$units/tehas-review.service|644"
  "systemd/tehas-review.timer|$units/tehas-review.timer|644"
  "systemd/tehas-top.service|$units/tehas-top.service|644"
  "pi/mcp.json|$agent/mcp.json|644"
  "$made/models.json|$agent/models.json|644"
  "$made/settings.json|$agent/settings.json|644"
)
# The two commands on PATH, and the scripts behind them.
for f in tehas tehasbot; do
  files+=("bin/$f|$TEHAS_HOME/bin/$f|755")
done
for f in \
  api \
  check-product \
  clean \
  cycle \
  dev \
  dev-login \
  dispatch \
  drain \
  export \
  github-token \
  history \
  improve \
  index \
  init-product \
  labels \
  mcp \
  plan \
  preview \
  protolab \
  refresh \
  review \
  resume \
  setup \
  status \
  stop \
  tidy \
  update \
  with-settings \
  work; do
  files+=("libexec/$f|$TEHAS_HOME/libexec/$f|755")
done
# The polyfills of pi-rukas features (scripts/polyfills.sh) are a glob, not a
# list: which ones the tree carries changes when upstream releases one, and a
# name in a list is one more thing an install can be missing.
# tehas-cycle caps mode reads a polyfill in the lib/ beside its own libexec/,
# so a host that never got the file answers
# `off` for a feature this checkout answers `polyfill` for. nullglob: a tree
# with none adds nothing.
shopt -s nullglob
for f in "$root"/lib/polyfill-*.sh; do
  files+=("lib/${f##*/}|$TEHAS_HOME/lib/${f##*/}|644")
done
shopt -u nullglob

# What the instance's own sandbox file was at the last install, or nothing when
# the host has never had one, or has one no install has seen yet. Read before the
# record of follow-ups below, so a host that changed the file is not called
# current and its build step is owed before any copy is made. A file that cannot
# be read is said and stops the run: recording it as seen would owe the build
# once and then never again for a file nobody has read.
sandbox_changed=0
sandbox_drift=0
sandboxunread=""
sandbox_at_file=$TEHAS_HOME/state/sandbox.Dockerfile.sha256
if [ -f "$instance/sandbox.Dockerfile" ]; then
  if ! hash=$(sha256sum "$instance/sandbox.Dockerfile" 2>/dev/null); then
    sandboxunread="$instance/sandbox.Dockerfile cannot be read (check its permissions)"
  else
    [ "${hash:0:64}" != "$(cat "$sandbox_at_file" 2>/dev/null)" ] && sandbox_changed=1
  fi
fi

changed=()
report=()
note() { if [ "$check" = 1 ]; then report+=("$1"); else echo "$1"; fi; }

# source_of <src>: the file of this checkout that a table entry names, in from.
source_of() {
  case "$1" in
    /*) from=$1 ;;
    bin/* | libexec/* | lib/*) from=$root/$1 ;;
    *) from=$host/$1 ;;
  esac
}

# put <entry>: install one file if its content or its mode differs.
put() {
  local src to mode from on_disk
  IFS='|' read -r src to mode <<<"$1"
  source_of "$src"
  [ -f "$from" ] || { echo "tehas: $from is missing from this checkout" >&2; exit 1; }
  if [ -f "$to" ] && cmp -s "$from" "$to"; then
    # Content alone is not enough: a command that matches but is not executable,
    # or a unit file that has picked up a wider mode, is not what this checkout
    # installs. -L: a symlink is judged by the file it points at.
    # A mode the list cannot name stops here, before it reaches chmod.
    [[ $mode =~ ^[0-7]{3,4}$ ]] || { echo "tehas: the mode $mode for $src is not a number" >&2; exit 1; }
    on_disk=$(stat -L -c %a "$to") || { echo "tehas: could not read $to" >&2; exit 1; }
    # 0644 and 644 are the same mode written two ways.
    on_disk=$((8#$on_disk))
    if [ "$on_disk" = "$((8#$mode))" ]; then return 0; fi
    changed+=("$src")
    if [ "$check" = 1 ]; then
      note "mode: $to is $(printf '%o' "$on_disk"), wants $mode"
      return 0
    fi
    # The mode is owed, not the copy: chmod on the file in place, so no .new,
    # no rename and no .prev beside a file whose content is already this one's.
    chmod "$mode" "$to" || { echo "tehas: could not set mode $mode on $to" >&2; exit 1; }
    echo "mode: $to -> $mode"
    return 0
  fi
  changed+=("$src")
  if [ "$check" = 1 ]; then
    if [ -f "$to" ]; then note "differs: $to"; else note "missing: $to"; fi
    return 0
  fi
  # Owed before it is copied, not after: a copy that fails halfway leaves behind
  # a record of what the copies so far asked for. tehas update finishes the
  # record and clears it; install.sh --check reports it while any of it is left.
  record_step "$src"
  [ -d "${to%/*}" ] || mkdir -p "${to%/*}"
  [ -f "$to" ] && cp -p "$to" "$to.prev"
  # Copied beside it and renamed over it, so a cycle that starts meanwhile finds
  # the old file or the new one, never a part of one.
  install -m "$mode" "$from" "$to.new" || { rm -f "$to.new"; echo "tehas: could not install $to" >&2; exit 1; }
  mv -f "$to.new" "$to" || { rm -f "$to.new"; echo "tehas: could not install $to" >&2; exit 1; }
  echo "installed: $to"
}

# write_file <path>: stdin into <path>, by the same rename.
write_file() {
  cat >"$1.new" || { rm -f "$1.new"; echo "tehas: could not write $1" >&2; exit 1; }
  mv -f "$1.new" "$1" || { rm -f "$1.new"; echo "tehas: could not write $1" >&2; exit 1; }
}

# The line that puts the tehas environment in every shell of the tehas user,
# ssh host '<command>' too. It has to come before the check many distributions
# have at the top of ~/.bashrc that returns early for a shell that is not
# interactive. A line that sources env.sh already, as the manual once had it
# written by hand, counts.
bashrc=$HOME/.bashrc
bashrc_line="[ -f $TEHAS_HOME/env.sh ] && . $TEHAS_HOME/env.sh"
bashrc_ok() {
  [ -f "$bashrc" ] || return 1
  grep -qF "$TEHAS_HOME/env.sh" "$bashrc" && return 0
  [ "$TEHAS_HOME" = "$HOME/tehas" ] && grep -q '^[^#]*~/tehas/env\.sh' "$bashrc"
}

# A first install owes the labels and the core units, recorded in this file
# until both are done, so a run that stops partway still owes them.
first_owed=$TEHAS_HOME/state/first-install

# Do the lines the sandbox image reads differ between this checkout and the copy
# the host holds? The build reads only the lines named SHELLCHECK_, JQ_,
# PLAYWRIGHT_, CBM_, PI_RUKAS_ or VIPUNE_ (host/sandbox/build.sh,
# host/sandbox/Dockerfile); the rest of versions.env is bootstrap.sh's, so a
# change to one of those asks for nothing but the copy. 0 when equal, 1 otherwise.
#
# The host's copy is read before the copies begin and the answer kept, so the
# record and the next: lines are made from one reading of the file the host held:
# that file is what the image now on the host was built from, so a pin moved on
# the host owes the build whether or not this run overwrites it, and a pin only
# the checkout moves owes it because the copy this run makes hands the host pins
# the image has never been built from. A host with no installed copy has never
# had a build made from these pins, which is a change too. Read once, before any
# copy: the reading after the copies would answer about the file this run writes
# and say nothing about the one it replaced.
versions_build_changed() {
  local host_file current kept line
  host_file=$TEHAS_HOME/versions.env
  [ -f "$host_file" ] || { echo 1; return 0; }
  current=$(grep -E '^(SHELLCHECK|JQ|PLAYWRIGHT|CBM|PI_RUKAS|VIPUNE)_' "$host/versions.env") || current=""
  # A file that names none of them cannot be compared with: the build reads what
  # it reads, so the build is owed rather than assumed already done.
  [ -n "$current" ] || { echo 1; return 0; }
  kept=$(grep -E '^(SHELLCHECK|JQ|PLAYWRIGHT|CBM|PI_RUKAS|VIPUNE)_' "$host_file") || kept=""
  # A copy that holds none of them is a difference and not an agreement, for the
  # same reason.
  [ -n "$kept" ] || { echo 1; return 0; }
  # One line among them is the change a person makes on a host: one pin moved,
  # the rest as some checkout wrote it. Then that line alone is compared, read
  # out of each file, so neither the other lines nor the order of the host's
  # copy can change the answer. A release's pins are more than one line - the
  # version and its checksum, a model with its revision and its hashes - so a
  # whole block still reads as a whole block, position and all.
  if [ "${kept//$'\n'}" = "$kept" ]; then
    line=$kept
    current=$(printf '%s\n' "$current" | grep -Fx "$line") || current=""
  fi
  [ "$kept" = "$current" ] && { echo 0; return 0; }
  echo 1
}
# Whether this run owes the sandbox build because of versions.env: 1 when the
# lines the image reads are not the ones the image the host holds was built
# from. Read once, before any copy is made, so the record and the next: lines
# below give one answer to one question.
versions_owed=0
if [ "$(versions_build_changed)" = 1 ]; then versions_owed=1; fi

# The step this source calls for, added to the record of steps owed. One call per
# copied file, so the record is never behind what the host has already been given.
record_step() { # one source path from the file table
  # versions.env calls for the build only for the lines the image reads; a change
  # to the host's own pins calls for nothing here. The file is still copied, and
  # still counted as drift for --check and for its later: line below.
  if [ "$1" = versions.env ] && [ "$versions_owed" != 1 ]; then return 0; fi
  tehas_steps_into "$1"
  if [ "${#tehas_steps[@]}" != 0 ]; then tehas_record_steps "${tehas_steps[@]}" || exit 1; fi
}

# --- 2b. The command root installed for the other accounts ------------------
# host/login/tehas reaches an account that is not the tehas user as
# /usr/local/bin/tehas, installed by root. Reading the marker bootstrap.sh dev
# leaves tells whether this host has that setup at all; the file beside it is
# compared with this checkout's copy. Nothing here writes to either.
login_bin=${BOOTSTRAP_BIN:-/usr/local/bin}/tehas
login_manager=${BOOTSTRAP_ETC:-/etc}/tehas/manager
login_fix=""
if [ -r "$login_manager" ]; then
  # The compare and the command it names are both files of this checkout. Said
  # as a missing checkout, as put() says it: cmp answers 2 for a source that is
  # not there, and that would read as a difference this host cannot fix.
  for need in host/login/tehas bootstrap.sh; do
    [ -f "$root/$need" ] || { echo "tehas: $root/$need is missing from this checkout" >&2; exit 1; }
  done
  IFS= read -r login_user <"$login_manager" || true
  # The name goes into a printed command line, so only the shape bootstrap.sh dev
  # accepts is used. Anything else is left for the person to fill in.
  [[ $login_user =~ ^[a-z_][a-z0-9_-]*$ ]] || login_user=""
  if [ ! -e "$login_bin" ] || ! cmp -s "$host/login/tehas" "$login_bin"; then
    login_fix="sudo $root/bootstrap.sh dev ${login_user:-<tehas user>}"
  fi
fi

# --- 3. Install ---------------------------------------------------------------
# The programs are built first and copied only after the old records are judged
# (3a): program_puts and program_stamps hold what is due. gate_store is the
# store this run installs, the one that judges them.
program_puts=()
program_stamps=()
gate_store=$TEHAS_HOME/libexec/store
# The compiled programs first, the console, analytics, the MCP server, the
# store and the attachment checker, the members of the Cargo workspace in
# console/: a build that fails must
# leave the host as it was, so every program that is due is built before any is
# installed. What is
# installed is judged without building: by the checksum of every file the
# program's build reads, and by the checksum of the program that was installed.
# A tree unpacked from a release has no console/ to build from: it carries the
# programs built, in its own libexec/, and they are installed as they are. Each
# one's stamp is then "release" and the program's checksum, so a later build
# from a checkout is not mistaken for this one.
if [ ! -d "$root/console" ] && [ -f "$root/VERSION" ]; then
  [ -x "$root/libexec/console" ] || { echo "tehas: this release has no libexec/console; nothing was installed" >&2; exit 1; }
  # The store is what the database is made and migrated with below, so a host
  # that would be left with none is refused before the first copy. --check
  # names it as a refusal, not drift, since no update can clear it.
  if [ ! -x "$root/libexec/store" ] && [ ! -x "$TEHAS_HOME/libexec/store" ]; then
    if [ "$check" != 1 ]; then
      echo "tehas: this release has no libexec/store and none is installed; nothing was installed" >&2
      exit 1
    fi
    store_refusal="${store_refusal:+$store_refusal; }this release has no libexec/store and none is installed"
  fi
  program_sum() { sha256sum <"$1" | cut -d' ' -f1; }
  [ ! -x "$root/libexec/store" ] || gate_store=$root/libexec/store
  for prog in console analytics tehas-mcp store attach tehas-dispatch tehas-cycle; do
    from=$root/libexec/$prog
    to=$TEHAS_HOME/libexec/$prog
    if [ ! -x "$from" ]; then
      [ "$prog" = store ] && [ ! -x "$to" ] && continue
      note "this release carries no libexec/$prog; the one installed stays"
      continue
    fi
    sum="release $(program_sum "$from")"
    [ ! -x "$to" ] || [ "$(cat "$to.stamp" 2>/dev/null)" != "$sum" ] || continue
    if [ "$check" = 1 ]; then
      changed+=("libexec/$prog")
      if [ -f "$to" ]; then note "differs: $to"; else note "missing: $to"; fi
    else
      program_puts+=("$from|$to|755")
      program_stamps+=("$sum")
    fi
  done
else
  [ -d "$root/console" ] || { echo "tehas: $root/console is missing from this checkout" >&2; exit 1; }
  # source_sum <find tests>: one checksum of the files under console/ that the
  # tests select, the build's own output left out.
  source_sum() {
    (cd "$root/console" && find . -type f ! -path './target/*' ! -path './analytics/target/*' "$@" -print0 |
      LC_ALL=C sort -z | xargs -0 sha256sum | sha256sum | cut -d' ' -f1)
  }
  # The console's source is all of console/ but its tests, analytics/, mcp/,
  # store/, attach/, dispatch/ and cycle/, so a change to one of those alone
  # does not build the console again.
  # analytics reads its own directory, the workspace's manifest and lock, and the
  # shared library in core/, and so do the MCP server and the store.
  console_source=$(source_sum ! -path './tests/*' ! -path './analytics/*' ! -path './mcp/*' ! -path './store/*' \
    ! -path './attach/*' ! -path './dispatch/*' ! -path './cycle/*') ||
    { echo "tehas: could not read the console's source in $root/console" >&2; exit 1; }
  analytics_source=$(source_sum \( \( -path './analytics/*' ! -path './analytics/tests/*' \) \
    -o -path ./Cargo.toml -o -path ./Cargo.lock -o -path './core/*' \)) ||
    { echo "tehas: could not read the analytics program's source in $root/console" >&2; exit 1; }
  mcp_source=$(source_sum \( \( -path './mcp/*' ! -path './mcp/tests/*' ! -path './mcp/target/*' \) \
    -o -path ./Cargo.toml -o -path ./Cargo.lock -o -path './core/*' \)) ||
    { echo "tehas: could not read the MCP server's source in $root/console" >&2; exit 1; }
  store_source=$(source_sum \( \( -path './store/*' ! -path './store/src/tests.rs' ! -path './store/target/*' \) \
    -o -path ./Cargo.toml -o -path ./Cargo.lock -o -path './core/*' \)) ||
    { echo "tehas: could not read the store's source in $root/console" >&2; exit 1; }
  # The attachment checker reads its own directory and the workspace's manifest
  # and lock.
  attach_source=$(source_sum \( \( -path './attach/*' ! -path './attach/src/tests.rs' ! -path './attach/target/*' \) \
    -o -path ./Cargo.toml -o -path ./Cargo.lock \)) ||
    { echo "tehas: could not read the attachment checker's source in $root/console" >&2; exit 1; }
  # The dispatcher's shadow and the cycle's each read their own directory,
  # the workspace's manifest and lock, and core/.
  dispatch_source=$(source_sum \( \( -path './dispatch/*' ! -path './dispatch/tests/*' ! -path './dispatch/target/*' \) \
    -o -path ./Cargo.toml -o -path ./Cargo.lock -o -path './core/*' \)) ||
    { echo "tehas: could not read the dispatcher's source in $root/console" >&2; exit 1; }
  cycle_source=$(source_sum \( \( -path './cycle/*' ! -path './cycle/tests/*' ! -path './cycle/target/*' \) \
    -o -path ./Cargo.toml -o -path ./Cargo.lock -o -path './core/*' \)) ||
    { echo "tehas: could not read the cycle's source in $root/console" >&2; exit 1; }
  program_sum() { sha256sum <"$1" | cut -d' ' -f1; }
  # program_label <name>: how a message names it.
  program_label() {
    case $1 in
      console) echo "the console" ;;
      tehas-mcp) echo "the MCP server" ;;
      store) echo "the store" ;;
      attach) echo "the attachment checker" ;;
      tehas-dispatch) echo "the dispatcher" ;;
      tehas-cycle) echo "the cycle's shadow" ;;
      *) echo "the $1 program" ;;
    esac
  }
  # program_triple <name>: the target a program is built for, empty for the host's.
  program_triple() { [ "$1" != tehas-mcp ] || echo x86_64-unknown-linux-musl; }
  # The build asks cargo for --offline first, so a host that has fetched once needs
  # no network, and asks again without it when cargo says a crate is missing from
  # its cache. cargo_miss_words is the one list of what cargo prints for that: a
  # line counts only when it opens with cargo's own `error:`, because a build
  # script can echo anything, so a line that merely quotes --offline sends no
  # second run on its own. t_stub_cargo's CARGO_OFFLINE answers in these words.
  cargo_miss_words=('no matching package named' 'attempting to make an HTTP request')
  # cargo_build <prog> <offline|online>: one build, cut off after build_timeout
  # seconds (124, as timeout(1) reports). It reads $root, $target, $triple and
  # $hidden from the build loop below, and leaves the build's words in $out, the
  # command it ran in $build_cmd and its exit status in $rc.
  cargo_build() {
    local args=(build --release --locked)
    case "$2" in
      offline) args+=(--offline) ;;
      online) ;;
      *)
        echo "tehas: cargo_build: no such mode: $2" >&2
        return 2
        ;;
    esac
    args+=(--quiet --manifest-path "$root/console/Cargo.toml" --target-dir "$target" ${triple:+--target "$triple"} -p "$1")
    build_cmd="cargo ${args[*]}"
    out=$(timeout "$build_timeout" env "${hidden[@]}" cargo "${args[@]}" 2>&1) && rc=0 || rc=$?
  }
  # cache_miss <cargo's words>: true when one line is cargo's own report that a
  # crate it needs is not in its cache.
  cache_miss() {
    local line word
    while IFS= read -r line; do
      case "$line" in
        error:*)
          for word in "${cargo_miss_words[@]}"; do
            case "$line" in *"$word"*) return 0 ;; esac
          done
          ;;
      esac
    done <<<"$1"
    return 1
  }
  # unsafe_tool_dir: a tool directory this run would take its compiler from that
  # is not the running user's own, or nothing. tehas_tool_paths puts those
  # directories first on PATH and the programs cargo builds here are installed in
  # libexec, where every cycle and the dispatcher run them. Only a directory that is there is judged, by the rule instance_findings
  # applies to the settings files.
  unsafe_tool_dir() {
    local dir mode owner group
    for dir in "$TEHAS_HOME/bin" "$TEHAS_HOME/tools/node/bin" "$NPM_CONFIG_PREFIX/bin" \
      "$BUN_INSTALL/bin" "$CARGO_HOME/bin"; do
      [ -d "$dir" ] || continue
      mode=$(stat -L -c %a "$dir" 2>/dev/null) || mode=0
      owner=$(stat -L -c %u "$dir" 2>/dev/null) || owner=-1
      group=$(stat -L -c %g "$dir" 2>/dev/null) || group=-1
      if [ "$owner" != "$(id -u)" ]; then
        echo "$dir is not owned by $(id -un)"
        return 0
      elif [ $((8#$mode & 2)) != 0 ]; then
        echo "$dir is writable by everyone (chmod o-w)"
        return 0
      elif [ $((8#$mode & 16)) != 0 ] && [ "$group" != "$(id -g)" ]; then
        echo "$dir is writable by another group (chmod g-w)"
        return 0
      fi
    done
    return 1
  }

  due=()
  for entry in "console $console_source" "analytics $analytics_source" "tehas-mcp $mcp_source" \
    "store $store_source" "attach $attach_source" "tehas-dispatch $dispatch_source" \
    "tehas-cycle $cycle_source"; do
    read -r prog sum <<<"$entry"
    to=$TEHAS_HOME/libexec/$prog
    [ ! -x "$to" ] || [ "$(cat "$to.stamp" 2>/dev/null)" != "$sum $(program_sum "$to")" ] || continue
    if [ "$check" = 1 ]; then
      changed+=("libexec/$prog")
      if [ -f "$to" ]; then note "differs: $to"; else note "missing: $to"; fi
    else
      due+=("$entry")
    fi
  done
  if [ "${#due[@]}" != 0 ]; then
    # Before a build, because this run is about to run a program found under one
    # of the tool directories and install what it makes.
    if tool_finding=$(unsafe_tool_dir) && [ -n "$tool_finding" ]; then
      echo "tehas: $tool_finding, so the tools there are not run; nothing was installed" >&2
      exit 1
    fi
    # The target directory is named, so a program is where this looks for it
    # whatever cargo's own configuration says, and the old one is removed first,
    # so what is installed is what this build made. The build scripts of the
    # crates run here: they are given no variable that is named like a secret,
    # the bot's token among them.
    target=${CARGO_TARGET_DIR:-$root/console/target}
    hidden=()
    while IFS= read -r name; do hidden+=(-u "$name"); done < <(compgen -e | grep -E 'TOKEN|SECRET|PASSWORD|API_KEY')
    # The bound is one build's own, so it names no tool of its own: without
    # timeout(1) every build would be reported as one that did not build.
    if ! command -v timeout >/dev/null; then
      echo "tehas: timeout is not installed, so $(program_label "${due[0]%% *}") cannot be built with a time limit (bootstrap.sh installs it); nothing was installed" >&2
      exit 1
    fi
    # tehas_load has checked it, from the instance or the environment.
    build_timeout=${TEHAS_BUILD_TIMEOUT:-1800}
    for entry in "${due[@]}"; do
      read -r prog _ <<<"$entry"
      triple=$(program_triple "$prog")
      built=$target/${triple:+$triple/}release/$prog
      rm -f "$built"
      cargo_build "$prog" offline
      offline_out=$out
      if [ "$rc" != 0 ] && cache_miss "$out"; then
        # The crate is not in the cache, so this one build goes online. The words
        # of each attempt are reported under its own heading: a build that fails
        # again is reported from both, so the miss is not lost behind the fetch
        # that followed it. The words of the attempt that ended the build are what
        # the message below names, since $rc and $build_cmd are its own.
        note "note: cargo's cache has no crate for $prog; building it again with the network"
        cargo_build "$prog" online
        build_report="offline build:
${offline_out:-cargo printed nothing}
online build:
${out:-cargo printed nothing}"
      else
        build_report=${out:-cargo printed nothing}
      fi
      if [ "$rc" != 0 ] || [ ! -f "$built" ]; then
        printf '%s\n' "$build_report" | tail -n 20 >&2
        if [ "$rc" = 127 ]; then
          echo "tehas: cargo is not installed, so $(program_label "$prog") cannot be built (bootstrap.sh installs it); nothing was installed" >&2
        elif [ "$rc" = 124 ]; then
          echo "tehas: $(program_label "$prog") did not build ($build_cmd in $root/console was cut off after ${build_timeout}s); nothing was installed" >&2
        else
          echo "tehas: $(program_label "$prog") did not build ($build_cmd in $root/console); nothing was installed" >&2
        fi
        exit 1
      fi
    done
    for entry in "${due[@]}"; do
      read -r prog sum <<<"$entry"
      to=$TEHAS_HOME/libexec/$prog
      triple=$(program_triple "$prog")
      [ "$prog" != store ] || gate_store=$target/${triple:+$triple/}release/store
      program_puts+=("$target/${triple:+$triple/}release/$prog|$to|755")
      program_stamps+=("build $sum")
    done
  fi
fi

# --- 3a. The old records and the dispatcher's lock ----------------------------
# The store keeps the host's records; an older tehas kept them as files under
# state/, and store_db below imports them (store import). From here until after
# that import this run holds state/dispatch.lock, so no poll starts a cycle or a
# planning turn in between. A poll that holds it now is waited for up to
# TEHAS_DISPATCH_LOCK_WAIT seconds (900), then nothing is installed.
#
# Before the first copy the store this run installs judges the old files with
# a dry run, which reads files only. A cycle or planning turn that runs while
# old files are there would write more of them after the import, so it stops
# the install; so does a file the import would hold (a link, too large, or
# unreadable). home_refusal has proved the home by now, so a home installed
# before the marker came in gets it here, and its old records are judged too.
dispatch_held=0
store_gate() {
  local out
  exec 9>>"$TEHAS_HOME/state/dispatch.lock"
  if ! flock -w "$dispatch_wait" 9; then
    echo "tehas: a poll is running ($TEHAS_HOME/state/dispatch.lock still held after ${dispatch_wait}s); nothing was installed" >&2
    exit 1
  fi
  dispatch_held=1
  if [ ! -e "$TEHAS_HOME/$home_marker" ] && [ ! -L "$TEHAS_HOME/$home_marker" ]; then
    printf 'tehas\n' | write_file "$TEHAS_HOME/$home_marker"
  fi
  [ -f "$TEHAS_HOME/$home_marker" ] && [ ! -L "$TEHAS_HOME/$home_marker" ] || return 0
  if [ ! -x "$gate_store" ]; then
    echo "tehas: there is no store to judge the old records with ($gate_store); nothing was installed" >&2
    exit 1
  fi
  out=$(TEHAS_HOME=$TEHAS_HOME "$gate_store" import --dry-run 2>&1) && return 0
  case $out in
    "store: live #"*)
      out=${out#store: live }
      echo "tehas: a cycle or planning turn runs (${out%%:*}); run tehas drain --wait and let it end, then install again; nothing was installed" >&2
      ;;
    "store: held "*)
      printf '%s\n' "$out" >&2
      echo "tehas: the old records listed above cannot be imported; move each one out of $TEHAS_HOME/state or mend it, then install again; nothing was installed" >&2
      ;;
    *)
      printf '%s\n' "$out" | tail -n 5 >&2
      echo "tehas: the store could not judge the old records under $TEHAS_HOME/state; nothing was installed" >&2
      ;;
  esac
  exit 1
}
# Given back before the first index, which takes the lock itself.
dispatch_release() {
  [ "$dispatch_held" != 1 ] || exec 9>&-
  dispatch_held=0
}
if [ "$check" != 1 ]; then
  store_gate
  for i in "${!program_puts[@]}"; do
    put "${program_puts[$i]}"
    to=${program_puts[$i]#*|}
    to=${to%|*}
    stamp=${program_stamps[$i]}
    case $stamp in "build "*) stamp="${stamp#build } $(program_sum "$to")" ;; esac
    echo "$stamp" | write_file "$to.stamp"
  done
fi

# A host has its tools from bootstrap.sh before its first install, so only a
# versions.env that replaces another one asks for it again.
had_versions=0
[ -f "$TEHAS_HOME/versions.env" ] && had_versions=1
# The home's own directories are made here, before the first copy: the servers
# mount live/ and protolab/, preview/ is where `tehasbot preview` keeps each pull
# request, and an empty home has none of them yet. Making them here rather than
# after the copies is what lets the marker go into a home that is not yet there.
# --check changes nothing, so it makes neither.
if [ "$check" != 1 ]; then
  mkdir -p "$TEHAS_HOME/logs" "$TEHAS_HOME/state" "$TEHAS_HOME/tmp" "$TEHAS_HOME/live" "$TEHAS_HOME/protolab" \
    "$TEHAS_HOME/preview"
  # A home with no env.sh has never been installed.
  [ -f "$TEHAS_HOME/env.sh" ] || : >"$first_owed"
  if ! bashrc_ok; then
    { echo "$bashrc_line"; [ ! -f "$bashrc" ] || cat "$bashrc"; } | write_file "$bashrc"
    echo "installed: the env.sh line at the top of $bashrc"
  fi
  # The proof that this home is tehas's, written once.
  if [ ! -f "$TEHAS_HOME/$home_marker" ]; then
    printf 'tehas\n' | write_file "$TEHAS_HOME/$home_marker"
  fi
fi
# The instance's own sandbox file is not copied anywhere: the build reads it from
# the instance directory, so a copy would never see the change. What it owes is the
# same follow-up a change to the tracked Dockerfile owes, recorded here before any
# copy is made and printed with the rest below. In --check it is drift: the file is
# the instance's to keep, so nothing here can bring it current, and a run says so
# until the copy that saw the same content recorded it below. It is counted with
# the files below, in its own line, and not in changed[], which holds only source
# paths of the file table.
if [ -n "$sandboxunread" ]; then
  if [ "$check" = 1 ]; then
    note "unreadable: $instance/sandbox.Dockerfile (check its permissions)"
  else
    echo "tehas: $sandboxunread" >&2
    echo "tehas: nothing was installed" >&2
    exit 78
  fi
fi
if [ "$sandbox_changed" = 1 ]; then
  sandbox_drift=1
  if [ "$check" = 1 ]; then
    note "changed: $instance/sandbox.Dockerfile since the last install"
  else
    record_step "sandbox/Dockerfile"
    # What this install saw, so a later run owes the build only when the file
    # changed again. Written where the step is taken, and not after the copies:
    # a run that had nothing else to do prints "tehas is up to date" and ends
    # before then. A run that died with the step still in the record leaves the
    # file unseen, so tehas update owes the build and the next install sees it.
    mkdir -p "$TEHAS_HOME/state"
    if hash=$(sha256sum "$instance/sandbox.Dockerfile" 2>/dev/null); then
      printf '%s\n' "${hash:0:64}" | write_file "$sandbox_at_file"
    else
      # Between the read above and this one the file became unreadable. The
      # step stays owed and nothing is recorded, so the next run asks again.
      echo "note: $instance/sandbox.Dockerfile cannot be read (check its permissions); the sandbox build stays owed" >&2
    fi
  fi
fi

for e in "${files[@]}"; do put "$e"; done

# The operating manual and the front page are directories; pages come and go.
sync_dir() { # name: host/<name>/* -> $TEHAS_HOME/<name>/
  local name=$1 from to
  for from in "$host/$name"/*; do
    put "$name/${from##*/}|$TEHAS_HOME/$name/${from##*/}|644"
  done
  [ -d "$TEHAS_HOME/$name" ] || return 0
  for to in "$TEHAS_HOME/$name"/*; do
    [ -e "$to" ] || continue
    # A file an earlier run of this script left beside the pages and did not
    # rename over. It is removed with the rest, and a --check names it.
    case "$to" in
      *.new)
        changed+=("$name/${to##*/}")
        if [ "$check" = 1 ]; then note "stale: $to"; else rm -f "$to" && echo "removed: $to"; fi
        continue
        ;;
    esac
    case "$to" in *.prev | */version.txt | */rukas-version.txt) continue ;; esac
    [ -e "$host/$name/${to##*/}" ] && continue
    changed+=("$name/${to##*/}")
    if [ "$check" = 1 ]; then note "stale: $to"; else rm -f "$to" "$to.prev" && echo "removed: $to"; fi
  done
}
sync_dir docs
sync_dir fun

# The version in the footer of every page: the newest tag this checkout holds
# and the commit it is at, in a file beside the pages, which read it. Only a
# checkout that is a git repository of its own has one; a tree that came from a
# release tarball has no git and no commit, and its VERSION file, written by
# scripts/pack-release.sh, names the release instead. It is no part of --check:
# a host is as current as its files are.
checkout_version() {
  local tag hash
  if [ -z "$(git -C "$root" rev-parse --show-prefix 2>/dev/null || echo none)" ]; then
    hash=$(git -C "$root" rev-parse --short=7 HEAD 2>/dev/null) || return 0
    tag=$(git -C "$root" describe --tags --abbrev=0 2>/dev/null) || tag=""
    printf '%s\n' "${tag:+$tag+}$hash" | tr -cd 'A-Za-z0-9._+\n-'
    return 0
  fi
  # One line, vX.Y.Z. Read as it is: what a tarball carries is this script's
  # own bytes, and tehas api and the footers bound what they take from it.
  [ -f "$root/VERSION" ] || return 0
  head -n 1 "$root/VERSION"
}
if [ "$check" != 1 ]; then
  version=$(checkout_version)
  for dir in docs fun; do
    to=$TEHAS_HOME/$dir/version.txt
    if [ -z "$version" ]; then
      rm -f "$to"
    elif [ "$(cat "$to" 2>/dev/null)" != "$version" ]; then
      echo "$version" | write_file "$to"
      echo "version: $version in $to"
    fi
  done
fi

# A build stages two names of the image's recipe beside the installed build
# script, in the directory this script owns: the copy it hands docker and the
# `mktemp` temporary it renames from. A run that ended takes both back and a run
# a signal took off leaves at most one, which `tehasbot clean` takes past
# TEHAS_CLEAN_AFTER hours. Either found here of an earlier install's is this
# directory's to retire: nothing installs them, and nothing else writes them.
for f in "$TEHAS_HOME"/sandbox/sandbox-uid.Dockerfile \
  "$TEHAS_HOME"/sandbox/sandbox-uid.Dockerfile.[0-9A-Za-z][0-9A-Za-z][0-9A-Za-z][0-9A-Za-z][0-9A-Za-z][0-9A-Za-z]; do
  [ -f "$f" ] && [ ! -L "$f" ] || continue
  changed+=("sandbox/${f##*/}")
  if [ "$check" = 1 ]; then note "stale: $f"; else rm -f "$f" && echo "removed: $f"; fi
done

# The commands of before bin/tehas, one file each in bin/, and the console of
# before it was a compiled program. And the Protolab's server files of before the
# edge router. And the wrappers of what is now a verb of the console, and the
# libraries of what is now a verb of tehas-cycle. Found here,
# removed in step 5 once systemd has the new units.
old=()
for f in \
  bin/tehas-console.ts \
  bin/tehas-dispatch \
  bin/tehas-energy \
  bin/tehas-health \
  bin/tehas-index \
  bin/tehas-protolab \
  bin/tehas-status \
  bin/tehas-test \
  bin/tehas-work \
  lib/describe.sh \
  lib/private.sh \
  libexec/analytics-cmd \
  libexec/console.ts \
  libexec/disk \
  libexec/energy \
  libexec/health \
  libexec/limit \
  libexec/pause \
  libexec/state \
  libexec/summary \
  libexec/top \
  libexec/top-publish \
  lib/top.sh \
  edge/protolab.Caddyfile \
  edge/protolab.compose.yaml; do
  to=$TEHAS_HOME/$f
  [ -e "$to" ] || continue
  old+=("$to")
  changed+=("$f")
  # A running sampler or publisher is the old file calling itself; only a
  # restart moves it on, and the units step is what restarts it.
  if [ "$check" = 1 ]; then note "stale: $to"; else record_step "$f"; fi
done

# The slice file on the host against the one in this checkout. Nothing here
# reads the instance or asks systemd.
slice_to=${BOOTSTRAP_ETC:-/etc}/systemd/system/tehas.slice
slice_state() {
  if [ -n "${T_IN_TEST:-}" ] && [ -z "${T_SLICE:-}" ]; then
    echo "not checked"
  elif [ ! -f "$slice_to" ]; then
    echo MISSING
  elif cmp -s "$host/slice/tehas.slice" "$slice_to"; then
    echo OK
  else
    echo DIFFERS
  fi
}

# TEHAS_WORK_IMAGES=1 promises a cycle the images its issue links. The pi-rukas
# in the sandbox image has to read the manifest tehas writes beside them for
# that to be true, and the image says whether it does, by a label its build
# writes (host/sandbox/build.sh). A disagreement is between the setting and the
# image, not in what this script installs, so it is said and not failed: the
# host rebuilds the image on a pi-rukas that reads them, or turns the setting off.
# Nothing when the setting is off, when the image reads them, or when there is
# no image to ask.
images_say() { # <check|run>
  local mode=$1 status sentence
  if [ "$TEHAS_WORK_IMAGES" != 1 ]; then return 0; fi
  docker image inspect "$PI_ENSEMBLE_IMAGE" >/dev/null 2>&1 || return 0
  case "$(tehas_rukas_reads_images)" in
    1) return 0 ;;
    0)
      status="NOT READ"
      sentence="TEHAS_WORK_IMAGES is 1 and the sandbox image $PI_ENSEMBLE_IMAGE says its pi-rukas does not read the images a cycle is given. The dispatcher holds every cycle while that stands, not only an issue whose text links a picture: rebuild the image on a pi-rukas that reads them (tehas update), or set TEHAS_WORK_IMAGES=0."
      ;;
    *)
      status="NOT SAID"
      sentence="TEHAS_WORK_IMAGES is 1 and the sandbox image $PI_ENSEMBLE_IMAGE does not say whether its pi-rukas reads the images a cycle is given. Rebuild it with $TEHAS_HOME/sandbox/build.sh, which records the answer."
      ;;
  esac
  if [ "$mode" = check ]; then
    echo "Images:       $status"
    echo "  $sentence"
  else
    echo "note: $sentence"
  fi
}

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

  # Every container runs in the slice, so a host without it, or with another
  # version of it, has something to do. Only root can do it; the check says
  # so until it is done.
  case "$(slice_state)" in
    OK) echo "Slice:        OK" ;;
    MISSING) echo "Slice:        MISSING (sudo ./bootstrap.sh slice)"; rc=1 ;;
    DIFFERS) echo "Slice:        DIFFERS (sudo ./bootstrap.sh slice)"; rc=1 ;;
    *) echo "Slice:        not checked (running inside the tests)" ;;
  esac

  images_say check

  # The last tehas check-product, read and not run: the product is not this
  # host's install, so it does not change the exit status.
  product=$TEHAS_HOME/state/check-product.json
  if [ -r "$product" ]; then
    jq -r '"Product:      " + (if .ok then "OK" else "\(.failures | length) TO FIX" end)
      + " (tehas check-product, \(.at))", (.failures[] | "  \(.)")' "$product" 2>/dev/null ||
      echo "Product:      NOT READ ($product)"
  fi

  if [ "${#changed[@]}" = 0 ] && [ "$sandbox_drift" = 0 ]; then
    echo "Installed:    OK"
  else
    echo "Installed:    DRIFT ($((${#changed[@]} + sandbox_drift)) files)"
    printf '%s\n' "${report[@]}" | sed 's/^/  /'
    rc=1
  fi

  # The command the other accounts type is root's to install, so it is not part
  # of the drift above: no run of this script can clear it. It is named apart,
  # with the command that does clear it.
  if bashrc_ok; then
    echo "Shell:        OK"
  else
    echo "Shell:        MISSING ($bashrc does not source $TEHAS_HOME/env.sh; ./install.sh puts the line at its top)"
    rc=1
  fi
  if [ -f "$first_owed" ]; then
    echo "First install: NOT DONE (labels and core units; ./install.sh does them)"
    rc=1
  fi
  if [ -n "$store_refusal" ]; then
    echo "Store:        REFUSED ($store_refusal; ./install.sh stops before it writes)"
    rc=1
  fi

  if [ -n "$login_fix" ]; then
    echo "Login:        NOT CURRENT"
    if [ -e "$login_bin" ]; then
      echo "  differs: $login_bin   ($login_fix)"
    else
      echo "  missing: $login_bin   ($login_fix)"
    fi
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

  if [ "$tests" = 0 ]; then
    echo "Tests:        not run (./install.sh --check --tests runs them)"
  elif [ -n "${T_IN_TEST:-}" ] && [ -z "$suite" ]; then
    echo "Tests:        not checked (running inside the tests)"
  elif out=$("${suite:-$root/scripts/test.sh}" 2>&1); then
    echo "Tests:        OK"
  else
    echo "Tests:        FAILED"
    printf '%s\n' "$out" | tail -n 30 | sed 's/^/  /'
    rc=1
  fi
  exit "$rc"
fi

# --- 5. After a real run ------------------------------------------------------
# The directories of a home are made in step 3, before the marker and the first
# copy.

# The store's database, under the update lock this run still holds: made when
# there is none, otherwise brought forward by the store, which copies it into
# state/backup/ first. Every real run asks, so a host is never left on a schema
# older than the store it was given. What the store is told is what this run
# loaded: the version and commit of this tree, the instance and the repository.
# A store that fails stops the run: the files are copied, the database is left
# as it was, and the store's own line says why.
store_db() {
  local store=$TEHAS_HOME/libexec/store db=$TEHAS_HOME/state/tehas.db version name out verb
  version=$(checkout_version)
  name=${instance%/}
  name=${name##*/}
  [ -n "$version" ] || version=${store_commit:0:7}
  if [ -e "$db" ] || [ -L "$db" ]; then
    verb=(migrate --backup "$TEHAS_HOME/state/backup")
  else
    verb=(init)
  fi
  [ -x "$store" ] || { echo "tehas: $store is not installed, so $db was neither made nor migrated" >&2; exit 1; }
  chmod 700 "$TEHAS_HOME/state" || { echo "tehas: could not keep $TEHAS_HOME/state mode 0700" >&2; exit 1; }
  if ! out=$(TEHAS_HOME=$TEHAS_HOME "$store" "${verb[@]}" --version "$version" --commit "$store_commit" \
    --instance "$name" --repo "$TEHAS_REPO" 2>&1); then
    printf '%s\n' "$out" | tail -n 5 >&2
    echo "tehas: the store could not ${verb[0]} $db; the files are installed, the database was left as it was" >&2
    exit 1
  fi
  case ${verb[0]} in
    init) echo "store: made $db" ;;
    *) echo "store: $db at schema $(jq -r '.to // "?"' <<<"$out" 2>/dev/null), backup $(jq -r '.backup // "?"' <<<"$out" 2>/dev/null)" ;;
  esac
  # The records an older tehas kept as files, made rows; each file is moved to
  # state/imported-<day>/ once its rows read back, and nothing is deleted. A run
  # again is safe: what is in the store already is not written twice.
  if ! out=$(TEHAS_HOME=$TEHAS_HOME "$store" import 2>&1); then
    printf '%s\n' "$out" | head -n 40 >&2
    echo "tehas: the store could not import the old records under $TEHAS_HOME/state; the files are installed, the database holds what was imported, and install.sh can run again" >&2
    exit 1
  fi
  # shellcheck disable=SC2016  # a jq program
  if jq -e '.moved > 0' <<<"$out" >/dev/null 2>&1; then
    echo "store: imported $(jq -r '[.imported | to_entries[] | select(.value > 0) | "\(.key) \(.value)"] | join(", ")' <<<"$out"); moved $(jq -r .moved <<<"$out") files to $TEHAS_HOME/state/imported-$(date -u +%Y%m%d)/"
  fi
}
store_db

# A planning session's kept directory, state/plan-<n>.json.keep, holds what a
# container wrote, so it moves out of state/ to beside the workspaces, where
# the planning turn now keeps it: $TEHAS_WORK_DIR-plan-<n>.keep. Only while no
# turn holds the issue's lock, and never over a directory that is there.
for keep in "$TEHAS_HOME"/state/plan-[0-9]*.json.keep; do
  [ -d "$keep" ] && [ ! -L "$keep" ] || continue
  n=${keep##*/plan-}
  n=${n%.json.keep}
  case $n in '' | *[!0-9]*) continue ;; esac
  to=$TEHAS_WORK_DIR-plan-$n.keep
  lock=$TEHAS_HOME/state/plan-$n.json.lock
  if [ -e "$lock" ] && ! { flock -n 8; } 8<"$lock"; then
    echo "later: ./install.sh again when the planning turn of #$n has ended   ($keep stays where it is)"
  elif [ -e "$to" ] || [ -L "$to" ]; then
    echo "note: $keep stays where it is: $to is there already" >&2
  elif mkdir -p "${to%/*}" && mv -T -- "$keep" "$to"; then
    echo "moved: $keep to $to"
  else
    echo "tehas: could not move $keep to $to" >&2
    exit 1
  fi
done

# A cycle used to leave state/cycles/<issue> - one empty file per issue number,
# the record that this host had run a cycle for it. Nothing read it, and the
# workspace is a fresh clone every time, so the record and the flag it fed are
# gone. The files of a host that ran cycles then are removed here; the lock,
# the taken mark and every file with a suffix stay.
for left in "$TEHAS_HOME"/state/cycles/[0-9]*; do
  [ -f "$left" ] && [ ! -L "$left" ] || continue
  case "${left##*/}" in *[!0-9]*) continue ;; esac
  rm -f "$left" && echo "removed: $left"
done

# The previews' network keeps a preview from the host: it reaches the router
# and the other previews and nothing else. What that takes is in lib/edge.sh,
# and `tehasbot preview` decides by the same functions before every run. The
# router's compose file names the network as external and `tehasbot preview`
# puts containers on it, so it is made here, whichever of the two starts first.
# A host with no Docker yet gets it from `tehasbot preview`. A Docker older
# than 27.1 is not asked: it does not know the option that keeps the host out.
# The bridge driver of Docker 29.1.3 was seen to accept an option it does not
# know, so an older one may make a network that looks right and gives the
# host an address. That was not tried, and with the version checked it does
# not have to be. A network that is there and does not keep the host out
# is not removed here: containers may be on it.
if command -v docker >/dev/null 2>&1; then
  if ! previews_fault=$(edge_docker_fault); then
    note "The network tehas-previews was not made or checked: $previews_fault. No preview is built until Docker is 27.1 or later."
  else
    previews_rc=0
    previews_fault=$(edge_net_fault tehas-previews) || previews_rc=$?
    case "$previews_rc" in
      0) ;;
      2)
        edge_net_create tehas-previews ||
          note "The network tehas-previews could not be made; the edge router will not start without it."
        ;;
      *)
        note "The network tehas-previews does not keep the host out ($previews_fault). It has to be remade before a preview is built; the manual's troubleshooting page says how."
        ;;
    esac
  fi
fi

case "$(slice_state)" in
  MISSING) echo "note: $slice_to is missing (sudo ./bootstrap.sh slice)" ;;
  DIFFERS) echo "note: $slice_to differs from host/slice/tehas.slice in this checkout (sudo ./bootstrap.sh slice)" ;;
esac

images_say run

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

# The labels and the core units, once, on a first install. The optional units
# only when their setting is on. Every later change to a unit file is the units
# step's, which `tehas update` runs.
first_install() {
  [ -f "$first_owed" ] || return 0
  local units=(tehas-dispatch.timer tehas-dispatch.path) why="" energy
  [ -z "${TEHAS_IMPROVE_REPO:-}" ] || units+=(tehas-improve.timer)
  [ -z "${TEHAS_DOMAIN:-}" ] || units+=(tehas-protolab.timer)
  [ "${TEHAS_PREVIEWS:-}" != 1 ] || units+=(tehas-preview.timer tehas-preview-sweep.timer)
  # A first install runs before any shell has read env.sh, which exports this
  # from the host's private file, so it is read from that file too.
  energy=${TEHAS_ENERGY:-}
  [ -n "$energy" ] || [ ! -r "$HOME/.config/tehas/env" ] ||
    energy=$(sed -n 's/^TEHAS_ENERGY=//p' "$HOME/.config/tehas/env" | tail -n 1)
  case "${energy:-off}" in off) ;; *) units+=(tehas-energy.service) ;; esac
  if ! "$TEHAS_HOME/bin/tehas" labels >/dev/null; then
    why="tehas labels failed"
  elif ! systemctl --user daemon-reload || ! systemctl --user enable --now "${units[@]}"; then
    why="systemctl --user enable failed"
  fi
  if [ -n "$why" ]; then
    echo "later: ./install.sh again   (the first install's labels or units are not done: $why)"
    return 0
  fi
  rm -f "$first_owed"
  echo "first install: labels made in $TEHAS_REPO, enabled ${units[*]}"
}

# A unit systemd gave up on stays failed until it is reset: a failed
# tehas-dispatch.path (its service hit its start limit) starts no poll when a
# cycle ends. Not on a first install, which has enabled nothing yet. The path
# unit is started again when it was the one that failed; the other units are
# reset and left to their timers.
reset_failed_units() {
  local path_state
  [ ! -f "$first_owed" ] || return 0
  path_state=$(systemctl --user is-failed tehas-dispatch.path 2>/dev/null) || true
  systemctl --user reset-failed 'tehas-*' >/dev/null 2>&1 || return 0
  [ "$path_state" = failed ] || return 0
  if systemctl --user start tehas-dispatch.path >/dev/null 2>&1; then
    echo "systemd: tehas-dispatch.path had failed; reset and started"
  else
    echo "later: systemctl --user start tehas-dispatch.path   (it had failed and did not start again)"
  fi
}

# The one file of the host this run cannot bring current, because it belongs to
# root: said as a later line, naming the command that does it. Not a step - no
# command runs sudo - so it is never printed as "next:".
report_login() {
  if [ -n "$login_fix" ]; then
    echo "later: $login_fix   ($login_bin is not this checkout's host/login/tehas; only root installs it)"
  fi
}

if [ "${#changed[@]}" = 0 ] && [ "$sandbox_changed" != 1 ]; then
  echo "tehas is up to date"
  dispatch_release
  reset_failed_units
  first_install
  report_login
  first_index
  exit 0
fi
# The follow-ups, by the ids lib/update-steps.sh defines: one line per step these
# changed files call for, in the order they run, and one for the instance's own
# sandbox file when it changed - the file is no source of this checkout, so it is
# in no list here and calls for the build under its own name. The steps are
# already in the record of steps owed, recorded file by file above, and
# `tehas update` is what runs them: it reloads the unit files before restarting
# any unit, and a step that cannot be performed leaves the run red and the step
# owed. A person who ran install.sh by hand reads the same lines, and the record
# stays until `tehas update` clears it.
# versions.env counts among these sources only for the lines the image reads:
# the same answer the record was made from, read before any copy was made.
step_sources=()
for src in "${changed[@]-}"; do
  [ -n "$src" ] || continue
  if [ "$src" = versions.env ] && [ "$versions_owed" != 1 ]; then continue; fi
  step_sources+=("$src")
done
seen_steps=$(while IFS= read -r step; do echo "next: $step"; done < <(tehas_steps_for "${step_sources[@]}"))
if [ "$sandbox_changed" = 1 ]; then
  # Once, whatever the file's bytes also happened to call for: the build the
  # instance's file owes is the same build the platform's Dockerfile does, and a
  # run that named it twice would read as two builds.
  case $seen_steps in
    *"next: sandbox"*) ;;
    *) seen_steps=${seen_steps}"$(printf 'next: sandbox')" ;;
  esac
fi
printf '%s\n' "$seen_steps"

# The pins moved, and the tools on the host follow them only when bootstrap.sh
# user runs. Not a step: it fetches from the network and may take long.
if [ "$had_versions" = 1 ] && [[ " ${changed[*]} " == *" versions.env "* ]]; then
  echo "later: $root/bootstrap.sh user   (versions.env changed; until it runs, --check says Dependencies: NOT OK)"
fi

# The old commands go while this holds the dispatcher's lock, taken in step 3a:
# a poll that starts meanwhile finds the lock taken and ends before it has run
# anything. The units systemd holds name them too, so systemd reloads first,
# and only silence next to exit 0 says it did. Otherwise the files are left and
# a later run of this script removes them: they are still there to be found, so
# nothing is recorded. A file that cannot be removed fails the install. The
# dispatcher's lock is given back here, before the first index below takes it.
retire_later=""
if [ "${#old[@]}" -gt 0 ]; then
  if ! out=$(systemctl --user daemon-reload 2>&1) || [ -n "$out" ]; then
    retire_later="once systemctl --user daemon-reload works   (systemd still holds the units that name the old commands and server files in $TEHAS_HOME, so they were left)"
  elif [ "$dispatch_held" = 1 ]; then
    for to in "${old[@]}"; do
      rm -f "$to" "$to.prev"
      echo "removed: $to"
    done
  fi
fi
dispatch_release
[ -z "$retire_later" ] || echo "later: ./install.sh again $retire_later"
reset_failed_units
first_install
report_login
first_index
exit 0
