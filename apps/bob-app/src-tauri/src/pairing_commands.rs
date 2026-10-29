//! Explicit, exact-peer first-pair ceremony. Only public certificates cross IPC;
//! the short-lived invitation challenge is deliberately shared out of band.
//! Neither a pasted code nor a successful proposal grants execution permission.
use crate::{device_identity::{self, NativeIdentity, PublicIdentity}, home_config::HomeConfig,
    home_link::HomeLinkState, AppState, RunnerState, now, with_app};
use bob_kernel::{app::LocalApp, pairing::{PairingInvitation, ProvenPairingPeer}};
use bob_transport::{pairing::PairingScope, Limits, PinnedClient, PinnedServer, ServerName, TlsIdentity, TransportError};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{net::{IpAddr, SocketAddr, TcpListener}, path::PathBuf,
    sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, time::Duration};
use tauri::{Manager, State};

const CODE_LIMIT: usize = 96 * 1024;
const INVITATION_MS: i64 = 10 * 60 * 1000;
const GRANT_MS: i64 = 60 * 60 * 1000;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceCode { version: u8, identity: PublicIdentity, session_id: String }
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InvitationCode {
    version: u8, invitation_id: String, home: PublicIdentity, home_epoch: i64,
    candidate: DeviceCode, challenge: [u8; 32], expires_at: i64,
    proposal_address: SocketAddr, confirmation_address: SocketAddr, control_address: SocketAddr,
}
impl InvitationCode {
    fn scope(&self) -> PairingScope { PairingScope {
        invitation_id: self.invitation_id.clone(), home_id: self.home.device_id.clone(),
        home_epoch: self.home_epoch, home_pin: self.home.certificate_sha256.clone(),
        candidate_id: self.candidate.identity.device_id.clone(),
        candidate_pin: self.candidate.identity.certificate_sha256.clone(),
        session_id: self.candidate.session_id.clone(),
    }}
}
#[derive(Serialize)]
pub(crate) struct DeviceExport { code: String, device_id: String, certificate_sha256: String, session_id: String }
#[derive(Serialize)]
pub(crate) struct InvitationExport {
    code: String, invitation_id: String, candidate_id: String, candidate_pin: String,
    session_id: String, expires_at: i64, status: &'static str,
}
#[derive(Serialize)]
pub(crate) struct CeremonyStatus {
    role: &'static str, state: String, candidate_id: Option<String>, candidate_pin: Option<String>,
    session_id: Option<String>, expires_at: Option<i64>, error: Option<String>,
    home_pin: Option<String>, grant_expires_at: Option<i64>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HomeRestore {
    version: u8, invitation_id: String, home: PublicIdentity, home_epoch: i64,
    candidate: DeviceCode, bind: SocketAddr, control_address: SocketAddr, grant_expiry: i64,
}
struct ListenerStop(Arc<AtomicBool>);
impl Drop for ListenerStop { fn drop(&mut self) { self.0.store(true, Ordering::SeqCst); } }
struct HomeCeremony {
    invitation: InvitationCode, bind: SocketAddr, root: PathBuf, owner: Arc<Mutex<Option<LocalApp>>>,
    _listeners: ListenerStop, error: Arc<Mutex<Option<String>>>, grant_expiry: Option<i64>,
    host: Option<crate::home_host::HomeHostHandle>,
}
enum Ceremony { Home(HomeCeremony), Replica { invitation: InvitationCode, state: String } }
#[derive(Default)]
pub(crate) struct PairingState(Mutex<Option<Ceremony>>);

fn id(value: &str) -> bool { !value.trim().is_empty() && value.len() <= 512 && !value.chars().any(char::is_control) }
fn valid_identity(value: &PublicIdentity) -> bool {
    id(&value.device_id) && !value.certificate_der.is_empty() && value.certificate_der.len() <= 16384
        && value.certificate_sha256 == format!("{:x}", Sha256::digest(&value.certificate_der))
        && ServerName::try_from(value.tls_server_name.clone()).is_ok()
}
fn valid_address(address: SocketAddr) -> bool {
    address.port() != 0 && !address.ip().is_unspecified() && !address.ip().is_multicast()
        && address.ip() != IpAddr::V4(std::net::Ipv4Addr::BROADCAST)
}
fn decode<T: for<'de> Deserialize<'de>>(text: &str) -> Result<T, &'static str> {
    if text.is_empty() || text.len() > CODE_LIMIT { return Err("PAIRING_CODE_INVALID"); }
    serde_json::from_str(text).map_err(|_| "PAIRING_CODE_INVALID")
}
fn tls(identity: &NativeIdentity) -> TlsIdentity { TlsIdentity {
    certificates: vec![identity.public.certificate_der.clone().into()], private_key: identity.private_key.clone_key(),
}}
fn client(identity: &NativeIdentity, invite: &InvitationCode) -> Result<PinnedClient, &'static str> {
    PinnedClient::new(tls(identity), vec![invite.home.certificate_der.clone().into()], invite.home.certificate_sha256.clone())
        .map_err(|_| "PAIRING_IDENTITY_INVALID")
}
fn server(identity: &NativeIdentity, candidate: &PublicIdentity) -> Result<Arc<PinnedServer>, &'static str> {
    PinnedServer::new(tls(identity), vec![candidate.certificate_der.clone().into()], candidate.certificate_sha256.clone())
        .map(Arc::new).map_err(|_| "PAIRING_IDENTITY_INVALID")
}
fn local_scope(state: &AppState) -> Result<(String, String), &'static str> {
    with_app(&state.0, |a| Ok((a.device_id().to_owned(), a.default_session().map_err(|_| "LOCAL_STORE_UNAVAILABLE")?)))
}
async fn native<R: tauri::Runtime>(app: &tauri::AppHandle<R>, state: &AppState) -> Result<NativeIdentity, &'static str> {
    let (device, _) = local_scope(state)?;
    device_identity::create(app, device.clone()).await?;
    let identity = device_identity::load_native(app).await?.ok_or("IDENTITY_UNAVAILABLE")?;
    if identity.public.device_id != device { return Err("IDENTITY_DEVICE_MISMATCH"); }
    Ok(identity)
}
fn status(ceremony: &Ceremony) -> Result<CeremonyStatus, &'static str> {
    let (invite, role, state, error, grant_expiry) = match ceremony {
        Ceremony::Replica { invitation, state } => (invitation, "replica", state.clone(), None, None),
        Ceremony::Home(home) => {
            let stored = with_app(&home.owner, |a| a.pairing_status(&home.invitation.invitation_id, now()?).map_err(|_| "PAIRING_STORE_UNAVAILABLE"))?;
            let expired = home.grant_expiry.is_some_and(|e| now().is_ok_and(|n| e <= n));
            let running = home.host.as_ref().is_some_and(|h| h.execution_enabled());
            let s = if expired { "expired".into() } else if running { "enabled".into() } else if stored.state == "consumed" { "confirmed".into() } else { stored.state };
            let mut error = home.error.lock().map_err(|_| "PAIRING_STATE_UNAVAILABLE")?.clone();
            if let Some(host) = home.host.as_ref() {
                if let Some(e) = *host.last_error.lock().map_err(|_| "PAIRING_STATE_UNAVAILABLE")? { error = Some(e.into()); }
            }
            (&home.invitation, "home", s, error, home.grant_expiry)
        }
    };
    Ok(CeremonyStatus { role, state, candidate_id: Some(invite.candidate.identity.device_id.clone()),
        candidate_pin: Some(invite.candidate.identity.certificate_sha256.clone()), session_id: Some(invite.candidate.session_id.clone()),
        expires_at: Some(invite.expires_at), error, home_pin: Some(invite.home.certificate_sha256.clone()), grant_expires_at: grant_expiry })
}

fn save_host(home: &HomeCeremony) -> Result<(), &'static str> {
    use std::io::Write;
    let root = &home.root;
    let data = HomeRestore { version: 1, invitation_id: home.invitation.invitation_id.clone(), home: home.invitation.home.clone(),
        home_epoch: home.invitation.home_epoch, candidate: home.invitation.candidate.clone(), bind: home.bind,
        control_address: home.invitation.control_address, grant_expiry: home.grant_expiry.ok_or("PAIRING_CONFIRMATION_REQUIRED")? };
    let body = serde_json::to_vec(&data).map_err(|_| "PAIRING_STORE_UNAVAILABLE")?;
    for (name, bytes) in [("home-host.pending", body.clone()), ("home-host.sha256", format!("{:x}", Sha256::digest(&body)).into_bytes())] {
        let mut f = std::fs::OpenOptions::new().create_new(true).write(true).open(root.join(name)).map_err(|_| "PAIRING_HOST_SAVE_UNCONFIRMED")?;
        f.write_all(&bytes).and_then(|_| f.sync_all()).map_err(|_| "PAIRING_HOST_SAVE_UNCONFIRMED")?;
    }
    std::fs::rename(root.join("home-host.pending"), root.join("home-host.json")).map_err(|_| "PAIRING_HOST_SAVE_UNCONFIRMED")
}
fn load_host(root: &std::path::Path, local: &PublicIdentity, owner: &Arc<Mutex<Option<LocalApp>>>) -> Result<Option<HomeCeremony>, &'static str> {
    use std::io::Read;
    match std::fs::symlink_metadata(root.join("home-host.pending")) { Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}, _ => return Err("PAIRING_HOST_SAVE_UNCONFIRMED") }
    fn read(path: PathBuf, max: usize) -> Result<Option<Vec<u8>>, &'static str> {
        match std::fs::symlink_metadata(&path) { Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Ok(m) if m.is_file() && !m.file_type().is_symlink() => {}, _ => return Err("PAIRING_HOST_SAVE_UNCONFIRMED") }
        let mut b = vec![]; std::fs::File::open(path).map_err(|_| "PAIRING_HOST_SAVE_UNCONFIRMED")?.take((max+1) as u64)
            .read_to_end(&mut b).map_err(|_| "PAIRING_HOST_SAVE_UNCONFIRMED")?;
        if b.len() > max { return Err("PAIRING_HOST_SAVE_UNCONFIRMED"); } Ok(Some(b))
    }
    let (body, seal) = match (read(root.join("home-host.json"), CODE_LIMIT)?, read(root.join("home-host.sha256"), 64)?) {
        (None, None) => return Ok(None), (Some(b), Some(s)) => (b,s), _ => return Err("PAIRING_HOST_SAVE_UNCONFIRMED") };
    if seal != format!("{:x}", Sha256::digest(&body)).as_bytes() { return Err("PAIRING_HOST_CHANGED"); }
    let data: HomeRestore = serde_json::from_slice(&body).map_err(|_| "PAIRING_HOST_CHANGED")?;
    if data.version != 1 || data.home.device_id != local.device_id || data.home.certificate_sha256 != local.certificate_sha256
        || !valid_identity(&data.home) || !valid_identity(&data.candidate.identity) || !id(&data.candidate.session_id)
        || !valid_address(data.control_address) || data.bind.port() != data.control_address.port() { return Err("PAIRING_HOST_CHANGED"); }
    let stored = with_app(owner, |a| a.pairing_status(&data.invitation_id, now()?).map_err(|_| "PAIRING_CONFIRMATION_REQUIRED"))?;
    if stored.state != "consumed" || stored.invitation.home_device_id != local.device_id || stored.invitation.home_epoch != data.home_epoch
        || stored.invitation.home_identity_sha256 != local.certificate_sha256 || stored.device_id.as_deref() != Some(data.candidate.identity.device_id.as_str())
        || stored.identity_sha256.as_deref() != Some(data.candidate.identity.certificate_sha256.as_str()) { return Err("PAIRING_HOST_CHANGED"); }
    // Revalidate persisted permission, not the saved UI's claim of confirmation.
    let ctx = with_app(owner, |a| a.authenticate_home_session(&local.device_id, data.home_epoch, &data.candidate.identity.certificate_sha256, &data.candidate.session_id, now()?)
        .map_err(|_| "PAIRING_GRANT_EXPIRED_OR_REVOKED"))?;
    if ctx.origin_device_id() != data.candidate.identity.device_id { return Err("PAIRING_HOST_CHANGED"); }
    let invitation = InvitationCode { version: 1, invitation_id: data.invitation_id, home: data.home, home_epoch: data.home_epoch,
        candidate: data.candidate, challenge: [0;32], expires_at: stored.invitation.expires_at,
        proposal_address: data.control_address, confirmation_address: data.control_address, control_address: data.control_address };
    Ok(Some(HomeCeremony { invitation, bind: data.bind, root: root.to_owned(), owner: owner.clone(), _listeners: ListenerStop(Arc::new(AtomicBool::new(true))),
        error: Arc::new(Mutex::new(None)), grant_expiry: Some(data.grant_expiry), host: None }))
}

#[tauri::command]
pub(crate) async fn app_pairing_device_code<R: tauri::Runtime>(app: tauri::AppHandle<R>, state: State<'_, AppState>) -> Result<DeviceExport, &'static str> {
    let identity = native(&app, &state).await?.public;
    let (_, session_id) = local_scope(&state)?;
    let code = serde_json::to_string(&DeviceCode { version: 1, identity: identity.clone(), session_id: session_id.clone() }).map_err(|_| "PAIRING_CODE_INVALID")?;
    Ok(DeviceExport { code, device_id: identity.device_id, certificate_sha256: identity.certificate_sha256, session_id })
}

fn listen(listener: TcpListener, server: Arc<PinnedServer>, invite: InvitationCode,
    owner: Arc<Mutex<Option<LocalApp>>>, stop: Arc<AtomicBool>, errors: Arc<Mutex<Option<String>>>, confirmation: bool) -> Result<(), &'static str> {
    listener.set_nonblocking(true).map_err(|_| "PAIRING_LISTENER_FAILED")?;
    std::thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) && now().is_ok_and(|n| n < invite.expires_at) {
            match listener.accept() {
                Ok((socket, _)) => {
                    // Windows accepted sockets inherit the listener's nonblocking
                    // flag; the bounded TLS transport expects blocking I/O with deadlines.
                    if socket.set_nonblocking(false).is_err() {
                        if let Ok(mut error) = errors.lock() { *error = Some("PAIRING_SOCKET_FAILED".into()); }
                        continue;
                    }
                    let scope = invite.scope();
                    let result = if confirmation {
                        server.serve_pairing_confirmation_rpc_once(socket, Limits::default(), &scope, |peer| {
                            with_app(&owner, |a| {
                                let stored = a.pairing_status(&scope.invitation_id, now()?).map_err(|_| "PAIRING_STORE_UNAVAILABLE")?;
                                if stored.state != "consumed" || stored.invitation.home_device_id != scope.home_id || stored.invitation.home_epoch != scope.home_epoch
                                    || stored.invitation.home_identity_sha256 != scope.home_pin || stored.device_id.as_deref() != Some(scope.candidate_id.as_str())
                                    || stored.identity_sha256.as_deref() != Some(scope.candidate_pin.as_str()) { return Err("PAIRING_CONFIRMATION_REQUIRED"); }
                                let ctx = a.authenticate_home_session(&scope.home_id, scope.home_epoch, peer.certificate_sha256(), &scope.session_id, now()?)
                                    .map_err(|_| "PAIRING_CONFIRMATION_REQUIRED")?;
                                if ctx.origin_device_id() != scope.candidate_id { return Err("PAIRING_CONFIRMATION_REQUIRED"); }
                                Ok(ctx.authorization_revision())
                            }).map_err(|_| TransportError::Authentication)
                        })
                    } else {
                        server.serve_pairing_rpc_once(socket, Limits::default(), &scope, |peer, challenge| {
                            with_app(&owner, |a| {
                                let stored = a.pairing_status(&scope.invitation_id, now()?).map_err(|_| "PAIRING_STORE_UNAVAILABLE")?;
                                if stored.state != "open" || stored.invitation.home_device_id != scope.home_id || stored.invitation.home_epoch != scope.home_epoch
                                    || stored.invitation.home_identity_sha256 != scope.home_pin { return Err("PAIRING_INVITATION_INVALID"); }
                                a.propose_pairing_peer(&scope.invitation_id, &challenge, &ProvenPairingPeer { device_id: scope.candidate_id.clone(),
                                    identity_sha256: peer.certificate_sha256().to_owned(), public_key_ref: format!("tls-cert-sha256:{}", peer.certificate_sha256()) }, now()?)
                                    .map_err(|_| "PAIRING_PROPOSAL_REJECTED")
                            }).map_err(|_| TransportError::Authentication)
                        })
                    };
                    if let Ok(mut error) = errors.lock() { *error = result.err().map(|_| "PAIRING_REQUEST_REJECTED".to_owned()); }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(50)),
                Err(_) => { if let Ok(mut e) = errors.lock() { *e = Some("PAIRING_LISTENER_FAILED".into()); } break; }
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub(crate) async fn app_pairing_home_invite<R: tauri::Runtime>(app: tauri::AppHandle<R>, state: State<'_, AppState>,
    pairing: State<'_, PairingState>, device_code: String, bind_address: String, advertised_host: String) -> Result<InvitationExport, &'static str> {
    let candidate: DeviceCode = decode(&device_code)?;
    if candidate.version != 1 || !valid_identity(&candidate.identity) || !id(&candidate.session_id) { return Err("PAIRING_CODE_INVALID"); }
    let identity = native(&app, &state).await?;
    if identity.public.device_id == candidate.identity.device_id { return Err("PAIRING_SELF_DENIED"); }
    with_app(&state.0, |a| if a.paired_home().map_err(|_| "LOCAL_STORE_UNAVAILABLE")?.is_none() { Ok(()) } else { Err("PAIRING_REPLICA_CANNOT_HOST") })?;
    let bind: SocketAddr = bind_address.parse().map_err(|_| "PAIRING_ADDRESS_INVALID")?;
    let advertised: IpAddr = advertised_host.parse().map_err(|_| "PAIRING_ADDRESS_INVALID")?;
    if !valid_address(SocketAddr::new(advertised, bind.port())) || bind.ip().is_multicast() || bind.is_ipv4() != advertised.is_ipv4() { return Err("PAIRING_ADDRESS_INVALID"); }
    let mut guard = pairing.0.lock().map_err(|_| "PAIRING_STATE_UNAVAILABLE")?;
    if guard.is_some() { return Err("PAIRING_ALREADY_ACTIVE"); }
    let proposal = TcpListener::bind(SocketAddr::new(bind.ip(), 0)).map_err(|_| "PAIRING_BIND_FAILED")?;
    let confirmation = TcpListener::bind(SocketAddr::new(bind.ip(), 0)).map_err(|_| "PAIRING_BIND_FAILED")?;
    let proposal_address = SocketAddr::new(advertised, proposal.local_addr().map_err(|_| "PAIRING_BIND_FAILED")?.port());
    let confirmation_address = SocketAddr::new(advertised, confirmation.local_addr().map_err(|_| "PAIRING_BIND_FAILED")?.port());
    let mut challenge = [0u8; 32]; SystemRandom::new().fill(&mut challenge).map_err(|_| "PAIRING_RANDOM_FAILED")?;
    let invitation_id = format!("invite-{:x}", Sha256::digest(challenge));
    let n = now()?;
    let invite = InvitationCode { version: 1, invitation_id: invitation_id.clone(), home: identity.public.clone(), home_epoch: 1,
        candidate, challenge, expires_at: n + INVITATION_MS, proposal_address, confirmation_address, control_address: SocketAddr::new(advertised, bind.port()) };
    let root = app.path().app_data_dir().map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    with_app(&state.0, |a| a.issue_pairing_invitation(&PairingInvitation { id: invitation_id.clone(), home_device_id: identity.public.device_id.clone(), home_epoch: 1,
        home_identity_sha256: identity.public.certificate_sha256.clone(), expires_at: invite.expires_at }, &challenge, n).map_err(|_| "PAIRING_INVITATION_FAILED"))?;
    let server = server(&identity, &invite.candidate.identity)?;
    let stop = Arc::new(AtomicBool::new(false));
    let errors = Arc::new(Mutex::new(None));
    let handle = ListenerStop(stop.clone());
    listen(proposal, server.clone(), invite.clone(), state.0.clone(), stop.clone(), errors.clone(), false)?;
    listen(confirmation, server, invite.clone(), state.0.clone(), stop, errors.clone(), true)?;
    let export = InvitationExport { code: serde_json::to_string(&invite).map_err(|_| "PAIRING_CODE_INVALID")?, invitation_id,
        candidate_id: invite.candidate.identity.device_id.clone(), candidate_pin: invite.candidate.identity.certificate_sha256.clone(),
        session_id: invite.candidate.session_id.clone(), expires_at: invite.expires_at, status: "open" };
    *guard = Some(Ceremony::Home(HomeCeremony { invitation: invite, bind, root, owner: state.0.clone(), _listeners: handle, error: errors, grant_expiry: None, host: None }));
    Ok(export)
}

#[tauri::command]
pub(crate) async fn app_pairing_status<R: tauri::Runtime>(app: tauri::AppHandle<R>, state: State<'_, AppState>, pairing: State<'_, PairingState>) -> Result<CeremonyStatus, &'static str> {
    let identity = device_identity::load_native(&app).await?;
    let mut guard = pairing.0.lock().map_err(|_| "PAIRING_STATE_UNAVAILABLE")?;
    if guard.is_none() {
        if let Some(identity) = identity {
            let (id, session) = local_scope(&state)?;
            if id != identity.public.device_id { return Err("IDENTITY_DEVICE_MISMATCH"); }
            let root = app.path().app_data_dir().map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
            if let Some(home) = load_host(&root, &identity.public, &state.0)? { *guard = Some(Ceremony::Home(home)); }
            else if let Some(config) = crate::home_config::load(&root, &identity.public, &session)? {
                let matched = with_app(&state.0, |a| Ok(a.paired_home().map_err(|_| "LOCAL_STORE_UNAVAILABLE")?
                    .is_some_and(|(id,epoch,pin)| id == config.home_device_id && epoch == config.home_epoch && pin == config.home_certificate_sha256)))?;
                return Ok(CeremonyStatus { role: "replica", state: if matched { "connected" } else { "recovery_required" }.into(),
                    candidate_id: Some(config.local_device_id), candidate_pin: Some(config.local_certificate_sha256), session_id: Some(config.session_id),
                    expires_at: None, error: None, home_pin: Some(config.home_certificate_sha256), grant_expires_at: None });
            }
        }
    }
    match guard.as_ref() { Some(c) => status(c), None => Ok(CeremonyStatus { role: "replica", state: "unconfigured".into(), candidate_id: None, candidate_pin: None, session_id: None, expires_at: None, error: None, home_pin: None, grant_expires_at: None }) }
}

#[tauri::command]
pub(crate) fn app_pairing_reset(pairing: State<'_, PairingState>) -> Result<(), &'static str> {
    let mut guard = pairing.0.lock().map_err(|_| "PAIRING_STATE_UNAVAILABLE")?;
    if let Some(Ceremony::Home(home)) = guard.as_ref() {
        if home.grant_expiry.is_some() || home.host.is_some() { return Err("PAIRING_ALREADY_CONFIRMED"); }
        with_app(&home.owner, |a| {
            let stored = a.pairing_status(&home.invitation.invitation_id, now()?).map_err(|_| "PAIRING_STORE_UNAVAILABLE")?;
            if stored.state == "open" || stored.state == "pending" { a.cancel_pairing_invitation(&home.invitation.invitation_id).map_err(|_| "PAIRING_CANCEL_FAILED")?; }
            Ok(())
        })?;
    }
    if matches!(guard.as_ref(), Some(Ceremony::Replica{state,..}) if state == "connected" || state == "proposing") { return Err("PAIRING_RESET_DENIED"); }
    *guard = None; // Stop provisional listeners; never delete durable pairing or authorization.
    Ok(())
}

#[tauri::command]
pub(crate) async fn app_pairing_recover<R: tauri::Runtime>(app: tauri::AppHandle<R>, state: State<'_, AppState>, link: State<'_, HomeLinkState>) -> Result<(), &'static str> {
    let identity = device_identity::load_native(&app).await?.ok_or("IDENTITY_UNAVAILABLE")?;
    let (device, session) = local_scope(&state)?;
    if device != identity.public.device_id { return Err("IDENTITY_DEVICE_MISMATCH"); }
    let root = app.path().app_data_dir().map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    let config = crate::home_config::load(&root, &identity.public, &session)?.ok_or("PAIRING_CONFIRMATION_REQUIRED")?;
    // This explicit recovery action finishes an interrupted local commit whose
    // immutable metadata was written only after authenticated remote confirmation.
    // It creates no Home permission; future RPCs recheck current Home grants.
    with_app(&state.0, |a| a.bind_confirmed_home(&config.home_device_id, config.home_epoch, &config.home_certificate_sha256, now()?).map_err(|_| "PAIRING_LOCAL_BIND_UNCONFIRMED"))?;
    *link.connection.lock().map_err(|_| "PAIRING_STATE_UNAVAILABLE")? = Some(Arc::new(config.connect(&identity)?));
    Ok(())
}

#[tauri::command]
pub(crate) async fn app_pairing_propose<R: tauri::Runtime>(app: tauri::AppHandle<R>, state: State<'_, AppState>,
    pairing: State<'_, PairingState>, invitation_code: String) -> Result<CeremonyStatus, &'static str> {
    let invite: InvitationCode = decode(&invitation_code)?;
    let identity = native(&app, &state).await?;
    let (_, session) = local_scope(&state)?;
    let n = now()?;
    if invite.version != 1 || invite.home_epoch < 1 || !id(&invite.invitation_id) || !valid_identity(&invite.home)
        || !valid_identity(&invite.candidate.identity) || invite.candidate.identity.device_id != identity.public.device_id
        || invite.candidate.identity.certificate_sha256 != identity.public.certificate_sha256 || invite.candidate.session_id != session
        || invite.home.device_id == identity.public.device_id || invite.expires_at <= n || invite.expires_at > n + INVITATION_MS
        || ![invite.proposal_address, invite.confirmation_address, invite.control_address].into_iter().all(valid_address) { return Err("PAIRING_CODE_INVALID"); }
    let client = client(&identity, &invite)?;
    let scope = invite.scope(); let name = ServerName::try_from(invite.home.tls_server_name.clone()).map_err(|_| "PAIRING_CODE_INVALID")?;
    {
        let mut guard = pairing.0.lock().map_err(|_| "PAIRING_STATE_UNAVAILABLE")?;
        if guard.is_some() { return Err("PAIRING_ALREADY_ACTIVE"); }
        // Remember exact attempt BEFORE network: ambiguous delivery cannot cause an automatic reproposal.
        *guard = Some(Ceremony::Replica { invitation: invite.clone(), state: "proposing".into() });
    }
    let result = tauri::async_runtime::spawn_blocking(move || client.propose_pairing(invite.proposal_address, name, &scope, invite.challenge, Limits::default())).await.map_err(|_| "PAIRING_REQUEST_FAILED")?;
    let mut guard = pairing.0.lock().map_err(|_| "PAIRING_STATE_UNAVAILABLE")?;
    if let Some(Ceremony::Replica { state, .. }) = guard.as_mut() { *state = if result.is_ok() { "pending" } else { "unknown" }.into(); }
    result.map_err(|_| "PAIRING_PROPOSAL_UNCONFIRMED")?;
    status(guard.as_ref().ok_or("PAIRING_STATE_UNAVAILABLE")?)
}

#[tauri::command]
pub(crate) fn app_pairing_home_confirm(pairing: State<'_, PairingState>) -> Result<CeremonyStatus, &'static str> {
    let mut guard = pairing.0.lock().map_err(|_| "PAIRING_STATE_UNAVAILABLE")?;
    let Some(Ceremony::Home(home)) = guard.as_mut() else { return Err("PAIRING_HOME_REQUIRED"); };
    let n = now()?; let expires = n + GRANT_MS;
    with_app(&home.owner, |a| a.confirm_pairing_peer(&home.invitation.invitation_id, &home.invitation.candidate.identity.device_id,
        &home.invitation.candidate.identity.certificate_sha256, &home.invitation.candidate.session_id, expires, true, n).map_err(|_| "PAIRING_CONFIRMATION_REJECTED"))?;
    home.grant_expiry = Some(expires);
    save_host(home)?;
    status(guard.as_ref().ok_or("PAIRING_STATE_UNAVAILABLE")?)
}

#[tauri::command]
pub(crate) async fn app_pairing_finish<R: tauri::Runtime>(app: tauri::AppHandle<R>, state: State<'_, AppState>,
    pairing: State<'_, PairingState>, link: State<'_, HomeLinkState>) -> Result<CeremonyStatus, &'static str> {
    let invite = {
        let guard = pairing.0.lock().map_err(|_| "PAIRING_STATE_UNAVAILABLE")?;
        match guard.as_ref() { Some(Ceremony::Replica { invitation, state }) if state == "pending" || state == "unknown" => invitation.clone(), _ => return Err("PAIRING_REPLICA_REQUIRED") }
    };
    let identity = native(&app, &state).await?; let (_, session) = local_scope(&state)?;
    let client = client(&identity, &invite)?; let scope = invite.scope();
    let name = ServerName::try_from(invite.home.tls_server_name.clone()).map_err(|_| "PAIRING_CODE_INVALID")?;
    let address = invite.confirmation_address;
    let proof = tauri::async_runtime::spawn_blocking(move || client.confirm_pairing(address, name, &scope, Limits::default())).await
        .map_err(|_| "PAIRING_CONFIRMATION_UNAVAILABLE")?.map_err(|_| "PAIRING_CONFIRMATION_UNAVAILABLE")?;
    let config = HomeConfig { version: 1, invitation_id: invite.invitation_id.clone(), home_device_id: invite.home.device_id.clone(), home_epoch: invite.home_epoch,
        home_certificate_der: invite.home.certificate_der.clone(), home_certificate_sha256: invite.home.certificate_sha256.clone(), tls_server_name: invite.home.tls_server_name.clone(),
        address: invite.control_address, local_device_id: identity.public.device_id.clone(), local_certificate_sha256: identity.public.certificate_sha256.clone(),
        session_id: session.clone(), authorization_revision: proof.authorization_revision() };
    let root = app.path().app_data_dir().map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    let mut pairing_guard = pairing.0.lock().map_err(|_| "PAIRING_STATE_UNAVAILABLE")?;
    // Public metadata is committed first; a crash before the kernel bind is fail-closed on restore.
    crate::home_config::persist_after_local_confirmation(&root, &config, &identity.public, &session, &proof)?;
    with_app(&state.0, |a| a.bind_confirmed_home(&config.home_device_id, config.home_epoch, &config.home_certificate_sha256, now()?).map_err(|_| "PAIRING_LOCAL_BIND_UNCONFIRMED"))?;
    *link.connection.lock().map_err(|_| "PAIRING_STATE_UNAVAILABLE")? = Some(Arc::new(config.connect(&identity)?));
    if let Some(Ceremony::Replica { state, .. }) = pairing_guard.as_mut() { *state = "connected".into(); }
    status(pairing_guard.as_ref().ok_or("PAIRING_STATE_UNAVAILABLE")?)
}

#[tauri::command]
pub(crate) async fn app_pairing_enable_home<R: tauri::Runtime>(app: tauri::AppHandle<R>, state: State<'_, AppState>,
    runner: State<'_, RunnerState>, pairing: State<'_, PairingState>) -> Result<CeremonyStatus, &'static str> {
    let identity = native(&app, &state).await?;
    let mut guard = pairing.0.lock().map_err(|_| "PAIRING_STATE_UNAVAILABLE")?;
    if guard.is_none() {
        let root = app.path().app_data_dir().map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
        *guard = load_host(&root, &identity.public, &state.0)?.map(Ceremony::Home);
    }
    let Some(Ceremony::Home(home)) = guard.as_mut() else { return Err("PAIRING_HOME_REQUIRED"); };
    if home.host.as_ref().is_some_and(|h| h.is_running()) { return status(guard.as_ref().unwrap()); }
    let expiry = home.grant_expiry.ok_or("PAIRING_CONFIRMATION_REQUIRED")?;
    if expiry <= now()? { return Err("PAIRING_GRANT_EXPIRED"); }
    let config = crate::home_host::HomeHostConfig { address: home.bind, home_device_id: identity.public.device_id.clone(), home_epoch: home.invitation.home_epoch,
        server: server(&identity, &home.invitation.candidate.identity)?, program: runner.program.clone().ok_or("EXECUTION_UNAVAILABLE")?,
        receipts: app.path().app_data_dir().map_err(|_| "LOCAL_STORE_UNAVAILABLE")?.join("reply-receipts"),
        policy: crate::home_host::ReadOnlyPolicy { device_id: home.invitation.candidate.identity.device_id.clone(), session_id: home.invitation.candidate.session_id.clone(),
            expires_at: expiry, execution_timeout: Duration::from_secs(120) } };
    home.host = Some(crate::home_host::start(config, state.0.clone(), runner.active.clone())?);
    status(guard.as_ref().ok_or("PAIRING_STATE_UNAVAILABLE")?)
}

/// Restore only an already committed exact kernel/vault/config combination. No new grant or authority change.
pub(crate) async fn restore<R: tauri::Runtime>(app: &tauri::AppHandle<R>, state: &AppState, link: &HomeLinkState) -> Result<bool, &'static str> {
    let Some(identity) = device_identity::load_native(app).await? else { return Ok(false); };
    let (device, session) = local_scope(state)?;
    if device != identity.public.device_id { return Err("IDENTITY_DEVICE_MISMATCH"); }
    let root = app.path().app_data_dir().map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    let Some(config) = crate::home_config::load(&root, &identity.public, &session)? else { return Ok(false); };
    with_app(&state.0, |a| match a.paired_home().map_err(|_| "LOCAL_STORE_UNAVAILABLE")? {
        Some((id, epoch, pin)) if id == config.home_device_id && epoch == config.home_epoch && pin == config.home_certificate_sha256 => Ok(()),
        _ => Err("PAIRING_LOCAL_BIND_UNCONFIRMED"),
    })?;
    *link.connection.lock().map_err(|_| "PAIRING_STATE_UNAVAILABLE")? = Some(Arc::new(config.connect(&identity)?));
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn untrusted_codes_are_bounded_and_addresses_are_explicit() {
        assert!(decode::<DeviceCode>(&"x".repeat(CODE_LIMIT + 1)).is_err());
        assert!(decode::<DeviceCode>("{}").is_err());
        assert!(!valid_address("0.0.0.0:42".parse().unwrap()));
        assert!(!valid_address("255.255.255.255:42".parse().unwrap()));
        assert!(!valid_address("127.0.0.1:0".parse().unwrap()));
        assert!(valid_address("127.0.0.1:42".parse().unwrap()));
    }

    #[cfg(windows)]
    #[test]
    fn native_pairing_commands_share_live_owner_and_restore_without_model() {
        use tauri::test::{mock_builder, mock_context, noop_assets};
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let make = |role: &str| {
            let mut context = mock_context(noop_assets());
            context.config_mut().identifier = format!("org.bobagent.test.pairing.{stamp}.{role}");
            let app = crate::configure(mock_builder()).build(context).unwrap();
            tauri::async_runtime::block_on(crate::app_bootstrap(app.handle().clone(), app.state())).unwrap();
            app
        };
        let home = make("home"); let phone = make("phone");
        let home_root = home.path().app_data_dir().unwrap();
        let phone_root = phone.path().app_data_dir().unwrap();
        let phone_code = tauri::async_runtime::block_on(app_pairing_device_code(phone.handle().clone(), phone.state())).unwrap();
        assert!(!phone_code.code.contains("private"));
        let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
        let bind = reservation.local_addr().unwrap(); drop(reservation);
        let invitation = tauri::async_runtime::block_on(app_pairing_home_invite(home.handle().clone(), home.state(), home.state(), phone_code.code,
            bind.to_string(), bind.ip().to_string())).unwrap();
        assert_eq!(invitation.status, "open");
        let pending = tauri::async_runtime::block_on(app_pairing_propose(phone.handle().clone(), phone.state(), phone.state(), invitation.code)).unwrap();
        assert_eq!(pending.state, "pending");
        // Home LocalApp continues to own its database across all actual commands.
        assert_eq!(tauri::async_runtime::block_on(app_pairing_status(home.handle().clone(), home.state(), home.state())).unwrap().state, "pending");
        let before = tauri::async_runtime::block_on(app_pairing_finish(phone.handle().clone(), phone.state(), phone.state(), phone.state()));
        assert_eq!(before.err(), Some("PAIRING_CONFIRMATION_UNAVAILABLE"));
        assert_eq!(app_pairing_home_confirm(home.state()).unwrap().state, "confirmed");
        assert_eq!(tauri::async_runtime::block_on(app_pairing_finish(phone.handle().clone(), phone.state(), phone.state(), phone.state())).unwrap().state, "connected");
        assert!(with_app(&phone.state::<AppState>().0, |a| Ok(a.paired_home().unwrap().is_some())).unwrap());
        assert!(app_pairing_reset(phone.state()).is_err());
        *phone.state::<HomeLinkState>().connection.lock().unwrap() = None;
        assert!(tauri::async_runtime::block_on(restore(phone.handle(), &phone.state::<AppState>(), &phone.state::<HomeLinkState>())).unwrap());
        // Recreate ceremony state from public metadata; no listener, runner, or newgrant is started.
        *home.state::<PairingState>().0.lock().unwrap() = None;
        let restored = tauri::async_runtime::block_on(app_pairing_status(home.handle().clone(), home.state(), home.state())).unwrap();
        assert_eq!(restored.state, "confirmed");
        assert!(matches!(home.state::<PairingState>().0.lock().unwrap().as_ref(), Some(Ceremony::Home(h)) if h.host.is_none()));
        *home.state::<PairingState>().0.lock().unwrap() = None;
        *phone.state::<PairingState>().0.lock().unwrap() = None;
        *home.state::<AppState>().0.lock().unwrap() = None;
        *phone.state::<AppState>().0.lock().unwrap() = None;
        drop(home); drop(phone);
        std::thread::sleep(Duration::from_millis(100));
        for root in [home_root, phone_root] {
            assert!(root.file_name().unwrap().to_str().unwrap().starts_with("org.bobagent.test.pairing."));
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}
