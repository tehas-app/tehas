#!/usr/bin/env bash
# Per-project paths are named under $TEHAS_PROJECT_HOME, not $TEHAS_HOME, so a
# host can later hold several projects. This test fails when installed code
# names one under $TEHAS_HOME again. The host-level files are listed by name;
# a line that names a host-level directory ends with `# host-level`. Comment
# lines are not checked. Tests, the manual and the records in docs/ may name
# any path.
# shellcheck disable=SC2016  # the samples are lines of code, not expanded
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

# Per-project paths named under TEHAS_HOME, as extended regular expressions.
per_project='TEHAS_HOME\}?"?/(state|live|logs|work|dev|protolab|preview)(/|"|$)|TEHAS_HOME\}?"?/repos/[^p]|TEHAS_HOME\}?"?/repos/\$|TEHAS_HOME\}?"?/tmp/improve'
# The host-level files that stay under TEHAS_HOME.
host_level='TEHAS_HOME\}?"?/state/(update\.lock|update-hold|update-pending|edge-takeover|platform-fetch|improve\.lock|dispatch\.again)|TEHAS_HOME\}?"?/live/(top|energy|platform)\.json'
# A path built by printf: the format names the directory, TEHAS_HOME is an argument.
printf_per_project='%s/(state|live|logs|work|dev|protolab|preview|repos/[^p]|repos/\$|tmp/improve)'
printf_host_level='%s/(state/(update\.lock|update-hold|update-pending|edge-takeover|platform-fetch|improve\.lock|dispatch\.again)|live/(top|energy|platform)\.json)'

code() { # every tracked file that is installed or run
  git -C "$T_ROOT" ls-files -- bin libexec lib host install.sh ':!host/docs'
}

# lines <file>...: the code lines of the files, as file:line:text, without
# comment lines.
lines() {
  local f
  for f in "$@"; do
    grep -nvE '^[[:space:]]*#' "$T_ROOT/$f" 2>/dev/null | sed "s|^|$f:|"
  done
}

# violations: per-project paths that are neither host-level nor marked.
violations() {
  local in
  in=$(cat)
  {
    grep -E -- "$per_project" <<<"$in" | grep -vE -- "$host_level"
    grep -E 'printf' <<<"$in" | grep -E 'TEHAS_HOME' | grep -E -- "$printf_per_project" | grep -vE -- "$printf_host_level"
  } | grep -vE -- '# host-level[[:space:]]*$' | sort -u || true
}

t_case "installed code names no per-project path under TEHAS_HOME"
files=()
mapfile -t files < <(code)
t_eq "$(lines "${files[@]}" | violations)" ""

t_case "the Pi agent directory is named only through tehas_agent_dir"
files=()
while IFS= read -r f; do [ "$f" = lib/config.sh ] || files+=("$f"); done < <(code)
t_eq "$(lines "${files[@]}" | grep -E -- '\$(\{HOME\}|HOME)/\.pi/agent/(models\.json|settings\.json|ensemble-runs)' || true)" ""

t_case "each marked line names TEHAS_HOME"
mapfile -t marked < <(code)
bad=$(lines "${marked[@]}" | grep -E -- '# host-level[[:space:]]*$' | grep -v -- 'TEHAS_HOME' || true)
t_eq "$bad" ""

t_case "each pattern finds what it is for"
hit() { printf '%s\n' "$1" | violations; }
t_contains "$(hit 'x=$TEHAS_HOME/state/cycles/1.lock')" "cycles/1.lock"
t_contains "$(hit 'x="${TEHAS_HOME}/logs/a.log"')" "logs/a.log"
t_contains "$(hit 'x=$TEHAS_HOME/repos/$name')" "repos/"
t_contains "$(hit 'x=$TEHAS_HOME/tmp/improve')" "tmp/improve"
t_eq "$(hit 'x=$TEHAS_HOME/repos/pi-rukas')" ""
t_eq "$(hit 'x=$TEHAS_HOME/state/update.lock')" ""
t_eq "$(hit 'x=$TEHAS_HOME/live/top.json')" ""
t_eq "$(hit 'x=$TEHAS_PROJECT_HOME/state')" ""
t_contains "$(hit 'for l in "$TEHAS_HOME"/state/cycles/*.lock; do')" "cycles"
t_eq "$(hit 'for l in "$TEHAS_PROJECT_HOME"/state/cycles/*.lock; do')" ""
t_eq "$(hit 'x="$TEHAS_HOME"/state/update.lock')" ""
t_contains "$(hit "park_id() { printf '%s/state/park-%s.id\\n' \"\$TEHAS_HOME\" \"\$1\"; }")" "park-"
t_eq "$(hit "park_id() { printf '%s/state/park-%s.id\\n' \"\$TEHAS_PROJECT_HOME\" \"\$1\"; }")" ""
t_eq "$(hit "f() { printf '%s/state/update-pending\\n' \"\$TEHAS_HOME\"; }")" ""
t_eq "$(hit 'mkdir -p "$TEHAS_HOME/state" # host-level')" ""
t_eq "$(hit 'x=$TEHAS_HOME/tmp/scratch')" ""

t_case "every host-level entry is still used"
# By file name: the code often reaches them through a variable.
mapfile -t installed < <(code)
all=$(lines "${installed[@]}")
for name in update.lock update-hold update-pending edge-takeover platform-fetch improve.lock dispatch.again top.json energy.json platform.json; do
  grep -qE -- "$host_level" < <(printf '%s\n' "x=\$TEHAS_HOME/state/$name" "x=\$TEHAS_HOME/live/$name") ||
    t_bad "host_level lacks $name"
  grep -qF -- "$name" <<<"$all" || t_bad "no installed code uses $name"
done

t_done
