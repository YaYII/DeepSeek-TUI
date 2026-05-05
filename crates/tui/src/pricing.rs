//! Cost estimation for DeepSeek API usage.
//!
//! Pricing loaded from `assets/pricing.json` at compile time.
//! Edit that file to update pricing without recompiling.

use std::collections::HashMap;
use std::sync::LazyLock;

use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;

use crate::models::Usage;

/// Per-million-token pricing for a model.
#[derive(Debug, Clone, Copy)]
pub struct ModelPricing {
    pub input_cache_hit_per_million: f64,
    pub input_cache_miss_per_million: f64,
    pub output_per_million: f64,
}

/// A model pricing entry that may have a discounted introductory period.
#[derive(Debug, Clone, Deserialize)]
struct PricingEntry {
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    discount_ends_at: Option<String>,
    #[serde(default)]
    discount: Option<FlatRates>,
    #[serde(default)]
    full: Option<FlatRates>,
    /// Flat rates used when no discount period applies.
    #[serde(default)]
    input_cache_hit_per_million: Option<f64>,
    #[serde(default)]
    input_cache_miss_per_million: Option<f64>,
    #[serde(default)]
    output_per_million: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
struct FlatRates {
    input_cache_hit_per_million: f64,
    input_cache_miss_per_million: f64,
    output_per_million: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct PricingData {
    #[serde(default)]
    schema_version: u32,
    models: HashMap<String, PricingEntry>,
    #[serde(default)]
    aliases: HashMap<String, String>,
}

/// Loaded pricing data from `assets/pricing.json`.
static PRICING_DATA: LazyLock<PricingData> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../assets/pricing.json"))
        .expect("Failed to parse embedded pricing.json — check its JSON syntax")
});

/// Resolve a model name to its canonical pricing key.
/// Handles aliases (deepseek-chat → deepseek-v4-flash) and
/// normalisation (case-insensitive matching).
fn resolve_pricing_key(model: &str) -> Option<String> {
    let lower = model.to_lowercase().trim().to_string();

    // Check exact alias match first.
    if let Some(canonical) = PRICING_DATA.aliases.get(&lower) {
        return Some(canonical.clone());
    }

    // Check direct model match.
    if PRICING_DATA.models.contains_key(&lower) {
        return Some(lower);
    }

    // Try matching common prefixes by checking if any model key is a
    // suffix or prefix of the given model name.
    for (key, _) in &PRICING_DATA.models {
        if lower.contains(key) || key.contains(&lower) {
            return Some(key.clone());
        }
    }

    None
}

/// Get pricing for a model, resolving aliases and discount periods.
fn pricing_for_model(model: &str) -> Option<ModelPricing> {
    pricing_for_model_at(model, Utc::now())
}

fn parse_discount_end(s: &str) -> Option<DateTime<Utc>> {
    // Try ISO 8601 format first.
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    // Try a more lenient parse.
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(Utc.from_utc_datetime(&dt));
    }
    None
}

fn pricing_for_model_at(model: &str, now: DateTime<Utc>) -> Option<ModelPricing> {
    let lower = model.to_lowercase();

    // NVIDIA NIM-hosted DeepSeek uses NVIDIA's catalog/account terms, not
    // DeepSeek Platform pricing. Avoid showing misleading DeepSeek costs.
    if lower.starts_with("deepseek-ai/") {
        return None;
    }

    if !lower.contains("deepseek") {
        return None;
    }

    let key = resolve_pricing_key(model)?;
    let entry = PRICING_DATA.models.get(&key)?;

    // If the entry has a discount period with full rates, check if discount applies.
    if let (Some(discount_ends), Some(discount_rates), Some(full_rates)) = (
        entry.discount_ends_at.as_ref(),
        entry.discount.as_ref(),
        entry.full.as_ref(),
    ) {
        if let Some(end_dt) = parse_discount_end(discount_ends) {
            if now <= end_dt {
                return Some(ModelPricing {
                    input_cache_hit_per_million: discount_rates.input_cache_hit_per_million,
                    input_cache_miss_per_million: discount_rates.input_cache_miss_per_million,
                    output_per_million: discount_rates.output_per_million,
                });
            }
            return Some(ModelPricing {
                input_cache_hit_per_million: full_rates.input_cache_hit_per_million,
                input_cache_miss_per_million: full_rates.input_cache_miss_per_million,
                output_per_million: full_rates.output_per_million,
            });
        }
    }

    // Flat rates (no discount period).
    Some(ModelPricing {
        input_cache_hit_per_million: entry.input_cache_hit_per_million.unwrap_or(0.0),
        input_cache_miss_per_million: entry.input_cache_miss_per_million.unwrap_or(0.0),
        output_per_million: entry.output_per_million.unwrap_or(0.0),
    })
}

/// Calculate cost for a turn given token usage and model.
#[must_use]
#[allow(dead_code)]
pub fn calculate_turn_cost(model: &str, input_tokens: u32, output_tokens: u32) -> Option<f64> {
    let pricing = pricing_for_model(model)?;
    Some(calculate_turn_cost_with_pricing(
        pricing,
        input_tokens,
        output_tokens,
    ))
}

fn calculate_turn_cost_with_pricing(
    pricing: ModelPricing,
    input_tokens: u32,
    output_tokens: u32,
) -> f64 {
    let input_cost = (input_tokens as f64 / 1_000_000.0) * pricing.input_cache_miss_per_million;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * pricing.output_per_million;
    input_cost + output_cost
}

/// Calculate cost from provider usage, honoring DeepSeek context-cache fields.
#[must_use]
pub fn calculate_turn_cost_from_usage(model: &str, usage: &Usage) -> Option<f64> {
    let pricing = pricing_for_model(model)?;
    Some(calculate_turn_cost_from_usage_with_pricing(pricing, usage))
}

fn calculate_turn_cost_from_usage_with_pricing(pricing: ModelPricing, usage: &Usage) -> f64 {
    let hit_tokens = usage.prompt_cache_hit_tokens.unwrap_or(0);
    let miss_tokens = usage
        .prompt_cache_miss_tokens
        .unwrap_or_else(|| usage.input_tokens.saturating_sub(hit_tokens));
    let accounted_input = hit_tokens.saturating_add(miss_tokens);
    let uncategorized_input = usage.input_tokens.saturating_sub(accounted_input);

    let hit_cost = (hit_tokens as f64 / 1_000_000.0) * pricing.input_cache_hit_per_million;
    let miss_cost = ((miss_tokens.saturating_add(uncategorized_input)) as f64 / 1_000_000.0)
        * pricing.input_cache_miss_per_million;
    let output_cost = (usage.output_tokens as f64 / 1_000_000.0) * pricing.output_per_million;
    hit_cost + miss_cost + output_cost
}

/// Format a USD cost for compact display.
#[must_use]
#[allow(dead_code)]
pub fn format_cost(cost: f64) -> String {
    if cost < 0.0001 {
        "<$0.0001".to_string()
    } else if cost < 0.01 {
        format!("${:.4}", cost)
    } else if cost < 1.0 {
        format!("${:.3}", cost)
    } else {
        format!("${:.2}", cost)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvidia_nim_deepseek_model_does_not_use_deepseek_platform_pricing() {
        assert!(calculate_turn_cost("deepseek-ai/deepseek-v4-pro", 1_000, 1_000).is_none());
    }

    #[test]
    fn v4_pro_uses_limited_time_discount_before_expiry() {
        let before_expiry = Utc
            .with_ymd_and_hms(2026, 5, 31, 15, 58, 59)
            .single()
            .unwrap();
        let pricing = pricing_for_model_at("deepseek-v4-pro", before_expiry).unwrap();

        assert_eq!(pricing.input_cache_hit_per_million, 0.003625);
        assert_eq!(pricing.input_cache_miss_per_million, 0.435);
        assert_eq!(pricing.output_per_million, 0.87);
    }

    #[test]
    fn v4_pro_returns_to_base_rates_after_discount_expiry() {
        let after_expiry = Utc
            .with_ymd_and_hms(2026, 5, 31, 16, 0, 0)
            .single()
            .unwrap();
        let pricing = pricing_for_model_at("deepseek-v4-pro", after_expiry).unwrap();

        assert_eq!(pricing.input_cache_hit_per_million, 0.0145);
        assert_eq!(pricing.input_cache_miss_per_million, 1.74);
        assert_eq!(pricing.output_per_million, 3.48);
    }

    #[test]
    fn v4_pro_discount_still_applies_just_before_old_may5_expiry() {
        // Regression for #267: extension to 2026-05-31 15:59 UTC.
        let after_old_expiry = Utc.with_ymd_and_hms(2026, 5, 6, 0, 0, 0).single().unwrap();
        let pricing = pricing_for_model_at("deepseek-v4-pro", after_old_expiry).unwrap();

        assert_eq!(pricing.input_cache_hit_per_million, 0.003625);
        assert_eq!(pricing.input_cache_miss_per_million, 0.435);
        assert_eq!(pricing.output_per_million, 0.87);
    }

    #[test]
    fn v4_flash_keeps_current_published_rates() {
        let now = Utc.with_ymd_and_hms(2026, 4, 25, 0, 0, 0).single().unwrap();
        let pricing = pricing_for_model_at("deepseek-v4-flash", now).unwrap();

        assert_eq!(pricing.input_cache_hit_per_million, 0.0028);
        assert_eq!(pricing.input_cache_miss_per_million, 0.14);
        assert_eq!(pricing.output_per_million, 0.28);
    }

    #[test]
    fn v4_pro_uses_discount_with_alias() {
        // v4pro alias should resolve to v4-pro pricing
        let now = Utc.with_ymd_and_hms(2026, 4, 25, 0, 0, 0).single().unwrap();
        let pricing = pricing_for_model_at("deepseek-v4pro", now).unwrap();
        assert_eq!(pricing.input_cache_hit_per_million, 0.003625);
    }

    #[test]
    fn flash_uses_flat_rates_with_alias() {
        let now = Utc.with_ymd_and_hms(2026, 4, 25, 0, 0, 0).single().unwrap();
        let pricing = pricing_for_model_at("deepseek-chat", now).unwrap();
        assert_eq!(pricing.input_cache_hit_per_million, 0.0028);
    }
}
