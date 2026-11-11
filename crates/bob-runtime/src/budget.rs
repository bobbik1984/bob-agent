//! Monthly spending budget cap with automatic month rollover and model price lookup.
//!
//! Enforces:
//! - Default monthly cap of $10.00 USD (`BOBBOT_CNY_PER_USD` default 7.2).
//! - Automatic month rollover in UTC+8 (resets spend and warnings).
//! - Model price resolution via `EMBEDDED_MODEL_REGISTRY` with fallbacks for standard families.
//! - Unknown model price rejection (cannot run models without known price).
//! - Atomic persistence to `budget.json`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const DEFAULT_CAP_USD: f64 = 10.0;
pub const DEFAULT_CNY_PER_USD: f64 = 7.2;
pub const DEFAULT_OFFSET_HOURS: i32 = 8;

fn default_cap_usd() -> f64 {
    DEFAULT_CAP_USD
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PriceCurrency {
    #[serde(alias = "usd", alias = "USD")]
    Usd,
    #[serde(alias = "cny", alias = "CNY")]
    Cny,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelPrice {
    pub input_per_million: f64,
    pub output_per_million: f64,
    pub currency: PriceCurrency,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BudgetStatus {
    pub month: String,
    pub cap_usd: f64,
    pub spent_usd: f64,
    pub warned: bool,
    pub near_80_pct: bool,
    pub exceeded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetRecord {
    #[serde(default)]
    pub month: String,
    #[serde(alias = "capUsd", default = "default_cap_usd")]
    pub cap_usd: f64,
    #[serde(alias = "spentUsd", default)]
    pub spent_usd: f64,
    #[serde(default)]
    pub warned: bool,
}

impl Default for BudgetRecord {
    fn default() -> Self {
        Self {
            month: String::new(),
            cap_usd: DEFAULT_CAP_USD,
            spent_usd: 0.0,
            warned: false,
        }
    }
}

/// Convert unix timestamp in millis and timezone offset in hours to "YYYY-MM".
pub fn format_month_utc_offset(ts_millis: i64, offset_hours: i32) -> String {
    let total_seconds = (ts_millis / 1000) + (offset_hours as i64 * 3600);
    let days = total_seconds.div_euclid(86400);
    let (year, month, _day) = civil_from_days(days);
    format!("{year:04}-{month:02}")
}

pub fn current_month_utc8() -> String {
    format_month_utc_offset(crate::now_millis(), DEFAULT_OFFSET_HOURS)
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

/// Resolves model pricing from an explicit custom price, the embedded registry,
/// or built-in standard model family rules. Returns `Err("UNKNOWN_MODEL_PRICE")`
/// if pricing cannot be determined.
pub fn resolve_model_price(
    model_id: &str,
    custom_price: Option<ModelPrice>,
) -> Result<ModelPrice, &'static str> {
    if let Some(price) = custom_price {
        return Ok(price);
    }

    let trimmed = model_id.trim();
    if trimmed.is_empty() {
        return Err("UNKNOWN_MODEL_PRICE");
    }

    // 1. Try resolving against EMBEDDED_MODEL_REGISTRY
    if let Ok(reg) = serde_json::from_str::<Value>(crate::EMBEDDED_MODEL_REGISTRY) {
        if let Some(providers) = reg.get("providers").and_then(Value::as_array) {
            for prov in providers {
                let prov_id = prov.get("id").and_then(Value::as_str).unwrap_or("");
                if let Some(models) = prov.get("models").and_then(Value::as_array) {
                    for m in models {
                        let m_id = m.get("id").and_then(Value::as_str).unwrap_or("");
                        let matches = m_id.eq_ignore_ascii_case(trimmed)
                            || format!("{prov_id}::{m_id}").eq_ignore_ascii_case(trimmed)
                            || trimmed.ends_with(m_id);
                        if matches {
                            if let Some(pricing) = m.get("pricing") {
                                let input = pricing
                                    .get("input")
                                    .and_then(Value::as_f64)
                                    .unwrap_or(0.0);
                                let output = pricing
                                    .get("output")
                                    .and_then(Value::as_f64)
                                    .unwrap_or(0.0);
                                if input > 0.0 || output > 0.0 {
                                    let currency = match prov_id {
                                        "openai" | "anthropic" | "google" | "groq" | "mistral"
                                        | "openrouter" => PriceCurrency::Usd,
                                        _ => PriceCurrency::Cny,
                                    };
                                    return Ok(ModelPrice {
                                        input_per_million: input,
                                        output_per_million: output,
                                        currency,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Built-in standard model family fallbacks
    let short_id = trimmed
        .split("::")
        .last()
        .unwrap_or(trimmed)
        .split('/')
        .last()
        .unwrap_or(trimmed)
        .to_ascii_lowercase();

    if short_id.starts_with("deepseek-chat") {
        Ok(ModelPrice {
            input_per_million: 1.441,
            output_per_million: 5.761,
            currency: PriceCurrency::Cny,
        })
    } else if short_id.starts_with("deepseek-reasoner") || short_id.starts_with("deepseek-r1") {
        Ok(ModelPrice {
            input_per_million: 4.0,
            output_per_million: 16.0,
            currency: PriceCurrency::Cny,
        })
    } else if short_id.starts_with("deepseek-flash") || short_id.starts_with("deepseek-v4") {
        Ok(ModelPrice {
            input_per_million: 1.0,
            output_per_million: 2.0,
            currency: PriceCurrency::Cny,
        })
    } else if short_id.starts_with("deepseek") {
        Ok(ModelPrice {
            input_per_million: 1.5,
            output_per_million: 6.0,
            currency: PriceCurrency::Cny,
        })
    } else if short_id.starts_with("gpt-4o-mini") {
        Ok(ModelPrice {
            input_per_million: 0.15,
            output_per_million: 0.60,
            currency: PriceCurrency::Usd,
        })
    } else if short_id.starts_with("gpt-")
        || short_id.starts_with("o1")
        || short_id.starts_with("o3")
    {
        Ok(ModelPrice {
            input_per_million: 2.50,
            output_per_million: 10.00,
            currency: PriceCurrency::Usd,
        })
    } else if short_id.starts_with("claude-3-5-sonnet")
        || short_id.starts_with("claude-3-7-sonnet")
    {
        Ok(ModelPrice {
            input_per_million: 3.0,
            output_per_million: 15.0,
            currency: PriceCurrency::Usd,
        })
    } else if short_id.starts_with("claude-3-5-haiku") || short_id.starts_with("claude-3-haiku") {
        Ok(ModelPrice {
            input_per_million: 0.80,
            output_per_million: 4.0,
            currency: PriceCurrency::Usd,
        })
    } else if short_id.starts_with("claude-") {
        Ok(ModelPrice {
            input_per_million: 3.0,
            output_per_million: 15.0,
            currency: PriceCurrency::Usd,
        })
    } else if short_id.starts_with("gemini-1.5-flash")
        || short_id.starts_with("gemini-2.0-flash")
        || short_id.starts_with("gemini-flash")
    {
        Ok(ModelPrice {
            input_per_million: 0.075,
            output_per_million: 0.30,
            currency: PriceCurrency::Usd,
        })
    } else if short_id.starts_with("gemini-1.5-pro")
        || short_id.starts_with("gemini-2.0-pro")
        || short_id.starts_with("gemini-pro")
    {
        Ok(ModelPrice {
            input_per_million: 1.25,
            output_per_million: 5.00,
            currency: PriceCurrency::Usd,
        })
    } else if short_id.starts_with("gemini-") {
        Ok(ModelPrice {
            input_per_million: 1.25,
            output_per_million: 5.00,
            currency: PriceCurrency::Usd,
        })
    } else {
        Err("UNKNOWN_MODEL_PRICE")
    }
}

pub struct MonthlyBudgetStore {
    file_path: PathBuf,
    record: BudgetRecord,
    pub cny_per_usd: f64,
    pub offset_hours: i32,
}

impl MonthlyBudgetStore {
    pub fn open(data_dir: &Path) -> Result<Self, &'static str> {
        Self::open_with_config(data_dir, DEFAULT_CNY_PER_USD, DEFAULT_OFFSET_HOURS)
    }

    pub fn open_with_config(
        data_dir: &Path,
        cny_per_usd: f64,
        offset_hours: i32,
    ) -> Result<Self, &'static str> {
        let _ = std::fs::create_dir_all(data_dir);
        let file_path = data_dir.join("budget.json");
        let now_month = format_month_utc_offset(crate::now_millis(), offset_hours);

        let mut record = if file_path.is_file() {
            let content = std::fs::read_to_string(&file_path)
                .map_err(|_| "BUDGET_READ_FAILED")?;
            serde_json::from_str::<BudgetRecord>(&content).unwrap_or_else(|_| BudgetRecord {
                month: now_month.clone(),
                cap_usd: DEFAULT_CAP_USD,
                spent_usd: 0.0,
                warned: false,
            })
        } else {
            BudgetRecord {
                month: now_month.clone(),
                cap_usd: DEFAULT_CAP_USD,
                spent_usd: 0.0,
                warned: false,
            }
        };

        if record.month != now_month {
            record.month = now_month;
            record.spent_usd = 0.0;
            record.warned = false;
        }

        let store = Self {
            file_path,
            record,
            cny_per_usd: if cny_per_usd <= 0.0 {
                DEFAULT_CNY_PER_USD
            } else {
                cny_per_usd
            },
            offset_hours,
        };

        store.save_atomic()?;
        Ok(store)
    }

    fn expected_month(&self) -> String {
        format_month_utc_offset(crate::now_millis(), self.offset_hours)
    }

    fn roll_month_if_needed(&mut self) -> bool {
        let current = self.expected_month();
        if self.record.month != current {
            self.record.month = current;
            self.record.spent_usd = 0.0;
            self.record.warned = false;
            true
        } else {
            false
        }
    }

    fn save_atomic(&self) -> Result<(), &'static str> {
        let serialized = serde_json::to_string_pretty(&self.record)
            .map_err(|_| "BUDGET_SERIALIZATION_FAILED")?;
        let tmp_path = self.file_path.with_extension("json.tmp");
        std::fs::write(&tmp_path, serialized.as_bytes())
            .map_err(|_| "BUDGET_WRITE_FAILED")?;
        std::fs::rename(&tmp_path, &self.file_path)
            .map_err(|_| "BUDGET_WRITE_FAILED")?;
        Ok(())
    }

    pub fn status(&mut self) -> BudgetStatus {
        if self.roll_month_if_needed() {
            let _ = self.save_atomic();
        }
        let cap_usd = self.record.cap_usd;
        let spent_usd = self.record.spent_usd;
        BudgetStatus {
            month: self.record.month.clone(),
            cap_usd,
            spent_usd,
            warned: self.record.warned,
            near_80_pct: spent_usd >= cap_usd * 0.8,
            exceeded: spent_usd >= cap_usd,
        }
    }

    pub fn can_start_call(&mut self) -> Result<BudgetStatus, &'static str> {
        let st = self.status();
        if st.exceeded {
            Err("MONTHLY_BUDGET_EXCEEDED")
        } else {
            Ok(st)
        }
    }

    pub fn record_usage(
        &mut self,
        model_id: &str,
        input_tokens: u64,
        output_tokens: u64,
        cache_tokens: u64,
        custom_price: Option<ModelPrice>,
    ) -> Result<BudgetStatus, &'static str> {
        let price = resolve_model_price(model_id, custom_price)?;
        if self.roll_month_if_needed() {
            let _ = self.save_atomic();
        }

        let total_input = (input_tokens + cache_tokens) as f64;
        let output = output_tokens as f64;
        let cost_in_curr =
            (total_input * price.input_per_million + output * price.output_per_million) / 1_000_000.0;

        let cost_usd = match price.currency {
            PriceCurrency::Usd => cost_in_curr,
            PriceCurrency::Cny => cost_in_curr / self.cny_per_usd.max(0.0001),
        };

        self.record.spent_usd += cost_usd;
        self.save_atomic()?;
        Ok(self.status())
    }

    pub fn set_cap_usd(&mut self, cap_usd: f64) -> Result<BudgetStatus, &'static str> {
        if cap_usd < 0.0 {
            return Err("INVALID_CAP_USD");
        }
        let _ = self.roll_month_if_needed();
        self.record.cap_usd = cap_usd;
        self.record.warned = false;
        self.save_atomic()?;
        Ok(self.status())
    }

    pub fn mark_warned(&mut self) -> Result<BudgetStatus, &'static str> {
        let _ = self.roll_month_if_needed();
        self.record.warned = true;
        self.save_atomic()?;
        Ok(self.status())
    }

    /// Explicitly force month setting for test simulation of rollover.
    #[cfg(test)]
    pub fn force_state_for_test(&mut self, month: &str, spent_usd: f64, warned: bool) {
        self.record.month = month.to_string();
        self.record.spent_usd = spent_usd;
        self.record.warned = warned;
        let _ = self.save_atomic();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_date_conversion_civil() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // 2026-11-10
        // Days between 1970-01-01 and 2026-11-10: 20767
        assert_eq!(civil_from_days(20767), (2026, 11, 10));
        assert_eq!(format_month_utc_offset(0, 0), "1970-01");
        assert_eq!(format_month_utc_offset(20767 * 86400 * 1000, 0), "2026-11");
    }

    #[test]
    fn test_default_cap_and_status() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MonthlyBudgetStore::open(tmp.path()).unwrap();
        let st = store.status();
        assert_eq!(st.cap_usd, 10.0);
        assert_eq!(st.spent_usd, 0.0);
        assert!(!st.warned);
        assert!(!st.near_80_pct);
        assert!(!st.exceeded);
        assert!(store.can_start_call().is_ok());
    }

    #[test]
    fn test_cny_to_usd_conversion_and_tokens() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MonthlyBudgetStore::open(tmp.path()).unwrap();
        // deepseek-flash: 1.0 CNY input, 2.0 CNY output per 1M tokens
        // 1M input tokens + 1M output tokens = (1.0 + 2.0) = 3.0 CNY
        // At 7.2 CNY/USD: 3.0 / 7.2 = 0.4166666... USD
        let st = store
            .record_usage("deepseek-flash", 1_000_000, 1_000_000, 0, None)
            .unwrap();
        let expected_usd = 3.0 / 7.2;
        assert!((st.spent_usd - expected_usd).abs() < 1e-6);
        assert!(!st.near_80_pct);
        assert!(!st.exceeded);
    }

    #[test]
    fn test_warning_threshold_80_pct_and_mark_warned() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MonthlyBudgetStore::open(tmp.path()).unwrap();
        // Record USD usage directly with custom price: $8.50 spent
        let custom = ModelPrice {
            input_per_million: 8.5,
            output_per_million: 0.0,
            currency: PriceCurrency::Usd,
        };
        let st = store
            .record_usage("custom-usd", 1_000_000, 0, 0, Some(custom))
            .unwrap();
        assert_eq!(st.spent_usd, 8.5);
        assert!(st.near_80_pct);
        assert!(!st.exceeded);
        assert!(!st.warned);

        let warned_st = store.mark_warned().unwrap();
        assert!(warned_st.warned);
    }

    #[test]
    fn test_budget_exceeded_hard_block_and_raising_cap_unblocks() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MonthlyBudgetStore::open(tmp.path()).unwrap();
        let custom = ModelPrice {
            input_per_million: 10.5,
            output_per_million: 0.0,
            currency: PriceCurrency::Usd,
        };
        store
            .record_usage("custom-big", 1_000_000, 0, 0, Some(custom))
            .unwrap();

        let st = store.status();
        assert!(st.exceeded);
        assert_eq!(
            store.can_start_call().unwrap_err(),
            "MONTHLY_BUDGET_EXCEEDED"
        );

        // Raising cap to 20.0 unblocks
        let unblocked = store.set_cap_usd(20.0).unwrap();
        assert_eq!(unblocked.cap_usd, 20.0);
        assert!(!unblocked.exceeded);
        assert!(store.can_start_call().is_ok());
    }

    #[test]
    fn test_month_rollover_resets_spend() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MonthlyBudgetStore::open(tmp.path()).unwrap();
        store.force_state_for_test("1999-01", 9.99, true);

        let st = store.status();
        // Since current month is not 1999-01, it must have rolled over!
        assert_ne!(st.month, "1999-01");
        assert_eq!(st.spent_usd, 0.0);
        assert!(!st.warned);
        assert!(!st.near_80_pct);
        assert!(!st.exceeded);
    }

    #[test]
    fn test_atomic_persistence_reload() {
        let tmp = tempfile::tempdir().unwrap();
        {
            let mut store = MonthlyBudgetStore::open(tmp.path()).unwrap();
            let custom = ModelPrice {
                input_per_million: 3.5,
                output_per_million: 0.0,
                currency: PriceCurrency::Usd,
            };
            store
                .record_usage("custom-persist", 1_000_000, 0, 0, Some(custom))
                .unwrap();
            store.mark_warned().unwrap();
        }

        // Reopen from same path
        let mut reloaded = MonthlyBudgetStore::open(tmp.path()).unwrap();
        let st = reloaded.status();
        assert_eq!(st.spent_usd, 3.5);
        assert!(st.warned);
        assert_eq!(st.cap_usd, 10.0);
    }

    #[test]
    fn test_unknown_model_price_rejected() {
        let err = resolve_model_price("totally-unknown-model-xyz", None).unwrap_err();
        assert_eq!(err, "UNKNOWN_MODEL_PRICE");

        let tmp = tempfile::tempdir().unwrap();
        let mut store = MonthlyBudgetStore::open(tmp.path()).unwrap();
        let call_res = store.record_usage("totally-unknown-model-xyz", 100, 100, 0, None);
        assert_eq!(call_res.unwrap_err(), "UNKNOWN_MODEL_PRICE");
    }

    #[test]
    fn test_standard_fallbacks_resolve_correctly() {
        let p_chat = resolve_model_price("deepseek-chat", None).unwrap();
        assert_eq!(p_chat.currency, PriceCurrency::Cny);

        let p_gpt = resolve_model_price("gpt-4o", None).unwrap();
        assert_eq!(p_gpt.currency, PriceCurrency::Usd);

        let p_claude = resolve_model_price("claude-3-5-sonnet-20241022", None).unwrap();
        assert_eq!(p_claude.currency, PriceCurrency::Usd);

        let p_gemini = resolve_model_price("gemini-1.5-flash", None).unwrap();
        assert_eq!(p_gemini.currency, PriceCurrency::Usd);
    }
}
