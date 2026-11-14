use crate::cdp::{CdpClient, CdpEvent};
use crate::error::BrowserResult;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

pub const REDACTED_TAKEOVER_TEXT: &str = "[REDACTED]";

fn deserialize_pointer_button<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let val = Option::<Value>::deserialize(deserializer)?;
    match val {
        None | Some(Value::Null) => Ok(0),
        Some(Value::Number(n)) => Ok(n.as_u64().unwrap_or(0) as u32),
        Some(Value::String(s)) => match s.trim().to_ascii_lowercase().as_str() {
            "middle" | "1" => Ok(1),
            "right" | "2" => Ok(2),
            _ => Ok(0),
        },
        _ => Ok(0),
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

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
enum WireTakeoverCommand {
    #[serde(rename = "viewport_meta")]
    ViewportMetaReq,

    #[serde(rename = "pointer_down")]
    PointerDown {
        x: f64,
        y: f64,
        #[serde(default, deserialize_with = "deserialize_pointer_button")]
        button: u32,
    },

    #[serde(rename = "pointer_move")]
    PointerMove { x: f64, y: f64 },

    #[serde(rename = "pointer_up")]
    PointerUp {
        x: f64,
        y: f64,
        #[serde(default, deserialize_with = "deserialize_pointer_button")]
        button: u32,
    },

    #[serde(rename = "pointer")]
    PointerAction {
        action: String,
        x: f64,
        y: f64,
        #[serde(default, deserialize_with = "deserialize_pointer_button")]
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
        #[serde(default, skip_serializing_if = "is_false")]
        masked: bool,
    },

    #[serde(rename = "ime_set_composition", alias = "imeSetComposition")]
    ImeSetComposition {
        text: String,
        #[serde(default, alias = "selectionStart", skip_serializing_if = "Option::is_none")]
        selection_start: Option<i32>,
        #[serde(default, alias = "selectionEnd", skip_serializing_if = "Option::is_none")]
        selection_end: Option<i32>,
        #[serde(default, skip_serializing_if = "is_false")]
        masked: bool,
    },

    #[serde(rename = "ime_commit", alias = "imeCommit")]
    ImeCommit {
        text: String,
        #[serde(default, skip_serializing_if = "is_false")]
        masked: bool,
    },

    #[serde(rename = "key_press", alias = "key")]
    KeyPress { key: String },

    #[serde(rename = "toggle_desktop")]
    ToggleDesktop {
        #[serde(alias = "desktop_mode")]
        desktop: bool,
    },

    #[serde(rename = "navigate")]
    Navigate { action: String },

    #[serde(rename = "complete")]
    Complete,

    #[serde(rename = "cancel")]
    Cancel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(from = "WireTakeoverCommand", into = "WireTakeoverCommand")]
pub enum ClientTakeoverCommand {
    ViewportMetaReq,

    PointerDown {
        x: f64,
        y: f64,
        button: u32,
    },

    PointerMove {
        x: f64,
        y: f64,
    },

    PointerUp {
        x: f64,
        y: f64,
        button: u32,
    },

    PointerAction {
        action: String, // "down" | "move" | "up" | "tap"
        x: f64,
        y: f64,
        button: u32,
    },

    Scroll {
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
    },

    InsertText {
        text: String,
    },

    InsertMaskedText {
        text: String,
    },

    ImeSetComposition {
        text: String,
        selection_start: Option<i32>,
        selection_end: Option<i32>,
        masked: bool,
    },

    ImeCommit {
        text: String,
        masked: bool,
    },

    KeyPress {
        key: String,
    },

    ToggleDesktop {
        desktop: bool,
    },

    Navigate {
        action: String, // "back" | "reload" | URL
    },

    Complete,

    Cancel,
}

impl From<WireTakeoverCommand> for ClientTakeoverCommand {
    fn from(wire: WireTakeoverCommand) -> Self {
        match wire {
            WireTakeoverCommand::ViewportMetaReq => Self::ViewportMetaReq,
            WireTakeoverCommand::PointerDown { x, y, button } => {
                Self::PointerDown { x, y, button }
            }
            WireTakeoverCommand::PointerMove { x, y } => Self::PointerMove { x, y },
            WireTakeoverCommand::PointerUp { x, y, button } => Self::PointerUp { x, y, button },
            WireTakeoverCommand::PointerAction {
                action,
                x,
                y,
                button,
            } => Self::PointerAction {
                action,
                x,
                y,
                button,
            },
            WireTakeoverCommand::Scroll {
                x,
                y,
                delta_x,
                delta_y,
            } => Self::Scroll {
                x,
                y,
                delta_x,
                delta_y,
            },
            WireTakeoverCommand::InsertText { text, masked } => {
                if masked {
                    Self::InsertMaskedText { text }
                } else {
                    Self::InsertText { text }
                }
            }
            WireTakeoverCommand::ImeSetComposition {
                text,
                selection_start,
                selection_end,
                masked,
            } => Self::ImeSetComposition {
                text,
                selection_start,
                selection_end,
                masked,
            },
            WireTakeoverCommand::ImeCommit { text, masked } => Self::ImeCommit { text, masked },
            WireTakeoverCommand::KeyPress { key } => Self::KeyPress { key },
            WireTakeoverCommand::ToggleDesktop { desktop } => Self::ToggleDesktop { desktop },
            WireTakeoverCommand::Navigate { action } => Self::Navigate { action },
            WireTakeoverCommand::Complete => Self::Complete,
            WireTakeoverCommand::Cancel => Self::Cancel,
        }
    }
}

impl From<ClientTakeoverCommand> for WireTakeoverCommand {
    fn from(cmd: ClientTakeoverCommand) -> Self {
        match cmd {
            ClientTakeoverCommand::ViewportMetaReq => Self::ViewportMetaReq,
            ClientTakeoverCommand::PointerDown { x, y, button } => {
                Self::PointerDown { x, y, button }
            }
            ClientTakeoverCommand::PointerMove { x, y } => Self::PointerMove { x, y },
            ClientTakeoverCommand::PointerUp { x, y, button } => Self::PointerUp { x, y, button },
            ClientTakeoverCommand::PointerAction {
                action,
                x,
                y,
                button,
            } => Self::PointerAction {
                action,
                x,
                y,
                button,
            },
            ClientTakeoverCommand::Scroll {
                x,
                y,
                delta_x,
                delta_y,
            } => Self::Scroll {
                x,
                y,
                delta_x,
                delta_y,
            },
            ClientTakeoverCommand::InsertText { text } => Self::InsertText {
                text,
                masked: false,
            },
            ClientTakeoverCommand::InsertMaskedText { text } => Self::InsertText {
                text,
                masked: true,
            },
            ClientTakeoverCommand::ImeSetComposition {
                text,
                selection_start,
                selection_end,
                masked,
            } => Self::ImeSetComposition {
                text,
                selection_start,
                selection_end,
                masked,
            },
            ClientTakeoverCommand::ImeCommit { text, masked } => Self::ImeCommit { text, masked },
            ClientTakeoverCommand::KeyPress { key } => Self::KeyPress { key },
            ClientTakeoverCommand::ToggleDesktop { desktop } => Self::ToggleDesktop { desktop },
            ClientTakeoverCommand::Navigate { action } => Self::Navigate { action },
            ClientTakeoverCommand::Complete => Self::Complete,
            ClientTakeoverCommand::Cancel => Self::Cancel,
        }
    }
}

pub type TakeoverCommand = ClientTakeoverCommand;

pub struct ScreencastBridge {
    pub remote_width: u32,
    pub remote_height: u32,
    pub desktop_mode: bool,
    pub sensitive_input_focused: bool,
    pub is_active: bool,
    pub is_pointer_down: bool,
    pub recorded_commands: Vec<ClientTakeoverCommand>,
}

pub type ScreencastSession = ScreencastBridge;

impl Default for ScreencastBridge {
    fn default() -> Self {
        Self {
            remote_width: 390,
            remote_height: 844,
            desktop_mode: false,
            sensitive_input_focused: false,
            is_active: false,
            is_pointer_down: false,
            recorded_commands: Vec::new(),
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

/// Converts a slice of recorded takeover commands into structured replayable routine steps
/// (`open`, `click_xy`, `drag_xy`, `type_text`, `scroll`, `key`), skipping any masked/redacted inputs.
pub fn export_commands_to_routine_steps(
    commands: &[ClientTakeoverCommand],
    initial_url: Option<&str>,
) -> Vec<Value> {
    let mut steps = Vec::new();

    if let Some(url) = initial_url {
        let trimmed = url.trim();
        if !trimmed.is_empty() {
            steps.push(json!({
                "action": "open",
                "url": trimmed,
            }));
        }
    }

    let mut active_drag: Option<(f64, f64, f64, f64, bool)> = None;

    let flush_drag = |drag: &mut Option<(f64, f64, f64, f64, bool)>,
                      end_x: f64,
                      end_y: f64,
                      out: &mut Vec<Value>| {
        if let Some((sx, sy, _, _, moved)) = drag.take() {
            let dist = ((end_x - sx).powi(2) + (end_y - sy).powi(2)).sqrt();
            if moved && dist >= 4.0 {
                out.push(json!({
                    "action": "drag_xy",
                    "from_x": sx,
                    "from_y": sy,
                    "to_x": end_x,
                    "to_y": end_y,
                }));
            } else {
                out.push(json!({
                    "action": "click_xy",
                    "x": sx,
                    "y": sy,
                }));
            }
        }
    };

    for cmd in commands {
        match cmd {
            ClientTakeoverCommand::PointerDown { x, y, .. } => {
                active_drag = Some((*x, *y, *x, *y, false));
            }
            ClientTakeoverCommand::PointerMove { x, y } => {
                if let Some((sx, sy, _, _, _)) = active_drag {
                    active_drag = Some((sx, sy, *x, *y, true));
                }
            }
            ClientTakeoverCommand::PointerUp { x, y, .. } => {
                flush_drag(&mut active_drag, *x, *y, &mut steps);
            }
            ClientTakeoverCommand::PointerAction { action, x, y, .. } => match action.as_str() {
                "down" => {
                    active_drag = Some((*x, *y, *x, *y, false));
                }
                "move" => {
                    if let Some((sx, sy, _, _, _)) = active_drag {
                        active_drag = Some((sx, sy, *x, *y, true));
                    }
                }
                "up" => {
                    flush_drag(&mut active_drag, *x, *y, &mut steps);
                }
                "tap" => {
                    active_drag = None;
                    steps.push(json!({
                        "action": "click_xy",
                        "x": x,
                        "y": y,
                    }));
                }
                _ => {}
            },
            ClientTakeoverCommand::InsertText { text } => {
                if text != REDACTED_TAKEOVER_TEXT && !text.is_empty() {
                    steps.push(json!({
                        "action": "type_text",
                        "text": text,
                    }));
                }
            }
            ClientTakeoverCommand::InsertMaskedText { .. } => {
                // Masked inputs (passwords/OTPs) never persist into replayable routines.
            }
            ClientTakeoverCommand::ImeCommit { text, masked } => {
                if !*masked && text != REDACTED_TAKEOVER_TEXT && !text.is_empty() {
                    steps.push(json!({
                        "action": "type_text",
                        "text": text,
                    }));
                }
            }
            ClientTakeoverCommand::ImeSetComposition { .. } => {
                // Pre-commit composition states are transient; final text is captured on ImeCommit.
            }
            ClientTakeoverCommand::Scroll {
                x,
                y,
                delta_x,
                delta_y,
            } => {
                steps.push(json!({
                    "action": "scroll",
                    "x": x,
                    "y": y,
                    "delta_x": delta_x,
                    "delta_y": delta_y,
                }));
            }
            ClientTakeoverCommand::KeyPress { key } => {
                steps.push(json!({
                    "action": "key",
                    "key": key,
                }));
            }
            ClientTakeoverCommand::Navigate { action } => {
                if action.starts_with("http://") || action.starts_with("https://") {
                    steps.push(json!({
                        "action": "open",
                        "url": action,
                    }));
                } else {
                    steps.push(json!({
                        "action": "open",
                        "navigation": action,
                    }));
                }
            }
            ClientTakeoverCommand::ViewportMetaReq
            | ClientTakeoverCommand::ToggleDesktop { .. }
            | ClientTakeoverCommand::Complete
            | ClientTakeoverCommand::Cancel => {}
        }
    }

    steps
}

impl ScreencastBridge {
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies privacy redaction for sensitive or masked inputs before persisting into `recorded_commands`.
    pub fn sanitize_for_recording(&self, cmd: &ClientTakeoverCommand) -> ClientTakeoverCommand {
        let force_redact = self.sensitive_input_focused;
        match cmd {
            ClientTakeoverCommand::InsertText { text } => {
                if force_redact {
                    ClientTakeoverCommand::InsertMaskedText {
                        text: REDACTED_TAKEOVER_TEXT.to_string(),
                    }
                } else {
                    ClientTakeoverCommand::InsertText { text: text.clone() }
                }
            }
            ClientTakeoverCommand::InsertMaskedText { .. } => {
                ClientTakeoverCommand::InsertMaskedText {
                    text: REDACTED_TAKEOVER_TEXT.to_string(),
                }
            }
            ClientTakeoverCommand::ImeSetComposition {
                text,
                selection_start,
                selection_end,
                masked,
            } => {
                if *masked || force_redact {
                    ClientTakeoverCommand::ImeSetComposition {
                        text: REDACTED_TAKEOVER_TEXT.to_string(),
                        selection_start: *selection_start,
                        selection_end: *selection_end,
                        masked: true,
                    }
                } else {
                    ClientTakeoverCommand::ImeSetComposition {
                        text: text.clone(),
                        selection_start: *selection_start,
                        selection_end: *selection_end,
                        masked: false,
                    }
                }
            }
            ClientTakeoverCommand::ImeCommit { text, masked } => {
                if *masked || force_redact {
                    ClientTakeoverCommand::ImeCommit {
                        text: REDACTED_TAKEOVER_TEXT.to_string(),
                        masked: true,
                    }
                } else {
                    ClientTakeoverCommand::ImeCommit {
                        text: text.clone(),
                        masked: false,
                    }
                }
            }
            other => other.clone(),
        }
    }

    pub fn record_command(&mut self, cmd: &ClientTakeoverCommand) -> ClientTakeoverCommand {
        let sanitized = self.sanitize_for_recording(cmd);
        self.recorded_commands.push(sanitized.clone());
        sanitized
    }

    pub fn export_routine_steps(&self, initial_url: Option<&str>) -> Vec<Value> {
        export_commands_to_routine_steps(&self.recorded_commands, initial_url)
    }

    pub fn start(&mut self, cdp: &CdpClient, max_width: u32, max_height: u32) -> BrowserResult<()> {
        if !self.desktop_mode {
            let _ = cdp.send_command(
                "Emulation.setDeviceMetricsOverride",
                Some(json!({
                    "width": max_width,
                    "height": max_height,
                    "deviceScaleFactor": 2,
                    "mobile": true
                })),
                Duration::from_secs(3),
            );
        }
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
        self.is_pointer_down = false;
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

    /// Translates client takeover commands into low-level CDP commands while recording privacy-sanitized steps.
    pub fn translate_command(
        &mut self,
        cmd: &ClientTakeoverCommand,
    ) -> Vec<(&'static str, Value)> {
        self.record_command(cmd);
        let mut ops = Vec::new();

        match cmd {
            ClientTakeoverCommand::ViewportMetaReq => {}

            ClientTakeoverCommand::PointerDown { x, y, button } => {
                self.is_pointer_down = true;
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
                let button = if self.is_pointer_down { "left" } else { "none" };
                let buttons = if self.is_pointer_down { 1 } else { 0 };
                ops.push((
                    "Input.dispatchMouseEvent",
                    json!({
                        "type": "mouseMoved",
                        "x": x,
                        "y": y,
                        "button": button,
                        "buttons": buttons
                    }),
                ));
            }

            ClientTakeoverCommand::PointerUp { x, y, button } => {
                self.is_pointer_down = false;
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
                        "buttons": 0,
                        "clickCount": 1
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
                    self.is_pointer_down = true;
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
                    let btn_str = if self.is_pointer_down { "left" } else { "none" };
                    let btn_mask = if self.is_pointer_down { 1 } else { 0 };
                    ops.push((
                        "Input.dispatchMouseEvent",
                        json!({
                            "type": "mouseMoved",
                            "x": x,
                            "y": y,
                            "button": btn_str,
                            "buttons": btn_mask
                        }),
                    ));
                }
                "up" => {
                    self.is_pointer_down = false;
                    let btn_str = if *button == 2 { "right" } else { "left" };
                    ops.push((
                        "Input.dispatchMouseEvent",
                        json!({
                            "type": "mouseReleased",
                            "x": x,
                            "y": y,
                            "button": btn_str,
                            "buttons": 0,
                            "clickCount": 1
                        }),
                    ));
                }
                "tap" => {
                    self.is_pointer_down = false;
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
                            "buttons": 0,
                            "clickCount": 1
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

            ClientTakeoverCommand::InsertText { text }
            | ClientTakeoverCommand::InsertMaskedText { text } => {
                ops.push(("Input.insertText", json!({ "text": text })));
            }

            ClientTakeoverCommand::ImeSetComposition {
                text,
                selection_start,
                selection_end,
                ..
            } => {
                let default_pos = text.encode_utf16().count() as i32;
                let start = selection_start.unwrap_or(default_pos);
                let end = selection_end.unwrap_or(default_pos);
                ops.push((
                    "Input.imeSetComposition",
                    json!({
                        "text": text,
                        "selectionStart": start,
                        "selectionEnd": end
                    }),
                ));
            }

            ClientTakeoverCommand::ImeCommit { text, .. } => {
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
                } else if action.starts_with("http://") || action.starts_with("https://") {
                    ops.push(("Page.navigate", json!({ "url": action })));
                }
            }

            ClientTakeoverCommand::Complete | ClientTakeoverCommand::Cancel => {
                self.is_active = false;
                self.is_pointer_down = false;
                ops.push(("Page.stopScreencast", json!({})));
            }
        }

        ops
    }

    /// Forces Chromium's SQLitePersistentCookieStore to flush buffered HTTP cookies to disk
    /// so session cookies survive an unexpected container restart or browser crash.
    pub fn flush_cookies_to_disk(&self, cdp: &CdpClient) {
        let flush_js = r#"(() => {
            try {
                if (location.protocol === 'http:' || location.protocol === 'https:') {
                    for (let i = 0; i < 520; i++) {
                        document.cookie = '__bob_cflush=' + i + '; max-age=1; path=/__bob_cflush';
                    }
                    document.cookie = '__bob_cflush=; max-age=0; path=/__bob_cflush';
                }
            } catch (_) {}
            return true;
        })()"#;
        let _ = cdp.evaluate(flush_js);
    }

    /// Dispatches a client takeover command over a CDP client.
    pub fn dispatch(
        &mut self,
        cdp: &CdpClient,
        cmd: &ClientTakeoverCommand,
    ) -> BrowserResult<()> {
        if matches!(cmd, ClientTakeoverCommand::Complete) {
            self.flush_cookies_to_disk(cdp);
        }
        let ops = self.translate_command(cmd);
        for (method, params) in ops {
            cdp.send_command(method, Some(params), Duration::from_secs(5))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod screencast_unit_tests {
    use super::*;

    #[test]
    fn test_pointer_down_move_up_tracks_drag_buttons_state() {
        let mut session = ScreencastSession::new();
        assert!(!session.is_pointer_down);

        // Hover move before pointer down -> button: "none", buttons: 0
        let hover_ops = session.translate_command(&TakeoverCommand::PointerMove { x: 10.0, y: 20.0 });
        assert_eq!(hover_ops.len(), 1);
        assert_eq!(hover_ops[0].1["button"], "none");
        assert_eq!(hover_ops[0].1["buttons"], 0);

        // Pointer down -> button: "left", buttons: 1
        let down_ops = session.translate_command(&TakeoverCommand::PointerDown {
            x: 30.0,
            y: 120.0,
            button: 0,
        });
        assert!(session.is_pointer_down);
        assert_eq!(down_ops[0].1["type"], "mousePressed");
        assert_eq!(down_ops[0].1["button"], "left");
        assert_eq!(down_ops[0].1["buttons"], 1);
        assert_eq!(down_ops[0].1["clickCount"], 1);

        // Drag move while pointer down -> button: "left", buttons: 1
        let move_ops = session.translate_command(&TakeoverCommand::PointerMove {
            x: 240.0,
            y: 120.0,
        });
        assert_eq!(move_ops[0].1["type"], "mouseMoved");
        assert_eq!(move_ops[0].1["button"], "left");
        assert_eq!(move_ops[0].1["buttons"], 1);

        // Pointer up -> button: "left", buttons: 0, clickCount: 1
        let up_ops = session.translate_command(&TakeoverCommand::PointerUp {
            x: 240.0,
            y: 120.0,
            button: 0,
        });
        assert!(!session.is_pointer_down);
        assert_eq!(up_ops[0].1["type"], "mouseReleased");
        assert_eq!(up_ops[0].1["button"], "left");
        assert_eq!(up_ops[0].1["buttons"], 0);
        assert_eq!(up_ops[0].1["clickCount"], 1);
    }

    #[test]
    fn test_ime_composition_privacy_redaction_and_routine_export() {
        let mut session = ScreencastSession::new();

        // Non-masked IME composition + commit
        let comp_ops = session.translate_command(&TakeoverCommand::ImeSetComposition {
            text: "nihao".to_string(),
            selection_start: Some(0),
            selection_end: Some(5),
            masked: false,
        });
        assert_eq!(comp_ops[0].0, "Input.imeSetComposition");
        assert_eq!(comp_ops[0].1["text"], "nihao");
        assert_eq!(comp_ops[0].1["selectionStart"], 0);
        assert_eq!(comp_ops[0].1["selectionEnd"], 5);

        let commit_ops = session.translate_command(&TakeoverCommand::ImeCommit {
            text: "你好世界".to_string(),
            masked: false,
        });
        assert_eq!(commit_ops[0].0, "Input.insertText");
        assert_eq!(commit_ops[0].1["text"], "你好世界");

        // Masked insert_text via JSON wire format
        let masked_insert: TakeoverCommand = serde_json::from_value(json!({
            "type": "insert_text",
            "text": "OtpCode889900",
            "masked": true
        }))
        .expect("deserialize masked insert_text");
        let masked_ops = session.translate_command(&masked_insert);
        assert_eq!(masked_ops[0].1["text"], "OtpCode889900");

        // Masked IME commit
        let _ = session.translate_command(&TakeoverCommand::ImeCommit {
            text: "MySecretPassword!".to_string(),
            masked: true,
        });

        // Click and drag
        let _ = session.translate_command(&TakeoverCommand::PointerDown {
            x: 20.0,
            y: 40.0,
            button: 0,
        });
        let _ = session.translate_command(&TakeoverCommand::PointerUp {
            x: 20.0,
            y: 40.0,
            button: 0,
        });

        let _ = session.translate_command(&TakeoverCommand::PointerDown {
            x: 30.0,
            y: 200.0,
            button: 0,
        });
        let _ = session.translate_command(&TakeoverCommand::PointerMove {
            x: 280.0,
            y: 200.0,
        });
        let _ = session.translate_command(&TakeoverCommand::PointerUp {
            x: 280.0,
            y: 200.0,
            button: 0,
        });

        let _ = session.translate_command(&TakeoverCommand::Scroll {
            x: 0.0,
            y: 0.0,
            delta_x: 0.0,
            delta_y: 320.0,
        });
        let _ = session.translate_command(&TakeoverCommand::KeyPress {
            key: "Enter".to_string(),
        });

        // Verify recorded_commands never store plaintext secrets
        let recorded_json = serde_json::to_string(&session.recorded_commands).unwrap();
        assert!(!recorded_json.contains("OtpCode889900"));
        assert!(!recorded_json.contains("MySecretPassword!"));
        assert!(recorded_json.contains(REDACTED_TAKEOVER_TEXT));

        // Verify exported routine steps
        let steps = session.export_routine_steps(Some("https://bob.example.com/portal"));
        let steps_json = serde_json::to_string(&steps).unwrap();
        assert!(!steps_json.contains("OtpCode889900"));
        assert!(!steps_json.contains("MySecretPassword!"));
        assert!(!steps_json.contains(REDACTED_TAKEOVER_TEXT));

        assert_eq!(steps[0]["action"], "open");
        assert_eq!(steps[0]["url"], "https://bob.example.com/portal");
        assert_eq!(steps[1]["action"], "type_text");
        assert_eq!(steps[1]["text"], "你好世界");
        assert_eq!(steps[2]["action"], "click_xy");
        assert_eq!(steps[3]["action"], "drag_xy");
        assert_eq!(steps[4]["action"], "scroll");
        assert_eq!(steps[5]["action"], "key");
        assert_eq!(steps[5]["key"], "Enter");
    }
}
