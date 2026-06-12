#!/bin/sh
set -e

# Seed the statusline bridge on first run so session cost, context %, and
# account rate limits flow into bauded (they only exist in the JSON Claude
# Code pipes to statusLine commands). Never touch an existing settings.json.
SETTINGS="${CLAUDE_CONFIG_DIR:-$HOME/.claude}/settings.json"
if [ ! -f "$SETTINGS" ]; then
  mkdir -p "$(dirname "$SETTINGS")"
  printf '{\n  "statusLine": { "type": "command", "command": "baude statusline" }\n}\n' > "$SETTINGS"
fi

exec bauded "$@"
