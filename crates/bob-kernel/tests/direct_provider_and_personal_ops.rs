use bob_kernel::{
    app::LocalApp,
    direct_provider::{DirectChatTurn, DirectExecutionOutcome, DirectProviderProfileInput},
    dispatch::DispatchPolicy,
    pairing::{PairingInvitation, ProvenPairingPeer},
};
use serde_json::json;

const PIN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn f1_standalone_direct_byok_chat_two_turn_continuity_and_cancel() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = LocalApp::open(&dir.path().join("bob.db")).unwrap();
    let session = app.default_session().unwrap();

    // Initially local_only before configuring BYOK
    let ready0 = app.readiness().unwrap();
    assert_eq!(ready0.mode, "local_only");
    assert!(!ready0.execution_available);

    // Configure BYOK provider profile (secret slot ref only, no raw key in DB)
    let profile = app
        .upsert_direct_provider_profile(
            &DirectProviderProfileInput {
                id: "default".into(),
                provider_kind: "openai_compatible".into(),
                label: "OpenAI Official".into(),
                base_url: "https://api.openai.com/v1".into(),
                model_id: "gpt-4o-mini".into(),
                api_key_slot: "provider-key-default".into(),
                api_key_configured: true,
                enabled: true,
            },
            100,
        )
        .unwrap();
    assert_eq!(profile.provider_kind, "openai_compatible");
    assert_eq!(profile.model_id, "gpt-4o-mini");
    assert!(profile.enabled);
    assert!(app.direct_provider_ready().unwrap());

    let ready1 = app.readiness().unwrap();
    assert_eq!(ready1.mode, "direct_provider");
    assert!(ready1.execution_available);

    // Record connectivity check
    let checked = app
        .record_direct_provider_check("default", "ok", "HTTP 200 OK", 110)
        .unwrap();
    assert_eq!(checked.last_check_status, "ok");

    // Turn 1: accept user message & prepare direct execution without pairing or workspace
    app.accept("req-1", "msg-1", &session, "帮我整理今天的三件事".into(), 120)
        .unwrap();
    let prep1 = app.prepare_direct_execution("req-1", 125).unwrap();
    assert!(prep1.can_start);
    assert_eq!(prep1.messages.len(), 1);
    assert_eq!(prep1.messages[0].role, "user");
    assert_eq!(prep1.messages[0].content, "帮我整理今天的三件事");

    // Idempotent re-prepare does not allow starting twice
    let prep1_dup = app.prepare_direct_execution("req-1", 126).unwrap();
    assert!(!prep1_dup.can_start);

    // Finish Turn 1
    let exec1 = app
        .finish_direct_execution(
            &prep1.execution_id,
            DirectExecutionOutcome::Completed {
                text: "好的：1. 推进 F1 直连闭环；2. 梳理旧版对照表；3. 验证多端同步。".into(),
                provider_reported_model: Some("gpt-4o-mini-2024-07-18".into()),
                input_tokens: Some(24),
                output_tokens: Some(36),
            },
            130,
        )
        .unwrap();
    assert_eq!(exec1.status, "completed");

    let hist1 = app.history(&session).unwrap();
    assert_eq!(hist1.len(), 2);
    assert_eq!(hist1[0].send_state, "execution_completed");
    assert_eq!(hist1[1].role, "assistant");
    assert_eq!(hist1[1].source, "direct_provider");
    assert_eq!(hist1[1].reply_to.as_deref(), Some("msg-1"));

    // Unpaired device does not enqueue replica_direct_sync_outbox
    assert!(app.pending_replica_direct_syncs().unwrap().is_empty());

    // Turn 2: follow-up in same session includes Turn 1 in bounded_history_context
    app.accept("req-2", "msg-2", &session, "把第一点拆成两个子步骤".into(), 200)
        .unwrap();
    let prep2 = app.prepare_direct_execution("req-2", 205).unwrap();
    assert!(prep2.can_start);
    assert_eq!(prep2.messages.len(), 3);
    assert_eq!(
        prep2.messages,
        vec![
            DirectChatTurn {
                message_id: "msg-1".into(),
                role: "user".into(),
                content: "帮我整理今天的三件事".into(),
            },
            DirectChatTurn {
                message_id: format!("direct-reply:{}", prep1.execution_id),
                role: "assistant".into(),
                content: "好的：1. 推进 F1 直连闭环；2. 梳理旧版对照表；3. 验证多端同步。".into(),
            },
            DirectChatTurn {
                message_id: "msg-2".into(),
                role: "user".into(),
                content: "把第一点拆成两个子步骤".into(),
            },
        ]
    );

    // Test user stop / cancellation on Turn 2
    assert!(app.request_direct_stop(&prep2.execution_id, 210).unwrap());
    let exec2 = app
        .finish_direct_execution(
            &prep2.execution_id,
            DirectExecutionOutcome::Cancelled {
                reason: "cancelled".into(),
            },
            215,
        )
        .unwrap();
    assert_eq!(exec2.status, "cancelled");
    let execs = app.local_executions(&session).unwrap();
    assert_eq!(execs.len(), 2);
    assert_eq!(execs[1].status, "cancelled");
}

#[test]
fn paired_replica_offline_direct_execution_converges_idempotently_to_home() {
    let home_dir = tempfile::tempdir().unwrap();
    let phone_dir = tempfile::tempdir().unwrap();
    let mut home = LocalApp::open(&home_dir.path().join("home.db")).unwrap();
    let mut phone = LocalApp::open(&phone_dir.path().join("phone.db")).unwrap();

    let home_id = home.device_id().to_owned();
    let phone_id = phone.device_id().to_owned();
    let phone_session = phone.default_session().unwrap();

    // Pair phone with home
    home.issue_pairing_invitation(
        &PairingInvitation {
            id: "inv-1".into(),
            home_device_id: home_id.clone(),
            home_epoch: 1,
            home_identity_sha256: PIN.into(),
            expires_at: 50_000,
        },
        &[7u8; 32],
        10,
    )
    .unwrap();
    home.propose_pairing_peer(
        "inv-1",
        &[7u8; 32],
        &ProvenPairingPeer {
            device_id: phone_id.clone(),
            identity_sha256: PIN.into(),
            public_key_ref: "phone-pub".into(),
        },
        20,
    )
    .unwrap();
    home.confirm_pairing_peer(
        "inv-1",
        &phone_id,
        PIN,
        &phone_session,
        60_000,
        true,
        30,
    )
    .unwrap();
    phone.bind_confirmed_home(&home_id, 1, PIN, 35).unwrap();

    // Configure BYOK on phone
    phone
        .upsert_direct_provider_profile(
            &DirectProviderProfileInput {
                id: "default".into(),
                provider_kind: "anthropic_messages".into(),
                label: "Anthropic Direct".into(),
                base_url: "https://api.anthropic.com/v1".into(),
                model_id: "claude-3-5-sonnet-latest".into(),
                api_key_slot: "provider-key-default".into(),
                api_key_configured: true,
                enabled: true,
            },
            40,
        )
        .unwrap();

    phone
        .set_conversation_execution_route(&phone_session, "direct_byok", 45)
        .unwrap();
    assert_eq!(
        phone.conversation_execution_route(&phone_session).unwrap(),
        "direct_byok"
    );

    // Home is offline: phone accepts message and executes directly via BYOK
    phone
        .accept(
            "offline-req-1",
            "offline-msg-1",
            &phone_session,
            "电脑关机了，手机直接用 API 总结一下会议纪要".into(),
            50,
        )
        .unwrap();

    let prep = phone.prepare_direct_execution("offline-req-1", 55).unwrap();
    assert!(prep.can_start);

    let finished_exec = phone
        .finish_direct_execution(
            &prep.execution_id,
            DirectExecutionOutcome::Completed {
                text: "会议纪要已总结：优先补齐 F1 独立助理直连能力。".into(),
                provider_reported_model: Some("claude-3-5-sonnet-20241022".into()),
                input_tokens: Some(40),
                output_tokens: Some(28),
            },
            60,
        )
        .unwrap();
    assert_eq!(finished_exec.status, "completed");

    // Phone now has 1 pending replica_direct_sync_outbox turn for Home convergence
    let pending_syncs = phone.pending_replica_direct_syncs().unwrap();
    assert_eq!(pending_syncs.len(), 1);
    let turn = &pending_syncs[0];
    assert_eq!(turn.request_id, "offline-req-1");
    assert_eq!(turn.user_message_id, "offline-msg-1");
    assert_eq!(
        turn.reply_message_id,
        format!("direct-reply:{}", prep.execution_id)
    );
    assert_eq!(turn.home_device_id, home_id);

    // When Home comes back online, phone syncs the completed turn to Home
    let ctx = home
        .authenticate_home_session(&home_id, 1, PIN, &phone_session, 70)
        .unwrap();
    let receipt1 = home
        .host_receive_replica_direct_turn(&ctx, turn, 75)
        .unwrap();
    assert!(receipt1.accepted);

    // Idempotent duplicate delivery returns identical receipt without duplicating rows
    let receipt2 = home
        .host_receive_replica_direct_turn(&ctx, turn, 76)
        .unwrap();
    assert_eq!(receipt1.request_id, receipt2.request_id);
    assert_eq!(receipt1.payload_sha256, receipt2.payload_sha256);
    assert_eq!(receipt1.accepted, receipt2.accepted);

    // Conflicting payload with same request_id is rejected with IdentityConflict
    let mut tampered = turn.clone();
    tampered.reply_text = "被篡改的回复".into();
    assert!(home
        .host_receive_replica_direct_turn(&ctx, &tampered, 77)
        .is_err());

    // Mark synced on phone
    phone
        .mark_replica_direct_turn_synced(turn, &receipt1, 80)
        .unwrap();
    assert!(phone.pending_replica_direct_syncs().unwrap().is_empty());

    // Verify Home has both the user message and assistant reply with acceptance='home'
    let home_msgs = home.history(&phone_session).unwrap();
    assert_eq!(home_msgs.len(), 2);
    assert_eq!(home_msgs[0].id, "offline-msg-1");
    assert_eq!(home_msgs[0].role, "user");
    assert_eq!(home_msgs[0].send_state, "execution_completed");
    assert_eq!(home_msgs[1].id, turn.reply_message_id);
    assert_eq!(home_msgs[1].role, "assistant");
    assert_eq!(home_msgs[1].source, "replica_direct_provider");
    assert_eq!(
        home_msgs[1].content,
        "会议纪要已总结：优先补齐 F1 独立助理直连能力。"
    );
}

#[test]
fn direct_execution_refuses_already_dispatched_home_intent() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = LocalApp::open(&dir.path().join("bob.db")).unwrap();
    let session = app.default_session().unwrap();
    app.upsert_direct_provider_profile(
        &DirectProviderProfileInput {
            id: "default".into(),
            provider_kind: "openai_compatible".into(),
            label: "OpenAI".into(),
            base_url: "https://api.openai.com/v1".into(),
            model_id: "gpt-4o-mini".into(),
            api_key_slot: "provider-key-default".into(),
            api_key_configured: true,
            enabled: true,
        },
        10,
    )
    .unwrap();

    // Pair with a remote home so we can claim dispatch
    app.bind_confirmed_home("home-pc", 1, PIN, 20).unwrap();
    app.accept("req-disp", "msg-disp", &session, "发给电脑".into(), 30)
        .unwrap();
    let claimed = app
        .claim_home_delivery(
            32,
            DispatchPolicy {
                lease_ms: 5_000,
                retry_ms: 1_000,
                max_attempts: 3,
            },
        )
        .unwrap();
    assert!(claimed.is_some());

    // Once dispatching/dispatched/unknown_send to Home, direct execution must refuse to claim it
    assert!(app.prepare_direct_execution("req-disp", 35).is_err());
}

#[test]
fn f3_personal_assistant_domain_lifecycle_works_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = LocalApp::open(&dir.path().join("bob.db")).unwrap();
    let session = app.default_session().unwrap();
    app.accept("req-cap", "msg-cap", &session, "记住下周三下午三点产品评审".into(), 100)
        .unwrap();

    // 1. Captures
    let cap = app
        .create_capture(
            "cap-1",
            Some("msg-cap"),
            "记住下周三下午三点产品评审",
            110,
        )
        .unwrap();
    assert_eq!(cap.content_ref, "记住下周三下午三点产品评审");
    assert_eq!(app.list_captures().unwrap().len(), 1);

    // 2. Memories (proposed -> active -> superseded -> forgotten)
    let mem1 = app
        .propose_memory(
            "mem-1",
            Some("cap-1"),
            Some("msg-cap"),
            "偏好使用简明明亮主题与 UTF-8 编码",
            None,
        )
        .unwrap();
    assert_eq!(mem1.state, "proposed");
    let mem1_active = app.update_memory_state("mem-1", "active").unwrap();
    assert_eq!(mem1_active.state, "active");

    let mem2 = app
        .propose_memory(
            "mem-2",
            Some("cap-1"),
            Some("msg-cap"),
            "偏好使用明暗自适应主题与 UTF-8 编码",
            Some("mem-1"),
        )
        .unwrap();
    assert_eq!(mem2.supersedes_id.as_deref(), Some("mem-1"));
    app.update_memory_state("mem-2", "active").unwrap();
    assert_eq!(app.list_memories(true).unwrap().len(), 1);
    assert_eq!(app.list_memories(false).unwrap().len(), 2);

    // 3. Notes
    let note = app
        .upsert_note(
            "note-1",
            None,
            Some(&session),
            "Bob 1.0.1 架构要点",
            "统一本地内核 + BYOK 直连 + 多端协同",
            &["architecture".to_owned(), "f1".to_owned()],
            140,
        )
        .unwrap();
    assert_eq!(note.title, "Bob 1.0.1 架构要点");
    assert_eq!(app.list_notes(None).unwrap().len(), 1);

    // 4. Schedule items (both 'event' and 'todo' kinds, plus negative status validation)
    let sched = app
        .create_schedule_item_with_kind(
            "sched-1",
            None,
            None,
            "产品评审会议",
            "event",
            1_762_000_000_000,
            Some(1_762_003_600_000),
            Some(1_761_996_400_000),
            "核对旧版 0.9.30 能力继承闭环",
            150,
        )
        .unwrap();
    assert_eq!(sched.kind, "event");
    assert_eq!(sched.status, "scheduled");
    let todo_item = app
        .create_schedule_item_with_kind(
            "sched-todo-1",
            None,
            None,
            "补齐契约对齐回归测试",
            "todo",
            1_762_005_000_000,
            None,
            None,
            "RV-01 ~ RV-07",
            155,
        )
        .unwrap();
    assert_eq!(todo_item.kind, "todo");
    assert!(app
        .create_schedule_item_with_kind(
            "sched-bad-kind",
            None,
            None,
            "非法分类",
            "invalid_kind",
            1_762_005_000_000,
            None,
            None,
            "",
            156,
        )
        .is_err());
    // Reject non-canonical 'done' status
    assert!(app.update_schedule_status("sched-1", "done", 158).is_err());
    let sched_done = app
        .update_schedule_status("sched-1", "completed", 160)
        .unwrap();
    assert_eq!(sched_done.status, "completed");
    assert_eq!(app.list_schedule_items().unwrap().len(), 2);

    // 5. Decisions
    let proj = app
        .create_project("proj-unify", "Bob Unification", 170)
        .unwrap();
    let dec = app
        .record_decision(
            "dec-1",
            &proj.id,
            None,
            "msg-cap",
            "模型调用位置、文件位置与记录权威三轴解耦",
            "{\"reason\":\"防止把主库权威误做成所有模型调用必须经由 Home\"}",
            "proposed",
            None,
        )
        .unwrap();
    assert_eq!(dec.project_id, proj.id);
    assert_eq!(dec.status, "proposed");
    let dec_accepted = app.update_decision_status("dec-1", "accepted").unwrap();
    assert_eq!(dec_accepted.status, "accepted");
    assert_eq!(dec_accepted.version, 2);
    assert_eq!(app.list_decisions(&proj.id).unwrap().len(), 1);
    assert_eq!(app.list_all_decisions().unwrap().len(), 1);
    assert_eq!(
        json!(dec.body),
        json!("模型调用位置、文件位置与记录权威三轴解耦")
    );

    // RV-06: Export canonical PersonalOpsSnapshot fixture directly from Rust serde_json
    let canonical_snapshot = json!({
        "schedules": app.list_schedule_items().unwrap(),
        "notes": app.list_notes(None).unwrap(),
        "captures": app.list_captures().unwrap(),
        "memories": app.list_memories(false).unwrap(),
        "decisions": app.list_all_decisions().unwrap(),
        "pending_sync_count": app.pending_personal_op_syncs().unwrap().len(),
    });
    let fixture_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("apps")
        .join("bob-app")
        .join("tests")
        .join("fixtures");
    std::fs::create_dir_all(&fixture_dir).unwrap();
    let fixture_path = fixture_dir.join("personal_snapshot.json");
    std::fs::write(
        &fixture_path,
        serde_json::to_string_pretty(&canonical_snapshot).unwrap(),
    )
    .unwrap();
    assert!(fixture_path.exists());
}

#[test]
fn rv13_paired_replica_personal_ops_sync_to_home_idempotently_with_version_checks() {
    let home_dir = tempfile::tempdir().unwrap();
    let phone_dir = tempfile::tempdir().unwrap();
    let mut home = LocalApp::open(&home_dir.path().join("home.db")).unwrap();
    let mut phone = LocalApp::open(&phone_dir.path().join("phone.db")).unwrap();

    let home_id = home.device_id().to_owned();
    let phone_id = phone.device_id().to_owned();
    let phone_session = phone.default_session().unwrap();

    home.issue_pairing_invitation(
        &PairingInvitation {
            id: "inv-psync".into(),
            home_device_id: home_id.clone(),
            home_epoch: 1,
            home_identity_sha256: PIN.into(),
            expires_at: 50_000,
        },
        &[9u8; 32],
        10,
    )
    .unwrap();
    home.propose_pairing_peer(
        "inv-psync",
        &[9u8; 32],
        &ProvenPairingPeer {
            device_id: phone_id.clone(),
            identity_sha256: PIN.into(),
            public_key_ref: "phone-pub".into(),
        },
        20,
    )
    .unwrap();
    home.confirm_pairing_peer(
        "inv-psync",
        &phone_id,
        PIN,
        &phone_session,
        60_000,
        true,
        30,
    )
    .unwrap();
    phone.bind_confirmed_home(&home_id, 1, PIN, 35).unwrap();

    // Phone creates schedule (todo), note, capture, and memory while offline
    let sched = phone
        .create_schedule_item_with_kind(
            "p-sched-1",
            None,
            None,
            "手机端新增待办事项",
            "todo",
            1_762_100_000_000,
            None,
            None,
            "离线创建，待同步回 Home",
            100,
        )
        .unwrap();
    assert_eq!(sched.acceptance, "local");
    assert_eq!(sched.kind, "todo");

    let note = phone
        .upsert_note(
            "p-note-1",
            None,
            None,
            "手机随手记",
            "# 离线笔记\n等待同步到电脑主库",
            &["mobile".to_owned()],
            110,
        )
        .unwrap();
    assert_eq!(note.acceptance, "local");

    let cap = phone
        .create_capture("p-cap-1", None, "移动端速记：下周五发版", 120)
        .unwrap();
    let mem = phone
        .propose_memory(
            "p-mem-1",
            Some(&cap.id),
            None,
            "用户习惯在周五下午进行版本发布",
            None,
        )
        .unwrap();
    assert_eq!(mem.state, "proposed");

    let pending = phone.pending_personal_op_syncs().unwrap();
    assert_eq!(pending.len(), 4);

    let ctx = home
        .authenticate_home_session(&home_id, 1, PIN, &phone_session, 130)
        .unwrap();

    for entry in &pending {
        let receipt = home
            .host_receive_replica_personal_op(&ctx, entry, 140)
            .unwrap();
        assert!(receipt.accepted);
        // Idempotent duplicate delivery succeeds
        let receipt_dup = home
            .host_receive_replica_personal_op(&ctx, entry, 141)
            .unwrap();
        assert!(receipt_dup.accepted);
        phone.ack_replica_personal_op(&entry.sync_id, 145).unwrap();
    }
    assert!(phone.pending_personal_op_syncs().unwrap().is_empty());

    // Verify phone items promoted to 'home' acceptance and Home has all 4 records
    assert_eq!(phone.list_schedule_items().unwrap()[0].acceptance, "home");
    assert_eq!(phone.list_notes(None).unwrap()[0].acceptance, "home");
    assert_eq!(home.list_schedule_items().unwrap().len(), 1);
    assert_eq!(home.list_schedule_items().unwrap()[0].kind, "todo");
    assert_eq!(home.list_notes(None).unwrap().len(), 1);
    assert_eq!(home.list_captures().unwrap().len(), 1);
    assert_eq!(home.list_memories(false).unwrap().len(), 1);

    // Update schedule status on phone (version 1 -> 2, expected_version = 1)
    phone
        .update_schedule_status("p-sched-1", "completed", 200)
        .unwrap();
    let update_entries = phone.pending_personal_op_syncs().unwrap();
    assert_eq!(update_entries.len(), 1);
    assert_eq!(update_entries[0].expected_version, 1);

    // Apply update on Home
    home.host_receive_replica_personal_op(&ctx, &update_entries[0], 210)
        .unwrap();
    assert_eq!(home.list_schedule_items().unwrap()[0].status, "completed");
    assert_eq!(home.list_schedule_items().unwrap()[0].version, 2);

    // Stale expected_version conflict: if Home already moved to version 2, an entry claiming expected_version = 1 with different status is rejected
    phone.ack_replica_personal_op(&update_entries[0].sync_id, 215).unwrap();
    phone
        .update_schedule_status("p-sched-1", "cancelled", 220)
        .unwrap();
    let mut stale = phone.pending_personal_op_syncs().unwrap()[0].clone();
    stale.expected_version = 1; // Stale expected version!
    assert!(home
        .host_receive_replica_personal_op(&ctx, &stale, 225)
        .is_err());
}
