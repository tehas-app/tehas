#!/usr/bin/env bash
# Install tehas on this host from this checkout, for the instance whose
# settings are in the instance directory (see lib/config.sh and
# instances/example). Run as the tehas user:
#
#   ./install.sh            copy changed files into place
#   ./install.sh --check    change nothing; report settings, dependencies,
#                           the slice, installed files, the command the other
#                           accounts type when root's copy is stale, and tests,
#                           each on its own line
#
# What is installed: the commands and units (issue to pull request), the
# sandbox files, the docs server, the edge router, the Protolab, the
# improvement loop and the manual, plus the instance's own models.json and, if it has one, settings.json.
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
# kept next to the new one as <name>.prev. Each file is copied to <name>.new
# and renamed over the installed one.
#
# It also removes the commands of an earlier layout, one file for each command
# in ~/tehas/bin and the console script in ~/tehas/libexec, unless a cycle is
# running that still uses them.
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
# It asks systemd for nothing, with one exception. A copied unit file is not in
# effect until systemd has reloaded it, and that reload belongs to the step
# that restarts the units, so no command can report a changed unit as applied
# when it was not. The exception is the earlier layout: its commands are
# removed only after systemd has reloaded, because the units it still holds
# name them.
#
# A real run holds $TEHAS_HOME/state/update.lock, the lock tehas update holds,
# and stops when another run has it.
#
# Exit: 0 done or nothing to report, 1 --check found something, a step failed
# or another update is running, 78 the instance settings are invalid (nothing
# was changed).
set -euo pipefail

root=$(cd "$(dirname "$0")" && pwd)
host=$root/host
check=0
[ "${1:-}" = "--check" ] && check=1
# Read before tehas_load, which drops it from the environment.
update_fd=${TEHAS_UPDATE_LOCK_FD:-}

# shellcheck source=lib/config.sh
. "$root/lib/config.sh"
# shellcheck source=lib/update-steps.sh
. "$root/lib/update-steps.sh"
# shellcheck source=lib/edge.sh
. "$root/lib/edge.sh"
# shellcheck source=lib/cycles.sh
. "$root/lib/cycles.sh"

# --- 1. Instance settings ---------------------------------------------------
# Every finding is one line. Any finding stops a real run before it changes
# anything.
instance_findings() {
  local dir file models product_model mode owner group provider model
  tehas_load 2>&1 || return 0
  dir=$TEHAS_INSTANCE_DIR
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

# One update at a time. Two runs side by side would build the image twice and
# each rewrite the record of steps owed with its own view. tehas update holds the
# lock while it runs this script and names the descriptor it holds it on; any
# other run takes the lock itself. --check changes nothing and takes no lock.
if [ "$check" != 1 ]; then
  update_lock=$TEHAS_HOME/state/update.lock
  mkdir -p "$TEHAS_HOME/state"
  if [[ $update_fd =~ ^[0-9]+$ ]] && [ "/dev/fd/$update_fd" -ef "$update_lock" ]; then
    flock -n "$update_fd" || { echo "tehas: another update is running ($update_lock); nothing was installed" >&2; exit 1; }
  else
    exec 7>>"$update_lock"
    flock -n 7 || { echo "tehas: another update is running ($update_lock); nothing was installed" >&2; exit 1; }
  fi
fi

# --- 2. What belongs to this instance ---------------------------------------
# One line per file: source|destination|mode. A source starting with / is taken
# as is; anything else is under host/, except bin/, libexec/ and lib/ at the top.
agent=$HOME/.pi/agent
units=$HOME/.config/systemd/user

files=(
  "env.sh|$TEHAS_HOME/env.sh|644"
  "lib/config.sh|$TEHAS_HOME/lib/config.sh|644"
  "lib/attachments.sh|$TEHAS_HOME/lib/attachments.sh|644"
  "lib/describe.sh|$TEHAS_HOME/lib/describe.sh|644"
  "lib/cycles.sh|$TEHAS_HOME/lib/cycles.sh|644"
  "lib/labels.sh|$TEHAS_HOME/lib/labels.sh|644"
  "lib/improve.sh|$TEHAS_HOME/lib/improve.sh|644"
  "lib/improve-prompt.md|$TEHAS_HOME/lib/improve-prompt.md|644"
  "lib/live.sh|$TEHAS_HOME/lib/live.sh|644"
  "lib/update-steps.sh|$TEHAS_HOME/lib/update-steps.sh|644"
  "lib/edge.sh|$TEHAS_HOME/lib/edge.sh|644"
  "lib/sandbox.sh|$TEHAS_HOME/lib/sandbox.sh|644"
  "lib/slice.sh|$TEHAS_HOME/lib/slice.sh|644"
  "lib/questions.sh|$TEHAS_HOME/lib/questions.sh|644"
  "lib/status.sh|$TEHAS_HOME/lib/status.sh|644"
  "lib/top.sh|$TEHAS_HOME/lib/top.sh|644"
  "lib/planning.sh|$TEHAS_HOME/lib/planning.sh|644"
  "versions.env|$TEHAS_HOME/versions.env|644"
  "sandbox/Dockerfile|$TEHAS_HOME/sandbox/Dockerfile|644"
  "sandbox/build.sh|$TEHAS_HOME/sandbox/build.sh|755"
  "sandbox/npm/playwright/package.json|$TEHAS_HOME/sandbox/npm/playwright/package.json|644"
  "sandbox/npm/playwright/package-lock.json|$TEHAS_HOME/sandbox/npm/playwright/package-lock.json|644"
  "edge/docs.Caddyfile|$TEHAS_HOME/edge/docs.Caddyfile|644"
  "edge/docs.compose.yaml|$TEHAS_HOME/edge/docs.compose.yaml|644"
  "edge/router.Caddyfile|$TEHAS_HOME/edge/router.Caddyfile|644"
  "edge/router.compose.yaml|$TEHAS_HOME/edge/router.compose.yaml|644"
  "systemd/tehas-cycle@.service|$units/tehas-cycle@.service|644"
  "systemd/tehas-dispatch.service|$units/tehas-dispatch.service|644"
  "systemd/tehas-dispatch.timer|$units/tehas-dispatch.timer|644"
  "systemd/tehas-energy.service|$units/tehas-energy.service|644"
  "systemd/tehas-improve.service|$units/tehas-improve.service|644"
  "systemd/tehas-improve.timer|$units/tehas-improve.timer|644"
  "systemd/tehas-protolab.service|$units/tehas-protolab.service|644"
  "systemd/tehas-protolab.timer|$units/tehas-protolab.timer|644"
  "systemd/tehas-preview.service|$units/tehas-preview.service|644"
  "systemd/tehas-preview.timer|$units/tehas-preview.timer|644"
  "systemd/tehas-preview-sweep.service|$units/tehas-preview-sweep.service|644"
  "systemd/tehas-preview-sweep.timer|$units/tehas-preview-sweep.timer|644"
  "systemd/tehas-top.service|$units/tehas-top.service|644"
  "pi/mcp.json|$agent/mcp.json|644"
  "$instance/models.json|$agent/models.json|644"
)
# The two commands on PATH, and the scripts behind them.
for f in tehas tehasbot; do
  files+=("bin/$f|$TEHAS_HOME/bin/$f|755")
done
for f in cycle dev dev-login dispatch energy health improve index labels pause plan preview protolab resume status summary top top-publish update work; do
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

# The step this source calls for, added to the record of steps owed. One call per
# copied file, so the record is never behind what the host has already been given.
record_step() { # one source path from the file table
  local step steps=()
  while IFS= read -r step; do steps+=("$step"); done < <(tehas_steps_for "$1")
  if [ "${#steps[@]}" != 0 ]; then tehas_record_steps "${steps[@]}" || exit 1; fi
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
    # of the crates run here: they are given no variable that is named like a
    # secret, the bot's token among them.
    target=${CARGO_TARGET_DIR:-$root/console/target}
    built=$target/release/console
    rm -f "$built"
    hidden=()
    while IFS= read -r name; do hidden+=(-u "$name"); done < <(compgen -e | grep -E 'TOKEN|SECRET|PASSWORD|API_KEY')
    out=$(env "${hidden[@]}" cargo build --release --locked --quiet \
      --manifest-path "$root/console/Cargo.toml" --target-dir "$target" 2>&1) && rc=0 || rc=$?
    if [ "$rc" != 0 ] || [ ! -f "$built" ]; then
      printf '%s\n' "$out" | tail -n 20 >&2
      if [ "$rc" = 127 ]; then
        echo "tehas: cargo is not installed, so the console cannot be built (bootstrap.sh installs it); nothing was installed" >&2
      else
        echo "tehas: the console did not build (cargo build --release --locked in $root/console); nothing was installed" >&2
      fi
      exit 1
    fi
    put "$built|$console_to|755"
    echo "$console_source $(console_sum "$console_to")" | write_file "$console_to.stamp"
  fi
fi

# A host has its tools from bootstrap.sh before its first install, so only a
# versions.env that replaces another one asks for it again.
had_versions=0
[ -f "$TEHAS_HOME/versions.env" ] && had_versions=1
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
    case "$to" in *.prev | */version.txt) continue ;; esac
    [ -e "$host/$name/$(basename "$to")" ] && continue
    changed+=("$name/$(basename "$to")")
    if [ "$check" = 1 ]; then note "stale: $to"; else rm -f "$to" "$to.prev" && echo "removed: $to"; fi
  done
}
sync_dir docs
sync_dir fun

# The version in the footer of every page: the newest tag this checkout holds
# and the commit it is at, in a file beside the pages, which read it. Only a
# checkout that is a git repository of its own has one. It is no part of
# --check: a host is as current as its files are.
checkout_version() {
  local tag hash
  [ -z "$(git -C "$root" rev-parse --show-prefix 2>/dev/null || echo none)" ] || return 0
  hash=$(git -C "$root" rev-parse --short=7 HEAD 2>/dev/null) || return 0
  tag=$(git -C "$root" describe --tags --abbrev=0 2>/dev/null) || tag=""
  printf '%s\n' "${tag:+$tag+}$hash" | tr -cd 'A-Za-z0-9._+\n-'
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

  if [ "${#changed[@]}" = 0 ]; then
    echo "Installed:    OK"
  else
    echo "Installed:    DRIFT (${#changed[@]} files)"
    printf '%s\n' "${report[@]}" | sed 's/^/  /'
    rc=1
  fi

  # The command the other accounts type is root's to install, so it is not part
  # of the drift above: no run of this script can clear it. It is named apart,
  # with the command that does clear it.
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
# preview/ is no mount: `tehasbot preview` keeps each pull request's state and
# its clone of the product there, and makes it itself as well. It is made here
# with the others so that every directory tehas writes to is there after an
# install, whether previews are on or not.
mkdir -p "$TEHAS_HOME/logs" "$TEHAS_HOME/state" "$TEHAS_HOME/tmp" "$TEHAS_HOME/live" "$TEHAS_HOME/protolab" \
  "$TEHAS_HOME/preview"

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

# The one file of the host this run cannot bring current, because it belongs to
# root: said as a later line, naming the command that does it. Not a step - no
# command runs sudo - so it is never printed as "next:".
report_login() {
  if [ -n "$login_fix" ]; then
    echo "later: $login_fix   ($login_bin is not this checkout's host/login/tehas; only root installs it)"
  fi
}

if [ "${#changed[@]}" = 0 ]; then
  echo "tehas is up to date"
  report_login
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

# The pins moved, and the tools on the host follow them only when bootstrap.sh
# user runs. Not a step: it fetches from the network and may take long.
if [ "$had_versions" = 1 ] && [[ " ${changed[*]} " == *" versions.env "* ]]; then
  echo "later: $root/bootstrap.sh user   (versions.env changed; until it runs, --check says Dependencies: NOT OK)"
fi

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
report_login
first_index
exit 0
