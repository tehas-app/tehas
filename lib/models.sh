# shellcheck shell=bash
# The endpoint each model provider is reached at, and the token it is asked
# with. A provider in models.json may name a second endpoint that serves the
# same models, and the token for each:
#
#   "baseUrl": "https://llm.example/v1",          "apiKey": "$TEHAS_MODEL_KEY_MAIN",
#   "backupBaseUrl": "https://fleet.example/v1",  "backupApiKey": "$TEHAS_MODEL_KEY_FLEET"
#
# An apiKey is a word without "$" or "!" (such as "unused"), or
# "$TEHAS_MODEL_KEY_<NAME>": a variable set in ~/.config/tehas/env, never in the
# instance directory, which is in a repository. backupApiKey defaults to apiKey.
# Pi reads the same "$NAME" form from its environment.
#
# Only the backup is new to Pi. A cycle's container is given a models.json of
# its own with every provider on the endpoint picked at its start, and without
# the backup keys. A cycle stays on its endpoint: one that fails mid-cycle is
# retried by Pi, not switched.
#
#   tehas_models_file                    the host's models.json
#   tehas_model_keys_ok [<file>]         every apiKey is a word or a set
#                                        TEHAS_MODEL_KEY_ name; else 78
#   tehas_model_key_names [<file>]       those names, one per line
#   tehas_endpoint_ask <url> <key> <model>...
#                                        ask <url>/models. Returns 0 when it
#                                        offers every model; else prints why
#   tehas_endpoint_pick <provider> [<file>]
#                                        "main" or "backup": main when it offers
#                                        the provider's models, else backup when
#                                        it does, else main. No call without a
#                                        backupBaseUrl
#   tehas_endpoint_url <provider> <main|backup> [<file>]
#   tehas_endpoint_key <provider> <main|backup> [<file>]
#   tehas_models_resolve <out> [<file>]  write <file> to <out> with each
#                                        provider on its picked endpoint; print
#                                        "<provider> backup" for each on one
#
# Source lib/config.sh and call tehas_load first. Sourcing this does nothing.

tehas_models_file() { printf '%s\n' "$HOME/.pi/agent/models.json"; }

tehas_model_key_names() { # [<file>]
  jq -r '.providers // {} | .[] | (.apiKey, .backupApiKey) | strings
    | select(startswith("$")) | ltrimstr("$") | ltrimstr("{") | rtrimstr("}")' \
    "${1:-$(tehas_models_file)}" 2>/dev/null | sort -u
}

tehas_model_keys_ok() { # [<file>]
  local file=${1:-$(tehas_models_file)} bad name
  bad=$(jq -r '.providers // {} | to_entries[] | .key as $p | .value
    | (.apiKey, .backupApiKey) | strings
    | select(test("^[$][{]?TEHAS_MODEL_KEY_[A-Z0-9_]+[}]?$") or (test("[$!]") | not) | not)
    | $p' "$file" 2>/dev/null | head -n 1)
  if [ -n "$bad" ]; then
    tehas_err "models.json: provider $bad has an apiKey that is neither a word nor \$TEHAS_MODEL_KEY_<NAME> (see $file)"
    return 78
  fi
  while read -r name; do
    [ -n "$name" ] || continue
    if [ -z "${!name:-}" ]; then
      tehas_err "models.json names $name and it is not set: put it in ~/.config/tehas/env"
      return 78
    fi
  done < <(tehas_model_key_names "$file")
  return 0
}

tehas_endpoint_url() { # <provider> <main|backup> [<file>]
  local k=baseUrl
  [ "$2" = main ] || k=backupBaseUrl
  jq -r --arg p "$1" --arg k "$k" '.providers[$p][$k] // "" | strings' "${3:-$(tehas_models_file)}" 2>/dev/null
}

tehas_endpoint_key() { # <provider> <main|backup> [<file>]
  jq -r --arg p "$1" --arg e "$2" '.providers[$p]
    | (if $e == "main" then .apiKey else .backupApiKey // .apiKey end) // "" | strings' \
    "${3:-$(tehas_models_file)}" 2>/dev/null
}

# A curl config on stdout that sends the token <key> names, or nothing for a
# word. The token goes through a file descriptor, never a command line.
_tehas_endpoint_auth() { # <key>
  local name=${1#\$}
  name=${name#\{}
  name=${name%\}}
  [ "$name" != "$1" ] || return 0
  [ -n "${!name:-}" ] || return 0
  printf 'header = "Authorization: Bearer %s"\n' "${!name}"
}

tehas_endpoint_ask() { # <url> <key> <model>...
  local url=$1 key=$2 list m
  shift 2
  list=$(curl -fsS -K <(_tehas_endpoint_auth "$key") -m 10 "$url/models" 2>/dev/null) ||
    { echo "does not answer"; return 1; }
  jq -e '(.data | type) == "array" and (.data | length) > 0' <<<"$list" >/dev/null 2>&1 ||
    { echo "offers no models"; return 1; }
  for m in "$@"; do
    jq -e --arg m "$m" '.data | any(.id == $m)' <<<"$list" >/dev/null 2>&1 ||
      { echo "does not offer $m"; return 1; }
  done
  return 0
}

# The models the agents use on <provider>.
_tehas_provider_models() { tehas_models | awk -v p="$1" '$1 == p { print $2 }'; }

tehas_endpoint_pick() { # <provider> [<file>]
  local p=$1 file=${2:-$(tehas_models_file)} backup
  local -a models
  backup=$(tehas_endpoint_url "$p" backup "$file")
  [ -n "$backup" ] || { echo main; return 0; }
  mapfile -t models < <(_tehas_provider_models "$p")
  if tehas_endpoint_ask "$(tehas_endpoint_url "$p" main "$file")" "$(tehas_endpoint_key "$p" main "$file")" "${models[@]}" >/dev/null; then
    echo main
  elif tehas_endpoint_ask "$backup" "$(tehas_endpoint_key "$p" backup "$file")" "${models[@]}" >/dev/null; then
    echo backup
  else
    echo main
  fi
}

tehas_models_resolve() { # <out> [<file>]
  local out=$1 file=${2:-$(tehas_models_file)} p json
  json=$(jq -c 'del(.providers[]? | .backupBaseUrl, .backupApiKey)' "$file") || return 1
  while read -r p; do
    [ "$(tehas_endpoint_pick "$p" "$file")" = backup ] || continue
    json=$(jq -c --arg p "$p" --arg u "$(tehas_endpoint_url "$p" backup "$file")" \
      --arg k "$(tehas_endpoint_key "$p" backup "$file")" \
      '.providers[$p].baseUrl = $u | if $k == "" then . else .providers[$p].apiKey = $k end' <<<"$json") || return 1
    echo "$p backup"
  done < <(jq -r '.providers // {} | to_entries[] | select(.value.backupBaseUrl | type == "string") | .key' "$file")
  jq . <<<"$json" >"$out.new" && mv "$out.new" "$out"
}
