#[cfg(test)]
mod tests {
    use crate::ax_snapshot::{
        build_page_snapshot, RawElement, SensitiveKind, SensitiveSurfaceDetector,
        REDACTED_PASSWORD_VALUE,
    };
    use crate::cdp::{CdpClient, CdpEvent};
    use crate::error::BrowserError;
    use crate::screencast::{ClientTakeoverCommand, ScreencastBridge};
    use crate::sessions::{
        assert_profile_download_forbidden, clear_site_session, load_sessions, record_site_visit,
    };
    use crate::url_guard::{validate_navigation_url, BrowserSecurityPolicy};
    use crate::BrowserTab;
    use serde_json::json;
    use std::collections::HashMap;
    use std::net::TcpListener;
    use std::path::Path;
    use std::sync::mpsc;
    use std::time::Duration;
    use tungstenite::Message;

    // --- Helper: Mock CDP Server ---
    fn run_mock_cdp_server<F>(handler: F) -> (String, mpsc::Sender<()>)
    where
        F: FnOnce(tungstenite::WebSocket<std::net::TcpStream>) + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral mock port");
        let local_addr = listener.local_addr().expect("local addr");
        let ws_url = format!("ws://{}", local_addr);
        let (stop_tx, stop_rx) = mpsc::channel();

        std::thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            let start = std::time::Instant::now();
            let mut stream = None;

            while start.elapsed() < Duration::from_secs(5) {
                if let Ok((s, _)) = listener.accept() {
                    s.set_nonblocking(false).unwrap();
                    stream = Some(s);
                    break;
                }
                if stop_rx.try_recv().is_ok() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(15));
            }

            if let Some(s) = stream {
                if let Ok(ws) = tungstenite::accept(s) {
                    handler(ws);
                }
            }
        });

        (ws_url, stop_tx)
    }

    // ==========================================
    // 1-4: url_guard tests
    // ==========================================

    #[test]
    fn test_url_guard_blocks_file_scheme() {
        let policy = BrowserSecurityPolicy::default();
        let res = validate_navigation_url("file:///etc/passwd", &policy);
        assert!(matches!(res, Err(BrowserError::BlockedUrlScheme(_))));
    }

    #[test]
    fn test_url_guard_blocks_javascript_scheme() {
        let policy = BrowserSecurityPolicy::default();
        let res = validate_navigation_url("javascript:alert(1)", &policy);
        assert!(matches!(res, Err(BrowserError::BlockedUrlScheme(_))));
    }

    #[test]
    fn test_url_guard_blocks_chrome_and_devtools() {
        let policy = BrowserSecurityPolicy::default();
        assert!(matches!(
            validate_navigation_url("chrome://settings", &policy),
            Err(BrowserError::BlockedUrlScheme(_))
        ));
        assert!(matches!(
            validate_navigation_url("devtools://devtools/bundled/inspector.html", &policy),
            Err(BrowserError::BlockedUrlScheme(_))
        ));
    }

    #[test]
    fn test_url_guard_blocks_loopback_by_default() {
        let policy = BrowserSecurityPolicy::default();
        let res1 = validate_navigation_url("http://127.0.0.1:8080/admin", &policy);
        assert!(matches!(res1, Err(BrowserError::SsrfBlocked(_, _))));

        let res2 = validate_navigation_url("http://localhost:3000", &policy);
        assert!(matches!(res2, Err(BrowserError::SsrfBlocked(_, _))));
    }

    #[test]
    fn test_url_guard_allows_loopback_in_test_mode() {
        let policy = BrowserSecurityPolicy::test_policy();
        let res = validate_navigation_url("http://127.0.0.1:9222/devtools", &policy);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), "http://127.0.0.1:9222/devtools");
    }

    #[test]
    fn test_url_guard_blocks_private_lan() {
        let policy = BrowserSecurityPolicy::default();
        let res_c = validate_navigation_url("http://192.168.1.1/router", &policy);
        assert!(matches!(res_c, Err(BrowserError::SsrfBlocked(_, _))));

        let res_a = validate_navigation_url("http://10.0.0.5/api", &policy);
        assert!(matches!(res_a, Err(BrowserError::SsrfBlocked(_, _))));

        let res_b = validate_navigation_url("http://172.20.0.1/dashboard", &policy);
        assert!(matches!(res_b, Err(BrowserError::SsrfBlocked(_, _))));
    }

    #[test]
    fn test_url_guard_blocks_cloud_metadata_link_local() {
        let policy = BrowserSecurityPolicy::default();
        let res = validate_navigation_url("http://169.254.169.254/latest/meta-data", &policy);
        assert!(matches!(res, Err(BrowserError::SsrfBlocked(_, _))));
    }

    #[test]
    fn test_url_guard_allows_valid_public_https() {
        let policy = BrowserSecurityPolicy::default();
        let res = validate_navigation_url("https://1.1.1.1/dns-query", &policy);
        assert!(res.is_ok());
    }

    // ==========================================
    // 5-8: cdp mock tests
    // ==========================================

    #[test]
    fn test_cdp_mock_connect_and_command_correlation() {
        let (ws_url, _stop) = run_mock_cdp_server(|mut ws| {
            // Read incoming command
            if let Ok(Message::Text(txt)) = ws.read() {
                let req: serde_json::Value = serde_json::from_str(&txt).unwrap();
                let id = req["id"].as_u64().unwrap();
                let method = req["method"].as_str().unwrap();
                assert_eq!(method, "Page.enable");

                let resp = json!({
                    "id": id,
                    "result": { "status": "ok" }
                });
                let _ = ws.send(Message::Text(resp.to_string().into()));
            }
        });

        let client = CdpClient::connect(&ws_url).expect("connect to mock cdp");
        let res = client
            .send_command("Page.enable", None, Duration::from_secs(3))
            .expect("command success");
        assert_eq!(res["status"], "ok");
        client.close();
    }

    #[test]
    fn test_cdp_mock_handles_out_of_order_events() {
        let (ws_url, _stop) = run_mock_cdp_server(|mut ws| {
            if let Ok(Message::Text(txt)) = ws.read() {
                let req: serde_json::Value = serde_json::from_str(&txt).unwrap();
                let id = req["id"].as_u64().unwrap();

                // Send event BEFORE response
                let event = json!({
                    "method": "Page.frameNavigated",
                    "params": { "frame": { "id": "frame-42", "url": "https://example.com" } }
                });
                let _ = ws.send(Message::Text(event.to_string().into()));

                // Now send command response
                let resp = json!({ "id": id, "result": { "ready": true } });
                let _ = ws.send(Message::Text(resp.to_string().into()));
            }
        });

        let client = CdpClient::connect(&ws_url).expect("connect to mock cdp");
        let res = client
            .send_command("Page.navigate", None, Duration::from_secs(3))
            .expect("command success");
        assert_eq!(res["ready"], true);

        // Wait a few ms for worker to buffer the event
        std::thread::sleep(Duration::from_millis(50));
        let events = client.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].method, "Page.frameNavigated");
        assert_eq!(events[0].params["frame"]["id"], "frame-42");
        client.close();
    }

    #[test]
    fn test_cdp_mock_target_crashed_detection() {
        let (ws_url, _stop) = run_mock_cdp_server(|mut ws| {
            let crash_event = json!({
                "method": "Inspector.targetCrashed",
                "params": {}
            });
            let _ = ws.send(Message::Text(crash_event.to_string().into()));
            // Keep connection open briefly
            std::thread::sleep(Duration::from_millis(150));
        });

        let client = CdpClient::connect(&ws_url).expect("connect to mock cdp");
        std::thread::sleep(Duration::from_millis(80));

        assert!(client.is_crashed());
        let res = client.send_command("DOM.getDocument", None, Duration::from_secs(1));
        assert!(matches!(res, Err(BrowserError::TargetCrashed(_))));
        client.close();
    }

    #[test]
    fn test_cdp_mock_enable_default_domains() {
        let (ws_url, _stop) = run_mock_cdp_server(|mut ws| {
            // Reply to 5 enable commands
            for _ in 0..5 {
                if let Ok(Message::Text(txt)) = ws.read() {
                    let req: serde_json::Value = serde_json::from_str(&txt).unwrap();
                    let id = req["id"].as_u64().unwrap();
                    let resp = json!({ "id": id, "result": {} });
                    let _ = ws.send(Message::Text(resp.to_string().into()));
                }
            }
        });

        let client = CdpClient::connect(&ws_url).expect("connect to mock cdp");
        let res = client.enable_default_domains();
        assert!(res.is_ok());
        client.close();
    }

    // ==========================================
    // 9-12: ax_snapshot tests
    // ==========================================

    #[test]
    fn test_ax_snapshot_numbers_elements() {
        let raw = vec![
            RawElement {
                tag: "button".to_string(),
                role: Some("button".to_string()),
                input_type: None,
                text: "Submit".to_string(),
                placeholder: None,
                value: None,
                center_x: 100.0,
                center_y: 50.0,
                rect_x: 80.0,
                rect_y: 40.0,
                rect_width: 40.0,
                rect_height: 20.0,
                is_shadow: false,
                is_occluded: false,
                frame_url: None,
                frame_offset_x: 0.0,
                frame_offset_y: 0.0,
                backend_node_id: None,
                attributes: HashMap::new(),
            },
            RawElement {
                tag: "a".to_string(),
                role: Some("link".to_string()),
                input_type: None,
                text: "Documentation".to_string(),
                placeholder: None,
                value: None,
                center_x: 150.0,
                center_y: 100.0,
                rect_x: 100.0,
                rect_y: 90.0,
                rect_width: 100.0,
                rect_height: 20.0,
                is_shadow: false,
                is_occluded: false,
                frame_url: None,
                frame_offset_x: 0.0,
                frame_offset_y: 0.0,
                backend_node_id: None,
                attributes: [("href".to_string(), "https://docs.rs".to_string())]
                    .into_iter()
                    .collect(),
            },
        ];

        let snapshot = build_page_snapshot("Test Page", "https://example.com", raw);
        assert_eq!(snapshot.element_map.len(), 2);
        assert_eq!(snapshot.element_map[&1].text, "Submit");
        assert_eq!(snapshot.element_map[&2].text, "Documentation");
        assert!(snapshot.formatted_text.contains("[1] Button \"Submit\""));
        assert!(snapshot.formatted_text.contains("[2] Link \"Documentation\" -> https://docs.rs"));
    }

    #[test]
    fn test_ax_snapshot_pierces_iframe_with_coordinate_offset() {
        let raw = vec![RawElement {
            tag: "button".to_string(),
            role: Some("button".to_string()),
            input_type: None,
            text: "Nested Button".to_string(),
            placeholder: None,
            value: None,
            center_x: 150.0, // 100.0 iframe offset + 50.0 local center
            center_y: 260.0, // 200.0 iframe offset + 60.0 local center
            rect_x: 130.0,
            rect_y: 250.0,
            rect_width: 40.0,
            rect_height: 20.0,
            is_shadow: false,
            is_occluded: false,
            frame_url: Some("https://example.com/iframe.html".to_string()),
            frame_offset_x: 100.0,
            frame_offset_y: 200.0,
            backend_node_id: None,
            attributes: HashMap::new(),
        }];

        let snapshot = build_page_snapshot("Iframe Page", "https://example.com", raw);
        let el = &snapshot.element_map[&1];
        assert_eq!(el.center_x, 150.0);
        assert_eq!(el.center_y, 260.0);
        assert_eq!(el.frame_offset_x, 100.0);
        assert_eq!(el.frame_offset_y, 200.0);
    }

    #[test]
    fn test_ax_snapshot_pierces_open_shadow_dom() {
        let raw = vec![RawElement {
            tag: "button".to_string(),
            role: Some("button".to_string()),
            input_type: None,
            text: "Shadow Root Control".to_string(),
            placeholder: None,
            value: None,
            center_x: 60.0,
            center_y: 80.0,
            rect_x: 40.0,
            rect_y: 70.0,
            rect_width: 40.0,
            rect_height: 20.0,
            is_shadow: true,
            is_occluded: false,
            frame_url: None,
            frame_offset_x: 0.0,
            frame_offset_y: 0.0,
            backend_node_id: None,
            attributes: HashMap::new(),
        }];

        let snapshot = build_page_snapshot("Shadow Page", "https://example.com", raw);
        assert_eq!(snapshot.element_map[&1].is_shadow, true);
        assert_eq!(snapshot.element_map[&1].text, "Shadow Root Control");
    }

    #[test]
    fn test_ax_snapshot_custom_clickable_div() {
        let mut attrs = HashMap::new();
        attrs.insert("tabindex".to_string(), "0".to_string());
        attrs.insert("onclick".to_string(), "handleClick()".to_string());

        let raw = vec![RawElement {
            tag: "div".to_string(),
            role: Some("button".to_string()),
            input_type: None,
            text: "Custom Expandable Card".to_string(),
            placeholder: None,
            value: None,
            center_x: 200.0,
            center_y: 300.0,
            rect_x: 100.0,
            rect_y: 280.0,
            rect_width: 200.0,
            rect_height: 40.0,
            is_shadow: false,
            is_occluded: false,
            frame_url: None,
            frame_offset_x: 0.0,
            frame_offset_y: 0.0,
            backend_node_id: None,
            attributes: attrs,
        }];

        let snapshot = build_page_snapshot("Card Page", "https://example.com", raw);
        assert_eq!(snapshot.element_map.len(), 1);
        assert_eq!(snapshot.element_map[&1].tag, "div");
        assert_eq!(snapshot.element_map[&1].text, "Custom Expandable Card");
    }

    #[test]
    fn test_ax_snapshot_occlusion_filter() {
        let raw = vec![
            RawElement {
                tag: "button".to_string(),
                role: Some("button".to_string()),
                input_type: None,
                text: "Background Hidden Button".to_string(),
                placeholder: None,
                value: None,
                center_x: 100.0,
                center_y: 100.0,
                rect_x: 80.0,
                rect_y: 90.0,
                rect_width: 40.0,
                rect_height: 20.0,
                is_shadow: false,
                is_occluded: true, // Covered by modal
                frame_url: None,
                frame_offset_x: 0.0,
                frame_offset_y: 0.0,
                backend_node_id: None,
                attributes: HashMap::new(),
            },
            RawElement {
                tag: "button".to_string(),
                role: Some("button".to_string()),
                input_type: None,
                text: "Modal Confirm".to_string(),
                placeholder: None,
                value: None,
                center_x: 100.0,
                center_y: 100.0,
                rect_x: 80.0,
                rect_y: 90.0,
                rect_width: 40.0,
                rect_height: 20.0,
                is_shadow: false,
                is_occluded: false,
                frame_url: None,
                frame_offset_x: 0.0,
                frame_offset_y: 0.0,
                backend_node_id: None,
                attributes: HashMap::new(),
            },
        ];

        let snapshot = build_page_snapshot("Modal Overlay", "https://example.com", raw);
        assert!(snapshot.element_map[&1].is_occluded);
        assert!(!snapshot.element_map[&2].is_occluded);
        assert!(snapshot.formatted_text.contains("[1] (occluded) <button>"));
        assert!(snapshot.formatted_text.contains("[2] Button \"Modal Confirm\""));
    }

    // ==========================================
    // 13-15: SensitiveSurfaceDetector & Blackout tests
    // ==========================================

    #[test]
    fn test_sensitive_surface_password_redaction() {
        let raw = vec![RawElement {
            tag: "input".to_string(),
            role: None,
            input_type: Some("password".to_string()),
            text: "".to_string(),
            placeholder: Some("Enter Password".to_string()),
            value: Some("super_secret_credentials_999".to_string()),
            center_x: 100.0,
            center_y: 150.0,
            rect_x: 50.0,
            rect_y: 140.0,
            rect_width: 100.0,
            rect_height: 20.0,
            is_shadow: false,
            is_occluded: false,
            frame_url: None,
            frame_offset_x: 0.0,
            frame_offset_y: 0.0,
            backend_node_id: None,
            attributes: HashMap::new(),
        }];

        let snapshot = build_page_snapshot("Login Page", "https://example.com/login", raw);
        let el = &snapshot.element_map[&1];
        assert!(el.is_sensitive);
        assert_eq!(el.value, None); // Password field value is never kept
        assert!(snapshot.formatted_text.contains(REDACTED_PASSWORD_VALUE));
        assert!(!snapshot.formatted_text.contains("super_secret_credentials_999"));
    }

    #[test]
    fn test_sensitive_surface_hard_blocks_automated_typing() {
        let mut attrs = HashMap::new();
        attrs.insert("autocomplete".to_string(), "one-time-code".to_string());

        let raw = vec![RawElement {
            tag: "input".to_string(),
            role: None,
            input_type: Some("text".to_string()),
            text: "".to_string(),
            placeholder: Some("Enter 6-digit OTP".to_string()),
            value: None,
            center_x: 100.0,
            center_y: 150.0,
            rect_x: 50.0,
            rect_y: 140.0,
            rect_width: 100.0,
            rect_height: 20.0,
            is_shadow: false,
            is_occluded: false,
            frame_url: None,
            frame_offset_x: 0.0,
            frame_offset_y: 0.0,
            backend_node_id: None,
            attributes: attrs,
        }];

        let snapshot = build_page_snapshot("2FA Verification", "https://example.com/2fa", raw);
        let el = &snapshot.element_map[&1];
        let check = SensitiveSurfaceDetector::verify_automated_input_allowed(el);

        assert!(matches!(
            check,
            Err(BrowserError::HumanTakeoverRequired { .. })
        ));
    }

    #[test]
    fn test_browser_tab_sensitive_typing_hard_block() {
        let (ws_url, _stop) = run_mock_cdp_server(|mut ws| {
            // Reply to Runtime.evaluate for take_snapshot
            if let Ok(Message::Text(txt)) = ws.read() {
                let req: serde_json::Value = serde_json::from_str(&txt).unwrap();
                let id = req["id"].as_u64().unwrap();
                let resp = json!({
                    "id": id,
                    "result": {
                        "value": {
                            "title": "Secure Login",
                            "url": "https://example.com/login",
                            "elements": [
                                {
                                    "tag": "input",
                                    "input_type": "password",
                                    "text": "",
                                    "placeholder": "Password",
                                    "value": "initial_masked",
                                    "center_x": 100.0,
                                    "center_y": 200.0,
                                    "rect_x": 50.0,
                                    "rect_y": 180.0,
                                    "rect_width": 100.0,
                                    "rect_height": 40.0,
                                    "is_shadow": false,
                                    "is_occluded": false,
                                    "attributes": {
                                        "autocomplete": "current-password"
                                    }
                                }
                            ]
                        }
                    }
                });
                let _ = ws.send(Message::Text(resp.to_string().into()));
            }
        });

        let client = CdpClient::connect(&ws_url).expect("connect to mock cdp");
        let mut tab = BrowserTab::new(client, "tab-1".to_string(), BrowserSecurityPolicy::test_policy());
        let snapshot = tab.take_snapshot().expect("take snapshot");
        assert!(snapshot.sensitive_surface_detected);

        // Attempt automated typing into element [1] (password)
        let res = tab.type_into_element(1, "secret_password_123");
        assert!(matches!(res, Err(BrowserError::HumanTakeoverRequired { .. })));
        tab.cdp.close();
    }

    #[test]
    fn test_sensitive_surface_flags_payment_and_delete_buttons() {
        let payment_btn = RawElement {
            tag: "button".to_string(),
            role: Some("button".to_string()),
            input_type: None,
            text: "立即支付".to_string(),
            placeholder: None,
            value: None,
            center_x: 100.0,
            center_y: 100.0,
            rect_x: 80.0,
            rect_y: 90.0,
            rect_width: 40.0,
            rect_height: 20.0,
            is_shadow: false,
            is_occluded: false,
            frame_url: None,
            frame_offset_x: 0.0,
            frame_offset_y: 0.0,
            backend_node_id: None,
            attributes: HashMap::new(),
        };

        let delete_btn = RawElement {
            tag: "button".to_string(),
            role: Some("button".to_string()),
            input_type: None,
            text: "Delete Account".to_string(),
            placeholder: None,
            value: None,
            center_x: 100.0,
            center_y: 100.0,
            rect_x: 80.0,
            rect_y: 90.0,
            rect_width: 40.0,
            rect_height: 20.0,
            is_shadow: false,
            is_occluded: false,
            frame_url: None,
            frame_offset_x: 0.0,
            frame_offset_y: 0.0,
            backend_node_id: None,
            attributes: HashMap::new(),
        };

        let sensitive_payment = SensitiveSurfaceDetector::detect(&payment_btn);
        let sensitive_delete = SensitiveSurfaceDetector::detect(&delete_btn);

        assert!(matches!(
            sensitive_payment,
            Some(SensitiveKind::HighRiskAction(_))
        ));
        assert!(matches!(
            sensitive_delete,
            Some(SensitiveKind::HighRiskAction(_))
        ));
    }

    // ==========================================
    // 16-19: screencast & sessions tests
    // ==========================================

    #[test]
    fn test_screencast_frame_and_ack() {
        let bridge = ScreencastBridge::new();
        let event = CdpEvent {
            method: "Page.screencastFrame".to_string(),
            params: json!({
                "data": "dGVzdC1qcGVnLWRhdGE=",
                "sessionId": 777,
                "metadata": {
                    "deviceWidth": 390,
                    "deviceHeight": 844,
                    "timestamp": 123456.78
                }
            }),
        };

        let processed = bridge.process_screencast_event(&event);
        assert!(processed.is_some());
        let (frame_msg, session_id) = processed.unwrap();
        assert_eq!(session_id, 777);
        assert_eq!(frame_msg.data_base64, "dGVzdC1qcGVnLWRhdGE=");
        assert_eq!(frame_msg.width, 390);
        assert_eq!(frame_msg.height, 844);

        let (ack_method, ack_params) = ScreencastBridge::build_screencast_ack(session_id);
        assert_eq!(ack_method, "Page.screencastFrameAck");
        assert_eq!(ack_params["sessionId"], 777);
    }

    #[test]
    fn test_screencast_slider_drag_commands() {
        let mut bridge = ScreencastBridge::new();

        // 1. Pointer down
        let down_cmd = ClientTakeoverCommand::PointerDown {
            x: 50.0,
            y: 150.0,
            button: 0,
        };
        let down_ops = bridge.translate_command(&down_cmd);
        assert_eq!(down_ops.len(), 1);
        assert_eq!(down_ops[0].0, "Input.dispatchMouseEvent");
        assert_eq!(down_ops[0].1["type"], "mousePressed");
        assert_eq!(down_ops[0].1["buttons"], 1);

        // 2. Continuous drag move
        let move_cmd = ClientTakeoverCommand::PointerMove {
            x: 180.0,
            y: 150.0,
        };
        let move_ops = bridge.translate_command(&move_cmd);
        assert_eq!(move_ops.len(), 1);
        assert_eq!(move_ops[0].0, "Input.dispatchMouseEvent");
        assert_eq!(move_ops[0].1["type"], "mouseMoved");
        assert_eq!(move_ops[0].1["buttons"], 1); // Dragging buttons flag

        // 3. Pointer up / release
        let up_cmd = ClientTakeoverCommand::PointerUp {
            x: 180.0,
            y: 150.0,
            button: 0,
        };
        let up_ops = bridge.translate_command(&up_cmd);
        assert_eq!(up_ops.len(), 1);
        assert_eq!(up_ops[0].0, "Input.dispatchMouseEvent");
        assert_eq!(up_ops[0].1["type"], "mouseReleased");
        assert_eq!(up_ops[0].1["buttons"], 0);
    }

    #[test]
    fn test_screencast_chinese_ime_insert_text() {
        let mut bridge = ScreencastBridge::new();
        let cmd = ClientTakeoverCommand::InsertText {
            text: "你好世界".to_string(),
        };
        let ops = bridge.translate_command(&cmd);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].0, "Input.insertText");
        assert_eq!(ops[0].1["text"], "你好世界");

        // Key press Enter
        let key_cmd = ClientTakeoverCommand::KeyPress {
            key: "Enter".to_string(),
        };
        let key_ops = bridge.translate_command(&key_cmd);
        assert_eq!(key_ops.len(), 2);
        assert_eq!(key_ops[0].0, "Input.dispatchKeyEvent");
        assert_eq!(key_ops[0].1["windowsVirtualKeyCode"], 13);
        assert_eq!(key_ops[1].1["windowsVirtualKeyCode"], 13);
    }

    #[test]
    fn test_screencast_toggle_desktop() {
        let mut bridge = ScreencastBridge::new();

        // Switch to desktop
        let desktop_cmd = ClientTakeoverCommand::ToggleDesktop { desktop: true };
        let desk_ops = bridge.translate_command(&desktop_cmd);
        assert_eq!(bridge.desktop_mode, true);
        assert_eq!(bridge.remote_width, 1280);
        assert_eq!(bridge.remote_height, 800);
        assert_eq!(desk_ops[0].1["width"], 1280);
        assert_eq!(desk_ops[0].1["mobile"], false);

        // Switch back to mobile
        let mobile_cmd = ClientTakeoverCommand::ToggleDesktop { desktop: false };
        let mob_ops = bridge.translate_command(&mobile_cmd);
        assert_eq!(bridge.desktop_mode, false);
        assert_eq!(bridge.remote_width, 390);
        assert_eq!(bridge.remote_height, 844);
        assert_eq!(mob_ops[0].1["width"], 390);
        assert_eq!(mob_ops[0].1["mobile"], true);
    }

    #[test]
    fn test_sessions_vault_and_cookie_clear() {
        let temp_dir = tempfile::tempdir().unwrap();
        let sessions_file = temp_dir.path().join("browser_sessions.json");

        // Record visits
        record_site_visit(&sessions_file, "github.com", "GitHub", 5, None).unwrap();
        record_site_visit(&sessions_file, "google.com", "Google", 12, None).unwrap();

        let list = load_sessions(&sessions_file).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].domain, "github.com");
        assert_eq!(list[1].domain, "google.com");

        // Clear github.com only
        clear_site_session(&sessions_file, None, "github.com").unwrap();
        let list2 = load_sessions(&sessions_file).unwrap();
        assert_eq!(list2.len(), 1);
        assert_eq!(list2[0].domain, "google.com");

        // Clear all with "*"
        clear_site_session(&sessions_file, None, "*").unwrap();
        let list3 = load_sessions(&sessions_file).unwrap();
        assert_eq!(list3.len(), 0);
    }

    #[test]
    fn test_sessions_forbids_raw_sqlite_export() {
        let bad_path = Path::new("C:/Users/test/AppData/Local/Google/Chrome/User Data/Default/Network/Cookies");
        let res = assert_profile_download_forbidden(bad_path);
        assert!(matches!(res, Err(BrowserError::RawProfileExportForbidden)));

        let safe_path = Path::new("D:/Code/project/data/report.json");
        assert!(assert_profile_download_forbidden(safe_path).is_ok());
    }
}
