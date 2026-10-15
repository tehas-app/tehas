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
#     product's browser checks.
#
# A product whose gates need another toolchain adds it here.
#
# Build with sandbox/build.sh, which passes the user id and the versions from
# versions.env. The base is pinned by digest, shellcheck by checksum, and
# Playwright by a lockfile beside this file: upgrade on purpose, then rebuild.
FROM ghcr.io/trail-openers/pi-rukas:v0.12.79@sha256:bb9c91281187298d238cc793ceb18f8c06cd39284078b9a494c11118c7e97d3f

ARG TEHAS_UID=1000
ARG SHELLCHECK_VERSION
ARG SHELLCHECK_SHA256
ARG PLAYWRIGHT_VERSION
ARG PLAYWRIGHT_LOCK_SHA256

USER root

RUN if [ "$TEHAS_UID" != 1000 ]; then \
      usermod -u "$TEHAS_UID" vscode && groupmod -g "$TEHAS_UID" vscode \
      && find / -xdev \( -user 1000 -o -group 1000 \) -exec chown -h vscode:vscode {} + ; \
    fi

# What a gate fetches stays in its own container and goes with it: a registry
# kept between cycles would be code one cycle leaves for the next one's gate.
RUN mkdir -p /usr/local/cargo/registry && chown -R vscode:vscode /usr/local/cargo/registry

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

# The pi-rukas commit the base was built from, as versions.env names it. The
# dispatcher and tehasbot cycle read it from the image, so an image put back
# for a rollback is known by its own commit. Last, so a new commit rebuilds
# nothing above it.
ARG PI_RUKAS_COMMIT=""

# Whether the pi-rukas in the base reads the manifest tehas writes beside the
# images it mounts for a cycle. The reader is a file of pi-rukas's own source,
# and the base image carries that source, so the base can be asked directly.
# build.sh asks it and passes what it found; this checks that answer against
# the base actually being built on, so the label cannot disagree with the
# image. An image built without build.sh says nothing, and tehas reads that as
# "not said" rather than as an answer. Last, like the commit above.
ARG TEHAS_RUKAS_IMAGES=unknown
RUN given=${TEHAS_RUKAS_IMAGES:-unknown}; reads=0; \
    if test -e /opt/pi-rukas/extension/src/work-driver-issue-images.ts; then reads=1; fi; \
    echo "sandbox: the base image's pi-rukas $reads issue images"; \
    if [ "$given" = unknown ]; then exit 0; fi; \
    if [ "$given" != "$reads" ]; then \
      echo "sandbox: TEHAS_RUKAS_IMAGES says $given and the base says $reads: build with sandbox/build.sh" >&2; \
      exit 1; \
    fi

LABEL tehas.rukas=$PI_RUKAS_COMMIT tehas.rukas.images=$TEHAS_RUKAS_IMAGES
