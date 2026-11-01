# BP4-06 Verification Evidence — Native `Claude Code` Execution Adapter, `TmuxHarnessRunner` & Governed V1→V2 Deliverable Loop

- **Date**: 2026-11-01
- **Scope**: `BP4-06` (`crates/bob-runtime/src/claude_harness.rs`, `crates/bob-node/src/claude_rpc.rs`, `crates/bob-node/src/lib.rs`, `apps/bob-app/src/kernel-bridge.mjs`, `apps/bob-app/src/components/ConversationDirectory.vue`, `apps/bob-app/src/components/DshSessions.vue`)
- **Testing Constraint**: Verified 100% offline using deterministic fake-claude CLI fixtures (`python` script fixtures) with **zero real Claude CLI invocations or paid model calls**.

---

## 1. Delivered Architecture & Four Core Capabilities

### 1.1 Capability 1: Host Capability Probe & Config Contract (`probe_capabilities`)
- Implemented `ClaudeHarnessGrant` (`claude-harness-grants.json`) and `probe_capabilities` in [`crates/bob-runtime/src/claude_harness.rs`](../../crates/bob-runtime/src/claude_harness.rs).
- Reports observed `available`, `version`, `auth_status`, `supports_output_format_json`, `supports_output_format_stream_json`, `supports_resume`, `tmux_available`, `tmux_version`, `execution_mode` (`"tmux"` when available and enabled, otherwise `"process"`), and `cwd_available` without guessing capabilities from node names.

### 1.2 Capability 2: Native Session Binding (`--resume <session_id>`), `bob-task-card-v1` Handoff, & Serialized Workspace Lock
- Persists native Claude `session_id` per grant in `claude-harness/sessions/<grant_id>.json` (`NativeSessionState`).
- **Direct follow-up routing (`native_resume`)**: When `native_session_id` is present and `last_harness == "claude"`, invokes `claude -p <prompt> --output-format <json|stream-json> --resume <session_id>` directly without forcing a BYOK detour.
- **Cross-tool handoff (`task_card_handoff`)**: When another tool (such as `DSH`) executes in the same workspace (`record_workspace_harness_activity`), the subsequent Claude turn detects `last_harness == "dsh"`, refreshes the `BP4-04` workspace context capsule, and attaches the `bob-task-card-v1` wire payload alongside `--resume <session_id>`.
- **Same-workspace serialized write lock (`acquire_workspace_lock`)**: Prevents concurrent writes to the same `cwd` across Claude and DSH (`HARNESS_WORKSPACE_BUSY`).

### 1.3 Capability 3: Process Keep-Alive (`TmuxHarnessRunner`), Cancel, Timeout, & Restart Reconciliation
- Supports detached `tmux` execution (`tmux new-session -d -s bob-claude-<hash> sh run_in_tmux.sh`) when `tmux` is available on the host, with automatic fallback to supervised child processes.
- Captures structured JSON output atomically to `stdout.json` and `exit_code.txt` in `claude-harness/spool/<grant_id>/<req_hash>/` — **never** scraping terminal panes via `tmux capture-pane` or injecting prompts via `tmux send-keys`.
- Enforces idempotent request deduplication (`CLAUDE_REQUEST_CONFLICT` on payload mismatch), confirmed process/session cleanup on `cancel_execution` before releasing the workspace lock, timeout termination (`CLAUDE_EXECUTION_TIMED_OUT`), and restart reconciliation (`reconcile_grant` recovers completed spools or marks interrupted processes `unknown` / `CLAUDE_RESTART_INTERRUPTED` without replaying the model).

### 1.4 Capability 4: Unified Sidebar Integration & Governed V1→V2 Deliverable Chain
- Claude sessions merge into the unified conversation directory (`app_dsh_list` / `ConversationDirectory.vue` / `DshSessions.vue`), displaying the device (`X1T`), project workspace (`cwd`), and tool badge (`Claude`) inside the same conversation UI without adding a standalone Claude product screen.
- Each completed turn snapshots declared deliverable files into an immutable version directory (`claude-harness/artifacts/<grant_id>/v<N>/files/`) with per-file SHA-256 and `bundle_sha256`, linking `parent_version_id` (`v1 -> v2`) and keeping `generated`, `verified` (`passed`/`failed`), `awaiting_review`, and `accepted` / `changes_requested` states distinct.

---

## 2. Verification Results

1. **`cargo test --offline -p bob-runtime`** (`30 passed; 0 failed`):
   - `claude_harness::tests::parse_json_and_stream_json_and_enforce_session_identity`: ok
   - `claude_harness::tests::web_deliverable_v1_to_parent_pinned_v2_and_native_resume_preserve_v1_snapshot`: ok (verifies Web deliverable `V1 -> parent-pinned V2`, `--resume`, immutable V1 SHA-256 after V2 workspace modification, and `awaiting_review -> accepted` transition)
   - `claude_harness::tests::workspace_lock_cancel_and_restart_reconciliation_never_replay_model`: ok (verifies `HARNESS_WORKSPACE_BUSY`, `CLAUDE_REQUEST_CONFLICT`, `cancel_execution` lock release, and `CLAUDE_RESTART_INTERRUPTED` non-replay reconciliation)
2. **`cargo test --offline -p bob-node`** (`6 passed; 0 failed`):
   - `claude_rpc::tests::unified_dsh_and_claude_rpc_handle_capsule_handoff_and_v1_v2_deliverables`: ok (verifies unified `app_dsh_list`, `app_dsh_prompt`, `app_claude_prompt`, `app_dsh_page`, `task_card_initial` → `native_resume` → `task_card_handoff` after DSH activity, and Markdown deliverable `V1 -> V2 -> V3`)
3. **Frontend Node Test Suite (`node --test tests/*.test.mjs`)** (`86 passed; 0 failed`):
   - Includes `BP4-06 Claude harness bridge maps capability, session, prompt, cancel, reconcile and review commands cleanly` and all existing directory, mobile recovery, and preview cache tests.

---

## 3. Live On-Host Verification on `X1T` (`2.1.316 (Claude Code)`) & `BP4-07` Security / Borrowing Gates

### 3.1 `X1T` Read-Only Inventory & Live 2-Round `V1 -> V2` Closed-Loop (`x1t_live_claude_v1_v2_evidence.json`)
- **Observed Host Capabilities (`X1T`)**:
  - Binary: `/home/bobbik/.local/bin/claude`
  - Version: `2.1.316 (Claude Code)`
  - Auth: `loggedIn: true`, `authMethod: "claude.ai"`, `subscriptionType: "pro"`
  - `tmux`: not installed on `X1T` (`tmux_available: false`, `execution_mode: "process"`)
  - Non-interactive stdin behavior: `Claude Code 2.1.316` waits 3s if `stdin` is not redirected; adapter explicitly binds `.stdin(Stdio::null())` / `< /dev/null` and Base64 prompt transport over SSH.
- **Live Two-Round Verification (`live_x1t_claude_code_v1_to_v2_two_round_closed_loop`)**:
  - **Round 1 (`req-x1t-live-v1`)**: Created `LIVE_NOTE.md` in isolated `/tmp/bob-bp406-x1t-live`, returned `native_session_id = "eeddd53b-e6a8-434f-ae8a-e20ee1fee104"`, snapshotted `v1` (`sha256: 32e1b284cb963eb3135b11b2f6bce2da19d03950295bb625891d69757f9d4a91`), `verification.status = "passed"`.
  - **Round 2 (`req-x1t-live-v2`, `parentVersionId: "v1"`)**: Executed via `route_kind = "native_resume"` (`--resume eeddd53b-e6a8-434f-ae8a-e20ee1fee104`), updated `LIVE_NOTE.md` to `v2` (`sha256: e998c3d462a48e95cd71a2c1671dae331ac5411cac8d185c33756fd1ea539e81`), `parent_version_id = "v1"`, `verification.status = "passed"`, while `v1` preview snapshot retained its exact SHA-256 (`32e1b284cb963eb3135b11b2f6bce2da19d03950295bb625891d69757f9d4a91`).

### 3.2 `BP4-07`: Node Capability Registry, `FileVaultSecretStore` Hardening, Overseas Egress & Render Gates
- Implemented in [`crates/bob-runtime/src/capability_borrowing.rs`](../../crates/bob-runtime/src/capability_borrowing.rs) and [`crates/bob-runtime/src/lib.rs`](../../crates/bob-runtime/src/lib.rs):
  1. **`FileVaultSecretStore` Audit & Hardening**: Replaced predictable timestamp+PID seed generation with multi-source OS CSPRNG entropy (`v2:` seed + per-slot random nonce + SHA-256 MAC tag), preserved 100% readback compatibility for legacy v1 slots, deterministic `PROVIDER_KEY_CORRUPT` tamper detection, and atomic `rotate_master_seed()` with `.bak` recovery.
  2. **Node Capability Registry (`negotiate_capability`)**: Tracks `supported`, `configured`, `authorized`, `reachable`, `busy`, and `observed_at` from `config_and_probe`, failing closed on version/state mismatch.
  3. **Authorized Overseas Model API Egress (`execute_overseas_egress`)**: Enforces provider allowlist, caller token SHA-256 verification, request size/concurrency/budget caps, idempotent request deduplication (no double-billing), secret redaction (`[REDACTED_SECRET]`), and strict rejection of arbitrary URL proxy attempts (`EGRESS_ARBITRARY_URL_FORBIDDEN`).
  4. **Controlled GPU/Display Render Gate (`execute_controlled_render`)**: Binds `source_commit`, `dirty_manifest`, `input_hash`, `render_config`, and `verifier_version`; reports `godot` / `blender` / `browser` capabilities separately; verifies output size & SHA-256; and serves partial-transfer retries from immutable snapshots without re-executing the renderer.
