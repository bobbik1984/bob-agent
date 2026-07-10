fn main() {
    // T-2224: 在交叉编译 Android 目标时自动同步自定义图标
    // 注意: build.rs 运行在 host (Windows) 上，不能用 #[cfg(target_os)]，
    // 必须检查 CARGO_CFG_TARGET_OS 环境变量来判断编译目标
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        use std::path::Path;
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let icons_dir = Path::new(&manifest_dir).join("icons").join("android");
        let res_dir = Path::new(&manifest_dir)
            .join("gen")
            .join("android")
            .join("app")
            .join("src")
            .join("main")
            .join("res");

        if icons_dir.exists() && res_dir.exists() {
            let densities = [
                "mipmap-mdpi",
                "mipmap-hdpi",
                "mipmap-xhdpi",
                "mipmap-xxhdpi",
                "mipmap-xxxhdpi",
            ];
            for density in &densities {
                let src = icons_dir.join(density);
                let dst = res_dir.join(density);
                if src.exists() {
                    let _ = std::fs::create_dir_all(&dst);
                    if let Ok(entries) = std::fs::read_dir(&src) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.extension().map_or(false, |ext| ext == "png") {
                                let dest_file = dst.join(entry.file_name());
                                let _ = std::fs::copy(&path, &dest_file);
                                println!(
                                    "cargo:warning=Synced icon: {}/{}",
                                    density,
                                    entry.file_name().to_string_lossy()
                                );
                            }
                        }
                    }
                }
            }
        }
        // 当图标源发生变化时重新运行
        println!("cargo:rerun-if-changed=icons/android");
    }

    // Generate static skills zip for mobile initial launch payload
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest_path = std::path::Path::new(&out_dir).join("bundled_skills.zip");
    
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let skills_dir = std::path::Path::new(&manifest_dir).join("../skills");
    
    let blacklist = [
        "AKP_Link_Harvester",
        "note_graphify",
        "pptx-translate",
        "skill-creator",
        "mckinsey-designer",
    ];

    if skills_dir.exists() {
        if let Ok(f) = std::fs::File::create(&dest_path) {
            let mut zip = zip::ZipWriter::new(f);
            let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

            if let Ok(entries) = std::fs::read_dir(&skills_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        let folder_name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        if blacklist.contains(&folder_name) {
                            continue; // Skip blacklisted
                        }
                        
                        // Recursively add files
                        let mut dirs = vec![p.clone()];
                        while let Some(dir) = dirs.pop() {
                            if let Ok(sub_entries) = std::fs::read_dir(&dir) {
                                for sub_entry in sub_entries.flatten() {
                                    let sub_path = sub_entry.path();
                                    if sub_path.is_dir() {
                                        dirs.push(sub_path);
                                    } else {
                                        if let Ok(rel_path) = sub_path.strip_prefix(&skills_dir) {
                                            use std::io::{Read, Write};
                                            let _ = zip.start_file(rel_path.to_string_lossy().replace("\\", "/"), options.clone());
                                            if let Ok(mut f_src) = std::fs::File::open(&sub_path) {
                                                let mut content = Vec::new();
                                                let _ = f_src.read_to_end(&mut content);
                                                let _ = zip.write_all(&content);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            let _ = zip.finish();
        }
    }
    println!("cargo:rerun-if-changed=../skills");

    tauri_build::build()
}
