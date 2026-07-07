use std::fs;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::Emitter;
use tauri::Manager;

/// payload.zip 在编译时被硬编码进二进制
/// 不再依赖任何外部文件——exe 自身就是完整的安装器
static PAYLOAD: &[u8] = include_bytes!("../payload.zip");

/// 获取默认安装路径
#[tauri::command]
fn get_default_install_dir() -> String {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .unwrap_or_else(|_| "C:\\Users\\Default\\AppData\\Local".to_string());
    format!("{}\\Programs\\BobAgent", local_app_data)
}

/// 选择安装目录（调用系统原生文件对话框）
/// 用户选择的父目录会自动追加 `\bob-agent` 子文件夹
#[tauri::command]
async fn select_install_dir(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let dir = app
        .dialog()
        .file()
        .set_title("选择安装目录")
        .blocking_pick_folder();

    Ok(dir.map(|p| {
        let selected = PathBuf::from(p.to_string());
        // 如果用户选中的目录末尾已经是 bob-agent，就不重复追加
        if selected.file_name().map(|n| n.to_ascii_lowercase()) == Some("bob-agent".into()) {
            selected.to_string_lossy().to_string()
        } else {
            selected.join("bob-agent").to_string_lossy().to_string()
        }
    }))
}

/// 执行安装：从内嵌 payload 解压 + 创建快捷方式 + 写注册表
#[tauri::command]
async fn install(app: tauri::AppHandle, install_dir: String) -> Result<(), String> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    
    let install_path = Path::new(&install_dir);

    // 0. Kill any existing bob.exe process to avoid file lock (OS error 32)
    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = Command::new("tasklist").args(&["/FI", "IMAGENAME eq bob.exe"]).output() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            if output_str.contains("bob.exe") {
                let confirmed = app.dialog()
                    .message("检测到 Bob 正在运行，必须关闭后才能继续安装。\n是否现在强制关闭它？")
                    .title("安装向导")
                    .kind(MessageDialogKind::Warning)
                    .buttons(MessageDialogButtons::OkCancel)
                    .blocking_show();
                
                if confirmed {
                    let _ = Command::new("taskkill").args(&["/F", "/IM", "bob.exe", "/T"]).output();
                    std::thread::sleep(std::time::Duration::from_millis(1000));
                } else {
                    return Err("用户取消了结束进程，安装中止。".to_string());
                }
            }
        }
    }

    // 1. 确保安装目录存在
    fs::create_dir_all(install_path)
        .map_err(|e| format!("无法创建安装目录: {}", e))?;

    // 2. 直接从内存中的 payload 解压
    let cursor = Cursor::new(PAYLOAD);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| format!("无法解析内嵌 payload: {}", e))?;

    let total = archive.len();
    for i in 0..total {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("读取 ZIP 条目失败: {}", e))?;

        let out_path = install_path.join(entry.mangled_name());

        if entry.is_dir() {
            fs::create_dir_all(&out_path)
                .map_err(|e| format!("创建目录失败: {}", e))?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("创建父目录失败: {}", e))?;
            }
            let mut out_file = fs::File::create(&out_path)
                .map_err(|e| format!("创建文件失败: {}", e))?;
            io::copy(&mut entry, &mut out_file)
                .map_err(|e| format!("写入文件失败: {}", e))?;
        }

        // 每解压 10 个文件或最后一个文件时，通知前端更新进度
        if i % 10 == 0 || i == total - 1 {
            let progress = ((i + 1) as f64 / total as f64 * 100.0) as u32;
            let _ = app.emit("install-progress", progress);
        }
    }

    // 3. 创建快捷方式
    let bob_exe = install_path.join("bob.exe");
    if bob_exe.exists() {
        // 桌面快捷方式
        if let Ok(desktop) = std::env::var("USERPROFILE") {
            let desktop_lnk = PathBuf::from(&desktop).join("Desktop").join("Bob Agent.lnk");
            let _ = mslnk::ShellLink::new(bob_exe.to_str().unwrap())
                .map(|sl| sl.create_lnk(desktop_lnk.to_str().unwrap()));
        }

        // 开始菜单快捷方式
        if let Ok(appdata) = std::env::var("APPDATA") {
            let start_menu_dir = PathBuf::from(&appdata)
                .join("Microsoft\\Windows\\Start Menu\\Programs\\Bob Agent");
            let _ = fs::create_dir_all(&start_menu_dir);
            let start_lnk = start_menu_dir.join("Bob Agent.lnk");
            let _ = mslnk::ShellLink::new(bob_exe.to_str().unwrap())
                .map(|sl| sl.create_lnk(start_lnk.to_str().unwrap()));
        }
    }

    // 4. 写入 Windows 注册表 Uninstall 记录
    write_uninstall_registry(&install_dir, &bob_exe)?;

    Ok(())
}

/// 写入注册表卸载信息
fn write_uninstall_registry(install_dir: &str, bob_exe: &Path) -> Result<(), String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let uninstall_path = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\BobAgent";

    let (key, _) = hkcu
        .create_subkey(uninstall_path)
        .map_err(|e| format!("无法创建注册表项: {}", e))?;

    let _ = key.set_value("DisplayName", &"Bob Agent");
    let _ = key.set_value("DisplayVersion", &"0.3.1");
    let _ = key.set_value("Publisher", &"xm_bo");
    let _ = key.set_value("InstallLocation", &install_dir);
    let _ = key.set_value(
        "UninstallString",
        &format!("\"{}\" --uninstall", bob_exe.display()),
    );
    let _ = key.set_value("NoModify", &1u32);
    let _ = key.set_value("NoRepair", &1u32);

    Ok(())
}

/// 启动已安装的 Bob 程序
#[tauri::command]
fn launch_bob(install_dir: String) -> Result<(), String> {
    let bob_exe = Path::new(&install_dir).join("bob.exe");
    if !bob_exe.exists() {
        return Err("bob.exe 不存在".to_string());
    }

    Command::new(bob_exe)
        .spawn()
        .map_err(|e| format!("启动 Bob 失败: {}", e))?;

    // 安装器使命完成，强制退出进程
    std::process::exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_default_install_dir,
            select_install_dir,
            install,
            launch_bob,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
