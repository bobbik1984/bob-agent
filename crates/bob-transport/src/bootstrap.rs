//! First-pair QR bootstrap: authenticated Home TLS, candidate proof of key
//! possession, and invitation-bound pending proposal only. This listener never
//! accepts messages, executes work, or grants access. Normal traffic still uses
//! exact pinned mutual TLS after explicit local confirmation.
use crate::pairing::PairingScope;
use crate::*;
use rustls::{sign::CertifiedKey, SignatureScheme};

const ALPN: &[u8] = b"bob-bootstrap/1";
const EXPORTER: &[u8] = b"bob-qr-pairing-v1";
const CERT_LIMIT: usize = 16 * 1024;
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BootstrapScope {
    pub invitation_id: String,
    pub home_id: String,
    pub home_epoch: i64,
    pub home_pin: String,
    pub expires_at: i64,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BootstrapEndpoints {
    pub confirmation_address: SocketAddr,
    pub control_address: SocketAddr,
}
/// Constructed only after validating the candidate's signature against the
/// public key inside this certificate on this exact TLS connection.
pub struct ProvenBootstrapCandidate {
    candidate: Candidate,
}
impl ProvenBootstrapCandidate {
    pub fn device_id(&self) -> &str {
        &self.candidate.device_id
    }
    pub fn session_id(&self) -> &str {
        &self.candidate.session_id
    }
    pub fn certificate_der(&self) -> &[u8] {
        &self.candidate.certificate_der
    }
    pub fn certificate_sha256(&self) -> &str {
        &self.candidate.certificate_sha256
    }
}
pub struct AuthenticatedBootstrapPending {
    pub peer: AuthenticatedPeer,
    pub scope: PairingScope,
    pub endpoints: BootstrapEndpoints,
}
pub struct BootstrapServer {
    config: Arc<ServerConfig>,
    own_pin: String,
}
pub struct BootstrapClient {
    config: Arc<ClientConfig>,
    peer_pin: String,
    identity: CertifiedKey,
    certificate: Vec<u8>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Candidate {
    device_id: String,
    session_id: String,
    certificate_der: Vec<u8>,
    certificate_sha256: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proposal {
    version: u8,
    scope: BootstrapScope,
    challenge: [u8; 32],
    candidate: Candidate,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedProposal {
    proposal: Proposal,
    signature: Vec<u8>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingWire {
    version: u8,
    proposal_sha256: String,
    endpoints: BootstrapEndpoints,
}
fn id(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 512 && !s.chars().any(char::is_control)
}
fn valid_scope(s: &BootstrapScope) -> bool {
    id(&s.invitation_id)
        && id(&s.home_id)
        && s.home_epoch > 0
        && valid_pin(&s.home_pin)
        && s.expires_at > 0
}
fn endpoints(e: &BootstrapEndpoints) -> bool {
    [&e.confirmation_address, &e.control_address]
        .iter()
        .all(|a| {
            a.port() != 0
                && !a.ip().is_unspecified()
                && !a.ip().is_multicast()
                && a.ip() != std::net::IpAddr::V4(std::net::Ipv4Addr::BROADCAST)
        })
}
fn transcript(p: &Proposal, binding: &[u8; 32]) -> Result<Vec<u8>, TransportError> {
    let mut bytes = b"Bob QR pairing proof v1\0".to_vec();
    bytes.extend_from_slice(binding);
    bytes.extend_from_slice(&encode(p, HARD_FRAME_LIMIT)?);
    Ok(bytes)
}
fn protocol(
    version: Option<rustls::ProtocolVersion>,
    alpn: Option<&[u8]>,
) -> Result<(), TransportError> {
    if version != Some(rustls::ProtocolVersion::TLSv1_3) || alpn != Some(ALPN) {
        return Err(TransportError::Authentication);
    }
    Ok(())
}
impl BootstrapServer {
    /// No client authentication is requested at bootstrap. This is NOT a
    /// permissive certificate verifier: candidate proof is checked separately
    /// before the sole pending-proposal callback can run.
    pub fn new(identity: TlsIdentity) -> Result<Self, TransportError> {
        let own_pin = certificate_fingerprint(
            identity
                .certificates
                .first()
                .ok_or(TransportError::Configuration)?,
        );
        let mut config =
            ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(&[&rustls::version::TLS13])
                .map_err(|_| TransportError::Configuration)?
                .with_no_client_auth()
                .with_single_cert(identity.certificates, identity.private_key)
                .map_err(|_| TransportError::Configuration)?;
        config.alpn_protocols = vec![ALPN.to_vec()];
        config.max_early_data_size = 0;
        config.send_tls13_tickets = 0;
        Ok(Self {
            config: Arc::new(config),
            own_pin,
        })
    }
    /// Callback must atomically check the one-use challenge and stage pending.
    /// It must not grant permissions. Kernel pairing invitation CAS provides
    /// durable replay protection; the exporter also rejects cross-TLS replay.
    pub fn serve_once(
        &self,
        socket: TcpStream,
        limits: Limits,
        scope: &BootstrapScope,
        clock: impl FnOnce() -> i64,
        handler: impl FnOnce(
            &ProvenBootstrapCandidate,
            [u8; 32],
        ) -> Result<BootstrapEndpoints, TransportError>,
    ) -> Result<(), TransportError> {
        let limits = limits.validate()?;
        if !valid_scope(scope) || scope.home_pin != self.own_pin {
            return Err(TransportError::Configuration);
        }
        let mut socket = DeadlineSocket {
            inner: socket,
            deadline: Instant::now() + limits.deadline,
        };
        let mut tls = ServerConnection::new(self.config.clone())
            .map_err(|_| TransportError::Configuration)?;
        while tls.is_handshaking() {
            tls.complete_io(&mut socket)?;
        }
        protocol(tls.protocol_version(), tls.alpn_protocol())?;
        let binding = tls
            .export_keying_material([0u8; 32], EXPORTER, None)
            .map_err(|_| TransportError::Authentication)?;
        let mut stream = StreamOwned::new(tls, socket);
        let wire: SignedProposal = decode(&read_frame(&mut stream, limits.frame_bytes)?)?;
        let p = &wire.proposal;
        let c = &p.candidate;
        let at = clock();
        if p.version != 1
            || p.scope != *scope
            || at < 0
            || at >= scope.expires_at
            || !id(&c.device_id)
            || c.device_id == scope.home_id
            || !id(&c.session_id)
            || c.certificate_der.is_empty()
            || c.certificate_der.len() > CERT_LIMIT
            || wire.signature.is_empty()
            || wire.signature.len() > 80
        {
            return Err(TransportError::Authentication);
        }
        let certificate = CertificateDer::from(c.certificate_der.as_slice());
        if certificate_fingerprint(&certificate) != c.certificate_sha256 {
            return Err(TransportError::Authentication);
        }
        let end = webpki::EndEntityCert::try_from(&certificate)
            .map_err(|_| TransportError::Authentication)?;
        end.verify_signature(
            webpki::ring::ECDSA_P256_SHA256,
            &transcript(p, &binding)?,
            &wire.signature,
        )
        .map_err(|_| TransportError::Authentication)?;
        let hash = format!("{:x}", Sha256::digest(encode(p, HARD_FRAME_LIMIT)?));
        let outcome = handler(
            &ProvenBootstrapCandidate {
                candidate: c.clone(),
            },
            p.challenge,
        )?;
        if !endpoints(&outcome) {
            return Err(TransportError::Configuration);
        }
        write_frame(
            &mut stream,
            &encode(
                &PendingWire {
                    version: 1,
                    proposal_sha256: hash,
                    endpoints: outcome,
                },
                limits.frame_bytes,
            )?,
        )
    }
}
impl BootstrapClient {
    pub fn new(
        identity: TlsIdentity,
        anchors: Vec<CertificateDer<'static>>,
        home_pin: String,
    ) -> Result<Self, TransportError> {
        if !valid_pin(&home_pin) {
            return Err(TransportError::Configuration);
        }
        let certificate = identity
            .certificates
            .first()
            .ok_or(TransportError::Configuration)?
            .to_vec();
        if certificate.len() > CERT_LIMIT {
            return Err(TransportError::Configuration);
        }
        let identity = CertifiedKey::from_der(
            identity.certificates,
            identity.private_key,
            &rustls::crypto::ring::default_provider(),
        )
        .map_err(|_| TransportError::Configuration)?;
        identity
            .keys_match()
            .map_err(|_| TransportError::Configuration)?;
        if identity
            .key
            .choose_scheme(&[SignatureScheme::ECDSA_NISTP256_SHA256])
            .is_none()
        {
            return Err(TransportError::Configuration);
        }
        let mut config =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(&[&rustls::version::TLS13])
                .map_err(|_| TransportError::Configuration)?
                .with_root_certificates(roots(anchors)?)
                .with_no_client_auth();
        config.alpn_protocols = vec![ALPN.to_vec()];
        config.enable_early_data = false;
        config.resumption = rustls::client::Resumption::disabled();
        Ok(Self {
            config: Arc::new(config),
            peer_pin: home_pin,
            identity,
            certificate,
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn propose(
        &self,
        address: SocketAddr,
        name: ServerName<'static>,
        scope: &BootstrapScope,
        challenge: [u8; 32],
        device_id: &str,
        session_id: &str,
        limits: Limits,
    ) -> Result<AuthenticatedBootstrapPending, SendFailure> {
        let before = |error| SendFailure {
            may_have_been_sent: false,
            error,
        };
        let after = |error| SendFailure {
            may_have_been_sent: true,
            error,
        };
        let limits = limits.validate().map_err(before)?;
        if !valid_scope(scope)
            || scope.home_pin != self.peer_pin
            || !id(device_id)
            || !id(session_id)
            || device_id == scope.home_id
        {
            return Err(before(TransportError::Configuration));
        }
        let candidate = Candidate {
            device_id: device_id.into(),
            session_id: session_id.into(),
            certificate_der: self.certificate.clone(),
            certificate_sha256: format!("{:x}", Sha256::digest(&self.certificate)),
        };
        let proposal = Proposal {
            version: 1,
            scope: scope.clone(),
            challenge,
            candidate,
        };
        let deadline = Instant::now() + limits.deadline;
        let socket =
            TcpStream::connect_timeout(&address, limits.deadline).map_err(|e| before(e.into()))?;
        let mut socket = DeadlineSocket {
            inner: socket,
            deadline,
        };
        let mut tls = ClientConnection::new(self.config.clone(), name)
            .map_err(|_| before(TransportError::Configuration))?;
        while tls.is_handshaking() {
            tls.complete_io(&mut socket).map_err(|e| before(e.into()))?;
        }
        protocol(tls.protocol_version(), tls.alpn_protocol()).map_err(before)?;
        let cert = tls
            .peer_certificates()
            .and_then(|c| c.first())
            .ok_or_else(|| before(TransportError::Authentication))?;
        if certificate_fingerprint(cert) != self.peer_pin {
            return Err(before(TransportError::PeerMismatch));
        }
        let binding = tls
            .export_keying_material([0u8; 32], EXPORTER, None)
            .map_err(|_| before(TransportError::Authentication))?;
        let signature = self
            .identity
            .key
            .choose_scheme(&[SignatureScheme::ECDSA_NISTP256_SHA256])
            .ok_or_else(|| before(TransportError::Configuration))?
            .sign(&transcript(&proposal, &binding).map_err(before)?)
            .map_err(|_| before(TransportError::Authentication))?;
        let hash = format!(
            "{:x}",
            Sha256::digest(encode(&proposal, HARD_FRAME_LIMIT).map_err(before)?)
        );
        let frame = encode(
            &SignedProposal {
                proposal: proposal.clone(),
                signature,
            },
            limits.frame_bytes,
        )
        .map_err(before)?;
        let mut stream = StreamOwned::new(tls, socket);
        write_frame(&mut stream, &frame).map_err(after)?;
        let response: PendingWire =
            decode(&read_frame(&mut stream, limits.frame_bytes).map_err(after)?).map_err(after)?;
        if response.version != 1
            || response.proposal_sha256 != hash
            || !endpoints(&response.endpoints)
        {
            return Err(after(TransportError::Protocol));
        }
        Ok(AuthenticatedBootstrapPending {
            peer: AuthenticatedPeer {
                certificate_sha256: self.peer_pin.clone(),
            },
            scope: PairingScope {
                invitation_id: scope.invitation_id.clone(),
                home_id: scope.home_id.clone(),
                home_epoch: scope.home_epoch,
                home_pin: scope.home_pin.clone(),
                candidate_id: device_id.into(),
                candidate_pin: proposal.candidate.certificate_sha256,
                session_id: session_id.into(),
            },
            endpoints: response.endpoints,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bob_kernel::{
        app::LocalApp,
        pairing::{PairingInvitation, ProvenPairingPeer},
    };
    use rcgen::{CertificateParams, ExtendedKeyUsagePurpose, KeyPair};
    use rustls::pki_types::PrivatePkcs8KeyDer;
    use std::{
        net::TcpListener,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Mutex,
        },
    };
    fn identity() -> (TlsIdentity, CertificateDer<'static>) {
        let key = KeyPair::generate().unwrap();
        let mut params = CertificateParams::new(vec!["bootstrap.test".into()]).unwrap();
        params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ServerAuth,
            ExtendedKeyUsagePurpose::ClientAuth,
        ];
        let cert = params.self_signed(&key).unwrap().der().clone();
        (
            TlsIdentity {
                certificates: vec![cert.clone()],
                private_key: PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
            },
            cert,
        )
    }
    struct Fixture {
        _dir: tempfile::TempDir,
        home: Arc<Mutex<LocalApp>>,
        server: Arc<BootstrapServer>,
        client: BootstrapClient,
        scope: BootstrapScope,
        hits: Arc<AtomicUsize>,
    }
    fn fixture() -> Fixture {
        let d = tempfile::tempdir().unwrap();
        let mut app = LocalApp::open(&d.path().join("app.sqlite")).unwrap();
        let (home, cert) = identity();
        let (candidate, _) = identity();
        let scope = BootstrapScope {
            invitation_id: "invitation".into(),
            home_id: app.device_id().into(),
            home_epoch: 1,
            home_pin: certificate_fingerprint(&cert),
            expires_at: 100,
        };
        app.issue_pairing_invitation(
            &PairingInvitation {
                id: scope.invitation_id.clone(),
                home_device_id: scope.home_id.clone(),
                home_epoch: 1,
                home_identity_sha256: scope.home_pin.clone(),
                expires_at: 100,
            },
            &[7; 32],
            1,
        )
        .unwrap();
        Fixture {
            _dir: d,
            home: Arc::new(Mutex::new(app)),
            server: Arc::new(BootstrapServer::new(home).unwrap()),
            client: BootstrapClient::new(candidate, vec![cert], scope.home_pin.clone()).unwrap(),
            scope,
            hits: Arc::new(AtomicUsize::new(0)),
        }
    }
    fn start(
        f: &Fixture,
        now: i64,
    ) -> (
        SocketAddr,
        std::thread::JoinHandle<Result<(), TransportError>>,
    ) {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let server = f.server.clone();
        let scope = f.scope.clone();
        let home = f.home.clone();
        let hits = f.hits.clone();
        let thread = std::thread::spawn(move || {
            server.serve_once(
                listener.accept().unwrap().0,
                Limits::default(),
                &scope,
                || now,
                |proof, challenge| {
                    hits.fetch_add(1, Ordering::SeqCst);
                    home.lock()
                        .unwrap()
                        .propose_pairing_peer(
                            &scope.invitation_id,
                            &challenge,
                            &ProvenPairingPeer {
                                device_id: proof.device_id().into(),
                                identity_sha256: proof.certificate_sha256().into(),
                                public_key_ref: proof.certificate_sha256().into(),
                            },
                            now,
                        )
                        .map_err(|_| TransportError::Authentication)?;
                    Ok(BootstrapEndpoints {
                        confirmation_address: addr,
                        control_address: addr,
                    })
                },
            )
        });
        (addr, thread)
    }
    fn name() -> ServerName<'static> {
        ServerName::try_from("bootstrap.test").unwrap()
    }
    #[test]
    fn actual_tls_proposal_only_stages_pending_and_repeated_invitation_cannot_grant() {
        let f = fixture();
        let (a, t) = start(&f, 2);
        let p = f
            .client
            .propose(
                a,
                name(),
                &f.scope,
                [7; 32],
                "phone",
                "session",
                Limits::default(),
            )
            .unwrap();
        t.join().unwrap().unwrap();
        assert_eq!(p.scope.candidate_id, "phone");
        assert_eq!(p.scope.session_id, "session");
        let mut app = f.home.lock().unwrap();
        assert_eq!(
            app.pairing_status("invitation", 3).unwrap().state,
            "pending"
        );
        assert!(app
            .authenticate_home_session(&f.scope.home_id, 1, &p.scope.candidate_pin, "session", 3)
            .is_err());
        drop(app);
        let (a, t) = start(&f, 4);
        assert!(f
            .client
            .propose(
                a,
                name(),
                &f.scope,
                [7; 32],
                "phone",
                "session",
                Limits::default()
            )
            .is_err());
        assert!(t.join().unwrap().is_err());
        assert_eq!(
            f.home
                .lock()
                .unwrap()
                .pairing_status("invitation", 5)
                .unwrap()
                .state,
            "pending"
        );
    }
    #[test]
    fn wrong_challenge_fails_durable_invitation_check_without_grant() {
        let f = fixture();
        let (a, t) = start(&f, 2);
        assert!(f
            .client
            .propose(
                a,
                name(),
                &f.scope,
                [8; 32],
                "phone",
                "session",
                Limits::default()
            )
            .is_err());
        assert!(t.join().unwrap().is_err());
        assert_eq!(
            f.home
                .lock()
                .unwrap()
                .pairing_status("invitation", 3)
                .unwrap()
                .state,
            "open"
        );
    }
    #[test]
    fn cancelled_invitation_cannot_be_revived_by_valid_proof() {
        let f = fixture();
        f.home.lock().unwrap().cancel_pairing_invitation("invitation").unwrap();
        let (a, t) = start(&f, 2);
        assert!(f.client.propose(a, name(), &f.scope, [7;32], "phone", "session", Limits::default()).is_err());
        assert!(t.join().unwrap().is_err());
        assert_eq!(f.home.lock().unwrap().pairing_status("invitation", 3).unwrap().state, "cancelled");
    }

    #[test]
    fn expired_invitation_never_reaches_kernel_callback() {
        let f = fixture();
        let (a, t) = start(&f, 100);
        assert!(f
            .client
            .propose(
                a,
                name(),
                &f.scope,
                [7; 32],
                "phone",
                "session",
                Limits::default()
            )
            .is_err());
        assert!(t.join().unwrap().is_err());
        assert_eq!(f.hits.load(Ordering::SeqCst), 0);
    }
    // Exercise malformed and replayed wire proofs through the real TLS socket;
    // direct calls to a signature helper would miss channel-binding mistakes.
    fn raw(
        f: &Fixture,
        address: SocketAddr,
        old: Option<Vec<u8>>,
        corrupt: bool,
    ) -> (Vec<u8>, bool) {
        let socket = TcpStream::connect(address).unwrap();
        let socket = DeadlineSocket {
            inner: socket,
            deadline: Instant::now() + Duration::from_secs(3),
        };
        let mut stream = StreamOwned::new(
            ClientConnection::new(f.client.config.clone(), name()).unwrap(),
            socket,
        );
        while stream.conn.is_handshaking() {
            stream.conn.complete_io(&mut stream.sock).unwrap();
        }
        let frame = if let Some(frame) = old {
            frame
        } else {
            let binding = stream
                .conn
                .export_keying_material([0; 32], EXPORTER, None)
                .unwrap();
            let proposal = Proposal {
                version: 1,
                scope: f.scope.clone(),
                challenge: [7; 32],
                candidate: Candidate {
                    device_id: "phone".into(),
                    session_id: "session".into(),
                    certificate_der: f.client.certificate.clone(),
                    certificate_sha256: format!("{:x}", Sha256::digest(&f.client.certificate)),
                },
            };
            let mut signature = f
                .client
                .identity
                .key
                .choose_scheme(&[SignatureScheme::ECDSA_NISTP256_SHA256])
                .unwrap()
                .sign(&transcript(&proposal, &binding).unwrap())
                .unwrap();
            if corrupt {
                let last = signature.len() - 1;
                signature[last] ^= 1;
            }
            encode(
                &SignedProposal {
                    proposal,
                    signature,
                },
                HARD_FRAME_LIMIT,
            )
            .unwrap()
        };
        write_frame(&mut stream, &frame).unwrap();
        let ok = read_frame(&mut stream, HARD_FRAME_LIMIT).is_ok();
        (frame, ok)
    }
    #[test]
    fn invalid_private_key_proof_cannot_stage_candidate() {
        let f = fixture();
        let (a, t) = start(&f, 2);
        assert!(!raw(&f, a, None, true).1);
        assert!(t.join().unwrap().is_err());
        assert_eq!(f.hits.load(Ordering::SeqCst), 0);
        assert_eq!(
            f.home
                .lock()
                .unwrap()
                .pairing_status("invitation", 3)
                .unwrap()
                .state,
            "open"
        );
    }
    #[test]
    fn captured_proof_cannot_replay_on_another_tls_connection() {
        let f = fixture();
        let (a, t) = start(&f, 2);
        let (frame, ok) = raw(&f, a, None, false);
        assert!(ok);
        t.join().unwrap().unwrap();
        let (a, t) = start(&f, 3);
        assert!(!raw(&f, a, Some(frame), false).1);
        assert!(t.join().unwrap().is_err());
        // Second request fails before callback, not merely because pending CAS rejects it.
        assert_eq!(f.hits.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn wrong_home_pin_rejects_before_sending_challenge() {
        let mut f = fixture();
        f.client.peer_pin = "a".repeat(64);
        let mut scope = f.scope.clone();
        scope.home_pin = f.client.peer_pin.clone();
        let (a, t) = start(&f, 2);
        let err = f
            .client
            .propose(
                a,
                name(),
                &scope,
                [7; 32],
                "phone",
                "session",
                Limits::default(),
            )
            .err()
            .unwrap();
        assert!(!err.may_have_been_sent);
        assert!(matches!(err.error, TransportError::PeerMismatch));
        assert!(t.join().unwrap().is_err());
        assert_eq!(f.hits.load(Ordering::SeqCst), 0);
    }
}
