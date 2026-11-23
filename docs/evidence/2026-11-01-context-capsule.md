# BP4-04 context extraction increment

Scope: explicit local workspace sources; no remote calls, models, renderer, production deployment or APK. Existing UI and parallel documentation changes preserved.

Implemented in bob-runtime project_context/workspace_context: deterministic bounded capsule; full selected-source hashes, small source excerpts, Git HEAD/status metadata; symlink/path rejection and UTF-8/size limits; repeat-read detection of moving selected inputs; version-addressed save/read; pre-dispatch current-state comparison. Missing/oversized/changed inputs return errors, leaving prior persisted capsule intact. The API performs no dispatch, model call, ownership transfer or permission grant.

Verification: cargo test --offline -p bob-runtime --lib:27 passed,0 failed/ignored. Includes5 context tests, a real temporary Git repo, save/read equality, same-repo source edits invalidating previous baseline and source deletion preserving old stored version. External Cargo cache used. Formatting and git diff checks run. This is runtime integration testing, not UI, remote-node, process isolation or production acceptance.

Limits: selected-source/status fingerprint does not hash every dirty/untracked file; do not use it as full workspace handoff proof. Caller must choose non-secret sources; filename denial is not a general secret scanner. Canonical/path checks do not provide race-free hostile-filesystem isolation. Source fragments are excerpts, not accepted decisions or symbol skeletons. Auto refresh, registered workspace authorization, RPC/UI, final Task/Execution linkage, safe extraction under adversarial filesystem, and crash/disk-full/concurrent-store hardening remain pending. No automatic fallback or tmux execution added.
