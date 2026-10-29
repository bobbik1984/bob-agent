use bob_kernel::outbox::{AppendMessage, HomeReceipt};
use bob_transport::*;
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose,
};
use rustls::pki_types::PrivatePkcs8KeyDer;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

struct Material {
    ca: CertificateDer<'static>,
    home: CertificateDer<'static>,
    home_key: Vec<u8>,
    replica: CertificateDer<'static>,
    replica_key: Vec<u8>,
    other: CertificateDer<'static>,
}
fn leaf(
    ca: &Certificate,
    key: &KeyPair,
    purpose: ExtendedKeyUsagePurpose,
) -> (CertificateDer<'static>, Vec<u8>) {
    let mut params = CertificateParams::new(vec!["bob-home.test".into()]).unwrap();
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![purpose];
    let own = KeyPair::generate().unwrap();
    let cert = params.signed_by(&own, ca, key).unwrap();
    (cert.der().clone(), own.serialize_der())
}
fn material() -> Material {
    let mut params = CertificateParams::new(vec![]).unwrap();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let key = KeyPair::generate().unwrap();
    let ca = params.self_signed(&key).unwrap();
    let (home, home_key) = leaf(&ca, &key, ExtendedKeyUsagePurpose::ServerAuth);
    let (replica, replica_key) = leaf(&ca, &key, ExtendedKeyUsagePurpose::ClientAuth);
    let (other, _) = leaf(&ca, &key, ExtendedKeyUsagePurpose::ClientAuth);
    Material {
        ca: ca.der().clone(),
        home,
        home_key,
        replica,
        replica_key,
        other,
    }
}
fn identity(cert: &CertificateDer<'static>, key: &[u8]) -> TlsIdentity {
    TlsIdentity {
        certificates: vec![cert.clone()],
        private_key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.to_vec())),
    }
}
fn server(m: &Material, pin: String) -> PinnedServer {
    PinnedServer::new(identity(&m.home, &m.home_key), vec![m.ca.clone()], pin).unwrap()
}
fn client(m: &Material, pin: String) -> PinnedClient {
    PinnedClient::new(
        identity(&m.replica, &m.replica_key),
        vec![m.ca.clone()],
        pin,
    )
    .unwrap()
}
fn request() -> AppendMessage {
    AppendMessage {
        request_id: "request".into(),
        message_id: "message".into(),
        session_id: "conversation".into(),
        origin_device_id: "replica".into(),
        home_device_id: "home".into(),
        home_epoch: 1,
        content: serde_json::json!({"text":"encrypted fixture"}),
    }
}
fn receipt(request: &AppendMessage) -> HomeReceipt {
    HomeReceipt {
        request_id: request.request_id.clone(),
        home_device_id: request.home_device_id.clone(),
        home_epoch: request.home_epoch,
        payload_sha256: format!("{:x}", Sha256::digest(serde_json::to_vec(request).unwrap())),
        accepted: true,
    }
}
fn name() -> ServerName<'static> {
    ServerName::try_from("bob-home.test").unwrap()
}
fn listener() -> TcpListener {
    TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap()
}
fn raw_client(
    m: &Material,
    auth: bool,
    address: std::net::SocketAddr,
) -> StreamOwned<ClientConnection, TcpStream> {
    let mut roots = RootCertStore::empty();
    roots.add(m.ca.clone()).unwrap();
    let builder =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_protocol_versions(&[&rustls::version::TLS13])
            .unwrap()
            .with_root_certificates(roots);
    let mut config = if auth {
        builder
            .with_client_auth_cert(
                vec![m.replica.clone()],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(m.replica_key.clone())),
            )
            .unwrap()
    } else {
        builder.with_no_client_auth()
    };
    config.alpn_protocols = vec![b"bob-home/1".to_vec()];
    let socket = TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    StreamOwned::new(
        ClientConnection::new(Arc::new(config), name()).unwrap(),
        socket,
    )
}

#[test]
fn actual_tls13_mutual_identity_and_correlated_exchange() {
    let m = material();
    let s = server(&m, certificate_fingerprint(&m.replica));
    let c = client(&m, certificate_fingerprint(&m.home));
    let l = listener();
    let address = l.local_addr().unwrap();
    let replica_pin = certificate_fingerprint(&m.replica);
    let worker = std::thread::spawn(move || {
        s.serve_once(l.accept().unwrap().0, Limits::default(), |peer, request| {
            assert_eq!(peer.certificate_sha256(), replica_pin);
            Ok(receipt(&request))
        })
    });
    let result = c
        .send(address, name(), &request(), Limits::default())
        .unwrap();
    assert_eq!(
        result.peer.certificate_sha256(),
        certificate_fingerprint(&m.home)
    );
    assert!(result.receipt.accepted);
    worker.join().unwrap().unwrap();
}
#[test]
fn wrong_home_pin_never_sends_application_intent() {
    let m = material();
    let s = server(&m, certificate_fingerprint(&m.replica));
    let c = client(&m, certificate_fingerprint(&m.other));
    let l = listener();
    let address = l.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        s.serve_once(l.accept().unwrap().0, Limits::default(), |_, _| {
            panic!("must not dispatch")
        })
    });
    let error = c
        .send(address, name(), &request(), Limits::default())
        .err()
        .unwrap();
    assert!(!error.may_have_been_sent);
    assert!(matches!(error.error, TransportError::PeerMismatch));
    assert!(worker.join().unwrap().is_err());
}
#[test]
fn wrong_replica_pin_cannot_reach_home_handler_even_with_valid_ca() {
    let m = material();
    let s = server(&m, certificate_fingerprint(&m.other));
    let c = client(&m, certificate_fingerprint(&m.home));
    let l = listener();
    let address = l.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        s.serve_once(l.accept().unwrap().0, Limits::default(), |_, _| {
            panic!("must not dispatch")
        })
    });
    assert!(c
        .send(address, name(), &request(), Limits::default())
        .is_err());
    assert!(matches!(
        worker.join().unwrap(),
        Err(TransportError::PeerMismatch)
    ));
}
#[test]
fn missing_client_certificate_cannot_reach_home_handler() {
    let m = material();
    let s = server(&m, certificate_fingerprint(&m.replica));
    let l = listener();
    let address = l.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        s.serve_once(l.accept().unwrap().0, Limits::default(), |_, _| {
            panic!("must not dispatch")
        })
    });
    let mut raw = raw_client(&m, false, address);
    let _ = raw.write_all(&[0, 0, 0, 1, 0]);
    let _ = raw.flush();
    drop(raw);
    assert!(worker.join().unwrap().is_err());
}
#[test]
fn untrusted_ca_and_wrong_dns_name_are_rejected() {
    for untrusted in [true, false] {
        let m = material();
        let other = material();
        let s = server(&m, certificate_fingerprint(&m.replica));
        let c = PinnedClient::new(
            identity(&m.replica, &m.replica_key),
            vec![if untrusted { other.ca } else { m.ca.clone() }],
            certificate_fingerprint(&m.home),
        )
        .unwrap();
        let l = listener();
        let address = l.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            s.serve_once(l.accept().unwrap().0, Limits::default(), |_, _| {
                panic!("must not dispatch")
            })
        });
        let requested_name = if untrusted {
            name()
        } else {
            ServerName::try_from("wrong-home.test").unwrap()
        };
        let error = c
            .send(address, requested_name, &request(), Limits::default())
            .err()
            .unwrap();
        assert!(!error.may_have_been_sent);
        assert!(worker.join().unwrap().is_err());
    }
}
#[test]
fn oversized_and_truncated_authenticated_frames_do_not_dispatch() {
    for oversized in [true, false] {
        let m = material();
        let s = server(&m, certificate_fingerprint(&m.replica));
        let l = listener();
        let address = l.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            s.serve_once(l.accept().unwrap().0, Limits::default(), |_, _| {
                panic!("must not dispatch")
            })
        });
        let mut raw = raw_client(&m, true, address);
        raw.write_all(&(if oversized { u32::MAX } else { 100u32 }).to_be_bytes())
            .unwrap();
        raw.write_all(b"{}").unwrap();
        raw.flush().unwrap();
        raw.conn.send_close_notify();
        let _ = raw.flush();
        drop(raw);
        let result = worker.join().unwrap();
        assert!(result.is_err());
        if oversized {
            assert!(matches!(result, Err(TransportError::FrameLimit)));
        }
    }
}
#[test]
fn silent_peer_cannot_extend_absolute_io_deadline() {
    let m = material();
    let s = server(&m, certificate_fingerprint(&m.replica));
    let l = listener();
    let address = l.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let start = Instant::now();
        let result = s.serve_once(
            l.accept().unwrap().0,
            Limits {
                deadline: Duration::from_millis(200),
                ..Limits::default()
            },
            |_, _| panic!("must not dispatch"),
        );
        (result, start.elapsed())
    });
    let raw = TcpStream::connect(address).unwrap();
    let (result, elapsed) = worker.join().unwrap();
    drop(raw);
    assert!(result.is_err());
    assert!(elapsed < Duration::from_secs(2));
}
#[test]
fn oversize_outbound_is_rejected_before_connect_and_mismatched_receipt_is_ambiguous() {
    let m = material();
    let c = client(&m, certificate_fingerprint(&m.home));
    let mut big = request();
    big.content = serde_json::json!({"text":"x".repeat(300*1024)});
    let error = c
        .send(
            (std::net::Ipv4Addr::LOCALHOST, 0).into(),
            name(),
            &big,
            Limits::default(),
        )
        .err()
        .unwrap();
    assert!(!error.may_have_been_sent);
    assert!(matches!(error.error, TransportError::FrameLimit));
    let s = server(&m, certificate_fingerprint(&m.replica));
    let l = listener();
    let address = l.local_addr().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let worker = std::thread::spawn(move || {
        s.serve_once(l.accept().unwrap().0, Limits::default(), |_, r| {
            seen.fetch_add(1, Ordering::SeqCst);
            let mut ack = receipt(&r);
            ack.request_id = "wrong".into();
            Ok(ack)
        })
    });
    let error = c
        .send(address, name(), &request(), Limits::default())
        .err()
        .unwrap();
    assert!(error.may_have_been_sent);
    assert!(matches!(error.error, TransportError::Protocol));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    worker.join().unwrap().unwrap();
}

#[test]
fn tls_observed_identity_authorizes_real_kernel_receipt_and_reopen() {
    use bob_kernel::outbox::Store;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("home.sqlite");
    let m = material();
    let peer_pin = certificate_fingerprint(&m.replica);
    let mut store = Store::open(&path, 1).unwrap();
    store
        .initialize_local_identity("replica", "home", 1, 1)
        .unwrap();
    // Test-only established pairing. Production pairing/vault is not implemented here.
    rusqlite::Connection::open(&path).unwrap().execute("UPDATE devices SET trust_state='paired',public_key_ref='ephemeral-fixture' WHERE id='replica'",[]).unwrap();
    store
        .grant_home_conversation(
            "home",
            1,
            "replica",
            &peer_pin,
            "conversation",
            0,
            100,
            false,
            1,
        )
        .unwrap();
    drop(store);
    let s = server(&m, peer_pin.clone());
    let c = client(&m, certificate_fingerprint(&m.home));
    let l = listener();
    let address = l.local_addr().unwrap();
    let worker_path = path.clone();
    let worker = std::thread::spawn(move || {
        let mut store = Store::open(&worker_path, 2).unwrap();
        // Two actual TLS exchanges use one idempotent request; the durable kernel owns deduplication.
        for _ in 0..2 {
            s.serve_home_once(
                l.accept().unwrap().0,
                Limits::default(),
                &mut store,
                "home",
                1,
                || 2,
            )
            .unwrap();
        }
        // A trusted grant revocation is rechecked for a new authenticated request.
        store
            .revoke_home_device("home", 1, "replica", 1, 3)
            .unwrap();
        assert!(matches!(
            s.serve_home_once(
                l.accept().unwrap().0,
                Limits::default(),
                &mut store,
                "home",
                1,
                || 3
            ),
            Err(TransportError::Authentication)
        ));
    });
    let one = c
        .send(address, name(), &request(), Limits::default())
        .unwrap();
    let two = c
        .send(address, name(), &request(), Limits::default())
        .unwrap();
    assert_eq!(one.receipt.payload_sha256, two.receipt.payload_sha256);
    assert!(c
        .send(address, name(), &request(), Limits::default())
        .is_err());
    worker.join().unwrap();
    let mut reopened = Store::open(&path, 3).unwrap();
    assert!(reopened
        .authenticate_home_session("home", 1, &peer_pin, "conversation", 3)
        .is_err());
    reopened
        .grant_home_conversation(
            "home",
            1,
            "replica",
            &peer_pin,
            "conversation",
            2,
            100,
            false,
            4,
        )
        .unwrap();
    let context = reopened
        .authenticate_home_session("home", 1, &peer_pin, "conversation", 4)
        .unwrap();
    let receipt = reopened
        .lookup_message_receipt(&context, &request(), 4)
        .unwrap()
        .unwrap();
    assert!(receipt.accepted);
    assert_eq!(receipt.payload_sha256, one.receipt.payload_sha256);
    let count: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row("SELECT count(*) FROM messages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn lost_response_lookup_is_read_only_durable_and_rechecks_authorization() {
    use bob_kernel::outbox::Store;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("home.sqlite");
    let m = material();
    let pin = certificate_fingerprint(&m.replica);
    let mut store = Store::open(&path, 1).unwrap();
    store
        .initialize_local_identity("replica", "home", 1, 1)
        .unwrap();
    rusqlite::Connection::open(&path).unwrap().execute("UPDATE devices SET trust_state='paired',public_key_ref='ephemeral-fixture' WHERE id='replica'",[]).unwrap();
    store
        .grant_home_conversation("home", 1, "replica", &pin, "conversation", 0, 100, false, 1)
        .unwrap();
    drop(store);
    let s = server(&m, pin);
    let c = client(&m, certificate_fingerprint(&m.home));
    let l = listener();
    let address = l.local_addr().unwrap();
    let home_path = path.clone();
    let worker = std::thread::spawn(move || {
        let mut home = Store::open(&home_path, 2).unwrap();
        // Client deliberately abandons the first response, simulating receipt loss.
        let _ = s.serve_home_once(
            l.accept().unwrap().0,
            Limits::default(),
            &mut home,
            "home",
            1,
            || 2,
        );
        drop(home);
        let mut home = Store::open(&home_path, 3).unwrap();
        for _ in 0..2 {
            s.serve_home_once(
                l.accept().unwrap().0,
                Limits::default(),
                &mut home,
                "home",
                1,
                || 3,
            )
            .unwrap();
        }
        // Same TLS peer cannot substitute an ungranted claimed origin.
        assert!(s
            .serve_home_once(
                l.accept().unwrap().0,
                Limits::default(),
                &mut home,
                "home",
                1,
                || 3
            )
            .is_err());
        home.revoke_home_device("home", 1, "replica", 1, 4).unwrap();
        assert!(matches!(
            s.serve_home_once(
                l.accept().unwrap().0,
                Limits::default(),
                &mut home,
                "home",
                1,
                || 4
            ),
            Err(TransportError::Authentication)
        ));
    });
    let mut raw = raw_client(&m, true, address);
    let bytes = serde_json::to_vec(
        &serde_json::json!({"version":1,"operation":"append","message":request()}),
    )
    .unwrap();
    raw.write_all(&(bytes.len() as u32).to_be_bytes()).unwrap();
    raw.write_all(&bytes).unwrap();
    raw.flush().unwrap();
    drop(raw);
    let found = c
        .lookup(address, name(), &request(), Limits::default())
        .unwrap();
    assert!(found.receipt.unwrap().accepted);
    let mut absent = request();
    absent.request_id = "never-sent".into();
    absent.message_id = "never-sent-message".into();
    assert!(c
        .lookup(address, name(), &absent, Limits::default())
        .unwrap()
        .receipt
        .is_none());
    let mut spoof = request();
    spoof.origin_device_id = "another-device".into();
    assert!(c
        .lookup(address, name(), &spoof, Limits::default())
        .is_err());
    assert!(c
        .lookup(address, name(), &request(), Limits::default())
        .is_err());
    worker.join().unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    for table in ["messages", "inbox_receipts", "domain_events"] {
        let count: i64 = db
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
#[test]
fn lookup_wrong_home_pin_rejects_before_application_query() {
    let m = material();
    let s = server(&m, certificate_fingerprint(&m.replica));
    let c = client(&m, certificate_fingerprint(&m.other));
    let l = listener();
    let address = l.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        s.serve_once(l.accept().unwrap().0, Limits::default(), |_, _| {
            panic!("must not dispatch")
        })
    });
    let error = c
        .lookup(address, name(), &request(), Limits::default())
        .err()
        .unwrap();
    assert!(!error.may_have_been_sent);
    assert!(matches!(error.error, TransportError::PeerMismatch));
    assert!(worker.join().unwrap().is_err());
}

fn control_store(path: &std::path::Path, pin: &str) -> bob_kernel::outbox::Store {
    let mut store = bob_kernel::outbox::Store::open(path, 1).unwrap();
    store
        .initialize_local_identity("replica", "home", 1, 1)
        .unwrap();
    rusqlite::Connection::open(path).unwrap().execute("UPDATE devices SET trust_state='paired',public_key_ref='control-fixture' WHERE id='replica'",[]).unwrap();
    store
        .grant_home_conversation("home", 1, "replica", pin, "conversation", 0, 100, true, 1)
        .unwrap();
    store
}
fn control_execution(path: &std::path::Path) {
    rusqlite::Connection::open(path).unwrap().execute_batch("PRAGMA foreign_keys=ON;
    INSERT INTO projects(id,title,status,version,created_at,updated_at) VALUES('project','','active',1,1,1);
    INSERT INTO workspaces VALUES('workspace','home','fixture','readonly',1);
    INSERT INTO project_workspaces VALUES('project','workspace');
    INSERT INTO tasks(id,project_id,source_message_id,title,status,constraints_json,version,created_at,updated_at) VALUES('task','project','message','','running','{}',1,1,1);
    INSERT INTO executions(id,project_id,task_id,workspace_id,request_id,status,requested_json,contract_json,contract_sha256,version) VALUES('execution','project','task','workspace','request','running','{}','{}','fixture',1);
    INSERT INTO workspace_leases VALUES('workspace','execution',1,1,'held',1);").unwrap();
}

#[test]
fn control_results_cancel_and_reopen_are_bound_to_original_request() {
    use bob_kernel::dispatch::CancelExecution;
    use bob_transport::control::HomeRequest;
    let m = material();
    let pin = certificate_fingerprint(&m.replica);
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("home.sqlite");
    let mut store = control_store(&path, &pin);
    let ctx = store
        .authenticate_home_session("home", 1, &pin, "conversation", 1)
        .unwrap();
    store.receive_message(&ctx, &request(), 2).unwrap();
    control_execution(&path);
    let s = server(&m, pin);
    let c = client(&m, certificate_fingerprint(&m.home));
    let l = listener();
    let address = l.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        for _ in 0..5 {
            s.serve_control_home_once(
                l.accept().unwrap().0,
                Limits::default(),
                &mut store,
                "home",
                1,
                || 3,
            )
            .unwrap();
        }
    });
    let result = c
        .control(
            address,
            name(),
            &HomeRequest::Result(request()),
            Limits::default(),
        )
        .unwrap();
    let e = result.result.execution.unwrap();
    assert_eq!(e.status, "running");
    assert!(!e.stop_requested);
    assert!(e.reply.is_none());
    let cancel = CancelExecution {
        request_id: "cancel-control".into(),
        session_id: "conversation".into(),
        origin_device_id: "replica".into(),
        home_device_id: "home".into(),
        home_epoch: 1,
        execution_id: e.execution_id,
        expected_version: e.version,
        permission_ref: "fixture-grant".into(),
    };
    assert!(
        c.control(
            address,
            name(),
            &HomeRequest::Cancel(cancel.clone()),
            Limits::default()
        )
        .unwrap()
        .result
        .receipt
        .unwrap()
        .accepted
    );
    assert!(
        c.control(
            address,
            name(),
            &HomeRequest::LookupCancel(cancel.clone()),
            Limits::default()
        )
        .unwrap()
        .result
        .receipt
        .unwrap()
        .accepted
    );
    assert!(
        c.control(
            address,
            name(),
            &HomeRequest::Cancel(cancel),
            Limits::default()
        )
        .unwrap()
        .result
        .receipt
        .unwrap()
        .accepted
    );
    let e = c
        .control(
            address,
            name(),
            &HomeRequest::Result(request()),
            Limits::default(),
        )
        .unwrap()
        .result
        .execution
        .unwrap();
    assert_eq!(e.status, "running");
    assert_eq!(e.version, 2);
    assert!(e.stop_requested);
    assert!(!e.stop_confirmed);
    worker.join().unwrap();
    let mut store = bob_kernel::outbox::Store::open(&path, 4).unwrap();
    let e = store
        .lookup_home_result(&ctx, &request(), 4)
        .unwrap()
        .execution
        .unwrap();
    assert!(e.stop_requested);
    assert!(!e.stop_confirmed);
    let db = rusqlite::Connection::open(&path).unwrap();
    let lease: String = db
        .query_row("SELECT state FROM workspace_leases", [], |r| r.get(0))
        .unwrap();
    assert_eq!(lease, "held");
}

#[test]
fn result_lookup_rejects_changed_request_and_revoked_grant_and_only_returns_matching_reply() {
    let m = material();
    let pin = certificate_fingerprint(&m.replica);
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("home.sqlite");
    let mut store = control_store(&path, &pin);
    let ctx = store
        .authenticate_home_session("home", 1, &pin, "conversation", 1)
        .unwrap();
    assert!(store
        .lookup_home_result(&ctx, &request(), 2)
        .unwrap()
        .receipt
        .is_none());
    store.receive_message(&ctx, &request(), 2).unwrap();
    control_execution(&path);
    let mut changed = request();
    changed.content = serde_json::json!({"text":"different"});
    assert!(store.lookup_home_result(&ctx, &changed, 3).is_err());
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("INSERT INTO messages VALUES('reply','conversation','home',NULL,'assistant','{\"text\":\"actual reply\",\"reply_to\":\"message\",\"execution_id\":\"execution\"}','home',4);
    INSERT INTO messages VALUES('unrelated','conversation','home',NULL,'assistant','{\"text\":\"wrong reply\",\"reply_to\":\"other\",\"execution_id\":\"execution\"}','home',5);").unwrap();
    assert!(store
        .lookup_home_result(&ctx, &request(), 3)
        .unwrap()
        .execution
        .unwrap()
        .reply
        .is_none());
    db.execute(
        "UPDATE executions SET status='completed',version=2 WHERE id='execution'",
        [],
    )
    .unwrap();
    let reply = store
        .lookup_home_result(&ctx, &request(), 4)
        .unwrap()
        .execution
        .unwrap()
        .reply
        .unwrap();
    assert_eq!(reply.text, "actual reply");
    db.execute("UPDATE home_device_authorizations SET revoked=1", [])
        .unwrap();
    assert!(store.lookup_home_result(&ctx, &request(), 5).is_err());
}

#[test]
fn native_delivery_saves_offline_then_reconciles_lost_ack_without_duplicate() {
    use bob_kernel::{
        app::LocalApp,
        dispatch::{DispatchPolicy, JobKind, TransportResult},
    };
    use bob_transport::delivery::HomeDelivery;
    let m = material();
    let d = tempfile::tempdir().unwrap();
    let replica_path = d.path().join("replica.sqlite");
    let home_path = d.path().join("home.sqlite");
    let mut replica = LocalApp::open(&replica_path).unwrap();
    let origin = replica.device_id().to_owned();
    let session = replica.default_session().unwrap();
    replica
        .bind_confirmed_home("home", 1, &certificate_fingerprint(&m.home), 1)
        .unwrap();
    let mut home = bob_kernel::outbox::Store::open(&home_path, 1).unwrap();
    home.initialize_local_identity(&origin, "home", 1, 1)
        .unwrap();
    rusqlite::Connection::open(&home_path).unwrap().execute("UPDATE devices SET trust_state='paired',public_key_ref='ephemeral-fixture' WHERE id=?1",[&origin]).unwrap();
    home.grant_home_conversation(
        "home",
        1,
        &origin,
        &certificate_fingerprint(&m.replica),
        &session,
        0,
        10000,
        true,
        1,
    )
    .unwrap();
    replica
        .accept(
            "delivery-request",
            "delivery-message",
            &session,
            "saved while offline".into(),
            2,
        )
        .unwrap();
    let policy = DispatchPolicy {
        retry_ms: 10,
        lease_ms: 2000,
        max_attempts: 5,
    };
    let c = client(&m, certificate_fingerprint(&m.home));
    let closed = listener();
    let unused = closed.local_addr().unwrap();
    drop(closed);
    let first = replica.claim_home_delivery(3, policy).unwrap().unwrap();
    let driver = HomeDelivery {
        client: &c,
        address: unused,
        server_name: name(),
        limits: Limits::default(),
    };
    let outcome = driver.run_delivery(&first);
    assert!(matches!(outcome, TransportResult::ConfirmedZero));
    replica
        .complete_home_delivery(&first, outcome, 4, policy)
        .unwrap();
    assert_eq!(
        replica.history(&session).unwrap()[0].send_state,
        "not_dispatched"
    );
    let job = replica.claim_home_delivery(20, policy).unwrap().unwrap();
    assert_eq!(job.kind, JobKind::Send);
    let s = server(&m, certificate_fingerprint(&m.replica));
    let l = listener();
    let address = l.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        // Authentic Home receives and commits after the client's response deadline.
        // This models a lost ACK, not a failed model execution.
        let _ = s.serve_control_home_once(
            l.accept().unwrap().0,
            Limits::default(),
            &mut home,
            "home",
            1,
            || {
                std::thread::sleep(Duration::from_millis(400));
                30
            },
        );
        home
    });
    let driver = HomeDelivery {
        client: &c,
        address,
        server_name: name(),
        limits: Limits {
            deadline: Duration::from_millis(200),
            ..Limits::default()
        },
    };
    let outcome = driver.run_delivery(&job);
    assert!(matches!(outcome, TransportResult::Uncertain));
    replica
        .complete_home_delivery(&job, outcome, 31, policy)
        .unwrap();
    let mut home = worker.join().unwrap();
    drop(replica);
    let mut replica = LocalApp::open(&replica_path).unwrap();
    let job = replica
        .claim_home_delivery(i64::MAX - 10000, policy)
        .unwrap()
        .unwrap();
    assert_eq!(job.kind, JobKind::Lookup);
    let s = server(&m, certificate_fingerprint(&m.replica));
    let l = listener();
    let address = l.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        s.serve_control_home_once(
            l.accept().unwrap().0,
            Limits::default(),
            &mut home,
            "home",
            1,
            || 40,
        )
    });
    let driver = HomeDelivery {
        client: &c,
        address,
        server_name: name(),
        limits: Limits::default(),
    };
    let outcome = driver.run_delivery(&job);
    assert!(matches!(outcome, TransportResult::Receipt(_)));
    replica
        .complete_home_delivery(&job, outcome, i64::MAX - 9999, policy)
        .unwrap();
    worker.join().unwrap().unwrap();
    assert_eq!(
        replica.history(&session).unwrap()[0].send_state,
        "peer_accepted"
    );
    let db = rusqlite::Connection::open(&home_path).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM messages", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM inbox_receipts", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

fn pairing_scope(m: &Material) -> bob_transport::pairing::PairingScope {
    bob_transport::pairing::PairingScope {
        invitation_id: "fixture-invitation".into(),
        home_id: "home".into(),
        home_epoch: 1,
        home_pin: certificate_fingerprint(&m.home),
        candidate_id: "replica".into(),
        candidate_pin: certificate_fingerprint(&m.replica),
        session_id: "conversation".into(),
    }
}
fn pairing_store(
    path: &std::path::Path,
    scope: &bob_transport::pairing::PairingScope,
) -> bob_kernel::outbox::Store {
    let mut store = bob_kernel::outbox::Store::open(path, 1).unwrap();
    store
        .initialize_local_identity("home", "home", 1, 1)
        .unwrap();
    store
        .issue_pairing_invitation(
            &bob_kernel::pairing::PairingInvitation {
                id: scope.invitation_id.clone(),
                home_device_id: scope.home_id.clone(),
                home_epoch: scope.home_epoch,
                home_identity_sha256: scope.home_pin.clone(),
                expires_at: 100,
            },
            &[7u8; 32],
            1,
        )
        .unwrap();
    store
}
#[test]
fn actual_tls_pairing_proves_key_but_requires_local_confirmation_before_grant() {
    let m = material();
    let scope = pairing_scope(&m);
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("pairing.sqlite");
    let mut store = pairing_store(&path, &scope);
    let s = server(&m, scope.candidate_pin.clone());
    let c = client(&m, scope.home_pin.clone());
    let l = listener();
    let address = l.local_addr().unwrap();
    let ss = scope.clone();
    let worker = std::thread::spawn(move || {
        s.serve_pairing_home_once(
            l.accept().unwrap().0,
            Limits::default(),
            &ss,
            &mut store,
            || 2,
        )
        .unwrap();
        store
    });
    let pending = c
        .propose_pairing(address, name(), &scope, [7u8; 32], Limits::default())
        .unwrap();
    assert_eq!(pending.candidate_pin, scope.candidate_pin);
    let mut store = worker.join().unwrap();
    assert_eq!(
        store.pairing_status(&scope.invitation_id, 3).unwrap().state,
        "pending"
    );
    assert!(store
        .authenticate_home_session("home", 1, &scope.candidate_pin, "conversation", 3)
        .is_err());
    let s = server(&m, scope.candidate_pin.clone());
    let l = listener();
    let address = l.local_addr().unwrap();
    let ss = scope.clone();
    let worker = std::thread::spawn(move || {
        let result = s.serve_pairing_confirmation_home_once(
            l.accept().unwrap().0,
            Limits::default(),
            &ss,
            &mut store,
            || 3,
        );
        assert!(result.is_err());
        store
    });
    assert!(c
        .confirm_pairing(address, name(), &scope, Limits::default())
        .is_err());
    let mut store = worker.join().unwrap();
    // This is the trusted local user decision, never a network operation.
    store
        .confirm_pairing_peer(
            &scope.invitation_id,
            &scope.candidate_id,
            &scope.candidate_pin,
            &scope.session_id,
            1000,
            true,
            4,
        )
        .unwrap();
    let s = server(&m, scope.candidate_pin.clone());
    let l = listener();
    let address = l.local_addr().unwrap();
    let ss = scope.clone();
    let worker = std::thread::spawn(move || {
        s.serve_pairing_confirmation_home_once(
            l.accept().unwrap().0,
            Limits::default(),
            &ss,
            &mut store,
            || 5,
        )
    });
    let proof = c
        .confirm_pairing(address, name(), &scope, Limits::default())
        .unwrap();
    worker.join().unwrap().unwrap();
    assert_eq!(proof.home_id(), "home");
    assert_eq!(proof.candidate_pin(), scope.candidate_pin);
    assert_eq!(proof.session_id(), "conversation");
    assert_eq!(proof.authorization_revision(), 1);
}
#[test]
fn pairing_wrong_challenge_expiry_and_wrong_candidate_cannot_stage_or_grant() {
    for mode in ["challenge", "expired", "candidate"] {
        let m = material();
        let scope = pairing_scope(&m);
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("pairing.sqlite");
        let mut store = pairing_store(&path, &scope);
        let s = server(&m, scope.candidate_pin.clone());
        let c = client(&m, scope.home_pin.clone());
        let l = listener();
        let address = l.local_addr().unwrap();
        let ss = scope.clone();
        let worker = std::thread::spawn(move || {
            assert!(s
                .serve_pairing_home_once(
                    l.accept().unwrap().0,
                    Limits::default(),
                    &ss,
                    &mut store,
                    || if mode == "expired" { 101 } else { 2 }
                )
                .is_err());
            store
        });
        let mut client_scope = scope.clone();
        if mode == "candidate" {
            client_scope.candidate_id = "impostor".into();
        }
        assert!(c
            .propose_pairing(
                address,
                name(),
                &client_scope,
                if mode == "challenge" {
                    [8u8; 32]
                } else {
                    [7u8; 32]
                },
                Limits::default()
            )
            .is_err());
        let store = worker.join().unwrap();
        assert_ne!(
            store.pairing_status(&scope.invitation_id, 2).unwrap().state,
            "pending"
        );
        let db = rusqlite::Connection::open(&path).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM home_device_authorizations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[test]
fn pairing_callbacks_use_live_localapp_without_reopening_owned_store() {
 use bob_kernel::{app::LocalApp,outbox::{Store,Error},pairing::{PairingInvitation,ProvenPairingPeer}};
 use std::sync::Mutex;
 let m=material();let mut scope=pairing_scope(&m);let d=tempfile::tempdir().unwrap();let path=d.path().join("app.sqlite");
 let mut app=LocalApp::open(&path).unwrap();scope.home_id=app.device_id().into();
 app.issue_pairing_invitation(&PairingInvitation{id:scope.invitation_id.clone(),home_device_id:scope.home_id.clone(),home_epoch:1,home_identity_sha256:scope.home_pin.clone(),expires_at:100},&[7u8;32],1).unwrap();
 // This is the production ownership constraint that a second Store violated.
 assert!(matches!(Store::open(&path,2),Err(Error::OwnershipUnavailable)));
 let shared=Arc::new(Mutex::new(app));let s=server(&m,scope.candidate_pin.clone());let c=client(&m,scope.home_pin.clone());
 let l=listener();let address=l.local_addr().unwrap();let ss=scope.clone();let owner=shared.clone();
 let worker=std::thread::spawn(move||s.serve_pairing_rpc_once(l.accept().unwrap().0,Limits::default(),&ss,|peer,challenge|{
  let mut app=owner.lock().unwrap();
  app.propose_pairing_peer(&ss.invitation_id,&challenge,&ProvenPairingPeer{device_id:ss.candidate_id.clone(),identity_sha256:peer.certificate_sha256().into(),public_key_ref:peer.certificate_sha256().into()},2).map_err(|_|TransportError::Authentication)
 }));
 c.propose_pairing(address,name(),&scope,[7u8;32],Limits::default()).unwrap();worker.join().unwrap().unwrap();
 {
  let mut app=shared.lock().unwrap();assert_eq!(app.pairing_status(&scope.invitation_id,3).unwrap().state,"pending");
  assert!(app.authenticate_home_session(&scope.home_id,1,&scope.candidate_pin,&scope.session_id,3).is_err());
  app.confirm_pairing_peer(&scope.invitation_id,&scope.candidate_id,&scope.candidate_pin,&scope.session_id,1000,true,4).unwrap();
 }
 let s=server(&m,scope.candidate_pin.clone());let l=listener();let address=l.local_addr().unwrap();let ss=scope.clone();let owner=shared.clone();
 let worker=std::thread::spawn(move||s.serve_pairing_confirmation_rpc_once(l.accept().unwrap().0,Limits::default(),&ss,|peer|{
  let mut app=owner.lock().unwrap();let status=app.pairing_status(&ss.invitation_id,5).map_err(|_|TransportError::Authentication)?;
  if status.state!="consumed" || status.device_id.as_deref()!=Some(ss.candidate_id.as_str()) || status.identity_sha256.as_deref()!=Some(peer.certificate_sha256()) {return Err(TransportError::Authentication);}
  app.authenticate_home_session(&ss.home_id,1,peer.certificate_sha256(),&ss.session_id,5).map(|ctx|ctx.authorization_revision()).map_err(|_|TransportError::Authentication)
 }));
 let proof=c.confirm_pairing(address,name(),&scope,Limits::default()).unwrap();worker.join().unwrap().unwrap();
 assert_eq!(proof.home_id(),scope.home_id);assert_eq!(proof.authorization_revision(),1);
 assert!(matches!(Store::open(&path,6),Err(Error::OwnershipUnavailable)));
}
