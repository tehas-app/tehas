# shellcheck shell=bash
# shellcheck disable=SC2034,SC2016  # the parts read the variables; the bodies
# below are written to the shim by printf and expanded when the shim runs
# The cost of one poll, measured on this machine: the programs the poll and the
# commands it runs start, counted by a PATH shim that notes every call before it
# runs the real one. The dispatch tests' own stubs of gh, systemctl, docker and
# df are shells like any other and t_stub notes their calls, so the figure is
# the processes one poll starts on the runner, which is what a ceiling on them
# is about. A cost test sources this after test/dispatch_lib.sh, before it runs
# the poll whose cost it pins.

# The programs of the host the poll and the commands it runs reach for, besides
# the stubs the dispatch tests install. A test that names another one adds it.
t_proc_names=(
  awk basename cat chmod cmp cp cut date dirname du find grep head jq ln
  mkdir mktemp mv rm sed sleep sort stat tail timeout tr uname wc whoami
)

# The counter: one line per call, in the file every shim of the run appends to.
# libexec/dispatch counts its own gh calls by a function of that name, so a
# shim of gh would be counted twice over: the stub gh of the dispatch tests
# notes its own calls, and t_gh_reads counts them.
t_procs_calls=$T/calls/procs

# t_stub_procs <name>...: for each name a shim that notes the call and then runs
# the real program of the rest of PATH. t_run puts $T/bin first, so without a
# shim of its own a name would never be seen at all; with one the count is
# honest and the command still works. A name the dispatch tests already stub is
# left as it is, since t_stub notes its calls too.
t_stub_procs() {
  local name
  for name in "$@"; do
    [ ! -e "$T/bin/$name" ] || continue
    {
      echo "#!$T_BASH"
      printf '%s\n' "printf '%s\\t%s\\n' procs \"\$0 \$*\" >>\"$t_procs_calls\"" \
        'real=$(PATH=${PATH#*:/} command -v -- "${0##*/}") || exit 127' \
        'exec "$real" "$@"'
    } >"$T/bin/$name"
    chmod +x "$T/bin/$name"
  done
}

# The store, which lib/store.sh runs by name, so no PATH shim sees it: the shim
# of the installed binary notes the store and the two processes lib/store.sh
# puts around every call - the timeout that bounds it and the jq that reads its
# answer - and then runs the real store beside the call t_stub already noted. A
# poll pays for all of them, so a ceiling that left them out would be worth
# nothing. The binary t_store_home installs is a hard link to the build of the
# run, so the real one is put aside once, on its first sight - the case that
# makes it is the one that knows its own shape - and the shim takes its place.
# The shim of a later case is the shim of the first, and does not nest.
t_stub_store_procs() {
  local bin=$T/home/tehas/libexec/store real=$T/store-real
  [ -f "$bin" ] || return 0
  if [ ! -f "$real" ]; then
    head -c 4 "$bin" | grep -q '#!.' || return 0
    mv "$bin" "$real"
  fi
  {
    echo "#!$T_BASH"
    printf '%s\n' \
      "printf 'procs\t%s/store %s\n' \"\$PWD\" \"\$*\" >>\"$t_procs_calls\"" \
      "printf 'procs\t%s/timeout %s\n' \"\$PWD\" \"\$*\" >>\"$t_procs_calls\"" \
      "printf 'procs\t%s/jq %s\n' \"\$PWD\" \"\$*\" >>\"$t_procs_calls\"" \
      "exec $(printf '%q' "$real") \"\$@\""
  } >"$bin"
  chmod 755 "$bin"
}

# t_procs_reset: the counter begins again and every name has its shim. A test
# that wants an empty write log does it itself, before this: the shim of the
# host's gh appends to that file, so its absence is the host having no app.
t_procs_reset() {
  rm -f "$t_procs_calls"
  : >"$t_procs_calls"
  t_stub_procs "${t_proc_names[@]}"
  t_stub_store_procs
}

# t_procs: how many programs one poll started. t_procs_list names the programs,
# the most often called first, with the calls of each counted beside it. The
# line is the word procs, a tab, the path the shim was called by, a single space
# and the arguments: a jq program among them holds spaces, tabs and newlines of
# its own, so only the path is read, and it is the text between the last slash
# and the first space after it. No path of a test has a space in it.
t_procs() { grep -c . "$t_procs_calls" 2>/dev/null || true; }
t_procs_list() {
  grep . "$t_procs_calls" 2>/dev/null | sed -n 's/^procs\t.*\/\([^ ]*\) .*/\1/p' |
    sort | uniq -c | sort -nr
}

# t_procs_of <program>: how many calls of one program the counter noted.
t_procs_of() { # <name>
  grep -c "^procs\t.*/${1} \|^procs\t.*/${1}$" "$t_procs_calls" 2>/dev/null || true
}

# t_gh_reads: the calls of the stub gh. A poll that starts nothing makes only
# reads; t_writes counts what the host's shim logged as writes, which is the
# figure the poll's own record keeps.
t_gh_reads() { t_calls gh | grep -c . || true; }
t_writes() {
  local log=$T/home/tehas/state/github-writes.log
  grep -c . "$log" 2>/dev/null || true
}
