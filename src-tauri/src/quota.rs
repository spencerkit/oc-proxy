use crate::models::{
    Group, ProxyConfig, QuotaStatus, QuotaUnitType, Rule, RuleQuotaSnapshot, RuleQuotaTestResult,
};
use chrono::Utc;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const QUOTA_TIMEOUT_SECONDS: u64 = 12;
const QUOTA_LOG_BODY_MAX_CHARS: usize = 12 * 1024;

struct FetchRuleQuotaResult {
    snapshot: RuleQuotaSnapshot,
    raw_response: Option<Value>,
}

fn quota_dev_log_enabled() -> bool {
    cfg!(debug_assertions)
}

fn clip_for_log(raw: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (idx, ch) in raw.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("...(truncated)");
            break;
        }
        out.push(ch);
    }
    out
}

fn headers_for_log(headers: &HeaderMap) -> Value {
    let mut pairs: Vec<(String, String)> = Vec::new();
    for (key, value) in headers {
        let key_text = key.as_str().to_string();
        let value_text = value
            .to_str()
            .map(|v| v.to_string())
            .unwrap_or_else(|_| "<non-utf8>".to_string());
        pairs.push((key_text, value_text));
    }
    pairs.sort_by(|a, b| a.0.cmp(&b.0));

    let mut map = serde_json::Map::new();
    for (k, v) in pairs {
        map.insert(k, Value::String(v));
    }
    Value::Object(map)
}

fn log_quota_event(group: &Group, rule: &Rule, stage: &str, details: Value) {
    if !quota_dev_log_enabled() {
        return;
    }
    let pretty = serde_json::to_string_pretty(&details).unwrap_or_else(|_| details.to_string());
    eprintln!(
        "[quota][{stage}] group_id={} group_name={} rule_id={} rule_name={} provider={}\n{}",
        group.id,
        group.name,
        rule.id,
        rule.name,
        rule.quota.provider,
        pretty
    );
}

fn body_to_value_for_debug(raw: &str) -> Value {
    if raw.trim().is_empty() {
        Value::String("<empty>".to_string())
    } else {
        serde_json::from_str::<Value>(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
    }
}

pub async fn fetch_rule_quota(
    config: &ProxyConfig,
    group_id: &str,
    rule_id: &str,
) -> Result<RuleQuotaSnapshot, String> {
    let group = config
        .groups
        .iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| format!("group not found: {group_id}"))?;
    let rule = group
        .rules
        .iter()
        .find(|r| r.id == rule_id)
        .ok_or_else(|| format!("rule not found: {rule_id}"))?;

    Ok(fetch_single_rule_quota(group, rule, false).await.snapshot)
}

pub async fn fetch_group_quotas(
    config: &ProxyConfig,
    group_id: &str,
) -> Result<Vec<RuleQuotaSnapshot>, String> {
    let group = config
        .groups
        .iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| format!("group not found: {group_id}"))?;

    let mut out = Vec::with_capacity(group.rules.len());
    for rule in &group.rules {
        out.push(fetch_single_rule_quota(group, rule, false).await.snapshot);
    }
    Ok(out)
}

pub async fn test_rule_quota_draft(group: &Group, rule: &Rule) -> RuleQuotaTestResult {
    let result = fetch_single_rule_quota(group, rule, true).await;
    let snapshot = result.snapshot;
    let ok = matches!(
        snapshot.status,
        QuotaStatus::Ok | QuotaStatus::Low | QuotaStatus::Empty
    );
    let message = if ok {
        None
    } else {
        snapshot
            .message
            .clone()
            .or_else(|| Some(default_test_failure_message(&snapshot.status)))
    };

    RuleQuotaTestResult {
        ok,
        snapshot: Some(snapshot),
        raw_response: result.raw_response,
        message,
    }
}

fn default_test_failure_message(status: &QuotaStatus) -> String {
    match status {
        QuotaStatus::Unknown => "remaining quota mapping returned empty result".to_string(),
        QuotaStatus::Unsupported => "quota query disabled".to_string(),
        QuotaStatus::Error => "quota query failed".to_string(),
        _ => "quota query failed".to_string(),
    }
}

async fn fetch_single_rule_quota(
    group: &Group,
    rule: &Rule,
    include_raw_response: bool,
) -> FetchRuleQuotaResult {
    let started_at = Instant::now();
    let mut snapshot = new_snapshot(group, rule);
    if !rule.quota.enabled {
        snapshot.status = QuotaStatus::Unsupported;
        snapshot.message = Some("quota query disabled".to_string());
        log_quota_event(
            group,
            rule,
            "skip",
            json!({
                "message": "quota query disabled",
                "enabled": false
            }),
        );
        return FetchRuleQuotaResult {
            snapshot,
            raw_response: None,
        };
    }

    let endpoint = render_template(group, rule, &rule.quota.endpoint);
    if endpoint.trim().is_empty() {
        snapshot.status = QuotaStatus::Error;
        snapshot.message = Some("quota endpoint is empty".to_string());
        log_quota_event(
            group,
            rule,
            "error",
            json!({
                "message": "quota endpoint is empty"
            }),
        );
        return FetchRuleQuotaResult {
            snapshot,
            raw_response: None,
        };
    }

    let client = match Client::builder()
        .timeout(Duration::from_secs(QUOTA_TIMEOUT_SECONDS))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            snapshot.status = QuotaStatus::Error;
            snapshot.message = Some(format!("create quota http client failed: {error}"));
            log_quota_event(
                group,
                rule,
                "error",
                json!({
                    "message": "create quota http client failed",
                    "error": error.to_string()
                }),
            );
            return FetchRuleQuotaResult {
                snapshot,
                raw_response: None,
            };
        }
    };

    let method_name = normalize_method_name(&rule.quota.method);
    let method = match Method::from_bytes(method_name.as_bytes()) {
        Ok(method) => method,
        Err(_) => {
            snapshot.status = QuotaStatus::Error;
            snapshot.message = Some(format!("invalid quota method: {method_name}"));
            log_quota_event(
                group,
                rule,
                "error",
                json!({
                    "message": "invalid quota method",
                    "method": method_name
                }),
            );
            return FetchRuleQuotaResult {
                snapshot,
                raw_response: None,
            };
        }
    };

    let headers = match build_headers(group, rule) {
        Ok(headers) => headers,
        Err(error) => {
            snapshot.status = QuotaStatus::Error;
            snapshot.message = Some(error);
            log_quota_event(
                group,
                rule,
                "error",
                json!({
                    "message": "build quota headers failed",
                    "error": snapshot.message.clone()
                }),
            );
            return FetchRuleQuotaResult {
                snapshot,
                raw_response: None,
            };
        }
    };

    log_quota_event(
        group,
        rule,
        "start",
        json!({
            "message": "quota query start",
            "requestAddress": endpoint,
            "requestMethod": method_name,
            "requestHeaders": headers_for_log(&headers),
            "requestBody": "<empty>"
        }),
    );

    let mut request = client.request(method, endpoint.clone());
    if !headers.is_empty() {
        request = request.headers(headers);
    }

    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            snapshot.status = QuotaStatus::Error;
            snapshot.message = Some(format!("quota request failed: {error}"));
            log_quota_event(
                group,
                rule,
                "error",
                json!({
                    "message": "quota request failed",
                    "requestAddress": endpoint,
                    "requestMethod": method_name,
                    "elapsedMs": started_at.elapsed().as_millis(),
                    "error": error.to_string()
                }),
            );
            return FetchRuleQuotaResult {
                snapshot,
                raw_response: None,
            };
        }
    };

    let status = response.status();
    let response_headers = response.headers().clone();
    let response_body = response.text().await.unwrap_or_default();

    log_quota_event(
        group,
        rule,
        "response",
        json!({
            "requestAddress": endpoint,
            "requestMethod": method_name,
            "httpStatus": status.as_u16(),
            "elapsedMs": started_at.elapsed().as_millis(),
            "responseHeaders": headers_for_log(&response_headers),
            "responseBody": body_to_value_for_debug(&clip_for_log(&response_body, QUOTA_LOG_BODY_MAX_CHARS))
        }),
    );

    let raw_response = if include_raw_response {
        Some(body_to_value_for_debug(&response_body))
    } else {
        None
    };

    if !status.is_success() {
        snapshot.status = QuotaStatus::Error;
        snapshot.message = Some(format!(
            "quota endpoint returned HTTP {}{}",
            status.as_u16(),
            render_body_suffix(&response_body)
        ));
        return FetchRuleQuotaResult {
            snapshot,
            raw_response,
        };
    }

    let payload = match serde_json::from_str::<Value>(&response_body) {
        Ok(payload) => payload,
        Err(error) => {
            snapshot.status = QuotaStatus::Error;
            snapshot.message = Some(format!("invalid quota response JSON: {error}"));
            log_quota_event(
                group,
                rule,
                "error",
                json!({
                    "message": "invalid quota response JSON",
                    "requestAddress": endpoint,
                    "requestMethod": method_name,
                    "error": error.to_string(),
                    "responseBody": clip_for_log(&response_body, QUOTA_LOG_BODY_MAX_CHARS)
                }),
            );
            return FetchRuleQuotaResult {
                snapshot,
                raw_response,
            };
        }
    };

    match map_payload_to_snapshot(&mut snapshot, rule, &payload) {
        Ok(()) => {
            log_quota_event(
                group,
                rule,
                "finish",
                json!({
                    "status": "ok",
                    "snapshot": snapshot.clone(),
                    "elapsedMs": started_at.elapsed().as_millis()
                }),
            );
            FetchRuleQuotaResult {
                snapshot,
                raw_response,
            }
        }
        Err(error) => {
            snapshot.status = QuotaStatus::Error;
            snapshot.message = Some(error);
            log_quota_event(
                group,
                rule,
                "error",
                json!({
                    "message": "map payload to snapshot failed",
                    "snapshot": snapshot.clone(),
                    "elapsedMs": started_at.elapsed().as_millis()
                }),
            );
            FetchRuleQuotaResult {
                snapshot,
                raw_response,
            }
        }
    }
}

fn new_snapshot(group: &Group, rule: &Rule) -> RuleQuotaSnapshot {
    RuleQuotaSnapshot {
        group_id: group.id.clone(),
        rule_id: rule.id.clone(),
        provider: if rule.quota.provider.trim().is_empty() {
            "custom".to_string()
        } else {
            rule.quota.provider.trim().to_string()
        },
        status: QuotaStatus::Unknown,
        remaining: None,
        total: None,
        percent: None,
        unit: None,
        reset_at: None,
        fetched_at: Utc::now().to_rfc3339(),
        message: None,
    }
}

fn normalize_method_name(method: &str) -> String {
    let trimmed = method.trim();
    if trimmed.is_empty() {
        "GET".to_string()
    } else {
        trimmed.to_ascii_uppercase()
    }
}

fn render_template(group: &Group, rule: &Rule, raw: &str) -> String {
    let resolved_token = if rule.quota.use_rule_token {
        rule.token.as_str()
    } else {
        rule.quota.custom_token.as_str()
    };

    raw.replace("{{group.id}}", &group.id)
        .replace("{{group.name}}", &group.name)
        .replace("{{rule.id}}", &rule.id)
        .replace("{{rule.name}}", &rule.name)
        .replace("{{rule.apiAddress}}", &rule.api_address)
        .replace("{{rule.defaultModel}}", &rule.default_model)
        .replace("{{rule.token}}", &rule.token)
        .replace("{{quota.token}}", resolved_token)
}

fn build_headers(group: &Group, rule: &Rule) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();

    for (key, value) in &rule.quota.custom_headers {
        let key_name = HeaderName::from_bytes(key.trim().as_bytes())
            .map_err(|_| format!("invalid quota header name: {key}"))?;
        let rendered = render_template(group, rule, value);
        let header_value = HeaderValue::from_str(rendered.trim())
            .map_err(|_| format!("invalid quota header value for {key}"))?;
        headers.insert(key_name, header_value);
    }

    let auth_header_name = rule.quota.auth_header.trim();
    let resolved_token = if rule.quota.use_rule_token {
        rule.token.trim()
    } else {
        rule.quota.custom_token.trim()
    };

    if !auth_header_name.is_empty() && !resolved_token.is_empty() {
        let key_name = HeaderName::from_bytes(auth_header_name.as_bytes())
            .map_err(|_| format!("invalid auth header name: {auth_header_name}"))?;
        if !headers.contains_key(&key_name) {
            let auth_value = if rule.quota.auth_scheme.trim().is_empty() {
                resolved_token.to_string()
            } else {
                format!("{} {}", rule.quota.auth_scheme.trim(), resolved_token)
            };
            let header_value = HeaderValue::from_str(auth_value.trim())
                .map_err(|_| "invalid auth header value".to_string())?;
            headers.insert(key_name, header_value);
        }
    }

    Ok(headers)
}

fn render_body_suffix(raw_body: &str) -> String {
    let trimmed = raw_body.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let max_len = 140usize;
    let shown = trimmed.chars().take(max_len).collect::<String>();
    if trimmed.chars().count() > max_len {
        format!(": {shown}...")
    } else {
        format!(": {shown}")
    }
}

fn map_payload_to_snapshot(
    snapshot: &mut RuleQuotaSnapshot,
    rule: &Rule,
    payload: &Value,
) -> Result<(), String> {
    let remaining = evaluate_mapping_number(&rule.quota.response.remaining, payload)?;
    let total = evaluate_mapping_number(&rule.quota.response.total, payload)?;
    let unit = evaluate_mapping_string(&rule.quota.response.unit, payload)?;
    let reset_at = evaluate_mapping_string(&rule.quota.response.reset_at, payload)?;

    snapshot.remaining = remaining;
    snapshot.total = total;
    snapshot.unit = unit;
    snapshot.reset_at = reset_at;

    let mut computed_percent = match (remaining, total) {
        (Some(rem), Some(all)) if all > 0.0 => Some((rem / all) * 100.0),
        _ => None,
    };
    if computed_percent.is_none() {
        if let Some(rem) = remaining {
            if (0.0..=1.0).contains(&rem) {
                computed_percent = Some(rem * 100.0);
            }
        }
    }

    let normalized_remaining = match rule.quota.unit_type {
        QuotaUnitType::Percentage => computed_percent.or(remaining),
        QuotaUnitType::Amount | QuotaUnitType::Tokens => remaining,
    };
    snapshot.percent = computed_percent;

    let low_threshold = if rule.quota.low_threshold_percent.is_finite()
        && rule.quota.low_threshold_percent >= 0.0
    {
        rule.quota.low_threshold_percent
    } else {
        10.0
    };

    snapshot.status = match normalized_remaining {
        None => QuotaStatus::Unknown,
        Some(rem) if rem <= 0.0 => QuotaStatus::Empty,
        Some(rem) => {
            if rem < low_threshold {
                QuotaStatus::Low
            } else {
                QuotaStatus::Ok
            }
        }
    };

    Ok(())
}

#[derive(Debug)]
enum MappingSpec {
    Empty,
    Path(String),
    Expr(String),
    Literal(Value),
}

fn parse_mapping_spec(source: &Value) -> MappingSpec {
    match source {
        Value::Null => MappingSpec::Empty,
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                MappingSpec::Empty
            } else if is_expression_string(trimmed) {
                MappingSpec::Expr(trimmed.to_string())
            } else {
                MappingSpec::Path(trimmed.to_string())
            }
        }
        Value::Object(obj) => {
            if let Some(expr) = obj.get("expr").and_then(Value::as_str) {
                if !expr.trim().is_empty() {
                    return MappingSpec::Expr(expr.trim().to_string());
                }
            }
            if let Some(path) = obj.get("path").and_then(Value::as_str) {
                if !path.trim().is_empty() {
                    return MappingSpec::Path(path.trim().to_string());
                }
            }
            if let Some(value) = obj.get("value") {
                return MappingSpec::Literal(value.clone());
            }
            MappingSpec::Empty
        }
        Value::Number(_) | Value::Bool(_) => MappingSpec::Literal(source.clone()),
        _ => MappingSpec::Empty,
    }
}

fn is_expression_string(source: &str) -> bool {
    if source.contains("path(") {
        return true;
    }
    let contains_operator = source.contains('+')
        || source.contains('*')
        || source.contains('/')
        || source.contains('(')
        || source.contains(')')
        || source.starts_with('-')
        || source.contains(" - ");
    contains_operator
}

fn evaluate_mapping_number(source: &Value, payload: &Value) -> Result<Option<f64>, String> {
    match parse_mapping_spec(source) {
        MappingSpec::Empty => Ok(None),
        MappingSpec::Path(path) => Ok(extract_json_path(payload, &path).and_then(value_to_f64)),
        MappingSpec::Expr(expr) => Ok(Some(evaluate_expression(&expr, payload)?)),
        MappingSpec::Literal(value) => Ok(value_to_f64(&value)),
    }
}

fn evaluate_mapping_string(source: &Value, payload: &Value) -> Result<Option<String>, String> {
    match parse_mapping_spec(source) {
        MappingSpec::Empty => Ok(None),
        MappingSpec::Path(path) => Ok(extract_json_path(payload, &path).and_then(value_to_string)),
        MappingSpec::Expr(expr) => Ok(Some(format!("{}", evaluate_expression(&expr, payload)?))),
        MappingSpec::Literal(value) => Ok(value_to_string(&value)),
    }
}

fn value_to_f64(value: &Value) -> Option<f64> {
    if let Some(v) = value.as_f64() {
        return Some(v);
    }
    value.as_str().and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        let stripped_percent = trimmed.trim_end_matches('%').trim();
        let normalized = stripped_percent.replace(',', "");
        normalized.parse::<f64>().ok()
    })
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => Some(text.to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn evaluate_expression(expression: &str, payload: &Value) -> Result<f64, String> {
    let normalized = normalize_expression(expression);
    let mut parser = ExprParser::new(&normalized, payload);
    let result = parser.parse_expression()?;
    parser.skip_whitespace();
    if parser.pos < parser.input.len() {
        return Err(format!(
            "unexpected token near byte {} in expression",
            parser.pos
        ));
    }
    if !result.is_finite() {
        return Err("expression result is not finite".to_string());
    }
    Ok(result)
}

fn normalize_expression(raw: &str) -> String {
    if raw.contains("path(") {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len() + 16);
    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '$' {
            let start = i;
            i += 1;
            while i < chars.len() {
                let current = chars[i];
                if current.is_whitespace() || matches!(current, '+' | '-' | '*' | '/' | '(' | ')')
                {
                    break;
                }
                i += 1;
            }
            let token: String = chars[start..i].iter().collect();
            out.push_str("path('");
            out.push_str(&token.replace('\'', "\\'"));
            out.push_str("')");
            continue;
        }
        out.push(ch);
        i += 1;
    }
    out
}

struct ExprParser<'a> {
    input: &'a str,
    pos: usize,
    payload: &'a Value,
}

impl<'a> ExprParser<'a> {
    fn new(input: &'a str, payload: &'a Value) -> Self {
        Self {
            input,
            pos: 0,
            payload,
        }
    }

    fn parse_expression(&mut self) -> Result<f64, String> {
        self.parse_add_sub()
    }

    fn parse_add_sub(&mut self) -> Result<f64, String> {
        let mut value = self.parse_mul_div()?;
        loop {
            self.skip_whitespace();
            if self.consume_char('+') {
                value += self.parse_mul_div()?;
            } else if self.consume_char('-') {
                value -= self.parse_mul_div()?;
            } else {
                break;
            }
        }
        Ok(value)
    }

    fn parse_mul_div(&mut self) -> Result<f64, String> {
        let mut value = self.parse_unary()?;
        loop {
            self.skip_whitespace();
            if self.consume_char('*') {
                value *= self.parse_unary()?;
            } else if self.consume_char('/') {
                let divisor = self.parse_unary()?;
                if divisor.abs() < f64::EPSILON {
                    return Err("division by zero in expression".to_string());
                }
                value /= divisor;
            } else {
                break;
            }
        }
        Ok(value)
    }

    fn parse_unary(&mut self) -> Result<f64, String> {
        self.skip_whitespace();
        if self.consume_char('-') {
            return Ok(-self.parse_unary()?);
        }
        if self.consume_char('+') {
            return self.parse_unary();
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<f64, String> {
        self.skip_whitespace();
        if self.consume_char('(') {
            let value = self.parse_expression()?;
            self.skip_whitespace();
            if !self.consume_char(')') {
                return Err("missing closing ')' in expression".to_string());
            }
            return Ok(value);
        }

        if self.peek_identifier("path") {
            self.pos += "path".len();
            self.skip_whitespace();
            if !self.consume_char('(') {
                return Err("missing '(' after path function".to_string());
            }
            self.skip_whitespace();
            let path = self.parse_string_literal()?;
            self.skip_whitespace();
            if !self.consume_char(')') {
                return Err("missing ')' after path function".to_string());
            }
            let value = extract_json_path(self.payload, &path)
                .and_then(value_to_f64)
                .ok_or_else(|| format!("path value is not numeric: {path}"))?;
            return Ok(value);
        }

        self.parse_number()
    }

    fn parse_number(&mut self) -> Result<f64, String> {
        self.skip_whitespace();
        let start = self.pos;
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_digit() || ch == '.' {
                self.pos += ch.len_utf8();
            } else {
                break;
            }
        }
        if start == self.pos {
            return Err(format!("expected number at byte {}", self.pos));
        }
        let raw = &self.input[start..self.pos];
        raw.parse::<f64>()
            .map_err(|_| format!("invalid number in expression: {raw}"))
    }

    fn parse_string_literal(&mut self) -> Result<String, String> {
        let quote = self
            .peek_char()
            .ok_or_else(|| "expected quoted string in expression".to_string())?;
        if quote != '\'' && quote != '"' {
            return Err("expected quoted string in expression".to_string());
        }
        self.pos += quote.len_utf8();
        let mut out = String::new();
        while let Some(ch) = self.peek_char() {
            self.pos += ch.len_utf8();
            if ch == quote {
                return Ok(out);
            }
            if ch == '\\' {
                let escaped = self
                    .peek_char()
                    .ok_or_else(|| "invalid escape in expression string".to_string())?;
                self.pos += escaped.len_utf8();
                out.push(escaped);
            } else {
                out.push(ch);
            }
        }
        Err("unterminated string literal in expression".to_string())
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() {
                self.pos += ch.len_utf8();
            } else {
                break;
            }
        }
    }

    fn peek_identifier(&self, expected: &str) -> bool {
        self.input
            .get(self.pos..)
            .map(|rest| rest.starts_with(expected))
            .unwrap_or(false)
    }

    fn consume_char(&mut self, expected: char) -> bool {
        if self.peek_char() == Some(expected) {
            self.pos += expected.len_utf8();
            true
        } else {
            false
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.input.get(self.pos..)?.chars().next()
    }
}

fn extract_json_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let chars: Vec<char> = path.chars().collect();
    if chars.first().copied() != Some('$') {
        return None;
    }

    let mut current = root;
    let mut i = 1usize;
    while i < chars.len() {
        match chars[i] {
            c if c.is_whitespace() => i += 1,
            '.' => {
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != '.' && chars[i] != '[' {
                    i += 1;
                }
                if start == i {
                    return None;
                }
                let key: String = chars[start..i].iter().collect();
                current = current.get(key.trim())?;
            }
            '[' => {
                i += 1;
                if i >= chars.len() {
                    return None;
                }
                if chars[i] == '\'' || chars[i] == '"' {
                    let quote = chars[i];
                    i += 1;
                    let start = i;
                    while i < chars.len() && chars[i] != quote {
                        i += 1;
                    }
                    if i >= chars.len() {
                        return None;
                    }
                    let key: String = chars[start..i].iter().collect();
                    i += 1;
                    if i >= chars.len() || chars[i] != ']' {
                        return None;
                    }
                    i += 1;
                    current = current.get(key.trim())?;
                } else {
                    let start = i;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                    if start == i || i >= chars.len() || chars[i] != ']' {
                        return None;
                    }
                    let index: usize = chars[start..i].iter().collect::<String>().parse().ok()?;
                    i += 1;
                    current = current.get(index)?;
                }
            }
            _ => return None,
        }
    }

    Some(current)
}

#[cfg(test)]
mod tests {
    use super::{evaluate_expression, extract_json_path, map_payload_to_snapshot};
    use crate::models::{
        default_rule_quota_config, default_quota_low_threshold_percent, QuotaStatus,
        QuotaUnitType,
        RuleQuotaResponseMapping,
    };
    use crate::models::{Rule, RuleProtocol};
    use serde_json::{json, Value};
    use std::collections::HashMap;

    fn build_rule_with_mapping(mapping: RuleQuotaResponseMapping) -> Rule {
        let mut quota = default_rule_quota_config();
        quota.enabled = true;
        quota.provider = "custom".to_string();
        quota.endpoint = "https://example.com/quota".to_string();
        quota.low_threshold_percent = default_quota_low_threshold_percent();
        quota.response = mapping;

        Rule {
            id: "rule-1".to_string(),
            name: "Rule 1".to_string(),
            protocol: RuleProtocol::Openai,
            token: "tok".to_string(),
            api_address: "https://api.example.com".to_string(),
            default_model: "gpt-4.1".to_string(),
            model_mappings: HashMap::new(),
            quota,
        }
    }

    #[test]
    fn extract_json_path_reads_nested_and_array_paths() {
        let payload = json!({
            "data": {
                "balances": [
                    { "remaining": "12.5" }
                ]
            }
        });
        assert_eq!(
            extract_json_path(&payload, "$.data.balances[0].remaining").and_then(|v| v.as_str()),
            Some("12.5")
        );
    }

    #[test]
    fn evaluate_expression_supports_inline_path_references() {
        let payload = json!({
            "data": {
                "remaining_balance": 25,
                "remaining_total": 100
            }
        });
        let result = evaluate_expression("$.data.remaining_balance/$.data.remaining_total", &payload)
            .expect("expression should parse");
        assert!((result - 0.25).abs() < 1e-8);
    }

    #[test]
    fn evaluate_expression_supports_path_function() {
        let payload = json!({
            "data": {
                "remaining_balance": 50,
                "remaining_total": 200
            }
        });
        let result = evaluate_expression(
            "path('$.data.remaining_balance') / path('$.data.remaining_total')",
            &payload,
        )
        .expect("expression should parse");
        assert!((result - 0.25).abs() < 1e-8);
    }

    #[test]
    fn map_payload_to_snapshot_supports_path_mapping() {
        let rule = build_rule_with_mapping(RuleQuotaResponseMapping {
            remaining: json!("$.quota.remaining"),
            unit: json!("$.quota.unit"),
            total: json!("$.quota.total"),
            reset_at: json!("$.quota.resetAt"),
        });
        let payload = json!({
            "quota": {
                "remaining": 20,
                "total": 100,
                "unit": "USD",
                "resetAt": "2026-03-31T00:00:00Z"
            }
        });
        let mut snapshot = super::new_snapshot(
            &crate::models::Group {
                id: "g1".to_string(),
                name: "G1".to_string(),
                models: vec![],
                active_rule_id: None,
                rules: vec![],
            },
            &rule,
        );
        map_payload_to_snapshot(&mut snapshot, &rule, &payload).expect("mapping should succeed");

        assert_eq!(snapshot.remaining, Some(20.0));
        assert_eq!(snapshot.total, Some(100.0));
        assert_eq!(snapshot.unit.as_deref(), Some("USD"));
        assert_eq!(snapshot.status, QuotaStatus::Ok);
        assert_eq!(snapshot.percent, Some(20.0));
    }

    #[test]
    fn map_payload_to_snapshot_supports_expr_mapping() {
        let rule = build_rule_with_mapping(RuleQuotaResponseMapping {
            remaining: json!({
                "expr": "$.data.remaining_balance/$.data.remaining_total"
            }),
            unit: json!("ratio"),
            total: Value::Null,
            reset_at: Value::Null,
        });
        let payload = json!({
            "data": {
                "remaining_balance": 5,
                "remaining_total": 100
            }
        });
        let mut snapshot = super::new_snapshot(
            &crate::models::Group {
                id: "g1".to_string(),
                name: "G1".to_string(),
                models: vec![],
                active_rule_id: None,
                rules: vec![],
            },
            &rule,
        );
        map_payload_to_snapshot(&mut snapshot, &rule, &payload).expect("mapping should succeed");

        assert_eq!(snapshot.remaining, Some(0.05));
        assert_eq!(snapshot.percent, Some(5.0));
        assert_eq!(snapshot.status, QuotaStatus::Low);
    }

    #[test]
    fn map_payload_to_snapshot_uses_remaining_value_for_amount_threshold() {
        let mut rule = build_rule_with_mapping(RuleQuotaResponseMapping {
            remaining: json!("$.quota.remaining"),
            unit: Value::Null,
            total: json!("$.quota.total"),
            reset_at: Value::Null,
        });
        rule.quota.unit_type = QuotaUnitType::Amount;
        rule.quota.low_threshold_percent = 10.0;

        let payload = json!({
            "quota": {
                "remaining": 20,
                "total": 100
            }
        });
        let mut snapshot = super::new_snapshot(
            &crate::models::Group {
                id: "g1".to_string(),
                name: "G1".to_string(),
                models: vec![],
                active_rule_id: None,
                rules: vec![],
            },
            &rule,
        );
        map_payload_to_snapshot(&mut snapshot, &rule, &payload).expect("mapping should succeed");
        assert_eq!(snapshot.status, QuotaStatus::Ok);
    }

    #[test]
    fn map_payload_to_snapshot_parses_percentage_string_remaining() {
        let mut rule = build_rule_with_mapping(RuleQuotaResponseMapping {
            remaining: json!("$.quota.remaining"),
            unit: Value::Null,
            total: Value::Null,
            reset_at: Value::Null,
        });
        rule.quota.unit_type = QuotaUnitType::Percentage;
        rule.quota.low_threshold_percent = 10.0;
        let payload = json!({
            "quota": {
                "remaining": "7.5%"
            }
        });
        let mut snapshot = super::new_snapshot(
            &crate::models::Group {
                id: "g1".to_string(),
                name: "G1".to_string(),
                models: vec![],
                active_rule_id: None,
                rules: vec![],
            },
            &rule,
        );
        map_payload_to_snapshot(&mut snapshot, &rule, &payload).expect("mapping should succeed");

        assert_eq!(snapshot.remaining, Some(7.5));
        assert_eq!(snapshot.status, QuotaStatus::Low);
    }
}
