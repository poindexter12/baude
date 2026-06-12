# bauded runtime: the headless session daemon plus everything its child
# sessions need (claude CLI, git, ssh). See docs/remote-daemon-plan.md.
#
# No node: Claude Code ships a self-contained native binary via its official
# installer, so the base is plain debian-slim.

FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
# baude comes along for its `baude statusline` bridge subcommand.
RUN cargo build --release -p bauded -p baude

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates curl git openssh-client procps \
    && rm -rf /var/lib/apt/lists/* \
    # Claude Code refuses bypassPermissions as root — sessions run as baude.
    && groupadd -g 1000 baude \
    && useradd -m -u 1000 -g baude -s /bin/bash baude \
    && mkdir -p /repos && chown baude:baude /repos

COPY --from=build /src/target/release/bauded /src/target/release/baude /usr/local/bin/
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh

# Native claude install (standalone binary, no node runtime).
USER baude
RUN curl -fsSL https://claude.ai/install.sh | bash
USER root
RUN ln -s /home/baude/.local/bin/claude /usr/local/bin/claude

USER baude
# Pre-create volume mountpoints so named volumes inherit baude's ownership.
RUN mkdir -p /home/baude/.claude /home/baude/.config/baude /home/baude/.ssh

# Sessions spawn under $SHELL -il (baude-core falls back to zsh, absent here).
ENV SHELL=/bin/bash
# Keep claude's .claude.json (onboarding/trust state) inside the volume —
# without this it lands at ~/.claude.json and is lost on container recreate.
ENV CLAUDE_CONFIG_DIR=/home/baude/.claude
# Updates arrive via image pulls; in-place self-updates would be ephemeral.
ENV DISABLE_AUTOUPDATER=1
# Inside the tailscale sidecar's netns; nothing is published to the host.
ENV BAUDED_BIND=0.0.0.0:8642

EXPOSE 8642
ENTRYPOINT ["docker-entrypoint.sh"]
