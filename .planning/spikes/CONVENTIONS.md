# Spike Conventions

Patterns and stack choices established across spike sessions. New spikes follow these unless the question requires otherwise.

## Stack

- Plain Node ESM scripts (`run-spike.mjs`), zero npm dependencies — global `fetch`, manual SSE parsing, `node:child_process` for spawning servers. The repo is Rust, but spikes probing external HTTP/JS surfaces stay in Node for speed.

## Structure

- One directory per spike: `NNN[-letter]-name/` containing `run-spike.mjs`, `README.md`, and a disposable `sandbox/` the script recreates from scratch on every run (never committed state the spike depends on).
- Ports: fixed per spike in the 147xx range (001a=14711, 001b=14712) so parallel reruns don't collide.

## Patterns

- **Forensic log layer:** every observed event and action appended to `events.jsonl` with ISO timestamps and a category tag; structured verdict written to `result.json`; raw server output to `serve.log`. Exit code encodes the verdict (0 validated, 1 invalidated, 2 error).
- **Prove deferral, not just outcome:** when validating a blocking/approval flow, hold the decision for a few seconds and assert the side effect did NOT happen during the hold, then assert it did (approve) or never did (reject) after.
- **Trust the wire, not the types:** opencode's bundled SDK `.d.ts` drifted from live server behavior twice in one spike. Write predicates against observed payloads, note the drift in the README.

## Tools & Libraries

- opencode 1.18.16 via mise; sandboxes inherit Joe's global `~/.config/opencode/opencode.json` model (`github-copilot/gpt-5.4`), so spike prompts hit a real model — keep prompts single-tool and tiny.
