//! QR bootstrap discovers a signed device identity; local confirmation remains
//! mandatory before the existing exact-peer mutual TLS path can grant access.
use super::*;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use bob_transport::bootstrap::{
    BootstrapClient, BootstrapEndpoints, BootstrapScope, BootstrapServer,
};
const QR_LIMIT: usize = 2200;
const PREFIX: &str = "bob-pair:1:";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct QrCode {
    v: u8,
    pub i: String,
    h: String,
    e: i64,
    c: String,
    n: String,
    k: String,
    x: i64,
    a: Vec<SocketAddr>,
    f: u16,
    w: u16,
}
impl QrCode {
    fn home(&self) -> Result<PublicIdentity, &'static str> {
        let cert = URL_SAFE_NO_PAD
            .decode(&self.c)
            .map_err(|_| "PAIRING_CODE_INVALID")?;
        if cert.is_empty() || cert.len() > 4096 {
            return Err("PAIRING_CODE_INVALID");
        }
        let identity = PublicIdentity {
            device_id: self.h.clone(),
            certificate_sha256: format!("{:x}", Sha256::digest(&cert)),
            certificate_der: cert,
            tls_server_name: self.n.clone(),
        };
        if !valid_identity(&identity) {
            return Err("PAIRING_CODE_INVALID");
        }
        Ok(identity)
    }
    fn challenge(&self) -> Result<[u8; 32], &'static str> {
        URL_SAFE_NO_PAD
            .decode(&self.k)
            .map_err(|_| "PAIRING_CODE_INVALID")?
            .try_into()
            .map_err(|_| "PAIRING_CODE_INVALID")
    }
    fn scope(&self) -> Result<BootstrapScope, &'static str> {
        Ok(BootstrapScope {
            invitation_id: self.i.clone(),
            home_id: self.h.clone(),
            home_epoch: self.e,
            home_pin: self.home()?.certificate_sha256,
            expires_at: self.x,
        })
    }
    fn encode(&self) -> Result<String, &'static str> {
        let code = format!(
            "{PREFIX}{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(self).map_err(|_| "PAIRING_CODE_INVALID")?)
        );
        if code.len() > QR_LIMIT {
            return Err("PAIRING_CODE_TOO_LARGE");
        }
        Ok(code)
    }
    fn decode(code: &str, at: i64) -> Result<Self, &'static str> {
        if code.len() > QR_LIMIT {
            return Err("PAIRING_CODE_INVALID");
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(code.strip_prefix(PREFIX).ok_or("PAIRING_CODE_INVALID")?)
            .map_err(|_| "PAIRING_CODE_INVALID")?;
        let value: Self = serde_json::from_slice(&bytes).map_err(|_| "PAIRING_CODE_INVALID")?;
        if value.v != 1
            || !id(&value.i)
            || !id(&value.h)
            || value.e < 1
            || value.x <= at
            || value.x > at + INVITATION_MS
            || value.a.is_empty()
            || value.a.len() > 4
            || value.f == 0
            || value.w == 0
            || !value
                .a
                .iter()
                .all(|a| valid_address(*a) && allowed_ip(a.ip()))
        {
            return Err("PAIRING_CODE_INVALID");
        }
        value.home()?;
        value.challenge()?;
        Ok(value)
    }
}
pub(super) struct OpenHome {
    pub code: QrCode,
    pub owner: Arc<Mutex<Option<LocalApp>>>,
    pub stop: Arc<AtomicBool>,
    errors: Arc<Mutex<Option<String>>>,
}
pub(super) fn open_status(home: &OpenHome) -> Result<CeremonyStatus, &'static str> {
    Ok(CeremonyStatus {
        role: "home",
        state: if now()? >= home.code.x {
            "expired"
        } else {
            "open"
        }
        .into(),
        candidate_id: None,
        candidate_pin: None,
        session_id: None,
        expires_at: Some(home.code.x),
        error: home
            .errors
            .lock()
            .map_err(|_| "PAIRING_STATE_UNAVAILABLE")?
            .clone(),
        home_pin: Some(home.code.home()?.certificate_sha256),
        grant_expires_at: None,
    })
}
#[derive(Serialize)]
pub(crate) struct Capabilities {
    can_host: bool,
    can_scan: bool,
    start_pairing: bool,
}
pub(crate) fn app_pairing_capabilities() -> Capabilities {
    Capabilities {
        can_host: cfg!(windows),
        can_scan: cfg!(target_os = "android"),
        start_pairing: cfg!(windows) && std::env::args().any(|a| a == "--pair-phone"),
    }
}
#[derive(Serialize)]
pub(crate) struct QrExport {
    pub code: String,
    pub expires_at: i64,
    pub state: &'static str,
    pub home_name: String,
    pub addresses: Vec<String>,
}
fn export(code: &QrCode) -> Result<QrExport, &'static str> {
    Ok(QrExport {
        code: code.encode()?,
        expires_at: code.x,
        state: "open",
        home_name: std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Bob Home".into()),
        addresses: code.a.iter().map(ToString::to_string).collect(),
    })
}
fn existing_open(ceremony: &Option<Ceremony>) -> Result<Option<QrExport>, &'static str> {
    match ceremony {
        None => Ok(None),
        Some(Ceremony::OpenHome(open))
            if open.code.x > now()? && !open.stop.load(Ordering::SeqCst) =>
        {
            export(&open.code).map(Some)
        }
        Some(_) => Err("PAIRING_ALREADY_ACTIVE"),
    }
}
fn allowed_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            v.is_private()
                || (v.octets()[0] == 100 && (64..=127).contains(&v.octets()[1]))
                || (cfg!(test) && v.is_loopback())
        }
        IpAddr::V6(v) => v.is_unique_local() || (cfg!(test) && v.is_loopback()),
    }
}
/// Read OS interface configuration only. No network probes, firewall edits or
/// personal addresses are embedded in the application.
fn local_addresses() -> Result<Vec<IpAddr>, &'static str> {
    if let Ok(value) = std::env::var("BOB_HOME_ADVERTISE_IP") {
        let ip = value.parse().map_err(|_| "PAIRING_ADDRESS_INVALID")?;
        if allowed_ip(ip) {
            return Ok(vec![ip]);
        }
        return Err("PAIRING_ADDRESS_INVALID");
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let script="[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new(); $primary=@(Get-NetIPConfiguration | Where-Object {$_.IPv4DefaultGateway} | ForEach-Object {$_.IPv4Address.IPAddress}); $all=@(Get-NetIPAddress -AddressFamily IPv4 -AddressState Preferred | Select-Object -ExpandProperty IPAddress); ConvertTo-Json -Compress -InputObject @($primary+$all | Select-Object -Unique)";
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .creation_flags(0x08000000)
            .output()
            .map_err(|_| "PAIRING_NETWORK_UNAVAILABLE")?;
        if !output.status.success() || output.stdout.len() > 16384 {
            return Err("PAIRING_NETWORK_UNAVAILABLE");
        }
        let values: Vec<String> =
            serde_json::from_slice(&output.stdout).map_err(|_| "PAIRING_NETWORK_UNAVAILABLE")?;
        let mut ips = Vec::new();
        for value in values {
            if let Ok(ip) = value.parse() {
                if allowed_ip(ip) && !ips.contains(&ip) {
                    ips.push(ip);
                }
            }
        }
        ips.truncate(4);
        if !ips.is_empty() {
            return Ok(ips);
        }
    }
    Err("PAIRING_NETWORK_UNAVAILABLE")
}
pub(crate) async fn app_pairing_qr_create<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
    pairing: State<'_, PairingState>,
) -> Result<QrExport, &'static str> {
    if !cfg!(windows) {
        return Err("PAIRING_HOME_UNSUPPORTED");
    }
    {
        let guard = pairing.0.lock().map_err(|_| "PAIRING_STATE_UNAVAILABLE")?;
        if let Some(code) = existing_open(&guard)? {
            return Ok(code);
        }
    }
    let addresses = tauri::async_runtime::spawn_blocking(local_addresses)
        .await
        .map_err(|_| "PAIRING_NETWORK_UNAVAILABLE")??;
    create_at(&app, &state, &pairing, addresses).await
}
async fn create_at<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &AppState,
    pairing: &PairingState,
    addresses: Vec<IpAddr>,
) -> Result<QrExport, &'static str> {
    if addresses.is_empty() || addresses.len() > 4 || !addresses.iter().all(|a| allowed_ip(*a)) {
        return Err("PAIRING_ADDRESS_INVALID");
    }
    let identity = native(app, state).await?;
    with_app(&state.0, |a| {
        if a.paired_home()
            .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?
            .is_none()
        {
            Ok(())
        } else {
            Err("PAIRING_REPLICA_CANNOT_HOST")
        }
    })?;
    let mut guard = pairing.0.lock().map_err(|_| "PAIRING_STATE_UNAVAILABLE")?;
    if let Some(code) = existing_open(&guard)? {
        return Ok(code);
    }
    let bind_ip = if addresses[0].is_ipv4() {
        IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)
    } else {
        IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED)
    };
    if addresses.iter().any(|a| a.is_ipv4() != bind_ip.is_ipv4()) {
        return Err("PAIRING_ADDRESS_INVALID");
    }
    let bootstrap =
        TcpListener::bind(SocketAddr::new(bind_ip, 0)).map_err(|_| "PAIRING_BIND_FAILED")?;
    let confirmation =
        TcpListener::bind(SocketAddr::new(bind_ip, 0)).map_err(|_| "PAIRING_BIND_FAILED")?;
    let control =
        TcpListener::bind(SocketAddr::new(bind_ip, 0)).map_err(|_| "PAIRING_BIND_FAILED")?;
    let b = bootstrap
        .local_addr()
        .map_err(|_| "PAIRING_BIND_FAILED")?
        .port();
    let f = confirmation
        .local_addr()
        .map_err(|_| "PAIRING_BIND_FAILED")?
        .port();
    let w = control
        .local_addr()
        .map_err(|_| "PAIRING_BIND_FAILED")?
        .port();
    let mut challenge = [0; 32];
    SystemRandom::new()
        .fill(&mut challenge)
        .map_err(|_| "PAIRING_RANDOM_FAILED")?;
    let n = now()?;
    let code = QrCode {
        v: 1,
        i: format!("invite-{:x}", Sha256::digest(challenge)),
        h: identity.public.device_id.clone(),
        e: 1,
        c: URL_SAFE_NO_PAD.encode(&identity.public.certificate_der),
        n: identity.public.tls_server_name.clone(),
        k: URL_SAFE_NO_PAD.encode(challenge),
        x: n + INVITATION_MS,
        a: addresses.iter().map(|ip| SocketAddr::new(*ip, b)).collect(),
        f,
        w,
    };
    code.encode()?; // Bound the actual invitation before publishing listeners.
    let root = app
        .path()
        .app_data_dir()
        .map_err(|_| "LOCAL_STORE_UNAVAILABLE")?;
    with_app(&state.0, |a| {
        a.issue_pairing_invitation(
            &PairingInvitation {
                id: code.i.clone(),
                home_device_id: code.h.clone(),
                home_epoch: code.e,
                home_identity_sha256: identity.public.certificate_sha256.clone(),
                expires_at: code.x,
            },
            &challenge,
            n,
        )
        .map_err(|_| "PAIRING_INVITATION_FAILED")
    })?;
    let server = BootstrapServer::new(tls(&identity)).map_err(|_| "PAIRING_IDENTITY_INVALID")?;
    bootstrap
        .set_nonblocking(true)
        .map_err(|_| "PAIRING_LISTENER_FAILED")?;
    let stop = Arc::new(AtomicBool::new(false));
    let errors = Arc::new(Mutex::new(None));
    *guard = Some(Ceremony::OpenHome(OpenHome {
        code: code.clone(),
        owner: state.0.clone(),
        stop: stop.clone(),
        errors: errors.clone(),
    }));
    drop(guard);
    let shared = pairing.0.clone();
    let owner = state.0.clone();
    let issued = code.clone();
    std::thread::Builder::new().name("bob-pair-bootstrap".into()).spawn(move||{
        let mut confirmation=Some(confirmation);let mut control=Some(control);
        while !stop.load(Ordering::SeqCst)&&now().is_ok_and(|n|n<issued.x){
            match bootstrap.accept(){
                Ok((socket,_))=>{
                    if socket.set_nonblocking(false).is_err(){continue;}
                    let local_ip=match socket.local_addr(){Ok(v)=>v.ip(),Err(_)=>continue};
                    let scope=match issued.scope(){Ok(v)=>v,Err(_)=>break};
                    let result=server.serve_once(socket,Limits::default(),&scope,||now().unwrap_or(i64::MAX),|proof,provided|{
                        let mut state=shared.lock().map_err(|_|TransportError::Protocol)?;
                        if !matches!(state.as_ref(),Some(Ceremony::OpenHome(h)) if h.code.i==issued.i&&!h.stop.load(Ordering::SeqCst)){return Err(TransportError::Authentication);}
                        let peer=PublicIdentity{device_id:proof.device_id().to_owned(),certificate_sha256:proof.certificate_sha256().to_owned(),certificate_der:proof.certificate_der().to_vec(),tls_server_name:"bob-device.invalid".into()};
                        let invite=InvitationCode{version:1,invitation_id:issued.i.clone(),home:identity.public.clone(),home_epoch:issued.e,candidate:DeviceCode{version:1,identity:peer.clone(),session_id:proof.session_id().to_owned()},challenge,expires_at:issued.x,proposal_address:SocketAddr::new(local_ip,b),confirmation_address:SocketAddr::new(local_ip,f),control_address:SocketAddr::new(local_ip,w)};
                        let pinned=super::server(&identity,&peer).map_err(|_|TransportError::Authentication)?;
                        with_app(&owner,|a|a.propose_pairing_peer(&issued.i,&provided,&ProvenPairingPeer{device_id:peer.device_id.clone(),identity_sha256:peer.certificate_sha256.clone(),public_key_ref:format!("tls-cert-sha256:{}",peer.certificate_sha256)},now()?).map_err(|_|"PAIRING_PROPOSAL_REJECTED")).map_err(|_|TransportError::Authentication)?;
                        let listener=confirmation.take().ok_or(TransportError::Protocol)?;
                        listen(listener,pinned,invite.clone(),owner.clone(),stop.clone(),errors.clone(),true).map_err(|_|TransportError::Protocol)?;
                        *state=Some(Ceremony::Home(HomeCeremony{invitation:invite.clone(),bind:SocketAddr::new(bind_ip,w),root:root.clone(),owner:owner.clone(),_listeners:ListenerStop(stop.clone()),error:errors.clone(),grant_expiry:None,host:None,control_reservation:control.take()}));
                        Ok(BootstrapEndpoints{confirmation_address:invite.confirmation_address,control_address:invite.control_address})
                    });
                    if let Ok(mut error)=errors.lock(){*error=result.err().map(|_|"PAIRING_REQUEST_REJECTED".into());}
                    if shared.lock().is_ok_and(|s|matches!(s.as_ref(),Some(Ceremony::Home(_)))){break;}
                }
                Err(e) if e.kind()==std::io::ErrorKind::WouldBlock=>std::thread::sleep(Duration::from_millis(25)),
                Err(_)=>break,
            }
        }
    }).map_err(|_|"PAIRING_LISTENER_FAILED")?;
    export(&code)
}
pub(crate) async fn app_pairing_qr_propose<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
    pairing: State<'_, PairingState>,
    invitation_code: String,
) -> Result<CeremonyStatus, &'static str> {
    let code = QrCode::decode(&invitation_code, now()?)?;
    let identity = native(&app, &state).await?;
    let (_, session) = local_scope(&state)?;
    let home = code.home()?;
    if home.device_id == identity.public.device_id {
        return Err("PAIRING_SELF_DENIED");
    }
    let candidate = DeviceCode {
        version: 1,
        identity: identity.public.clone(),
        session_id: session.clone(),
    };
    let client = BootstrapClient::new(
        tls(&identity),
        vec![home.certificate_der.clone().into()],
        home.certificate_sha256.clone(),
    )
    .map_err(|_| "PAIRING_IDENTITY_INVALID")?;
    let scope = code.scope()?;
    let challenge = code.challenge()?;
    let name =
        ServerName::try_from(home.tls_server_name.clone()).map_err(|_| "PAIRING_CODE_INVALID")?;
    let first = code.a[0];
    let invite = InvitationCode {
        version: 1,
        invitation_id: code.i.clone(),
        home,
        home_epoch: code.e,
        candidate,
        challenge,
        expires_at: code.x,
        proposal_address: first,
        confirmation_address: SocketAddr::new(first.ip(), code.f),
        control_address: SocketAddr::new(first.ip(), code.w),
    };
    let attempt = pairing_attempt_id()?;
    let invitation_id = invite.invitation_id.clone();
    {
        let mut guard = pairing.0.lock().map_err(|_| "PAIRING_STATE_UNAVAILABLE")?;
        if guard.is_some() {
            return Err("PAIRING_ALREADY_ACTIVE");
        }
        *guard = Some(Ceremony::Replica {
            attempt_id: attempt.clone(),
            invitation: invite,
            state: "proposing".into(),
        });
    }
    let shared = pairing.0.clone();
    let device = identity.public.device_id;
    let operation_attempt = attempt.clone();
    let operation_invitation = invitation_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        for address in code.a {
            {
                let mut guard = shared.lock().map_err(|_| "PAIRING_STATE_UNAVAILABLE")?;
                require_replica_attempt(
                    &guard,
                    &operation_attempt,
                    &operation_invitation,
                    &["proposing"],
                )?;
                if let Some(Ceremony::Replica { invitation, .. }) = guard.as_mut() {
                    invitation.proposal_address = address;
                    invitation.confirmation_address = SocketAddr::new(address.ip(), code.f);
                    invitation.control_address = SocketAddr::new(address.ip(), code.w);
                }
            }
            match client.propose(
                address,
                name.clone(),
                &scope,
                challenge,
                &device,
                &session,
                Limits::default(),
            ) {
                Ok(proof) => {
                    if proof.endpoints.confirmation_address != SocketAddr::new(address.ip(), code.f)
                        || proof.endpoints.control_address != SocketAddr::new(address.ip(), code.w)
                    {
                        return Err("PAIRING_ENDPOINT_CHANGED");
                    }
                    return Ok(());
                }
                Err(failure) if !failure.may_have_been_sent => continue,
                Err(_) => return Err("PAIRING_PROPOSAL_UNCONFIRMED"),
            }
        }
        Err("PAIRING_HOME_UNREACHABLE")
    })
    .await
    .map_err(|_| "PAIRING_REQUEST_FAILED")?;
    let mut guard = pairing.0.lock().map_err(|_| "PAIRING_STATE_UNAVAILABLE")?;
    proposal_outcome(&mut guard, &attempt, &invitation_id, result)
}
fn proposal_outcome(
    guard: &mut Option<Ceremony>,
    attempt: &str,
    invitation_id: &str,
    result: Result<(), &'static str>,
) -> Result<CeremonyStatus, &'static str> {
    require_replica_attempt(guard, attempt, invitation_id, &["proposing"])?;
    if result == Err("PAIRING_HOME_UNREACHABLE") {
        // Every endpoint failed before an application write. Do not strand a
        // known-unsent proposal in the ambiguous acknowledgement-recovery state.
        *guard = None;
        return Err("PAIRING_HOME_UNREACHABLE");
    }
    if let Some(Ceremony::Replica { state, .. }) = guard.as_mut() {
        *state = if result.is_ok() { "pending" } else { "unknown" }.into();
    }
    result?;
    status(guard.as_ref().ok_or("PAIRING_STATE_UNAVAILABLE")?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn qr_input_is_bounded_private_and_expiring() {
        assert!(QrCode::decode(&"x".repeat(QR_LIMIT + 1), 1).is_err());
        assert!(QrCode::decode("bob-pair:1:e30", 1).is_err());
        assert!(!allowed_ip("8.8.8.8".parse().unwrap()));
    }
    #[cfg(windows)]
    #[test]
    fn native_qr_commands_resume_confirmation_after_home_restart() {
        use tauri::test::{mock_builder, mock_context, noop_assets};
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let make = |role: &str| {
            let mut ctx = mock_context(noop_assets());
            ctx.config_mut().identifier = format!("org.bobagent.test.qr.{stamp}.{role}");
            let app = crate::configure(mock_builder()).build(ctx).unwrap();
            tauri::async_runtime::block_on(crate::app_bootstrap(app.handle().clone(), app.state()))
                .unwrap();
            app
        };
        let home = make("home");
        let phone = make("phone");
        let roots = [
            home.path().app_data_dir().unwrap(),
            phone.path().app_data_dir().unwrap(),
        ];
        let code = tauri::async_runtime::block_on(create_at(
            home.handle(),
            &home.state::<AppState>(),
            &home.state::<PairingState>(),
            vec![std::net::Ipv4Addr::LOCALHOST.into()],
        ))
        .unwrap();
        assert!(code.code.len() < 2000);
        let again = tauri::async_runtime::block_on(create_at(
            home.handle(),
            &home.state::<AppState>(),
            &home.state::<PairingState>(),
            vec![std::net::Ipv4Addr::LOCALHOST.into()],
        ))
        .unwrap();
        assert_eq!(again.code, code.code);
        assert_eq!(again.expires_at, code.expires_at);
        assert_eq!(again.addresses, code.addresses);
        if let Some(path) = std::env::var_os("BOB_QR_TEST_CODE") {
            std::fs::write(path, &code.code).unwrap();
        }

        assert_eq!(
            status(
                home.state::<PairingState>()
                    .0
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
            )
            .unwrap()
            .state,
            "open"
        );
        let mut unreachable = QrCode::decode(&code.code, now().unwrap()).unwrap();
        let closed = TcpListener::bind("127.0.0.1:0").unwrap();
        unreachable.a = vec![closed.local_addr().unwrap()];
        drop(closed);
        assert_eq!(
            tauri::async_runtime::block_on(app_pairing_qr_propose(
                phone.handle().clone(),
                phone.state(),
                phone.state(),
                unreachable.encode().unwrap()
            ))
            .err(),
            Some("PAIRING_HOME_UNREACHABLE")
        );
        assert!(phone.state::<PairingState>().0.lock().unwrap().is_none());
        let proposed = tauri::async_runtime::block_on(app_pairing_qr_propose(
            phone.handle().clone(),
            phone.state(),
            phone.state(),
            code.code,
        ))
        .unwrap();
        assert_eq!(proposed.state, "pending");
        {
            let state = phone.state::<PairingState>();
            let guard = state.0.lock().unwrap();
            let Some(Ceremony::Replica { invitation, .. }) = guard.as_ref() else {
                panic!("replica");
            };
            let mut ambiguous = Some(Ceremony::Replica {
                invitation: invitation.clone(),
                attempt_id: "uncertain-attempt".into(),
                state: "proposing".into(),
            });
            assert_eq!(
                proposal_outcome(
                    &mut ambiguous,
                    "uncertain-attempt",
                    &invitation.invitation_id,
                    Err("PAIRING_PROPOSAL_UNCONFIRMED")
                )
                .err(),
                Some("PAIRING_PROPOSAL_UNCONFIRMED")
            );
            assert!(
                matches!(ambiguous, Some(Ceremony::Replica { state, .. }) if state == "unknown")
            );
        }
        let pending = status(
            home.state::<PairingState>()
                .0
                .lock()
                .unwrap()
                .as_ref()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(pending.state, "pending");
        assert_eq!(pending.candidate_pin, proposed.candidate_pin);
        assert!(tauri::async_runtime::block_on(app_pairing_finish(
            phone.handle().clone(),
            phone.state(),
            phone.state(),
            phone.state()
        ))
        .is_err());
        // A metadata save failure must leave the exact proposal unconsumed.
        {
            let state = home.state::<PairingState>();
            let mut guard = state.0.lock().unwrap();
            let Some(Ceremony::Home(h)) = guard.as_mut() else {
                panic!("home");
            };
            h.root = roots[0].join("nonexistent-save-parent");
        }
        assert_eq!(
            app_pairing_home_confirm(home.state()).err(),
            Some("PAIRING_HOST_SAVE_UNCONFIRMED")
        );
        {
            let state = home.state::<PairingState>();
            let mut guard = state.0.lock().unwrap();
            let Some(Ceremony::Home(h)) = guard.as_mut() else {
                panic!("home");
            };
            assert_eq!(
                with_app(&h.owner, |a| Ok(a
                    .pairing_status(&h.invitation.invitation_id, now().unwrap())
                    .unwrap()))
                .unwrap()
                .state,
                "pending"
            );
            h.root = roots[0].clone();
            // Simulate process loss after the durable provisional save, before
            // database confirmation. Repeating this save must be exact/idempotent.
            save_host(h).unwrap();
            save_host(h).unwrap();
        }
        *home.state::<PairingState>().0.lock().unwrap() = None;
        *home.state::<AppState>().0.lock().unwrap() = None;
        std::thread::sleep(Duration::from_millis(150));
        tauri::async_runtime::block_on(crate::app_bootstrap(home.handle().clone(), home.state()))
            .unwrap();
        let resumed = tauri::async_runtime::block_on(app_pairing_status(
            home.handle().clone(),
            home.state(),
            home.state(),
        ))
        .unwrap();
        assert_eq!(resumed.state, "pending");
        assert_eq!(resumed.candidate_pin, pending.candidate_pin);
        // Metadata alone never authorizes phone completion or control access.
        assert!(tauri::async_runtime::block_on(app_pairing_finish(
            phone.handle().clone(),
            phone.state(),
            phone.state(),
            phone.state()
        ))
        .is_err());
        assert_eq!(
            app_pairing_home_confirm(home.state()).unwrap().state,
            "confirmed"
        );
        assert_eq!(
            app_pairing_home_confirm(home.state()).unwrap().state,
            "confirmed"
        );
        // Simulate Home process loss after durable local confirmation but before
        // the phone has received proof or committed its connection metadata.
        *home.state::<PairingState>().0.lock().unwrap() = None;
        *home.state::<AppState>().0.lock().unwrap() = None;
        std::thread::sleep(Duration::from_millis(150));
        tauri::async_runtime::block_on(crate::app_bootstrap(home.handle().clone(), home.state()))
            .unwrap();
        let restored = tauri::async_runtime::block_on(app_pairing_status(
            home.handle().clone(),
            home.state(),
            home.state(),
        ))
        .unwrap();
        assert_eq!(restored.state, "confirmed");
        assert!(restored.error.is_none());

        assert_eq!(
            tauri::async_runtime::block_on(app_pairing_finish(
                phone.handle().clone(),
                phone.state(),
                phone.state(),
                phone.state()
            ))
            .unwrap()
            .state,
            "connected"
        );
        assert!(with_app(&phone.state::<AppState>().0, |a| Ok(a
            .paired_home()
            .unwrap()
            .is_some()))
        .unwrap());
        let grant = status(
            home.state::<PairingState>()
                .0
                .lock()
                .unwrap()
                .as_ref()
                .unwrap(),
        )
        .unwrap()
        .grant_expires_at
        .unwrap();
        assert!(grant > now().unwrap() + 350 * 24 * 60 * 60 * 1000);
        assert_eq!(EXECUTION_PERMISSION_MS, 60 * 60 * 1000);
        let revoked = tauri::async_runtime::block_on(app_pairing_revoke(
            home.handle().clone(),
            home.state(),
            home.state(),
        ))
        .unwrap();
        assert_eq!(revoked.state, "revoked");
        assert_eq!(
            tauri::async_runtime::block_on(app_pairing_status(
                home.handle().clone(),
                home.state(),
                home.state()
            ))
            .unwrap()
            .state,
            "revoked"
        );
        *home.state::<PairingState>().0.lock().unwrap() = None;
        *home.state::<AppState>().0.lock().unwrap() = None;
        std::thread::sleep(Duration::from_millis(150));
        tauri::async_runtime::block_on(crate::app_bootstrap(home.handle().clone(), home.state()))
            .unwrap();
        assert_eq!(
            tauri::async_runtime::block_on(app_pairing_status(
                home.handle().clone(),
                home.state(),
                home.state()
            ))
            .unwrap()
            .state,
            "revoked"
        );
        *home.state::<PairingState>().0.lock().unwrap() = None;
        *phone.state::<PairingState>().0.lock().unwrap() = None;
        *home.state::<AppState>().0.lock().unwrap() = None;
        *phone.state::<AppState>().0.lock().unwrap() = None;
        drop(home);
        drop(phone);
        std::thread::sleep(Duration::from_millis(100));
        for root in roots {
            assert!(root
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("org.bobagent.test.qr."));
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}
