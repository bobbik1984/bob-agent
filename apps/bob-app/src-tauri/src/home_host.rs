//! Native-only authenticated Home listener. Pairing and execution approval are
//! separate: this host requires an explicit, bounded local read-only policy.
use super::{now, remove_receipt, runner, save_receipt, with_app, CompletionReceipt};
use bob_kernel::{
    app::LocalApp, home_auth::HomeMessageContext, home_execution::ReadOnlyExecutionGrant,
    home_result::HomeResult, local_execution::LocalExecution, outbox::AppendMessage,
};
use bob_transport::{control::HomeRequest, Limits, PinnedServer, TransportError};
use std::{
    collections::{HashMap, HashSet},
    net::{SocketAddr, TcpListener},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
type App = Arc<Mutex<Option<LocalApp>>>;
type Active = Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>;

/// Construct only after explicit local execution approval; never deserialize a
/// policy from an incoming request. Permission is exact-device/exact-session.
pub(crate) struct ReadOnlyPolicy {
    pub device_id: String,
    pub session_id: String,
    pub expires_at: i64,
    pub execution_timeout: Duration,
}
pub(crate) struct HomeHostConfig {
    pub address: SocketAddr,
    pub home_device_id: String,
    pub home_epoch: i64,
    pub server: Arc<PinnedServer>,
    pub program: PathBuf,
    pub receipts: PathBuf,
    pub policy: ReadOnlyPolicy,
}
pub(crate) struct HomeHostHandle {
    pub address: SocketAddr,
    stop: Arc<AtomicBool>,
    listener: Option<JoinHandle<()>>,
    owner: App,
    active: Active,
    own: Arc<Mutex<HashSet<String>>>,
    pub last_error: Arc<Mutex<Option<&'static str>>>,
    policy_expires_at: i64,
}
impl HomeHostHandle {
    pub fn is_running(&self) -> bool {
        !self.stop.load(Ordering::SeqCst)
            && self.listener.as_ref().is_some_and(|h| !h.is_finished())
    }
    pub fn execution_enabled(&self) -> bool {
        self.is_running() && now().is_ok_and(|n| n < self.policy_expires_at)
    }

    /// Stop accepting and request cancellation of this host's jobs only. The
    /// worker persists the eventual stop/cleanup result using the normal spool.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let ids = self
            .own
            .lock()
            .map(|ids| ids.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        {
            for id in ids.iter() {
                if let Ok(active) = self.active.lock() {
                    let committed = with_app(&self.owner, |app| {
                        app.request_local_stop(id, now()?)
                            .map_err(|_| "STOP_NOT_CONFIRMED")
                    })
                    .is_ok();
                    if committed {
                        if let Some(flag) = active.get(id) {
                            flag.store(true, Ordering::SeqCst);
                        }
                    }
                }
            }
        }
        if let Some(listener) = self.listener.take() {
            let _ = listener.join();
        }
    }
}
impl Drop for HomeHostHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(crate) fn start(
    config: HomeHostConfig,
    owner: App,
    active: Active,
) -> Result<HomeHostHandle, &'static str> {
    // Windows Job ownership is the current real runner boundary. Do not expose
    // a misleading remote executor on other targets.
    if !cfg!(windows) {
        return Err("HOME_RUNNER_UNSUPPORTED");
    }
    if config.home_device_id.is_empty()
        || config.home_epoch <= 0
        || config.policy.device_id.is_empty()
        || config.policy.session_id.is_empty()
        || config.policy.expires_at <= now()?
        || config.policy.execution_timeout.is_zero()
        || config.policy.execution_timeout > Duration::from_secs(120)
        || !config.program.is_absolute()
        || !config.program.is_file()
        || !config.receipts.is_absolute()
    {
        return Err("HOME_HOST_CONFIGURATION_INVALID");
    }
    with_app(&owner, |app| {
        if app.device_id() == config.home_device_id {
            Ok(())
        } else {
            Err("HOME_IDENTITY_MISMATCH")
        }
    })?;
    std::fs::create_dir_all(&config.receipts).map_err(|_| "RECEIPT_STORE_UNAVAILABLE")?;
    let listener = TcpListener::bind(config.address).map_err(|_| "HOME_BIND_FAILED")?;
    listener
        .set_nonblocking(true)
        .map_err(|_| "HOME_BIND_FAILED")?;
    let address = listener.local_addr().map_err(|_| "HOME_BIND_FAILED")?;
    let policy_expires_at = config.policy.expires_at;
    let stop = Arc::new(AtomicBool::new(false));
    let own = Arc::new(Mutex::new(HashSet::new()));
    let last_error = Arc::new(Mutex::new(None));
    let stop_loop = stop.clone();
    let app = owner.clone();
    let jobs = active.clone();
    let owned = own.clone();
    let error = last_error.clone();
    // Serial bounded RPCs avoid an unbounded peer-created thread pool. TLS and
    // frame I/O have a deadline; the DB is locked only inside the callback.
    let handle = thread::Builder::new()
        .name("bob-home-listener".into())
        .spawn(move || {
            while !stop_loop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((socket, _)) => {
                        // Accepted sockets inherit nonblocking mode on Windows;
                        // TLS itself enforces bounded read/write deadlines.
                        if socket.set_nonblocking(false).is_err() {
                            if let Ok(mut e) = error.lock() {
                                *e = Some("HOME_SOCKET_CONFIGURATION_FAILED");
                            }
                            continue;
                        }
                        let result = config.server.serve_control_rpc_once(
                            socket,
                            Limits::default(),
                            |peer, request| {
                                handle_request(
                                    &config,
                                    &app,
                                    &jobs,
                                    &owned,
                                    &stop_loop,
                                    peer.certificate_sha256(),
                                    request,
                                )
                                .map_err(|code| {
                                    if let Ok(mut e) = error.lock() {
                                        *e = Some(code);
                                    }
                                    TransportError::Protocol
                                })
                            },
                        );
                        if result.is_err() {
                            if let Ok(mut e) = error.lock() {
                                if e.is_none() {
                                    *e = Some("HOME_REQUEST_REJECTED");
                                }
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(25))
                    }
                    Err(_) => {
                        if let Ok(mut e) = error.lock() {
                            *e = Some("HOME_LISTENER_FAILED");
                        }
                        break;
                    }
                }
            }
        })
        .map_err(|_| "HOME_LISTENER_FAILED")?;
    Ok(HomeHostHandle {
        address,
        stop,
        listener: Some(handle),
        owner,
        active,
        own,
        last_error,
        policy_expires_at,
    })
}

fn handle_request(
    config: &HomeHostConfig,
    owner: &App,
    active: &Active,
    own: &Arc<Mutex<HashSet<String>>>,
    stop: &Arc<AtomicBool>,
    pin: &str,
    request: HomeRequest,
) -> Result<HomeResult, &'static str> {
    let binding = request.binding();
    // Same lock order as local app_execute. No socket operation under either lock.
    let mut jobs = active.lock().map_err(|_| "RUNNER_UNAVAILABLE")?;
    let mut launch: Option<(
        HomeMessageContext,
        AppendMessage,
        ReadOnlyExecutionGrant,
        LocalExecution,
        PathBuf,
    )> = None;
    let response = with_app(owner, |app| {
        let at = now()?;
        let ctx = app
            .authenticate_home_peer(
                &config.home_device_id,
                config.home_epoch,
                pin,
                &binding.session_id,
                at,
            )
            .map_err(|_| "HOME_AUTH_REJECTED")?;
        match request {
            HomeRequest::Result(ref r) => app
                .host_lookup_result(&ctx, r, at)
                .map_err(|_| "HOME_RESULT_FAILED"),
            HomeRequest::Lookup(ref r) => app
                .host_lookup_message(&ctx, r, at)
                .map(|r| HomeResult {
                    receipt: r,
                    execution: None,
                })
                .map_err(|_| "HOME_LOOKUP_FAILED"),
            HomeRequest::LookupCancel(ref r) => app
                .host_lookup_cancel(&ctx, r, at)
                .map(|r| HomeResult {
                    receipt: r,
                    execution: None,
                })
                .map_err(|_| "HOME_LOOKUP_FAILED"),
            HomeRequest::Cancel(ref r) => {
                let receipt = app
                    .host_receive_cancel(&ctx, r, at)
                    .map_err(|_| "HOME_CANCEL_REJECTED")?;
                if receipt.accepted {
                    if let Some(flag) = jobs.get(&r.execution_id) {
                        flag.store(true, Ordering::SeqCst);
                    }
                }
                Ok(HomeResult {
                    receipt: Some(receipt),
                    execution: None,
                })
            }
            HomeRequest::Append(ref r) => {
                if let Some(receipt) = app
                    .host_lookup_message(&ctx, r, at)
                    .map_err(|_| "HOME_LOOKUP_FAILED")?
                {
                    // A duplicate or uncertain retry NEVER creates an execution,
                    // including accepted-but-unprepared crash recovery.
                    return Ok(HomeResult {
                        receipt: Some(receipt),
                        execution: None,
                    });
                }
                if stop.load(Ordering::SeqCst) || !jobs.is_empty() {
                    return app
                        .host_reject_message(&ctx, r, "HOME_RUNNER_BUSY", at)
                        .map(|receipt| HomeResult {
                            receipt: Some(receipt),
                            execution: None,
                        })
                        .map_err(|_| "HOME_REJECTION_SAVE_FAILED");
                }
                if config.policy.expires_at <= at
                    || config.policy.device_id != r.origin_device_id
                    || config.policy.session_id != r.session_id
                {
                    return app
                        .host_reject_message(&ctx, r, "HOME_EXECUTION_NOT_APPROVED", at)
                        .map(|receipt| HomeResult {
                            receipt: Some(receipt),
                            execution: None,
                        })
                        .map_err(|_| "HOME_REJECTION_SAVE_FAILED");
                }
                if app
                    .managed_executions()
                    .map_err(|_| "EXECUTION_READ_FAILED")?
                    .iter()
                    .any(|e| {
                        matches!(
                            e.status.as_str(),
                            "pending" | "dispatching" | "running" | "paused" | "unknown"
                        )
                    })
                {
                    return app
                        .host_reject_message(&ctx, r, "HOME_RUNNER_BUSY", at)
                        .map(|receipt| HomeResult {
                            receipt: Some(receipt),
                            execution: None,
                        })
                        .map_err(|_| "HOME_REJECTION_SAVE_FAILED");
                }
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| "CLOCK_UNAVAILABLE")?
                    .as_nanos();
                let directory = std::env::temp_dir()
                    .join(format!("bob-home-readonly-{}-{stamp}", std::process::id()));
                let workspace = directory.to_string_lossy().into_owned();
                let grant = app
                    .approve_remote_readonly(
                        r,
                        &workspace,
                        config.policy.expires_at.min(at.saturating_add(600_000)),
                        at,
                    )
                    .map_err(|_| "HOME_EXECUTION_NOT_APPROVED")?;
                std::fs::create_dir(&directory).map_err(|_| "WORKSPACE_UNAVAILABLE")?;
                let (receipt, execution) =
                    match app.host_accept_and_prepare(&ctx, r, &workspace, &grant, at) {
                        Ok(v) => v,
                        Err(_) => {
                            let _ = std::fs::remove_dir(&directory);
                            return Err("HOME_EXECUTION_PREPARE_FAILED");
                        }
                    };
                if let Some(execution) = execution {
                    if execution.can_start {
                        launch = Some((ctx, r.clone(), grant, execution, directory));
                    }
                } else {
                    let _ = std::fs::remove_dir(&directory);
                }
                Ok(HomeResult {
                    receipt: Some(receipt),
                    execution: None,
                })
            }
        }
    })?;
    if let Some((ctx, request, grant, execution, directory)) = launch {
        let cancel = Arc::new(AtomicBool::new(false));
        jobs.insert(execution.id.clone(), cancel.clone());
        own.lock()
            .map_err(|_| "RUNNER_UNAVAILABLE")?
            .insert(execution.id.clone());
        let task = execution.clone();
        let app = owner.clone();
        let runners = active.clone();
        let owned = own.clone();
        let program = config.program.clone();
        let receipts = config.receipts.clone();
        let timeout = config.policy.execution_timeout;
        let stopping = stop.clone();
        let unstarted_directory = directory.clone();
        let spawned = thread::Builder::new()
            .name("bob-home-readonly".into())
            .spawn(move || {
                let job = runner::job_name(&task.id);
                let result = runner::execute(
                    &program,
                    &directory,
                    task.prompt.clone(),
                    cancel,
                    timeout,
                    &job,
                    || {
                        if stopping.load(Ordering::SeqCst) {
                            return false;
                        }
                        with_app(&app, |app| {
                            app.recheck_remote_readonly(&ctx, &request, &grant, now()?)
                                .map_err(|_| "HOME_PERMISSION_CHANGED")?;
                            app.record_local_owner(&task.id, &job, now()?)
                                .map_err(|_| "OWNER_SAVE_FAILED")?;
                            app.mark_local_started(&task.id, now()?)
                                .map_err(|_| "START_NOT_CONFIRMED")
                        })
                        .is_ok()
                    },
                );
                let receipt = CompletionReceipt {
                    execution_id: task.id.clone(),
                    stream: result.stream.clone(),
                    exit: result.exit,
                    reason: result.reason.map(str::to_owned),
                    cleanup: result.cleanup,
                };
                let _ = save_receipt(&receipts, &receipt);
                let mut committed = false;
                for _ in 0..3 {
                    committed = with_app(&app, |app| {
                        app.finish_local_execution(
                            &task.id,
                            &result.stream,
                            result.exit,
                            result.reason,
                            result.cleanup,
                            now()?,
                        )
                        .map_err(|_| "RESULT_SAVE_UNCONFIRMED")
                    })
                    .is_ok();
                    if committed {
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                if committed {
                    let _ = remove_receipt(&receipts, &task.id);
                }
                if result.cleanup && committed {
                    if let Ok(mut jobs) = runners.lock() {
                        jobs.remove(&task.id);
                    }
                    if let Ok(mut ids) = owned.lock() {
                        ids.remove(&task.id);
                    }
                }
                if result.cleanup {
                    let _ = std::fs::remove_dir(&directory);
                }
            });
        if spawned.is_err() {
            let _ = std::fs::remove_dir(&unstarted_directory);
            let committed = with_app(owner, |app| {
                app.finish_local_execution(
                    &execution.id,
                    "",
                    None,
                    Some("spawn_failed"),
                    true,
                    now()?,
                )
                .map_err(|_| "RESULT_SAVE_UNCONFIRMED")
            })
            .is_ok();
            if committed {
                jobs.remove(&execution.id);
                if let Ok(mut ids) = own.lock() {
                    ids.remove(&execution.id);
                }
            }
            return Err("HOME_WORKER_START_FAILED");
        }
    }
    Ok(response)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use bob_kernel::{
        outbox::Store,
        pairing::{PairingInvitation, ProvenPairingPeer},
    };
    use bob_transport::{certificate_fingerprint, PinnedClient, ServerName, TlsIdentity};
    use rustls::pki_types::PrivatePkcs8KeyDer;
    struct Fixture {
        root: PathBuf,
        owner: App,
        active: Active,
        config: HomeHostConfig,
        client: PinnedClient,
        request: AppendMessage,
        pin: String,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Ok(mut app) = self.owner.lock() {
                app.take();
            }
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    fn fixture() -> Fixture {
        let n = now().unwrap();
        let root = std::env::temp_dir().join(format!(
            "bob-host-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let h = rcgen::generate_simple_self_signed(vec!["bob-host.test".into()]).unwrap();
        let p = rcgen::generate_simple_self_signed(vec!["bob-phone.test".into()]).unwrap();
        let pin = certificate_fingerprint(p.cert.der());
        let homepin = certificate_fingerprint(h.cert.der());
        let server = PinnedServer::new(
            TlsIdentity {
                certificates: vec![h.cert.der().clone()],
                private_key: PrivatePkcs8KeyDer::from(h.key_pair.serialize_der()).into(),
            },
            vec![p.cert.der().clone()],
            pin.clone(),
        )
        .unwrap();
        let client = PinnedClient::new(
            TlsIdentity {
                certificates: vec![p.cert.der().clone()],
                private_key: PrivatePkcs8KeyDer::from(p.key_pair.serialize_der()).into(),
            },
            vec![h.cert.der().clone()],
            homepin.clone(),
        )
        .unwrap();
        let db = root.join("kernel.sqlite");
        let app = LocalApp::open(&db).unwrap();
        let home = app.device_id().to_owned();
        drop(app);
        let mut store = Store::open(&db, n).unwrap();
        store
            .issue_pairing_invitation(
                &PairingInvitation {
                    id: "host-test".into(),
                    home_device_id: home.clone(),
                    home_epoch: 1,
                    home_identity_sha256: homepin,
                    expires_at: n + 100_000,
                },
                &[3; 32],
                n,
            )
            .unwrap();
        store
            .propose_pairing_peer(
                "host-test",
                &[3; 32],
                &ProvenPairingPeer {
                    device_id: "phone".into(),
                    identity_sha256: pin.clone(),
                    public_key_ref: "test-key".into(),
                },
                n,
            )
            .unwrap();
        store
            .confirm_pairing_peer(
                "host-test",
                "phone",
                &pin,
                "phone-session",
                n + 600_000,
                true,
                n,
            )
            .unwrap();
        drop(store);
        let owner = Arc::new(Mutex::new(Some(LocalApp::open(&db).unwrap())));
        let config = HomeHostConfig {
            address: SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 0)),
            home_device_id: home.clone(),
            home_epoch: 1,
            server: Arc::new(server),
            program: std::env::current_exe().unwrap(),
            receipts: root.join("receipts"),
            policy: ReadOnlyPolicy {
                device_id: "phone".into(),
                session_id: "phone-session".into(),
                expires_at: n + 600_000,
                execution_timeout: Duration::from_secs(20),
            },
        };
        Fixture {
            root,
            owner,
            active: Arc::new(Mutex::new(HashMap::new())),
            config,
            client,
            request: AppendMessage {
                request_id: "request".into(),
                message_id: "phone-message".into(),
                session_id: "phone-session".into(),
                origin_device_id: "phone".into(),
                home_device_id: home,
                home_epoch: 1,
                content: serde_json::json!({"text":"fixture-success"}),
            },
            pin,
        }
    }
    #[test]
    fn busy_or_unapproved_never_durably_accepts() {
        let mut f = fixture();
        let own = Arc::new(Mutex::new(HashSet::new()));
        let stop = Arc::new(AtomicBool::new(false));
        f.active
            .lock()
            .unwrap()
            .insert("other-local-run".into(), Arc::new(AtomicBool::new(false)));
        let busy = handle_request(
            &f.config,
            &f.owner,
            &f.active,
            &own,
            &stop,
            &f.pin,
            HomeRequest::Append(f.request.clone()),
        )
        .unwrap();
        assert!(!busy.receipt.unwrap().accepted);
        f.active.lock().unwrap().clear();
        f.config.policy.device_id = "other-phone".into();
        f.request.request_id = "second-request".into();
        f.request.message_id = "second-message".into();
        let unapproved = handle_request(
            &f.config,
            &f.owner,
            &f.active,
            &own,
            &stop,
            &f.pin,
            HomeRequest::Append(f.request.clone()),
        )
        .unwrap();
        assert!(!unapproved.receipt.unwrap().accepted);
        let r = handle_request(
            &f.config,
            &f.owner,
            &f.active,
            &own,
            &stop,
            &f.pin,
            HomeRequest::Result(f.request.clone()),
        )
        .unwrap();
        assert!(!r.receipt.unwrap().accepted);
        assert!(r.execution.is_none());
    }

    #[test]
    fn accepted_without_execution_duplicate_never_replays() {
        let f = fixture();
        with_app(&f.owner, |app| {
            let n = now()?;
            let ctx = app
                .authenticate_home_peer(
                    &f.config.home_device_id,
                    1,
                    &f.pin,
                    &f.request.session_id,
                    n,
                )
                .unwrap();
            app.host_receive_message(&ctx, &f.request, n).unwrap();
            Ok(())
        })
        .unwrap();
        let r = handle_request(
            &f.config,
            &f.owner,
            &f.active,
            &Arc::new(Mutex::new(HashSet::new())),
            &Arc::new(AtomicBool::new(false)),
            &f.pin,
            HomeRequest::Append(f.request.clone()),
        )
        .unwrap();
        assert!(r.receipt.unwrap().accepted);
        assert!(f.active.lock().unwrap().is_empty());
        with_app(&f.owner, |app| {
            assert!(app.managed_executions().unwrap().is_empty());
            Ok(())
        })
        .unwrap();
    }
    #[test]
    #[ignore = "requires explicitly built native worker and fake CLI; no model"]
    fn pinned_listener_to_native_runner_reply_and_duplicate() {
        let mut f = fixture();
        f.config.program =
            PathBuf::from(std::env::var_os("BOB_TEST_FIXTURE").expect("configure fake CLI"));
        assert!(std::env::var_os("BOB_TEST_WORKER").is_some());
        let mut host = start(f.config_for_test(), f.owner.clone(), f.active.clone()).unwrap();
        let send = |body| {
            f.client
                .control(
                    host.address,
                    ServerName::try_from("bob-host.test".to_owned()).unwrap(),
                    &body,
                    Limits::default(),
                )
                .unwrap()
                .result
        };
        let accepted = send(HomeRequest::Append(f.request.clone()));
        assert!(accepted.receipt.unwrap().accepted);
        assert!(
            send(HomeRequest::Append(f.request.clone()))
                .receipt
                .unwrap()
                .accepted
        );
        let until = std::time::Instant::now() + Duration::from_secs(25);
        let final_result = loop {
            let r = send(HomeRequest::Result(f.request.clone()));
            if r.execution
                .as_ref()
                .is_some_and(|e| matches!(e.status.as_str(), "completed" | "failed" | "unknown"))
            {
                break r;
            }
            assert!(std::time::Instant::now() < until);
            thread::sleep(Duration::from_millis(50));
        };
        let e = final_result.execution.unwrap();
        assert_eq!(e.status, "completed");
        assert!(e.reply.is_some());
        assert!(
            send(HomeRequest::Append(f.request.clone()))
                .receipt
                .unwrap()
                .accepted
        );
        with_app(&f.owner, |app| {
            assert_eq!(app.managed_executions().unwrap().len(), 1);
            Ok(())
        })
        .unwrap();
        host.stop();
    }
    impl Fixture {
        fn config_for_test(&mut self) -> HomeHostConfig {
            HomeHostConfig {
                address: self.config.address,
                home_device_id: self.config.home_device_id.clone(),
                home_epoch: self.config.home_epoch,
                server: self.config.server.clone(),
                program: self.config.program.clone(),
                receipts: self.config.receipts.clone(),
                policy: ReadOnlyPolicy {
                    device_id: self.config.policy.device_id.clone(),
                    session_id: self.config.policy.session_id.clone(),
                    expires_at: self.config.policy.expires_at,
                    execution_timeout: self.config.policy.execution_timeout,
                },
            }
        }
    }
}
