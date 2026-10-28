# shellcheck shell=bash
# What a sandbox container is given, and the tree it works in. Sourced by the
# commands that start one: tehas work (a cycle), tehas plan (a planning turn),
# tehas dev (a manual session), tehasbot review (a review) and tehas index (the
# indexer). It is in one place so they cannot drift apart; test/work_test.sh,
# test/plan_test.sh, test/dev_test.sh and test/review_test.sh pin the result
# option by option. The container's limits and
# the slice it runs in are not made here: they come from lib/slice.sh. The
# security options go to the cycle, the turn, the session, the review and the
# push check before a cycle; the indexer is not given them.
#
#   tehas_sandbox_env <tree> <name> <email> [no-token]
#                                             the -e options; with no-token,
#                                             every one of them but the bot's
#                                             token: a run that writes nothing
#                                             to the tracker is given no token
#                                             to write with, and no -e carries
#                                             its name at all
#   tehas_sandbox_settings_ok                 is Pi's settings.json there
#   tehas_sandbox_mounts <tree> <host> <state> <runs> [no-token]
#                                             the -v options; with no-token,
#                                             not the GitHub App's token
#                                             directory
#   tehas_sandbox_retry <host>                the provider retry a container's
#                                             Pi runs with, as one line
#   tehas_sandbox_seed <state> [<index dir>]  give <state> the host's code index
#   tehas_sandbox_cache_dir                   where a container has that copy
#   tehas_sandbox_target_dir                  where a manual session has its
#                                             cargo build output; only tehas dev
#                                             gives a container one
#   tehas_index_dir                           where the host keeps the index
#   tehas_git_github_host                     the github.com host git authenticates to
#   tehas_git_credential_helper_key           the config key of that host's helper
#   tehas_sandbox_helper_command              the helper command a container's git runs
#   tehas_sandbox_hosts                       the --add-host options
#   tehas_sandbox_security                    the options that take privileges
#                                             away from a container
#   tehas_sandbox_clone <src> <tree> <branch> make <tree>, a clone of <src>
#   tehas_sandbox_image_repo                  the sandbox image without its tag,
#                                             the name the build tags and the
#                                             cleaner lists dated tags under
#   images_held <the image's tags, "<tag> <id>" per line>
#                                             the images of those tags that
#                                             something holds: a container's
#                                             image ids and the ids of the uid
#                                             tags
#   tehas_base_image                          the pulled base image of the
#                                             sandbox build, as its tag or its
#                                             digest: the one name that image
#                                             is known by. Needs lib/rukas.sh
#                                             sourced beside this file, which
#                                             answers which commit the host
#                                             follows
#
# env, mounts, hosts and the security options print docker options separated
# by NUL bytes. Read them with
#   mapfile -d '' -t envs < <(tehas_sandbox_env "$tree" "$name" "$email")
#
# Source lib/config.sh and call tehas_load first: the settings are read from
# the environment it leaves.
#
# Not given to any container, and not a setting: the Docker socket, SSH keys,
# ~/.config/tehas, the product checkout, the rest of the host environment. Of
# ~/.config/tehas/env a container is given the bot's token and the model
# endpoints' tokens that models.json names, by name.

# shellcheck source=lib/models.sh
. "$(dirname "${BASH_SOURCE[0]}")/models.sh"

# Where a container has its own copy of the code index, and the variable that
# points the indexer and the agents' code search at it. It is a path inside the
# container, not a setting: no instance names it. The host directory behind it
# belongs to one container and goes with it, so nothing one container writes
# there is what a later container reads.
tehas_sandbox_cache_dir() { printf '%s\n' /home/vscode/.tehas-cache; }

# Where a manual session has its cargo build output: the path inside the
# container CARGO_TARGET_DIR names. It is not mounted or named here, because
# only tehas dev gives a container a build output of its own: a cycle's and a
# planning turn's stay in the container's own layer and go with it. libexec/dev
# makes the host directory beside <state> and prints the -v and -e pair from
# this function, so the mount, the variable, the manual and the tests name one
# path. A container path that did not match the image's home directory would
# send the build into the container's layer and the session would exit 0 with
# the cache discarded, so it is named once, as the code index's path is.
tehas_sandbox_target_dir() { printf '%s\n' /home/vscode/.tehas-target; }

# Where the host keeps the code index it gives to containers: the one directory
# the indexer writes and every container is given a copy of. No container is
# given it itself. Derived from TEHAS_HOME, not a setting.
tehas_index_dir() { printf '%s\n' "$TEHAS_HOME/state/index"; }

# The github.com host, and the git config key that names its credential
# helper. Two things read that key: the probe of tehas setup, which asks the
# host's git whether it has a helper, and the environment-level config below,
# which is what a container's git is given. A change here (another host,
# another subsection spelling) moves both; no other github.com of the platform
# is named by these.
tehas_git_github_host() { printf '%s\n' github.com; }
tehas_git_credential_helper_key() {
  local h
  h=$(tehas_git_github_host)
  printf '%s\n' "credential.https://$h.helper"
}

# The command a container's git runs to ask gh for a credential, read out of
# GH_TOKEN. The host's gh is asked to write the same one; the test of that
# write and the container's entry read it here.
tehas_sandbox_helper_command() { printf '%s\n' '!gh auth git-credential'; }

# The same variables as the pi-rukas wrapper sets, without the host's whole
# environment: the bot token by name, the PI_ENSEMBLE_* settings, and git's
# configuration. And the model endpoints' tokens models.json names, by name:
# Pi reads "$TEHAS_MODEL_KEY_<NAME>" from its environment.
#
# With no-token the bot's token is left out of the list altogether: not given
# an empty value and not given its bare name, but absent. A caller that starts
# a run which writes nothing to the tracker asks for that, so the name the
# token carries is not among the container's variables at all. What is kept back
# is decided here, before the list is made, and not by a caller editing the list
# afterwards: this is the one place a name is dropped.
#
# git inside the sandbox has no configuration of its own: no credential helper
# and no identity. Without the helper the driver commits but cannot push, and
# a cycle parks after all the work is done. Hand git the bot's token through gh
# (which reads GH_TOKEN) and an identity, as environment-level config: it
# outranks any file, reaches every agent process, and writes nothing into the
# tree.
tehas_sandbox_env() { # <tree> <git name> <git email> [no-token]
  local tree=$1 name=$2 email=$3 no_token=${4:-} var key
  key=$(tehas_git_credential_helper_key)
  local envs=(-e PI_ENSEMBLE_SANDBOX_MODE=1 -e "PI_ENSEMBLE_WORKSPACE_ROOT=$tree"
    -e "CBM_CACHE_DIR=$(tehas_sandbox_cache_dir)")
  [ -n "$no_token" ] || envs=(-e GH_TOKEN "${envs[@]}")
  envs+=(-e GIT_CONFIG_COUNT=4
    -e "GIT_CONFIG_KEY_0=$key" -e "GIT_CONFIG_VALUE_0="
    -e "GIT_CONFIG_KEY_1=$key" -e "GIT_CONFIG_VALUE_1=$(tehas_sandbox_helper_command)"
    -e "GIT_CONFIG_KEY_2=user.name" -e "GIT_CONFIG_VALUE_2=$name"
    -e "GIT_CONFIG_KEY_3=user.email" -e "GIT_CONFIG_VALUE_3=$email"
    -e GIT_TERMINAL_PROMPT=0)
  # Not the settings of pi-rukas's own docker wrapper, which no container
  # runs: the image, the aliases (given as --add-host) and the socket and SSH
  # switches. The runner does not list them as inputs.
  while IFS='=' read -r var _; do
    case "$var" in
      PI_ENSEMBLE_IMAGE | PI_ENSEMBLE_HOST_ALIASES | PI_ENSEMBLE_NO_DOCKER_SOCKET | PI_ENSEMBLE_NO_SSH) ;;
      PI_ENSEMBLE_*) envs+=(-e "$var") ;;
    esac
  done < <(env)
  while read -r var; do
    [ -z "$var" ] || [ -z "${!var:-}" ] || envs+=(-e "$var")
  done < <(tehas_model_key_names)
  printf '%s\0' "${envs[@]}"
}

# What one container is given, and none of what another container wrote.
# <tree> is the path inside the container and <host> the directory mounted
# there. <state> is a host directory of this container's own: its copy of the
# code index, its memory and its sessions go under it. <runs> is the host
# directory of its transcripts. All four are named by the caller: there is no
# default that lets one container's writes be another's input. The caller makes
# <host>; this makes <state> and <runs>, because Docker would create a missing
# mount source as a root-owned directory.
#
# With no-token the GitHub App's token directory is not mounted: a run that is
# given no GH_TOKEN is not given the app's token another way either.
#
# The two cache volumes pi-rukas mounts are not mounted here. What a container
# writes under its home, caches of packages among them, stays in its own layer
# and goes when the container is removed. Pi's settings.json is the one
# install.sh puts in ~/.pi/agent: its provider retry and timeout. The image has
# none, and without it Pi makes no retry of a failed request. A missing file is
# a failure, not a default: Docker would make a root-owned directory in its
# place.
#
# models.json is the container's own, <state>/models.json: the host's with
# each provider on the endpoint picked now (lib/models.sh). <state>/endpoints
# says which providers are on their backup.
# Whether Pi's settings.json and models.json are there to be mounted, and
# the tokens models.json names are set. The commands ask before
# they call Docker or git or write anything; tehas_sandbox_mounts asks again.
tehas_sandbox_settings_ok() {
  local f
  for f in settings.json models.json; do
    [ -f "$HOME/.pi/agent/$f" ] || { tehas_err "no $HOME/.pi/agent/$f: run install.sh"; return 1; }
  done
  tehas_model_keys_ok
}

tehas_sandbox_mounts() { # <tree> <host> <state> <runs> [no-token]
  local tree=$1 host=$2 state=$3 runs=$4 no_token=${5:-} agent=$HOME/.pi/agent cache
  [ -n "$tree" ] && [ -n "$host" ] && [ -n "$state" ] && [ -n "$runs" ] || return 1
  tehas_sandbox_settings_ok || return 1
  cache=$(tehas_sandbox_cache_dir)
  mkdir -p "$runs" "$state/cache" "$state/memory" "$state/sessions" || return 1
  tehas_models_resolve "$state/models.json" >"$state/endpoints" || return 1
  printf '%s\0' -v "$host:$tree" \
    -v "$state/cache:$cache" \
    -v "$state/memory:/home/vscode/.vipune" \
    -v "$state/sessions:/home/vscode/.pi/agent/sessions" \
    -v "$runs:/home/vscode/.pi/agent/ensemble-runs" \
    -v "$state/models.json:/home/vscode/.pi/agent/models.json:ro" \
    -v "$agent/mcp.json:/home/vscode/.pi/agent/mcp.json:ro" \
    -v "$agent/settings.json:/home/vscode/.pi/agent/settings.json:ro"
  # The GitHub App's token directory, read-only, when the instance names an
  # app: the image's gh (host/sandbox/gh-shim) reads the token the host keeps
  # fresh there. Not the key, which stays in ~/.config/tehas. A security change
  # under AGENTS.md rule 4: test/work_test.sh pins it.
  if [ -n "${TEHAS_GITHUB_APP_ID:-}" ] && [ -z "$no_token" ]; then
    mkdir -p "$TEHAS_HOME/state/github" || return 1
    printf '%s\0' -v "$TEHAS_HOME/state/github:/run/tehas/github:ro"
  fi
}

# The provider and agent retry and timeout a container's Pi runs with: the host's
# settings.json that tehas_sandbox_mounts gives it, under the product's own
# .pi/settings.json in <host>, merged the way Pi merges them. A value neither
# sets is Pi's default. For the cycle's log, so a reader of it knows them.
tehas_sandbox_retry() { # <host>
  local global=$HOME/.pi/agent/settings.json project=$1/.pi/settings.json
  [ -f "$global" ] || { echo "provider retry: unknown, $global is missing"; return 0; }
  [ -r "$project" ] || project=/dev/null
  jq -n -r --slurpfile g "$global" --slurpfile p "$project" '
    (($g[0] // {}) * ($p[0] // {})) as $s
    | def v($x): if $x == null then "unset" else ($x | tostring) end;
    "provider retry: timeoutMs=\(v($s.retry.provider.timeoutMs))"
    + " maxRetries=\(v($s.retry.provider.maxRetries))"
    + " maxRetryDelayMs=\(v($s.retry.provider.maxRetryDelayMs))"
    + " httpIdleTimeoutMs=\(v($s.httpIdleTimeoutMs))"
    + " agentMaxRetries=\(v($s.retry.maxRetries))"
    + " agentBaseDelayMs=\(v($s.retry.baseDelayMs))"
    + " agentMaxDelayMs=\(v($s.retry.maxAgentDelayMs))"' 2>/dev/null ||
    echo "provider retry: unknown, $global or $project is not JSON"
}

# Put the host's code index in <state>/cache, the copy this container is given.
# The index is one sqlite file per tree that was indexed. Not copied: the
# indexer's own logs, which a container has no use for, and what a run that
# was stopped left behind (a staging file about half the size of the index it
# was writing, read by nothing). Without that the copy would be several times
# the index and grow with every interrupted run.
# <index dir> is tehas_index_dir unless another is named. A host that never
# indexed anything has nothing to give: that is not a failure, and a container
# then searches by reading files.
tehas_sandbox_seed() { # <state> [<index dir>]
  local state=$1 src=${2:-$(tehas_index_dir)} f
  mkdir -p "$state/cache" || return 1
  [ -d "$src" ] || return 0
  for f in "$src"/*; do
    [ -e "$f" ] || continue
    case "$f" in */logs | *.db.stage.* | *.tmp) continue ;; esac
    cp -a "$f" "$state/cache/" || { rm -rf "$state/cache"; mkdir -p "$state/cache"; return 1; }
  done
}

# What a container is not allowed to do, so that an agent's code cannot give
# itself more of the host than it was given: every capability Docker grants by
# default is taken away, and a program cannot gain rights through its mode or
# file capabilities. The same options for the four containers tehas starts an
# agent in: a cycle, a planning turn, a manual session and the push check
# before a cycle. The contract is one array: tehas_sandbox_security_count prints
# how many words it prints, and tehas_sandbox_security_ok says whether the words
# it prints are the documented ones in order, so a guard asked before Docker
# names what drifted instead of counting somewhere else. host/docs/host.md
# documents the same options in a table, and test/work_caps_test.sh compares
# that table with what is printed here, so adding one option is an edit here, a
# row there, and nothing else.
#
# Only CHOWN and FOWNER are given back:
#
#   CHOWN    npm's install drops a package to the unprivileged user with
#            chown(2) and lchown(2), and Playwright's install of Chromium
#            copies a cache tree that way. Both run in a gate of the product.
#            Without it the gate fails on EPERM.
#   FOWNER   npm moves its staging directory onto its destination with rename,
#            and rename of a file one does not own is refused even when the
#            directory is writable. tar's unpack, cargo's install of a built
#            crate and a gate's own mv do the same.
#
# Nothing else in the list is named. In particular:
#
#   SETUID, SETGID   nothing a cycle runs becomes another user. The image has
#                    no sudo, the container runs as one uid and tehas gives it
#                    no setuid program to reach another. Playwright's Chromium
#                    runs its own sandbox only with a capability that would
#                    let it make a setuid helper, and it is run with
#                    --no-sandbox instead, which pi-rukas's browser tool
#                    passes. no-new-privileges does not break anything a cycle
#                    does: it stops a program gaining rights, and nothing in
#                    the image is meant to have any.
#   NET_RAW          the agents answer over HTTP with curl and fetch through
#                    git, npm and cargo, none of which opens a raw socket. It
#                    is off Docker's own default list too.
#   SYS_ADMIN        the one capability that matters for a container's escape,
#                    and the one Chromium's sandbox would want. Nothing here
#                    mounts anything: tehas makes every mount on the host
#                    before it starts a container.
#
# The root filesystem stays writable, unlike a preview's: a cycle's gate
# installs into its own tree and its caches, and a read-only root would need
# one --tmpfs per directory a tool writes in, which is a list nobody can pin.
# One --tmpfs is given all the same, for /tmp: not to hide the root but to
# bound it. The writable layer it replaces sits on the host's root disk with no
# limit of its own; on 2026-10-20 one cycle filled the host's disk with a log
# under /tmp and four others died with it. Why the bit stays exec, what the
# container's own TMPDIR is, and what the size means are in the manual, at
# "What a container may not do" in host/docs/host.md. The size is half the
# container's --memory, TEHAS_SLICE_CEILING_MB from tehas_slice_ok: tmpfs pages
# count against that limit and cannot be reclaimed, so a /tmp as large as the
# limit ends in OOM kills of the agents (#761). At half, a full /tmp is ENOSPC
# for its writer and the other half is left to the processes. Without a
# ceiling nothing is printed, and tehas_sandbox_security_take refuses.
tehas_sandbox_security() {
  [[ ${TEHAS_SLICE_CEILING_MB:-} =~ ^[1-9][0-9]{1,8}$ ]] || return 1
  printf '%s\0' --cap-drop ALL --cap-add CHOWN --cap-add FOWNER \
    --security-opt no-new-privileges:true \
    --tmpfs "/tmp:rw,exec,nosuid,nodev,size=$((TEHAS_SLICE_CEILING_MB / 2))m,mode=1777"
}

# tehas_sandbox_security_take <command> <what>: the options in the caller's
# sec, or 1 with a line on stderr naming what does not start. Asked before
# Docker is called, so a list that drifted from the documented one starts
# nothing.
tehas_sandbox_security_take() {
  mapfile -d '' -t sec < <(tehas_sandbox_security)
  [ "${#sec[@]}" -gt 0 ] && [ "${#sec[@]}" -eq "$(tehas_sandbox_security_count)" ] &&
    tehas_sandbox_security_ok && return 0
  echo "$1: tehas_sandbox_security gave ${#sec[@]} words, wants $(tehas_sandbox_security_count): not starting $2" >&2
  return 1
}

# How many words the function above prints, in the caller's own words. It is
# asked of the function, so the number stands in one place.
tehas_sandbox_security_count() {
  local sec=()
  mapfile -d '' -t sec < <(tehas_sandbox_security)
  printf '%s\n' "${#sec[@]}"
}

# True when what tehas_sandbox_security prints is the documented list below, in
# this order and as separate arguments (docker also accepts --tmpfs=<value>,
# which is not what is printed and would be a shorter list). A caller that
# cannot ask this has only a length to go on; the tests ask it of the function
# itself (test/work_caps_test.sh).
tehas_sandbox_security_ok() {
  local documented sec i
  [[ ${TEHAS_SLICE_CEILING_MB:-} =~ ^[1-9][0-9]{1,8}$ ]] || return 1
  documented=(
    --cap-drop ALL
    --cap-add CHOWN
    --cap-add FOWNER
    --security-opt no-new-privileges:true
    --tmpfs "/tmp:rw,exec,nosuid,nodev,size=$((TEHAS_SLICE_CEILING_MB / 2))m,mode=1777"
  )
  sec=()
  mapfile -d '' -t sec < <(tehas_sandbox_security)
  [ "${#sec[@]}" -eq "${#documented[@]}" ] || return 1
  for i in "${!documented[@]}"; do
    [ "${sec[i]}" = "${documented[i]}" ] || return 1
  done
}

# Names the sandbox cannot resolve by itself, from PI_ENSEMBLE_HOST_ALIASES.
tehas_sandbox_hosts() {
  local pairs=() p
  IFS=',' read -ra pairs <<<"${PI_ENSEMBLE_HOST_ALIASES:-}"
  for p in "${pairs[@]}"; do
    if [ -n "$p" ]; then printf '%s\0' "--add-host=$p"; fi
  done
}

# Make <tree>: a clone at the tip the product checkout <src> last fetched for
# <branch>, with the product's address as origin. Nothing is fetched from the
# network. The host runs git in <tree> here and only here: once a container has
# written a tree, anything in it, hooks and git configuration included, may be
# an agent's. The caller decides whether <tree> may be made.
tehas_sandbox_clone() { # <src> <tree> <branch>
  local src=$1 tree=$2 branch=$3 base url
  base=$(git -C "$src" rev-parse -q --verify "refs/remotes/origin/$branch^{commit}") ||
    { tehas_err "$src has no origin/$branch"; return 1; }
  url=$(git -C "$src" config --get remote.origin.url) || return 1
  git init -q -b "$branch" "$tree" &&
    git -C "$tree" remote add origin "$url" &&
    timeout -k 10 300 git -C "$tree" fetch -q --no-tags "$src" '+refs/remotes/origin/*:refs/remotes/origin/*' &&
    git -C "$tree" symbolic-ref refs/remotes/origin/HEAD "refs/remotes/origin/$branch" &&
    git -C "$tree" checkout -q -B "$branch" "$base" &&
    git -C "$tree" branch -q --set-upstream-to="origin/$branch" "$branch"
}

# The images that something holds, out of the tags it is given: the id of every
# image a container on this host -- running or stopped -- was made from, and
# the id of every image a uid tag names. Asked with one docker call per side;
# a docker that answers neither is a docker that holds nothing, and its caller
# removes nothing. Given the tags as "<tag> <id>" per line, as `docker image ls
# --no-trunc --format '{{.Tag}} {{.ID}}'` lists them; the uid tags of another
# uid count with the host's own. Both the sandbox build and the cleaner ask
# here, so the two cannot drift apart on what is safe to remove. The listing is
# host-wide on purpose: an image a container of any other program on this host
# was made from is as held as one a cycle's was, and the step that takes tags
# away is not the place to guess which account started a container.
#
# A nonzero status is the whole of the proof: docker answered nothing, stdout
# says nothing, and the caller removes nothing.
images_held() { # <the image's tags, "<tag> <id>" per line>
  local ids held
  ids=$(docker ps -aq --no-trunc) || return 1
  if [ -n "$ids" ]; then
    # shellcheck disable=SC2086  # one argument per container id
    # Buffer, then print: an inspect that answered some ids and failed would
    # otherwise leave a half a held-set on stdout beside a nonzero status.
    held=$(docker inspect --format '{{.Image}}' $ids) || return 1
    [ -n "$held" ] && printf '%s\n' "$held"
  fi
  awk '$1 ~ /^uid/ { print $2 }' <<<"$1"
}

# The name of the image a cycle runs in, without its tag: the name
# host/sandbox/build.sh tags what it builds with and the name `tehasbot clean`
# lists the dated tags under. The build tags this name, so it is the one name
# the cleaner can look dated tags up under; an instance that points
# PI_ENSEMBLE_IMAGE at another repository keeps the build's tag for itself and
# the dated tags of the build's name go uncleaned. The tag of the image cycles
# run (uid<uid>, or what PI_ENSEMBLE_IMAGE names) is cut from the last slash
# on, so a repository that carries a registry and its port keeps it: only the
# part after the last slash may hold a tag.
tehas_sandbox_image_repo() {
  local name tail repo
  name=${PI_ENSEMBLE_IMAGE:-tehas/pi-rukas:uid$(id -u)}
  tail=${name##*/}
  [[ $tail =~ ^[A-Za-z0-9][A-Za-z0-9._-]*(:[A-Za-z0-9][A-Za-z0-9._-]*)?$ ]] ||
    { tehas_err "PI_ENSEMBLE_IMAGE=$name names no image docker can list"; return 1; }
  # The name before the last slash, plus the last part with its tag off. A name
  # with no slash is one part, so the prefix is empty and the cut is the whole
  # answer; a name with one has its prefix ending in the slash, which must stay.
  repo=${name%/*}
  [ "$repo" = "$name" ] && repo= || repo=$repo/
  printf '%s\n' "$repo${tail%:*}"
}

# The base image of the sandbox build, as one name: the image upstream
# publishes for the pi-rukas commit the host follows, under ghcr.io, tagged by
# the twelve leading characters of that commit -- or the digest
# PI_RUKAS_IMAGE_DIGEST names, which is what a release pins its base to. The
# three pins are the settings host/sandbox/build.sh builds on, read the same
# way: the instance's line over the line of versions.env, which is the one file
# the settings library does not read and so the one file read here as text. The
# cleaner removes the base by this name and the build pulls it by it, so the two
# cannot name different images. Needs the settings loaded, and lib/rukas.sh
# sourced beside this file, which answers which commit is followed.
#
# A followed release or branch asks GitHub, and that call is bounded: the
# cleaner runs unattended on a timer and may not wait on a network that does
# not answer. timeout runs its command outside this shell, where a sourced
# function cannot be found, so the bounded lookup is asked of a child shell that
# sources the library itself and takes its pins from the environment.
#
# Returns 1 when the host's pins name no base, or the lookup failed: the caller
# says what that means for it.
tehas_base_image() {
  local envs follow digest repo commit
  [ -n "${TEHAS_HOME:-}" ] || return 1
  envs=$TEHAS_HOME/versions.env
  [ -r "$envs" ] || return 1
  # Read as text, the way lib/update-release.sh reads a pin from this file: a
  # value of another shape names an image that cannot exist, and is refused.
  # Each falls back to the environment only when the file has no line for it,
  # so a line of the file is never hidden by an unrelated name in the run's
  # environment.
  local line
  line=$(sed -n 's/^PI_RUKAS_FOLLOW=\([A-Za-z0-9:.\/-]*\)$/\1/p' "$envs" | tail -n 1)
  follow=${line-${PI_RUKAS_FOLLOW:-}}
  line=$(sed -n 's/^PI_RUKAS_IMAGE_DIGEST=\([a-z0-9:]\{71\}\)$/\1/p' "$envs" | tail -n 1)
  digest=${line-${PI_RUKAS_IMAGE_DIGEST:-}}
  line=$(sed -n 's/^PI_RUKAS_REPO=\([A-Za-z0-9:.\/-]*\)$/\1/p' "$envs" | tail -n 1)
  repo=${line-${PI_RUKAS_REPO:-}}
  [[ $repo == https://github.com/*/* ]] || return 1
  [[ $digest == '' || $digest == sha256:* ]] || return 1
  if [ -z "$follow" ] && [ -z "$digest" ]; then return 1; fi
  if [ -z "$digest" ]; then
    # lib/rukas.sh answers which commit is followed. The commands that ask this
    # function source it beside this file; one that forgot gets the named error
    # rather than a base it was never given.
    if ! declare -f tehas_rukas_target >/dev/null; then
      tehas_err "tehas_base_image needs lib/rukas.sh sourced beside lib/sandbox.sh"
      return 1
    fi
    if [[ $follow == commit:* ]]; then
      # Answered from the setting alone: asking upstream again could only move
      # the commit a release pinned.
      commit=$(PI_RUKAS_FOLLOW=$follow PI_RUKAS_REPO=$repo tehas_rukas_target) || return 1
    else
      # Bounded: the cleaner runs unattended on a timer and may not wait on a
      # network that does not answer. The name given to timeout is run outside
      # this shell, where a sourced function cannot be found, so bash is the
      # command and it reads the library and asks the lookup under the bound.
      # A host with no library beside the cleaner answers nothing here, which is
      # an unknown and not a base to remove.
      commit=$(timeout -k 5 30 bash -c \
        ". \"\${TEHAS_LIB_DIR:-$(dirname "${BASH_SOURCE[0]}")}/rukas.sh\"; PI_RUKAS_FOLLOW='$follow' PI_RUKAS_REPO='$repo' tehas_rukas_target") ||
        return 1
    fi
    commit=${commit%% *}
    # A lookup that answered no commit names no image: a name built from
    # nothing would read in the log as an image that was never pulled, which is
    # the answer an operator must never be given for an unknown.
    [[ $commit =~ ^[0-9a-f]{40}$ ]] || return 1
  fi
  repo=${repo#https://github.com/}
  repo=${repo%.git}
  repo=$(tr '[:upper:]' '[:lower:]' <<<"$repo")
  if [ -n "$digest" ]; then
    printf 'ghcr.io/%s@%s\n' "$repo" "$digest"
  else
    printf 'ghcr.io/%s:sha-%s\n' "$repo" "${commit:0:12}"
  fi
}
