# Deferred Items — Phase 11

Out-of-scope discoveries logged during execution (not fixed; pre-existing, unrelated to phase tasks).

## From 11-01 execution (2026-09-16)

- **Pre-existing clippy failures under local toolchain:** `cargo clippy --all-targets -- -D warnings` (the CI invocation) fails locally on (a) `clippy::cargo`-group package-metadata lints (`missing package.readme/keywords/categories` for `baude`, `baude-core`) and (b) ~40 errors in the vendored `vendor/vt100` crate, plus assorted style warnings (`derivable_impls`, `default_constructed_unit_structs`, `manual arithmetic check`) elsewhere in the workspace. None originate in `baude/src/keys.rs`, `baude/src/main.rs`, or `baude/src/app.rs` (verified: zero clippy diagnostics attributed to those files). Likely a local clippy newer than CI's; CI is presumed green on its pinned toolchain. Not touched per scope boundary.
