# Deferred Items

- Pre-existing `.github/workflows/ci.yml` Docker smoke script emits actionlint/shellcheck SC2015 and SC2034 at the unchanged job starting on line 36. The new artifact-readiness job passes the plan's static contract; changing unrelated Docker smoke behavior is outside Plan 07-05 scope.
