#!/usr/bin/env bash
# Tests for the Dockerfiles this repository ships. docker build continues an
# instruction only when the line ends with a backslash, so a line that was
# meant to continue is read as a new instruction and the build stops with an
# unknown-instruction error before it runs anything. No other step of the gate
# hands a Dockerfile to a parser: the shell tests stub docker, so this one
# reads the files themselves.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

# Every instruction Docker knows, as its parser names it.
instructions="FROM RUN CMD LABEL MAINTAINER EXPOSE ENV ADD COPY ENTRYPOINT VOLUME USER WORKDIR ONBUILD STOPSIGNAL HEALTHCHECK ARG SHELL"

# parse <file>: one line per instruction Docker would refuse, naming the line
# the instruction starts on. A trailing backslash joins the next line, as the
# parser does; without one the instruction ends, and its first word has to be
# one Docker knows.
parse() { # <file>
  local file=$1 line joined="" instr start=0 n=0 open=0 bad=0 bs=$'\\'
  while IFS= read -r line || [ -n "$line" ]; do
    n=$((n + 1))
    line=${line%$'\r'}
    while [ "${line%[[:space:]]}" != "$line" ]; do line=${line%[[:space:]]}; done
    while [ "${line#[[:space:]]}" != "$line" ]; do line=${line#[[:space:]]}; done
    if [ "$open" = 0 ]; then
      case "$line" in '' | '#'*) continue ;; esac
      joined=$line
      start=$n
    else
      joined+=" $line"
    fi
    case "$joined" in
      *"$bs")
        joined=${joined%"$bs"}
        open=1
        ;;
      *)
        instr=${joined%%[[:space:]]*}
        case " $instructions " in
          *" ${instr^^} "*) ;;
          *)
            echo "$file:$start: unknown instruction: ${instr^^}"
            bad=1
            ;;
        esac
        open=0
        ;;
    esac
  done <"$file"
  [ "$open" = 0 ] || echo "$file:$start: the instruction ends with a continuation and nothing follows"
  return "$bad"
}

# The RUN of the sandbox image, with the continuations left out: the parser
# sees three instructions where the build meant one.
t_case "a RUN that forgets its continuations is named, not read as more instructions"
mkdir -p "$T/dockerfile"
cat >"$T/dockerfile/Dockerfile" <<'EOF'
FROM base
RUN given=${X:-unknown}; reads=0;
    if test -e /opt/x; then reads=1; fi;
    if [ "$given" != "$reads"; then exit 1; fi
EOF
t_eq "$(parse "$T/dockerfile/Dockerfile")" \
  "$T/dockerfile/Dockerfile:3: unknown instruction: IF
$T/dockerfile/Dockerfile:4: unknown instruction: IF"

t_case "the same RUN with its continuations parses"
cat >"$T/dockerfile/Dockerfile" <<'EOF'
FROM base
RUN given=${X:-unknown}; reads=0; \
    if test -e /opt/x; then reads=1; fi; \
    if [ "$given" != "$reads"; then exit 1; fi
EOF
t_eq "$(parse "$T/dockerfile/Dockerfile")" ""

t_case "a continuation with nothing after it is named"
cat >"$T/dockerfile/Dockerfile" <<'EOF'
FROM base
RUN echo one \
EOF
t_contains "$(parse "$T/dockerfile/Dockerfile")" "ends with a continuation and nothing follows"

t_case "every Dockerfile in this repository parses"
found=""
for f in $(git -C "$T_ROOT" ls-files | grep -i 'Dockerfile$'); do
  found="$found $f"
  t_eq "$(parse "$T_ROOT/$f")" ""
done
t_contains "$found" "host/sandbox/Dockerfile"

# The label is asked for by the build, declared by the image and written from
# the same answer: the base's --capabilities. A drift between them is a label
# the host reads as something else.
t_case "the images label is asked for, declared and written by the same names"
dockerfile=$T_ROOT/host/sandbox/Dockerfile
build=$T_ROOT/host/sandbox/build.sh
recipe=$(cat "$dockerfile")
t_contains "$(cat "$dockerfile")" 'ARG TEHAS_RUKAS_IMAGES=unknown'
t_contains "$(cat "$dockerfile")" 'LABEL tehas.rukas=$PI_RUKAS_COMMIT tehas.rukas.images=$TEHAS_RUKAS_IMAGES'
t_contains "$(cat "$build")" '--build-arg TEHAS_RUKAS_IMAGES="$reads"'
t_contains "$(cat "$dockerfile")" '/opt/pi-rukas/bin/pi-rukas-headless --capabilities'
t_contains "$(cat "$dockerfile")" '.name == "PI_ENSEMBLE_ISSUE_IMAGES" and .manifestVersion == 1'
t_contains "$(cat "$T_ROOT/lib/headless.sh")" '.name == "PI_ENSEMBLE_ISSUE_IMAGES" and .manifestVersion == 1'

# --- What the image has to carry --------------------------------------------
# A cycle reaches for the tools its instructions name. Each of these cases is
# about a tool an agent was seen reaching for and not finding, or one that was
# found and broken (issue #261).

t_case "a cycle can look at a picture: file and a Python image library are installed"
t_contains "$recipe" 'apt-get install -y -qq --no-install-recommends file python3-pil'
t_contains "$recipe" "python3 -c 'import PIL'"
# The apt lists the step fetched do not stay in the image.
t_eq "$(grep -c 'rm -rf /var/lib/apt/lists/\*' "$dockerfile")" 2

t_case "the wigolo the base ships broken is off PATH, and no tehas prompt names it"
t_contains "$recipe" 'rm -rf /usr/lib/node_modules/wigolo'
t_contains "$recipe" 'if command -v wigolo >/dev/null 2>&1; then'
# The removal is not complete when a launcher survives somewhere else on PATH.
t_contains "$(sed -n '/rm -rf \/usr\/lib\/node_modules\/wigolo/,/wigolo is off PATH/p' "$dockerfile")" 'exit 1'
# Only what a cycle runs is searched: the units and the prompts in lib/ and
# host/pi. The manual names wigolo to say why it is gone. pi-rukas's own recipe
# preflights the command and says when it is not there.
t_eq "$(grep -rl -e wigolo "$T_ROOT/lib" "$T_ROOT/host/pi" 2>/dev/null | wc -l)" 0

t_case "the code indexer in the image is the version versions.env names, not upstream's main"
versions=$T_ROOT/host/versions.env
t_eq "$(grep -cE '^CBM_VERSION=[0-9]+\.[0-9]+\.[0-9]+$' "$versions")" 1
t_eq "$(grep -cE '^CBM_INSTALLER_SHA256=[0-9a-f]{64}$' "$versions")" 1
t_contains "$recipe" 'raw.githubusercontent.com/DeusData/codebase-memory-mcp/v${CBM_VERSION}/install.sh'
t_lacks "$recipe" 'codebase-memory-mcp/main/install.sh'
t_contains "$recipe" 'echo "${CBM_INSTALLER_SHA256}  /tmp/cbm-install.sh" | sha256sum -c -'
t_contains "$recipe" '--skip-config'
# The installer's own activation cannot publish onto the image's overlayfs when
# Docker runs inside an unprivileged Incus container, as a factory's does. It
# installs into a tmpfs instead, made 755 because the installer refuses a
# directory others can write, and only the binary is copied into place. So the
# updater file it leaves beside the binary never reaches PATH.
t_contains "$recipe" 'RUN --mount=type=tmpfs,target=/cbm'
t_contains "$recipe" 'chmod 755 /cbm'
t_contains "$recipe" '--dir=/cbm'
t_lacks "$recipe" '--dir=/usr/local/bin'
t_contains "$recipe" 'install -m 755 /cbm/codebase-memory-mcp /usr/local/bin/codebase-memory-mcp'
# The pinned copy replaces the one the base put in /usr/local/bin, and the
# version is read back from it. The base sets HOME for every user, so this step
# names its own.
t_contains "$recipe" 'env HOME=/root CBM_DOWNLOAD_URL='
t_contains "$recipe" 'rm -f /tmp/cbm-install.sh'
t_lacks "$recipe" '/home/vscode/.local'
t_contains "$recipe" 'codebase-memory-mcp is ${found:-not there}, versions.env names $CBM_VERSION'
t_contains "$(cat "$build")" '--build-arg CBM_VERSION="$CBM_VERSION"'
t_contains "$(cat "$build")" '--build-arg CBM_INSTALLER_SHA256="$CBM_INSTALLER_SHA256"'

t_case "the image carries vipune's embedding model, pinned, and proves vipune reads it offline"
t_eq "$(grep -cE '^VIPUNE_MODEL=[A-Za-z0-9_-]+/[A-Za-z0-9._-]+$' "$versions")" 1
t_eq "$(grep -cE '^VIPUNE_MODEL_REVISION=[0-9a-f]{40}$' "$versions")" 1
t_eq "$(grep -cE '^VIPUNE_MODEL_SHA256_ONNX=[0-9a-f]{64}$' "$versions")" 1
t_eq "$(grep -cE '^VIPUNE_MODEL_SHA256_TOKENIZER=[0-9a-f]{64}$' "$versions")" 1
t_contains "$recipe" 'url="https://huggingface.co/$VIPUNE_MODEL/resolve/$VIPUNE_MODEL_REVISION"'
t_contains "$recipe" 'echo "${VIPUNE_MODEL_SHA256_ONNX}  $snap/onnx/model.onnx" | sha256sum -c -'
t_contains "$recipe" 'echo "${VIPUNE_MODEL_SHA256_TOKENIZER}  $snap/tokenizer.json" | sha256sum -c -'
# hf_hub finds the snapshot through the ref named after the revision asked for.
t_contains "$recipe" 'printf %s "$VIPUNE_MODEL_REVISION" >"$repo/refs/$VIPUNE_MODEL_REVISION"'
t_contains "$recipe" '/home/vscode/.cache/huggingface/hub/models--'
# The gate runs as the container's user with the network shut off, and fails on
# an error or an empty answer rather than only on an exit status.
gate_step=$(sed -n '/^USER vscode$/,/^USER root$/p' "$dockerfile")
t_contains "$gate_step" 'export HF_HUB_OFFLINE=1 HF_ENDPOINT=http://127.0.0.1:9 https_proxy=http://127.0.0.1:9'
t_contains "$gate_step" "jq -e '.status == \"added\"'"
t_contains "$gate_step" "jq -e '.results | length > 0'"
t_contains "$gate_step" 'versions.env names $VIPUNE_VERSION'
# Each vipune call names the scratch store: one in ~/.vipune would be left in
# the image, under the directory a cycle mounts its own store on.
calls=$(grep -E 'vipune (version|add|search) ' <<<"$gate_step")
t_eq "$(wc -l <<<"$calls")" 3
t_eq "$(grep -c -- '--db-path "$db/gate.db"' <<<"$calls")" 3
t_contains "$(cat "$build")" '--build-arg VIPUNE_MODEL_SHA256_ONNX="$VIPUNE_MODEL_SHA256_ONNX"'

t_case "the image refuses a build that is missing a tool a cycle reaches for"
gate=$(sed -n 's/^RUN for tool in \(.*\); do.*$/\1/p' "$dockerfile")
t_eq "$(wc -w <<<"$gate")" 16
for tool in file python3 rg git jq curl gh node cargo shellcheck codebase-memory-mcp fd find bash pi /opt/pi-rukas/bin/pi-rukas-headless; do
  case " $gate " in *" $tool "*) ;; *) t_bad "$tool is not one of the tools the image checks" ;; esac
done
t_lacks " $gate " ' wigolo '
t_contains "$recipe" 'is not in this image'

# The command each unit in <dir> gives its container. In a docker run it is the
# word after --entrypoint when there is one, and the word after the image itself
# when there is not: with an --entrypoint, what follows the image is that
# command's arguments. A line ending with a backslash continues, and the joined
# line keeps one space between the words, so a command on its own continued line
# is still seen. Only a docker run counts: a unit that names the image in a
# message starts no container. A command named through a variable (work's
# runner) is resolved from its assignment in the same directory; one with no
# assignment there is reported as the variable itself, so the check below says
# so rather than passing without it.
container_cmds() { # <dir of units>
  local cmd name resolved value
  awk '
    function strip(t) { sub(/^"/, "", t); sub(/"$/, "", t); return t }
    function is_image(t) {
      t = strip(t)
      return t == "$image" || t == "${image}" || t == "$PI_ENSEMBLE_IMAGE" || t == "${PI_ENSEMBLE_IMAGE}"
    }
    {
      stmt = $0
      while (stmt ~ /\\$/) {
        if ((getline nxt) <= 0) break
        sub(/\\$/, "", stmt)
        sub(/^[[:space:]]+/, "", nxt)
        stmt = stmt " " nxt
      }
      if (stmt !~ /(^|[[:space:]])docker run([[:space:]]|$)/) next
      n = split(stmt, w, /[[:space:]]+/)
      cmd = ""
      for (i = 1; i <= n; i++) {
        if (w[i] == "--entrypoint") { cmd = strip(w[i + 1]); break }
        if (cmd == "" && is_image(w[i])) cmd = strip(w[i + 1])
      }
      if (cmd != "") print cmd
    }
  ' "$1"/* |
    grep -E -- '^[/A-Za-z][A-Za-z0-9_./-]*$|^\$\{?[A-Za-z_][A-Za-z0-9_]*\}?$' | LC_ALL=C sort -u |
    while read -r cmd; do
      case "$cmd" in
        '$'*)
          name=${cmd#'$'}
          name=${name#'{'}
          name=${name%'}'}
          resolved=$(grep -h -oE -- "(^|[[:space:]])${name}=[^[:space:]]+" "$1"/* |
            while read -r assign; do
              value=${assign#*=}
              value=${value#'"'}
              printf '%s\n' "${value%'"'}"
            done)
          printf '%s\n' "${resolved:-$cmd}"
          ;;
        *) printf '%s\n' "$cmd" ;;
      esac
    done | LC_ALL=C sort -u
}

# Each of these shapes hid a command from the first version of the scan. They
# are checked here, in a directory of their own, so a shape that stops matching
# fails on the shape and not only on the units.
t_case "the scan reads a container command from a continuation, an --entrypoint and a name"
mkdir -p "$T/libexec"
cat >"$T/libexec/one" <<'EOF'
timeout -k 30 9 docker run --rm --init \
  -e HOME=/home/vscode \
  "$image" \
  fixture-one --flag
EOF
cat >"$T/libexec/two" <<'EOF'
runner_two=/opt/fixture/bin/runner-two
timeout -k 9 docker run --rm --init \
  --entrypoint "$runner_two" "$image" \
  sub --dry-run
EOF
cat >"$T/libexec/three" <<'EOF'
docker run --rm --init "$image" /opt/fixture/bin/three --json
echo "tehas three: $image gave no answer" >&2
EOF
cat >"$T/libexec/four" <<'EOF'
timeout -k 9 docker run --rm \
  --entrypoint "${runner_four}" "$image" cli arg
EOF
cat >"$T/libexec/five" <<'EOF'
runner_four=/opt/fixture/bin/four-brace
docker run --rm --init "$image" "$no_assignment" work 1
EOF
cmds=$(container_cmds "$T/libexec")
t_eq "$cmds" '$no_assignment
/opt/fixture/bin/four-brace
/opt/fixture/bin/runner-two
/opt/fixture/bin/three
fixture-one'
# What follows an --entrypoint is arguments, and a message that names the image
# starts no container.
found=" $(tr '\n' ' ' <<<"$cmds") "
t_lacks "$found" ' sub '
t_lacks "$found" ' cli '
t_lacks "$found" ' gave '

t_case "every command tehas gives a container is one the image checks"
cmds=$(container_cmds "$T_ROOT/libexec")
# The derivation is not vacuous: these are what today's libexec/ names, and a
# scan that stopped matching would fail here instead of passing with an empty
# list. improve names pi on its own continued line, so it is found on its own.
for cmd in /opt/pi-rukas/bin/pi-rukas-headless codebase-memory-mcp git pi; do
  case "
$cmds
" in *"
$cmd
"*) ;; *) t_bad "$cmd is not among the container commands the scan found: $cmds" ;; esac
done
missing=""
while read -r cmd; do
  [ -n "$cmd" ] || continue
  case " $gate " in *" $cmd "*) ;; *) missing="$missing $cmd" ;; esac
done <<<"$cmds"
[ -z "$missing" ] || t_bad "$missing runs in a container and the image does not check it"

t_done
