# bauded runtime: the headless session daemon plus everything its child
# sessions need (claude CLI, git, ssh). See docs/remote-daemon-plan.md.

FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
# baude comes along for its `baude statusline` bridge subcommand.
RUN cargo build --release -p bauded -p baude

FROM node:22-bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates curl git openssh-client procps \
    && rm -rf /var/lib/apt/lists/* \
    && npm install -g @anthropic-ai/claude-code \
    && mkdir -p /repos && chown node:node /repos

COPY --from=build /src/target/release/bauded /src/target/release/baude /usr/local/bin/

# Claude Code refuses bypassPermissions as root — run as the stock node user.
USER node
# Pre-create volume mountpoints so named volumes inherit node's ownership.
RUN mkdir -p /home/node/.claude /home/node/.config/baude /home/node/.ssh

# Sessions spawn under $SHELL -il (baude-core falls back to zsh, absent here).
ENV SHELL=/bin/bash
# Inside the tailscale sidecar's netns; nothing is published to the host.
ENV BAUDED_BIND=0.0.0.0:8642

EXPOSE 8642
ENTRYPOINT ["bauded"]
