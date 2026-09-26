use std::path::{Path, PathBuf};
use std::sync::RwLock;

pub const TEST_PROFILE_MARKER_FILE: &str = ".bob_test_profile";
pub const TEST_PROFILE_MARKER_CONTENT: &str = "BOB_A4_TEST_PROFILE_V1";
pub const TEST_WORKSPACE_SUBDIR: &str = "test_workspace";
pub const ARMED_FAULT_FILE: &str = "fault_injection_armed.json";

static ACTIVE_TEST_DATA_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

/// 获取生产环境默认数据目录路径（用于排他性冲突检查）
pub fn get_production_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        Some(PathBuf::from("/data/data/bob.agent/files"))
    }
    #[cfg(not(target_os = "android"))]
    {
        dirs::data_dir().map(|d| d.join("bob.agent"))
    }
}

/// 逻辑消解路径中的 `.` 与 `..`（不依赖底层文件系统实际存在性）
pub fn clean_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut prefix = None;
    let mut has_root = false;
    let mut stack: Vec<std::ffi::OsString> = Vec::new();

    for comp in path.components() {
        match comp {
            Component::Prefix(p) => {
                prefix = Some(p);
            }
            Component::RootDir => {
                has_root = true;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if let Some(last) = stack.last() {
                    if last != ".." {
                        stack.pop();
                    } else {
                        stack.push(std::ffi::OsString::from(".."));
                    }
                } else if !has_root {
                    stack.push(std::ffi::OsString::from(".."));
                }
            }
            Component::Normal(c) => {
                stack.push(c.to_os_string());
            }
        }
    }

    let mut out = PathBuf::new();
    if let Some(p) = prefix {
        out.push(p.as_os_str());
    }
    if has_root {
        out.push(std::path::MAIN_SEPARATOR.to_string());
    }
    for comp in stack {
        out.push(comp);
    }
    out
}

/// 路径规范化（彻底净化相对跳转并基于最长既有祖先目录解析真实规范化路径）
pub fn normalize_path(path: &Path) -> PathBuf {
    let cleaned = clean_path(path);
    if let Ok(canon) = cleaned.canonicalize() {
        return canon;
    }

    // 若目标路径暂不存在（例如新建多级嵌套文件），寻找最长既有祖先目录 (longest existing ancestor)
    let mut curr = cleaned.as_path();
    let mut suffix = Vec::new();
    while !curr.exists() {
        if let Some(file_name) = curr.file_name() {
            suffix.push(file_name.to_os_string());
            if let Some(parent) = curr.parent() {
                curr = parent;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    if curr.exists() {
        if let Ok(canon_ancestor) = curr.canonicalize() {
            let mut result = canon_ancestor;
            for comp in suffix.into_iter().rev() {
                result.push(comp);
            }
            return result;
        }
    }

    cleaned
}

/// 严密校验隔离测试 Profile 数据目录
/// 规则：
/// 1. 必须为绝对路径；
/// 2. 严禁指向、包含或被包含于生产日常 AppData 目录；
/// 3. 必须包含有效测试防伪标记文件 `.bob_test_profile`；
/// 4. 确保存在专用测试工作区 `test_workspace`。
pub fn validate_test_data_dir(dir: &Path) -> Result<PathBuf, String> {
    if !dir.is_absolute() {
        return Err(format!("测试数据目录必须为绝对路径: {:?}", dir));
    }

    let norm_dir = normalize_path(dir);

    // 生产目录排他性校验 (Anti-Production Hard Gate)
    if let Some(prod_dir) = get_production_data_dir() {
        let norm_prod = normalize_path(&prod_dir);
        if norm_dir == norm_prod {
            return Err(format!(
                "安全阻断：测试数据目录严禁指向生产目录 ({:?})",
                norm_prod
            ));
        }
        if norm_dir.starts_with(&norm_prod) || norm_prod.starts_with(&norm_dir) {
            return Err(format!(
                "安全阻断：测试数据目录严禁与生产目录重叠或嵌套 (test: {:?}, prod: {:?})",
                norm_dir, norm_prod
            ));
        }
    }

    if let Ok(appdata_var) = std::env::var("APPDATA") {
        let appdata_bob = normalize_path(&PathBuf::from(appdata_var).join("bob.agent"));
        if norm_dir == appdata_bob || norm_dir.starts_with(&appdata_bob) || appdata_bob.starts_with(&norm_dir) {
            return Err(format!(
                "安全阻断：测试数据目录严禁指向 %APPDATA%\\bob.agent 生产数据 ({:?})",
                appdata_bob
            ));
        }
    }

    if !dir.exists() {
        return Err(format!("测试数据目录不存在: {:?}", dir));
    }

    // 标记文件校验 (Anti-False-Positive Marker Check)
    let marker_path = dir.join(TEST_PROFILE_MARKER_FILE);
    if !marker_path.is_file() {
        return Err(format!(
            "安全阻断：缺少测试标记文件 {:?}，拒绝将其识别为测试 Profile",
            marker_path
        ));
    }

    let content = std::fs::read_to_string(&marker_path)
        .map_err(|e| format!("读取测试标记文件失败 {:?}: {}", marker_path, e))?;
    if !content.trim().contains(TEST_PROFILE_MARKER_CONTENT) {
        return Err(format!(
            "安全阻断：测试标记文件内容无效 (期望包含 {}): {:?}",
            TEST_PROFILE_MARKER_CONTENT, marker_path
        ));
    }

    // 确保专用工作区目录存在
    let workspace = dir.join(TEST_WORKSPACE_SUBDIR);
    if !workspace.exists() {
        std::fs::create_dir_all(&workspace)
            .map_err(|e| format!("创建测试工作区目录失败 {:?}: {}", workspace, e))?;
    }

    Ok(norm_dir)
}

/// 校验目标文件是否严格位于该测试 Profile 的 `test_workspace` 工作区内
/// 严禁相对路径越界（如 `..`）或指向工作区以外的任何文件
pub fn validate_target_in_test_workspace(target_path: &Path, test_data_dir: &Path) -> Result<PathBuf, String> {
    let workspace = test_data_dir.join(TEST_WORKSPACE_SUBDIR);
    if !workspace.exists() {
        std::fs::create_dir_all(&workspace)
            .map_err(|e| format!("创建测试工作区目录失败 {:?}: {}", workspace, e))?;
    }
    let norm_workspace = normalize_path(&workspace);

    // 统一将相对路径解析为绝对路径
    let resolved_target = if target_path.is_relative() {
        if target_path.starts_with(TEST_WORKSPACE_SUBDIR) {
            test_data_dir.join(target_path)
        } else {
            workspace.join(target_path)
        }
    } else {
        target_path.to_path_buf()
    };

    // 1. 消解所有 ParentDir (..) 相对路径跳转
    let cleaned_target = clean_path(&resolved_target);

    // 严禁存在无法消解的相对逃逸跳转
    for comp in cleaned_target.components() {
        if let std::path::Component::ParentDir = comp {
            return Err(format!(
                "安全阻断：目标路径包含越界 ParentDir (..): {:?}",
                target_path
            ));
        }
    }

    // 2. 核验最长既有祖先目录归属
    let mut curr = cleaned_target.as_path();
    while !curr.exists() {
        if let Some(parent) = curr.parent() {
            curr = parent;
        } else {
            break;
        }
    }

    if curr.exists() {
        let norm_ancestor = normalize_path(curr);
        if !norm_ancestor.starts_with(&norm_workspace) {
            return Err(format!(
                "安全阻断：目标路径既有祖先目录 {:?} 越界，不属于测试工作区 ({:?})",
                norm_ancestor, norm_workspace
            ));
        }
    } else {
        return Err(format!(
            "安全阻断：未能找到目标路径的任何既有祖先目录: {:?}",
            cleaned_target
        ));
    }

    // 3. 消解后最终规范化路径必须严格归属于 norm_workspace
    let norm_target = normalize_path(&cleaned_target);

    if !norm_target.starts_with(&norm_workspace) {
        return Err(format!(
            "安全阻断：目标文件 {:?} 越界，必须严格位于测试工作区内 ({:?})",
            norm_target, norm_workspace
        ));
    }

    if norm_target == norm_workspace {
        return Err(format!(
            "安全阻断：目标文件不可指向测试工作区根目录自身: {:?}",
            norm_target
        ));
    }

    // 严禁目标文件指向测试数据目录内部的系统文件（如 bob.db, config.json 等）
    let norm_test_data = normalize_path(test_data_dir);
    if norm_target == norm_test_data.join("bob.db")
        || norm_target == norm_test_data.join("config.json")
        || norm_target == norm_test_data.join(TEST_PROFILE_MARKER_FILE)
    {
        return Err(format!("安全阻断：目标文件不可指向测试 Profile 自身元数据文件: {:?}", norm_target));
    }

    Ok(norm_target)
}

/// 初始化激活当前诊断测试 Profile
pub fn init_diagnostic_profile(dir: PathBuf) -> Result<PathBuf, String> {
    let validated = validate_test_data_dir(&dir)?;
    if let Ok(mut lock) = ACTIVE_TEST_DATA_DIR.write() {
        *lock = Some(validated.clone());
    }
    // 同步初始化 lib.rs 的全局 DATA_DIR
    crate::set_custom_data_dir(validated.clone())?;
    log::info!("[DiagnosticProfile] Initialized with test data dir: {:?}", validated);
    Ok(validated)
}

/// 获取当前激活的测试数据目录
pub fn get_active_test_data_dir() -> Option<PathBuf> {
    ACTIVE_TEST_DATA_DIR.read().ok().and_then(|lock| lock.clone())
}

/// 判断当前是否处于已激活的测试 Profile 运行环境中
pub fn is_diagnostic_profile_active() -> bool {
    ACTIVE_TEST_DATA_DIR.read().ok().and_then(|lock| lock.clone()).is_some()
}

/// 自动化测试辅助函数：在指定临时目录下安全创建并初始化合法的测试 Profile
pub fn setup_test_profile_in_dir(dir: &Path) -> Result<PathBuf, String> {
    if !dir.is_absolute() {
        return Err(format!("测试目录必须为绝对路径: {:?}", dir));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("创建测试目录失败: {}", e))?;
    let marker_path = dir.join(TEST_PROFILE_MARKER_FILE);
    std::fs::write(&marker_path, format!("{}\n", TEST_PROFILE_MARKER_CONTENT))
        .map_err(|e| format!("写入测试标记文件失败: {}", e))?;
    let workspace = dir.join(TEST_WORKSPACE_SUBDIR);
    std::fs::create_dir_all(&workspace)
        .map_err(|e| format!("创建测试工作区失败: {}", e))?;
    validate_test_data_dir(dir)
}

/// 提取命令行参数或环境变量中指定的测试 Profile 路径
pub fn extract_test_data_dir_arg() -> Option<PathBuf> {
    let mut target_dir_str = None;

    let args: Vec<String> = std::env::args().collect();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--test-data-dir" && i + 1 < args.len() {
            target_dir_str = Some(args[i + 1].clone());
            break;
        }
        if args[i].starts_with("--test-data-dir=") {
            target_dir_str = Some(args[i].trim_start_matches("--test-data-dir=").to_string());
            break;
        }
        i += 1;
    }

    if target_dir_str.is_none() {
        if let Ok(env_val) = std::env::var("BOB_TEST_DATA_DIR") {
            if !env_val.trim().is_empty() {
                target_dir_str = Some(env_val.trim().to_string());
            }
        }
    }

    target_dir_str.map(PathBuf::from)
}

/// 严格确保测试 Profile 已被正确初始化。
/// 当运行在 fault-injection 特性下时，必须显式指定有效的测试数据目录；
/// 若未提供参数或初始化校验失败，必须 Fail-Closed 拒绝启动，严禁静默回退至日常生产数据目录。
pub fn ensure_diagnostic_profile_initialized() -> Result<PathBuf, String> {
    if let Some(active) = get_active_test_data_dir() {
        return Ok(active);
    }

    let target_dir = extract_test_data_dir_arg().ok_or_else(|| {
        "未提供 --test-data-dir 命令行参数或 BOB_TEST_DATA_DIR 环境变量，故障注入诊断包严禁静默回退至日常生产数据目录".to_string()
    })?;

    init_diagnostic_profile(target_dir)
}

/// 启动时检查命令行参数或环境变量以加载测试 Profile（保持旧接口兼容）
pub fn try_init_from_env_or_args() -> Option<PathBuf> {
    ensure_diagnostic_profile_initialized().ok()
}

#[cfg(any(test, feature = "fault-injection"))]
pub fn reset_diagnostic_profile_for_test() {
    if let Ok(mut lock) = ACTIVE_TEST_DATA_DIR.write() {
        *lock = None;
    }
}
