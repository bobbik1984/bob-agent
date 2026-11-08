# Task Handoff: BOB-D-014 Steps 3–4 — Inbox UI, 5-Tab Navigation, Confirmed Write-Back

> Source material (2026-11-08), not an authority. Current status lives in `docs/CURRENT_STAGE.md §3.10`.

You are continuing BOB-D-014 ("One Inbox, Sorted Once a Day") in `bob-agent-next`, branch `1.0.1`.
Steps 1–2 (kernel contract + Bob Core triage engine) are implemented and tested but NOT yet committed.

## 0. Before anything else
1. Read: `AGENTS.md`, `docs/DECISIONS.md` (BOB-D-014, BOB-D-012), `docs/CURRENT_STAGE.md §3.10`
   (incl. the two "进展（2026-11-08）" notes), `docs/DESIGN_PRINCIPLES.md` (whole file, esp. BOB-D-014,
   calendar, chat-message and badge rules), `docs/FEATURES.md §F3 补充验收`.
2. Open the approved visual reference: `references/ui-design/bob-ui-concept.html`
   (phone screens 01–06, desktop screens A/B). It is the target look; tokens there mirror `style.css`.
3. `git status`. Expected uncommitted work (ask the user before committing; suggested split):
   - Commit 1 `[bob] BOB-D-014 Step 1: unified inbox triage contract` —
     `crates/bob-kernel/migrations/0011.sql`, `src/spark_inbox.rs` (triage part), hooks in
     `lib.rs`/`outbox.rs`/`personal_ops.rs`, `tests/spark_inbox.rs`, node RPCs
     `app_inbox_capture|list|triage_set` in `crates/bob-node/src/lib.rs`.
   - Commit 2 `[bob] BOB-D-014 Step 2: daily triage draft engine` — `migrations/0012.sql`,
     draft functions in `spark_inbox.rs`, `crates/bob-runtime/src/inbox_triage.rs`,
     `crates/bob-node/src/inbox_triage.rs`, `app_inbox_triage_draft|close`.
   - `references/ui-design/` and the docs/todo edits can go with Commit 2 or separately.

## 1. What already exists (do not rebuild)
Kernel (`bob_kernel::spark_inbox`):
- `list_inbox(status: Option<"unsorted"|"sorted">) -> Vec<InboxItem>` (capture + triage fields, `triage_version` 0 = never triaged)
- `set_capture_triage(capture_id, Option<&TriageDestination{kind,ref_id,label}>, batch_id, expected_version, now)`
  kinds: `project_todo|calendar|note|merged`; `None` = back to unsorted. Versioned, idempotent retry,
  enqueues Replica→Home sync in the same transaction.
- `save_triage_draft / latest_triage_draft / triage_batch / close_triage_batch(id, "applied"|"dismissed")`

Runtime (`bob_runtime::inbox_triage`): `triage_prompt`, `parse_triage_batch` → `DailyReviewBatch{suggestions, discarded, spark_count}`,
`TriageSuggestion{spark_ids, destination: project_todo|calendar|note|keep, project_id, project_title, title, start_time, start_at_ms, notes, reason, fallback}`,
`planned_triage(&suggestion, created_ref)` → per-spark `TriageDestination` (secondaries → `merged` → primary spark).

Node RPC (`/api/rpc/<cmd>`, owner token): `app_inbox_capture {id, content, mediaId?}`, `app_inbox_list {status?}`,
`app_inbox_triage_set {captureId, destination?{kind,refId,label}, batchId?, expectedVersion}`,
`app_inbox_triage_draft {regenerate?}` (returns `{id,status,model_ref,created_at,batch}`; `status:"empty"` when no sparks),
`app_inbox_triage_close {batchId, status}`.

## 2. Step 3 — Five-tab navigation and the 收集箱 UI (`apps/bob-app`)
Lightweight `node --test` runs are allowed. NO `vite build` / `tauri build` / cargo for src-tauri without explicit approval.

### 3.1 Bridge
- Add `inboxList`, `inboxCapture`, `inboxTriageSet`, `inboxTriageDraft`, `inboxTriageClose` to
  `apps/bob-app/src/kernel-bridge.mjs` (same pattern as `captureCreate` → `call('app_…')`).
- Check how the web build routes `call()` (`http-bridge.mjs` / `online-client.mjs`) so the new commands reach `/api/rpc/`.
- Tauri side (`src-tauri/src/lib.rs`, near `app_capture_create` ~L657 and the handler list ~L1059): add the same five
  commands. Compiling src-tauri needs user approval — write it, then ask.
- Tests: extend `apps/bob-app/tests/kernel-bridge.test.mjs` with argument-shape assertions for each command.

### 3.2 Navigation (`App.vue` bottom nav ~L427–480, `NavigationDrawer.vue`)
- Bottom nav = `对话 | 今天 | 收集箱 | 知识 | 我的`, equal-width (DESIGN_PRINCIPLES: no empty columns).
- `今天` reuses `ScheduleView.vue` (rename labels only; timeline + todos + cron stay).
- `我的` opens what `设置` opens now, plus 票夹. Remove the 票夹 subtab from BOTH ScheduleView and KnowledgeView.
- Remove the inbox-capture card at the top of ScheduleView's todo subtab ("dashboard #inbox closed loop", ~L661).
- Remove KnowledgeView's `闪念摘录` subtab (its data now lives in 收集箱).
- Inbox badge: unsorted count, accent-soft background (NOT red). Desktop (`layout-mode.mjs`): same five items as a left rail.

### 3.3 New `views/InboxView.vue` (+ `composables/useInbox.mjs`)
- Header: title, filter pills `全部 / 待整理 / 稍后 / 已整理`, `立即整理` (calls draft with `regenerate:true`).
- Daily Review card on top when `inboxTriageDraft()` returns a draft: one row per suggestion with original spark text(s),
  destination chip (日程=accent-soft, 待办=success-soft, 笔记/稍后=neutral), reason, `改一下` (choose another destination)
  and tonal `确认`; footer: node status line + ghost `明天再说` (close as dismissed) + the ONLY solid button `全部确认`.
  In Step 3 the confirm buttons may call a stub that only shows "Step 4 pending" — do not fake success.
- Spark list: unsorted first; sorted items keep a clickable destination chip (from `destination_label`/`destination_kind`).
- Mobile = stacked; desktop ≥1100px = list left, review table right (see concept page screen B).

### 3.4 Zero-decision capture
- `QuickNoteOverlay.vue` / `useQuickNote.mjs`: remove the project picker from the save path; save via `inboxCapture`
  (text + optional photo `mediaId`). Title the sheet `灵感速记`. Keep offline draft + photo/recognize flows intact.
- Desktop: global shortcut to open the capture sheet is a follow-up; note it, don't block on it.

### 3.5 Rules for every new line
- Colours only via tokens in `style.css` (`--bg-*`, `--text-*`, `--line`, `--accent-primary`, `--accent-soft`,
  `--color-success|warning|error`). Solid accent only for the one primary action per screen; repeated actions tonal.
- All visible strings in BOTH `src/locales/zh-CN.json` and `src/locales/en-US.json` (not `i18n.mjs`).
- SVG icons only, no emoji. Keep accessible names/focus rings. User messages narrow right card, Bob replies full width.
- Prefer new radius/spacing values as tokens (skin audit: 89 literal radii, 219 literal paddings exist — don't add more).

### 3.6 Verify
- `node --test apps/bob-app/tests/*.test.mjs` (report counts).
- Ask the user before any build or browser run against the real app; when allowed, verify 390px and 1180px widths,
  light/dark, empty inbox, offline capture, draft present/absent.

## 3. Step 4 — Confirmed write-back
### 4.1 Kernel/runtime
One function `apply_triage_suggestion(batch_id, suggestion_index, edited_suggestion?)` that in ONE transaction:
- `calendar` → `create_schedule_item_with_kind(kind "event")`, ref = schedule id
- `note` → `upsert_note` (body = merged spark texts + notes, tag `capture:<id>`), ref = note id
- `project_todo` → insert a row into a NEW `pending_todo_writes` table (project_id, line text, source spark ids,
  status `pending|written|failed`), ref = that row id
- then `set_capture_triage` for every spark from `planned_triage(...)`; `keep` writes nothing.
- When all suggestions of a batch are applied/kept → `close_triage_batch(id, "applied")`.

Idempotency: re-applying the same suggestion returns the same refs (key by batch id + index).

### 4.2 `todo.md` writer (bob-node, PC side)
- Resolve project → workspace path via `project_registry.yaml` (lives in the Gemini workspace, NOT this repo —
  ask the user for its location/format; never hard-code paths or node names).
- Append-only, UTF-8, under a dedicated heading; mark row `written` with timestamp; on missing path → `failed` with reason.
- If the owning PC node is offline: row stays `pending`, UI shows `Waiting for PC`; the PC drains the queue on reconnect.
  Never claim written without the PC's receipt.

### 4.3 `知识` → `展开这个想法`
Button on notes that opens the assistant chat pre-seeded with note + source spark text.

### 4.4 Tests
Kernel tests for apply (each kind, merge, idempotent retry, partial failure rollback), pending-write lifecycle;
runtime tests for the line format. Cargo needs explicit approval per command (cache dirs already in `.cargo/config.toml`).

## 4. Still open (raise, don't decide alone)
- The 4 questions in `todo.md` §五 (suggested defaults written there: 21:00 + roll-up, 100% confirmation, text+photo first, 30-day redirect).
- Daily 21:00 trigger: where the scheduler lives (bob-node background thread vs existing cron subtab).
- Narrow capture-only token for the legacy Dashboard ingestion (today only owner tokens reach `/api/rpc/`).
- Real-model test of `app_inbox_triage_draft`, real old-DB upgrade through migrations 0011/0012, production deploy — each needs user authorization.

## 5. Handoff format when you stop
Goal, baseline commit, file list, checks passed/failed/not run (with counts), remaining gates, rollback steps.
Update `CURRENT_STAGE.md §3.10` with a dated 进展 line; don't mark anything accepted without evidence.
