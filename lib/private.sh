# shellcheck shell=bash
# The names the host knows that an agent can print, for a mask to hide: the
# host aliases, the host of every endpoint of every provider the agents use
# and the bot's email. They are names, not secrets, so they may go on a command
# line; a token never does. A name under 4 characters would mask ordinary text.
#
#   tehas_private_names <models.json> <bot email>
#                       one name per line. Needs the settings loaded
#                       (lib/config.sh, tehas_load) for the aliases and the
#                       providers

_tehas_private() { if [ "${#1}" -ge 4 ]; then printf '%s\n' "$1"; fi; }

tehas_private_names() { # <models.json> <bot email>
  local models=$1 email=$2 pair model_host model_provider pairs=()
  IFS=',' read -ra pairs <<<"${PI_ENSEMBLE_HOST_ALIASES:-}"
  for pair in "${pairs[@]}"; do
    _tehas_private "${pair%%:*}"
    [ "${pair#*:}" = "$pair" ] || _tehas_private "${pair#*:}"
  done
  # A role's own provider among them.
  while read -r model_host; do
    model_host=${model_host#*://}
    model_host=${model_host%%/*}
    model_host=${model_host##*@}
    case "$model_host" in
      \[*) model_host=${model_host#[}; model_host=${model_host%%]*} ;;
      *) model_host=${model_host%%:*} ;;
    esac
    _tehas_private "$model_host"
  done < <(tehas_models | while read -r model_provider _; do
    jq -r --arg p "$model_provider" '.providers[$p] | .baseUrl, .backupBaseUrl | strings' "$models" 2>/dev/null
  done)
  _tehas_private "$email"
}
