# The tehas sandbox image: the pi-rukas image plus what cycles need on top.
#
# Adds what the upstream image lacks:
#   - the in-container user remapped to the tehas user's id, in the steps after
#     the uid enters (upstream hardcodes vscode = UID 1000; with another id on
#     the host, bind-mounted host directories are not writable). This file ends at
#     a stage that reads the uid nowhere above it, so the instance users of one
#     host share those layers: the manual says why the split is where it is, in
#     What two instance users of one host share (host/docs/host.md).
#   - shellcheck, which gates commonly run (.pi/verify-cmd, .pi/smoke-cmd),
#   - cargo's registry given to that user (the upstream image has Rust, and
#     a registry only root can write, so a gate could not fetch a crate),
#   - Playwright's Chromium with its system libraries and fonts, for a
#     product's browser checks,
#   - `file` and a Python image library, so a cycle can look at a picture,
#   - the code indexer the agents' code search answers from, at the version
#     versions.env names rather than whatever upstream's main carried,
#   - the embedding model vipune searches with, which a cycle cannot download,
#   - no wigolo: the base ships one that fails on every call.
#
# A product whose gates need another toolchain does not add it here: it adds it
# to <instance dir>/sandbox.Dockerfile, which sandbox/build.sh builds on top of
# this image when the instance has one (host/docs/new-project.md). Editing this
# file makes `tehas update` refuse with "has local changes" for good.
#
# Build with sandbox/build.sh, which passes the user id, the versions from
# versions.env and the base: upstream's image for the pi-rukas commit
# versions.env follows. It hands docker a copy of these lines with the uid's
# argument moved below the boundary, so the file a person edits is this one and
# the order the build runs is the order written here. shellcheck is pinned by
# checksum, Playwright by a lockfile beside this file, and codebase-memory-mcp by
# its installer's checksum plus a version read back from the binary. A step near
# the end checks that every command a cycle reaches for, and every command tehas
# gives a container, answers in the image.
ARG PI_RUKAS_IMAGE
# The layers every instance user of one host shares: everything this file does
# that the uid does not decide. A second stage below takes them with a FROM of
# its own.
FROM ${PI_RUKAS_IMAGE} AS shared

# The version arguments, declared here rather than beside the steps that read
# them: sandbox/build.sh passes one --build-arg for each ARG line of this file,
# and test/build_test.sh compares the two lists in both directions. A build that
# says nothing about one of them gets the empty value of the stage that reads it,
# which its own check refuses. TEHAS_UID is the exception: the remap below declares
# it, and the copy the build hands docker declares that one argument once.
ARG SHELLCHECK_VERSION
ARG SHELLCHECK_SHA256
ARG JQ_VERSION
ARG JQ_SHA256
ARG PLAYWRIGHT_VERSION
ARG PLAYWRIGHT_LOCK_SHA256
ARG CBM_VERSION
ARG CBM_INSTALLER_SHA256
ARG VIPUNE_VERSION
ARG VIPUNE_MODEL
ARG VIPUNE_MODEL_REVISION
ARG VIPUNE_MODEL_SHA256_ONNX
ARG VIPUNE_MODEL_SHA256_TOKENIZER

USER root

# What a gate fetches stays in its own container and goes with it: a registry
# kept between cycles would be code one cycle leaves for the next one's gate.
RUN mkdir -p /usr/local/cargo/registry && chown -R vscode:vscode /usr/local/cargo/registry

# A cycle that is shown a picture has no way to look at it without these: with
# neither, an agent writes its own PNG decoder to measure a screenshot. They
# are distribution packages, taken at whatever version the distribution carries,
# and they are in the container only: bootstrap.sh installs neither and its
# check does not look for them.
RUN apt-get update -qq \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends file python3-pil \
 && rm -rf /var/lib/apt/lists/* \
 && echo "sandbox: file and a Python image library are in the image"

# The base installs wigolo with npm's scripts off, which skips the native build
# of its better-sqlite3, so every call dies on a missing bindings file. A binary
# that fails on every invocation costs a cycle a call each time an agent reaches
# for it and teaches them the helper is unreliable. pi-rukas's own recipe
# preflights `command -v wigolo` and reports "wigolo not installed" when it is
# absent, so taking it off PATH answers this; repairing it belongs upstream.
RUN rm -rf /usr/lib/node_modules/wigolo /usr/local/lib/node_modules/wigolo \
 && rm -f /usr/bin/wigolo /usr/local/bin/wigolo /usr/bin/wigolo.cmd /usr/local/bin/wigolo.cmd \
 && if command -v wigolo >/dev/null 2>&1; then echo "sandbox: wigolo is still on PATH at $(command -v wigolo)" >&2; exit 1; fi \
 && echo "sandbox: wigolo is off PATH"

# The index a cycle searches and the search that reads it are one program:
# `tehas index` runs codebase-memory-mcp in this image, and a cycle's
# codebase_memory calls answer from what it wrote. The base installs it from
# upstream's main, unpinned, so its tool schemas move under a host between
# builds and a call that worked in one cycle is refused in the next. Here it is
# the version versions.env names, the one bootstrap.sh puts on the host: the
# installer is checked by checksum as there, and the version the image ends
# with is read back from the binary. The installer's own activation cannot
# publish onto the image's overlayfs when Docker runs inside an unprivileged
# Incus container, as a factory's does: it stops with cleanup_timeout. So it
# installs into a tmpfs, made 755 because it refuses a directory others can
# write, and only the binary is copied over the base's copy in /usr/local/bin.
RUN --mount=type=tmpfs,target=/cbm \
    chmod 755 /cbm \
 && curl -fsSL --connect-timeout 20 --max-time 300 --retry 3 --retry-all-errors --retry-connrefused --retry-delay 5 --retry-max-time 300 "https://raw.githubusercontent.com/DeusData/codebase-memory-mcp/v${CBM_VERSION}/install.sh" -o /tmp/cbm-install.sh \
 && echo "${CBM_INSTALLER_SHA256}  /tmp/cbm-install.sh" | sha256sum -c - \
 && env HOME=/root CBM_DOWNLOAD_URL="https://github.com/DeusData/codebase-memory-mcp/releases/download/v${CBM_VERSION}" \
     bash /tmp/cbm-install.sh --skip-config --dir=/cbm \
 && install -m 755 /cbm/codebase-memory-mcp /usr/local/bin/codebase-memory-mcp \
 && rm -f /tmp/cbm-install.sh \
 && found=$(codebase-memory-mcp --version 2>&1 | grep -E '^codebase-memory-mcp' | grep -o -E '[0-9]+\.[0-9]+\.[0-9]+' | head -n 1) \
 && if [ "${found:-}" != "$CBM_VERSION" ]; then echo "sandbox: codebase-memory-mcp is ${found:-not there}, versions.env names $CBM_VERSION" >&2; exit 1; fi \
 && echo "sandbox: codebase-memory-mcp $CBM_VERSION"

RUN curl -fsSL --connect-timeout 20 --max-time 300 --retry 3 --retry-all-errors --retry-connrefused --retry-delay 5 --retry-max-time 300 "https://github.com/koalaman/shellcheck/releases/download/v${SHELLCHECK_VERSION}/shellcheck-v${SHELLCHECK_VERSION}.linux.x86_64.tar.xz" -o /tmp/shellcheck.tar.xz \
 && echo "${SHELLCHECK_SHA256}  /tmp/shellcheck.tar.xz" | sha256sum -c - \
 && tar -xJf /tmp/shellcheck.tar.xz -C /tmp \
 && install -m 755 "/tmp/shellcheck-v${SHELLCHECK_VERSION}/shellcheck" /usr/local/bin/shellcheck \
 && rm -rf /tmp/shellcheck.tar.xz "/tmp/shellcheck-v${SHELLCHECK_VERSION}" \
 && shellcheck --version

# tehas's jq, ahead of the base image's on the PATH: the gate a cycle runs uses
# the jq the host and CI use (host/versions.env).
RUN curl -fsSL --connect-timeout 20 --max-time 300 --retry 3 --retry-all-errors --retry-connrefused --retry-delay 5 --retry-max-time 300 "https://github.com/jqlang/jq/releases/download/jq-${JQ_VERSION}/jq-linux-amd64" -o /tmp/jq \
 && echo "${JQ_SHA256}  /tmp/jq" | sha256sum -c - \
 && install -m 755 /tmp/jq /usr/local/bin/jq \
 && rm -f /tmp/jq \
 && /usr/local/bin/jq --version

# vipune downloads its embedding model on its first search. A cycle has no
# route to the publisher, so the download failed and every later search answered
# an empty store, which reads as a host that recorded nothing. The model is put
# where hf_hub looks for it, in the home of the user a container runs as, by the
# name that user has in this stage: vscode at the id the base gave it. The remap
# below moves that id and takes the files by the same name, so where the uid
# enters does not change what this layer holds. hf_hub reads a file from the
# snapshot without asking the network.
RUN repo="/home/vscode/.cache/huggingface/hub/models--$(printf %s "$VIPUNE_MODEL" | sed 's#/#--#g')" \
 && snap="$repo/snapshots/$VIPUNE_MODEL_REVISION" \
 && url="https://huggingface.co/$VIPUNE_MODEL/resolve/$VIPUNE_MODEL_REVISION" \
 && mkdir -p "$repo/refs" "$snap/onnx" \
 && curl -fsSL --connect-timeout 20 --max-time 300 --retry 3 --retry-all-errors --retry-connrefused --retry-delay 5 --retry-max-time 300 "$url/onnx/model.onnx" -o "$snap/onnx/model.onnx" \
 && curl -fsSL --connect-timeout 20 --max-time 300 --retry 3 --retry-all-errors --retry-connrefused --retry-delay 5 --retry-max-time 300 "$url/tokenizer.json" -o "$snap/tokenizer.json" \
 && echo "${VIPUNE_MODEL_SHA256_ONNX}  $snap/onnx/model.onnx" | sha256sum -c - \
 && echo "${VIPUNE_MODEL_SHA256_TOKENIZER}  $snap/tokenizer.json" | sha256sum -c - \
 && printf %s "$VIPUNE_MODEL_REVISION" >"$repo/refs/$VIPUNE_MODEL_REVISION" \
 && chown -R vscode:vscode /home/vscode/.cache

# The model is read as a cycle reads it: as that user, with the network shut
# off, by adding a memory to a scratch store and searching it back. A model
# vipune cannot find fails the build here, not in the next cycle. The vipune in
# the base has to be the version versions.env names, since that version picks
# the revision. Every call names the scratch store: any vipune call, version
# included, otherwise creates one in ~/.vipune, where a cycle mounts its own.
#
# The remap, and the step the two stages are drawn around: it is the only step of
# the image that reads the uid, it is the first one below the boundary, and it goes
# before any other step of this stage, since what they write is owned by whoever
# wrote it and the find below takes only what was there when the id moved.
#
# The remap cannot go higher. Each step above writes files, as root or as vscode at
# the id the base gave it, and a remap above them would leave what they wrote owned
# by the id that has since moved. The same find (-xdev, the image's own filesystem
# only) takes those files by the user's name once the id has moved, so the image
# ends as it did before the move. test/build_test.sh reads the image as the build
# sees it and refuses a step above the boundary that reads the id or holds it.
FROM shared
USER root

# An argument is scoped to the stage that declares it, so the steps of `shared`
# hold no value for it whatever the build passes. sandbox/build.sh passes the uid
# of the person who ran the build and refuses one that is not a number; the default
# is the id the base gave the user, so a build made without that argument holds the
# image as it came. The compare is numeric, so a value that is not a number stops
# the build here rather than trying to move the user to it.
ARG TEHAS_UID=1000
RUN if [ "${TEHAS_UID:-1000}" -ne 1000 ]; then \
      usermod -u "$TEHAS_UID" vscode && groupmod -g "$TEHAS_UID" vscode \
      && find / -xdev \( -user 1000 -o -group 1000 \) -exec chown -h vscode:vscode {} + ; \
    fi

# The model is read as a cycle reads it, as the user a container runs as: the id
# that user holds is the one the remap above gave it. The steps under it run as
# that user, or as root, exactly as they did before the remap moved here.
USER vscode
RUN db=$(mktemp -d) \
 && found=$(vipune version --db-path "$db/gate.db" | grep -o -E '[0-9]+\.[0-9]+\.[0-9]+' | head -n 1) \
 && if [ "${found:-}" != "$VIPUNE_VERSION" ]; then echo "sandbox: vipune is ${found:-not there}, versions.env names $VIPUNE_VERSION" >&2; exit 1; fi \
 && export HF_HUB_OFFLINE=1 HF_ENDPOINT=http://127.0.0.1:9 https_proxy=http://127.0.0.1:9 \
 && added=$(vipune add 'the sandbox image carries the embedding model' --json --db-path "$db/gate.db") \
 && answer=$(vipune search 'embedding model of the sandbox image' --json --limit 1 --db-path "$db/gate.db") \
 && rm -rf "$db" \
 && printf '%s' "$added" | jq -e '.status == "added"' >/dev/null \
 && printf '%s' "$answer" | jq -e '.results | length > 0' >/dev/null \
 || { echo "sandbox: vipune cannot embed with $VIPUNE_MODEL: $added $answer" >&2; exit 1; }; \
    echo "sandbox: vipune $VIPUNE_VERSION embeds and searches with $VIPUNE_MODEL"
USER root

# Playwright comes from the npm tree pinned beside this file, npm/playwright:
# package.json names one version and package-lock.json fixes every package in
# that tree by hash. The build stops when the tree is not the one versions.env
# records (build.sh passes its sha256 and its version here), and npm ci then
# refuses a package whose hash does not match the lockfile. Scripts are off for
# the install; the browsers are fetched by the command below, not by a package's
# postinstall hook.
COPY npm/playwright/package.json npm/playwright/package-lock.json /opt/pin/playwright/
# Browsers live outside any home directory so every user can read them.
ENV PLAYWRIGHT_BROWSERS_PATH=/opt/ms-playwright
RUN cd /opt/pin/playwright \
 && echo "${PLAYWRIGHT_LOCK_SHA256}  package-lock.json" | sha256sum -c - \
 && node -e 'const v=process.argv[1], p=require("./package.json"), l=require("./package-lock.json"); if (p.dependencies.playwright !== v || l.packages[""].dependencies.playwright !== v || l.packages["node_modules/playwright"].version !== v) { console.error("sandbox: the Playwright pin is not " + v); process.exit(1) }' "$PLAYWRIGHT_VERSION" \
 && npm ci --ignore-scripts --no-fund --no-audit \
 && ./node_modules/.bin/playwright install --with-deps chromium \
 && chmod -R a+rX /opt/ms-playwright \
 && rm -rf /var/lib/apt/lists/* /root/.npm /opt/pin

# gh reads the GitHub App's token the host keeps fresh, when the host gives the
# container that directory (host/sandbox/gh-shim). The image's own gh stays at
# /usr/bin/gh; this one is earlier on PATH.
COPY gh-shim /usr/local/bin/gh
RUN chmod 755 /usr/local/bin/gh && [ -x /usr/bin/gh ]

# Every command a cycle's instructions name, and every command tehas gives a
# container (by --entrypoint or after the image), has to answer there. A tool
# that quietly leaves the image costs a cycle a call each time an agent reaches
# for it, and teaches them the helper is unreliable; a build that loses one
# stops here instead. pi and pi-rukas's headless runner are the container's
# command for every cycle and every tehas dev session: without one of them no
# cycle starts at all, which today only shows up as a refused cycle. Near the
# end, so it checks what every step above put in the image. As the cycle's own
# user, so a tool that only root can run does not pass the gate; the image ends
# as root again so the steps after it are unaffected.
# test/sandbox_test.sh reads these commands out of the docker runs in libexec/
# and fails when one of them is not checked here.
USER vscode
RUN for tool in bash cargo curl fd file find gh git jq node pi python3 rg shellcheck codebase-memory-mcp /opt/pi-rukas/bin/pi-rukas-headless; do \
      command -v "$tool" >/dev/null 2>&1 || { echo "sandbox: $tool is not in this image" >&2; exit 1; }; \
    done \
 && python3 -c 'import PIL' \
 && if [ "$(jq --version)" != "jq-${JQ_VERSION}" ]; then echo "sandbox: a cycle finds $(jq --version), versions.env names jq-${JQ_VERSION}" >&2; exit 1; fi \
 && echo "sandbox: the tools a cycle reaches for are in the image"
USER root

# The pi-rukas commit the base was built from, the head build.sh found. The
# dispatcher and tehasbot cycle read it from the image, so an image put back
# for a rollback is known by its own commit. Last, so a new commit rebuilds
# nothing above it.
ARG PI_RUKAS_COMMIT=""

# Whether the pi-rukas in the base reads the manifest tehas writes beside the
# images it mounts for a cycle: its --capabilities answer lists
# PI_ENSEMBLE_ISSUE_IMAGES for manifest format 1. build.sh asks it and passes
# what it found; this asks the base actually being built on, so the label
# cannot disagree with the image. An image built without build.sh says
# nothing, and tehas reads that as "not said" rather than as an answer. Last,
# like the commit above.
ARG TEHAS_RUKAS_IMAGES=unknown
RUN given=${TEHAS_RUKAS_IMAGES:-unknown}; \
    caps=$(/opt/pi-rukas/bin/pi-rukas-headless --capabilities 2>/dev/null) \
      && reads=$(printf '%s' "$caps" | jq -e -r 'if any(.inputs[]? | objects; .name == "PI_ENSEMBLE_ISSUE_IMAGES" and .manifestVersion == 1) then 1 else 0 end') \
      || { echo "sandbox: the base image gave no --capabilities answer" >&2; exit 1; }; \
    echo "sandbox: the base image's pi-rukas $reads issue images"; \
    if [ "$given" = unknown ]; then exit 0; fi; \
    if [ "$given" != "$reads" ]; then \
      echo "sandbox: TEHAS_RUKAS_IMAGES says $given and the base says $reads: build with sandbox/build.sh" >&2; \
      exit 1; \
    fi

LABEL tehas.rukas=$PI_RUKAS_COMMIT tehas.rukas.images=$TEHAS_RUKAS_IMAGES
