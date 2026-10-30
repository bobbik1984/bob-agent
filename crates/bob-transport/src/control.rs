//! Versioned Home control RPC on the same pinned mutual TLS connection.
//! All operations are bounded; accepting a message never starts a model here.
use super::*;
use bob_kernel::{
    direct_provider::ReplicaDirectTurnPayload, dispatch::CancelExecution, home_result::HomeResult,
    outbox::Store,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(
    tag = "operation",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum HomeRequest {
    Append(AppendMessage),
    Lookup(AppendMessage),
    Result(AppendMessage),
    Cancel(CancelExecution),
    LookupCancel(CancelExecution),
    ReplicaDirectTurn(ReplicaDirectTurnPayload),
}
impl HomeRequest {
    pub fn binding(&self) -> AppendMessage {
        match self {
            Self::Append(r) | Self::Lookup(r) | Self::Result(r) => r.clone(),
            Self::Cancel(r) | Self::LookupCancel(r) => AppendMessage {
                request_id: r.request_id.clone(),
                session_id: r.session_id.clone(),
                message_id: r.execution_id.clone(),
                origin_device_id: r.origin_device_id.clone(),
                home_device_id: r.home_device_id.clone(),
                home_epoch: r.home_epoch,
                content: serde_json::Value::Null,
            },
            Self::ReplicaDirectTurn(r) => AppendMessage {
                request_id: r.sync_request_id.clone(),
                session_id: r.conversation_id.clone(),
                message_id: r.user_message_id.clone(),
                origin_device_id: r.origin_device_id.clone(),
                home_device_id: r.home_device_id.clone(),
                home_epoch: r.home_epoch,
                content: serde_json::json!({"text": r.user_text}),
            },
        }
    }
    fn payload_hash(&self) -> Result<String, TransportError> {
        if let Self::ReplicaDirectTurn(r) = self {
            return Ok(r.payload_sha256.clone());
        }
        let bytes = match self {
            Self::Append(r) | Self::Lookup(r) | Self::Result(r) => serde_json::to_vec(r),
            Self::Cancel(r) | Self::LookupCancel(r) => serde_json::to_vec(r),
            Self::ReplicaDirectTurn(_) => unreachable!(),
        }
        .map_err(|_| TransportError::Protocol)?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u8,
    body: HomeRequest,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    version: u8,
    request_sha256: String,
    result: HomeResult,
}
pub struct AuthenticatedHomeResult {
    pub peer: AuthenticatedPeer,
    pub result: HomeResult,
}

impl PinnedClient {
    pub fn control(
        &self,
        address: SocketAddr,
        name: ServerName<'static>,
        request: &HomeRequest,
        limits: Limits,
    ) -> Result<AuthenticatedHomeResult, SendFailure> {
        let before = |error| SendFailure {
            may_have_been_sent: false,
            error,
        };
        let limits = limits.validate().map_err(before)?;
        let frame = encode(
            &Request {
                version: 2,
                body: request.clone(),
            },
            limits.frame_bytes,
        )
        .map_err(before)?;
        let expected = format!("{:x}", Sha256::digest(&frame));
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
        let response: Response =
            decode(&read_frame(&mut stream, limits.frame_bytes).map_err(after)?).map_err(after)?;
        let binding = request.binding();
        let hash = request.payload_hash().map_err(after)?;
        if response.version != 2
            || response.request_sha256 != expected
            || response.result.receipt.as_ref().is_some_and(|r| {
                r.request_id != binding.request_id
                    || r.home_device_id != binding.home_device_id
                    || r.home_epoch != binding.home_epoch
                    || r.payload_sha256 != hash
            })
        {
            return Err(after(TransportError::Protocol));
        }
        if !matches!(request, HomeRequest::Result(_)) && response.result.execution.is_some() {
            return Err(after(TransportError::Protocol));
        }
        if response.result.execution.is_some()
            && !response.result.receipt.as_ref().is_some_and(|r| r.accepted)
        {
            return Err(after(TransportError::Protocol));
        }
        Ok(AuthenticatedHomeResult {
            peer,
            result: response.result,
        })
    }
}
impl PinnedServer {
    /// Use on a dedicated v2 listener. Local identity/epoch and clock are trusted
    /// host configuration. No peer-provided field chooses the local authority.
    pub fn serve_control_home_once(
        &self,
        socket: TcpStream,
        limits: Limits,
        store: &mut Store,
        home_id: &str,
        epoch: i64,
        clock: impl FnOnce() -> i64,
    ) -> Result<(), TransportError> {
        self.serve_control_rpc_once(socket, limits, |peer, request| {
            let binding = request.binding();
            let now = clock();
            let ctx = store
                .authenticate_home_session(
                    home_id,
                    epoch,
                    peer.certificate_sha256(),
                    &binding.session_id,
                    now,
                )
                .map_err(|_| TransportError::Authentication)?;
            match &request {
                HomeRequest::Result(r) => store.lookup_home_result(&ctx, r, now),
                HomeRequest::Append(r) => store.receive_message(&ctx, r, now).map(|r| HomeResult {
                    receipt: Some(r),
                    execution: None,
                }),
                HomeRequest::Lookup(r) => {
                    store
                        .lookup_message_receipt(&ctx, r, now)
                        .map(|r| HomeResult {
                            receipt: r,
                            execution: None,
                        })
                }
                HomeRequest::Cancel(r) => store.receive_cancel(&ctx, r, now).map(|r| HomeResult {
                    receipt: Some(r),
                    execution: None,
                }),
                HomeRequest::LookupCancel(r) => {
                    store
                        .lookup_cancel_receipt(&ctx, r, now)
                        .map(|r| HomeResult {
                            receipt: r,
                            execution: None,
                        })
                }
                HomeRequest::ReplicaDirectTurn(r) => store
                    .receive_replica_direct_turn(&ctx, r, now)
                    .map(|r| HomeResult {
                        receipt: Some(r),
                        execution: None,
                    }),
            }
            .map_err(|_| TransportError::Protocol)
        })
    }
    /// TLS and frame I/O happen outside the host callback, allowing the host to
    /// acquire its database mutex only while processing a bounded request.
    pub fn serve_control_rpc_once(
        &self,
        socket: TcpStream,
        limits: Limits,
        handler: impl FnOnce(&AuthenticatedPeer, HomeRequest) -> Result<HomeResult, TransportError>,
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
        let bytes = read_frame(&mut stream, limits.frame_bytes)?;
        let request: Request = decode(&bytes)?;
        if request.version != 2 {
            return Err(TransportError::Protocol);
        }
        let result = handler(&peer, request.body)?;
        let frame = encode(
            &Response {
                version: 2,
                request_sha256: format!("{:x}", Sha256::digest(&bytes)),
                result,
            },
            limits.frame_bytes,
        )?;
        write_frame(&mut stream, &frame)
    }
}
