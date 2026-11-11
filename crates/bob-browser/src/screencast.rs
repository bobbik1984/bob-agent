use crate::cdp::{CdpClient, CdpEvent};
use crate::error::BrowserResult;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewportMeta {
    #[serde(rename = "type", default = "default_viewport_meta_type")]
    pub msg_type: String,
    pub url: String,
    pub title: String,
    #[serde(alias = "width", alias = "remoteWidth")]
    pub remote_width: u32,
    #[serde(alias = "height", alias = "remoteHeight")]
    pub remote_height: u32,
    #[serde(alias = "desktopMode", default)]
    pub desktop_mode: bool,
    #[serde(alias = "sensitiveInputFocused", default)]
    pub sensitive_input_focused: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

fn default_viewport_meta_type() -> String {
    "viewport_meta".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreencastFrameMsg {
    #[serde(rename = "type", default = "default_frame_type")]
    pub msg_type: String,
    pub format: String,
    pub data_base64: String,
    pub width: u32,
    pub height: u32,
    pub timestamp: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<u64>,
}

fn default_frame_type() -> String {
    "frame".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientTakeoverCommand {
    #[serde(rename = "viewport_meta")]
    ViewportMetaReq,

    #[serde(rename = "pointer_down")]
    PointerDown {
        x: f64,
        y: f64,
        #[serde(default)]
        button: u32,
    },

    #[serde(rename = "pointer_move")]
    PointerMove {
        x: f64,
        y: f64,
    },

    #[serde(rename = "pointer_up")]
    PointerUp {
        x: f64,
        y: f64,
        #[serde(default)]
        button: u32,
    },

    #[serde(rename = "pointer")]
    PointerAction {
        action: String, // "down" | "move" | "up" | "tap"
        x: f64,
        y: f64,
        #[serde(default)]
        button: u32,
    },

    #[serde(rename = "scroll")]
    Scroll {
        #[serde(default)]
        x: f64,
        #[serde(default)]
        y: f64,
        #[serde(alias = "deltaX", default)]
        delta_x: f64,
        #[serde(alias = "deltaY", default)]
        delta_y: f64,
    },

    #[serde(rename = "insert_text")]
    InsertText {
        text: String,
    },

    #[serde(rename = "key_press", alias = "key")]
    KeyPress {
        key: String,
    },

    #[serde(rename = "toggle_desktop")]
    ToggleDesktop {
        #[serde(alias = "desktop_mode")]
        desktop: bool,
    },

    #[serde(rename = "navigate")]
    Navigate {
        action: String, // "back" | "reload"
    },

    #[serde(rename = "complete")]
    Complete,

    #[serde(rename = "cancel")]
    Cancel,
}

pub struct ScreencastBridge {
    pub remote_width: u32,
    pub remote_height: u32,
    pub desktop_mode: bool,
    pub sensitive_input_focused: bool,
    pub is_active: bool,
}

impl Default for ScreencastBridge {
    fn default() -> Self {
        Self {
            remote_width: 390,
            remote_height: 844,
            desktop_mode: false,
            sensitive_input_focused: false,
            is_active: false,
        }
    }
}

pub fn get_virtual_key_code(key: &str) -> i64 {
    match key {
        "Enter" => 13,
        "Backspace" => 8,
        "Tab" => 9,
        "Escape" => 27,
        "ArrowLeft" => 37,
        "ArrowUp" => 38,
        "ArrowRight" => 39,
        "ArrowDown" => 40,
        "Delete" => 46,
        " " | "Space" => 32,
        other => {
            if let Some(ch) = other.chars().next() {
                if ch.is_ascii_alphanumeric() {
                    return ch.to_ascii_uppercase() as i64;
                }
            }
            0
        }
    }
}

impl ScreencastBridge {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&mut self, cdp: &CdpClient, max_width: u32, max_height: u32) -> BrowserResult<()> {
        let params = json!({
            "format": "jpeg",
            "quality": 68,
            "maxWidth": max_width,
            "maxHeight": max_height,
            "everyNthFrame": 1,
        });
        cdp.send_command("Page.startScreencast", Some(params), Duration::from_secs(5))?;
        self.is_active = true;
        self.remote_width = max_width;
        self.remote_height = max_height;
        Ok(())
    }

    pub fn stop(&mut self, cdp: &CdpClient) -> BrowserResult<()> {
        let _ = cdp.send_command("Page.stopScreencast", None, Duration::from_secs(3));
        self.is_active = false;
        Ok(())
    }

    pub fn build_meta(&self, url: &str, title: &str, reason: Option<String>) -> ViewportMeta {
        ViewportMeta {
            msg_type: "viewport_meta".to_string(),
            url: url.to_string(),
            title: title.to_string(),
            remote_width: self.remote_width,
            remote_height: self.remote_height,
            desktop_mode: self.desktop_mode,
            sensitive_input_focused: self.sensitive_input_focused,
            reason,
        }
    }

    pub fn process_screencast_event(&self, event: &CdpEvent) -> Option<(ScreencastFrameMsg, u64)> {
        if event.method != "Page.screencastFrame" {
            return None;
        }

        let data = event.params.get("data")?.as_str()?;
        let session_id = event.params.get("sessionId")?.as_u64()?;
        let metadata = event.params.get("metadata")?;

        let width = metadata
            .get("deviceWidth")
            .and_then(|v| v.as_u64())
            .unwrap_or(self.remote_width as u64) as u32;

        let height = metadata
            .get("deviceHeight")
            .and_then(|v| v.as_u64())
            .unwrap_or(self.remote_height as u64) as u32;

        let timestamp = metadata
            .get("timestamp")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let msg = ScreencastFrameMsg {
            msg_type: "frame".to_string(),
            format: "jpeg".to_string(),
            data_base64: data.to_string(),
            width,
            height,
            timestamp,
            session_id: Some(session_id),
        };

        Some((msg, session_id))
    }

    pub fn build_screencast_ack(session_id: u64) -> (&'static str, Value) {
        ("Page.screencastFrameAck", json!({ "sessionId": session_id }))
    }

    /// Translates client takeover commands into low-level CDP commands.
    pub fn translate_command(
        &mut self,
        cmd: &ClientTakeoverCommand,
    ) -> Vec<(&'static str, Value)> {
        let mut ops = Vec::new();

        match cmd {
            ClientTakeoverCommand::ViewportMetaReq => {}

            ClientTakeoverCommand::PointerDown { x, y, button } => {
                let btn_str = match button {
                    1 => "middle",
                    2 => "right",
                    _ => "left",
                };
                let btn_mask = match button {
                    1 => 4,
                    2 => 2,
                    _ => 1,
                };
                ops.push((
                    "Input.dispatchMouseEvent",
                    json!({
                        "type": "mousePressed",
                        "x": x,
                        "y": y,
                        "button": btn_str,
                        "buttons": btn_mask,
                        "clickCount": 1
                    }),
                ));
            }

            ClientTakeoverCommand::PointerMove { x, y } => {
                // Continuous drag (e.g. slider-CAPTCHA) uses buttons: 1
                ops.push((
                    "Input.dispatchMouseEvent",
                    json!({
                        "type": "mouseMoved",
                        "x": x,
                        "y": y,
                        "buttons": 1
                    }),
                ));
            }

            ClientTakeoverCommand::PointerUp { x, y, button } => {
                let btn_str = match button {
                    1 => "middle",
                    2 => "right",
                    _ => "left",
                };
                ops.push((
                    "Input.dispatchMouseEvent",
                    json!({
                        "type": "mouseReleased",
                        "x": x,
                        "y": y,
                        "button": btn_str,
                        "buttons": 0
                    }),
                ));
            }

            ClientTakeoverCommand::PointerAction {
                action,
                x,
                y,
                button,
            } => match action.as_str() {
                "down" => {
                    let btn_str = if *button == 2 { "right" } else { "left" };
                    let btn_mask = if *button == 2 { 2 } else { 1 };
                    ops.push((
                        "Input.dispatchMouseEvent",
                        json!({
                            "type": "mousePressed",
                            "x": x,
                            "y": y,
                            "button": btn_str,
                            "buttons": btn_mask,
                            "clickCount": 1
                        }),
                    ));
                }
                "move" => {
                    ops.push((
                        "Input.dispatchMouseEvent",
                        json!({
                            "type": "mouseMoved",
                            "x": x,
                            "y": y,
                            "buttons": 1
                        }),
                    ));
                }
                "up" => {
                    let btn_str = if *button == 2 { "right" } else { "left" };
                    ops.push((
                        "Input.dispatchMouseEvent",
                        json!({
                            "type": "mouseReleased",
                            "x": x,
                            "y": y,
                            "button": btn_str,
                            "buttons": 0
                        }),
                    ));
                }
                "tap" => {
                    ops.push((
                        "Input.dispatchMouseEvent",
                        json!({
                            "type": "mousePressed",
                            "x": x,
                            "y": y,
                            "button": "left",
                            "buttons": 1,
                            "clickCount": 1
                        }),
                    ));
                    ops.push((
                        "Input.dispatchMouseEvent",
                        json!({
                            "type": "mouseReleased",
                            "x": x,
                            "y": y,
                            "button": "left",
                            "buttons": 0
                        }),
                    ));
                }
                _ => {}
            },

            ClientTakeoverCommand::Scroll {
                x,
                y,
                delta_x,
                delta_y,
            } => {
                ops.push((
                    "Input.dispatchMouseEvent",
                    json!({
                        "type": "mouseWheel",
                        "x": x,
                        "y": y,
                        "deltaX": delta_x,
                        "deltaY": delta_y
                    }),
                ));
            }

            ClientTakeoverCommand::InsertText { text } => {
                ops.push(("Input.insertText", json!({ "text": text })));
            }

            ClientTakeoverCommand::KeyPress { key } => {
                let vkey = get_virtual_key_code(key);
                ops.push((
                    "Input.dispatchKeyEvent",
                    json!({
                        "type": "rawKeyDown",
                        "windowsVirtualKeyCode": vkey,
                        "key": key
                    }),
                ));
                ops.push((
                    "Input.dispatchKeyEvent",
                    json!({
                        "type": "keyUp",
                        "windowsVirtualKeyCode": vkey,
                        "key": key
                    }),
                ));
            }

            ClientTakeoverCommand::ToggleDesktop { desktop } => {
                self.desktop_mode = *desktop;
                if *desktop {
                    self.remote_width = 1280;
                    self.remote_height = 800;
                    ops.push((
                        "Emulation.setDeviceMetricsOverride",
                        json!({
                            "width": 1280,
                            "height": 800,
                            "deviceScaleFactor": 1,
                            "mobile": false
                        }),
                    ));
                    ops.push((
                        "Emulation.setUserAgentOverride",
                        json!({
                            "userAgent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36"
                        }),
                    ));
                } else {
                    self.remote_width = 390;
                    self.remote_height = 844;
                    ops.push((
                        "Emulation.setDeviceMetricsOverride",
                        json!({
                            "width": 390,
                            "height": 844,
                            "deviceScaleFactor": 3,
                            "mobile": true
                        }),
                    ));
                    ops.push((
                        "Emulation.setUserAgentOverride",
                        json!({
                            "userAgent": "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1"
                        }),
                    ));
                }
            }

            ClientTakeoverCommand::Navigate { action } => {
                if action == "back" {
                    ops.push((
                        "Runtime.evaluate",
                        json!({ "expression": "window.history.back()" }),
                    ));
                } else if action == "reload" {
                    ops.push(("Page.reload", json!({ "ignoreCache": false })));
                }
            }

            ClientTakeoverCommand::Complete | ClientTakeoverCommand::Cancel => {
                self.is_active = false;
                ops.push(("Page.stopScreencast", json!({})));
            }
        }

        ops
    }

    /// Dispatches a client takeover command over a CDP client.
    pub fn dispatch(
        &mut self,
        cdp: &CdpClient,
        cmd: &ClientTakeoverCommand,
    ) -> BrowserResult<()> {
        let ops = self.translate_command(cmd);
        for (method, params) in ops {
            cdp.send_command(method, Some(params), Duration::from_secs(5))?;
        }
        Ok(())
    }
}
