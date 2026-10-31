use bob_node::{spawn_node_server, BobNodeConfig};
use std::path::PathBuf;
use std::time::Duration;

fn main() {
    let mut bind_addr =
        std::env::var("BOB_NODE_BIND").unwrap_or_else(|_| "127.0.0.1:53020".to_owned());
    let mut data_dir = std::env::var("BOB_NODE_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./bob-node-data"));
    let mut static_dir = std::env::var("BOB_NODE_STATIC_DIR").ok().map(PathBuf::from);
    let mut legacy_db_path = std::env::var("BOB_NODE_LEGACY_DB").ok().map(PathBuf::from);

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
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
            _ => {
                i += 1;
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

    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}
