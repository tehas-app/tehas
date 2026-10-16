#!/usr/bin/env bash
# Tests for the Dockerfiles this repository ships. docker build continues an
# instruction only when the line ends with a backslash, so a line that was
# meant to continue is read as a new instruction and the build stops with an
# unknown-instruction error before it runs anything. No other step of the gate
# hands a Dockerfile to a parser: the shell tests stub docker, so this one
# reads the files themselves.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

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
t_contains "$(cat "$dockerfile")" 'ARG TEHAS_RUKAS_IMAGES=unknown'
t_contains "$(cat "$dockerfile")" 'LABEL tehas.rukas=$PI_RUKAS_COMMIT tehas.rukas.images=$TEHAS_RUKAS_IMAGES'
t_contains "$(cat "$build")" '--build-arg TEHAS_RUKAS_IMAGES="$reads"'
t_contains "$(cat "$dockerfile")" '/opt/pi-rukas/bin/pi-rukas-headless --capabilities'
t_contains "$(cat "$dockerfile")" '.name == "PI_ENSEMBLE_ISSUE_IMAGES" and .manifestVersion == 1'
t_contains "$(cat "$T_ROOT/lib/headless.sh")" '.name == "PI_ENSEMBLE_ISSUE_IMAGES" and .manifestVersion == 1'

t_done
