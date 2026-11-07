# The base of the tehas sandbox image: pi-rukas's runtime, built by tehas from
# the pi-rukas commit the host follows. sandbox/build.sh builds it from the
# host's pi-rukas clone (git archive of that commit, with this file added at
# its root), then builds host/sandbox/Dockerfile on top of it.
#
# It follows pi-rukas's .devcontainer/Dockerfile step by step, minus what a
# cycle never runs or what tehas's own layer installs again at pinned versions:
#   - ubuntu:24.04, not devcontainers/base: no VS Code features.
#   - vscode is made at the tehas user's uid in the first layer, so the remap of
#     host/sandbox/Dockerfile finds nothing to copy (it stored every file twice).
#   - rustup only: tehas's layer installs the pinned toolchain.
#   - vipune and double-o are built in a stage of their own; only the binaries
#     reach the image. That stage is ubuntu:24.04 too: vipune's embeddings link
#     the onnxruntime that ort-sys downloads, which needs the glibc (2.38 or
#     newer) and libstdc++ of the image it runs in; on Debian bookworm (glibc
#     2.36) it does not link.
#   - no glab, no wigolo, no codebase-memory-mcp from main, no huggingface_hub
#     and no model download: tehas's layer fetches the model itself.
# Kept: gh, git, node 22, bun, pi, ctx7, parallel-cli, vipune, oo, rg, fd,
# docker.io, tini, the build tools, the pi-rukas checkout with its extension's
# packages, install.sh and the entrypoint, and the everyday tools the
# devcontainer base brought.
#
# pi is the version pi-rukas's own recipe pins, read from that file in the
# checkout, so the base cannot drift from upstream's floor. vipune and oo are
# the versions host/versions.env names, the ones tehas's layer checks.

FROM ubuntu:24.04 AS cargo-tools
ARG VIPUNE_VERSION
ARG OO_VERSION
ENV DEBIAN_FRONTEND=noninteractive RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl pkg-config libssl-dev g++ make perl \
 && rm -rf /var/lib/apt/lists/*
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --no-modify-path --profile minimal --default-toolchain stable
RUN test -n "$VIPUNE_VERSION" && test -n "$OO_VERSION" \
 && cargo install --locked vipune --version "$VIPUNE_VERSION" --root /out \
 && cargo install --locked double-o --version "$OO_VERSION" --root /out
# The binaries run here, in the distro of the image, so one that does not start
# stops the base build rather than the first cycle. vipune names a scratch store,
# as the sandbox layer's check does; its embeddings are read there.
RUN db=$(mktemp -d) && /out/bin/vipune version --db-path "$db/v.db" && rm -rf "$db" \
 && /out/bin/oo --help >/dev/null

FROM ubuntu:24.04
ARG TEHAS_UID
ENV DEBIAN_FRONTEND=noninteractive LANG=C.UTF-8 LC_ALL=C.UTF-8
USER root

# pi-rukas's package list, plus what the devcontainer base brought that the
# image relies on: procps and openssh-client.
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl wget gnupg lsb-release \
      git jq tree less unzip xz-utils \
      build-essential pkg-config libssl-dev \
      python3 python3-pip \
      sudo procps openssh-client \
      ripgrep fd-find \
      docker.io \
      util-linux \
      tini \
 && rm -rf /var/lib/apt/lists/* \
 && ln -sf /usr/bin/fdfind /usr/local/bin/fd

# ubuntu:24.04 has a user "ubuntu" at 1000; it goes so the ids cannot clash.
RUN test -n "$TEHAS_UID" \
 && { userdel -r ubuntu 2>/dev/null || true; } \
 && groupadd -g "$TEHAS_UID" vscode \
 && useradd -m -u "$TEHAS_UID" -g vscode -s /bin/bash vscode \
 && echo 'vscode ALL=(ALL) NOPASSWD:ALL' >/etc/sudoers.d/vscode && chmod 440 /etc/sudoers.d/vscode

RUN install -m 0755 -d /etc/apt/keyrings \
 && curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg -o /etc/apt/keyrings/githubcli-archive-keyring.gpg \
 && chmod a+r /etc/apt/keyrings/githubcli-archive-keyring.gpg \
 && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" >/etc/apt/sources.list.d/github-cli.list \
 && apt-get update && apt-get install -y --no-install-recommends gh \
 && rm -rf /var/lib/apt/lists/*

RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
 && apt-get install -y --no-install-recommends nodejs \
 && rm -rf /var/lib/apt/lists/* \
 && npm install -g npm@latest && npm cache clean --force

RUN curl -fsSL https://bun.sh/install | BUN_INSTALL=/usr/local bash \
 && chmod +x /usr/local/bin/bun && bun --version

ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --no-modify-path --profile minimal --default-toolchain none \
 && chmod -R a+rwX "$CARGO_HOME" "$RUSTUP_HOME"

COPY --from=cargo-tools /out/bin/vipune /out/bin/oo /usr/local/bin/

RUN --mount=type=bind,source=.devcontainer/Dockerfile,target=/tmp/upstream.Dockerfile \
    pi=$(grep -oE '@earendil-works/pi-coding-agent@[0-9]+\.[0-9]+\.[0-9]+' /tmp/upstream.Dockerfile | head -n 1) \
 && if [ -z "$pi" ]; then echo "base: pi-rukas's recipe pins no pi-coding-agent version" >&2; exit 1; fi \
 && npm install -g --ignore-scripts "$pi" \
 && (npm install -g --ignore-scripts ctx7 || true) \
 && (npm install -g parallel-web-cli || true) \
 && npm cache clean --force

ENV PI_ENSEMBLE_DIR=/opt/pi-rukas
COPY --chown=vscode:vscode . ${PI_ENSEMBLE_DIR}
ARG GITHUB_SHA=
RUN set -e; if printf '%s' "$GITHUB_SHA" | grep -Eq '^[0-9a-f]{40}$'; then \
      mkdir -p ${PI_ENSEMBLE_DIR}/.pi && printf '%s\n' "$GITHUB_SHA" > ${PI_ENSEMBLE_DIR}/.pi/pi-rukas-commit; \
    fi

USER vscode
ENV PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin HOME=/home/vscode
RUN cd ${PI_ENSEMBLE_DIR}/extension && bun install --frozen-lockfile && rm -rf /home/vscode/.bun/install/cache
RUN cd ${PI_ENSEMBLE_DIR} && ./install.sh || true
RUN test ! -e "$HOME/.pi/agent/npm/node_modules/pi-mcp-adapter" \
 && test ! -e "$HOME/.pi/agent/extensions/pi-mcp-adapter"

USER root
RUN ln -sf ${PI_ENSEMBLE_DIR}/bin/pi-rukas-headless /usr/local/bin/pi-rukas-headless \
 && pi-rukas-headless --capabilities > /tmp/caps.json && jq -e .schemaVersion /tmp/caps.json >/dev/null && rm -f /tmp/caps.json

USER vscode
RUN mkdir -p /home/vscode/.pi/agent/sessions /home/vscode/.pi/agent/ensemble-runs /home/vscode/.pi/agent/review-state \
             /home/vscode/.vipune /home/vscode/.cache /home/vscode/.bun /home/vscode/.cargo
USER root
RUN mkdir -p /commandhistory && chown vscode:vscode /commandhistory
COPY --chown=vscode:vscode .devcontainer/entrypoint.sh /usr/local/bin/pi-rukas-entrypoint
RUN chmod +x /usr/local/bin/pi-rukas-entrypoint

# Everyday tools an agent may reach for: what the devcontainer base had on its
# PATH, less man pages, zsh, gettext tooling, glab and the VS Code helpers, and
# a few common ones neither image had.
RUN apt-get update && apt-get install -y --no-install-recommends \
      rsync lsof strace iproute2 net-tools psmisc zip bsdextrautils gettext-base \
      vim-tiny nano htop ncdu sq \
      zstd p7zip-full gawk yq sqlite3 bc dnsutils netcat-openbsd iputils-ping git-lfs tmux \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace
ENV PI_ENSEMBLE_SANDBOX_MODE=1
ENTRYPOINT ["/usr/bin/tini", "-s", "--", "/usr/local/bin/pi-rukas-entrypoint"]
CMD ["pi"]
