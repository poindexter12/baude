# Changelog

## 2.0.0-beta readiness

This source state ships as the `v2.0.0-beta` prerelease, published 2026-09-01
as a manually cut bootstrap of the beta channel (4-platform tarballs bundling
`baude` and `bauded`, plus `ghcr.io` images on the `beta` channel). Later
betas (`2.0.0-beta.1` and onward) will be cut by release-please. Formal phase
certification gates (Linux runtime certification, verification sign-off) are
tracked in `.planning/phases/07-local-tui-dogfood-release/`.

### Features

* durable repository parents with persisted oldest-first checkout children and deterministic restart initialization
* context-aware local TUI create-or-activate, retained close, explicit reopen, and separately confirmed clean managed-worktree removal with branch retention
* responsive wide and narrow hierarchy presentation while older remote session rows remain flat and non-destructive

### Safety and readiness

* shared lifecycle authority, exact durable repository/checkout identity, and fail-closed Git reconciliation across local actions
* exact `2.0.0-beta` workspace metadata, locked package checks, and two-binary supported-target artifact readiness
* isolated local source installation and manual dogfood instructions that do not replace an existing installation or authorize remote distribution


## [0.14.1](https://github.com/poindexter12/baude/compare/v0.14.0...v0.14.1) (2026-08-24)


### Bug Fixes

* pass ctrl-x through to terminal apps ([d06f047](https://github.com/poindexter12/baude/commit/d06f0479a7dda9d1d37b468a9ccac1323180f349))
* pass ctrl-x through to terminal apps ([8dcf016](https://github.com/poindexter12/baude/commit/8dcf0161fe04823b0c85651277f16c135c0c7576))

## [0.14.0](https://github.com/poindexter12/baude/compare/v0.13.0...v0.14.0) (2026-08-17)


### Features

* workspace display labels — named pools show their platform, claude reads as Claude Code ([fde1743](https://github.com/poindexter12/baude/commit/fde1743c0b27360c1056a25cb88e4867acdf8d58))
* workspace display labels — named pools show their platform, claude reads as Claude Code ([256ef7a](https://github.com/poindexter12/baude/commit/256ef7a377a3c89ec8d838d167e81d2a8570ee17))


### Bug Fixes

* resolve the session base command per backend — claude_cmd no longer poisons opencode spawns ([3826ba1](https://github.com/poindexter12/baude/commit/3826ba199c945fdccacebd530f916bfe178ce68c))
* resolve the session base command per backend — claude_cmd no longer poisons opencode spawns ([cd0ea87](https://github.com/poindexter12/baude/commit/cd0ea87b6c5e80fd1c1f787de9a257f038e49056))
* ship bauded in the release tarballs alongside baude ([5aed6f2](https://github.com/poindexter12/baude/commit/5aed6f2baf9816c631965644be03af53428399f1))
* ship bauded in the release tarballs alongside baude ([99b8b30](https://github.com/poindexter12/baude/commit/99b8b30005f7e5c4ded54364229e8744352dc321))

## [0.13.0](https://github.com/poindexter12/baude/compare/v0.12.0...v0.13.0) (2026-08-16)


### Features

* macOS desktop notifications when sessions need attention ([900c7d0](https://github.com/poindexter12/baude/commit/900c7d0abe794c3ca78aaa42c3313b71a2e85e45))
* macOS desktop notifications when sessions need attention ([790d6ce](https://github.com/poindexter12/baude/commit/790d6cedf50dc9208b2df0156b04a8d9934c83cf))
* workspaces — named session pools hard-bound to a backend ([30f9d53](https://github.com/poindexter12/baude/commit/30f9d5377cff09b4264cca7f64b247d3c9903ad5))
* workspaces — named session pools hard-bound to a backend ([a028bf2](https://github.com/poindexter12/baude/commit/a028bf2cbae6dad4a86d24cbc52ad5fad8306a18))

## [0.12.0](https://github.com/poindexter12/baude/compare/v0.11.0...v0.12.0) (2026-08-15)


### Features

* opencode backend — spawn, live metadata, and remote permission approval ([1b0479d](https://github.com/poindexter12/baude/commit/1b0479d4bbf7c9145034006f578a3d1448dcaf0b))
* opencode backend — spawn, live metadata, and remote permission approval ([36ae74d](https://github.com/poindexter12/baude/commit/36ae74d997b9ec59f5aea3ed56894d582bec0222))

## [0.11.0](https://github.com/poindexter12/baude/compare/v0.10.0...v0.11.0) (2026-08-13)


### Features

* alphabetical sidebar order, cycle into archive, configurable idle timeout ([c5a8196](https://github.com/poindexter12/baude/commit/c5a819677c08fd620872f6f5b0b8ea5a636ba401))
* alphabetical sidebar order, cycle into archive, configurable idle timeout ([64ba3e4](https://github.com/poindexter12/baude/commit/64ba3e4f7c50502207a974dad8e93222b0fb9dfe))

## [0.10.0](https://github.com/poindexter12/baude/compare/v0.9.0...v0.10.0) (2026-08-12)


### Features

* make ctrl+e/n/x global chords for editor, new session, close ([c44f224](https://github.com/poindexter12/baude/commit/c44f224c79149df4136306f802c540963f23e587))
* make ctrl+e/n/x global chords for editor, new session, close ([9705e35](https://github.com/poindexter12/baude/commit/9705e35844c66f38248603496df02b54cecc2cae))


### Bug Fixes

* keep input tail visible and support ctrl+u clear in prompts ([9686492](https://github.com/poindexter12/baude/commit/9686492769e3d0f5b954580ea316e017a36422e9))
* keep input tail visible and support ctrl+u clear in prompts ([477a2d2](https://github.com/poindexter12/baude/commit/477a2d24dcba38e76431daeaf6307503c47fc8d9))

## [0.9.0](https://github.com/poindexter12/baude/compare/v0.8.0...v0.9.0) (2026-08-04)


### Features

* clone a repo into a new session with the c key ([2c595f7](https://github.com/poindexter12/baude/commit/2c595f7869c9a10d8c73ef29860c4dabf335a95f))
* clone a repo into a new session with the c key ([b5d11ad](https://github.com/poindexter12/baude/commit/b5d11ad8ce45f9a514941c444fb00f223bbacdfa))
* fall through from the n prompt to the clone flow ([a014cc4](https://github.com/poindexter12/baude/commit/a014cc403aca8b70a1a6aeab72358de86ad094a2))

## [0.8.0](https://github.com/poindexter12/baude/compare/v0.7.4...v0.8.0) (2026-07-04)


### Features

* make sidebar selection and session grouping legible ([7276531](https://github.com/poindexter12/baude/commit/7276531a325c9343ee09250bd4e2c7b7895077b3))
* make sidebar selection and session grouping legible ([b3dafc3](https://github.com/poindexter12/baude/commit/b3dafc3785753ffc65fa7a112fecdffea04a34ab))


### Bug Fixes

* declare crate versions literally so release-please can bump them ([bcedff4](https://github.com/poindexter12/baude/commit/bcedff4bc28cdd3270c69e233f37f13b33cc4eda))
* declare crate versions literally so release-please can bump them ([a8cad78](https://github.com/poindexter12/baude/commit/a8cad78b9d35ee9d940aa41280ac4fb32024b014))

## Changelog

Releases from v0.7.4 onward are automated by [release-please](https://github.com/googleapis/release-please)
from Conventional Commits; entries below this line are appended automatically.
For releases up to and including v0.7.4, see the
[GitHub Releases](https://github.com/poindexter12/baude/releases) page.
