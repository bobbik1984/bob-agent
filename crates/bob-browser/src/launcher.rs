use crate::error::{BrowserError, BrowserResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserConfig {
    pub binary_path: Option<PathBuf>,
    pub user_data_dir: PathBuf,
    pub remote_debugging_port: u16, // 0 for ephemeral
    pub window_width: u32,
    pub window_height: u32,
    pub idle_tab_timeout_secs: u64,
    pub headless: bool,
    pub additional_args: Vec<String>,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            binary_path: None,
            user_data_dir: std::env::temp_dir().join("bob_browser_profile"),
            remote_debugging_port: 0,
            window_width: 390,
            window_height: 844,
            idle_tab_timeout_secs: 600, // 10 minutes
            headless: true,
            additional_args: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabState {
    pub target_id: String,
    pub url: String,
    pub ws_url: String,
    pub last_active_at: u64,
    pub is_sleeping: bool,
}

pub struct ChromiumManager {
    config: BrowserConfig,
    child_process: Option<Child>,
    active_port: Option<u16>,
    tabs: HashMap<String, TabState>,
}

pub fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Discovers installed Chromium / Chrome / Edge binary cross-platform.
pub fn find_chromium_binary() -> BrowserResult<PathBuf> {
    // 1. Check BOB_CHROMIUM_BIN environment variable override
    if let Ok(env_bin) = std::env::var("BOB_CHROMIUM_BIN") {
        let p = PathBuf::from(env_bin);
        if p.is_file() {
            return Ok(p);
        }
    }

    // 2. Platform-specific search paths
    let candidate_paths: &[&str] = if cfg!(target_os = "windows") {
        &[
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ]
    } else {
        &[
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
            "/usr/bin/google-chrome",
            "/usr/bin/google-chrome-stable",
            "/usr/bin/microsoft-edge",
        ]
    };

    for path_str in candidate_paths {
        let p = PathBuf::from(path_str);
        if p.is_file() {
            return Ok(p);
        }
    }

    // Check LOCALAPPDATA on Windows
    if cfg!(target_os = "windows") {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            let p1 = PathBuf::from(&local_app_data).join(r"Google\Chrome\Application\chrome.exe");
            if p1.is_file() {
                return Ok(p1);
            }
            let p2 = PathBuf::from(&local_app_data).join(r"Microsoft\Edge\Application\msedge.exe");
            if p2.is_file() {
                return Ok(p2);
            }
        }
    }

    Err(BrowserError::BinaryNotFound)
}

impl ChromiumManager {
    pub fn new(config: BrowserConfig) -> Self {
        Self {
            config,
            child_process: None,
            active_port: None,
            tabs: HashMap::new(),
        }
    }

    pub fn config(&self) -> &BrowserConfig {
        &self.config
    }

    pub fn active_port(&self) -> Option<u16> {
        self.active_port
    }

    pub fn is_running(&mut self) -> bool {
        if let Some(ref mut child) = self.child_process {
            match child.try_wait() {
                Ok(None) => true,
                _ => false,
            }
        } else {
            false
        }
    }

    pub fn ensure_healthy(&mut self) -> BrowserResult<u16> {
        if self.is_running() {
            if let Some(port) = self.active_port {
                return Ok(port);
            }
        }
        self.spawn_process()
    }

    pub fn spawn_process(&mut self) -> BrowserResult<u16> {
        let bin_path = if let Some(ref p) = self.config.binary_path {
            p.clone()
        } else {
            find_chromium_binary()?
        };

        // Ensure user-data-dir directory exists
        fs::create_dir_all(&self.config.user_data_dir)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(
                &self.config.user_data_dir,
                fs::Permissions::from_mode(0o700),
            );
        }

        // Remove old DevToolsActivePort to avoid stale reads
        let active_port_file = self.config.user_data_dir.join("DevToolsActivePort");
        let _ = fs::remove_file(&active_port_file);

        let mut cmd = Command::new(&bin_path);
        if self.config.headless {
            cmd.arg("--headless=new");
        }

        cmd.arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--disable-background-networking")
            .arg("--disable-gpu")
            .arg("--mute-audio")
            .arg(format!("--window-size={},{}", self.config.window_width, self.config.window_height))
            .arg(format!("--user-data-dir={}", self.config.user_data_dir.display()))
            .arg(format!("--remote-debugging-port={}", self.config.remote_debugging_port));

        for arg in &self.config.additional_args {
            cmd.arg(arg);
        }

        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = cmd
            .spawn()
            .map_err(|e| BrowserError::LaunchFailed(format!("{}: {}", bin_path.display(), e)))?;

        self.child_process = Some(child);

        // Discover active remote debugging port
        let port = self.discover_port(active_port_file)?;
        self.active_port = Some(port);
        Ok(port)
    }

    fn discover_port(&self, active_port_file: PathBuf) -> BrowserResult<u16> {
        if self.config.remote_debugging_port > 0 {
            return Ok(self.config.remote_debugging_port);
        }

        let start = Instant::now();
        let timeout = Duration::from_secs(10);

        while start.elapsed() < timeout {
            if active_port_file.exists() {
                if let Ok(content) = fs::read_to_string(&active_port_file) {
                    if let Some(first_line) = content.lines().next() {
                        if let Ok(port) = first_line.trim().parse::<u16>() {
                            if port > 0 {
                                return Ok(port);
                            }
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        Err(BrowserError::LaunchFailed(
            "Timed out waiting for DevToolsActivePort".to_string(),
        ))
    }

    pub fn get_or_create_page(&mut self) -> BrowserResult<(String, String)> {
        let port = self.ensure_healthy()?;
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()?;

        let list_url = format!("http://127.0.0.1:{}/json/list", port);
        if let Ok(resp) = client.get(&list_url).send() {
            if let Ok(targets) = resp.json::<Vec<serde_json::Value>>() {
                for target in targets {
                    let t_type = target.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if t_type == "page" {
                        if let (Some(id), Some(ws)) = (
                            target.get("id").and_then(|v| v.as_str()),
                            target.get("webSocketDebuggerUrl").and_then(|v| v.as_str()),
                        ) {
                            let url = target.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            self.tabs.insert(
                                id.to_string(),
                                TabState {
                                    target_id: id.to_string(),
                                    url,
                                    ws_url: ws.to_string(),
                                    last_active_at: now_unix_secs(),
                                    is_sleeping: false,
                                },
                            );
                            return Ok((id.to_string(), ws.to_string()));
                        }
                    }
                }
            }
        }

        // If no page target exists, create a new one
        let new_url = format!("http://127.0.0.1:{}/json/new?about:blank", port);
        let resp = client.put(&new_url).send()?;
        let target: serde_json::Value = resp.json()?;
        let id = target.get("id").and_then(|v| v.as_str()).ok_or_else(|| {
            BrowserError::Other("Missing id in /json/new response".to_string())
        })?;
        let ws = target
            .get("webSocketDebuggerUrl")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                BrowserError::Other("Missing webSocketDebuggerUrl in /json/new response".to_string())
            })?;

        self.tabs.insert(
            id.to_string(),
            TabState {
                target_id: id.to_string(),
                url: "about:blank".to_string(),
                ws_url: ws.to_string(),
                last_active_at: now_unix_secs(),
                is_sleeping: false,
            },
        );

        Ok((id.to_string(), ws.to_string()))
    }

    pub fn record_tab_active(&mut self, target_id: &str) {
        if let Some(tab) = self.tabs.get_mut(target_id) {
            tab.last_active_at = now_unix_secs();
            tab.is_sleeping = false;
        }
    }

    pub fn check_idle_tabs(&mut self, now: u64) -> BrowserResult<Vec<String>> {
        let mut hibernated = Vec::new();
        let timeout = self.config.idle_tab_timeout_secs;
        let port = match self.active_port {
            Some(p) => p,
            None => return Ok(hibernated),
        };

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()?;

        for (id, tab) in self.tabs.iter_mut() {
            if !tab.is_sleeping && now.saturating_sub(tab.last_active_at) > timeout {
                let close_url = format!("http://127.0.0.1:{}/json/close/{}", port, id);
                let _ = client.get(&close_url).send();
                tab.is_sleeping = true;
                hibernated.push(id.clone());
            }
        }

        Ok(hibernated)
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.child_process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.active_port = None;
    }
}

impl Drop for ChromiumManager {
    fn drop(&mut self) {
        self.stop();
    }
}
