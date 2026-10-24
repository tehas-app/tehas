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

# The container of each unit in <dir>, as one record per docker run:
# <unit>TAB<line>TAB<image>TAB<command>, or <unit>TAB<line>TABunplacedTABTAB<why>
# when the scan cannot place one. A run is read only from the words of its own
# simple command, so the words of a command before it (the image of a run that
# came first, a message that names an image) are no part of the scan of it.
#
# The scan reads a unit as the shell does, so far as a test has to. A line whose
# last character is a backslash continues, and the words of the lines joined are
# the words of one statement. A word ends at a ; | ( ) or & that the shell reads
# as its own, and not at one held behind a quote, so the argument of a runner
# (-c "a | b") is one word. A run stands at the head of its command, behind the
# negation of a test, behind the program the unit runs it with (timeout) and that
# program's options, or after the words of a compound statement (then, do, else).
# Only a docker run counts, so the text of a comment names no run and a message
# that names the image starts no container. --entrypoint names the command; with
# no --entrypoint the command is the word after the image, and a word of the
# container written after the image (--live of a server) is no command. The
# options of docker run, and the few options of the runner the units name, carry
# the word after them as their value; a flag (--rm, --init) and the long words a
# runner reads from an image (--tools, --no-session) carry none, and are no more
# guessed at than they are read. An expansion names the image through its
# assignment in the same file, and a name the units only write, never assign
# (one that begins TEHAS_ or PI_, which lib/config.sh exports) is a setting of
# the instance directory, named as the unit names it: no default of one host is
# written into this test. The words of an array (${sec[@]}) are options or
# arguments, and no image of a run lies among them. An image built from parts
# the file reads (caddy@$digest, tehas-preview-$n) is no image the scan can join,
# and a name the file assigns nowhere is no image it may invent: both are
# reported with why, so the checks below say what is unplaced instead of passing
# without it.
container_records() { # <file>
  local unit
  unit=$(basename "$1")
  # The scanner is written to a file of its own and read from it, quoted from the
  # shell here: the words of the scan name the words of a unit, and no word of
  # this test is filled in behind them.
  cat >"$T/container_scan.awk" <<'AWK'
  function quotes(t,  n) {
    # How many double quotes a word holds. An odd number means the word is
    # not a whole one: the value it names continues into the words after it.
    n = gsub(/"/, "Q", t)
    return n
  }
  function record(t) {
    # One record of a run, kept for the account and printed when the file is read.
    tally(t)
    kept = kept + 1
    out_of[kept] = t
  }
  function unquote(t) {
    # The value a word names, with the quotes that hold it taken off: the
    # scan compares values, not the shells spelling of them.
    if (t ~ /^".*"$/ || t ~ /^\047.*\047$/) return substr(t, 2, length(t) - 2)
    return t
  }
  function name_of(t,  u) {
    # The name a variable word names: image for $image and for ${image},
    # with the quotes that hold it taken off.
    u = t
    if (u ~ /^".*"$/) u = substr(u, 2, length(u) - 2)
    if (u !~ /^\$\{?[A-Za-z_]/) return ""
    sub(/^\$\{/, "", u)
    sub(/\}$/, "", u)
    sub(/^\$/, "", u)
    return u
  }
  function is_var(t,  u) {
    # The word names a variable, as the shell reads it: "$image", ${image} and
    # $image alike, each with its $ or its ${. A word that holds anything more
    # (an array slice, a command substitution) is no name the scan can resolve,
    # and a word without a $ names none.
    u = unbrace(t)
    return u ~ /^\$\{?[A-Za-z_][A-Za-z0-9_]*\}?$/
  }
  function is_expansion(t) {
    # The word holds an expansion or a command substitution: a $ or a ( that
    # is no part of a plain name. Such a word either names a value the file
    # assigns, or it is shell syntax, which names no image.
    if (index(t, "$(") > 0) return 1
    if (index(t, "${") > 0) return 1
    if (index(t, "$") > 0) return 1
    return 0
  }
  function is_array(t) {
    # A word the units expand from an array of the shell, such as "${envs[@]}"
    # or ${sec[@]}: it stands for the options or arguments of many words, so it
    # is no image and no command. A scan that read one as a value would name the
    # option written after it as the image of the run.
    return array_word(t) != ""
  }
  function unbrace(t,  u) {
    # The word as the shell reads it, with the braces of an expansion taken off:
    # ${ limits [ @ ] } and "${limits[@]}" both reach it as ${limits[@]}. The
    # spaces a unit writes inside the braces are its own, and no part of a name.
    u = unquote(t)
    gsub(/[ \t]+/, "", u)
    return u
  }
  function array_word(t,  u) {
    # The name of the array a word expands, or nothing for a word that expands
    # none: ${limits[@]}, ${ limits [ @ ] }, and "$limits[@]" all name limits.
    # The spaces inside the braces are the shells own, and no part of the name.
    u = unbrace(t)
    if (u !~ /^\$\{?[A-Za-z_][A-Za-z0-9_]*\[[*@]\]\}?$/) return ""
    sub(/^\$\{?/, "", u)
    sub(/\[[*@]\]\}?$/, "", u)
    return u
  }
  function only_expansions(t,  u) {
    # Whether every expansion the word holds is a command substitution the shell
    # fills in, as the argument of --user is ($(id -u):$(id -g)). Such a word is
    # one argument, and the scan reads it whole rather than look past it for the
    # image of the run.
    u = unquote(t)
    if (index(u, "$(") == 0) return 0
    while (match(u, /\$\([^()]*\)/)) u = substr(u, 1, RSTART - 1) substr(u, RSTART + RLENGTH)
    return index(u, "$") == 0
  }
  function is_composite(t,  u) {
    # A word that builds a name from values the file reads: an image written from
    # a name and a digest or a tag (caddy@$digest, tehas-preview:$n). The scan
    # cannot join the parts, and says so rather than name half of an image. A word
    # whose parts are command substitutions only ($(id -u):$(id -g)) is one
    # argument the shell fills in, and the scan reads it whole.
    if (is_array(t)) return 0
    if (is_var(t)) return 0
    if (only_expansions(t)) return 0
    if (!is_expansion(t)) return 0
    # Take the $( ... ) of the word away, and what is left of a $ names a value
    # the file reads and the scan cannot join: a digest, a tag, a number. A name
    # the file assigns wholly ("$image") is not such a word, and is read by the
    # name through its assignment.
    u = unbrace(t)
    while (match(u, /\$\([^()]*\)/)) u = substr(u, 1, RSTART - 1) substr(u, RSTART + RLENGTH)
    if (u ~ /^\$\{?[A-Za-z_][A-Za-z0-9_]*\}?$/) return 0
    return index(u, "$") > 0
  }
  function cut(t, parts, nw,  n, j, c, out, nest, dq, k, ch) {
    # One word of the shells input, which may hold more than one word of the
    # statement: a ; | ( ) or & ends a word when it lies outside a double
    # quote and outside $( ... ), and && and || are one word each. What a
    # quote or a $( ... ) holds stays inside its own word, because what a unit
    # writes there is an argument and not a command of its own.
    n = length(t)
    out = ""
    j = 1
    nest = 0
    dq = 0
    while (j <= n) {
      c = substr(t, j, 1)
      if (dq) { out = out c; j = j + 1; continue }
      if (c == "\"") {
        if (nest > 0) { out = out c; j = j + 1; continue }
        # The rest of the word belongs to the quoted value, quotes and all, up
        # to the quote that closes it. A word of a unit that opens a quote and
        # never closes it (the $( ... ) of a value, whose quotes the shell
        # reads and the scan does not) keeps the rest of the word as well.
        k = index(substr(t, j + 1), "\"")
        if (k == 0) { out = out substr(t, j); break }
        out = out substr(t, j, k + 1)
        j = j + k + 1
        continue
      }
      if (nest > 0) {
        if (c == "(") nest++
        if (c == ")") nest--
        out = out c
        j = j + 1
        continue
      }
      if (redirect_here(t, j)) {
        # A redirect, whose words the unit gives to a file: the name of the file
        # with its > and its descriptor, and the whole of a quoted word that
        # opens with the > of a redirect ($">$log") for the shell reads no word
        # there either.
        nw = flush(out, parts, nw)
        out = ""
        k = j
        if (substr(t, k, 1) == ">") k = k + 1
        if (k <= n && substr(t, k, 1) == ">") k = k + 1
        if (k <= n && substr(t, k, 1) == "\"") {
          k = k + 1
          while (k <= n && substr(t, k, 1) != "\"") k = k + 1
          if (k <= n) k = k + 1
        }
        parts[++nw] = substr(t, j, k - j)
        j = k
        continue
      }
      if (c == "$") {
        # What a unit writes after a $ is its own, and the scan reads it whole:
        # the ( ... ) of a command substitution, the { ... } of a braced name or
        # an array, and the ( ) of a brace expansion (${a[,]}). A scan that cut
        # at those characters read half of one word as the word before it.
        if (substr(t, j + 1, 1) == "(") {
          nest = 1
          k = j + 2
          while (k <= n && nest > 0) {
            ch = substr(t, k, 1)
            if (ch == "(") nest++
            else if (ch == ")") nest--
            k = k + 1
          }
          out = out substr(t, j, k - j)
          j = k
          continue
        }
        if (substr(t, j + 1, 1) == "{") {
          nest = 1
          k = j + 2
          while (k <= n && nest > 0) {
            ch = substr(t, k, 1)
            if (ch == "{") nest++
            else if (ch == "}") nest--
            k = k + 1
          }
          out = out substr(t, j, k - j)
          j = k
          continue
        }
        out = out c
        j = j + 1
        continue
      }
      if (c == ";") { nw = flush(out, parts, nw); out = ""; parts[++nw] = ";"; j = j + 1; continue }
      if (c == "(" || c == ")") { nw = flush(out, parts, nw); out = ""; parts[++nw] = c; j = j + 1; continue }
      if (c == "&") {
        nw = flush(out, parts, nw)
        out = ""
        if (substr(t, j + 1, 1) == "&") { parts[++nw] = "&&"; j = j + 2; continue }
        parts[++nw] = "&"
        j = j + 1
        continue
      }
      if (c == "|") {
        nw = flush(out, parts, nw)
        out = ""
        if (substr(t, j + 1, 1) == "|") { parts[++nw] = "||"; j = j + 2; continue }
        parts[++nw] = "|"
        j = j + 1
        continue
      }
      out = out c
      j = j + 1
    }
    return flush(out, parts, nw)
  }
  function redirect_here(t, j) {
    # Whether the character at j of a word begins the > or >> of a redirect, and
    # so ends the word where the shell ends it: only what stands outside a quote
    # does, and only the start of a word does, or what a descriptor stands before.
    if (substr(t, j, 1) != ">") return 0
    if (j == 1) return 1
    return substr(t, j - 1, 1) ~ /[0-9]/
  }
  function flush(o, parts, nw) {
    if (o != "") parts[++nw] = o
    return nw
  }
  function join_words(stmt, raw, parts, words,  nr, i, nw, t) {
    # The words of the statement, read into the callers words array, which is
    # no other array of the caller: mawk lets a scalar of one function and an
    # array of another share a name, and the words of one scan would then cut
    # into the words of another. Each raw word is cut into the words it holds,
    # and a word that opens a double quote without closing it takes the words
    # after it until the quote closes, so a quoted value with a space in it is
    # one word and keeps its space.
    nr = split(stmt, raw, /[ \t]+/)
    nw = 0
    for (i = 1; i <= nr; i = i + 1) nw = cut(raw[i], parts, nw)
    i = 1
    nr = nw
    nw = 0
    while (i <= nr) {
      t = parts[i]
      if (t ~ /^"/ && quotes(t) % 2 == 1) {
        while (i < nr) {
          i = i + 1
          t = t " " parts[i]
          if (quotes(parts[i]) % 2 == 1) break
        }
      }
      words[++nw] = t
      i = i + 1
    }
    return nw
  }
  function ends_continued(t) {
    # Whether a line ends in the backslash that continues it: a backslash that
    # stands last but for the spaces after it.
    return t ~ /\\[ \t]*$/
  }
  function trim_head(t) {
    sub(/^[ \t]+/, "", t)
    return t
  }
  function ends_word(t) {
    # What ends the simple command a walk is in: the next command of the
    # statement, a pipeline, a subshell, or the > of a redirect with the
    # descriptor before it, whose words the unit gives to a file and not to the
    # run. A redirect is only one where the shell reads one: written at the start
    # of a word, or after its descriptor, and not held by a quote. A unit that
    # gives an argument the > of a file (>"$log") is read as what it is, and no
    # image of a run lies beyond the redirect the shell would have ended at.
    if (t == ";" || t == "|" || t == "&&" || t == "||" || t == "(" || t == ")") return 1
    if (t == "then" || t == "do" || t == "else" || t == "elif" || t == "fi" ||
        t == "done" || t == "esac" || t == "in") return 1
    if (!unquoted(t)) return 0
    if (t ~ /^[0-9]*(<|<<|<<<)/) return 1
    if (t !~ /^[0-9]*>/) return 0
    # The > of a redirect reaches the word unquoted: a word that opens with a
    # quote holds the > as the text of an argument, as the word of a value written
    # for a test does. The spelling $">file" is the shells own way of writing the
    # name of a file behind a dollar, and is read as the redirect it is.
    if (t ~ /^\$">/) return 1
    if (t ~ /^"/) return 0
    return quotes(t) % 2 == 0
  }
  function is_option(t) {
    # A word that is an option of docker run, and so may take the word after it
    # as its value. An array word is no option, though it opens with the - of an
    # option: "${sec[@]}" is what --user is given, and a scan that read it as the
    # value of --user would read the next word, the image, as an option.
    if (array_word(t) != "") return 0
    # A short option of docker run (-e, -v, -w) always takes a word, and a long
    # one only when it is known to: see takes_value.
    if (t !~ /^-/) return 0
    return 1
  }
  function is_word_of_container(t) {
    # Whether a word that opens with a dash names a word of the container: an
    # option that owns the word after it, or a flag that owns none. A word that
    # names neither is no word of a container, and the scan stops at it rather
    # than name the word after it as the image of the run.
    if (t !~ /^-/) return 0
    if (array_word(t) != "") return 0
    if (index(" " option_words " ", " " t " ") > 0) return 1
    return index(" " flag_words " ", " " t " ") > 0
  }
  function unquoted(t) {
    # Whether a word is written without the quotes that hold a value: the > of a
    # redirect, the | of a pipeline and the rest are what the shell reads only
    # when no quote holds them, and the scan reads them the same way. A word that
    # opens with a quote holds all of them, and a word of a unit that opens a
    # quote and never closes it holds the rest of the line.
    if (t ~ /^"/) return 0
    return quotes(t) % 2 == 0
  }
  function takes_value(t) {
    # Whether an option takes a word of its own. The options of docker run that
    # do are listed here, as are the few of the runner the units name; a flag
    # (--rm, --init) and the long words a runner reads from an image (--tools,
    # --no-session) take none, and no guess is made about them. An option whose
    # word holds = carries its value inside the word (--memory=2g), and a short
    # option (-e, -v, -w, -p) always takes one.
    if (index(t, "=") > 0) return 0
    if (t ~ /^-[A-Za-z]$/) return 1
    if (t !~ /^--/) return 0
    return index(" " option_words " ", " " t " ") > 0
  }
  function no_image(t) {
    # A word that is no image: a word of the container, an assignment, or a word
    # that is not a name at all once its quotes are taken off.
    if (is_word_of_container(t)) return 1
    if (t ~ /^[A-Za-z_][A-Za-z0-9_]*=/) return 1
    if (unquote(t) !~ /^[A-Za-z0-9][A-Za-z0-9._:@\/-]*$/) return 1
    return 0
  }
  function is_assignment_of_container(t,  u, name) {
    # Whether a word is the setting of a container written after its image: the
    # name of a setting and the = that joins it to a value (FOO=$bar), as the
    # shell reads them, quotes aside. A scan that read such a word as no image
    # would end the walk where it stands, and the run would be reported unplaced,
    # past the reach of the check of the image the run gives its container.
    u = unquote(t)
    if (u !~ /^[A-Za-z_][A-Za-z0-9_]*=/) return 0
    name = substr(u, 1, length(u) - 1)
    return name ~ /^[A-Za-z_][A-Za-z0-9_]*$/
  }
  function note_assignment(line,  raw, nw, i, t, eq, an, at) {
    # The assignments a file writes on lines of its own, and the settings of
    # the instance directory that a unit only names. A run reads its image
    # from either, and either may be written after the run, so a file is read
    # whole before its first run is placed.
    if (line ~ /^[ \t]*#/) return
    # lib/config.sh exports the settings, and no unit assigns them.
    if (line ~ /^[ \t]*(export[ \t]+)?(TEHAS_|PI_)[A-Za-z0-9_]*=/) {
      an = line
      sub(/^[ \t]*(export[ \t]+)?/, "", an)
      eq = index(an, "=")
      if (eq > 1) {
        an = substr(an, 1, eq - 1)
        if (an ~ /^(TEHAS_|PI_)[A-Za-z0-9_]*$/) assign[an] = rest_after(line, eq)
      }
      return
    }
    if (line !~ /^[ \t]*(local[ \t]+)?[A-Za-z_][A-Za-z0-9_]*[ \t]*=/) return
    if (line ~ /(^|[ \t])while[ \t]/) return
    nw = split(line, raw, /[ \t]+/)
    for (i = 1; i <= nw; i = i + 1) {
      t = raw[i]
      if (t == "local" || t == "export" || t == "declare" || t == "readonly") continue
      eq = index(t, "=")
      if (eq < 2) continue
      an = substr(t, 1, eq - 1)
      if (an !~ /^[A-Za-z_][A-Za-z0-9_]*$/) continue
      at = index(line, an "=")
      if (at == 0) continue
      assign[an] = rest_after(line, at + length(an))
    }
  }
  function rest_after(line, eq,  t) {
    # The value an assignment gives a name: the text after the = to the end of
    # the line, which is one value when it is quoted and one word when it is
    # not. The quotes that hold a value are not part of it.
    if (eq < 1) return ""
    t = substr(line, eq + 1)
    sub(/^[ \t]+/, "", t)
    sub(/[ \t]+$/, "", t)
    if (t ~ /^".*"$/) return substr(t, 2, length(t) - 2)
    sub(/[ \t].*$/, "", t)
    return t
  }
  function walk(w, nw, from, nm, f, r,  i, t, v, ent, img, cmd, reason, value) {
    # One docker run, from the word after run to the end of its simple
    # command, printed as the record of the unit and the line it starts on.
    ent = ""
    img = ""
    cmd = ""
    reason = ""
    value = 0
    for (i = from; i <= nw; i = i + 1) {
      t = w[i]
      # A word that is nothing (an empty argument) is no part of the run and
      # is not where the image is read.
      if (t == "") continue
      # The words the shell reads, and no word of the unit: the separator that
      # ends a simple command, the word after which a command may stand, and the
      # descriptor and > of a redirect, whose words the unit gives to a file.
      if (is_separator(t) || ends_word(t)) break
      if (value) { value = 0; continue }
      if (t == "--entrypoint") {
        if (i == nw || ends_word(w[i + 1])) { reason = "--entrypoint has no word after it"; break }
        ent = unquote(w[i + 1])
        i = i + 1
        continue
      }
      if (img == "" && is_option(t)) {
        if (takes_value(t)) value = 1
        continue
      }
      # The expansion of an array stands for the words of the array, which are
      # the options of the container written before the image, and the arguments
      # of the command written after it. No unit holds an image in an array, so
      # an option that is given one carries all its words (--user "${sec[@]}")
      # and no word is left to be read as the image: a scan that stopped at the
      # word, or read the word after it, would name an option as the image. The
      # command of a run is only known once the image stands, so an array that
      # ends the words of the run ends the walk with it.
      if (is_array(t)) {
        if (img != "") break
        continue
      }
      # A word that builds a name the scan cannot join is no image to name: a
      # digest, a tag or a number the file reads sits inside it (caddy@$digest,
      # tehas-preview-$n), and a $( ... ) the shell fills in is one argument the
      # scan reads whole. Either way the walk ends where the word stands, rather
      # than read the words after it for an image.
      if (is_composite(t)) { reason = "no image follows docker run: " t " names an image from parts the scan cannot join"; break }
      # The command of the run: the word after the image, which no option of
      # docker run owns, because every option of a container lies before it. A
      # word the scan knows no option of a container is no command either: the
      # walk ends where it stands, because the name of a file or of a program
      # may be read after it, and a scan that named it would name what follows.
      if (img != "") {
        # The word after the image may be a word of the container that lies
        # where a command should: --live of a server written after its image.
        # Such a word names no command, and the walk ends where it stands rather
        # than name the name of a file that follows it as one.
        if (is_word_of_container(t)) {
          reason = "no command follows the image " img ": " t " is a word of the container written after it"
          break
        }
        # Or the setting of the container written where a command should.
        if (is_assignment_of_container(t)) {
          reason = "no command follows the image " img ": " t " is a setting of the container written after it"
          break
        }
        cmd = unquote(t)
        break
      }
      if (is_expansion(t)) {
        # A name the units only name, and never assign, names a setting of the
        # instance directory: lib/config.sh exports it, and the run of a unit
        # reads its image from it. The scan names the setting, as the unit does,
        # so no default of one host is written into a test. What a file does
        # assign may itself name a value the file assigns, as
        # image=$PI_ENSEMBLE_IMAGE and image=$runner do, so the name is read
        # through as far as the file carries it.
        v = name_of(t)
        depth = 0
        while (depth < 6) {
          if (v ~ /^(TEHAS_|PI_)[A-Za-z0-9_]*$/) { img = v; break }
          if (!(v in assign)) { reason = "the image " t " is assigned by no statement of " nm; break }
          if (assign[v] !~ /^\$\{?[A-Za-z_]/) break
          v = name_of(assign[v])
          depth = depth + 1
        }
        if (reason != "") break
        if (depth >= 6) { reason = "the image " t " is named by more assignments than the scan follows"; break }
        if (v ~ /^(TEHAS_|PI_)[A-Za-z0-9_]*$/) { img = v; continue }
        if (!(v in assign)) { reason = "the image " t " is assigned by no statement of " nm; break }
        img = assign[v]
        continue
      }
      if (no_image(t)) { reason = "no image follows docker run: " t " is no image"; break }
      img = unquote(t)
    }
    if (reason == "" && img == "") reason = "no image follows docker run"
    if (reason == "" && cmd == "" && ent == "") reason = "nothing follows the image " img
    if (reason != "") { record(sprintf("%s\t%d\tunplaced\t\t%s", nm, start_of[f, r], reason)); return }
    if (ent != "") cmd = ent
    record(sprintf("%s\t%d\t%s\t%s\t", nm, start_of[f, r], img, cmd))
  }
  # The scan of the words of a unit, kept in one place and read as a program, so
  # that the shell that runs it quotes nothing of it: a test that names the file
  # of the scan can be read on its own, and a body that holds a quote is no
  # trouble to the shell that calls it.
  BEGIN {
    FS = ""
    nfiles = 0
    # The options that stand between the head of a run and its image, and take
    # a word of their own: those of docker run, and those of the runner the
    # units name. A flag (--rm, --init, --read-only) is no entry.
    # The long words of a runner that name no value of their own: the read of a
    # model, the choice of a session, the tools a turn may use. The units write
    # them after the image, and a scan that read one as the option of a container
    # would name the word written after it as the image of the run.
    flag_words = "--init --rm --no-session --no-extensions --no-skills --no-prompt-templates --no-context-files --json"
    option_words = "--cidfile --device --entrypoint --env --env-file --expose --group-add --label --label-file --link --network --network-alias --name --pid --user --userns --uts --volume --volumes-from --workdir --tty --interactive --gpus --hostname --add-host --annotation --cap-add --cap-drop --cgroup-parent --cgroupns --cpus --cpu-shares --cpuset-cpus --detach --device-cgroup-rule --dns --dns-option --dns-search --domainname --ipc --isolation --kernel-memory --log-driver --log-opt --mac-address --memory --memory-reservation --memory-swap --memory-swappiness --mount --oom-score-adj --pids-limit --publish --restart --security-opt --shm-size --stop-signal --stop-timeout --sysctl --tmpfs --ulimit --volume-driver --model --provider --session --tools --capabilities --repo-path --server-version"
    # A run may begin behind the words of another program, as a run inside
    # timeout does. The words of the program and of its own options are no part
    # of the run, and no run is read behind a word the scan cannot tell from a
    # value. scan_part looks for a run at every word of the command, so the
    # words of the program before one (timeout, exec, command, env) name no
    # entry of their own: the run behind them is read from its own docker.
  }
  # The words of a run are scanned by one pass of the file: the assignments are
  # read first, and then the lines, so that a value written after the run that
  # uses it names the image of that run.
  # One unit at a time: the test calls the scan with a file of its own, because
  # the assignments that name an image are read from the file the run stands in,
  # and no name of another file is read into this one.
  FNR == 1 {
    if (nfiles != 0) {
      print "the scan reads one unit at a time" > "/dev/stderr"
      exit 2
    }
    nfiles = 1
    unit_of[nfiles] = unit
    lines_of[nfiles] = 0
  }
  # The record of a run is kept for the account, and printed by END: a line that
  # holds the run of another unit, in the text of a message or of a comment, is
  # dropped before the account is made, so no record of it is printed.
  {
    line = $0
    gsub(/\r/, "", line)
    lines_of[nfiles] = lines_of[nfiles] + 1
    text[nfiles, lines_of[nfiles]] = line
    start_of[nfiles, lines_of[nfiles]] = FNR
  }
  END {
    # A file is read whole, so that an assignment written after the run that
    # reads it is still there when the run is placed.
    for (f = 1; f <= nfiles; f = f + 1) {
      # The assignments of one file are no other files. Each element is
      # cleared by name, which every awk reads: a whole-array delete is a gawk
      # spelling mawk will not parse.
      for (v in assign) delete assign[v]
      for (r = 1; r <= lines_of[f]; r = r + 1) note_assignment(text[f, r])
      for (r = 1; r <= lines_of[f]; r = r + 1) scan_line(f, r, unit_of[f])
    }
    # The records of the runs, and the account of them on the standard error,
    # where no record is read: the test asks for the account and compares it with
    # the count the units were made from.
    for (i = 1; i <= kept; i = i + 1) print out_of[i]
    printf "placed %d unplaced %d bad %d\n", placed, unplaced, bad > "/dev/stderr"
  }
  function scan_line(f, r, nm,  stmt, nxt, raw4, parts4, nw4, words2, i, j) {
    # The statements of one line of a unit: the line with its continuations
    # joined, divided at the separators the shell reads, and each simple
    # command scanned for the runs it holds.
    stmt = text[f, r]
    # A comment is no command: its text names no run and holds no image. It is
    # dropped before the continuations are joined, so that the text of a
    # comment continued from the line above is dropped with it.
    while (ends_continued(stmt)) {
      if (r >= lines_of[f]) break
      r = r + 1
      nxt = text[f, r]
      stmt = substr(stmt, 1, length(stmt) - 1) " " trim_head(nxt)
    }
    # A comment is no command: its text names no run and holds no image, and
    # the words of the help text of a unit would else be read as the words of
    # a statement.
    if (stmt ~ /^[ \t]*#/) return
    if (stmt ~ /^[ \t]*$/) return
    # The assignments the shell makes and the tests it asks are no commands of
    # their own: caps=$( ... ) would else open a command whose head is the name of
    # a value. A run written inside the parentheses is still a command, and is
    # read from them: the words before them name a value, and no word of them
    # belongs to the run.
    if (assign_only(stmt)) {
      stmt = run_of_parentheses(stmt)
      if (stmt == "") return
    }
    nw4 = join_words(stmt, raw4, parts4, words2)
    i = 1
    while (i <= nw4) {
      j = i
      while (j <= nw4 && !is_separator(words2[j])) j = j + 1
      scan_part(words2, i, j - 1, nm, f, r)
      i = j + 1
    }
  }
  function run_of_parentheses(stmt,  u, a, b) {
    # The words of the run a line assigns: the text inside the $( ... ) that
    # opens the value, with the words that name the value taken off. Nothing for
    # a line that holds no run inside parentheses.
    u = stmt
    if (u !~ /=\$\(/) return ""
    a = index(u, "$( ")
    if (a == 0) a = index(u, "$(")
    if (a == 0) return ""
    b = rindex(u, ")")
    if (b <= a) return ""
    return substr(u, a + 2, b - a - 2)
  }
  function rindex(t, x,  i) {
    # The last place a word is written in another, or nothing.
    i = 0
    while (index(substr(t, i + 1), x) > 0) i = i + index(substr(t, i + 1), x)
    return i
  }
  function assign_only(stmt,  w, n, i, t) {
    # Whether a statement is no command of its own: an assignment the shell makes
    # (caps=$( ... )), a test it asks ([ ... ]), or a line whose words are all
    # assignments (local bin=$x digest=$y). None is a place where a run stands.
    if (stmt ~ /^[ \t]*[A-Za-z_][A-Za-z0-9_]*=\$\(/) return 1
    if (stmt ~ /^[ \t]*\[/) return 1
    # The read of a value the file asks for ([[ ... =~ ... ]]) holds the text of
    # a pattern, and no command stands there: a pattern is not read as a run.
    if (stmt ~ /^[ \t]*\[\[/) return 1
    n = split(stmt, w, /[ \t]+/)
    for (i = 1; i <= n; i = i + 1) {
      t = w[i]
      if (t == "local" || t == "export" || t == "declare" || t == "readonly") continue
      if (t ~ /^[A-Za-z_][A-Za-z0-9_]*=/) continue
      return 0
    }
    return n > 0
  }
  function is_separator(t) {
    # A word that ends a simple command and begins the next: the separators the
    # shell reads, and the words after which a command may stand. Only as the
    # shell reads them, and only at the head of a command: the -a of a test or
    # the -o of a value carries the & of a background as the text of a word, and
    # the word of a runner that opens with a quote holds all of them.
    if (!unquoted(t)) return 0
    if (t ~ /^-/) return 0
    if (t == ";" || t == "|" || t == "&&" || t == "||" || t == "(" || t == ")") return 1
    if (t == "then" || t == "do" || t == "else" || t == "elif" || t == "fi" ||
        t == "done" || t == "esac" || t == "if" || t == "while" || t == "until") return 1
    return 0
  }
  function tally(t,  u) {
    # One line of the account the scan keeps: how many runs it placed, how many
    # it left unplaced, and how many it read as neither, which no run of a unit
    # writes. The shape of a record says which it is, read from its own fields:
    # one of a run the scan placed names an image and a command, and ends with
    # the tab that tells an empty command from a missing one; one of a run it
    # could not place names its image unplaced and ends with the reason for it.
    u = t
    sub(/^[^	]*	[^	]*	/, "", u)
    if (u ~ /^unplaced		[^	]+$/) unplaced = unplaced + 1
    else if (u !~ /^	/ && u ~ /	$/) placed = placed + 1
    else bad = bad + 1
  }
  function scan_part(w, from, to, nm, f, r,  k, asked) {
    # The runs of the words of one simple command, which a separator or the
    # head of the statement bounds. A run stands at the head, behind the ! of
    # a negated command, or behind the program the unit runs it with (timeout)
    # and that programs own options.
    if (from > to) return
    # The head of the command, and the runs after each word a command may only
    # follow. A run is looked for at every word, because the words before it
    # belong to whatever the line holds around it (the arguments of the program
    # that runs it, the words of a test) and are no part of the scan of the run:
    # walk_from reads the words of a run from its own docker. The two words of a
    # run read behind the quote that holds a value name no run, and are asked
    # for once: the words of one quoted value ("a docker run b") are no place
    # where a command stands, and a scan that read them twice would make two
    # records of the one line and answer the count of a directory with a run the
    # units never start.
    asked = ""
    for (k = from; k <= to - 1; k = k + 1) {
      if (w[k] != "docker" || w[k + 1] != "run") continue
      if (!unquoted(w[k]) || !unquoted(w[k + 1])) continue
      if (index(" " asked " ", " " (k - from) " ") > 0) continue
      asked = asked (asked == "" ? "" : " ") (k - from)
      # The head of the command, or a command the words of a compound statement
      # put at this word: if ( ! docker run ... ; then do docker run ...
      walk_from(w, k, to, nm, f, r)
    }
  }
  function walk_from(w, k, last, nm, f, r,  words6, i, nw6) {
    # The words of one run, read as a command of their own so that no word
    # before the run is seen: the words of the part from its docker to the end.
    nw6 = last - k + 1
    for (i = 1; i <= nw6; i = i + 1) words6[i] = w[k + i - 1]
    walk(words6, nw6, 3, nm, f, r)
  }

AWK
  awk -v unit="$unit" -f "$T/container_scan.awk" "$1"
}

# The records of the units in <dir>, in the order of the file and of the line.
records_of() { # <dir of units>
  local file
  for file in "$1"/*; do
    [ -f "$file" ] || continue
    container_records "$file"
  done
}

# The runs the scan cannot place, as <unit>:<line>, one per line, in the order
# of the file and of the line. A run the scan reads no image for is named here,
# with the file and the line it stands on, and by no count of a number.
unplaced_of() { # <dir of units>
  records_of "$1" | awk -F '\t' '$3 == "unplaced" { print $1 ":" $2 }' | sort
}

# The account the scan keeps of its own records, as it writes it to the error
# of the scan: how many it placed, how many it left unplaced, and how many it
# read as neither, which no run of a unit writes.
records_account() { # <dir of units>
  # The account of the whole directory: one program reads the account each file
  # writes, and adds them. The scan of one file cannot know the directory, and
  # no record of one file is read into another.
  records_of "$1" 2>&1 >/dev/null |
    awk '{ placed += $2; unplaced += $4; bad += $6 }
      END { printf "placed %d unplaced %d bad %d\n", placed, unplaced, bad }'
}

# How many docker run the units in <dir> hold, counted by a rule the scan does
# not share: a line that writes docker run where the shell reads a command, which
# is the start of a line, the start of a command, or what a separator, a pipeline
# or the parentheses of a value put before it. The text of a comment is no command
# and holds no run, and the words of a message that names the image start none.
# The words before a run a line does hold are read through, because a run behind
# them is still a run: timeout runs the check of a runner, and that check is a
# container.
docker_run_count() { # <dir of units>
  # How many lines of the units write docker run where the shell reads a
  # command: the start of a line, the start of a command, or what a separator, a
  # pipeline or the parentheses of a value put before it. A comment is no
  # command, and the text of a message that names the image starts no container.
  #
  # Three shapes are no command, whatever the two words of a run they hold. A
  # line that assigns the value of a name from $( ... ), or that opens a
  # subshell with (, is read by the shell as more than the words of a line, and
  # the run inside it is read from the words of the parentheses: no unit writes
  # one today, and a run read there is named by the file and the line it fails
  # on. The text behind the # of a comment names no command, and
  # the shell reads that # only where a word begins, so the line is cut at a #
  # that a space opens or that opens the line: a run written after such a # is
  # read by no shell, and a # held inside a quoted value ("a # b") is a word of
  # the run and cuts nothing.
  awk '
    {
      line = $0
      sub(/\r/, "", line)
      sub(/^[ \t]+/, "", line)
      if (line ~ /^#/) next
      cut = match(line, /(^|[ \t])#/)
      if (cut > 0) line = substr(line, 1, cut - 1)
      if (line !~ /(^|[;&|()]|[ ])(env[ ]+[^ ]+[ ]+)*(timeout[ ]+(-[^ ]+[ ]+)*[0-9]+[ ]+)?docker run([ \t;]|$)/) next
      n++
    }
    END { print n + 0 }
  ' "$1"/*
}

# Each of these shapes hid a run from the first version of the scan, or read a
# run where the unit starts none. They are checked in a directory of their own,
# so a shape that stops matching fails on the shape and not only on the units.
# The image each fixture names is one the units name too (an image assigned in
# the file, and read back by its name), and each fixture is named for the shape
# it holds.
t_case "the scan reads the image and the command of each shape of a docker run"
mkdir -p "$T/scan"
# The words before the run belong to no part of it: the runner of a run that
# came first, and the arguments written after a run of the same statement.
cat >"$T/scan/two-in-one-statement" <<'EOF'
image=/opt/fixture/sandbox:tag
docker run --rm "$image" one; docker run --rm "$image" two
EOF
# improve runs its container behind timeout and that program's own options.
cat >"$T/scan/behind-timeout" <<'EOF'
image=/opt/fixture/sandbox:tag
runner=/opt/fixture/bin/runner
caps=$(timeout -k 10 60 docker run --rm "$image" "$runner" --capabilities 2>/dev/null) || caps=""
EOF
# dev and work ask a run whether it worked, which is no word of the run.
cat >"$T/scan/negated" <<'EOF'
image=/opt/fixture/sandbox:tag
if ! docker run --rm --init "$image" one; then echo ok; fi
EOF
# work runs one container in a pipeline with a filter.
cat >"$T/scan/pipeline" <<'EOF'
image=/opt/fixture/sandbox:tag
docker run --rm --init "$image" one | grep one
EOF
# check-product asks a test of a name, and the run stands after it.
cat >"$T/scan/inside-case" <<'EOF'
image=/opt/fixture/sandbox:tag
case $x in a) docker run --rm --init "$image" one ;; esac
EOF
cat >"$T/scan/inside-loop" <<'EOF'
image=/opt/fixture/sandbox:tag
while read -r x; do docker run --rm --init "$image" one; done < "$list"
EOF
# mcp hands its run to exec.
cat >"$T/scan/behind-exec" <<'EOF'
image=/opt/fixture/sandbox:tag
exec docker run --rm --init "$image" one
EOF
# work gives --user the words of an array, and the image stands after them.
cat >"$T/scan/array-value-of-option" <<'EOF'
image=/opt/fixture/sandbox:tag
docker run --rm --init --name "$name" --user "${sec[@]}" -w "$repo" "$image" pi
EOF
# The words of an array that holds the options of the container.
cat >"$T/scan/array-before-image" <<'EOF'
image=/opt/fixture/sandbox:tag
runner=/opt/fixture/bin/runner
docker run --rm --init --name "$name" "${limits[@]}" --workdir "$repo" "$image" "$runner" work
EOF
# The words of an array that holds the arguments of the command.
cat >"$T/scan/array-after-image" <<'EOF'
image=/opt/fixture/sandbox:tag
runner=/opt/fixture/bin/runner
docker run --rm "$image" "$runner" work "${run_flags[@]}"
EOF
# index names its command through --entrypoint, and the word after the image is
# an argument of that command and no command of its own.
cat >"$T/scan/entrypoint" <<'EOF'
image=/opt/fixture/sandbox:tag
runner_two=/opt/fixture/bin/runner-two
timeout -k 9 docker run --rm --init \
  --entrypoint "$runner_two" "$image" \
  sub --dry-run
EOF
# improve names pi on a line of its own, continued from the run above it.
cat >"$T/scan/continued-line" <<'EOF'
image=/opt/fixture/sandbox:tag
timeout -k 30 90 docker run --rm --init \
  -e HOME=/home/vscode \
  "$image" \
  fixture-one --flag
EOF
# work passes --user a name of a user and a group the shell fills in.
cat >"$T/scan/expansion-as-argument" <<'EOF'
image=/opt/fixture/sandbox:tag
docker run --rm --user "$(id -u):$(id -g)" "$image" pi
EOF
# work gives a runner an argument that holds the | of a pipeline and a ;.
cat >"$T/scan/quoted-word-with-pipe" <<'EOF'
image=/opt/fixture/sandbox:tag
runner=/opt/fixture/bin/runner
docker run --rm "$image" "$runner" -c "echo one; echo two | three"
EOF
# The text of a comment names no run, and the words of a message that names the
# image start no container.
cat >"$T/scan/comment-says-docker-run" <<'EOF'
image=/opt/fixture/sandbox:tag
# docker run answers once the process is started.
docker run --rm "$image" pi
EOF
cat >"$T/scan/message-names-image" <<'EOF'
image=/opt/fixture/sandbox:tag
echo "tehas: $image gave no answer" >&2
EOF
# The two words of a run written inside the text of a message name no run, and
# no record: the words of one quoted value are no place where a command stands,
# and a scan that read them as one would answer the count with a run no unit
# starts. The message of this fixture asks for one record, not for none, so the
# case names the line it went wrong on.
cat >"$T/scan/message-names-docker-run" <<'EOF'
image=/opt/fixture/sandbox:tag
echo "tehas: docker run is what starts a container" >&2
docker run --rm "$image" pi
EOF
# A quoted value that holds a space is one image, and one word.
cat >"$T/scan/quoted-value-with-space" <<'EOF'
image="/opt/fixture/sandbox image:tag"
docker run --rm "$image" pi
EOF
# The setting of a container written after its image is no command of its own,
# and the run keeps the command it does name: a scan that read the word as no
# image would end the walk there and report the run unplaced, where no check of
# the image the run gives its container would reach it.
cat >"$T/scan/setting-after-the-image" <<'EOF'
image=/opt/fixture/sandbox:tag
docker run --rm "$image" pi HOME=/home/vscode
EOF
records=$(records_of "$T/scan")
t_eq "$records" 'array-after-image	3	/opt/fixture/sandbox:tag	$runner	
array-before-image	3	/opt/fixture/sandbox:tag	$runner	
array-value-of-option	2	/opt/fixture/sandbox:tag	pi	
behind-exec	2	/opt/fixture/sandbox:tag	one	
behind-timeout	3	/opt/fixture/sandbox:tag	$runner	
comment-says-docker-run	3	/opt/fixture/sandbox:tag	pi	
continued-line	5	/opt/fixture/sandbox:tag	fixture-one	
entrypoint	5	/opt/fixture/sandbox:tag	$runner_two	
expansion-as-argument	2	/opt/fixture/sandbox:tag	pi	
inside-case	2	/opt/fixture/sandbox:tag	one	
inside-loop	2	/opt/fixture/sandbox:tag	one	
message-names-docker-run	3	/opt/fixture/sandbox:tag	pi	
negated	2	/opt/fixture/sandbox:tag	one	
pipeline	2	/opt/fixture/sandbox:tag	one	
quoted-value-with-space	2	/opt/fixture/sandbox image:tag	pi	
quoted-word-with-pipe	3	/opt/fixture/sandbox:tag	$runner	
setting-after-the-image	2	/opt/fixture/sandbox:tag	pi	
two-in-one-statement	2	/opt/fixture/sandbox:tag	one	
two-in-one-statement	2	/opt/fixture/sandbox:tag	two	'
records=$(records_of "$T/scan")
# The account of the scan is written to the standard error and never reaches the
# records; a record of a placed run ends with the tab that tells the empty account
# of it from a missing one, and a shell that reads a command substitution drops it.
# A record of a run the scan cannot place has no such tab, and is checked whole.
found=" $(tr '\n' ' ' <<<"$records") "
t_lacks "$found" ' sub '
t_lacks "$found" ' --dry-run '
t_lacks "$found" ' answers '
t_lacks "$found" ' starts a container '
t_lacks "$found" ' gave '

t_case "the scan says why it cannot place a run it cannot read"
mkdir -p "$T/unplaced"
# The image of the run of a unit is named by another file, and the scan reads a
# file on its own: no image of another file is read into this one.
cat >"$T/unplaced/variable-in-another-file" <<'EOF'
docker run --rm "$image" pi
EOF
# preview serves an image the unit itself built, named from a number the file
# reads. caddy runs the server of a digest install.sh wrote to a file the unit
# reads. Neither name is one the scan can join.
cat >"$T/unplaced/tag-built-in-the-file" <<'EOF'
n=7
docker run --rm --init "tehas-preview-$n" pi
EOF
cat >"$T/unplaced/image-from-digest" <<'EOF'
digest=$(sed -n 's/^D=//p' "$versions")
docker run --rm --entrypoint /srv "caddy@$digest" --live /live
EOF
# A redirect ends the words of a run, and an image written after it is no image
# the unit gives any container.
cat >"$T/unplaced/redirect-before-image" <<'EOF'
image=/opt/fixture/sandbox:tag
docker run --rm -e HOME=/home/vscode >"$log" "$image" pi
EOF
records=$(records_of "$T/unplaced")
t_eq "$records" 'image-from-digest	2	unplaced		no image follows docker run: "caddy@$digest" names an image from parts the scan cannot join
redirect-before-image	2	unplaced		no image follows docker run
tag-built-in-the-file	2	unplaced		no image follows docker run: "tehas-preview-$n" names an image from parts the scan cannot join
variable-in-another-file	1	unplaced		the image "$image" is assigned by no statement of variable-in-another-file'

t_case "the scan places one record for every docker run the units hold"
# The count is made by a rule the scan does not share, and the two must agree:
# the count is what the units hold today, one run for each of the eight units
# that start a container, three for work (the check of a runner, the check of a
# push, and the cycle), and one for preview, the run of the comment that names
# docker run being read by neither.
count=$(docker_run_count "$T_ROOT/libexec")
t_eq "$count" 11
# The account of the records is read off the records themselves, and not from
# two totals that could drift together: the lines of the records are the runs
# the scan made records for, so a record of anything at all answers one line of
# the count, and no record answers two.
records=$(records_of "$T_ROOT/libexec")
t_eq "$(wc -l <<<"$records")" "$count"
# The shape of a record carries its own account, so the two assertions above
# cannot be met by a record that names neither an image nor a reason: a record
# of a run the scan placed ends with the tab that tells an empty command from a
# missing one, and one of a run it could not place names its image unplaced and
# ends with the reason it gives. The scan keeps the two apart itself, and says
# how many records it read as neither, which is what the account on the error
# of the scan is asked for.
t_eq "$(records_account "$T_ROOT/libexec")" "placed 9 unplaced 2 bad 0"
# Any run the scan cannot place fails this case, and is named by the file and
# the line it stands on. The two the units leave today are named by who and
# where, and by no number that a third unplaced run could drift beside: one is
# the preview of an image the unit builds itself, and one the server of a digest
# install.sh wrote to a file. A run that joins them, or one of them leaving,
# fails here rather than holding the count.
t_eq "$(unplaced_of "$T_ROOT/libexec")" 'mcp:107
preview:676'

t_case "every container tehas starts runs an image and a command the image checks"
records=$(records_of "$T_ROOT/libexec")
# The derivation is not vacuous: these are what today's libexec/ names, and a
# scan that stopped matching would fail here instead of passing with an empty
# list.
for want in \
  'check-product	145	PI_ENSEMBLE_IMAGE	bash	' \
  'dev	336	PI_ENSEMBLE_IMAGE	pi	' \
  'improve	272	PI_ENSEMBLE_IMAGE	pi	' \
  'index	313	PI_ENSEMBLE_IMAGE	codebase-memory-mcp	' \
  'plan	249	PI_ENSEMBLE_IMAGE	$runner	' \
  'plan	436	PI_ENSEMBLE_IMAGE	$runner	' \
  'work	464	PI_ENSEMBLE_IMAGE	$runner	' \
  'work	697	PI_ENSEMBLE_IMAGE	git	' \
  'work	805	PI_ENSEMBLE_IMAGE	$runner	'; do
  case "
$records
" in *"
$want
"*) ;; *) t_bad "the scan does not record: $want" ;; esac
done
# The two containers of the units the scan cannot place are the preview it builds
# itself and the server of a digest install.sh chose: neither runs in the sandbox
# image, so neither is a command the image would check.
missing=""
while IFS=$'\t' read -r unit line image cmd _; do
  [ -n "$unit" ] || continue
  [ "$image" = unplaced ] && continue
  [ "$image" = PI_ENSEMBLE_IMAGE ] || { missing="$missing $unit:$image"; continue; }
  cmd=${cmd%$'\t'}
  case "$cmd" in
    '$runner') cmd=/opt/pi-rukas/bin/pi-rukas-headless ;;
  esac
  case " $gate " in *" $cmd "*) ;; *) missing="$missing $unit:$cmd" ;; esac
done <<<"$records"
[ -z "$missing" ] || t_bad "$missing runs in the sandbox image and the image does not check it"

t_done
