# Changelog

## [2.0.0-beta.1](https://github.com/poindexter12/baude/compare/v0.14.1...v2.0.0-beta.1) (2026-09-03)


### Features

* **05-01:** implement canonical repository discovery ([1575b2f](https://github.com/poindexter12/baude/commit/1575b2fa538dc6eb530a545aacd2783996c9cbeb))
* **05-01:** resolve and ensure verified default worktree ([6446f89](https://github.com/poindexter12/baude/commit/6446f894925128feae06884c22fb1be07fa95c1c))
* **05-02:** fail visibly and replace state atomically ([efd0e5c](https://github.com/poindexter12/baude/commit/efd0e5ce6c9ca67e56706e597a5474fc6fc78bf5))
* **05-02:** implement validated durable repository state ([85756d1](https://github.com/poindexter12/baude/commit/85756d144c2f1bc48c9bdc482161e5f3e2cad13f))
* **05-02:** migrate selected legacy workspace state ([acde27c](https://github.com/poindexter12/baude/commit/acde27c8b14fa0bcb32225bf3916067cb042a943))
* **05-03:** admit durable primary sessions idempotently ([caace91](https://github.com/poindexter12/baude/commit/caace918d0adbe665afacae6719ab9a229389d21))
* **05-03:** converge admission routes and secure daemon state ([9cf4fbb](https://github.com/poindexter12/baude/commit/9cf4fbbc7d42167a70c8f102d0d4a92e9ad753b8))
* **05-03:** reconcile durable intent before runtime actions ([1aca2a2](https://github.com/poindexter12/baude/commit/1aca2a29f07c6f052a5f34af4fded5078efc0aaa))
* **06-01:** implement verified branch activation ([ec5926e](https://github.com/poindexter12/baude/commit/ec5926e946d08608db7a737d7638a23971a461de))
* **06-01:** wire durable activation to runtime owners ([ea70706](https://github.com/poindexter12/baude/commit/ea707066f88502a6319a0e8ecd0a4eefe27d145a))
* **06-02:** harden managed branch creation safety ([838b6d0](https://github.com/poindexter12/baude/commit/838b6d09b04822078250d69b5dc7e51cf7ccd31d))
* **06-02:** make creation rollback commit-aware ([39ec427](https://github.com/poindexter12/baude/commit/39ec427dca574d90d4e22c8fb8dcd12f6de5dc22))
* **06-03:** parse removal status fail closed ([9f5d660](https://github.com/poindexter12/baude/commit/9f5d660176683892bcad767fe96a4fb1b7f8a239))
* **06-03:** require verified removal topology ([5b7cf76](https://github.com/poindexter12/baude/commit/5b7cf769a8caf829ed1f3e77110d2e7d5a2bd5ae))
* **06-04:** execute commit-aware retained close ([a6cb2a3](https://github.com/poindexter12/baude/commit/a6cb2a39c1dacbd5fa800f759870a94a988b4c52))
* **06-04:** model complete retained close state ([e0cc63e](https://github.com/poindexter12/baude/commit/e0cc63e502fee4246e8236c6a5bd05476d17a9ae))
* **06-05:** plan fail-closed retained checkout reopen ([f41f675](https://github.com/poindexter12/baude/commit/f41f6753fb49a32f1ccc51634b55a69795e2c404))
* **06-05:** reopen retained checkouts through shared plan ([0c0c918](https://github.com/poindexter12/baude/commit/0c0c9186a1b7bdbd5de5eabb1fca36e8ad95937d))
* **06-05:** transport targeted resume IDs opaquely ([dd68bd1](https://github.com/poindexter12/baude/commit/dd68bd1901a7a83a2dc0da64956ef180af900026))
* **06-06:** execute compensating double-preflight removal ([9595660](https://github.com/poindexter12/baude/commit/95956600844e4b7545b85db57ced800b78f63219))
* **06-06:** separate close from confirmed physical removal ([6760585](https://github.com/poindexter12/baude/commit/67605857a0a3c8dbebbd3d0b51d1cf8688bd8bb5))
* **06-06:** verify plain worktree removal postconditions ([06198c8](https://github.com/poindexter12/baude/commit/06198c86cdb30699c65243a4753b8aa2215c3b3b))
* **06-07:** cut owners over to lifecycle engine ([7dad443](https://github.com/poindexter12/baude/commit/7dad443198c72a7a5e4fcc416b3558a36e086039))
* **06-07:** enforce registered runtime ownership ([6d9ebb4](https://github.com/poindexter12/baude/commit/6d9ebb41f6b78941f65c50bbfa583b4094bed836))
* **06-07:** establish durable lifecycle authority ([06c2076](https://github.com/poindexter12/baude/commit/06c20764b87fd21beceb420556fb7e421e3cafdb))
* **06-07:** unify lifecycle adapter semantics ([c97bbaf](https://github.com/poindexter12/baude/commit/c97bbaf1d41fc647355452aa6c9034d217051ea0))
* **07-01:** stabilize hierarchy selection across refresh ([5252f4f](https://github.com/poindexter12/baude/commit/5252f4f099320b8ffbb0f11b04d1cf08198b69ab))
* **07-01:** trace durable hierarchy into Ratatui ([02cdfe7](https://github.com/poindexter12/baude/commit/02cdfe76ab710c0c707d835400a7cee34e10646d))
* **07-02:** dispatch capability-gated local actions ([b661784](https://github.com/poindexter12/baude/commit/b66178425507a0ce68b90f397aa398853e8ce289))
* **07-02:** project lifecycle action capabilities ([7e0391b](https://github.com/poindexter12/baude/commit/7e0391b3594bf95681bcfa6949af5734ce8ece6c))
* **07-03:** complete hierarchy interaction copy ([2fa1769](https://github.com/poindexter12/baude/commit/2fa1769134a41d7bd74cfe9dd6d6abd7bda103b1))
* **07-03:** enforce responsive hierarchy viewport ([42527e7](https://github.com/poindexter12/baude/commit/42527e73aeb9ad4ba560a3aefa8fece2c69ce6bf))
* **07-04:** automate isolated restart dogfood ([a402d03](https://github.com/poindexter12/baude/commit/a402d038f932e95d3b4d4f416debbc5cc37260b1))
* **07-04:** prove flat remote compatibility ([da493aa](https://github.com/poindexter12/baude/commit/da493aaed38ad8e1d60d2c6e73f367fcc0cf9faa))
* **07:** add standalone sessions and checkout-first hierarchy ([82faef9](https://github.com/poindexter12/baude/commit/82faef9019942beac219a87ca9566d0bec55b50a))
* **07:** auto-populate existing worktrees as checkout rows ([59e67b6](https://github.com/poindexter12/baude/commit/59e67b6e5d02201c44a7093a1dc09ab031cece12))
* v2.0.0-beta — checkout-first hierarchy, safe worktree lifecycle, standalone sessions ([0f16f2d](https://github.com/poindexter12/baude/commit/0f16f2d686dbf1a0a7930f4896b060fddf5310e6))


### Bug Fixes

* **05-02:** preserve monotonic counter origins ([dfa0b10](https://github.com/poindexter12/baude/commit/dfa0b10d51a398c984cadf9b30f8845d452926cc))
* **05-03:** block daemon launches after state load failure ([df75e4e](https://github.com/poindexter12/baude/commit/df75e4ed5e958c5b96b7a72f3b014589ab74cc02))
* **05-03:** isolate Claude activity fixtures from active backend ([3101b10](https://github.com/poindexter12/baude/commit/3101b10a4f65b8e0eabd7b23f465306b50b8ee1c))
* **05:** CR-01 enforce daemon restore path consistency ([8a2a5e8](https://github.com/poindexter12/baude/commit/8a2a5e819fda0df0686c5b2c98737ef5ded87492))
* **05:** CR-01 preserve authoritative clipboard line breaks ([8ed0048](https://github.com/poindexter12/baude/commit/8ed0048ca3ca3b5c9f73be60bbf68862f9bee842))
* **05:** CR-01 preserve daemon transaction consistency ([22562cc](https://github.com/poindexter12/baude/commit/22562ccf43244d932076b04a39c92878ba8beb43))
* **05:** CR-01 preserve typed persistence failures ([8a898c2](https://github.com/poindexter12/baude/commit/8a898c2b92cb2e7c11877d10fe93a969a0842992))
* **05:** CR-02 reconcile daemon restarts before spawn ([c33df5c](https://github.com/poindexter12/baude/commit/c33df5c1dd6e425d8abab68c79a4612cf4cb70c2))
* **05:** CR-02 restore all active migrated checkouts ([5447e84](https://github.com/poindexter12/baude/commit/5447e8403c3e57de848d213c81a2b6cc83c84419))
* **05:** CR-03 persist reconciled full branch refs ([6b3af97](https://github.com/poindexter12/baude/commit/6b3af97de826a3d85ec1027e04e506b5bb9f9719))
* **05:** CR-03 reject counter exhaustion before daemon spawn ([8d0cf9c](https://github.com/poindexter12/baude/commit/8d0cf9c487da2b6c2899947ccb716e3e8456e6e4))
* **05:** CR-04 reconcile daemon checkouts before restore ([2084147](https://github.com/poindexter12/baude/commit/208414786f6a86e55682f5e601e57e71e1c3f86f))
* **05:** CR-05 retain duplicate legacy checkout sessions ([db59edb](https://github.com/poindexter12/baude/commit/db59edbb9c0724c1f0651a1d91d199194613a0cd))
* **05:** CR-06 reject ambiguous repository identities ([ea77a36](https://github.com/poindexter12/baude/commit/ea77a3642847feca9a43d5e771823499bace8b0a))
* **05:** CR-07 prevent checkout ownership transfer ([03e841d](https://github.com/poindexter12/baude/commit/03e841d10f2b403facd216bdd1870c4df042866e))
* **05:** CR-08 sync state directory after rename ([6b382a4](https://github.com/poindexter12/baude/commit/6b382a40480bfe4d2a3d50d91c5c2b26bad203d7))
* **05:** CR-09 enforce one state writer per workspace ([7f664b4](https://github.com/poindexter12/baude/commit/7f664b4e2d7e9fa1112dda837382a965be9f2ac8))
* **05:** CR-10 reconcile managed checkout before restart ([9e4efb2](https://github.com/poindexter12/baude/commit/9e4efb27c76ad74a98a2ba49268089d513761c9f))
* **05:** CR-11 reject exhausted durable counters ([c20d1a3](https://github.com/poindexter12/baude/commit/c20d1a3a96194d3626208d17e8f048ba10578cc9))
* **05:** WR-01 expose daemon persistence failures through API ([1fc7e36](https://github.com/poindexter12/baude/commit/1fc7e36668ea2b48efe35ce202c29a4a28c670bc))
* **05:** WR-01 surface persistence failures ([bdae808](https://github.com/poindexter12/baude/commit/bdae80891067999819ae0d8e7893da6a251f4642))
* **05:** WR-01 verify admission recovery from disk ([a1b24e6](https://github.com/poindexter12/baude/commit/a1b24e63b51fdcb7d1e11b11397df74806cb73cc))
* **05:** WR-02 exercise production save-before-spawn seam ([0bc8f40](https://github.com/poindexter12/baude/commit/0bc8f4024cd1d6d4bbdcbcafaf46114177055c73))
* **05:** WR-02 test production admission save-before-spawn ([a54919c](https://github.com/poindexter12/baude/commit/a54919c2e7682b6512a32c35b08f03009823c221))
* **06:** CR-01 await runtime termination before removal ([2e63421](https://github.com/poindexter12/baude/commit/2e63421625ffca83149db21193c71ee838c3385b))
* **06:** CR-01 make multi-PTY teardown retry-safe ([59fa4b6](https://github.com/poindexter12/baude/commit/59fa4b623a52a69af95d5a2e41a90b87f89bd47c))
* **06:** CR-01 preserve truthful partial teardown ownership ([45d7292](https://github.com/poindexter12/baude/commit/45d72926413b75c7dd02e85d0cc6d7d8fba87759))
* **06:** CR-01 reconcile durable activation recovery ([3970b5d](https://github.com/poindexter12/baude/commit/3970b5dcb2f41acdb5616802436cb33f6a82a7c7))
* **06:** CR-01 recover occupied branch activation ([ab85320](https://github.com/poindexter12/baude/commit/ab853203c713129dbadaed0070e019eccdf139ed))
* **06:** CR-02 compensate every post-add activation failure ([1e19878](https://github.com/poindexter12/baude/commit/1e198788568a64b57aa752e603e4ab2a7a7acea6))
* **06:** CR-02 durably record failed add compensation ([1a29c28](https://github.com/poindexter12/baude/commit/1a29c281f9b59feac1cb104d1d659f896c2acf36))
* **06:** CR-02 reconcile teardown before explicit reopen ([4a9216c](https://github.com/poindexter12/baude/commit/4a9216c478fe641ccc4829be822b5aba53663147))
* **06:** CR-02 restore manager agent and shell parity ([1dec466](https://github.com/poindexter12/baude/commit/1dec46697f16190949dc82e92b2153b7bedaf895))
* **06:** CR-02 type post-verification compensation recovery ([bd086e8](https://github.com/poindexter12/baude/commit/bd086e8474a30164b6960b411e3d01aaa83a6db2))
* **06:** CR-02 verify teardown process identity ([bca48b7](https://github.com/poindexter12/baude/commit/bca48b7f210dc65d4318f3ac056ee40de8a1778a))
* **06:** CR-03 durably own pending activations ([8e44ffc](https://github.com/poindexter12/baude/commit/8e44ffce9cd77e4a9b32774527cb1af90e3923bf))
* **06:** CR-03 preserve retained resume IDs before polling ([23377df](https://github.com/poindexter12/baude/commit/23377df0e7337611966b86cf5304c8bf6e832cf4))
* **06:** CR-03 restore shell during close rollback ([6628395](https://github.com/poindexter12/baude/commit/6628395a7475b68302f396aa6f28bf4e300eea2e))
* **06:** CR-03 retain fresh context through removal rollback ([0d15161](https://github.com/poindexter12/baude/commit/0d151615047e234f72db8089f282991d6a88a3a0))
* **06:** CR-03 share durable destructive teardown ([2f42ad8](https://github.com/poindexter12/baude/commit/2f42ad865e2095e55152ad7979c23155c7fb6ae3))
* **06:** CR-04 compensate close persistence by commit stage ([47c9e59](https://github.com/poindexter12/baude/commit/47c9e592af3817d7a327b82251a9b2f192d2550d))
* **06:** CR-04 preserve retained runtime identity ([32aea0e](https://github.com/poindexter12/baude/commit/32aea0ed69125300a23635f62c963261cdf89ec1))
* **06:** CR-04 reopen exited manager activation runtimes ([18a6096](https://github.com/poindexter12/baude/commit/18a6096ae82a0475a077b53b44f828e3544c9c83))
* **06:** CR-04 restore removal authority before runtime ([9addf5c](https://github.com/poindexter12/baude/commit/9addf5cb00a785f40a60ae80de97f8f3565e63ac))
* **06:** CR-04 rollback occupied activation on save failure ([4066e56](https://github.com/poindexter12/baude/commit/4066e56340a38edf51d35f3b6aa91d6c07ef5254))
* **06:** CR-05 persist non-reopenable removal tombstones ([b107d6d](https://github.com/poindexter12/baude/commit/b107d6d9af9cfea271a0d8b74dc299ddf21dbf83))
* **06:** CR-06 align blocked reconciliation with save commit stage ([1e2159b](https://github.com/poindexter12/baude/commit/1e2159ba3635794b73609fa0f54be2cf511609d5))
* **06:** WR-01 assert recovery and process liveness ([16f17dd](https://github.com/poindexter12/baude/commit/16f17dd55f1d9ef7cefa064cd9de2924386b5c7f))
* **06:** WR-01 preserve activation recovery provenance ([881a7ef](https://github.com/poindexter12/baude/commit/881a7ef8f1580d4153cbf4237dc0450d549f8a4b))
* **07:** apply the three UI-audit priority fixes ([ad37088](https://github.com/poindexter12/baude/commit/ad370885cb66f87df30b73c0016126899434453e))
* **07:** exempt baude's own pure seed files from removal preflight ([6014b63](https://github.com/poindexter12/baude/commit/6014b63a0c6103ca65f3671bede41980134545dc))
* **07:** initialize restart selection on checkouts before standalone rows ([da1b7f2](https://github.com/poindexter12/baude/commit/da1b7f222036818f0d17029af6262c91b550a791))
* **07:** parse proc stat fields through a generic helper ([0c24995](https://github.com/poindexter12/baude/commit/0c24995a8e004131658f17bc058b337d2e345eb7))
* **07:** resolve remaining plan blockers ([73f11a5](https://github.com/poindexter12/baude/commit/73f11a5b2b6a367e355916ba651a246764b6664b))
* **07:** revise plans from checker feedback ([e5f34ee](https://github.com/poindexter12/baude/commit/e5f34ee08352a345d93610cfdf50a6c4fbf327e2))
* **test:** shield fake-agent fixtures from the appended permission flag ([76e6a91](https://github.com/poindexter12/baude/commit/76e6a9172ef9631803d97864c7ba150060d35c7a))
* **test:** shield fake-agent fixtures from the appended permission flag ([865325f](https://github.com/poindexter12/baude/commit/865325fbbd56a56d5d881b395b23af05b48057fd))
* **tui:** omit visual wraps from copied text ([fd1f8a6](https://github.com/poindexter12/baude/commit/fd1f8a62eaad33e8084162893dfc382ca406e758))

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
