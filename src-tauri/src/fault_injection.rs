use std::cell::RefCell;
use std::path::{Path, PathBuf};
use crate::diagnostic_profile::{
    normalize_path, validate_target_in_test_workspace, validate_test_data_dir,
    get_active_test_data_dir, is_diagnostic_profile_active, ARMED_FAULT_FILE,
};

pub const FAULT_INTERRUPT_EXIT_CODE: i32 = 77;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaultStage {
    WriteError07a,
    Prepared,
    BackedUp,
    Replaced,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArmedFaultConfig {
    pub target_change_id: String,
    pub target_file_path: String,
    pub stage: FaultStage,
    #[serde(default = "default_true")]
    pub one_shot: bool,
}

fn default_true() -> bool {
    true
}

// 线程级布防（用于在单元测试中并发隔离使用）
thread_local! {
    static ARMED_FAULT_TL: RefCell<Option<ArmedFaultConfig>> = const { RefCell::new(None) };
    static TEST_DIR_TL: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    static SIMULATE_CONSUMPTION_FAILURE_TL: RefCell<bool> = const { RefCell::new(false) };
}

/// 线程级测试辅助：模拟布防文件消费持久化失败
pub fn set_simulate_consumption_failure(fail: bool) {
    SIMULATE_CONSUMPTION_FAILURE_TL.with(|cell| {
        *cell.borrow_mut() = fail;
    });
}

/// 线程级测试辅助：设置当前线程专属的测试 Profile 目录
pub fn set_thread_test_dir(dir: Option<PathBuf>) {
    TEST_DIR_TL.with(|cell| {
        *cell.borrow_mut() = dir;
    });
}

/// 线程级测试辅助：设置当前线程专属的布防配置
pub fn arm_fault_thread_local(config: Option<ArmedFaultConfig>) {
    ARMED_FAULT_TL.with(|cell| {
        *cell.borrow_mut() = config;
    });
}

/// 在指定测试 Profile 目录下安全布防一次性故障注入配置
pub fn arm_fault_file(test_data_dir: &Path, config: ArmedFaultConfig) -> Result<PathBuf, String> {
    let valid_data_dir = validate_test_data_dir(test_data_dir)?;
    let target_path = PathBuf::from(&config.target_file_path);
    let valid_target = validate_target_in_test_workspace(&target_path, &valid_data_dir)?;

    if config.stage != FaultStage::WriteError07a && !config.one_shot {
        return Err("07B 崩溃窗口注入必须强制配置 one_shot: true，严禁配置 one_shot: false".to_string());
    }

    let mut canonical_config = config;
    canonical_config.target_file_path = valid_target.to_string_lossy().to_string();

    let armed_path = valid_data_dir.join(ARMED_FAULT_FILE);
    let json_bytes = serde_json::to_vec_pretty(&canonical_config)
        .map_err(|e| format!("序列化布防配置失败: {}", e))?;
    std::fs::write(&armed_path, json_bytes)
        .map_err(|e| format!("写入布防配置文件失败 {:?}: {}", armed_path, e))?;

    log::info!(
        "[FaultInjection] Armed one-shot fault config at {:?}: change={}, stage={:?}, target={}, one_shot={}",
        armed_path,
        canonical_config.target_change_id,
        canonical_config.stage,
        canonical_config.target_file_path,
        canonical_config.one_shot
    );

    Ok(armed_path)
}

/// 取消指定测试 Profile 目录下的故障布防
pub fn disarm_fault_file(test_data_dir: &Path) -> Result<bool, String> {
    let armed_path = test_data_dir.join(ARMED_FAULT_FILE);
    if armed_path.exists() {
        std::fs::remove_file(&armed_path)
            .map_err(|e| format!("删除布防文件失败 {:?}: {}", armed_path, e))?;
        log::info!("[FaultInjection] Disarmed fault file at {:?}", armed_path);
        Ok(true)
    } else {
        Ok(false)
    }
}

/// 安全消费一次性布防文件。
/// 优先尝试物理删除；若物理删除失败，尝试以已消费标记 {"consumed":true} 覆写。
/// 若两步均告失败，必须返回 Err，严禁继续触发中断以防陷入重启死循环。
pub fn consume_armed_fault_file(armed_file: &Path) -> Result<(), String> {
    let sim_fail = SIMULATE_CONSUMPTION_FAILURE_TL.with(|cell| *cell.borrow());
    if sim_fail {
        let err = "CRITICAL SECURITY HARD GATE: (模拟测试) 无法持久化清除布防状态".to_string();
        log::error!("[FaultInjection] {}", err);
        return Err(err);
    }

    match std::fs::remove_file(armed_file) {
        Ok(()) => {
            log::info!("[FaultInjection] Successfully consumed armed file {:?} prior to trigger", armed_file);
            Ok(())
        }
        Err(rem_err) => {
            log::warn!(
                "[FaultInjection] Failed to remove armed file {:?}: {}. Attempting fallback overwrite with consumed status",
                armed_file, rem_err
            );
            match std::fs::write(armed_file, b"{\"consumed\":true}") {
                Ok(()) => {
                    log::info!("[FaultInjection] Successfully marked armed file {:?} as consumed", armed_file);
                    Ok(())
                }
                Err(write_err) => {
                    let err = format!(
                        "CRITICAL SECURITY HARD GATE: 无法持久化清除布防状态 (remove 失败: {}, write 覆写失败: {})",
                        rem_err, write_err
                    );
                    log::error!("[FaultInjection] {}", err);
                    Err(err)
                }
            }
        }
    }
}

/// 读取并校验当前环境中的布防配置
pub fn read_and_validate_armed(
    change_id: &str,
    target_path: &Path,
    expected_stage: FaultStage,
) -> Option<ArmedFaultConfig> {
    // 1. 优先检查当前线程隔离配置 (Unit Tests)
    let tl_hit = ARMED_FAULT_TL.with(|cell| {
        if let Some(ref config) = *cell.borrow() {
            if config.target_change_id == change_id && config.stage == expected_stage {
                let norm_target = normalize_path(target_path);
                let norm_config_target = normalize_path(Path::new(&config.target_file_path));
                if norm_target == norm_config_target {
                    return Some(config.clone());
                }
            }
        }
        None
    });

    if let Some(conf) = tl_hit {
        // 07B 崩溃窗口注入必须强制要求 one_shot: true
        if expected_stage != FaultStage::WriteError07a && !conf.one_shot {
            log::error!(
                "[FaultInjection] 07B 崩溃窗口注入强制要求 one_shot: true，检测到 one_shot: false，拒绝触发中断以防无限死循环"
            );
            return None;
        }

        if conf.one_shot {
            ARMED_FAULT_TL.with(|cell| {
                *cell.borrow_mut() = None;
            });
        }
        return Some(conf);
    }

    // 2. 检查基于测试 Profile 文件的布防配置 (Subprocess & Physical Diagnostics)
    let test_dir = TEST_DIR_TL.with(|cell| cell.borrow().clone())
        .or_else(get_active_test_data_dir);

    let test_data_dir = match test_dir {
        Some(d) => d,
        None => {
            // 没有激活的测试 Profile，坚决不响应任何故障注入！
            return None;
        }
    };

    let armed_file = test_data_dir.join(ARMED_FAULT_FILE);
    if !armed_file.is_file() {
        return None;
    }

    let content = match std::fs::read_to_string(&armed_file) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[FaultInjection] Failed to read armed fault file {:?}: {}", armed_file, e);
            return None;
        }
    };

    // 若已被标记为已消费状态，直接返回 None
    if content.contains("\"consumed\":true") || content.contains("\"consumed\": true") {
        log::info!("[FaultInjection] Armed file {:?} is already marked consumed, skipping", armed_file);
        return None;
    }

    let config: ArmedFaultConfig = match serde_json::from_str(&content) {
        Ok(cfg) => cfg,
        Err(e) => {
            log::error!("[FaultInjection] Invalid JSON in armed fault file {:?}: {}", armed_file, e);
            return None;
        }
    };

    if config.target_change_id != change_id || config.stage != expected_stage {
        return None;
    }

    // 07B 崩溃窗口注入必须强制要求 one_shot: true，若布防配置为 one_shot: false，严禁触发退出 77，记录错误并返回 None
    if expected_stage != FaultStage::WriteError07a && !config.one_shot {
        log::error!(
            "[FaultInjection] 07B 崩溃窗口注入强制要求 one_shot: true，检测到 one_shot: false，拒绝触发中断以防无限死循环"
        );
        return None;
    }

    let norm_target = normalize_path(target_path);
    let norm_config_target = normalize_path(Path::new(&config.target_file_path));
    if norm_target != norm_config_target {
        log::warn!(
            "[FaultInjection] Target path mismatch for change {}: given {:?}, armed {:?}",
            change_id, norm_target, norm_config_target
        );
        return None;
    }

    // 严密二次核验：目标文件必须严格落在 test_workspace 中
    if let Err(e) = validate_target_in_test_workspace(target_path, &test_data_dir) {
        log::error!("[FaultInjection] Target file escape validation failed: {}", e);
        return None;
    }

    // 单次布防消费防崩溃循环：在执行中断或报错前，必须确保布防状态已被持久化清除！
    if config.one_shot {
        if let Err(e) = consume_armed_fault_file(&armed_file) {
            log::error!(
                "[FaultInjection] 拒绝触发故障中断: 布防状态未能持久化清除 ({})，为防止重启无限崩溃死循环立即终止注入",
                e
            );
            return None;
        }
    }

    Some(config)
}

/// 检查并执行 07A 受控写入失败注入
pub fn check_07a_write_error(change_id: &str, target_path: &Path) -> Option<String> {
    if let Some(cfg) = read_and_validate_armed(change_id, target_path, FaultStage::WriteError07a) {
        let msg = format!(
            "07A 受控写入注入失败: 模拟磁盘写保护或权限拒绝 (PermissionDenied) [change_id: {}, target: {}]",
            change_id, cfg.target_file_path
        );
        log::warn!("[FaultInjection] Triggering Case 07A simulated error: {}", msg);
        Some(msg)
    } else {
        None
    }
}

/// 检查并在恢复记录成功落库后执行 07B 崩溃窗口真实进程中断
pub fn check_and_trigger_07b_interrupt(change_id: &str, target_path: &Path, stage: FaultStage) {
    if let Some(cfg) = read_and_validate_armed(change_id, target_path, stage) {
        eprintln!(
            "[FaultInjection AUDIT] 07B Crash window simulated interrupt triggered! Stage: {:?}, Change: {}, Target: {}. Exiting with code {}",
            stage, change_id, cfg.target_file_path, FAULT_INTERRUPT_EXIT_CODE
        );
        log::error!(
            "[FaultInjection AUDIT] 07B Crash window simulated interrupt triggered! Stage: {:?}, Change: {}, Target: {}. Exiting with code {}",
            stage, change_id, cfg.target_file_path, FAULT_INTERRUPT_EXIT_CODE
        );
        // 确保标准输出与标准错误全部刷盘
        use std::io::Write;
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();

        std::process::exit(FAULT_INTERRUPT_EXIT_CODE);
    }
}
