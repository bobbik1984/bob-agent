fn main() {
  // T-2224: 在交叉编译 Android 目标时自动同步自定义图标
  // 注意: build.rs 运行在 host (Windows) 上，不能用 #[cfg(target_os)]，
  // 必须检查 CARGO_CFG_TARGET_OS 环境变量来判断编译目标
  if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
    use std::path::Path;
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let icons_dir = Path::new(&manifest_dir).join("icons").join("android");
    let res_dir = Path::new(&manifest_dir).join("gen").join("android")
        .join("app").join("src").join("main").join("res");

    if icons_dir.exists() && res_dir.exists() {
      let densities = ["mipmap-mdpi", "mipmap-hdpi", "mipmap-xhdpi", "mipmap-xxhdpi", "mipmap-xxxhdpi"];
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
                println!("cargo:warning=Synced icon: {}/{}", density, entry.file_name().to_string_lossy());
              }
            }
          }
        }
      }
    }
    // 当图标源发生变化时重新运行
    println!("cargo:rerun-if-changed=icons/android");
  }

  tauri_build::build()
}

