#!/usr/bin/env bash
# Tests for lib/paths.sh: the tool directories it puts first on PATH, that a
# second call adds nothing, that a home the caller set survives the call, and
# that host/env.sh and lib/config.sh take the same list and the same TEHAS_HOME.
# shellcheck disable=SC2016  # the snippets run in the child shell t_run starts
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

home=$T/home
th=$home/tehas
# Only the stubs and the base system: a tool of the person running the tests
# must not be found.
T_PATH=$T/bin:/usr/bin:/bin
# The five directories, in the order tehas_tool_paths names them.
want="$th/bin:$th/tools/node/bin:$home/.local/bin:$home/.bun/bin:$home/.cargo/bin"
T_ENV=(PATH="$T_PATH")

t_case "tehas_tool_paths puts the tool directories first, in order"
t_run '. "$T_ROOT/lib/paths.sh"; tehas_tool_paths; printf "%s\n" "$PATH"'
t_eq "$t_out" "$want:$T_PATH"

t_case "tehas_tool_paths exports the home of each tool"
t_run '. "$T_ROOT/lib/paths.sh"; tehas_tool_paths; printf "%s %s %s %s %s\n" "$TEHAS_HOME" "$NPM_CONFIG_PREFIX" "$BUN_INSTALL" "$CARGO_HOME" "$RUSTUP_HOME"'
t_eq "$t_out" "$th $home/.local $home/.bun $home/.cargo $home/.rustup"

t_case "calling tehas_tool_paths twice changes PATH once"
t_run '. "$T_ROOT/lib/paths.sh"; tehas_tool_paths; tehas_tool_paths; printf "%s\n" "$PATH"'
t_eq "$t_out" "$want:$T_PATH"

# The shape a host actually sees: env.sh gives the login shell its PATH, and
# the install or update that shell then runs calls the same function again.
t_case "a PATH that already holds the directories is left as it is"
t_run '. "$T_ROOT/lib/paths.sh"; tehas_tool_paths; printf "%s\n" "$PATH"'
once=$t_out
T_ENV=(PATH="$once")
t_run '. "$T_ROOT/lib/paths.sh"; tehas_tool_paths; printf "%s\n" "$PATH"'
t_eq "$t_out" "$once"
T_ENV=(PATH="$T_PATH")

t_case "a tool home the caller set wins over the default"
T_ENV=(PATH="$T_PATH" CARGO_HOME="$T/other-cargo" NPM_CONFIG_PREFIX="$T/other-local")
t_run '. "$T_ROOT/lib/paths.sh"; tehas_tool_paths; printf "%s %s\n" "$CARGO_HOME" "$PATH"'
t_eq "$t_out" "$T/other-cargo $th/bin:$th/tools/node/bin:$T/other-local/bin:$home/.bun/bin:$T/other-cargo/bin:$T_PATH"
T_ENV=(PATH="$T_PATH")

# The installed pair: env.sh is the copy a login shell and every unit source,
# and it holds no list of its own.
t_case "env.sh gives a login the list lib/paths.sh names"
mkdir -p "$th/lib"
cp "$T_ROOT/host/env.sh" "$th/env.sh"
cp "$T_ROOT/lib/paths.sh" "$th/lib/paths.sh"
t_run '. "$TEHAS_HOME/env.sh"; printf "%s\n" "$PATH"'
t_eq "$t_out" "$want:$T_PATH"

# Three files give TEHAS_HOME the same default: this one, the settings loader
# every command sources, and the login environment.
t_case "lib/paths.sh, lib/config.sh and host/env.sh default TEHAS_HOME alike"
t_run 'unset TEHAS_HOME; . "$T_ROOT/lib/paths.sh"; tehas_tool_paths; printf "%s\n" "$TEHAS_HOME"'
t_eq "$t_out" "$th"
t_instance "${T_MINIMAL[@]}"
t_run 'unset TEHAS_HOME; . "$T_ROOT/lib/config.sh"; tehas_load >/dev/null 2>&1; printf "%s\n" "$TEHAS_HOME"'
t_eq "$t_out" "$th"
t_run 'unset TEHAS_HOME; . "$T/home/tehas/env.sh"; printf "%s\n" "$TEHAS_HOME"'
t_eq "$t_out" "$th"

t_done
