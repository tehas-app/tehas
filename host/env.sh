# shellcheck shell=bash
# Tehas environment. Sourced from ~/.bashrc and by the systemd units.
#
# This file is the same on every host. What differs between instances is in
# <instance dir>/instance.env, loaded at the end through lib/config.sh.
export TEHAS_HOME="${TEHAS_HOME:-$HOME/tehas}"
export TMPDIR="$TEHAS_HOME/tmp"
# The tool homes and their bin directories come from lib/paths.sh, the same
# list install.sh and bootstrap.sh use, installed beside this file. A home that
# has this one without that one is an install that did not finish: --check says
# so, and the rest of this file still leaves a usable shell.
if [ -r "$TEHAS_HOME/lib/paths.sh" ]; then
  # shellcheck source=lib/paths.sh
  . "$TEHAS_HOME/lib/paths.sh"
  tehas_tool_paths
fi
# The model endpoints' tokens (TEHAS_MODEL_KEY_*, named by models.json). The
# file is chmod 600 and stays outside the tehas tree.
# shellcheck source=/dev/null
if [ -r "$HOME/.config/tehas/env" ]; then set -a; . "$HOME/.config/tehas/env"; set +a; fi
# Instance settings: the served repository, who may label, limits, the model
# for every pi-rukas role, the sandbox image. A broken instance.env prints one
# line here and leaves the shell usable; the commands refuse to run until it
# is fixed.
if [ -r "$TEHAS_HOME/lib/config.sh" ]; then
  # shellcheck source=lib/config.sh
  . "$TEHAS_HOME/lib/config.sh"
  tehas_load || true
fi
# The GitHub App: gh and git act as the app, and a user token left in the
# environment is dropped so nothing acts with it.
if [ -r "$TEHAS_HOME/lib/github-app.sh" ]; then
  # shellcheck source=lib/github-app.sh
  . "$TEHAS_HOME/lib/github-app.sh"
  tehas_github_app_env
fi
# A read-only instance (TEHAS_READ_ONLY=1) sends every gh through the shim: the
# console behind it refuses each write (#608).
if [ "${TEHAS_READ_ONLY:-0}" = 1 ]; then
  case ":$PATH:" in *":$TEHAS_HOME/shim:"*) ;; *) export PATH="$TEHAS_HOME/shim:$PATH" ;; esac
fi
# Energy use on the dashboard: tehasbot energy samples the model mesh when it
# is a viiwork mesh, and the section appears by itself. TEHAS_ENERGY=off removes
# it (the file tells every visitor the machines' names, their draw and their
# models). TEHAS_ENERGY_URL names the cluster URL when it is not the model
# provider's own, for example "http://mesh.example:8086/v1/cluster".
# TEHAS_ENERGY_SOURCE_URL is the address the page links under DATA SOURCE; only
# an https:// address is used, and with none set the page shows no link. Set
# either in ~/.config/tehas/env, not here: install.sh replaces this file.
# The service reads these when it starts: restart tehas-energy.service after a change.
