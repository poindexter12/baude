# Deferred Items

## 06-03 execution

- The legacy App removal route still calls the fail-closed compatibility `git::is_dirty` boolean after stopping its runtime. Plan 06-06 owns replacement of that orchestration with the new double-preflight `inspect_removal` contract; 06-03 did not edit `baude/src/app.rs`, which was concurrently assigned to 06-04.
  status: acknowledged
