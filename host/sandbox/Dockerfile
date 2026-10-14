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
# versions.env. The base is pinned by digest and every tool by version:
# upgrade on purpose, then rebuild.
FROM ghcr.io/janit/pi-rukas@sha256:28f2bf34cc801cf8507b685f4345b4cff5ddc1d8b69126aed4b1f8de7b836951

ARG TEHAS_UID=1000
ARG SHELLCHECK_VERSION
ARG SHELLCHECK_SHA256
ARG PLAYWRIGHT_VERSION=1.55.1

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

# Browsers live outside any home directory so every user can read them.
ENV PLAYWRIGHT_BROWSERS_PATH=/opt/ms-playwright
RUN npx --yes "playwright@${PLAYWRIGHT_VERSION}" install --with-deps chromium \
 && chmod -R a+rX /opt/ms-playwright \
 && rm -rf /var/lib/apt/lists/* /root/.npm
