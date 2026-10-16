# The tehas sandbox image: the pi-rukas image plus what cycles need on top.
#
# Adds what the upstream image lacks:
#   - the in-container user remapped to the tehas user's id (upstream hardcodes
#     vscode = UID 1000; with another id on the host, bind-mounted host
#     directories are not writable),
#   - shellcheck, which gates commonly run (.pi/verify-cmd, .pi/smoke-cmd),
#   - cargo's registry given to that user (the upstream image has Rust, and
#     a registry only root can write, so a gate could not fetch a crate),
#   - Playwright's Chromium with its system libraries and fonts, for a
#     product's browser checks,
#   - `file` and a Python image library, so a cycle can look at a picture,
#   - the code indexer the agents' code search answers from, at the version
#     versions.env names rather than whatever upstream's main carried,
#   - no wigolo: the base ships one that fails on every call.
#
# A product whose gates need another toolchain adds it here.
#
# Build with sandbox/build.sh, which passes the user id, the versions from
# versions.env and the base: upstream's image for the pi-rukas commit
# versions.env follows. shellcheck is pinned by checksum, Playwright by a
# lockfile beside this file, and codebase-memory-mcp by its installer's
# checksum plus a version read back from the binary. A step near the end checks
# that every command a cycle reaches for, and every command tehas gives a
# container, answers in the image.
ARG PI_RUKAS_IMAGE
FROM ${PI_RUKAS_IMAGE}

ARG TEHAS_UID=1000
ARG SHELLCHECK_VERSION
ARG SHELLCHECK_SHA256
ARG PLAYWRIGHT_VERSION
ARG PLAYWRIGHT_LOCK_SHA256
ARG CBM_VERSION
ARG CBM_INSTALLER_SHA256

USER root

RUN if [ "$TEHAS_UID" != 1000 ]; then \
      usermod -u "$TEHAS_UID" vscode && groupmod -g "$TEHAS_UID" vscode \
      && find / -xdev \( -user 1000 -o -group 1000 \) -exec chown -h vscode:vscode {} + ; \
    fi

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
# with is read back from the binary. It installs into /usr/local/bin, where the
# base's copy already is: the installer configures PATH for a directory that is
# not on it, and fails when it cannot.
RUN curl -fsSL "https://raw.githubusercontent.com/DeusData/codebase-memory-mcp/v${CBM_VERSION}/install.sh" -o /tmp/cbm-install.sh \
 && echo "${CBM_INSTALLER_SHA256}  /tmp/cbm-install.sh" | sha256sum -c - \
 && env HOME=/root CBM_DOWNLOAD_URL="https://github.com/DeusData/codebase-memory-mcp/releases/download/v${CBM_VERSION}" \
     bash /tmp/cbm-install.sh --skip-config --dir=/usr/local/bin \
 && rm -f /tmp/cbm-install.sh /usr/local/bin/install.sh \
 && found=$(codebase-memory-mcp --version 2>/dev/null | grep -o -E '[0-9]+\.[0-9]+\.[0-9]+' | head -n 1) \
 && if [ "${found:-}" != "$CBM_VERSION" ]; then echo "sandbox: codebase-memory-mcp is ${found:-not there}, versions.env names $CBM_VERSION" >&2; exit 1; fi \
 && echo "sandbox: codebase-memory-mcp $CBM_VERSION"

RUN curl -fsSL "https://github.com/koalaman/shellcheck/releases/download/v${SHELLCHECK_VERSION}/shellcheck-v${SHELLCHECK_VERSION}.linux.x86_64.tar.xz" -o /tmp/shellcheck.tar.xz \
 && echo "${SHELLCHECK_SHA256}  /tmp/shellcheck.tar.xz" | sha256sum -c - \
 && tar -xJf /tmp/shellcheck.tar.xz -C /tmp \
 && install -m 755 "/tmp/shellcheck-v${SHELLCHECK_VERSION}/shellcheck" /usr/local/bin/shellcheck \
 && rm -rf /tmp/shellcheck.tar.xz "/tmp/shellcheck-v${SHELLCHECK_VERSION}" \
 && shellcheck --version

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

# Every command a cycle's instructions name, and every command tehas gives a
# container (by --entrypoint or after the image), has to answer there. A tool
# that quietly leaves the image costs a cycle a call each time an agent reaches
# for it, and teaches them the helper is unreliable; a build that loses one
# stops here instead. pi and pi-rukas's headless runner are the container's
# command for every cycle and every tehas dev session: without one of them no
# cycle starts at all, which today only shows up as a refused cycle. Near the
# end, so it checks what every step above put in the image.
# test/sandbox_test.sh reads these commands out of the docker runs in libexec/
# and fails when one of them is not checked here.
RUN for tool in bash cargo curl fd file find gh git jq node pi python3 rg shellcheck codebase-memory-mcp /opt/pi-rukas/bin/pi-rukas-headless; do \
      command -v "$tool" >/dev/null 2>&1 || { echo "sandbox: $tool is not in this image" >&2; exit 1; }; \
    done \
 && python3 -c 'import PIL' \
 && echo "sandbox: the tools a cycle reaches for are in the image"

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
