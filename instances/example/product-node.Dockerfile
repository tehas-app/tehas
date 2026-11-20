# An instance layer that pins the product's own Node, so a tehas update does
# not move the Node its gates run on. To use it, copy it to sandbox.Dockerfile
# in the instance directory, fill in the two values below, and run
# ~/tehas/sandbox/build.sh and tehas check-product --sandbox.
#
# The version and its sum come from https://nodejs.org/dist/v<version>/SHASUMS256.txt,
# the line for node-v<version>-linux-x64.tar.xz. A value left empty fails the
# layer, and cycles keep running on the image without it.
#
# The product's Node goes in a prefix of its own and first on PATH, so the
# product's install, its gates and the agents' shell commands find its node and
# npm. Pi and the npm tools tehas ships run tehas's Node by its path. Do not
# unpack over /usr/local or /opt/tehas-node. See new-project.md, The product's
# Node.
ARG TEHAS_BASE
FROM $TEHAS_BASE
ARG PRODUCT_NODE_VERSION=
ARG PRODUCT_NODE_SHA256=
RUN test -n "$PRODUCT_NODE_VERSION" && test -n "$PRODUCT_NODE_SHA256" \
 && curl -fsSL "https://nodejs.org/dist/v${PRODUCT_NODE_VERSION}/node-v${PRODUCT_NODE_VERSION}-linux-x64.tar.xz" -o /tmp/node.tar.xz \
 && echo "${PRODUCT_NODE_SHA256}  /tmp/node.tar.xz" | sha256sum -c - \
 && mkdir -p /opt/product-node \
 && tar -xJf /tmp/node.tar.xz -C /opt/product-node --strip-components=1 --no-same-owner \
 && rm -f /tmp/node.tar.xz \
 && test "$(/opt/product-node/bin/node --version)" = "v${PRODUCT_NODE_VERSION}"
ENV PATH=/opt/product-node/bin:$PATH
