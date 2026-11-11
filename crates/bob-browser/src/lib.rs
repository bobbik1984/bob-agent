//! Pure-Rust CDP browser automation, AXTree snapshotter, password blackout guard, and screencast bridge.

pub mod ax_snapshot;
pub mod cdp;
pub mod error;
pub mod launcher;
pub mod screencast;
pub mod sessions;
pub mod url_guard;

#[cfg(test)]
mod tests;

pub use ax_snapshot::{
    build_page_snapshot, InteractiveElementRef, PageSnapshot, RawElement, SensitiveKind,
    SensitiveSurfaceDetector, DOM_INSPECTION_SCRIPT, REDACTED_PASSWORD_VALUE,
};
pub use cdp::{CdpClient, CdpCommandRequest, CdpErrorObject, CdpEvent, CdpRawMessage};
pub use error::{BrowserError, BrowserResult};
pub use launcher::{find_chromium_binary, BrowserConfig, ChromiumManager, TabState};
pub use screencast::{
    ClientTakeoverCommand, ScreencastBridge, ScreencastFrameMsg, ViewportMeta,
};
pub use sessions::{
    assert_profile_download_forbidden, clear_domain_cookies, clear_site_session, load_sessions,
    record_site_visit, save_sessions, SiteSession,
};
pub use url_guard::{
    is_ip_blocked, is_ipv4_blocked, is_ipv6_blocked, validate_navigation_url, BrowserSecurityPolicy,
};

use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;

/// High-level browser tab wrapper providing safe navigation, AXTree snapshots, and takeover handling.
pub struct BrowserTab {
    pub cdp: CdpClient,
    pub target_id: String,
    pub current_url: String,
    pub policy: BrowserSecurityPolicy,
    pub screencast: ScreencastBridge,
    last_elements: HashMap<u32, InteractiveElementRef>,
}

impl BrowserTab {
    pub fn new(cdp: CdpClient, target_id: String, policy: BrowserSecurityPolicy) -> Self {
        Self {
            cdp,
            target_id,
            current_url: "about:blank".to_string(),
            policy,
            screencast: ScreencastBridge::new(),
            last_elements: HashMap::new(),
        }
    }

    pub fn navigate(&mut self, target_url: &str) -> BrowserResult<()> {
        let validated = validate_navigation_url(target_url, &self.policy)?;
        let params = json!({ "url": validated });
        self.cdp
            .send_command("Page.navigate", Some(params), Duration::from_secs(10))?;
        self.current_url = validated;
        Ok(())
    }

    pub fn take_snapshot(&mut self) -> BrowserResult<PageSnapshot> {
        let val = self.cdp.evaluate(DOM_INSPECTION_SCRIPT)?;
        let raw_title = val
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let raw_url = val
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.current_url)
            .to_string();

        let raw_elements: Vec<RawElement> = if let Some(elements_val) = val.get("elements") {
            serde_json::from_value(elements_val.clone()).unwrap_or_default()
        } else {
            Vec::new()
        };

        let snapshot = build_page_snapshot(&raw_title, &raw_url, raw_elements);
        self.last_elements = snapshot.element_map.clone();
        Ok(snapshot)
    }

    /// Hard security gate: automated typing is blocked if element is a sensitive password / OTP / CAPTCHA.
    pub fn type_into_element(&self, index: u32, text: &str) -> BrowserResult<()> {
        let el = self
            .last_elements
            .get(&index)
            .ok_or(BrowserError::ElementNotFound(index))?;

        if el.is_occluded {
            return Err(BrowserError::ElementOccluded(index));
        }

        SensitiveSurfaceDetector::verify_automated_input_allowed(el)?;

        // Focus and insert text directly via CDP
        let click_down = json!({
            "type": "mousePressed",
            "x": el.center_x,
            "y": el.center_y,
            "button": "left",
            "buttons": 1,
            "clickCount": 1
        });
        let click_up = json!({
            "type": "mouseReleased",
            "x": el.center_x,
            "y": el.center_y,
            "button": "left",
            "buttons": 0
        });
        self.cdp
            .send_command("Input.dispatchMouseEvent", Some(click_down), Duration::from_secs(3))?;
        self.cdp
            .send_command("Input.dispatchMouseEvent", Some(click_up), Duration::from_secs(3))?;

        let insert = json!({ "text": text });
        self.cdp
            .send_command("Input.insertText", Some(insert), Duration::from_secs(3))?;
        Ok(())
    }

    pub fn click_element(&self, index: u32) -> BrowserResult<()> {
        let el = self
            .last_elements
            .get(&index)
            .ok_or(BrowserError::ElementNotFound(index))?;

        if el.is_occluded {
            return Err(BrowserError::ElementOccluded(index));
        }

        let click_down = json!({
            "type": "mousePressed",
            "x": el.center_x,
            "y": el.center_y,
            "button": "left",
            "buttons": 1,
            "clickCount": 1
        });
        let click_up = json!({
            "type": "mouseReleased",
            "x": el.center_x,
            "y": el.center_y,
            "button": "left",
            "buttons": 0
        });
        self.cdp
            .send_command("Input.dispatchMouseEvent", Some(click_down), Duration::from_secs(3))?;
        self.cdp
            .send_command("Input.dispatchMouseEvent", Some(click_up), Duration::from_secs(3))?;
        Ok(())
    }

    pub fn handle_takeover_command(&mut self, cmd: &ClientTakeoverCommand) -> BrowserResult<()> {
        self.screencast.dispatch(&self.cdp, cmd)
    }
}
