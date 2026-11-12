# shellcheck shell=bash
# shellcheck disable=SC2016,SC2034  # stub bodies are expanded when they run; the parts read t_err
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
# What the tests of the gh write log share: test/shim_writes_*_test.sh source
# this. The installed shim, a stub gh, and the helpers that read the log.
# The part has sourced test/lib.sh before this.
t_console || t_bad "no console: $t_console_why"
log=$T/home/tehas/state/github-writes.log

# The poll's token (#938), measured on the same installed shim. The dispatcher
# gives every gh child of a poll TEHAS_GH_TOKEN, and the shim then exports it
# as GH_TOKEN and runs no tehasbot github-token. A tehasbot of the case's
# stands under bin and says on a file of its own that it ran, so the one
# thing gh's arguments cannot say - that it was not run at all - is seen.
install_shim() {
  local h=$T/home/tehas
  rm -rf "$h" "$T"/calls/* "$T/err" "$T/token-runs"
  mkdir -p "$h/bin" "$h/shim" "$h/libexec"
  cp "$T_ROOT/host/shim/gh" "$h/shim/gh"
  chmod +x "$h/shim/gh"
  # The two lines lib/config.sh and lib/github-app.sh, from the source of this
  # run: tehasbot github-token is asked in the environment of the run,
  # so it must be able to load its settings as the host's does.
  mkdir -p "$h/lib"
  cp "$T_ROOT/lib/config.sh" "$T_ROOT/lib/github-app.sh" "$T_ROOT/lib/paths.sh" "$h/lib/"
  # A gh that answers what it is told to: the words, the error and the status
  # are named by the case, in the environment of its run. With nothing named it
  # says the call back on stdout, which is how a case sees that the shim
  # reached gh at all and handed its words back. SHOW_TTY adds whether the
  # stub's stdout is a terminal: the body's own is a file of t_stub's, so it
  # reads the stub's ($$ in the body's subshell) from /proc. The words name no token: the
  # token belongs to test/github_app_test.sh, which keeps a real one, and a
  # case of this file names one itself when it wants gh to have had it.
  t_stub gh 'if [ -n "${GH_OUT:-}" ]; then printf "%s\n" "$GH_OUT"; else echo "gh $*"; fi
if [ -n "${SHOW_TTY:-}" ]; then case $(readlink "/proc/$$/fd/1") in /dev/pts/*) echo stdout-tty ;; *) echo stdout-pipe ;; esac; fi
if [ -n "${SHOW_TOKEN:-}" ]; then printf "token=%s tehas=%s\n" "${GH_TOKEN:-none}" "${TEHAS_GH_TOKEN:-unset}"; fi
[ -z "${GH_ERR:-}" ] || printf "%s\n" "$GH_ERR" >&2
exit "${GH_RC:-0}"'
  # bin/tehasbot of this run with the console beside it, whose github-token
  # loads the settings and prints a token that has at least 10 minutes left;
  # the case keeps the one it wants
  # and the run notes here that the program was asked at all. openssl and curl
  # are never reached, because the kept token has its time left.
  mkdir -p "$h/bin"
  cp "$T_ROOT/bin/tehasbot" "$h/bin/"
  ln -sf "$T_CONSOLE" "$h/libexec/console"
  # t_instance writes the instance and names it in the runs that follow.
  t_instance TEHAS_REPO=acme/widget TEHAS_GITHUB_APP_ID=5452597
  mkdir -p "$T/home/.config/tehas"
  printf 'a key\n' >"$T/home/.config/tehas/github-app.pem"
  chmod 600 "$T/home/.config/tehas/github-app.pem"
  # The note of a run, beside the kept token of the case, with an hour left.
  # The real command stays under bin, where its own directory finds the
  # console; a case says whether it was asked by what it leaves on
  # $T/token-runs.
  mkdir -p "$h/state/github"
  printf '%s\n' "${TOKEN_ANSWER:-ghs_token}" >"$h/state/github/token"
  echo $(($(date +%s) + 3600)) >"$h/state/github/token.expires"
  {
    printf '%s\n' "printf x >>\"$T/token-runs\""
    cat "$h/bin/tehasbot"
  } >"$h/bin/tehasbot.new"
  mv "$h/bin/tehasbot.new" "$h/bin/tehasbot"
  chmod 755 "$h/bin/tehasbot"
  mv "$T/bin/gh" "$h/bin/gh"
  chmod +x "$h/bin/gh"
}
reset() {
  install_shim
  T_ENV=()
}
# shim <arguments...>: one call of the installed shim. t_run keeps no stderr of
# its own: it reads the command's two streams together into t_out, so a case
# that wants the error stream alone names it in the snippet it runs itself.
shim() { t_run "$T/home/tehas/shim/gh $(printf '%q ' "$@")"; }
# A snippet for a case that wants the two streams apart: t_run keeps no stream
# of its own, so the error goes to a file the case reads itself.
shim_split() {
  local err=$T/err
  rm -f "$err"
  t_run "$T/home/tehas/shim/gh $(printf '%q ' "$@") 2>$(printf '%q' "$err")"
  t_err=$(cat "$err" 2>/dev/null)
}
# lines: the log's content, empty when there is no log at all.
lines() { cat "$log" 2>/dev/null || true; }
# field <n>: the nth field of the last line, cut at the tabs.
field() { tail -n 1 <<<"$(lines)" | cut -f"$1"; }
