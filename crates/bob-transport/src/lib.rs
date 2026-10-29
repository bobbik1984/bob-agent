//! Bounded, mutually authenticated Home message transport, not a Home server.
//! Callers provision identities from their vault and pair certificate fingerprints.
//! No trust-on-first-use, plaintext fallback, discovery, or model execution here.
use bob_kernel::outbox::{AppendMessage, HomeReceipt};
pub use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::{
    ClientConfig, ClientConnection, RootCertStore, ServerConfig, ServerConnection, StreamOwned,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream},
    sync::Arc,
    time::{Duration, Instant},
};

const HARD_FRAME_LIMIT: usize = 256 * 1024;
#[derive(Clone, Copy)]
pub struct Limits {
    pub frame_bytes: usize,
    pub deadline: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            frame_bytes: HARD_FRAME_LIMIT,
            deadline: Duration::from_secs(5),
        }
    }
}
impl Limits {
    fn validate(self) -> Result<Self, TransportError> {
        if self.frame_bytes == 0
            || self.frame_bytes > HARD_FRAME_LIMIT
            || self.deadline.is_zero()
            || self.deadline > Duration::from_secs(60)
        {
            return Err(TransportError::InvalidLimits);
        }
        Ok(self)
    }
}
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("invalid transport limits")]
    InvalidLimits,
    #[error("invalid TLS identity or trust configuration")]
    Configuration,
    #[error("authenticated certificate does not match paired identity")]
    PeerMismatch,
    #[error("TLS protocol or authenticated peer missing")]
    Authentication,
    #[error("frame exceeds configured limit")]
    FrameLimit,
    #[error("invalid or mismatched protocol message")]
    Protocol,
    #[error("transport I/O failed")]
    Io(#[from] io::Error),
}
#[derive(Debug)]
pub struct SendFailure {
    /// false only when no application-frame write was attempted. TLS handshake
    /// bytes are not a dispatched business intent. true requires receipt lookup.
    pub may_have_been_sent: bool,
    pub error: TransportError,
}
/// Never serialized or constructible from peer-supplied JSON.
#[derive(Debug)]
pub struct AuthenticatedPeer {
    certificate_sha256: String,
}
impl AuthenticatedPeer {
    pub fn certificate_sha256(&self) -> &str {
        &self.certificate_sha256
    }
}
pub struct AuthenticatedReceipt {
    pub peer: AuthenticatedPeer,
    pub receipt: HomeReceipt,
}
/// An absent receipt is an observation, never permission to resend an intent.
pub struct AuthenticatedLookup {
    pub peer: AuthenticatedPeer,
    pub receipt: Option<HomeReceipt>,
}
pub struct TlsIdentity {
    pub certificates: Vec<CertificateDer<'static>>,
    pub private_key: PrivateKeyDer<'static>,
}
pub struct PinnedClient {
    config: Arc<ClientConfig>,
    peer_pin: String,
}
pub struct PinnedServer {
    config: Arc<ServerConfig>,
    peer_pin: String,
}
pub fn certificate_fingerprint(cert: &CertificateDer<'_>) -> String {
    format!("{:x}", Sha256::digest(cert.as_ref()))
}
fn roots(certificates: Vec<CertificateDer<'static>>) -> Result<RootCertStore, TransportError> {
    if certificates.is_empty() {
        return Err(TransportError::Configuration);
    }
    let mut roots = RootCertStore::empty();
    for certificate in certificates {
        roots
            .add(certificate)
            .map_err(|_| TransportError::Configuration)?;
    }
    Ok(roots)
}
fn valid_pin(pin: &str) -> bool {
    pin.len() == 64
        && pin
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl PinnedClient {
    pub fn new(
        identity: TlsIdentity,
        anchors: Vec<CertificateDer<'static>>,
        expected_peer_sha256: String,
    ) -> Result<Self, TransportError> {
        if !valid_pin(&expected_peer_sha256) {
            return Err(TransportError::Configuration);
        }
        let mut config =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(&[&rustls::version::TLS13])
                .map_err(|_| TransportError::Configuration)?
                .with_root_certificates(roots(anchors)?)
                .with_client_auth_cert(identity.certificates, identity.private_key)
                .map_err(|_| TransportError::Configuration)?;
        config.enable_early_data = false;
        config.resumption = rustls::client::Resumption::disabled();
        config.alpn_protocols = vec![b"bob-home/1".to_vec()];
        Ok(Self {
            config: Arc::new(config),
            peer_pin: expected_peer_sha256,
        })
    }
    pub fn send(
        &self,
        address: SocketAddr,
        server_name: ServerName<'static>,
        request: &AppendMessage,
        limits: Limits,
    ) -> Result<AuthenticatedReceipt, SendFailure> {
        let response = self.exchange(address, server_name, request, limits, Operation::Append)?;
        Ok(AuthenticatedReceipt {
            peer: response.peer,
            receipt: response.receipt.ok_or(SendFailure {
                may_have_been_sent: true,
                error: TransportError::Protocol,
            })?,
        })
    }
    /// Queries the original exact intent. Never appends, executes, or retries it.
    pub fn lookup(
        &self,
        address: SocketAddr,
        server_name: ServerName<'static>,
        request: &AppendMessage,
        limits: Limits,
    ) -> Result<AuthenticatedLookup, SendFailure> {
        self.exchange(address, server_name, request, limits, Operation::Lookup)
    }
    fn exchange(
        &self,
        address: SocketAddr,
        server_name: ServerName<'static>,
        request: &AppendMessage,
        limits: Limits,
        operation: Operation,
    ) -> Result<AuthenticatedLookup, SendFailure> {
        let before = |error| SendFailure {
            may_have_been_sent: false,
            error,
        };
        let limits = limits.validate().map_err(before)?;
        // Serialize with an incremental cap before opening a connection.
        let wire = RequestWire {
            version: 1,
            operation,
            message: request.clone(),
        };
        let frame = encode(&wire, limits.frame_bytes).map_err(before)?;
        let deadline = Instant::now() + limits.deadline;
        let socket =
            TcpStream::connect_timeout(&address, limits.deadline).map_err(|e| before(e.into()))?;
        let mut socket = DeadlineSocket {
            inner: socket,
            deadline,
        };
        let mut tls = ClientConnection::new(self.config.clone(), server_name)
            .map_err(|_| before(TransportError::Configuration))?;
        while tls.is_handshaking() {
            tls.complete_io(&mut socket).map_err(|e| before(e.into()))?;
        }
        let peer = check_peer(
            tls.peer_certificates(),
            tls.protocol_version(),
            tls.alpn_protocol(),
            &self.peer_pin,
        )
        .map_err(before)?;
        let mut stream = StreamOwned::new(tls, socket);
        let after = |error| SendFailure {
            may_have_been_sent: true,
            error,
        };
        write_frame(&mut stream, &frame).map_err(after)?;
        let response: ReceiptWire =
            decode(&read_frame(&mut stream, limits.frame_bytes).map_err(after)?).map_err(after)?;
        let payload_hash = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(request).map_err(|_| after(TransportError::Protocol))?
            )
        );
        if response.version != 1
            || response.operation != operation
            || response.request_id != request.request_id
            || response.home_device_id != request.home_device_id
            || response.home_epoch != request.home_epoch
            || response.payload_sha256 != payload_hash
            || response.receipt.as_ref().is_some_and(|receipt| {
                receipt.request_id != request.request_id
                    || receipt.home_device_id != request.home_device_id
                    || receipt.home_epoch != request.home_epoch
                    || receipt.payload_sha256 != payload_hash
            })
        {
            return Err(after(TransportError::Protocol));
        }
        Ok(AuthenticatedLookup {
            peer,
            receipt: response.receipt,
        })
    }
}
impl PinnedServer {
    pub fn new(
        identity: TlsIdentity,
        anchors: Vec<CertificateDer<'static>>,
        expected_peer_sha256: String,
    ) -> Result<Self, TransportError> {
        if !valid_pin(&expected_peer_sha256) {
            return Err(TransportError::Configuration);
        }
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
            Arc::new(roots(anchors)?),
            provider.clone(),
        )
        .build()
        .map_err(|_| TransportError::Configuration)?;
        let mut config = ServerConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|_| TransportError::Configuration)?
            .with_client_cert_verifier(verifier)
            .with_single_cert(identity.certificates, identity.private_key)
            .map_err(|_| TransportError::Configuration)?;
        config.max_early_data_size = 0;
        config.send_tls13_tickets = 0;
        config.alpn_protocols = vec![b"bob-home/1".to_vec()];
        Ok(Self {
            config: Arc::new(config),
            peer_pin: expected_peer_sha256,
        })
    }
    /// Trusted Home host adapter. Home identity/epoch and clock are local host
    /// configuration, never taken from the incoming message. The kernel resolves
    /// the device from the TLS-observed certificate and rechecks persisted grants
    /// before durable acceptance. This does not authorize model execution.
    #[allow(clippy::too_many_arguments)]
    pub fn serve_home_once(
        &self,
        socket: TcpStream,
        limits: Limits,
        store: &mut bob_kernel::outbox::Store,
        home_id: &str,
        epoch: i64,
        clock: impl FnOnce() -> i64,
    ) -> Result<(), TransportError> {
        self.serve_rpc_once(socket, limits, |peer, operation, request| {
            let now = clock();
            let context = store
                .authenticate_home_session(
                    home_id,
                    epoch,
                    peer.certificate_sha256(),
                    &request.session_id,
                    now,
                )
                .map_err(|_| TransportError::Authentication)?;
            match operation {
                Operation::Append => store.receive_message(&context, &request, now).map(Some),
                Operation::Lookup => store.lookup_message_receipt(&context, &request, now),
            }
            .map_err(|_| TransportError::Protocol)
        })
    }
    /// Listener lifetime/address and policy authorization belong to the host.
    /// The callback receives TLS-observed identity, never a claimed device id.
    /// A host callback must be bounded; this deadline bounds network I/O only.
    pub fn serve_once(
        &self,
        socket: TcpStream,
        limits: Limits,
        handler: impl FnOnce(&AuthenticatedPeer, AppendMessage) -> Result<HomeReceipt, TransportError>,
    ) -> Result<(), TransportError> {
        self.serve_rpc_once(socket, limits, |peer, operation, request| {
            if operation != Operation::Append {
                return Err(TransportError::Protocol);
            }
            handler(peer, request).map(Some)
        })
    }
    fn serve_rpc_once(
        &self,
        socket: TcpStream,
        limits: Limits,
        handler: impl FnOnce(
            &AuthenticatedPeer,
            Operation,
            AppendMessage,
        ) -> Result<Option<HomeReceipt>, TransportError>,
    ) -> Result<(), TransportError> {
        let limits = limits.validate()?;
        let mut socket = DeadlineSocket {
            inner: socket,
            deadline: Instant::now() + limits.deadline,
        };
        let mut tls = ServerConnection::new(self.config.clone())
            .map_err(|_| TransportError::Configuration)?;
        while tls.is_handshaking() {
            tls.complete_io(&mut socket)?;
        }
        let peer = check_peer(
            tls.peer_certificates(),
            tls.protocol_version(),
            tls.alpn_protocol(),
            &self.peer_pin,
        )?;
        let mut stream = StreamOwned::new(tls, socket);
        let request: RequestWire = decode(&read_frame(&mut stream, limits.frame_bytes)?)?;
        if request.version != 1 {
            return Err(TransportError::Protocol);
        }
        let payload_sha256 = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&request.message).map_err(|_| TransportError::Protocol)?
            )
        );
        let request_id = request.message.request_id.clone();
        let home_device_id = request.message.home_device_id.clone();
        let home_epoch = request.message.home_epoch;
        let response = handler(&peer, request.operation, request.message)?;
        let frame = encode(
            &ReceiptWire {
                version: 1,
                receipt: response,
                operation: request.operation,
                request_id,
                home_device_id,
                home_epoch,
                payload_sha256,
            },
            limits.frame_bytes,
        )?;
        write_frame(&mut stream, &frame)?;
        Ok(())
    }
}
fn check_peer(
    certs: Option<&[CertificateDer<'static>]>,
    version: Option<rustls::ProtocolVersion>,
    alpn: Option<&[u8]>,
    pin: &str,
) -> Result<AuthenticatedPeer, TransportError> {
    if version != Some(rustls::ProtocolVersion::TLSv1_3) || alpn != Some(b"bob-home/1".as_slice()) {
        return Err(TransportError::Authentication);
    }
    let leaf = certs
        .and_then(|c| c.first())
        .ok_or(TransportError::Authentication)?;
    let fingerprint = certificate_fingerprint(leaf);
    if fingerprint != pin {
        return Err(TransportError::PeerMismatch);
    }
    Ok(AuthenticatedPeer {
        certificate_sha256: fingerprint,
    })
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Operation {
    Append,
    Lookup,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestWire {
    version: u8,
    operation: Operation,
    message: AppendMessage,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptWire {
    version: u8,
    receipt: Option<HomeReceipt>,
    operation: Operation,
    request_id: String,
    home_device_id: String,
    home_epoch: i64,
    payload_sha256: String,
}
struct Capped(Vec<u8>, usize);
impl Write for Capped {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.len() > self.1.saturating_sub(self.0.len()) {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn encode<T: Serialize>(message: &T, limit: usize) -> Result<Vec<u8>, TransportError> {
    let mut out = Capped(Vec::new(), limit);
    serde_json::to_writer(&mut out, message).map_err(|e| {
        if e.is_io() {
            TransportError::FrameLimit
        } else {
            TransportError::Protocol
        }
    })?;
    Ok(out.0)
}
fn decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, TransportError> {
    serde_json::from_slice(bytes).map_err(|_| TransportError::Protocol)
}
fn write_frame(stream: &mut impl Write, frame: &[u8]) -> Result<(), TransportError> {
    stream.write_all(&(frame.len() as u32).to_be_bytes())?;
    stream.write_all(frame)?;
    stream.flush()?;
    Ok(())
}
fn read_frame(stream: &mut impl Read, limit: usize) -> Result<Vec<u8>, TransportError> {
    let mut size = [0; 4];
    stream.read_exact(&mut size)?;
    let size = u32::from_be_bytes(size) as usize;
    if size == 0 || size > limit {
        return Err(TransportError::FrameLimit);
    }
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes)?;
    Ok(bytes)
}
/// Every read/write recomputes the same absolute deadline, preventing slow-drip
/// peers from extending a per-read timeout indefinitely.
struct DeadlineSocket {
    inner: TcpStream,
    deadline: Instant,
}
impl DeadlineSocket {
    fn remaining(&self) -> io::Result<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| io::ErrorKind::TimedOut.into())
    }
}
impl Read for DeadlineSocket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.set_read_timeout(Some(self.remaining()?))?;
        self.inner.read(buf)
    }
}
impl Write for DeadlineSocket {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.set_write_timeout(Some(self.remaining()?))?;
        self.inner.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.set_write_timeout(Some(self.remaining()?))?;
        self.inner.flush()
    }
}
