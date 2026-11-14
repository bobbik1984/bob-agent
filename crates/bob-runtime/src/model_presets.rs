//! Authoritative Dual-Model Presets (`WP-E1.2` & `WP-E1.3`).
//!
//! Maps each supported LLM provider to a validated `(clerk_model, maker_model)` pair
//! backed exclusively by `EMBEDDED_MODEL_REGISTRY` (`model_providers.json`).

use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderDualPreset {
    pub provider_id: String,
    pub display_name: String,
    pub provider_kind: String,
    pub base_url: String,
    pub clerk_model: String,
    pub clerk_display_name: String,
    pub maker_model: String,
    pub maker_display_name: String,
    pub recommended: bool,
    pub region_hint: String,
}

impl ProviderDualPreset {
    pub fn to_json_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

impl Serialize for ProviderDualPreset {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(22))?;
        map.serialize_entry("id", &self.provider_id)?;
        map.serialize_entry("provider_id", &self.provider_id)?;
        map.serialize_entry("providerId", &self.provider_id)?;
        map.serialize_entry("name", &self.display_name)?;
        map.serialize_entry("display_name", &self.display_name)?;
        map.serialize_entry("displayName", &self.display_name)?;
        map.serialize_entry("provider_kind", &self.provider_kind)?;
        map.serialize_entry("providerKind", &self.provider_kind)?;
        map.serialize_entry("base_url", &self.base_url)?;
        map.serialize_entry("baseUrl", &self.base_url)?;
        map.serialize_entry("clerk_model", &self.clerk_model)?;
        map.serialize_entry("clerkModel", &self.clerk_model)?;
        map.serialize_entry("clerk_display_name", &self.clerk_display_name)?;
        map.serialize_entry("clerkDisplayName", &self.clerk_display_name)?;
        map.serialize_entry("maker_model", &self.maker_model)?;
        map.serialize_entry("makerModel", &self.maker_model)?;
        map.serialize_entry("maker_display_name", &self.maker_display_name)?;
        map.serialize_entry("makerDisplayName", &self.maker_display_name)?;
        map.serialize_entry("recommended", &self.recommended)?;
        map.serialize_entry("featured", &self.recommended)?;
        map.serialize_entry("region_hint", &self.region_hint)?;
        map.serialize_entry("regionHint", &self.region_hint)?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for ProviderDualPreset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = Value::deserialize(deserializer)?;
        let get_str = |snake: &str, camel: &str| -> Result<String, D::Error> {
            v.get(snake)
                .or_else(|| v.get(camel))
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| serde::de::Error::custom(format!("missing field {snake}")))
        };
        let recommended = v
            .get("recommended")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Ok(Self {
            provider_id: get_str("provider_id", "providerId")?,
            display_name: get_str("display_name", "displayName")?,
            provider_kind: get_str("provider_kind", "providerKind")?,
            base_url: get_str("base_url", "baseUrl")?,
            clerk_model: get_str("clerk_model", "clerkModel")?,
            clerk_display_name: get_str("clerk_display_name", "clerkDisplayName")?,
            maker_model: get_str("maker_model", "makerModel")?,
            maker_display_name: get_str("maker_display_name", "makerDisplayName")?,
            recommended,
            region_hint: get_str("region_hint", "regionHint")?,
        })
    }
}

struct CuratedPresetDef {
    provider_id: &'static str,
    clerk_model: &'static str,
    maker_model: &'static str,
    recommended: bool,
    region_hint: &'static str,
}

const CURATED_PRESET_DEFS: &[CuratedPresetDef] = &[
    CuratedPresetDef {
        provider_id: "deepseek",
        clerk_model: "deepseek-v4.1-flash-1017",
        maker_model: "deepseek-v4-pro",
        recommended: true,
        region_hint: "cn",
    },
    CuratedPresetDef {
        provider_id: "gemini",
        clerk_model: "models/gemini-2.5-flash",
        maker_model: "models/gemini-2.5-pro",
        recommended: true,
        region_hint: "global",
    },
    CuratedPresetDef {
        provider_id: "openai",
        clerk_model: "gpt-4o-mini",
        maker_model: "gpt-4o",
        recommended: true,
        region_hint: "global",
    },
    CuratedPresetDef {
        provider_id: "anthropic",
        clerk_model: "claude-haiku-4-5",
        maker_model: "claude-3-7-sonnet-20250219",
        recommended: true,
        region_hint: "global",
    },
    CuratedPresetDef {
        provider_id: "openrouter",
        clerk_model: "deepseek/deepseek-v4.1-flash",
        maker_model: "anthropic/claude-fable-5.1",
        recommended: true,
        region_hint: "global",
    },
    CuratedPresetDef {
        provider_id: "qwen",
        clerk_model: "qwen3.8-flash",
        maker_model: "qwen3.8-max",
        recommended: false,
        region_hint: "cn",
    },
    CuratedPresetDef {
        provider_id: "zhipu",
        clerk_model: "glm-5.3-flash",
        maker_model: "glm-5.3",
        recommended: false,
        region_hint: "cn",
    },
    CuratedPresetDef {
        provider_id: "kimi",
        clerk_model: "kimi-k2.6",
        maker_model: "kimi-k3.1",
        recommended: false,
        region_hint: "cn",
    },
    CuratedPresetDef {
        provider_id: "doubao",
        clerk_model: "doubao-seed-1-6-flash-250828",
        maker_model: "doubao-seed-2-0-pro-260215",
        recommended: false,
        region_hint: "cn",
    },
    CuratedPresetDef {
        provider_id: "minimax",
        clerk_model: "MiniMax-M2.7",
        maker_model: "MiniMax-M3",
        recommended: false,
        region_hint: "cn",
    },
];

pub fn embedded_registry_value() -> Value {
    serde_json::from_str(crate::EMBEDDED_MODEL_REGISTRY)
        .unwrap_or_else(|_| serde_json::json!({ "providers": [] }))
}

/// Validates that every curated preset provider and its clerk/maker models exist in `registry`.
pub fn validate_presets_against_registry(registry: &Value) -> Result<Vec<ProviderDualPreset>, String> {
    let providers = registry
        .get("providers")
        .and_then(Value::as_array)
        .ok_or_else(|| "EMBEDDED_REGISTRY_MISSING_PROVIDERS".to_string())?;

    let mut resolved = Vec::with_capacity(CURATED_PRESET_DEFS.len());
    for def in CURATED_PRESET_DEFS {
        let prov = providers
            .iter()
            .find(|p| p.get("id").and_then(Value::as_str) == Some(def.provider_id))
            .ok_or_else(|| format!("PRESET_PROVIDER_MISSING_IN_REGISTRY: {}", def.provider_id))?;

        let display_name = prov
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(def.provider_id)
            .to_owned();
        let base_url = prov
            .get("base_url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let provider_kind = crate::infer_provider_protocol(def.provider_id, &base_url).to_owned();

        let models = prov
            .get("models")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("PRESET_MODELS_MISSING_FOR_PROVIDER: {}", def.provider_id))?;

        let clerk_entry = models
            .iter()
            .find(|m| m.get("id").and_then(Value::as_str) == Some(def.clerk_model))
            .ok_or_else(|| {
                format!(
                    "PRESET_CLERK_MODEL_MISSING: {}::{}",
                    def.provider_id, def.clerk_model
                )
            })?;
        let maker_entry = models
            .iter()
            .find(|m| m.get("id").and_then(Value::as_str) == Some(def.maker_model))
            .ok_or_else(|| {
                format!(
                    "PRESET_MAKER_MODEL_MISSING: {}::{}",
                    def.provider_id, def.maker_model
                )
            })?;

        let clerk_display_name = clerk_entry
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(def.clerk_model)
            .to_owned();
        let maker_display_name = maker_entry
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(def.maker_model)
            .to_owned();

        resolved.push(ProviderDualPreset {
            provider_id: def.provider_id.to_owned(),
            display_name,
            provider_kind,
            base_url,
            clerk_model: def.clerk_model.to_owned(),
            clerk_display_name,
            maker_model: def.maker_model.to_owned(),
            maker_display_name,
            recommended: def.recommended,
            region_hint: def.region_hint.to_owned(),
        });
    }

    Ok(resolved)
}

/// Lists all curated dual-model presets validated against `EMBEDDED_MODEL_REGISTRY`.
pub fn list_dual_presets() -> Vec<ProviderDualPreset> {
    let reg = embedded_registry_value();
    validate_presets_against_registry(&reg).unwrap_or_default()
}

/// Lists all curated dual-model presets as JSON objects (including snake_case + camelCase fields).
pub fn list_dual_presets_json() -> Vec<Value> {
    list_dual_presets()
        .into_iter()
        .map(|p| p.to_json_value())
        .collect()
}

/// Looks up a single curated dual-model preset by `provider_id`.
pub fn find_dual_preset(provider_id: &str) -> Option<ProviderDualPreset> {
    let target = provider_id.trim();
    list_dual_presets()
        .into_iter()
        .find(|p| p.provider_id == target)
}

/// Probes live provider connectivity with a single lightweight `/models` request before saving credentials.
pub fn probe_provider_connectivity(
    provider_id: &str,
    base_url_override: Option<&str>,
    api_key: &str,
) -> Result<Value, String> {
    let pid = provider_id.trim();
    let trimmed_key = api_key.trim();
    if trimmed_key.is_empty() {
        return Err("API_KEY_REQUIRED".to_owned());
    }

    let resolved_base = base_url_override
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| find_dual_preset(pid).map(|p| p.base_url))
        .or_else(|| {
            let reg = embedded_registry_value();
            reg.get("providers")
                .and_then(Value::as_array)?
                .iter()
                .find(|p| p.get("id").and_then(Value::as_str) == Some(pid))?
                .get("base_url")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .ok_or_else(|| "PROVIDER_BASE_URL_REQUIRED".to_owned())?;

    let base_trimmed = resolved_base.trim_end_matches('/');
    if !base_trimmed.starts_with("https://") && !base_trimmed.starts_with("http://") {
        return Err("PROVIDER_BASE_URL_INVALID".to_owned());
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|_| "HTTP_CLIENT_INIT_FAILED".to_owned())?;

    let is_native_gemini = (pid == "gemini"
        || base_trimmed.contains("generativelanguage.googleapis.com"))
        && !base_trimmed.ends_with("/openai")
        && (base_url_override.is_none()
            || base_trimmed.contains("generativelanguage.googleapis.com"));
    let is_anthropic = (pid == "anthropic" || base_trimmed.contains("api.anthropic.com"))
        && (base_url_override.is_none() || base_trimmed.contains("anthropic"));

    let req = if is_native_gemini {
        let url = format!("{base_trimmed}/models?key={trimmed_key}");
        client.get(url).header("Accept", "application/json")
    } else if is_anthropic {
        let url = if base_trimmed.ends_with("/v1") {
            format!("{base_trimmed}/models")
        } else {
            format!("{base_trimmed}/v1/models")
        };
        client
            .get(url)
            .header("x-api-key", trimmed_key)
            .header("anthropic-version", "2023-06-01")
            .header("Accept", "application/json")
    } else {
        let url = if base_trimmed.ends_with("/v1")
            || base_trimmed.ends_with("/v3")
            || base_trimmed.ends_with("/v4")
            || base_trimmed.contains("/compatible-mode/")
        {
            format!("{base_trimmed}/models")
        } else {
            format!("{base_trimmed}/v1/models")
        };
        client
            .get(url)
            .header("Authorization", format!("Bearer {trimmed_key}"))
            .header("Accept", "application/json")
    };

    let resp = req
        .send()
        .map_err(|e| format!("PROVIDER_CONNECTIVITY_FAILED: {e}"))?;
    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err("PROVIDER_AUTH_FAILED".to_owned());
    }
    if !status.is_success() {
        return Err(format!(
            "PROVIDER_CONNECTIVITY_FAILED: HTTP {}",
            status.as_u16()
        ));
    }

    let body = resp.json::<Value>().unwrap_or_else(|_| serde_json::json!({}));
    Ok(serde_json::json!({
        "ok": true,
        "status": status.as_u16(),
        "baseUrl": base_trimmed,
        "sample": body.get("object").cloned().unwrap_or(Value::Null),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_rpc::dispatch_runtime_rpc;
    use crate::ModelHubRuntime;
    use bob_kernel::app::LocalApp;
    use serde_json::json;

    #[test]
    fn all_ten_curated_presets_validate_against_embedded_registry() {
        let reg = embedded_registry_value();
        let presets = validate_presets_against_registry(&reg)
            .expect("every curated preset must exist in EMBEDDED_MODEL_REGISTRY");
        assert_eq!(presets.len(), 10);

        let expected = [
            ("deepseek", "deepseek-v4.1-flash-1017", "deepseek-v4-pro", true, "cn"),
            ("gemini", "models/gemini-2.5-flash", "models/gemini-2.5-pro", true, "global"),
            ("openai", "gpt-4o-mini", "gpt-4o", true, "global"),
            ("anthropic", "claude-haiku-4-5", "claude-3-7-sonnet-20250219", true, "global"),
            ("openrouter", "deepseek/deepseek-v4.1-flash", "anthropic/claude-fable-5.1", true, "global"),
            ("qwen", "qwen3.8-flash", "qwen3.8-max", false, "cn"),
            ("zhipu", "glm-5.3-flash", "glm-5.3", false, "cn"),
            ("kimi", "kimi-k2.6", "kimi-k3.1", false, "cn"),
            ("doubao", "doubao-seed-1-6-flash-250828", "doubao-seed-2-0-pro-260215", false, "cn"),
            ("minimax", "MiniMax-M2.7", "MiniMax-M3", false, "cn"),
        ];

        for (idx, (pid, clerk, maker, rec, region)) in expected.iter().enumerate() {
            let p = &presets[idx];
            assert_eq!(p.provider_id, *pid);
            assert_eq!(p.clerk_model, *clerk);
            assert_eq!(p.maker_model, *maker);
            assert_eq!(p.recommended, *rec);
            assert_eq!(p.region_hint, *region);
            assert!(!p.base_url.is_empty());
            assert!(!p.clerk_display_name.is_empty());
            assert!(!p.maker_display_name.is_empty());

            // Verify both snake_case and camelCase keys serialize & roundtrip
            let val = p.to_json_value();
            assert_eq!(val["provider_id"], *pid);
            assert_eq!(val["providerId"], *pid);
            assert_eq!(val["clerk_model"], *clerk);
            assert_eq!(val["clerkModel"], *clerk);
            assert_eq!(val["maker_model"], *maker);
            assert_eq!(val["makerModel"], *maker);
            assert_eq!(val["region_hint"], *region);
            assert_eq!(val["regionHint"], *region);

            let roundtrip: ProviderDualPreset = serde_json::from_value(val).unwrap();
            assert_eq!(&roundtrip, p);
        }
    }

    #[test]
    fn apply_dual_preset_configures_clerk_and_maker_from_single_key_and_supports_override() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = LocalApp::open(&dir.path().join("kernel.sqlite")).unwrap();
        let runtime = ModelHubRuntime::with_file_vault(dir.path().to_path_buf()).unwrap();

        assert!(!runtime.has_any_provider_key());

        // Tool-only keys must not count as LLM provider keys
        runtime
            .set_provider_key(&mut app, "tavily", Some("tvly-key-123"), false)
            .unwrap();
        assert!(!runtime.has_any_provider_key());

        // Apply DeepSeek dual preset with one API key
        let snap = runtime
            .apply_provider_dual_preset(&mut app, "deepseek", Some("sk-ds-single-123"), None, None)
            .unwrap();
        assert!(runtime.has_any_provider_key());
        assert_eq!(
            snap["activeRoles"]["clerk"],
            "deepseek::deepseek-v4.1-flash-1017"
        );
        assert_eq!(
            snap["activeRoles"]["flash"],
            "deepseek::deepseek-v4.1-flash-1017"
        );
        assert_eq!(snap["activeRoles"]["maker"], "deepseek::deepseek-v4-pro");
        assert_eq!(snap["activeRoles"]["main"], "deepseek::deepseek-v4-pro");

        // Ensure deepseek-v4-pro was made visible in UI pool
        let pool = snap["pool"].as_array().unwrap();
        assert!(pool
            .iter()
            .any(|m| m["id"] == "deepseek::deepseek-v4.1-flash-1017"));
        assert!(pool.iter().any(|m| m["id"] == "deepseek::deepseek-v4-pro"));

        let (clerk_prof, clerk_key) = runtime.resolve_role_profile_with_key("clerk").unwrap();
        let (maker_prof, maker_key) = runtime.resolve_role_profile_with_key("maker").unwrap();
        assert_eq!(clerk_prof.model_id, "deepseek-v4.1-flash-1017");
        assert_eq!(maker_prof.model_id, "deepseek-v4-pro");
        assert_eq!(clerk_key, "sk-ds-single-123");
        assert_eq!(maker_key, "sk-ds-single-123");

        // Verify custom model override on dual preset + RPC preset listing
        let snap_override = runtime
            .apply_provider_dual_preset(
                &mut app,
                "openai",
                Some("sk-oa-override-456"),
                Some("gpt-4o-mini"),
                Some("o3-mini"),
            )
            .unwrap();
        assert_eq!(snap_override["activeRoles"]["clerk"], "openai::gpt-4o-mini");
        assert_eq!(snap_override["activeRoles"]["maker"], "openai::o3-mini");
        let (oa_clerk, oa_key) = runtime.resolve_role_profile_with_key("clerk").unwrap();
        let oa_maker = runtime.resolve_role_profile("maker").unwrap();
        assert_eq!(oa_clerk.model_id, "gpt-4o-mini");
        assert_eq!(oa_maker.model_id, "o3-mini");
        assert_eq!(oa_key, "sk-oa-override-456");

        let rpc_list = dispatch_runtime_rpc(dir.path(), "app_model_presets_list", &json!({})).unwrap();
        assert_eq!(rpc_list.as_array().unwrap().len(), 10);
    }
}
