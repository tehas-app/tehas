# shellcheck shell=bash
# shellcheck disable=SC2034  # the variables here are read by the test files
# The readers of a container's docker arguments, shared by the tests of the
# four containers an agent runs in: the cycle, the planning turn, the manual
# session and the push check. Sourced by test/work_lib.sh (the cycle's tests)
# and by test/plan_test.sh and test/dev_test.sh, so the documented security
# options have one home in the tests: the literal below. It is not read from
# tehas_sandbox_security, which is the function under test.
#
# Every helper reads the arguments of one docker call on standard input, one
# argument per line, as the stub of docker records them (test/lib.sh).
#
#   security_opts      the documented options as one line
#   security_tmpfs     the mount and its options, the value of --tmpfs
#   docker_opts        the options by name, for the allow-list of unknown_options
#   security_of <flag> the words after one option, as docker reads them
#   security_line      the documented options in order, as one line; empty
#                      when a container is given them another way
#   count_of <flag>    how many times an option stands on the line
#   unknown_options <image> <options that take a value> <options that take none>
#                      every option of the call that is not one the command is
#                      known to pass, up to the image name
T_ROOT=${T_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}
# shellcheck source=lib/sandbox.sh
. "$T_ROOT/lib/sandbox.sh"
# The documented list, for the ceiling the stubbed slice gives a container
# (--memory 2048m): /tmp is half of it. The names below are made from this
# string. They are strings, not arrays: the suites that source this write their
# own lists of mounts and variables, and an array of the same name here would
# read as one variable declared two ways.
security_opts='--cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true --tmpfs /tmp:rw,exec,nosuid,nodev,size=1024m,mode=1777'
# The mount of the tmpfs and its options: the word after --tmpfs in the list.
security_tmpfs=${security_opts#*--tmpfs }
security_tmpfs=${security_tmpfs%% *}
# The options of the documented list, spelled as the second argument of
# unknown_options wants them: the option itself, not its value. Derived from the
# same words, so an option added to the list is allow-listed in every suite
# at once, and one left out of the list is refused by every container test.
docker_opts=
for _word in $security_opts; do
  case $_word in -*) docker_opts="${docker_opts:+$docker_opts }$_word" ;; esac
done
unset _word

# How many times an option is given, and with what value: the words after it,
# read the way docker reads them. <flag> is one word, as it stands on the line.
security_of() { # <flag> (reads the arguments on standard input)
  awk -v flag="$1" 'prev == flag { print } { prev = $0 }'
}

# The documented options, in order, as one line. Empty when a container is
# given them another way or not at all. Reads the arguments on standard input.
# The words of the list travel in the environment, not as -v assignments: awk
# takes an assignment whose value begins with a dash for a flag of its own, and
# an environment value is read as data.
security_line() {
  local i n words doc
  words=$(tr ' ' '\n' <<<"$security_opts")
  n=$(printf '%s\n' "$words" | sed -n '$=')
  for i in $(seq 0 $((n - 1))); do
    export "TEHAS_TEST_W$i=$(printf '%s\n' "$words" | sed -n "$((i + 1))p")"
  done
  TEHAS_TEST_N=$n awk '
    BEGIN {
      n = ENVIRON["TEHAS_TEST_N"] + 0
      # The name is not "want": this file is sourced into suites that hold a
      # variable of that name themselves, and shellcheck reads the two as one
      # declared an array here and a string there.
      for (k = 1; k <= n; k++) doc[k] = ENVIRON["TEHAS_TEST_W" k - 1]
    }
    { arg[NR] = $0 }
    END {
      for (i = 1; i <= NR; i++) {
        if (seen >= n) break
        if (arg[i] != doc[seen + 1]) continue
        for (j = 1; j <= n; j++) if (arg[i + j - 1] != doc[j]) break
        if (j > n) { seen = n; start = i }
      }
      if (seen == n) for (i = start; i < start + n; i++) printf "%s ", arg[i]
    }'
}

# How many times an option stands on a docker line. An option written
# --flag=<value>, a spelling docker also accepts, counts as that option too:
# a mount written that way is still a mount, and must not be invisible to the
# guard that counts them. <flag> is one word, as it stands on the line.
count_of() { # <flag> (reads the arguments on standard input)
  awk -v flag="$1" '{ t = index($0, "=") ? substr($0, 1, index($0, "=") - 1) : $0 }
    t == flag { n++ } END { print n + 0 }'
}

# Every option of a docker call that is not one this command is known to
# pass, up to the image name. Options are read the way docker reads them: the
# ones that take a value consume the next argument. A mount or a variable
# spelled another way (--volume, --mount, --env, -eNAME), or any other option
# (--network, --cap-add, --user 0:0), shows up here. An option written
# --flag=<value> is matched by its flag, so a second mount or an undocumented
# option in that form shows up too.
unknown_options() { # <image> <options that take a value> <options that take none>
  awk -v image="$1" -v valued=" $2 " -v plain=" $3 " '
    NR == 1 { next } # "run"
    skip { skip = 0; next }
    $0 == image { exit }
    { t = index($0, "=") ? substr($0, 1, index($0, "=") - 1) : $0 }
    index(valued, " " t " ") { skip = (t == $0); next }
    index(plain, " " t " ") { next }
    /^--add-host=[A-Za-z0-9.-]+:[0-9a-fA-F.:]+$/ { next }
    { print }'
}

# The security options, in the words the manual's table of
# "What a container may not do" in host/docs/host.md gives them: one row per
# option, its value in the second cell. test/work_caps_test.sh compares what
# tehas_sandbox_security prints with this, so the table and the container are
# pinned to the same list and one is not a copy of the other kept by hand.
security_doc_rows() {
  printf '%s\n' '--cap-drop|ALL' '--cap-add|CHOWN' '--cap-add|FOWNER' \
    '--security-opt|no-new-privileges:true' "--tmpfs|${security_tmpfs/size=1024m/size=<half of --memory>m}"
}
