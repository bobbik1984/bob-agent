use bob_node::{spawn_node_server, BobNodeConfig};
use std::path::PathBuf;
use std::time::Duration;

fn main() {
    let mut bind_addr =
        std::env::var("BOB_NODE_BIND").unwrap_or_else(|_| "0.0.0.0:53020".to_owned());
    let mut data_dir = std::env::var("BOB_NODE_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./bob-node-data"));
    let mut static_dir = std::env::var("BOB_NODE_STATIC_DIR").ok().map(PathBuf::from);
    let mut legacy_db_path = std::env::var("BOB_NODE_LEGACY_DB").ok().map(PathBuf::from);
    let mut public_url = std::env::var("BOB_NODE_PUBLIC_URL").ok();
    let mut node_label = std::env::var("BOB_NODE_LABEL").ok();
    let mut print_qr = std::env::var("BOB_NODE_NO_QR").ok().as_deref() != Some("1");

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
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
                i += 2;
            }
            "--data-dir" if i + 1 < args.len() => {
                data_dir = PathBuf::from(&args[i + 1]);
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
            "--print-qr" | "--pair-qr" => {
                print_qr = true;
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    if args.iter().any(|a| a == "--set-pairing-password") {
        use std::io::Read;
        let mut password=String::new();
        std::io::stdin().take(1025).read_to_string(&mut password).expect("password input unavailable");
        match bob_node::node_access::initialize(&data_dir,password.trim_end_matches(['\r','\n'])) {
            Ok(())=>println!("Node owner password configured"),
            Err(e)=>{eprintln!("{e}");std::process::exit(1);}
        }
        return;
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

    if !bob_node::node_access::enabled(&data_dir) {
    if let Some(boot_code) = server.bootstrap_pairing_code() {
        println!("  Initial Web Onboarding Code (single-use, 30m): {boot_code}");
    }
    if let Ok(inv) = server.issue_startup_pairing(public_url.as_deref(), node_label.as_deref()) {
        let qr_uri = inv.encode_qr_code();
        println!("----------------------------------------------------------------");
        println!(
            "  Bob Node Ready: {} ({}) [role: {}]",
            inv.label, inv.base_url, inv.role
        );
        if !inv.harnesses.is_empty() {
            println!("  Auto-discovered local agents: {}", inv.harnesses.join(", "));
        }
        println!("  Pairing Code (6-digit): {}", inv.user_code);
        println!("  Pairing URI:            {}", qr_uri);
        println!("----------------------------------------------------------------");
        if print_qr {
            if let Ok(qr_art) = bob_runtime::node_pairing::render_terminal_qr(&qr_uri) {
                println!("  Scan this QR code with the Bob Mobile App to connect:\n");
                print!("{qr_art}");
                println!("----------------------------------------------------------------");
            }
        }
    }

    }
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}
