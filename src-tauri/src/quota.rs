use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitWindow {
    pub remaining_percent: Option<f64>,
    pub resets_at: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Credits {
    pub balance: Option<f64>,
    pub limit: Option<f64>,
    pub reset_credits_available: Option<u64>,
    pub plan_type: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaStatus {
    pub state: String,
    pub message: Option<String>,
    pub five_hour: Option<LimitWindow>,
    pub weekly: Option<LimitWindow>,
    pub credits: Option<Credits>,
    pub updated_at: Option<i64>,
    pub last_success_at: Option<i64>,
    pub error_code: Option<String>,
}

impl QuotaStatus {
    pub fn loading() -> Self {
        Self {
            state: "loading".into(),
            message: Some("正在读取 Codex 额度…".into()),
            five_hour: None,
            weekly: None,
            credits: None,
            updated_at: None,
            last_success_at: None,
            error_code: None,
        }
    }

    pub fn unavailable(code: impl Into<String>, message: impl Into<String>, now_ms: i64) -> Self {
        Self {
            state: "unavailable".into(),
            message: Some(message.into()),
            five_hour: None,
            weekly: None,
            credits: None,
            updated_at: Some(now_ms),
            last_success_at: None,
            error_code: Some(code.into()),
        }
    }

    pub fn with_failure(
        previous: &Self,
        code: impl Into<String>,
        message: impl Into<String>,
        now_ms: i64,
    ) -> Self {
        if previous.last_success_at.is_some() {
            let mut stale = previous.clone();
            stale.state = "stale".into();
            stale.message = Some(message.into());
            stale.updated_at = Some(now_ms);
            stale.error_code = Some(code.into());
            stale
        } else {
            Self::unavailable(code, message, now_ms)
        }
    }
}

pub fn parse_snapshot(
    rate_limits: &Value,
    account: Option<&Value>,
    now_ms: i64,
) -> Result<QuotaStatus, String> {
    let root = rate_limits
        .get("rateLimitsByLimitId")
        .and_then(|value| value.get("codex"))
        .or_else(|| {
            rate_limits.get("rateLimits").filter(|value| {
                value
                    .get("limitId")
                    .and_then(Value::as_str)
                    .is_none_or(|id| id == "codex")
            })
        })
        .ok_or("未返回 Codex 额度。")?;

    let mut five_hour = None;
    let mut weekly = None;
    for slot in ["primary", "secondary"] {
        let Some(window) = root.get(slot) else {
            continue;
        };
        let parsed = parse_window(window);
        match window.get("windowDurationMins").and_then(Value::as_i64) {
            Some(300) => five_hour = parsed,
            Some(10_080) => weekly = parsed,
            _ => {}
        }
    }
    if five_hour.is_none() && weekly.is_none() {
        return Err("未识别到 5 小时或周额度窗口。".into());
    }

    let credits = parse_credits(rate_limits, root, account);
    Ok(QuotaStatus {
        state: "ready".into(),
        message: None,
        five_hour,
        weekly,
        credits,
        updated_at: Some(now_ms),
        last_success_at: Some(now_ms),
        error_code: None,
    })
}

#[cfg(test)]
pub fn parse_rate_limits(value: &Value, now_ms: i64) -> Result<QuotaStatus, String> {
    parse_snapshot(value, None, now_ms)
}

fn parse_window(value: &Value) -> Option<LimitWindow> {
    let used = finite_number(value.get("usedPercent")?)?;
    Some(LimitWindow {
        remaining_percent: Some((100.0 - used).clamp(0.0, 100.0)),
        resets_at: value
            .get("resetsAt")
            .and_then(Value::as_i64)
            .and_then(|seconds| seconds.checked_mul(1000)),
    })
}

fn parse_credits(rate_limits: &Value, root: &Value, account: Option<&Value>) -> Option<Credits> {
    let credit_value = root
        .get("credits")
        .or_else(|| rate_limits.get("credits"))
        .filter(|value| value.is_object());
    let balance = credit_value.and_then(|value| {
        first_number(
            value,
            &["balance", "remaining", "remainingCredits", "amount"],
        )
    });
    let limit = credit_value
        .and_then(|value| first_number(value, &["limit", "total", "creditLimit", "allocated"]));
    let reset_credits_available = rate_limits
        .get("rateLimitResetCredits")
        .and_then(|value| value.get("availableCount"))
        .and_then(non_negative_u64);
    let plan_type = root
        .get("planType")
        .and_then(Value::as_str)
        .or_else(|| {
            account
                .and_then(|value| value.get("account"))
                .and_then(|value| value.get("planType"))
                .and_then(Value::as_str)
        })
        .map(str::to_owned);

    let parsed = Credits {
        balance,
        limit,
        reset_credits_available,
        plan_type,
    };
    (parsed.balance.is_some()
        || parsed.limit.is_some()
        || parsed.reset_credits_available.is_some()
        || parsed.plan_type.is_some())
    .then_some(parsed)
}

fn first_number(value: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(non_negative_number))
}

fn finite_number(value: &Value) -> Option<f64> {
    let number = value
        .as_f64()
        .or_else(|| value.as_str()?.parse::<f64>().ok())?;
    number.is_finite().then_some(number)
}

fn non_negative_number(value: &Value) -> Option<f64> {
    finite_number(value).filter(|number| *number >= 0.0)
}

fn non_negative_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
        .or_else(|| value.as_str()?.parse::<u64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn recognizes_windows_without_slot_order() {
        let status = parse_rate_limits(&json!({"rateLimits": {"limitId":"codex", "primary":{"usedPercent":30,"windowDurationMins":10080}, "secondary":{"usedPercent":20,"windowDurationMins":300}}}), 1).unwrap();
        assert_eq!(status.five_hour.unwrap().remaining_percent, Some(80.0));
        assert_eq!(status.weekly.unwrap().remaining_percent, Some(70.0));
    }

    #[test]
    fn prefers_the_multi_bucket_codex_record() {
        let status = parse_rate_limits(&json!({"rateLimitsByLimitId":{"spark":{"primary":{"usedPercent":99,"windowDurationMins":300}}, "codex":{"primary":{"usedPercent":50,"windowDurationMins":300}}}}), 1).unwrap();
        assert_eq!(status.five_hour.unwrap().remaining_percent, Some(50.0));
    }

    #[test]
    fn does_not_guess_unknown_durations() {
        let result = parse_rate_limits(
            &json!({"rateLimits":{"limitId":"codex","primary":{"usedPercent":20,"windowDurationMins":15}}}),
            1,
        );
        assert!(result.unwrap_err().contains("未识别"));
    }

    #[test]
    fn preserves_absent_windows_as_none() {
        let status = parse_rate_limits(&json!({"rateLimits":{"limitId":"codex","primary":{"usedPercent":20,"windowDurationMins":300}}}), 1).unwrap();
        assert!(status.weekly.is_none());
    }

    #[test]
    fn parses_optional_account_and_credit_fields() {
        let status = parse_snapshot(
            &json!({
                "rateLimits": {
                    "limitId": "codex",
                    "primary": {"usedPercent": 25, "windowDurationMins": 300},
                    "credits": {"remaining": "12.5", "total": 20}
                },
                "rateLimitResetCredits": {"availableCount": 2}
            }),
            Some(&json!({"account": {"type": "chatgpt", "planType": "pro"}})),
            1,
        )
        .unwrap();
        assert_eq!(
            status.credits,
            Some(Credits {
                balance: Some(12.5),
                limit: Some(20.0),
                reset_credits_available: Some(2),
                plan_type: Some("pro".into()),
            })
        );
    }

    #[test]
    fn invalid_credit_values_are_ignored() {
        let status = parse_snapshot(
            &json!({
                "rateLimits": {
                    "limitId": "codex",
                    "primary": {"usedPercent": 25, "windowDurationMins": 300},
                    "credits": {"remaining": "NaN", "total": -1}
                },
                "rateLimitResetCredits": {"availableCount": -4}
            }),
            None,
            1,
        )
        .unwrap();
        assert!(status.credits.is_none());
    }

    #[test]
    fn failed_refresh_keeps_last_good_snapshot() {
        let ready = parse_rate_limits(
            &json!({"rateLimits":{"limitId":"codex","primary":{"usedPercent":20,"windowDurationMins":300}}}),
            100,
        )
        .unwrap();
        let stale = QuotaStatus::with_failure(&ready, "timeout", "读取超时。", 200);
        assert_eq!(stale.state, "stale");
        assert_eq!(stale.five_hour, ready.five_hour);
        assert_eq!(stale.last_success_at, Some(100));
        assert_eq!(stale.updated_at, Some(200));
    }

    #[test]
    fn first_failure_is_unavailable() {
        let status = QuotaStatus::with_failure(
            &QuotaStatus::loading(),
            "codex_not_found",
            "未找到 Codex。",
            200,
        );
        assert_eq!(status.state, "unavailable");
        assert!(status.five_hour.is_none());
    }
}
