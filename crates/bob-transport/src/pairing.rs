//! Provisional pairing uses only explicit out-of-band certificate pins.
//! Native host exchanges public certificates before creating this exact-peer
//! listener. It never trusts arbitrary presented certificates. Private-key
//! possession is proven by TLS CertificateVerify; local confirmation is separate.
use super::*;
use bob_kernel::{outbox::Store, pairing::ProvenPairingPeer};

/// Trusted native ceremony input. Never derive candidate_pin from network JSON.
#[derive(Clone)]
pub struct PairingScope {
    pub invitation_id: String,
    pub home_id: String,
    pub home_epoch: i64,
    pub home_pin: String,
    pub candidate_id: String,
    pub candidate_pin: String,
    pub session_id: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proposal {
    version: u8,
    invitation_id: String,
    challenge: [u8; 32],
    candidate_id: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    version: u8,
    proposal_sha256: String,
    invitation_id: String,
    candidate_pin: String,
    state: String,
}
pub struct AuthenticatedPairingPending {
    pub peer: AuthenticatedPeer,
    pub invitation_id: String,
    pub candidate_pin: String,
}
impl PairingScope {
    fn validate(&self) -> Result<(), TransportError> {
        if [
            self.invitation_id.as_str(),
            self.home_id.as_str(),
            self.candidate_id.as_str(),
            self.session_id.as_str(),
        ]
        .iter()
        .any(|s| s.trim().is_empty() || s.len() > 512)
            || self.home_epoch < 1
            || !valid_pin(&self.home_pin)
            || !valid_pin(&self.candidate_pin)
            || self.home_id == self.candidate_id
        {
            return Err(TransportError::Configuration);
        }
        Ok(())
    }
}
impl PinnedClient {
    pub fn propose_pairing(
        &self,
        address: SocketAddr,
        name: ServerName<'static>,
        scope: &PairingScope,
        challenge: [u8; 32],
        limits: Limits,
    ) -> Result<AuthenticatedPairingPending, SendFailure> {
        let before = |error| SendFailure {
            may_have_been_sent: false,
            error,
        };
        scope.validate().map_err(before)?;
        if scope.home_pin != self.peer_pin || scope.candidate_pin != self.own_pin {
            return Err(before(TransportError::Configuration));
        }
        let limits = limits.validate().map_err(before)?;
        let frame = encode(
            &Proposal {
                version: 1,
                invitation_id: scope.invitation_id.clone(),
                challenge,
                candidate_id: scope.candidate_id.clone(),
            },
            limits.frame_bytes,
        )
        .map_err(before)?;
        let hash = format!("{:x}", Sha256::digest(&frame));
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
        let pending: Pending =
            decode(&read_frame(&mut stream, limits.frame_bytes).map_err(after)?).map_err(after)?;
        if pending.version != 1
            || pending.proposal_sha256 != hash
            || pending.invitation_id != scope.invitation_id
            || pending.candidate_pin != scope.candidate_pin
            || pending.state != "pending_local_confirmation"
        {
            return Err(after(TransportError::Protocol));
        }
        Ok(AuthenticatedPairingPending {
            peer,
            invitation_id: pending.invitation_id,
            candidate_pin: pending.candidate_pin,
        })
    }
}
impl PinnedServer {
    /// Listener is provisional and scoped to one locally selected invitation
    /// and one candidate. Receiving this proposal never grants conversation access.
    pub fn serve_pairing_home_once(
        &self,
        socket: TcpStream,
        limits: Limits,
        scope: &PairingScope,
        store: &mut Store,
        clock: impl FnOnce() -> i64,
    ) -> Result<(), TransportError> {
        self.serve_pairing_rpc_once(socket, limits, scope, |peer, challenge| {
            let now = clock();
            let status = store
                .pairing_status(&scope.invitation_id, now)
                .map_err(|_| TransportError::Authentication)?;
            if status.state != "open"
                || status.invitation.home_device_id != scope.home_id
                || status.invitation.home_epoch != scope.home_epoch
                || status.invitation.home_identity_sha256 != scope.home_pin
            {
                return Err(TransportError::Authentication);
            }
            store
                .propose_pairing_peer(
                    &scope.invitation_id,
                    &challenge,
                    &ProvenPairingPeer {
                        device_id: scope.candidate_id.clone(),
                        identity_sha256: peer.certificate_sha256().to_owned(),
                        public_key_ref: format!("tls-cert-sha256:{}", peer.certificate_sha256()),
                    },
                    now,
                )
                .map_err(|_| TransportError::Authentication)
        })
    }
    /// The callback may acquire the database only after authenticated I/O has
    /// completed; it must check invitation expiry/current authority and persist
    /// pending state. It MUST NOT confirm or grant access on behalf of the user.
    pub fn serve_pairing_rpc_once(
        &self,
        socket: TcpStream,
        limits: Limits,
        scope: &PairingScope,
        handler: impl FnOnce(&AuthenticatedPeer, [u8; 32]) -> Result<(), TransportError>,
    ) -> Result<(), TransportError> {
        scope.validate()?;
        if self.peer_pin != scope.candidate_pin || self.own_pin != scope.home_pin {
            return Err(TransportError::Configuration);
        }
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
        let frame = read_frame(&mut stream, limits.frame_bytes)?;
        let proposal: Proposal = decode(&frame)?;
        if proposal.version != 1
            || proposal.invitation_id != scope.invitation_id
            || proposal.candidate_id != scope.candidate_id
        {
            return Err(TransportError::Authentication);
        }
        handler(&peer, proposal.challenge)?;
        let response = encode(
            &Pending {
                version: 1,
                proposal_sha256: format!("{:x}", Sha256::digest(&frame)),
                invitation_id: scope.invitation_id.clone(),
                candidate_pin: scope.candidate_pin.clone(),
                state: "pending_local_confirmation".into(),
            },
            limits.frame_bytes,
        )?;
        write_frame(&mut stream, &response)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfirmationRequest {
    version: u8,
    invitation_id: String,
    session_id: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfirmationResponse {
    version: u8,
    request_sha256: String,
    home_id: String,
    home_epoch: i64,
    home_pin: String,
    candidate_id: String,
    candidate_pin: String,
    session_id: String,
    authorization_revision: i64,
}
/// Cannot be created from renderer JSON. Only an authenticated, exact-scope
/// response from the pinned Home produces this proof. Local user consent is
/// still required by the caller before saving a connection.
pub struct AuthenticatedPairingConfirmation {
    scope: PairingScope,
    revision: i64,
}
impl AuthenticatedPairingConfirmation {
    pub fn invitation_id(&self) -> &str {
        &self.scope.invitation_id
    }
    pub fn home_id(&self) -> &str {
        &self.scope.home_id
    }
    pub fn home_epoch(&self) -> i64 {
        self.scope.home_epoch
    }
    pub fn home_pin(&self) -> &str {
        &self.scope.home_pin
    }
    pub fn candidate_id(&self) -> &str {
        &self.scope.candidate_id
    }
    pub fn candidate_pin(&self) -> &str {
        &self.scope.candidate_pin
    }
    pub fn session_id(&self) -> &str {
        &self.scope.session_id
    }
    pub fn authorization_revision(&self) -> i64 {
        self.revision
    }
}
impl PinnedClient {
    /// Query only; does not confirm on the Home or consume an invitation.
    pub fn confirm_pairing(
        &self,
        address: SocketAddr,
        name: ServerName<'static>,
        scope: &PairingScope,
        limits: Limits,
    ) -> Result<AuthenticatedPairingConfirmation, SendFailure> {
        let before = |error| SendFailure {
            may_have_been_sent: false,
            error,
        };
        scope.validate().map_err(before)?;
        if scope.home_pin != self.peer_pin || scope.candidate_pin != self.own_pin {
            return Err(before(TransportError::Configuration));
        }
        let limits = limits.validate().map_err(before)?;
        let frame = encode(
            &ConfirmationRequest {
                version: 2,
                invitation_id: scope.invitation_id.clone(),
                session_id: scope.session_id.clone(),
            },
            limits.frame_bytes,
        )
        .map_err(before)?;
        let hash = format!("{:x}", Sha256::digest(&frame));
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
        check_peer(
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
        let response: ConfirmationResponse =
            decode(&read_frame(&mut stream, limits.frame_bytes).map_err(after)?).map_err(after)?;
        if response.version != 2
            || response.request_sha256 != hash
            || response.home_id != scope.home_id
            || response.home_epoch != scope.home_epoch
            || response.home_pin != scope.home_pin
            || response.candidate_id != scope.candidate_id
            || response.candidate_pin != scope.candidate_pin
            || response.session_id != scope.session_id
            || response.authorization_revision < 1
        {
            return Err(after(TransportError::Protocol));
        }
        Ok(AuthenticatedPairingConfirmation {
            scope: scope.clone(),
            revision: response.authorization_revision,
        })
    }
}
impl PinnedServer {
    pub fn serve_pairing_confirmation_home_once(
        &self,
        socket: TcpStream,
        limits: Limits,
        scope: &PairingScope,
        store: &mut Store,
        clock: impl FnOnce() -> i64,
    ) -> Result<(), TransportError> {
        self.serve_pairing_confirmation_rpc_once(socket, limits, scope, |peer| {
            let now = clock();
            let status = store
                .pairing_status(&scope.invitation_id, now)
                .map_err(|_| TransportError::Authentication)?;
            if status.state != "consumed"
                || status.invitation.home_device_id != scope.home_id
                || status.invitation.home_epoch != scope.home_epoch
                || status.invitation.home_identity_sha256 != scope.home_pin
                || status.device_id.as_deref() != Some(scope.candidate_id.as_str())
                || status.identity_sha256.as_deref() != Some(scope.candidate_pin.as_str())
            {
                return Err(TransportError::Authentication);
            }
            let ctx = store
                .authenticate_home_session(
                    &scope.home_id,
                    scope.home_epoch,
                    peer.certificate_sha256(),
                    &scope.session_id,
                    now,
                )
                .map_err(|_| TransportError::Authentication)?;
            if ctx.origin_device_id() != scope.candidate_id {
                return Err(TransportError::Authentication);
            }
            Ok(ctx.authorization_revision())
        })
    }
    /// Trusted callback must check consumed invitation, exact candidate/scope,
    /// current authority and unrevoked/unexpired persisted session grant.
    pub fn serve_pairing_confirmation_rpc_once(
        &self,
        socket: TcpStream,
        limits: Limits,
        scope: &PairingScope,
        handler: impl FnOnce(&AuthenticatedPeer) -> Result<i64, TransportError>,
    ) -> Result<(), TransportError> {
        scope.validate()?;
        if self.peer_pin != scope.candidate_pin || self.own_pin != scope.home_pin {
            return Err(TransportError::Configuration);
        }
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
        let frame = read_frame(&mut stream, limits.frame_bytes)?;
        let request: ConfirmationRequest = decode(&frame)?;
        if request.version != 2
            || request.invitation_id != scope.invitation_id
            || request.session_id != scope.session_id
        {
            return Err(TransportError::Authentication);
        }
        let revision = handler(&peer)?;
        if revision < 1 {
            return Err(TransportError::Authentication);
        }
        let response = encode(
            &ConfirmationResponse {
                version: 2,
                request_sha256: format!("{:x}", Sha256::digest(&frame)),
                home_id: scope.home_id.clone(),
                home_epoch: scope.home_epoch,
                home_pin: scope.home_pin.clone(),
                candidate_id: scope.candidate_id.clone(),
                candidate_pin: scope.candidate_pin.clone(),
                session_id: scope.session_id.clone(),
                authorization_revision: revision,
            },
            limits.frame_bytes,
        )?;
        write_frame(&mut stream, &response)
    }
}
