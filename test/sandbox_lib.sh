# shellcheck shell=bash
# shellcheck disable=SC2034  # the test files read what this names
# What the tests of every sandbox command share. A test of libexec/work,
# libexec/plan, libexec/dev or libexec/review sources this beside test/lib.sh.
#
# AGENTS.md rule 4 makes the mounts and the variables of a cycle's container
# the security statement, and lib/sandbox.sh is what assembles them. Pinning
# them once per command file lets one file drift from another: a mount or an
# -e added to the library then passes the tests of the command that never
# named it, as though it had been vetted. So the lists below are the one
# account of what the library gives, and every consumer's test reads them from
# here. A change to lib/sandbox.sh that adds a mount or a variable breaks the
# test of every command at once, which is the point.
#
# A command reads the whole account and says which part of it it does not
# give: tehas work adds the cycle's two of its own and the handoff names, and
# tehasbot review gives no token and none of a planning turn's. A test asserts
# against the list, so the library cannot hand a container something new
# quietly.

# The security options, as lib/sandbox.sh prints them: the same eight words for
# a cycle, a planning turn, a manual session, the push check and a review.
sandbox_security_words=("--cap-drop" "ALL" "--cap-add" "CHOWN" "--cap-add" "FOWNER" \
  "--security-opt" "no-new-privileges:true")

# Where lib/sandbox.sh gives a container its copy of the code index. Said here
# as well, so a test that pins a mount names the path without reading the
# library; test/sandbox_test.sh pins the two against each other.
sandbox_cache_dir_value=/home/vscode/.tehas-cache

# The two of lib/sandbox.sh's own that a cycle is given: the ceiling it runs the
# product's gate at, and the floor it promises to. A run that reads code and
# writes nothing is given neither, and says so where it names its variables.
sandbox_cycle_env=(PI_ENSEMBLE_MAX_WORKSTREAMS PI_ENSEMBLE_VERIFY_TIMEOUT_MS
  PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS)

# The variables of lib/sandbox.sh that are not the token and not one of the
# cycle's: its workspace root and code index, git's configuration and identity,
# and the prompt that must not be asked. A PI_ENSEMBLE_* of the environment is
# added to these by the library itself, so a test adds what its own run put
# into the environment.
sandbox_env_shared=(CBM_CACHE_DIR GIT_CONFIG_COUNT GIT_CONFIG_KEY_0 GIT_CONFIG_KEY_1
  GIT_CONFIG_KEY_2 GIT_CONFIG_KEY_3 GIT_CONFIG_VALUE_0 GIT_CONFIG_VALUE_1
  GIT_CONFIG_VALUE_2 GIT_CONFIG_VALUE_3 GIT_TERMINAL_PROMPT PI_ENSEMBLE_SANDBOX_MODE
  PI_ENSEMBLE_WORKSPACE_ROOT)

# The token's name: the one variable the sandbox library gives every container
# that a command may keep back. A review reads code and writes nothing to the
# tracker, so it is given no token and no -e carries its name at all.
sandbox_token_name=GH_TOKEN

# The names of every -e option of a docker run call, sorted and without
# duplicates. Options are read the way docker reads them, so an -e whose value
# begins with -e counts as one name and not two.
sandbox_all_env_names() { # (reads the arguments on standard input)
  awk '
    { line[NR] = $0 }
    END {
      for (i = 1; i <= NR; i++) {
        if (line[i] != "-e") continue
        v = line[i + 1]; sub(/=.*/, "", v)
        if (v != "-e") print v
      }
    }' | LC_ALL=C sort | uniq
}

# The docker options of a container that take a value, as the commands pass
# them. Read by sandbox_unknown_options: anything else before the image name is
# an option no command of tehas is known to pass, so a mount or a variable
# spelled another way (--volume, --mount, --env, -eNAME) shows up there. The
# options that take nothing are --rm and --init, the two every container gets.
sandbox_valued_options="--name --cap-drop --cap-add --security-opt --user -w -v -e --cgroup-parent \
  --memory --memory-swap --memory-reservation --cpus --pids-limit --cpu-shares --cidfile --label"

# Every option of a docker run call that tehas does not recognise. An option
# that is not one of the two above, and is not one of the two every container
# is given, and is not an address the host aliases made, is an option no
# command of tehas passes.
sandbox_unknown_options() { # <image> (reads the arguments on standard input)
  awk -v image="$1" -v valued=" $sandbox_valued_options " '
    NR == 1 { next } # "run"
    skip { skip = 0; next }
    $0 == image { exit }
    index(valued, " " $0 " ") { skip = 1; next }
    index(" --rm --init ", " " $0 " ") { next }
    /^--add-host=[A-Za-z0-9.-]+:[0-9a-fA-F.:]+$/ { next }
    /^PI_ENSEMBLE_[A-Z0-9_]*(=.*)?$/ { next }
    /^GIT_CONFIG_[A-Z0-9_]*=.*$/ { next }
    { print }'
}

# The mounts lib/sandbox.sh gives a container, with the four paths of its own
# filled in: <host> the workspace on the host, <inside> the path the container
# has it at, <state> the run's own directory beside it, <runs> its transcripts.
# The same eight for every kind of container, so a command that mounts one more
# or one less fails against this list rather than a copy of it. Pi's two files
# are read from <home>, named by the caller because a test's home is not the
# home of the shell that runs the test.
sandbox_mounts() { # <home> <host> <inside> <state> <runs>
  local home=$1 host=$2 inside=$3 state=$4 runs=$5
  printf '%s\n' "$host:$inside" \
    "$state/cache:$sandbox_cache_dir_value" \
    "$state/memory:/home/vscode/.vipune" \
    "$state/sessions:/home/vscode/.pi/agent/sessions" \
    "$runs:/home/vscode/.pi/agent/ensemble-runs" \
    "$state/models.json:/home/vscode/.pi/agent/models.json:ro" \
    "$home/.pi/agent/mcp.json:/home/vscode/.pi/agent/mcp.json:ro" \
    "$home/.pi/agent/settings.json:/home/vscode/.pi/agent/settings.json:ro"
}

# The values after one option, as docker reads them: every -v, every -e.
sandbox_after() { # <flag> (reads the arguments on standard input)
  awk -v flag="$1" 'prev == flag { print } { prev = $0 }'
}

# The same eight words of the security options, in order, as one line. Empty
# when a container is given them another way or not at all.
sandbox_security_line() {
  awk -v words="${sandbox_security_words[*]}" '
    BEGIN { n = split(words, want, " ") }
    { arg[NR] = $0 }
    END {
      for (i = 1; i <= NR; i++) {
        if (seen >= n) break
        if (arg[i] != want[seen + 1]) continue
        for (j = 1; j <= n; j++) if (arg[i + j - 1] != want[j]) break
        if (j > n) { seen = n; start = i }
      }
      if (seen == n) for (i = start; i < start + n; i++) printf "%s ", arg[i]
    }'
}
