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
#   - ubuntu by digest, and node, gh, bun, rustup, pi, ctx7 and parallel-cli at
#     the versions host/versions.env names: each download is checked against
#     its sha256 there before it runs, and each npm tool is installed from its
#     tree in host/npm, as bootstrap.sh installs them on the host. Nothing is
#     piped from the network into a shell, and nothing is taken at latest.
#   - vipune and double-o are built in a stage of their own; only the binaries
#     reach the image. That stage is ubuntu:24.04 too: vipune's embeddings link
#     the onnxruntime that ort-sys downloads, which needs the glibc (2.38 or
#     newer) and libstdc++ of the image it runs in; on Debian bookworm (glibc
#     2.36) it does not link.
#   - no glab, no wigolo, no codebase-memory-mcp from main, no huggingface_hub
#     and no model download: tehas's layer fetches the model itself.
# Kept: gh, git, node, bun, pi, ctx7, parallel-cli, vipune, oo, rg, fd,
# docker.io, tini, the build tools, the pi-rukas checkout with its extension's
# packages, install.sh and the entrypoint, and the everyday tools the
# devcontainer base brought.
#
# pi is PI_VERSION, which may not be older than the version pi-rukas's own
# recipe pins: that file in the checkout is upstream's floor. vipune and oo are
# the versions host/versions.env names, the ones tehas's layer checks.
#
# The ubuntu digest is the index of ubuntu:24.04, from
# `docker buildx imagetools inspect ubuntu:24.04`. Both FROM lines name it.
# The npm trees are the build context named npm (sandbox/build.sh passes
# host/npm as it is installed on the host).

FROM ubuntu:24.04@sha256:2198e4c65076e9cb0b3d695219df667c0e3b0072da64ca3c2648aca17dcecaf5 AS cargo-tools
ARG VIPUNE_VERSION
ARG OO_VERSION
ARG RUST_VERSION
ARG RUSTUP_VERSION
ARG RUSTUP_SHA256
ENV DEBIAN_FRONTEND=noninteractive RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl pkg-config libssl-dev g++ make perl \
 && rm -rf /var/lib/apt/lists/*
RUN test -n "$RUST_VERSION" \
 && curl -fsSL --proto '=https' --tlsv1.2 "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/x86_64-unknown-linux-gnu/rustup-init" -o /tmp/rustup-init \
 && echo "${RUSTUP_SHA256}  /tmp/rustup-init" | sha256sum -c - \
 && chmod +x /tmp/rustup-init \
 && /tmp/rustup-init -y --no-modify-path --profile minimal --default-toolchain "$RUST_VERSION" \
 && rm -f /tmp/rustup-init
RUN test -n "$VIPUNE_VERSION" && test -n "$OO_VERSION" \
 && cargo install --locked vipune --version "$VIPUNE_VERSION" --root /out \
 && cargo install --locked double-o --version "$OO_VERSION" --root /out
# The binaries run here, in the distro of the image, so one that does not start
# stops the base build rather than the first cycle. vipune names a scratch store,
# as the sandbox layer's check does; its embeddings are read there.
RUN db=$(mktemp -d) && /out/bin/vipune version --db-path "$db/v.db" && rm -rf "$db" \
 && /out/bin/oo --help >/dev/null

FROM ubuntu:24.04@sha256:2198e4c65076e9cb0b3d695219df667c0e3b0072da64ca3c2648aca17dcecaf5
ARG TEHAS_UID
ARG NODE_VERSION
ARG NODE_SHA256
ARG GH_VERSION
ARG GH_SHA256
ARG BUN_VERSION
ARG BUN_SHA256
ARG RUSTUP_VERSION
ARG RUSTUP_SHA256
ARG PI_VERSION
ARG PI_LOCK_SHA256
ARG CTX7_VERSION
ARG CTX7_LOCK_SHA256
ARG PARALLEL_CLI_VERSION
ARG PARALLEL_CLI_LOCK_SHA256
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

# gh at /usr/bin/gh, where host/sandbox/gh-shim looks for it.
RUN curl -fsSL "https://github.com/cli/cli/releases/download/v${GH_VERSION}/gh_${GH_VERSION}_linux_amd64.tar.gz" -o /tmp/gh.tar.gz \
 && echo "${GH_SHA256}  /tmp/gh.tar.gz" | sha256sum -c - \
 && tar -xzf /tmp/gh.tar.gz -C /tmp \
 && install -m 755 "/tmp/gh_${GH_VERSION}_linux_amd64/bin/gh" /usr/bin/gh \
 && rm -rf /tmp/gh.tar.gz "/tmp/gh_${GH_VERSION}_linux_amd64" \
 && gh --version

# node with the npm it ships.
RUN curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-x64.tar.xz" -o /tmp/node.tar.xz \
 && echo "${NODE_SHA256}  /tmp/node.tar.xz" | sha256sum -c - \
 && tar -xJf /tmp/node.tar.xz -C /usr/local --strip-components=1 --no-same-owner \
 && rm -f /tmp/node.tar.xz \
 && node --version && npm --version

RUN curl -fsSL "https://github.com/oven-sh/bun/releases/download/bun-v${BUN_VERSION}/bun-linux-x64.zip" -o /tmp/bun.zip \
 && echo "${BUN_SHA256}  /tmp/bun.zip" | sha256sum -c - \
 && unzip -q /tmp/bun.zip -d /tmp \
 && install -m 755 /tmp/bun-linux-x64/bun /usr/local/bin/bun \
 && ln -sf bun /usr/local/bin/bunx \
 && rm -rf /tmp/bun.zip /tmp/bun-linux-x64 \
 && bun --version

ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN curl -fsSL --proto '=https' --tlsv1.2 "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/x86_64-unknown-linux-gnu/rustup-init" -o /tmp/rustup-init \
 && echo "${RUSTUP_SHA256}  /tmp/rustup-init" | sha256sum -c - \
 && chmod +x /tmp/rustup-init \
 && /tmp/rustup-init -y --no-modify-path --profile minimal --default-toolchain none \
 && rm -f /tmp/rustup-init \
 && chmod -R a+rwX "$CARGO_HOME" "$RUSTUP_HOME"

COPY --from=cargo-tools /out/bin/vipune /out/bin/oo /usr/local/bin/

# Each npm tool from its pinned tree, as bootstrap.sh installs it: the lockfile
# has to be the file whose sha256 versions.env records, and package.json and the
# lockfile have to name the version there; npm ci then refuses any package whose
# hash is not the lockfile's. parallel-web-cli keeps its install script, which
# fetches its binary; the others run none.
COPY --from=npm pi/package.json pi/package-lock.json /opt/npm/pi/
COPY --from=npm ctx7/package.json ctx7/package-lock.json /opt/npm/ctx7/
COPY --from=npm parallel-web-cli/package.json parallel-web-cli/package-lock.json /opt/npm/parallel-web-cli/
RUN --mount=type=bind,source=.devcontainer/Dockerfile,target=/tmp/upstream.Dockerfile \
    floor=$(grep -oE '@earendil-works/pi-coding-agent@[0-9]+\.[0-9]+\.[0-9]+' /tmp/upstream.Dockerfile | head -n 1 | sed 's/.*@//') \
 && if [ -z "$floor" ]; then echo "base: pi-rukas's recipe pins no pi-coding-agent version" >&2; exit 1; fi \
 && if [ "$(printf '%s\n%s\n' "$floor" "$PI_VERSION" | sort -V | head -n 1)" != "$floor" ]; then \
      echo "base: versions.env names pi $PI_VERSION, older than the $floor pi-rukas's recipe pins" >&2; exit 1; fi \
 && pin() { \
      cd "/opt/npm/$1" \
      && echo "$4  package-lock.json" | sha256sum -c - \
      && node -e 'const [p, v] = process.argv.slice(1), j = require("./package.json"), l = require("./package-lock.json"); if (j.dependencies[p] !== v || l.packages[""].dependencies[p] !== v || l.packages["node_modules/" + p].version !== v) { console.error("base: the npm pin " + p + " is not " + v); process.exit(1) }' "$2" "$3" \
      && pkg=$2 && shift 4 && npm ci --no-fund --no-audit "$@" \
      && bins=$(node -p 'const p = process.argv[1], b = require("./node_modules/" + p + "/package.json").bin; typeof b === "string" ? p.split("/").pop() : Object.keys(b || {}).join(" ")' "$pkg") \
      && test -n "$bins" \
      && for bin in $bins; do test -x "node_modules/.bin/$bin" && ln -sf "$PWD/node_modules/.bin/$bin" "/usr/local/bin/$bin" || exit 1; done; \
    } \
 && pin pi @earendil-works/pi-coding-agent "$PI_VERSION" "$PI_LOCK_SHA256" --ignore-scripts \
 && pin ctx7 ctx7 "$CTX7_VERSION" "$CTX7_LOCK_SHA256" --ignore-scripts \
 && pin parallel-web-cli parallel-web-cli "$PARALLEL_CLI_VERSION" "$PARALLEL_CLI_LOCK_SHA256" \
 && command -v pi >/dev/null && command -v ctx7 >/dev/null && command -v parallel-cli >/dev/null \
 && rm -rf /root/.npm

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
