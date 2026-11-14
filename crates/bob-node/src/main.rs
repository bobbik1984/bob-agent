use bob_node::{
    issue_node_access_cli_pairing, issue_offline_online_pairing, resolve_public_url,
    spawn_node_server, BobNodeConfig,
};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn resolve_cli_data_dir(explicit: Option<PathBuf>, one_shot_pair: bool) -> PathBuf {
    if let Some(dir) = explicit {
        return dir;
    }
    if let Ok(env_dir) = std::env::var("BOB_NODE_DATA_DIR") {
        let trimmed = env_dir.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    if one_shot_pair {
        if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            let trimmed = home.trim();
            if !trimmed.is_empty() {
                let user_node_access = PathBuf::from(trimmed).join(".local/share/bob-node-access");
                if user_node_access.join("node_access.sqlite").exists() {
                    return user_node_access;
                }
            }
        }
        let node_access_default = PathBuf::from("/var/lib/bob-node-access");
        if node_access_default.join("node_access.sqlite").exists() {
            return node_access_default;
        }
        let opt_online_default = PathBuf::from("/opt/bob-online/data");
        if opt_online_default.is_dir() {
            return opt_online_default;
        }
        let online_default = PathBuf::from("/var/lib/bob-online");
        if online_default.is_dir() {
            return online_default;
        }
    }
    PathBuf::from("./bob-node-data")
}

fn parse_port(bind_addr: &str) -> u16 {
    bind_addr
        .rsplit_once(':')
        .and_then(|(_, p)| p.parse::<u16>().ok())
        .unwrap_or(53020)
}

fn try_local_daemon_qr_create(
    port: u16,
    public_url: &str,
    label: Option<&str>,
) -> Option<bob_runtime::node_pairing::NodePairingInvitation> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));
    let mut payload = serde_json::json!({ "origin": public_url });
    if let Some(l) = label.map(str::trim).filter(|s| !s.is_empty()) {
        payload["label"] = serde_json::json!(l);
    }
    let body = payload.to_string();
    let req = format!(
        "POST /api/rpc/app_pairing_qr_create HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(req.as_bytes()).ok()?;
    let mut resp = String::new();
    stream.read_to_string(&mut resp).ok()?;
    let (_, json_body) = resp.split_once("\r\n\r\n")?;
    let parsed: serde_json::Value = serde_json::from_str(json_body).ok()?;
    if parsed.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        return None;
    }
    let res = &parsed["result"];
    let qr = res.get("code").and_then(serde_json::Value::as_str)?;
    let inv = bob_runtime::node_pairing::NodePairingInvitation::decode_qr_code(qr, 0).ok()?;
    Some(bob_runtime::node_pairing::NodePairingInvitation {
        v: 1,
        role: res
            .get("role")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&inv.role)
            .to_owned(),
        node_id: res
            .get("node_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&inv.node_id)
            .to_owned(),
        label: res
            .get("home_name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&inv.label)
            .to_owned(),
        base_url: res
            .get("base_url")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&inv.base_url)
            .to_owned(),
        pairing_id: res
            .get("pairing_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&inv.pairing_id)
            .to_owned(),
        user_code: res
            .get("user_code")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&inv.user_code)
            .to_owned(),
        expires_at: res
            .get("expires_at")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(inv.expires_at),
        harnesses: inv.harnesses,
    })
}

fn print_invitation_banner(
    inv: &bob_runtime::node_pairing::NodePairingInvitation,
    print_qr: bool,
    boot_code: Option<&str>,
    extra_note: Option<&str>,
) {
    let qr_uri = inv.encode_compact_qr_code();
    println!("----------------------------------------------------------------");
    println!("  Bob Node Ready: {} ({}) [role: {}]", inv.label, inv.base_url, inv.role);
    if !inv.harnesses.is_empty() {
        println!("  Auto-discovered local agents: {}", inv.harnesses.join(", "));
    }
    if let Some(code) = boot_code {
        println!("  Initial Web Onboarding Code (single-use, 30m): {code}");
    }
    println!("  Pairing URI (10m single-use): {qr_uri}");
    if let Some(note) = extra_note {
        println!("  {note}");
    }
    println!("----------------------------------------------------------------");
    if print_qr {
        if let Ok(qr_art) = bob_runtime::node_pairing::render_terminal_qr(&qr_uri) {
            println!("  Scan this QR code with the Bob App (Mobile / Web / PC):\n");
            print!("{qr_art}");
            println!("----------------------------------------------------------------");
        }
    }
}

fn main() {
    let mut bind_explicit = std::env::var("BOB_NODE_BIND")
        .ok()
        .is_some_and(|s| !s.trim().is_empty());
    let mut bind_addr =
        std::env::var("BOB_NODE_BIND").unwrap_or_else(|_| "0.0.0.0:53020".to_owned());
    let mut explicit_data_dir: Option<PathBuf> = None;
    let mut static_dir = std::env::var("BOB_NODE_STATIC_DIR").ok().map(PathBuf::from);
    let mut legacy_db_path = std::env::var("BOB_NODE_LEGACY_DB").ok().map(PathBuf::from);
    let mut public_url = std::env::var("BOB_NODE_PUBLIC_URL").ok();
    let mut node_label = std::env::var("BOB_NODE_LABEL").ok();
    let mut print_qr = std::env::var("BOB_NODE_NO_QR").ok().as_deref() != Some("1");
    let mut one_shot_pair = false;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "pair" | "qr" | "--pair-qr" | "--pair-only" => {
                one_shot_pair = true;
                print_qr = true;
                i += 1;
            }
            "--probe" if i + 1 < args.len() => {
                match bob_runtime::agy_directory::run_local_probe(&args[i + 1], 60) {
                    Ok(bytes) => {
                        let _ = std::io::Write::write_all(&mut std::io::stdout(), &bytes);
                        return;
                    }
                    Err(err) => {
                        eprintln!(r#"{{"error":"{err}"}}"#);
                        std::process::exit(1);
                    }
                }
            }
            "--bind" if i + 1 < args.len() => {
                bind_addr = args[i + 1].clone();
                bind_explicit = true;
                i += 2;
            }
            "--data-dir" if i + 1 < args.len() => {
                explicit_data_dir = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--static-dir" if i + 1 < args.len() => {
                static_dir = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--legacy-db" if i + 1 < args.len() => {
                legacy_db_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--public-url" if i + 1 < args.len() => {
                public_url = Some(args[i + 1].clone());
                i += 2;
            }
            "--label" if i + 1 < args.len() => {
                node_label = Some(args[i + 1].clone());
                i += 2;
            }
            "--no-qr" => {
                print_qr = false;
                i += 1;
            }
            "--print-qr" => {
                print_qr = true;
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    let data_dir = resolve_cli_data_dir(explicit_data_dir, one_shot_pair);

    if args.iter().any(|a| a == "--set-pairing-password") {
        let mut password = String::new();
        std::io::stdin()
            .take(1025)
            .read_to_string(&mut password)
            .expect("password input unavailable");
        match bob_node::node_access::initialize(&data_dir, password.trim_end_matches(['\r', '\n']))
        {
            Ok(()) => println!("Node owner password configured"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }

    if one_shot_pair {
        let port = if !bind_explicit && bob_node::node_access::enabled(Path::new(&data_dir)) {
            9527
        } else {
            parse_port(&bind_addr)
        };
        let pseudo_addr = format!("0.0.0.0:{port}").parse().ok();
        let resolved_url = resolve_public_url(public_url.as_deref(), pseudo_addr);
        let inv_res = if bob_node::node_access::enabled(Path::new(&data_dir)) {
            issue_node_access_cli_pairing(&data_dir, &resolved_url, node_label.as_deref())
        } else if let Some(live_inv) =
            try_local_daemon_qr_create(port, &resolved_url, node_label.as_deref())
        {
            Ok(live_inv)
        } else {
            issue_offline_online_pairing(&data_dir, &resolved_url, node_label.as_deref())
        };
        match inv_res {
            Ok(inv) => {
                print_invitation_banner(&inv, print_qr, None, None);
                return;
            }
            Err(err) => {
                eprintln!("failed to create pairing invitation: {err}");
                std::process::exit(1);
            }
        }
    }

    let config = BobNodeConfig {
        bind_addr: bind_addr.clone(),
        data_dir: data_dir.clone(),
        static_dir: static_dir.clone(),
        legacy_db_path: legacy_db_path.clone(),
    };

    let server = spawn_node_server(config).expect("failed to start bob-node server");
    println!(
        "bob-node listening on http://{} (data_dir: {:?}, static_dir: {:?}, legacy_db: {:?})",
        server.local_addr, data_dir, static_dir, legacy_db_path
    );

    if bob_node::node_access::enabled(&data_dir) {
        if let Ok(inv) = server.issue_node_access_startup_pairing(
            public_url.as_deref(),
            node_label.as_deref(),
        ) {
            print_invitation_banner(&inv, print_qr, None, Some("Auth mode: owner_password + QR invitation"));
        }
    } else {
        let boot_code = server.bootstrap_pairing_code();
        if let Ok(inv) =
            server.issue_startup_pairing(public_url.as_deref(), node_label.as_deref())
        {
            print_invitation_banner(&inv, print_qr, boot_code.as_deref(), None);
        }
    }

    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}
