# Phase 18 deferred items

Out-of-scope discoveries logged by executors. Nothing here was fixed under the phase's plans.

## scripts/gate.sh --linux fails on this macOS host from a Docker Desktop bind-mount race (found by 18-07, first seen by 18-08)

- **Symptom:** `scripts/gate.sh --linux` exits 1 in its `isolated cargo test` step with `error[E0463]: can't find crate for <dep>` while compiling a dependency crate, after fmt, clippy, both commit-message checks and the real-roots self-test passed in the container. The named crates differ per run (`http`, `sha1`, `thiserror` on one run; `nix`, `serial`, `filedescriptor` on another; `hmac` on a third; the workspace's own `baude_core` and `ratatui` on the retry) and every one of them had already compiled earlier in the same build, so the rlib and rmeta exist on disk when rustc reports them missing.
- **Not the cause:** stale cargo state. 18-07 cleared `target/linux-gate` before its first run and the step failed the same way; a verbatim retry on the resulting cache failed again.
- **Isolation runs in the same `baude-gate` image as uid 1000, each in a fresh target dir:**
  - `cargo test --no-run` alone on the bind mount: pass.
  - `cargo clippy --all-targets` then `cargo test --no-run` on the bind mount: fail, `E0463`.
  - same sequence on the bind mount with `CARGO_BUILD_PIPELINED=false`: fail, `E0463`, so pipelining is not the factor.
  - same sequence with `CARGO_TARGET_DIR` on container-local disk: pass.
  - the unmodified `scripts/gate.sh` run inside the container exactly as the `--linux` branch runs it, with only `CARGO_TARGET_DIR=/tmp/linux-gate/target` changed: exit 0, every suite green, on `f1351d2`.
- **Reading:** the check-mode artifacts clippy writes and the build-mode artifacts the test step writes carry different hashes, so nothing is overwritten; rustc simply cannot see a file cargo just wrote in a `deps/` directory that clippy has already doubled in size. That is a file-visibility race in Docker Desktop's bind mount (virtiofs), and the `--linux` lane is the only place the project compiles through one: CI's `check (ubuntu)` job builds on the runner's own disk.
- **Suggested fix, outside 18-07's file allowlist:** keep the container's `CARGO_TARGET_DIR` off the bind mount, for example a named Docker volume mounted at `/work/target/linux-gate/target` or a container-local path, while leaving `CARGO_HOME` on the mount so the registry cache survives between runs. One line in the `docker run` invocation in `scripts/gate.sh`. Until it lands, the `--linux` lane on a macOS host needs the manual variant recorded in 18-07-SUMMARY.md.
  status: acknowledged
