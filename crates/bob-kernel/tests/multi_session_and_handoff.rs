use bob_kernel::{
    app::LocalApp,
    dispatch::{DispatchPolicy, TransportResult, WireIntent},
    home_result::{HomeExecutionResult, HomeReply, HomeResult},
    outbox::HomeReceipt,
    pairing::{PairingInvitation, ProvenPairingPeer},
    workspace_handoff::WorkspaceStateUpdate,
};
use rusqlite::Connection;
use serde_json::Value;

const PIN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HOME_PIN: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn policy() -> DispatchPolicy {
    DispatchPolicy {
        retry_ms: 10,
        lease_ms: 100,
        max_attempts: 3,
    }
}

#[test]
fn multi_project_and_conversation_catalog_preserves_default_session_and_draft_cas() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("app.sqlite");
    let mut app = LocalApp::open(&db_path).unwrap();
    let default_s = app.default_session().unwrap();
    assert_eq!(app.selected_conversation().unwrap(), default_s);

    // Save message in default session and run a local execution to verify temporary
    // `local-project:*` never leaks into user projects.
    app.accept("req-default", "msg-default", &default_s, "hello default".into(), 1)
        .unwrap();
    let exec = app
        .prepare_local_execution("req-default", "ephemeral/default", 2)
        .unwrap();
    app.record_local_owner(&exec.id, exec.owner_job.as_deref().unwrap(), 3)
        .unwrap();
    app.mark_local_started(&exec.id, 4).unwrap();
    app.finish_local_execution(
        &exec.id,
        include_str!("fixtures/codex-reply-probe.jsonl"),
        Some(0),
        None,
        true,
        5,
    )
    .unwrap();

    let snap0 = app.catalog_snapshot().unwrap();
    assert!(
        snap0.projects.is_empty(),
        "internal local-project:* must not appear in user projects"
    );
    assert_eq!(snap0.standalone_conversations.len(), 1);
    assert_eq!(snap0.standalone_conversations[0].id, default_s);

    // Create a user project and two conversations inside it, plus one standalone conversation.
    let proj = app.create_project("proj-bob", "Bob 1.0.1", 10).unwrap();
    assert_eq!(proj.title, "Bob 1.0.1");

    let conv_a = app
        .create_conversation("conv-a", Some("proj-bob"), "架构设计", 11)
        .unwrap();
    assert_eq!(conv_a.project_id.as_deref(), Some("proj-bob"));
    assert_eq!(app.selected_conversation().unwrap(), "conv-a");

    let conv_b = app
        .create_conversation("conv-b", None, "独立随手记", 12)
        .unwrap();
    assert!(conv_b.project_id.is_none());
    assert_eq!(app.selected_conversation().unwrap(), "conv-b");

    // Save distinct drafts and scroll positions on conv-a and conv-b.
    let vs_a = app
        .save_conversation_view_state(
            "conv-a",
            Some("draft for A"),
            Some(Some("anchor-a")),
            Some(240),
            false,
            13,
        )
        .unwrap();
    assert_eq!(vs_a.draft, "draft for A");
    assert_eq!(vs_a.draft_revision, 1);
    assert_eq!(vs_a.scroll_anchor_message_id.as_deref(), Some("anchor-a"));
    assert_eq!(vs_a.scroll_offset, 240);

    let vs_b = app
        .save_conversation_view_state("conv-b", Some("draft for B"), None, Some(80), false, 14)
        .unwrap();
    assert_eq!(vs_b.draft, "draft for B");
    assert_eq!(vs_b.draft_revision, 1);

    // Send on conv-a with matching draft_revision=1 -> clears conv-a draft.
    app.accept_with_draft_revision(
        "req-a1",
        "msg-a1",
        "conv-a",
        "first message in A".into(),
        Some(1),
        15,
    )
    .unwrap();
    assert_eq!(app.conversation_view_state("conv-a").unwrap().draft, "");

    // Simulate user typing new characters in conv-b (advancing revision to 2)
    // while a send submitted with revision 1 completes -> must NOT clear new draft!
    app.save_conversation_view_state("conv-b", Some("draft for B + new typing"), None, None, false, 16)
        .unwrap();
    app.accept_with_draft_revision(
        "req-b1",
        "msg-b1",
        "conv-b",
        "submitted B".into(),
        Some(1),
        17,
    )
    .unwrap();
    assert_eq!(
        app.conversation_view_state("conv-b").unwrap().draft,
        "draft for B + new typing"
    );

    // Save UI preferences and device offline status, then reopen to verify persistence.
    app.save_ui_preferences(Some("device"), Some("en-US"), Some("dark"), 18)
        .unwrap();
    app.set_device_availability("node-x1t", "offline", "节点 X1T 已休眠离线。", 19)
        .unwrap();

    drop(app);
    let app = LocalApp::open(&db_path).unwrap();
    assert_eq!(app.default_session().unwrap(), default_s);
    assert_eq!(app.selected_conversation().unwrap(), "conv-b");
    let snap = app.catalog_snapshot().unwrap();
    assert_eq!(snap.preferences.sidebar_group_mode, "device");
    assert_eq!(snap.preferences.locale, "en-US");
    assert_eq!(snap.preferences.theme, "dark");
    assert_eq!(snap.projects.len(), 1);
    assert_eq!(snap.projects[0].conversation_count, 1);
    assert_eq!(snap.conversations.len(), 3);
    let offline_group = snap
        .device_groups
        .iter()
        .find(|g| g.device_id == "node-x1t")
        .unwrap();
    assert_eq!(offline_group.availability_state, "offline");
    assert!(!offline_group.immediate_execution_available);
    assert!(offline_group.status_reason.contains("离线"));
}

#[test]
fn in_flight_execution_routes_reply_to_originating_conversation_and_tracks_unread() {
    let dir = tempfile::tempdir().unwrap();
    let replica_db = dir.path().join("replica.sqlite");
    let home_db = dir.path().join("home.sqlite");

    let mut replica = LocalApp::open(&replica_db).unwrap();
    let mut home = LocalApp::open(&home_db).unwrap();
    let home_id = home.device_id().to_owned();
    let replica_id = replica.device_id().to_owned();
    let default_s = replica.default_session().unwrap();

    // QR-pair replica with Home on default_s.
    home.issue_pairing_invitation(
        &PairingInvitation {
            id: "inv-1".into(),
            home_device_id: home_id.clone(),
            home_epoch: 1,
            home_identity_sha256: HOME_PIN.into(),
            expires_at: 10_000,
        },
        &[3; 32],
        1,
    )
    .unwrap();
    home.propose_pairing_peer(
        "inv-1",
        &[3; 32],
        &ProvenPairingPeer {
            device_id: replica_id.clone(),
            identity_sha256: PIN.into(),
            public_key_ref: "replica-key".into(),
        },
        2,
    )
    .unwrap();
    home.confirm_scanned_pairing_peer("inv-1", &replica_id, PIN, &default_s, 50_000, true, 3)
        .unwrap();
    replica.bind_confirmed_home(&home_id, 1, HOME_PIN, 4).unwrap();

    // Replica creates two new conversations (conv-a and conv-b) without re-scanning QR.
    replica
        .create_conversation("conv-a", None, "Task A", 5)
        .unwrap();
    replica
        .create_conversation("conv-b", None, "Task B", 6)
        .unwrap();

    // Select conv-a, send message in conv-a, and switch immediately to conv-b while conv-a is running!
    replica.select_conversation("conv-a", 7).unwrap();
    let accepted_a = replica
        .accept("req-a", "msg-a", "conv-a", "run task A".into(), 8)
        .unwrap();
    replica.select_conversation("conv-b", 9).unwrap();
    assert_eq!(replica.selected_conversation().unwrap(), "conv-b");

    // Delivery & Home admission for conv-a must succeed via QR device session policy.
    let job_a = replica.claim_home_delivery(10, policy()).unwrap().unwrap();
    let WireIntent::Message(wire_req_a) = job_a.intent.clone() else {
        panic!("expected message intent");
    };
    assert_eq!(wire_req_a.session_id, "conv-a");

    let ctx_a = home
        .authenticate_home_peer(&home_id, 1, PIN, "conv-a", 11)
        .unwrap();
    let grant_a = home
        .approve_remote_readonly(&wire_req_a, "ws/a", 20_000, 11)
        .unwrap();
    let (receipt_a, exec_a) = home
        .host_accept_and_prepare(&ctx_a, &wire_req_a, "ws/a", &grant_a, 12)
        .unwrap();
    assert!(receipt_a.accepted);
    let exec_a = exec_a.unwrap();

    replica
        .complete_home_delivery(
            &job_a,
            TransportResult::Receipt(HomeReceipt {
                request_id: "req-a".into(),
                home_device_id: home_id.clone(),
                home_epoch: 1,
                payload_sha256: accepted_a.payload_sha256.clone(),
                accepted: true,
            }),
            13,
            policy(),
        )
        .unwrap();

    // Even though replica is currently viewing conv-b, pending_home_result_at(0) must find req-a!
    let pending = replica.pending_home_result_at(0).unwrap().unwrap();
    assert_eq!(pending.request_id, "req-a");
    assert_eq!(pending.session_id, "conv-a");

    // Apply running status for conv-a and verify remote_executions("conv-a") has it, not conv-b.
    replica
        .apply_authenticated_home_result(
            &wire_req_a,
            &HomeResult {
                receipt: Some(receipt_a.clone()),
                execution: Some(HomeExecutionResult {
                    execution_id: exec_a.id.clone(),
                    version: 1,
                    status: "running".into(),
                    stop_requested: false,
                    stop_confirmed: false,
                    reply: None,
                }),
            },
            14,
        )
        .unwrap();
    assert_eq!(replica.remote_executions("conv-a").unwrap().len(), 1);
    assert_eq!(replica.remote_executions("conv-b").unwrap().len(), 0);

    // Complete execution on Home and apply reply to replica while replica is still on conv-b.
    replica
        .apply_authenticated_home_result(
            &wire_req_a,
            &HomeResult {
                receipt: Some(receipt_a),
                execution: Some(HomeExecutionResult {
                    execution_id: exec_a.id.clone(),
                    version: 2,
                    status: "completed".into(),
                    stop_requested: false,
                    stop_confirmed: false,
                    reply: Some(HomeReply {
                        message_id: "reply-a".into(),
                        text: "Reply for conversation A".into(),
                        created_at: 15,
                    }),
                }),
            },
            16,
        )
        .unwrap();

    // Verify reply landed in conv-a (not conv-b!) and conv-a has unread_count == 1.
    let hist_a = replica.history("conv-a").unwrap();
    let hist_b = replica.history("conv-b").unwrap();
    assert_eq!(hist_a.len(), 2);
    assert_eq!(hist_a[1].content, "Reply for conversation A");
    assert_eq!(hist_b.len(), 0);

    let snap = replica.catalog_snapshot().unwrap();
    let item_a = snap.conversations.iter().find(|c| c.id == "conv-a").unwrap();
    assert_eq!(item_a.unread_count, 1);

    // Selecting conv-a marks it read (unread_count == 0).
    let item_a_selected = replica.select_conversation("conv-a", 17).unwrap();
    assert_eq!(item_a_selected.unread_count, 0);
}

#[test]
fn multi_turn_bounded_context_includes_prior_turns_without_cross_conversation_leakage() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("app.sqlite");
    let mut app = LocalApp::open(&db_path).unwrap();

    app.create_conversation("conv-1", None, "Multi-turn 1", 1)
        .unwrap();
    app.create_conversation("conv-2", None, "Isolated 2", 2)
        .unwrap();

    // Turn 1 in conv-1
    app.accept("req-1", "msg-1", "conv-1", "My project code name isProjectOrion.".into(), 3)
        .unwrap();
    let e1 = app.prepare_local_execution("req-1", "ws/1", 4).unwrap();
    assert_eq!(e1.prompt, "My project code name isProjectOrion.");
    app.record_local_owner(&e1.id, e1.owner_job.as_deref().unwrap(), 5)
        .unwrap();
    app.mark_local_started(&e1.id, 6).unwrap();
    app.finish_local_execution(
        &e1.id,
        include_str!("fixtures/codex-reply-probe.jsonl"),
        Some(0),
        None,
        true,
        7,
    )
    .unwrap();

    // Turn 2 in conv-1: must carry bounded history context from Turn 1!
    app.accept("req-2", "msg-2", "conv-1", "What was my project code name?".into(), 8)
        .unwrap();
    let e2 = app.prepare_local_execution("req-2", "ws/2", 9).unwrap();
    assert!(e2.prompt.contains("bounded_history_context"));
    assert!(e2.prompt.contains("My project code name isProjectOrion."));
    assert!(e2.prompt.contains("BOB_REPLY_OK 2+3=5"));
    assert!(e2.prompt.contains("What was my project code name?"));

    let db = Connection::open(&db_path).unwrap();
    let contract_str: String = db
        .query_row(
            "SELECT contract_json FROM executions WHERE id=?1",
            [&e2.id],
            |r| r.get(0),
        )
        .unwrap();
    let contract: Value = serde_json::from_str(&contract_str).unwrap();
    assert_eq!(contract["continuation_mode"], "bounded_history_context");
    assert_eq!(contract["native_session_resume"], false);
    assert_eq!(contract["context_message_ids"].as_array().unwrap().len(), 2);

    // Finish e2 so runner capacity is free, then run Turn 1 in conv-2: must NOT leak conv-1!
    app.record_local_owner(&e2.id, e2.owner_job.as_deref().unwrap(), 10)
        .unwrap();
    app.mark_local_started(&e2.id, 11).unwrap();
    app.finish_local_execution(
        &e2.id,
        include_str!("fixtures/codex-reply-probe.jsonl"),
        Some(0),
        None,
        true,
        12,
    )
    .unwrap();

    app.accept("req-c2", "msg-c2", "conv-2", "Fresh question in conv 2".into(), 13)
        .unwrap();
    let ec2 = app.prepare_local_execution("req-c2", "ws/c2", 14).unwrap();
    assert_eq!(ec2.prompt, "Fresh question in conv 2");
    assert!(!ec2.prompt.contains("ProjectOrion"));
}

#[test]
fn online_provider_catalog_and_controlled_continuation_enforce_honest_resume_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = LocalApp::open(&dir.path().join("app.sqlite")).unwrap();

    let s_active = app
        .upsert_online_provider_session(
            "node-x1t",
            "X1T Tablet",
            "codex",
            "native-x1t-01",
            "D:/Code/bob-agent-next",
            "X1T Active CLI",
            "Summary of X1T work",
            4,
            true,
            true,
            10,
        )
        .unwrap();
    // Concurrent native resume while write_active=true must be rejected!
    assert!(app
        .bind_online_provider_session(
            &s_active.id,
            "conv-x1t-active",
            "node-x1t",
            "codex",
            "D:/Code/bob-agent-next",
            true,
            11
        )
        .is_err());

    // Bounded history continuation while write_active=true is allowed and explicitly degrades.
    let degraded = app
        .bind_online_provider_session(
            &s_active.id,
            "conv-x1t-degraded",
            "node-x1c",
            "codex",
            "D:/Code/bob-agent-next",
            false,
            12,
        )
        .unwrap();
    assert_eq!(degraded.continuation_mode, "bounded_history_context");
    assert!(!degraded.native_session_resume);

    // Idle resumable session on same node + same harness + same path -> native_resume_verified.
    let s_idle = app
        .upsert_online_provider_session(
            "node-x1t",
            "X1T Tablet",
            "codex",
            "native-x1t-02",
            "D:/Code/bob-agent-next",
            "X1T Idle CLI",
            "Completed phase 1",
            6,
            false,
            true,
            13,
        )
        .unwrap();
    let verified = app
        .bind_online_provider_session(
            &s_idle.id,
            "conv-x1t-verified",
            "node-x1t",
            "codex",
            "D:/Code/bob-agent-next",
            true,
            14,
        )
        .unwrap();
    assert_eq!(verified.continuation_mode, "native_resume_verified");
    assert!(verified.native_session_resume);

    // Cross-node or cross-harness resume request must degrade to bounded_history_context.
    let cross_node = app
        .bind_online_provider_session(
            &s_idle.id,
            "conv-x1c-cross",
            "node-x1c",
            "claude",
            "D:/Code/bob-agent-next",
            true,
            15,
        )
        .unwrap();
    assert_eq!(cross_node.continuation_mode, "bounded_history_context");
    assert!(!cross_node.native_session_resume);
}

#[test]
fn cross_node_workspace_handoff_verifies_and_blocks_all_five_fault_scenarios() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = LocalApp::open(&dir.path().join("app.sqlite")).unwrap();
    app.create_project("proj-1", "Cross-Node Project", 1)
        .unwrap();

    // Base helper for workspace states
    let base_ws = |ws_id: &str, dev_id: &str| WorkspaceStateUpdate {
        workspace_id: ws_id.into(),
        project_id: Some("proj-1".into()),
        device_id: dev_id.into(),
        path_ref: "D:/Code/bob-agent-next".into(),
        availability_state: "online".into(),
        availability_reason: "在线".into(),
        head_commit: "commit-abc123".into(),
        branch: "1.0.1".into(),
        upstream_delta_ahead: 0,
        upstream_delta_behind: 0,
        consistency_state: "synced".into(),
        work_state: "clean".into(),
        dirty_manifest_sha256: "clean-tree-hash".into(),
        missing_assets: vec![],
    };

    // Scenario 1: Source node shut down before push (ahead > 0 & offline)
    let mut s1 = base_ws("ws-x1c", "node-x1c");
    s1.availability_state = "offline".into();
    s1.upstream_delta_ahead = 2;
    s1.consistency_state = "ahead".into();
    app.record_workspace_state(&s1, 2).unwrap();
    app.record_workspace_state(&base_ws("ws-x1t", "node-x1t"), 2)
        .unwrap();

    let h1 = app
        .initiate_workspace_handoff(
            "handoff-1",
            "proj-1",
            None,
            "ws-x1c",
            "ws-x1t",
            "git_commit_pushed",
            "commit-abc123",
            None,
            "clean-tree-hash",
            &[],
            3,
        )
        .unwrap();
    assert_eq!(h1.state, "blocked");
    assert!(h1.blocked_reason.as_deref().unwrap().contains("离线"));

    // Restore source online & pushed for remaining scenarios
    app.record_workspace_state(&base_ws("ws-x1c", "node-x1c"), 4)
        .unwrap();

    // Scenario 2: Same commit, different dirty tree
    let mut t2 = base_ws("ws-x1t", "node-x1t");
    t2.work_state = "dirty_uncommitted".into();
    t2.dirty_manifest_sha256 = "different-dirty-hash".into();
    app.record_workspace_state(&t2, 5).unwrap();
    app.initiate_workspace_handoff(
        "handoff-2",
        "proj-1",
        None,
        "ws-x1c",
        "ws-x1t",
        "patch_bundle_pushed",
        "commit-abc123",
        Some("bundle-sha"),
        "expected-dirty-hash",
        &[],
        6,
    )
    .unwrap();
    let v2 = app
        .verify_and_complete_workspace_handoff("handoff-2", 7)
        .unwrap();
    assert_eq!(v2.state, "blocked");
    assert!(v2.blocked_reason.as_deref().unwrap().contains("脏工作树"));

    // Scenario 3: Missing local assets / Git LFS objects
    let mut t3 = base_ws("ws-x1t", "node-x1t");
    t3.missing_assets = vec!["assets/model.onnx".into()];
    app.record_workspace_state(&t3, 8).unwrap();
    app.initiate_workspace_handoff(
        "handoff-3",
        "proj-1",
        None,
        "ws-x1c",
        "ws-x1t",
        "git_commit_pushed",
        "commit-abc123",
        None,
        "clean-tree-hash",
        &["assets/model.onnx".into()],
        9,
    )
    .unwrap();
    let v3 = app
        .verify_and_complete_workspace_handoff("handoff-3", 10)
        .unwrap();
    assert_eq!(v3.state, "blocked");
    assert!(v3
        .blocked_reason
        .as_deref()
        .unwrap()
        .contains("assets/model.onnx"));

    // Scenario 4: Diverged target branch
    let mut t4 = base_ws("ws-x1t", "node-x1t");
    t4.head_commit = "commit-diverged-999".into();
    t4.upstream_delta_ahead = 1;
    t4.upstream_delta_behind = 2;
    t4.consistency_state = "diverged".into();
    app.record_workspace_state(&t4, 11).unwrap();
    app.initiate_workspace_handoff(
        "handoff-4",
        "proj-1",
        None,
        "ws-x1c",
        "ws-x1t",
        "git_commit_pushed",
        "commit-abc123",
        None,
        "clean-tree-hash",
        &[],
        12,
    )
    .unwrap();
    let v4 = app
        .verify_and_complete_workspace_handoff("handoff-4", 13)
        .unwrap();
    assert_eq!(v4.state, "blocked");
    assert!(v4.blocked_reason.as_deref().unwrap().contains("分叉"));

    // Scenario 5: Interrupted / unverified sync
    let mut t5 = base_ws("ws-x1t", "node-x1t");
    t5.work_state = "sync_failed".into();
    t5.consistency_state = "unverified".into();
    app.record_workspace_state(&t5, 14).unwrap();
    app.initiate_workspace_handoff(
        "handoff-5",
        "proj-1",
        None,
        "ws-x1c",
        "ws-x1t",
        "git_commit_pushed",
        "commit-abc123",
        None,
        "clean-tree-hash",
        &[],
        15,
    )
    .unwrap();
    let v5 = app
        .verify_and_complete_workspace_handoff("handoff-5", 16)
        .unwrap();
    assert_eq!(v5.state, "blocked");
    assert!(v5.blocked_reason.as_deref().unwrap().contains("同步中断"));

    // Clean verified handoff -> completes and advances target fencing token!
    app.record_workspace_state(&base_ws("ws-x1t", "node-x1t"), 17)
        .unwrap();
    app.initiate_workspace_handoff(
        "handoff-ok",
        "proj-1",
        None,
        "ws-x1c",
        "ws-x1t",
        "git_commit_pushed",
        "commit-abc123",
        None,
        "clean-tree-hash",
        &[],
        18,
    )
    .unwrap();
    let vok = app
        .verify_and_complete_workspace_handoff("handoff-ok", 19)
        .unwrap();
    assert_eq!(vok.state, "completed");
    assert!(vok.blocked_reason.is_none());
    assert_eq!(vok.target_fencing_token, Some(1));
}
