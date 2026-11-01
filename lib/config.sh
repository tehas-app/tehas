# shellcheck shell=bash
# Tehas instance settings. Sourced by every tehas command and by env.sh.
#
# An instance is one host serving one product repository. Its settings are in
# <instance dir>/instance.env, a shell file of KEY=value lines written by the
# operator. It is sourced as written: treat it like a script, keep it writable
# by the tehas user only, and put no secrets in it.
#
#   tehas_load           find the instance, load and check its settings.
#                        Returns 78 with one line on stderr when it cannot. It
#                        never exits, because login shells source it too.
#   tehas_require NAME.. exit 78 unless each named setting is set and valid.
#   tehas_require_instance_settings
#                        exit 78 unless every setting an instance has to set for
#                        itself is set and valid. One list, so no command that
#                        checks it before changing anything has its own copy.
#   tehas_checkout       check the product checkout is a clone of TEHAS_REPO;
#                        returns 78 if not.
#   tehas_branch         print the branch work lands on; returns 78 if unknown.
#   tehas_models         print "provider model" for every model the agents use.
#
# A command starts with:
#   here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
#   . "$here/../lib/config.sh"
#   tehas_load || exit 78
#   tehas_require TEHAS_REPO ...   (or tehas_require_instance_settings)
#
# Rules:
#   - instance.env is authoritative. Settings of the same name in the
#     environment are dropped; only TEHAS_HOME, TEHAS_INSTANCE_DIR and the test
#     hooks TEHAS_WORK_CMD, TEHAS_STATUS_CMD, TEHAS_CONSOLE, TEHAS_CONSOLE_CMD, TEHAS_INDEX_CMD, TEHAS_PLAN_CMD,
#     TEHAS_PLAN_START_CMD, TEHAS_INDEX_START_CMD, TEHAS_INDEX_LOCK_WAIT, TEHAS_DEV_LOCK_WAIT,
#     TEHAS_CYCLE_START_CMD, TEHAS_CYCLE_STOP_CMD, TEHAS_CYCLE_START_WAIT,
#     TEHAS_CGROUP_ROOT, TEHAS_SLICE_WAIT, TEHAS_DISK_WAIT, TEHAS_PREFLIGHT_WAIT, TEHAS_STOP_WAIT, TEHAS_FETCH_WAIT, TEHAS_CHECK_WAIT,
#     TEHAS_CONSOLE_WAIT, TEHAS_WORK_GRACE, TEHAS_CAPS_WAIT, TEHAS_TEXT_WAIT,
#     TEHAS_BUILD_TIMEOUT, TEHAS_CYCLE_RETRY_WAIT, TEHAS_CYCLE_STOP_GH_WAIT, TEHAS_TIDY_CMD, TEHAS_DISPATCH_CMD, TEHAS_HEALTH_CMD,
#     TEHAS_TIDY_WAIT, TEHAS_ANALYTICS_WAIT, TEHAS_GITHUB_API,
#     TEHAS_GITHUB_APP_KEY, TEHAS_GITHUB_RENEW_EVERY, TEHAS_CONSOLE and
#     TEHAS_PROC_DIR are kept, so are the
#     two words a cycle hands down, TEHAS_CYCLE_FROM (tehas-cycle@.service)
#     and TEHAS_CYCLE_ATTEMPT (tehasbot cycle to tehas work), and from
#     ~/.config/tehas/env TEHAS_ENERGY, TEHAS_ENERGY_URL and the model
#     endpoints' tokens TEHAS_MODEL_KEY_*. instance.env that sets a token is
#     refused: it is in a repository.
#   - PI_RUKAS_FOLLOW may be set in instance.env, over the line of
#     versions.env, and is checked whenever it is set: release, branch:<name>
#     or commit:<40 hex>.
#   - Loading runs no git command, makes no network call and writes nothing.
#     TEHAS_PLAN_TIMEOUT_MAX and TEHAS_CHILD_TIMEOUT_MAX, the ceiling of a planning
#     turn and the bound of one child, are local to tehas_load.
#   - TEHAS_IMPROVE_REPO switches the improvement loop on; unset, there is
#     none. TEHAS_IMPROVE_RUKAS_REPO needs it and not TEHAS_IMPROVE_RUKAS_SKIP=1.
#     Neither is TEHAS_REPO: a person triages findings before they are tickets.
#     TEHAS_REVIEW is the review loop's switch: 0, the default, and tehasbot
#     review exits at once and calls nothing.
#   - TEHAS_GITHUB_APP_ID, TEHAS_GITHUB_APP_INSTALLATION and TEHAS_BOT_LOGIN
#     name the GitHub App the factory acts as (lib/github-app.sh), all three or
#     none. With none, it acts with GH_TOKEN from ~/.config/tehas/env.
#   - TEHAS_MILLWRIGHT_LOGIN names the millwright: an account whose tehas:ready
#     and planning answers the dispatcher takes within a narrower grant than a
#     person's (lib/millwright.sh). Unset means there is none. An App's
#     <slug>[bot] (REST) and <slug> (GraphQL) are one account, never the bot nor
#     an allowed labeller. A TEHAS_FIXER_* name stops the load with 78.
#   - TEHAS_LOW_GATE names a command the dispatcher runs before it starts the
#     lowest issues (libexec/dispatch, rule 7). Unset is valid and means there
#     is no gate, and the dispatcher behaves as it did before the setting. It is
#     asked with nothing of the poll's environment but PATH, and it may not name
#     a file under TEHAS_HOME or the instance directory: those are the trees a
#     merged pull request writes, and the gate runs as the tehas user.
#     TEHAS_LOW_GATE_WAIT is how many seconds that command gets, at most 600:
#     the poll holds its own lock while it waits, so a longer bound only delays
#     every later poll.
#   - Nothing falls back to a project: a setting with no generic default stays
#     unset until tehas_require asks for it.

tehas_err() { echo "tehas: $*" >&2; }

# Where the instance's settings are: the environment, else the one-line file
# ~/.config/tehas/instance-dir, else ~/.config/tehas. _tehas_instance_dir puts
# it in the caller's $dir instead, without a subshell.
tehas_instance_dir() { local dir; _tehas_instance_dir; printf '%s\n' "$dir"; }
_tehas_instance_dir() {
  local pointer=$HOME/.config/tehas/instance-dir
  dir=${TEHAS_INSTANCE_DIR:-}
  [ -n "$dir" ] || [ ! -r "$pointer" ] || read -r dir <"$pointer" || true
  dir=${dir:-$HOME/.config/tehas}
}

_tehas_host_cpus() { nproc 2>/dev/null || echo 1; }

# A Docker memory value (512m, 8g, a bare number of bytes) in MiB, in the
# caller's $mb; fails on anything else, with $mb empty.
_tehas_mem_mb() {
  local v=$1 n
  mb=""
  case "$v" in
    '' | *[!0-9bkmgBKMG]* | [!0-9]*) return 1 ;;
  esac
  n=${v%[bkmgBKMG]}
  case "$n" in '' | *[!0-9]*) return 1 ;; esac
  case "$v" in
    *[gG]) mb=$((n * 1024)) ;;
    *[mM]) mb=$n ;;
    *[kK]) mb=$((n / 1024)) ;;
    *) mb=$((n / 1048576)) ;;
  esac
}

# A count of the store's own grammar (verbs::count): at most nine digits, and
# the only one beginning with 0 is 0 itself. A longer number makes [ -gt ] fail,
# and a failed test reads as a passed one.
_tehas_is_count() { case "$1" in '' | *[!0-9]* | 0?* | ??????????*) return 1 ;; esac; }

# The directory names tehasbot clean removes when the instance names none: the
# build output of the toolchains tehas hosts, none of which a product keeps in
# source. The one place they are written: instances/example and the manual
# quote them, and test/config_test.sh pins the copies against this.
_tehas_clean_dirs_default='target node_modules __pycache__ .venv .pytest_cache .mypy_cache .next .turbo .gradle'

# Print why a value is not acceptable for a setting; return 1 if it is not.
_tehas_valid() {
  local name=$1 v=$2 mb base clean_dir part
  local -a clean_dirs
  case "$name" in
    TEHAS_REPO | TEHAS_IMPROVE_REPO | TEHAS_IMPROVE_RUKAS_REPO | TEHAS_RELEASE_REPO | TEHAS_REVIEW_REPO)
      [[ $v =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || { echo "expected owner/name"; return 1; }
      ;;
    # The counts refuse `auto` by name until cycles can use it.
    TEHAS_CONTROL_ISSUE | TEHAS_MAX_AWAITING | TEHAS_IMPROVE_MAX_OPEN | TEHAS_MILLWRIGHT_MAX_RETRIES | TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS | TEHAS_MILLWRIGHT_MAX_FEEDS | TEHAS_REFRESH_MAX | TEHAS_WORK_TIMEOUT | TEHAS_WORK_STALL | TEHAS_CONCURRENCY | TEHAS_TICKET_CONCURRENCY | TEHAS_SLICE_CONTAINERS | \
      TEHAS_PLAN_MAX_TURNS | TEHAS_PLAN_TIMEOUT | TEHAS_VERIFY_TIMEOUT | TEHAS_CHILD_TIMEOUT | TEHAS_BUILD_TIMEOUT | TEHAS_SIZE_MAX_FILES)
      case "$name:$v" in TEHAS_TICKET_CONCURRENCY:auto | TEHAS_SLICE_CONTAINERS:auto) echo 'expected a whole number (auto is not usable yet)'; return 1 ;; esac
      [[ $name != TEHAS_SLICE_CONTAINERS || $v =~ ^([2-9]|1[0-6])$ ]] || { echo "expected a whole number from 2 to 16"; return 1; }
      [[ $v =~ ^[1-9][0-9]*$ ]] || { echo 'expected a positive whole number'; return 1; }
      _tehas_is_count "$v" || { echo "expected at most 9 digits"; return 1; }
      ;;
    TEHAS_LOW_GATE_WAIT)
      # The dispatcher waits for the gate while it holds state/dispatch.lock, so
      # these seconds delay every later poll too: that is the ceiling. The length
      # is checked before the value, so a number no comparison can hold is refused
      # by the length and not by a failing comparison.
      [[ $v =~ ^[1-9][0-9]*$ ]] || { echo "expected a positive whole number"; return 1; }
      [ "${#v}" -le 3 ] && [ "$v" -le 600 ] || { echo "expected at most 600 seconds"; return 1; }
      ;;
    TEHAS_ALLOWED_LABELERS)
      # Exactly one space between logins and none around them: the dispatcher
      # matches " login " inside " list ", and a stray space would make an
      # empty name match.
      [[ $v =~ ^[A-Za-z0-9][A-Za-z0-9-]*(\ [A-Za-z0-9][A-Za-z0-9-]*)*$ ]] ||
        { echo "expected GitHub logins separated by single spaces"; return 1; }
      ;;
    TEHAS_MILLWRIGHT_LOGIN)
      [[ $v =~ ^[A-Za-z0-9][A-Za-z0-9-]*(\[bot\])?$ ]] || { echo "expected one GitHub login, such as tehas-millwright or tehas-millwright[bot]"; return 1; }
      ;;
    TEHAS_GITHUB_APP_ID | TEHAS_GITHUB_APP_INSTALLATION)
      [[ $v =~ ^[1-9][0-9]{0,11}$ ]] || { echo "expected the ID GitHub shows, a whole number"; return 1; }
      ;;
    TEHAS_BOT_LOGIN)
      [[ $v =~ ^[A-Za-z0-9][A-Za-z0-9-]*(\[bot\])?$ ]] || { echo "expected one GitHub login, such as tehas-app-bot[bot]"; return 1; }
      ;;
    TEHAS_ANALYTICS)
      case "$v" in on | off) ;; *) echo "expected on or off"; return 1 ;; esac
      ;;
    TEHAS_UPDATE_FROM)
      case "$v" in git | release) ;; *) echo "expected git or release"; return 1 ;; esac
      ;;
    TEHAS_DISPATCHER | TEHAS_CYCLE_REPORT)
      case "$v" in bash | rust) ;; *) echo "expected bash or rust"; return 1 ;; esac
      ;;
    # The look of every page: host/fun/theme-<name>.css. The servers write the
    # name into a path, so only a theme that ships is let through.
    TEHAS_THEME)
      case "$v" in pixel | paper | beesknees | beesknees26) ;; *) echo "expected pixel, paper, beesknees or beesknees26"; return 1 ;; esac
      ;;
    # What the front page is: host/fun/index.html for the floor, host/fun/<name>.html
    # for another. The docs server picks the page by this name.
    TEHAS_LAYOUT)
      case "$v" in floor | business) ;; *) echo "expected floor or business"; return 1 ;; esac
      ;;
    TEHAS_INDEXING | TEHAS_ANALYTICS_COOKIE | TEHAS_WORK_IMAGES | TEHAS_WORK_DESCRIBE | TEHAS_WORK_FILES | TEHAS_WORK_COMMENTS | TEHAS_WORK_ZIPS | TEHAS_TOP | TEHAS_REFRESH_CONFLICTS | TEHAS_EASY_MERGE | TEHAS_IMPROVE_RUKAS_SKIP | TEHAS_REVIEW | TEHAS_MILLWRIGHT_FEED)
      case "$v" in 0 | 1) ;; *) echo "expected 0 or 1"; return 1 ;; esac
      ;;
    TEHAS_ANALYTICS_SRC)
      # Both end up in an HTML attribute of every page, so no quote, space or
      # angle bracket may get through.
      [[ $v =~ ^https://[A-Za-z0-9.-]+(:[0-9]+)?/[A-Za-z0-9._/-]*$ ]] ||
        { echo "expected the https address of a script"; return 1; }
      ;;
    TEHAS_ANALYTICS_SITE)
      [[ $v =~ ^[A-Za-z0-9._-]+$ ]] || { echo "expected one word of letters, digits, dots and dashes"; return 1; }
      ;;
    TEHAS_CYCLE_RESERVE | TEHAS_CONTAINER_MEMORY)
      # The shape lib/slice.sh reads at every start: no leading zero, at most
      # twelve digits of bytes or nine with a unit, and a size Docker takes: it
      # refuses a memory value under 6m.
      [[ $v =~ ^([1-9][0-9]{0,11}[bB]?|[1-9][0-9]{0,8}[kmgKMG])$ ]] &&
        _tehas_mem_mb "$v" && [ "$mb" -ge 6 ] ||
        { echo "expected a size of at least 6m, such as 512m or 2g"; return 1; }
      ;;
    TEHAS_HOST_ADDR)
      [[ $v =~ ^[A-Za-z0-9.:-]+$ ]] || { echo "expected an address or host name"; return 1; }
      ;;
    TEHAS_PROTOLAB_URL)
      # The dashboard puts a ticket's number straight after it.
      [[ $v =~ ^https?://[A-Za-z0-9.-]+(:[0-9]+)?$ ]] ||
        { echo "expected an address such as https://lab.example, with no path and no slash at the end"; return 1; }
      ;;
    TEHAS_DOMAIN)
      # A name is made by putting uat-<n>. or lab-<n>. in front of it.
      [[ $v =~ ^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+$ ]] ||
        { echo "expected a domain such as example.org, in lower case, with no scheme and no path"; return 1; }
      ;;
    TEHAS_PREVIEWS)
      [[ $v =~ ^[01]$ ]] || { echo "expected 0 or 1"; return 1; }
      ;;
    TEHAS_PREVIEW_MAX)
      [[ $v =~ ^[1-9][0-9]?$ ]] || { echo "expected a number from 1 to 99"; return 1; }
      ;;
    TEHAS_PREVIEW_MIN_FREE | TEHAS_CYCLE_MIN_FREE | TEHAS_CLEAN_MIN_FREE)
      _tehas_mem_mb "$v" && [ "$mb" -gt 0 ] ||
        { echo "expected a size such as 512m or 5g"; return 1; }
      ;;
    TEHAS_RUKAS_WORKAROUNDS)
      [[ $v =~ ^(PI_ENSEMBLE|TEHAS)_[A-Z0-9_]+@[0-9]{1,6}(\.[0-9]{1,6}){1,3}(\ (PI_ENSEMBLE|TEHAS)_[A-Z0-9_]+@[0-9]{1,6}(\.[0-9]{1,6}){1,3})*$ ]] ||
        { echo "expected NAME@VERSION entries separated by single spaces, such as PI_ENSEMBLE_COMMENT_RETENTION@0.12.80"; return 1; }
      ;;
    # What tehasbot clean removes, and when: the hours are docker's `until` and
    # find's `-newermt`, the names are each one directory name, and the minutes
    # between two runs of tehasbot refresh are at most a day.
    TEHAS_REFRESH_SLOTS)
      [[ $v =~ ^[0-8]$ ]] || { echo "expected a whole number from 0 to 8"; return 1; }
      ;;
    TEHAS_REFRESH_EVERY)
      [[ $v =~ ^[1-9][0-9]{0,3}$ ]] && [ "$v" -le 1440 ] ||
        { echo "expected a whole number of minutes from 1 to 1440"; return 1; }
      ;;
    TEHAS_CLEAN_AFTER)
      [[ $v =~ ^[1-9][0-9]{0,4}$ ]] || { echo "expected a whole number of hours of at least 1"; return 1; }
      ;;
    # An idle poll's budget for `store poll-idle --budget`: the three figures in
    # this order, any left out; another order would bound the wrong figure.
    TEHAS_POLL_BUDGET)
      [[ $v =~ ^(cpu=[0-9]{1,9}(,wall=[0-9]{1,9}(,writes=[0-9]{1,9})?)?|wall=[0-9]{1,9}(,writes=[0-9]{1,9})?|writes=[0-9]{1,9})$ ]] ||
        { echo "expected cpu=<s>,wall=<s>,writes=<n>, any part left out"; return 1; }
      # Each figure as the store's count reads it (no leading 0), so the load
      # stops with 78; the local IFS keeps the instance file's own away.
      local IFS=$' \t\n,'
      for part in ${v//,/ }; do
        _tehas_is_count "${part#*=}" ||
          { echo "expected cpu=<s>,wall=<s>,writes=<n>, and no leading 0"; return 1; }
      done
      ;;
    # What tehas work measures of an approved body, and no further: the two
    # bounds of console/core/src/size.rs, each holding what the other leaves unbounded (a
    # body of one huge line, a body of many short ones). Both must be numbers.
    TEHAS_SIZE_MAX_BYTES | TEHAS_SIZE_MAX_LINES)
      [[ $v =~ ^[1-9][0-9]{0,7}$ ]] || { echo "expected a whole number of at least 1"; return 1; }
      ;;
    TEHAS_SCRATCH_DIR)
      [[ $v =~ ^(/[A-Za-z0-9._-]+)+$ && $v != *..* ]] || { echo "expected an absolute path such as /mnt/scratch"; return 1; }
      ;;
    TEHAS_CLEAN_DIRS)
      # Names, not paths: the cleaner matches them with find -name, so the pattern
      # below admits no slash, no bare dot and none of the dot-names a workspace
      # keeps, in any spelling.
      [[ $v =~ ^[A-Za-z0-9._-]+(\ [A-Za-z0-9._-]+)*$ ]] ||
        { echo "expected directory names of letters, digits and ._-, separated by single spaces"; return 1; }
      # Split on the single spaces the pattern above allows, whatever IFS was set.
      IFS=' ' read -r -a clean_dirs <<<"$v"
      for clean_dir in "${clean_dirs[@]}"; do
        case "$clean_dir" in
          . | .. | .[gG][iI][tT] | .[pP][iI] | .[pP][iI].*)
            echo "refuses $clean_dir: that is not build output"
            return 1
            ;;
        esac
      done
      ;;
    PI_RUKAS_FOLLOW)
      case "$v" in
        release) ;;
        release:*)
          [[ ${v#release:} =~ ^[0-9]{1,6}(\.[0-9]{1,6})?$ ]] ||
            { echo "expected release:<line>, a major version or major.minor such as 0.14"; return 1; }
          ;;
        branch:*)
          [[ ${v#branch:} =~ ^[A-Za-z0-9._/-]+$ ]] ||
            { echo "expected branch:<name>, a name of letters, digits and ._/-"; return 1; }
          ;;
        commit:*)
          # The whole commit: an abbreviation could name another one later.
          [[ ${v#commit:} =~ ^[0-9a-f]{40}$ ]] ||
            { echo "expected commit: and 40 lower-case hex digits"; return 1; }
          ;;
        *) echo "expected release, release:<line>, branch:<name> or commit:<40 hex>"; return 1 ;;
      esac
      ;;
    TEHAS_BRANCH_CHECK)
      # A check run's name goes into a URL and into the log.
      [[ $v =~ ^[A-Za-z0-9._()/-]+(\ [A-Za-z0-9._()/-]+)*$ ]] ||
        { echo "expected the name of a check, words of letters, digits and ._()/- separated by single spaces"; return 1; }
      ;;
    TEHAS_LOW_GATE)
      # Run by the dispatcher as one word, with no shell between. Every segment
      # is named: //, /./ and /../ would let one file be named another way,
      # which is how a check on where a path sits is defeated.
      [[ $v =~ ^/[A-Za-z0-9._-]+(/[A-Za-z0-9._-]+)*$ ]] ||
        { echo "expected an absolute path of /, letters, digits and ._-, with no empty or dot segment"; return 1; }
      case "/$v/" in
        *"/./"* | *"/../"*)
          echo "expected an absolute path of /, letters, digits and ._-, with no empty or dot segment"
          return 1
          ;;
      esac
      # The gate is run as the tehas user by every poll. A merged pull request
      # writes the trees under the host's tehas home and the instance directory,
      # and install.sh puts a new bin/ or libexec/ file there from the same
      # change, so a gate named by an instance setting must be a program no pull
      # request can put in place.
      for base in "${TEHAS_HOME:-}" "${TEHAS_INSTANCE_DIR:-}" "${TEHAS_WORK_REPO:-}"; do
        base=${base%/}
        [ -n "$base" ] || continue
        case "$v/" in
          "$base/"*)
            echo "expected a command outside $base, which a merged pull request can change"
            return 1
            ;;
        esac
      done
      ;;
    PI_ENSEMBLE_SUBAGENT_MODEL | PI_ENSEMBLE_SUBAGENT_PROVIDER | PI_ENSEMBLE_MODEL_* | PI_ENSEMBLE_PROVIDER_* | TEHAS_BRANCH)
      [[ $v =~ ^[^[:space:]]+$ ]] || { echo "expected one word"; return 1; }
      ;;
  esac
  return 0
}

# _tehas_why <name> <value>: 0 when _tehas_valid accepts it, else 1 with its
# reason in the caller's $reason. The check runs in this shell and the reason
# is asked for only when there is one: every load checks some forty settings.
_tehas_why() {
  _tehas_valid "$1" "$2" >/dev/null && return 0
  reason=$(_tehas_valid "$1" "$2")
  return 1
}

# The roles an instance may give a model of their own, as the suffix of
# PI_ENSEMBLE_MODEL_<ROLE> and PI_ENSEMBLE_PROVIDER_<ROLE>, checked when the
# settings load, before any image is asked. They are the members pi-rukas
# v0.12.80 and later list for that family in --capabilities. tehas work checks
# a cycle's names against the members its image lists, so a role pi-rukas
# drops is refused there; one it adds is usable once it is added here. All six
# lens reviewers are one role, CODE_REVIEW_SPECIALIST. The project manager is
# the session: its model is the product's .pi/settings.json.
_tehas_roles=(DEVELOPER OPS EXPLORE ADVERSARIAL_DEVELOPER CODE_REVIEW_SPECIALIST)

# Check the per-role model settings and give each role model a provider. A name
# pi-rukas does not read would reach the container and be ignored, and that role
# would run on PI_ENSEMBLE_SUBAGENT_MODEL without a word, so it is refused. So is
# a provider without its model: pi-rukas ignores it. A role model with no provider
# of its own gets PI_ENSEMBLE_SUBAGENT_PROVIDER, because Pi cannot find a custom
# provider's model by its id alone. An empty line is not set. Returns 78 with one
# line on stderr.
_tehas_role_models() { # <instance.env>
  local file=$1 v m p role reason roles
  roles=" ${_tehas_roles[*]} "
  for v in ${!PI_ENSEMBLE_MODEL_@} ${!PI_ENSEMBLE_PROVIDER_@}; do
    case "$v" in
      PI_ENSEMBLE_MODEL_*) role=${v#PI_ENSEMBLE_MODEL_} ;;
      PI_ENSEMBLE_PROVIDER_*) role=${v#PI_ENSEMBLE_PROVIDER_} ;;
      *) continue ;;
    esac
    case "$roles" in
      *" $role "*) ;;
      *)
        tehas_err "$v names no role pi-rukas gives a model to; the roles are$roles (see $file)"
        return 78 ;;
    esac
    # An empty line is not a setting: the name goes, so no container is given it.
    [ -n "${!v}" ] || { unset "$v"; continue; }
    _tehas_why "$v" "${!v}" || { tehas_err "$v is not valid: $reason (see $file)"; return 78; }
  done
  for role in $roles; do
    m=PI_ENSEMBLE_MODEL_$role p=PI_ENSEMBLE_PROVIDER_$role
    if [ -z "${!m:-}" ] && [ -n "${!p:-}" ]; then
      tehas_err "$p is set and $m is not: pi-rukas ignores a provider without its model (see $file)"
      return 78
    fi
    [ -z "${!m:-}" ] || [ -n "${!p:-}" ] || [ -z "${PI_ENSEMBLE_SUBAGENT_PROVIDER:-}" ] ||
      export "$p=$PI_ENSEMBLE_SUBAGENT_PROVIDER"
  done
  return 0
}

# The names an earlier version read are refused, not ignored: a line that does
# nothing in an instance file is a mistake the operator sees (TEHAS_FIXER_* were
# renamed TEHAS_MILLWRIGHT_* in v0.14.0 #676, unread since #799). The list's
# order picks which of two retired lines is reported. A name's fate is its own
# arm, and the last arm speaks for any name without one, so the list and the
# arms cannot drift apart into a refusal with no message.
_tehas_retired_names() { # <instance.env>
  local file=$1 v reason
  for v in TEHAS_FIXER_LOGIN TEHAS_FIXER_MAX_RETRIES TEHAS_FIXER_MAX_PLAN_ANSWERS TEHAS_CLONES TEHAS_WORK_CPUS TEHAS_WORK_MEMORY; do
    [ -n "${!v+x}" ] || continue
    # The off value of the one name that took one: an earlier version read 0 and
    # a blank line alike as no clones, which is what this version does, so the
    # line stays loadable. The name is unset with it, so it never reaches a
    # cycle, and the sweep in tehas_load took the host's TEHAS_* names before
    # instance.env was read, so this is the file's own line.
    if [ "$v" = TEHAS_CLONES ] && { [ "${!v}" = 0 ] || [ -z "${!v}" ]; }; then
      unset "$v"
      continue
    fi
    case "$v" in
      TEHAS_FIXER_*) reason="is no longer read: rename it TEHAS_MILLWRIGHT_${v#TEHAS_FIXER_}" ;;
      TEHAS_CLONES) reason="is set and this version of tehas has no database clones" ;;
      TEHAS_WORK_CPUS) reason="is set and this version of tehas has no CPU budget to set: the limit around all containers is /etc/systemd/system/tehas.slice" ;;
      TEHAS_WORK_MEMORY) reason="is set and this version of tehas has no memory budget to set: the limit around all containers is /etc/systemd/system/tehas.slice" ;;
      *) reason="is set and this version of tehas does not read it" ;;
    esac
    tehas_err "$v $reason (see $file)"
    return 78
  done
  return 0
}

tehas_load() {
  local dir file v reason line mb
  : "${TEHAS_HOME:=$HOME/tehas}"
  export TEHAS_HOME
  _tehas_instance_dir
  file=$dir/instance.env
  [ -r "$file" ] || { tehas_err "no instance.env in $dir (run tehas setup, or copy instances/example and fill it in)"; return 78; }

  # TEHAS_ENERGY, TEHAS_ENERGY_URL, the model endpoints' tokens,
  # TEHAS_MODEL_KEY_*, and the MCP server's TEHAS_MCP and the two lists of keys
  # are host choices in ~/.config/tehas/env, which env.sh reads before this.
  for v in ${!TEHAS_@} ${!PI_ENSEMBLE_@}; do
    case "$v" in
      TEHAS_HOME | TEHAS_INSTANCE_DIR | TEHAS_WORK_CMD | TEHAS_STATUS_CMD | TEHAS_CONSOLE | TEHAS_CONSOLE_CMD | TEHAS_INDEX_CMD | TEHAS_PLAN_CMD | \
        TEHAS_PLAN_START_CMD | TEHAS_INDEX_START_CMD | TEHAS_INDEX_LOCK_WAIT | TEHAS_DEV_LOCK_WAIT | TEHAS_CYCLE_START_CMD | TEHAS_CYCLE_STOP_CMD | TEHAS_CYCLE_START_WAIT | TEHAS_CGROUP_ROOT | TEHAS_SLICE_WAIT | TEHAS_DISK_WAIT | \
        TEHAS_PREFLIGHT_WAIT | TEHAS_STOP_WAIT | TEHAS_FETCH_WAIT | TEHAS_CHECK_WAIT | TEHAS_CONSOLE_WAIT | TEHAS_WORK_GRACE | TEHAS_CAPS_WAIT | TEHAS_TEXT_WAIT | TEHAS_BUILD_TIMEOUT | \
        TEHAS_CODE | TEHAS_CYCLE_RETRY_WAIT | TEHAS_CYCLE_STOP_GH_WAIT | TEHAS_CYCLE_FROM | TEHAS_CYCLE_ATTEMPT | TEHAS_TIDY_CMD | TEHAS_DISPATCH_CMD | TEHAS_HEALTH_CMD | TEHAS_TIDY_WAIT | TEHAS_ANALYTICS_WAIT | TEHAS_GITHUB_API | TEHAS_GITHUB_APP_KEY | TEHAS_GITHUB_RENEW_EVERY | TEHAS_ENERGY | TEHAS_ENERGY_URL | TEHAS_MODEL_KEY_* | TEHAS_MCP | TEHAS_MCP_KEYS | TEHAS_MCP_DASHBOARD_KEYS | TEHAS_PROC_DIR) ;;
      TEHAS_* | PI_ENSEMBLE_*) unset "$v" ;;
    esac
  done
  export TEHAS_INSTANCE_DIR=$dir

  # A token in the instance directory would be in its repository. Read with
  # bash alone, and trust the file: it is operator-owned and run as shell here,
  # so a hostile line needs a reader that does not execute it, as this scan is.
  while IFS= read -r line || [ -n "$line" ]; do
    line=${line#"${line%%[![:space:]]*}"}
    line=${line#export }
    line=${line#"${line%%[![:space:]]*}"}
    case "$line" in
      TEHAS_MODEL_KEY_*)
        tehas_err "$file sets a TEHAS_MODEL_KEY_ token: put it in ~/.config/tehas/env"
        return 78
        ;;
      TEHAS_MCP_KEYS=* | TEHAS_MCP_DASHBOARD_KEYS=*)
        tehas_err "$file sets MCP keys: put TEHAS_MCP_KEYS and TEHAS_MCP_DASHBOARD_KEYS in ~/.config/tehas/env"
        return 78
        ;;
    esac
  done <"$file"

  set -a
  # shellcheck disable=SC1090
  . "$file" || { set +a; tehas_err "$file could not be read as shell"; return 78; }
  set +a

  _tehas_role_models "$file" || return 78
  _tehas_retired_names "$file" || return 78

  # Derived and optional settings.
  if [ -n "${TEHAS_REPO:-}" ]; then
    export TEHAS_WORK_REPO=${TEHAS_WORK_REPO:-$TEHAS_HOME/repos/${TEHAS_REPO##*/}}
    # Where a cycle works, as its container sees it. On the host the tree is
    # $TEHAS_WORK_DIR-<issue>, a fresh clone that tehas work makes for each
    # cycle and the only tree that cycle's container can write. Not a setting.
    export TEHAS_WORK_DIR=$TEHAS_HOME/work/${TEHAS_REPO##*/}
  fi
  # Workstreams one cycle runs side by side. One by default: the factory fans
  # out across issues, not inside one. Parts planned apart from one base do
  # not see each other's new symbols, and each workstream's gate runs the full
  # suite on the same CPUs. More than one is accepted only with one cycle at a
  # time, checked below.
  export TEHAS_CONCURRENCY=${TEHAS_CONCURRENCY:-1}
  # The most issues worked on side by side, each in a cycle of its own. A
  # ceiling: the dispatcher starts fewer while the slice has no room. By default
  # one per CPU, less one for the host. The only host read here.
  v=$(_tehas_host_cpus)
  export TEHAS_TICKET_CONCURRENCY=${TEHAS_TICKET_CONCURRENCY:-$((v > 1 ? v - 1 : 1))}
  # The memory one container is given as its floor, what the dispatcher counts
  # it at, and how many containers tehas.slice holds: each may use 1/n of it.
  # Whether they fit this host is lib/slice.sh's question.
  export TEHAS_CYCLE_RESERVE=${TEHAS_CYCLE_RESERVE:-2g} TEHAS_SLICE_CONTAINERS=${TEHAS_SLICE_CONTAINERS:-4}
  # Whether search engines may index the dashboard and the manual, and whether
  # the host's load and processes are published for the dashboard by `tehasbot
  # top`: off unless the instance says so, because the dashboard can be on the
  # internet and this names the processes of the host.
  export TEHAS_INDEXING=${TEHAS_INDEXING:-0} TEHAS_TOP=${TEHAS_TOP:-0}
  # Visitor analytics on the dashboard and the manual: off unless the instance
  # names a script and a site. The look of every page and the front page.
  export TEHAS_ANALYTICS_SRC=${TEHAS_ANALYTICS_SRC:-} TEHAS_ANALYTICS_SITE=${TEHAS_ANALYTICS_SITE:-}
  export TEHAS_ANALYTICS_COOKIE=${TEHAS_ANALYTICS_COOKIE:-0} TEHAS_THEME=${TEHAS_THEME:-pixel} TEHAS_LAYOUT=${TEHAS_LAYOUT:-floor}
  # What tehasbot clean removes and when: the build output of the toolchains
  # tehas hosts; an instance that builds something else adds its name to the
  # list, it does not replace it.
  export TEHAS_CLEAN_AFTER=${TEHAS_CLEAN_AFTER:-24} TEHAS_CLEAN_DIRS=${TEHAS_CLEAN_DIRS:-$_tehas_clean_dirs_default}
  # Over the medians of the idle polls of the last hour, health warns.
  export TEHAS_POLL_BUDGET=${TEHAS_POLL_BUDGET:-cpu=5,wall=30,writes=0}
  # The two bounds of the measure tehas work makes of an approved body, in
  # bytes and in lines (console/core/src/size.rs).
  export TEHAS_SIZE_MAX_BYTES=${TEHAS_SIZE_MAX_BYTES:-131072} TEHAS_SIZE_MAX_LINES=${TEHAS_SIZE_MAX_LINES:-1000}
  # The factory's own 365 days of runs, events and samples in state/analytics.db
  # (lib/analytics.sh), not the visitor analytics above: on unless the instance
  # says off, which stops every write.
  export TEHAS_ANALYTICS=${TEHAS_ANALYTICS:-on}
  # The improvement loop: the repository it files its findings in (unset, there
  # is no loop, nothing is recorded of a cycle and tehasbot improve does
  # nothing), where it files what only a change in pi-rukas can fix (unset,
  # those findings go with the rest), 1 for nowhere at all, and how many of its
  # issues may stay open in one repository before it files no more there.
  export TEHAS_IMPROVE_REPO=${TEHAS_IMPROVE_REPO:-} TEHAS_IMPROVE_RUKAS_REPO=${TEHAS_IMPROVE_RUKAS_REPO:-}
  export TEHAS_IMPROVE_RUKAS_SKIP=${TEHAS_IMPROVE_RUKAS_SKIP:-0} TEHAS_IMPROVE_MAX_OPEN=${TEHAS_IMPROVE_MAX_OPEN:-10}
  # Where `tehasbot review` files the findings of a run, off until the instance
  # names one as the loop's two above are (never TEHAS_REPO: the check below
  # refuses it). TEHAS_REVIEW is whether it reviews at all: off, since it is an
  # agent on a timer with no person in front of it.
  export TEHAS_REVIEW_REPO=${TEHAS_REVIEW_REPO:-} TEHAS_REVIEW=${TEHAS_REVIEW:-0}
  # The millwright: its account (unset, none), its ceilings of cycles on one issue
  # since a person approved it and of planning questions a session may get, and
  # its feed: whether it may start an issue a person wrote (off), and how many at once.
  export TEHAS_MILLWRIGHT_LOGIN=${TEHAS_MILLWRIGHT_LOGIN:-} TEHAS_MILLWRIGHT_MAX_RETRIES=${TEHAS_MILLWRIGHT_MAX_RETRIES:-2} TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS=${TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS:-3} TEHAS_MILLWRIGHT_FEED=${TEHAS_MILLWRIGHT_FEED:-0} TEHAS_MILLWRIGHT_MAX_FEEDS=${TEHAS_MILLWRIGHT_MAX_FEEDS:-2}
  # The GitHub App the factory acts as, or none (lib/github-app.sh).
  export TEHAS_GITHUB_APP_ID=${TEHAS_GITHUB_APP_ID:-} TEHAS_GITHUB_APP_INSTALLATION=${TEHAS_GITHUB_APP_INSTALLATION:-} TEHAS_BOT_LOGIN=${TEHAS_BOT_LOGIN:-}
  # Refreshes: whether the factory resolves conflicts in its own pull requests
  # that wait for a merge (off unless the instance says so: it starts cycles with
  # no fresh label) and whether an easy merge may come first (TEHAS_EASY_MERGE,
  # off), how many of one pull request since the approval its cycle rests on, the
  # minutes between two runs of tehasbot refresh, and its lanes (0: the cycles').
  export TEHAS_REFRESH_CONFLICTS=${TEHAS_REFRESH_CONFLICTS:-0} TEHAS_REFRESH_MAX=${TEHAS_REFRESH_MAX:-2} TEHAS_EASY_MERGE=${TEHAS_EASY_MERGE:-0}
  export TEHAS_REFRESH_EVERY=${TEHAS_REFRESH_EVERY:-10} TEHAS_REFRESH_SLOTS=${TEHAS_REFRESH_SLOTS:-1}
  # The check on the branch's head commit whose failure holds new cycles back.
  # Unset, the poll does not ask.
  export TEHAS_BRANCH_CHECK=${TEHAS_BRANCH_CHECK:-}
  # The command that decides whether the lowest issues may start, and the seconds
  # it gets before its answer is a closed gate; unset, there is no gate and every
  # ready issue starts as before.
  export TEHAS_LOW_GATE=${TEHAS_LOW_GATE:-} TEHAS_LOW_GATE_WAIT=${TEHAS_LOW_GATE_WAIT:-60}
  # Settings that only work around a pi-rukas defect, each with the release that
  # fixes it: tehas work refuses a cycle once the image has that release, so a
  # workaround does not outlive its reason. And the address the docs server and
  # the edge router bind to.
  export TEHAS_RUKAS_WORKAROUNDS=${TEHAS_RUKAS_WORKAROUNDS:-} TEHAS_HOST_ADDR=${TEHAS_HOST_ADDR:-127.0.0.1}
  # Room for every cycle and two pull requests that wait for a review: a fixed
  # number would leave slots that can never be used. A concurrency that is not a
  # number leaves the floor of three.
  v=3
  ! _tehas_is_count "$TEHAS_TICKET_CONCURRENCY" || v=$((TEHAS_TICKET_CONCURRENCY + 2))
  export TEHAS_MAX_AWAITING=${TEHAS_MAX_AWAITING:-$v}
  # logs before it is stopped as stalled, longer than a run of the gate, which
  # may print nothing. These and the child bound below are the chain of time
  # bounds checked further down.
  export TEHAS_WORK_TIMEOUT=${TEHAS_WORK_TIMEOUT:-21600} TEHAS_WORK_STALL=${TEHAS_WORK_STALL:-3600}
  # Whether a cycle is given the images and the text files its issue links, and
  # whether the model describes those images in words first (one request per
  # image to the endpoint): off unless the instance says so, because each is one
  # more thing a container is given and the images need a model that reads them.
  export TEHAS_WORK_IMAGES=${TEHAS_WORK_IMAGES:-0} TEHAS_WORK_DESCRIBE=${TEHAS_WORK_DESCRIBE:-0}
  export TEHAS_WORK_FILES=${TEHAS_WORK_FILES:-0} TEHAS_WORK_COMMENTS=${TEHAS_WORK_COMMENTS:-0}
  # Whether a zip the issue links is unpacked into the same directory: an archive
  # holds anything, so it is opt-in on top of the images or the files.
  export TEHAS_WORK_ZIPS=${TEHAS_WORK_ZIPS:-0}
  # A planning session: how many turns it may take, and how long one turn may.
  # TEHAS_PLAN_TIMEOUT_MAX, kept out of the environment, is the one place the
  # ceiling of a turn is written: the default below is that ceiling, the check
  # further down refuses more, and the tests pin the copies in libexec/plan,
  # libexec/dispatch and instances/example/optional.env against it. The unit of
  # a turn gets RuntimeMaxSec = the limit + 900 (lib/planning.sh).
  local TEHAS_PLAN_TIMEOUT_MAX=7200
  export TEHAS_PLAN_MAX_TURNS=${TEHAS_PLAN_MAX_TURNS:-20} TEHAS_PLAN_TIMEOUT=${TEHAS_PLAN_TIMEOUT:-$TEHAS_PLAN_TIMEOUT_MAX}
  # Previews of pull requests and the names of the edge: off, and no names, unless
  # the instance says so; how many run at once, and the free disk under which
  # none is built.
  export TEHAS_DOMAIN=${TEHAS_DOMAIN:-} TEHAS_PREVIEWS=${TEHAS_PREVIEWS:-0}
  export TEHAS_PREVIEW_MAX=${TEHAS_PREVIEW_MAX:-5} TEHAS_PREVIEW_MIN_FREE=${TEHAS_PREVIEW_MIN_FREE:-5g}
  # Free disk below which the dispatcher starts no cycle, and below which tehasbot
  # clean takes build output and Docker's build cache whatever their age (the
  # 2026-10-20 outage, when the host disk filled and every running cycle died). The
  # cleaner's hold sits above the dispatcher's, so space goes before work stops.
  export TEHAS_CYCLE_MIN_FREE=${TEHAS_CYCLE_MIN_FREE:-10g} TEHAS_CLEAN_MIN_FREE=${TEHAS_CLEAN_MIN_FREE:-30g}
  # The sandbox image, the private names it must resolve, and how many workstreams
  # one cycle runs side by side inside it: pi-rukas reads the last name, and its
  # own default is more than one, so it is always told.
  export PI_ENSEMBLE_IMAGE=${PI_ENSEMBLE_IMAGE:-tehas/pi-rukas:uid$EUID}
  export PI_ENSEMBLE_HOST_ALIASES=${PI_ENSEMBLE_HOST_ALIASES:-} \
    PI_ENSEMBLE_MAX_WORKSTREAMS=$TEHAS_CONCURRENCY
  # Seconds one run of the product's gate may take inside a cycle. pi-rukas reads
  # the same bound in ms under two names, one for a workstream's gate and one for
  # the gate on the combined tree; it gives the first ten minutes when it is told
  # nothing. A gate refused below is not passed on.
  export TEHAS_VERIFY_TIMEOUT=${TEHAS_VERIFY_TIMEOUT:-1800}
  if _tehas_is_count "$TEHAS_VERIFY_TIMEOUT"; then
    export PI_ENSEMBLE_VERIFY_TIMEOUT_MS=$((TEHAS_VERIFY_TIMEOUT * 1000))
    export PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS=$PI_ENSEMBLE_VERIFY_TIMEOUT_MS
  fi
  # The most files an issue's own body may name before the dispatcher holds it
  # instead of starting a cycle (libexec/dispatch, console/core/src/size.rs): unset, it never
  # asks, and an empty value stays empty rather than become a ceiling.
  export TEHAS_SIZE_MAX_FILES="${TEHAS_SIZE_MAX_FILES-}"
  # Agents get neither the host Docker socket nor SSH keys: not a setting here.
  export PI_ENSEMBLE_NO_DOCKER_SOCKET=1 PI_ENSEMBLE_NO_SSH=1
  # The bound of one child of a cycle: tehas work passes it to the container as
  # PI_ENSEMBLE_SPAWN_TIMEOUT_MS, and TEHAS_CHILD_TIMEOUT_MAX, the one place that
  # bound is written, is pi-rukas's own two hours. A written-but-empty line goes
  # away above, so the default comes after that sweep and not from the export's
  # :- alone; the bounds below keep it under TEHAS_WORK_TIMEOUT and
  # test/config_test.sh pins the copies of it in the manual and the example.
  local TEHAS_CHILD_TIMEOUT_MAX=7200
  [ -n "${TEHAS_CHILD_TIMEOUT:-}" ] || TEHAS_CHILD_TIMEOUT=$TEHAS_CHILD_TIMEOUT_MAX
  export TEHAS_CHILD_TIMEOUT
  # The settings an instance may write, each with the rule for its value. An
  # optional one an instance left out is checked only when it is there, as
  # TEHAS_BUILD_TIMEOUT is: its default belongs to the command that reads it.
  for v in TEHAS_THEME TEHAS_LAYOUT TEHAS_INDEXING TEHAS_WORK_IMAGES TEHAS_WORK_DESCRIBE TEHAS_WORK_FILES TEHAS_WORK_COMMENTS TEHAS_WORK_ZIPS TEHAS_ANALYTICS TEHAS_ANALYTICS_COOKIE TEHAS_TOP TEHAS_EASY_MERGE TEHAS_IMPROVE_RUKAS_SKIP TEHAS_REVIEW TEHAS_CONCURRENCY TEHAS_TICKET_CONCURRENCY TEHAS_HOST_ADDR TEHAS_MAX_AWAITING TEHAS_WORK_TIMEOUT TEHAS_WORK_STALL TEHAS_VERIFY_TIMEOUT TEHAS_CHILD_TIMEOUT TEHAS_PLAN_MAX_TURNS TEHAS_PLAN_TIMEOUT \
    TEHAS_MILLWRIGHT_MAX_RETRIES TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS TEHAS_MILLWRIGHT_FEED TEHAS_MILLWRIGHT_MAX_FEEDS TEHAS_REFRESH_CONFLICTS TEHAS_REFRESH_MAX TEHAS_REFRESH_EVERY TEHAS_REFRESH_SLOTS TEHAS_PREVIEWS TEHAS_PREVIEW_MAX TEHAS_PREVIEW_MIN_FREE TEHAS_CYCLE_MIN_FREE TEHAS_CLEAN_MIN_FREE TEHAS_CYCLE_RESERVE TEHAS_SLICE_CONTAINERS TEHAS_LOW_GATE_WAIT TEHAS_CLEAN_AFTER TEHAS_POLL_BUDGET TEHAS_SIZE_MAX_BYTES TEHAS_SIZE_MAX_LINES; do
    _tehas_why "$v" "${!v}" || { tehas_err "$v is not valid: $reason (see $file)"; return 78; }
  done
  # Its default belongs to install.sh, so an instance that left it out is checked
  # only when it wrote one. PI_RUKAS_FOLLOW is checked whenever it is named.
  if [ -n "${TEHAS_BUILD_TIMEOUT:-}" ]; then
    _tehas_why TEHAS_BUILD_TIMEOUT "$TEHAS_BUILD_TIMEOUT" ||
      { tehas_err "TEHAS_BUILD_TIMEOUT is not valid: $reason (see $file)"; return 78; }
  fi
  if [ -n "${PI_RUKAS_FOLLOW+x}" ]; then
    _tehas_why PI_RUKAS_FOLLOW "$PI_RUKAS_FOLLOW" || { tehas_err "PI_RUKAS_FOLLOW is not valid: $reason (see $file)"; return 78; }
  fi
  for v in TEHAS_BRANCH TEHAS_DOMAIN TEHAS_PROTOLAB_URL TEHAS_UPDATE_FROM TEHAS_RELEASE_REPO TEHAS_SIZE_MAX_FILES TEHAS_DISPATCHER TEHAS_CYCLE_REPORT TEHAS_SCRATCH_DIR; do
    [ -n "${!v:-}" ] || continue
    _tehas_why "$v" "${!v}" || { tehas_err "$v is not valid: $reason (see $file)"; return 78; }
  done
  for v in TEHAS_ANALYTICS_SRC TEHAS_ANALYTICS_SITE TEHAS_IMPROVE_REPO TEHAS_IMPROVE_RUKAS_REPO TEHAS_IMPROVE_MAX_OPEN TEHAS_MILLWRIGHT_LOGIN TEHAS_GITHUB_APP_ID TEHAS_GITHUB_APP_INSTALLATION TEHAS_BOT_LOGIN TEHAS_RUKAS_WORKAROUNDS TEHAS_BRANCH_CHECK TEHAS_LOW_GATE TEHAS_CLEAN_DIRS; do
    [ -n "${!v}" ] || continue
    _tehas_why "$v" "${!v}" || { tehas_err "$v is not valid: $reason (see $file)"; return 78; }
  done
  # An entry for a setting the instance no longer has is a workaround removed
  # by half.
  for v in $TEHAS_RUKAS_WORKAROUNDS; do
    v=${v%@*}
    [ "$v" != TEHAS_RUKAS_WORKAROUNDS ] && [ -n "${!v:-}" ] && continue
    tehas_err "TEHAS_RUKAS_WORKAROUNDS names $v, which is not set: remove its entry (see $file)"
    return 78
  done
  # A GitHub App is named by all three or not at all: an ID without the
  # installation makes no token, and a token without the bot's login leaves the
  # dispatcher no name to keep out of the labellers.
  if [ -n "$TEHAS_GITHUB_APP_ID$TEHAS_GITHUB_APP_INSTALLATION$TEHAS_BOT_LOGIN" ] &&
    { [ -z "$TEHAS_GITHUB_APP_ID" ] || [ -z "$TEHAS_GITHUB_APP_INSTALLATION" ] || [ -z "$TEHAS_BOT_LOGIN" ]; }; then
    tehas_err "TEHAS_GITHUB_APP_ID, TEHAS_GITHUB_APP_INSTALLATION and TEHAS_BOT_LOGIN are set together or not at all (see $file)"
    return 78
  fi
  if [ -n "$TEHAS_ANALYTICS_SRC" ] && [ -z "$TEHAS_ANALYTICS_SITE" ]; then
    tehas_err "TEHAS_ANALYTICS_SITE is not set and TEHAS_ANALYTICS_SRC is (see $file)"
    return 78
  fi
  if [ -n "$TEHAS_IMPROVE_RUKAS_REPO" ] && [ -z "$TEHAS_IMPROVE_REPO" ]; then
    tehas_err "TEHAS_IMPROVE_REPO is not set and TEHAS_IMPROVE_RUKAS_REPO is (see $file)"
    return 78
  fi
  if [ -n "$TEHAS_IMPROVE_RUKAS_REPO" ] && [ "$TEHAS_IMPROVE_RUKAS_SKIP" = 1 ]; then
    tehas_err "TEHAS_IMPROVE_RUKAS_SKIP is 1 and TEHAS_IMPROVE_RUKAS_REPO is set: findings about pi-rukas are either filed there or not at all (see $file)"
    return 78
  fi
  # A model's findings go to a repository of their own, where a person triages
  # them into tickets of the product: filing them in the product is the one
  # setting that cannot be allowed, for the loop and for the review alike.
  for v in "$TEHAS_IMPROVE_REPO" "$TEHAS_IMPROVE_RUKAS_REPO" "${TEHAS_REVIEW_REPO:-}"; do
    [ -z "${TEHAS_REPO:-}" ] || [ "${v,,}" != "${TEHAS_REPO,,}" ] || {
      tehas_err "the improvement loop files in $TEHAS_REPO, the product repository: a model writes its findings from what cycles saw, so they go to a repository of their own where a person triages them (see $file)"
      return 78
    }
  done
  case " ${TEHAS_ALLOWED_LABELERS:-} ${TEHAS_BOT_LOGIN%\[bot\]} " in
    *" ${TEHAS_MILLWRIGHT_LOGIN%\[bot\]} "*)
      [ -z "$TEHAS_MILLWRIGHT_LOGIN" ] || {
        tehas_err "TEHAS_MILLWRIGHT_LOGIN ($TEHAS_MILLWRIGHT_LOGIN) is in TEHAS_ALLOWED_LABELERS or is TEHAS_BOT_LOGIN: the millwright is an account of its own and works within its own grant, never as the bot or a person (see $file)"
        return 78
      }
      ;;
  esac
  if [ "$TEHAS_WORK_DESCRIBE" = 1 ] && [ "$TEHAS_WORK_IMAGES" != 1 ]; then
    tehas_err "TEHAS_WORK_DESCRIBE is 1 and TEHAS_WORK_IMAGES is not: there are no images to describe (see $file)"
    return 78
  fi
  if [ "$TEHAS_WORK_COMMENTS" = 1 ] && [ "$TEHAS_WORK_IMAGES" != 1 ] && [ "$TEHAS_WORK_FILES" != 1 ]; then
    tehas_err "TEHAS_WORK_COMMENTS is 1 and neither TEHAS_WORK_IMAGES nor TEHAS_WORK_FILES is: nothing is taken from comments (see $file)"
    return 78
  fi
  if [ "$TEHAS_WORK_ZIPS" = 1 ] && [ "$TEHAS_WORK_IMAGES" != 1 ] && [ "$TEHAS_WORK_FILES" != 1 ]; then
    tehas_err "TEHAS_WORK_ZIPS is 1 and neither TEHAS_WORK_IMAGES nor TEHAS_WORK_FILES is: an unpacked file goes to the directory of one of them (see $file)"
    return 78
  fi
  # One axis of parallelism: several cycles of one workstream, or one cycle of
  # several. Both would multiply the agents and the gates on the same CPUs.
  if [ "$TEHAS_CONCURRENCY" -gt 1 ] && { [ "$TEHAS_TICKET_CONCURRENCY" = auto ] || [ "$TEHAS_TICKET_CONCURRENCY" -gt 1 ]; }; then
    tehas_err "TEHAS_CONCURRENCY ($TEHAS_CONCURRENCY) and TEHAS_TICKET_CONCURRENCY ($TEHAS_TICKET_CONCURRENCY) are both above 1: choose one, several issues at a time with one workstream each, or one issue at a time with TEHAS_TICKET_CONCURRENCY=1 (see $file)"
    return 78
  fi
  # The cycle's time bounds in the order they must sit in: the gate under the
  # stall that must not fire while a gate prints nothing, one child at or under
  # the whole cycle, and the cycle under what the unit outlives. Each is refused,
  # not clamped, and each name is a count by now (_tehas_is_count, in the sweep
  # above), so no [ -gt ] here reads a failed comparison as no refusal. The unit
  # gets RuntimeMaxSec = TEHAS_PLAN_TIMEOUT + 900 (lib/planning.sh); 7200 + 900 is
  # 2h15, inside the unit's own bound, and beyond it a turn outlives its unit. The
  # length is bounded with the value: a count longer than the ceiling's digits
  # cannot be compared by [ ], and would wrap in that arithmetic.
  if [ "$TEHAS_WORK_STALL" -le "$TEHAS_VERIFY_TIMEOUT" ]; then
    tehas_err "TEHAS_WORK_STALL ($TEHAS_WORK_STALL) is not longer than TEHAS_VERIFY_TIMEOUT ($TEHAS_VERIFY_TIMEOUT): a gate that prints nothing would be stopped as stalled (see $file)"
    return 78
  fi
  if [ "$TEHAS_CHILD_TIMEOUT" -gt "$TEHAS_WORK_TIMEOUT" ]; then
    tehas_err "TEHAS_CHILD_TIMEOUT ($TEHAS_CHILD_TIMEOUT) is more than TEHAS_WORK_TIMEOUT ($TEHAS_WORK_TIMEOUT): a child that outlives its cycle is one tehas stopped too late (see $file)"
    return 78
  fi
  # tehas-cycle@.service stops a cycle after 8 hours (RuntimeMaxSec) and a unit
  # cannot read this file; the hour between is for the stop and the report.
  if [ "$TEHAS_WORK_TIMEOUT" -gt 25200 ]; then
    tehas_err "TEHAS_WORK_TIMEOUT ($TEHAS_WORK_TIMEOUT) is more than 25200: tehas-cycle@.service stops a cycle after 8 hours, and the stop and the report need the hour after the limit (see $file)"
    return 78
  fi
  if [ "${#TEHAS_PLAN_TIMEOUT}" -gt "${#TEHAS_PLAN_TIMEOUT_MAX}" ] ||
    [ "$TEHAS_PLAN_TIMEOUT" -gt "$TEHAS_PLAN_TIMEOUT_MAX" ]; then
    tehas_err "TEHAS_PLAN_TIMEOUT ($TEHAS_PLAN_TIMEOUT) is more than $TEHAS_PLAN_TIMEOUT_MAX: a planning turn may take at most two hours, and while one runs no other planning turn starts (see $file)"
    return 78
  fi
  if [ "$TEHAS_PREVIEWS" = 1 ] && [ -z "$TEHAS_DOMAIN" ]; then
    tehas_err "TEHAS_PREVIEWS is 1 and TEHAS_DOMAIN is not set: a preview has no address without it (see $file)"
    return 78
  fi
  # The three sizes above in MiB, for the df figures of dispatch, clean and
  # preview, which are in KiB. Each is a valid size by now, so the call's own
  # status carries nothing.
  _tehas_mem_mb "$TEHAS_PREVIEW_MIN_FREE"; TEHAS_PREVIEW_MIN_FREE_MB=$mb
  _tehas_mem_mb "$TEHAS_CYCLE_MIN_FREE"; TEHAS_CYCLE_MIN_FREE_MB=$mb
  _tehas_mem_mb "$TEHAS_CLEAN_MIN_FREE"; TEHAS_CLEAN_MIN_FREE_MB=$mb
  export TEHAS_PREVIEW_MIN_FREE_MB TEHAS_CYCLE_MIN_FREE_MB TEHAS_CLEAN_MIN_FREE_MB
  # The dashboard's lab host name follows the edge's unless the instance named one.
  [ -n "${TEHAS_DOMAIN:-}" ] && [ -z "${TEHAS_PROTOLAB_URL:-}" ] &&
    export TEHAS_PROTOLAB_URL=https://lab.$TEHAS_DOMAIN
  return 0
}

tehas_require() {
  local name reason file=${TEHAS_INSTANCE_DIR:-?}/instance.env
  for name in "$@"; do
    # Only spaces counts as not set.
    [ -n "${!name:-}" ] && [ -n "${!name// /}" ] || { tehas_err "$name is not set (see $file)"; exit 78; }
    _tehas_why "$name" "${!name}" || { tehas_err "$name is not valid: $reason (see $file)"; exit 78; }
  done
}

# The settings an instance has to set itself, because nothing here can give them
# a sensible default: install.sh, tehas-dispatch and tehas update each check this
# one list before they call GitHub, Docker, git or write a file.
tehas_require_instance_settings() {
  tehas_require TEHAS_REPO TEHAS_CONTROL_ISSUE TEHAS_ALLOWED_LABELERS \
    PI_ENSEMBLE_SUBAGENT_MODEL PI_ENSEMBLE_SUBAGENT_PROVIDER
}

# Every provider and model pair the agents use, one "provider model" line
# each, without repeats: the subagent default first, then each role's own.
tehas_models() {
  local role m p
  {
    printf '%s %s\n' "${PI_ENSEMBLE_SUBAGENT_PROVIDER:-}" "${PI_ENSEMBLE_SUBAGENT_MODEL:-}"
    for role in "${_tehas_roles[@]}"; do
      m=PI_ENSEMBLE_MODEL_$role p=PI_ENSEMBLE_PROVIDER_$role
      [ -z "${!m:-}" ] || printf '%s %s\n' "${!p:-}" "${!m}"
    done
  } | awk '$1 != "" && $2 != "" && !seen[$0]++'
}

# The product checkout must exist and be a clone of TEHAS_REPO: issues are
# polled from one and the work happens in the other, so they have to be the
# same repository. Returns 78 otherwise. Reads the checkout's configuration
# only; nothing is fetched.
tehas_checkout() {
  local dir=${TEHAS_WORK_REPO:-} url
  if [ -z "$dir" ] || [ ! -d "$dir/.git" ]; then
    tehas_err "no product checkout at ${dir:-(unset)}: clone $TEHAS_REPO there, or set TEHAS_WORK_REPO in ${TEHAS_INSTANCE_DIR:-?}/instance.env"
    return 78
  fi
  url=$(git -C "$dir" config --get remote.origin.url 2>/dev/null) || url=""
  url=${url%/}
  url=${url%.git}
  case "$url" in
    */"$TEHAS_REPO" | *:"$TEHAS_REPO") return 0 ;;
  esac
  tehas_err "$dir is a checkout of ${url:-nothing}, not of $TEHAS_REPO"
  return 78
}

# The branch cycles start from and pull requests target. Read locally: the
# checkout's origin/HEAD is set by install.sh, never looked up here.
tehas_branch() {
  local b=""
  [ -z "${TEHAS_BRANCH:-}" ] || { printf '%s\n' "$TEHAS_BRANCH"; return 0; }
  b=$(git -C "${TEHAS_WORK_REPO:-/nonexistent}" symbolic-ref -q --short refs/remotes/origin/HEAD 2>/dev/null) || b=""
  b=${b#origin/}
  [ -n "$b" ] || {
    tehas_err "the default branch of ${TEHAS_WORK_REPO:-the product checkout} is unknown: run install.sh, or set TEHAS_BRANCH in ${TEHAS_INSTANCE_DIR:-?}/instance.env"
    return 78
  }
  printf '%s\n' "$b"
}
