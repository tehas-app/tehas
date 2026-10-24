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
# Bot credentials (GH_TOKEN) and the model endpoints' tokens (TEHAS_MODEL_KEY_*,
# named by models.json). File is chmod 600 and stays outside the tehas tree.
# shellcheck disable=SC1091
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
# The GitHub App, when the instance names one: gh and git act as the app, and a
# user token left in ~/.config/tehas/env is dropped so nothing acts with it.
if [ -n "${TEHAS_GITHUB_APP_ID:-}" ] && [ -r "$TEHAS_HOME/lib/github-app.sh" ]; then
  # shellcheck source=lib/github-app.sh
  . "$TEHAS_HOME/lib/github-app.sh"
  tehas_github_app_env
fi
# Energy use on the dashboard: tehasbot energy samples the model mesh when it
# is a viiwork mesh, and the section appears by itself. TEHAS_ENERGY=off removes
# it (the file tells every visitor the machines' names, their draw and their
# models). TEHAS_ENERGY_URL names the cluster URL when it is not the model
# provider's own, for example "http://mesh.example:8086/v1/cluster". Set either
# in ~/.config/tehas/env, not here: install.sh replaces this file.
# The service reads these when it starts: restart tehas-energy.service after a change.
