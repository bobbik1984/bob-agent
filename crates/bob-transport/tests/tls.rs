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
