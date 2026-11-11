use crate::error::{BrowserError, BrowserResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const REDACTED_PASSWORD_VALUE: &str = "[REDACTED_SENSITIVE_INPUT]";

pub const DOM_INSPECTION_SCRIPT: &str = r#"
(() => {
  const elements = [];
  function isInteractive(el, style) {
    const tag = el.tagName.toLowerCase();
    if (tag === 'a' && el.hasAttribute('href')) return true;
    if (tag === 'button' || tag === 'input' || tag === 'textarea' || tag === 'select') return true;
    const role = el.getAttribute('role');
    if (role && ['button','link','tab','menuitem','checkbox','combobox','textbox','switch','radio'].includes(role)) return true;
    if (el.hasAttribute('onclick') || typeof el.onclick === 'function') return true;
    const tabindex = el.getAttribute('tabindex');
    if (tabindex !== null && parseInt(tabindex, 10) >= 0) return true;
    if (style && style.cursor === 'pointer') return true;
    return false;
  }

  function collect(node, frameOffsetX, frameOffsetY, isShadow) {
    if (!node) return;
    const all = node.querySelectorAll ? Array.from(node.querySelectorAll('*')) : [];
    for (const el of all) {
      if (el.shadowRoot) {
        collect(el.shadowRoot, frameOffsetX, frameOffsetY, true);
      }
      if (el.tagName && el.tagName.toLowerCase() === 'iframe') {
        try {
          const iframeDoc = el.contentDocument || (el.contentWindow && el.contentWindow.document);
          if (iframeDoc) {
            const rect = el.getBoundingClientRect();
            collect(iframeDoc, frameOffsetX + rect.left, frameOffsetY + rect.top, isShadow);
          }
        } catch (e) {}
      }

      const style = window.getComputedStyle ? window.getComputedStyle(el) : null;
      if (style && (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0')) {
        continue;
      }

      if (isInteractive(el, style)) {
        const rect = el.getBoundingClientRect();
        if (rect.width <= 0 || rect.height <= 0) continue;

        const cx = rect.left + rect.width / 2;
        const cy = rect.top + rect.height / 2;

        let isOccluded = false;
        if (cx >= 0 && cx <= window.innerWidth && cy >= 0 && cy <= window.innerHeight) {
          const topEl = document.elementFromPoint(cx, cy);
          if (topEl && topEl !== el && !el.contains(topEl) && !topEl.contains(el)) {
            isOccluded = true;
          }
        } else {
          isOccluded = true;
        }

        const tag = el.tagName.toLowerCase();
        const role = el.getAttribute('role') || null;
        const inputType = el.getAttribute('type') || (tag === 'textarea' ? 'textarea' : null);
        const text = (el.innerText || el.textContent || el.getAttribute('aria-label') || el.getAttribute('title') || '').trim().slice(0, 100);
        const placeholder = el.getAttribute('placeholder') || null;
        const val = (tag === 'input' || tag === 'textarea' || tag === 'select') ? (el.value || '') : null;
        const autocomplete = el.getAttribute('autocomplete') || '';
        const name = el.getAttribute('name') || '';
        const id = el.getAttribute('id') || '';
        const className = el.className || '';

        elements.push({
          tag,
          role,
          input_type: inputType,
          text,
          placeholder,
          value: val,
          center_x: frameOffsetX + cx,
          center_y: frameOffsetY + cy,
          rect_x: frameOffsetX + rect.left,
          rect_y: frameOffsetY + rect.top,
          rect_width: rect.width,
          rect_height: rect.height,
          is_shadow: isShadow,
          is_occluded: isOccluded,
          attributes: {
            id,
            name,
            class: typeof className === 'string' ? className : '',
            autocomplete,
            href: el.getAttribute('href') || '',
            src: el.getAttribute('src') || '',
            'aria-label': el.getAttribute('aria-label') || '',
          }
        });
      }
    }
  }

  collect(document, 0, 0, false);
  return {
    title: document.title || '',
    url: window.location.href || '',
    elements: elements,
  };
})()
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawElement {
    pub tag: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub input_type: Option<String>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub center_x: f64,
    #[serde(default)]
    pub center_y: f64,
    #[serde(default)]
    pub rect_x: f64,
    #[serde(default)]
    pub rect_y: f64,
    #[serde(default)]
    pub rect_width: f64,
    #[serde(default)]
    pub rect_height: f64,
    #[serde(default)]
    pub is_shadow: bool,
    #[serde(default)]
    pub is_occluded: bool,
    #[serde(default)]
    pub frame_url: Option<String>,
    #[serde(default)]
    pub frame_offset_x: f64,
    #[serde(default)]
    pub frame_offset_y: f64,
    #[serde(default)]
    pub backend_node_id: Option<i64>,
    #[serde(default)]
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveElementRef {
    pub index: u32,
    pub tag: String,
    pub role: Option<String>,
    pub input_type: Option<String>,
    pub text: String,
    pub placeholder: Option<String>,
    pub value: Option<String>,
    #[serde(default)]
    pub center_x: f64,
    #[serde(default)]
    pub center_y: f64,
    #[serde(default)]
    pub rect_x: f64,
    #[serde(default)]
    pub rect_y: f64,
    #[serde(default)]
    pub rect_width: f64,
    #[serde(default)]
    pub rect_height: f64,
    pub backend_node_id: Option<i64>,
    pub frame_url: Option<String>,
    pub frame_offset_x: f64,
    pub frame_offset_y: f64,
    pub is_shadow: bool,
    pub is_occluded: bool,
    pub is_sensitive: bool,
    pub sensitive_reason: Option<String>,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSnapshot {
    pub title: String,
    pub url: String,
    pub formatted_text: String,
    pub element_map: HashMap<u32, InteractiveElementRef>,
    pub sensitive_surface_detected: bool,
    pub sensitive_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SensitiveKind {
    Password,
    Otp,
    CreditCard,
    Captcha,
    HighRiskAction(String),
}

pub struct SensitiveSurfaceDetector;

impl SensitiveSurfaceDetector {
    pub fn detect(el: &RawElement) -> Option<SensitiveKind> {
        let _tag = el.tag.to_lowercase();
        let input_type = el.input_type.as_deref().unwrap_or("").to_lowercase();
        let text = el.text.to_lowercase();

        let id = el.attributes.get("id").map(|s| s.as_str()).unwrap_or("").to_lowercase();
        let name = el.attributes.get("name").map(|s| s.as_str()).unwrap_or("").to_lowercase();
        let class = el.attributes.get("class").map(|s| s.as_str()).unwrap_or("").to_lowercase();
        let autocomplete = el.attributes.get("autocomplete").map(|s| s.as_str()).unwrap_or("").to_lowercase();
        let placeholder = el.placeholder.as_deref().unwrap_or("").to_lowercase();
        let aria_label = el.attributes.get("aria-label").map(|s| s.as_str()).unwrap_or("").to_lowercase();
        let src = el.attributes.get("src").map(|s| s.as_str()).unwrap_or("").to_lowercase();

        // 1. Password detection
        if input_type == "password"
            || autocomplete.contains("current-password")
            || autocomplete.contains("new-password")
        {
            return Some(SensitiveKind::Password);
        }

        // 2. OTP / verification code
        if autocomplete.contains("one-time-code")
            || id.contains("otp")
            || name.contains("otp")
            || id.contains("verify_code")
            || name.contains("vcode")
            || placeholder.contains("验证码")
            || aria_label.contains("验证码")
            || text.contains("验证码")
            || placeholder.contains("verification code")
            || aria_label.contains("verification code")
        {
            return Some(SensitiveKind::Otp);
        }

        // 3. Credit Card / CVV
        if autocomplete.contains("cc-number")
            || autocomplete.contains("cc-csc")
            || autocomplete.contains("cc-exp")
            || id.contains("cvv")
            || name.contains("cvv")
            || placeholder.contains("安全码")
            || aria_label.contains("安全码")
        {
            return Some(SensitiveKind::CreditCard);
        }

        // 4. CAPTCHA challenges
        let captcha_matches = |s: &str| {
            s.contains("recaptcha")
                || s.contains("hcaptcha")
                || s.contains("turnstile")
                || s.contains("geetest")
                || s.contains("captcha")
                || s.contains("slider")
                || s.contains("人机验证")
                || s.contains("滑块验证")
        };

        if captcha_matches(&id)
            || captcha_matches(&class)
            || captcha_matches(&src)
            || captcha_matches(&text)
            || captcha_matches(&aria_label)
        {
            return Some(SensitiveKind::Captcha);
        }

        // 5. High-risk actions (payment / irreversible deletion)
        let check_high_risk = |s: &str| {
            if s.contains("pay now") || s.contains("confirm payment") || s.contains("立即支付") || s.contains("确认付款") || s.contains("去支付") {
                Some("Payment confirmation".to_string())
            } else if s.contains("delete account") || s.contains("注销账号") || s.contains("清空数据") {
                Some("Account deletion / destructive action".to_string())
            } else {
                None
            }
        };

        if let Some(reason) = check_high_risk(&text) {
            return Some(SensitiveKind::HighRiskAction(reason));
        }
        if let Some(reason) = check_high_risk(&aria_label) {
            return Some(SensitiveKind::HighRiskAction(reason));
        }

        None
    }

    /// Hard security gate: returns Err(HumanTakeoverRequired) if attempting automated LLM input into sensitive field.
    pub fn verify_automated_input_allowed(el: &InteractiveElementRef) -> BrowserResult<()> {
        if !el.is_sensitive {
            return Ok(());
        }

        let reason = el
            .sensitive_reason
            .clone()
            .unwrap_or_else(|| "Automated input blocked on protected surface".to_string());

        let takeover_card = serde_json::json!({
            "id": format!("takeover-sensitive-{}", el.index),
            "kind": "takeover",
            "title": "Human Takeover Required",
            "summary": reason,
            "risk_level": "High",
            "element_index": el.index,
            "tag": el.tag,
            "input_type": el.input_type,
        });

        Err(BrowserError::HumanTakeoverRequired {
            reason,
            takeover_card,
        })
    }
}

/// Merges raw inspection elements into a compact PageSnapshot and ElementMap.
pub fn build_page_snapshot(title: &str, url: &str, raw_elements: Vec<RawElement>) -> PageSnapshot {
    let mut element_map = HashMap::new();
    let mut lines = Vec::new();
    let mut sensitive_reasons = Vec::new();
    let mut sensitive_detected = false;

    lines.push(format!("Page: {}", title));
    lines.push(format!("URL: {}", url));
    lines.push("Interactive Elements:".to_string());

    let mut current_index = 1u32;

    for raw in raw_elements {
        let sensitive = SensitiveSurfaceDetector::detect(&raw);
        let is_sensitive = sensitive.is_some();
        let reason_str = match &sensitive {
            Some(SensitiveKind::Password) => Some("Password input field".to_string()),
            Some(SensitiveKind::Otp) => Some("One-time passcode / verification code".to_string()),
            Some(SensitiveKind::CreditCard) => Some("Credit card / payment credentials".to_string()),
            Some(SensitiveKind::Captcha) => Some("CAPTCHA / human verification challenge".to_string()),
            Some(SensitiveKind::HighRiskAction(act)) => Some(format!("High-risk action: {}", act)),
            None => None,
        };

        if let Some(ref r) = reason_str {
            sensitive_detected = true;
            if !sensitive_reasons.contains(r) {
                sensitive_reasons.push(r.clone());
            }
        }

        let el_ref = InteractiveElementRef {
            index: current_index,
            tag: raw.tag.clone(),
            role: raw.role.clone(),
            input_type: raw.input_type.clone(),
            text: raw.text.clone(),
            placeholder: raw.placeholder.clone(),
            value: if is_sensitive && raw.input_type.as_deref() == Some("password") {
                None // Never keep password value
            } else {
                raw.value.clone()
            },
            center_x: raw.center_x,
            center_y: raw.center_y,
            rect_x: raw.rect_x,
            rect_y: raw.rect_y,
            rect_width: raw.rect_width,
            rect_height: raw.rect_height,
            backend_node_id: raw.backend_node_id,
            frame_url: raw.frame_url.clone(),
            frame_offset_x: raw.frame_offset_x,
            frame_offset_y: raw.frame_offset_y,
            is_shadow: raw.is_shadow,
            is_occluded: raw.is_occluded,
            is_sensitive,
            sensitive_reason: reason_str.clone(),
            attributes: raw.attributes.clone(),
        };

        // Formats snapshot line for LLM
        if raw.is_occluded {
            // Occluded background elements marked clearly
            lines.push(format!(
                "  [{}] (occluded) <{}> \"{}\"",
                current_index, raw.tag, raw.text
            ));
        } else if is_sensitive {
            let label = if !raw.text.is_empty() {
                &raw.text
            } else if let Some(ref ph) = raw.placeholder {
                ph
            } else {
                "sensitive"
            };
            lines.push(format!(
                "  [{}] ProtectedInput <{}> \"{}\" (value: {}, gate: {})",
                current_index,
                raw.tag,
                label,
                REDACTED_PASSWORD_VALUE,
                reason_str.as_deref().unwrap_or("Sensitive")
            ));
        } else if raw.tag == "a" {
            let href = raw.attributes.get("href").map(|s| s.as_str()).unwrap_or("");
            lines.push(format!("  [{}] Link \"{}\" -> {}", current_index, raw.text, href));
        } else if raw.tag == "button" || raw.role.as_deref() == Some("button") {
            lines.push(format!("  [{}] Button \"{}\"", current_index, raw.text));
        } else if raw.tag == "input" || raw.tag == "textarea" || raw.tag == "select" {
            let val = raw.value.as_deref().unwrap_or("");
            let ph = raw.placeholder.as_deref().unwrap_or("");
            lines.push(format!(
                "  [{}] Input [{}] ph=\"{}\" val=\"{}\"",
                current_index,
                raw.input_type.as_deref().unwrap_or(&raw.tag),
                ph,
                val
            ));
        } else {
            lines.push(format!(
                "  [{}] Clickable <{}> \"{}\"",
                current_index, raw.tag, raw.text
            ));
        }

        element_map.insert(current_index, el_ref);
        current_index += 1;
    }

    PageSnapshot {
        title: title.to_string(),
        url: url.to_string(),
        formatted_text: lines.join("\n"),
        element_map,
        sensitive_surface_detected: sensitive_detected,
        sensitive_reasons,
    }
}
