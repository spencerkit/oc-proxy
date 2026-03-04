//! Module Overview
//! Quota response parser and expression evaluator.
//! Extracts normalized remaining/total/percent values across heterogeneous provider payloads.

use crate::models::{QuotaStatus, QuotaUnitType, Rule};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParsedQuotaSnapshot {
    pub remaining: Option<f64>,
    pub total: Option<f64>,
    pub unit: Option<String>,
    pub reset_at: Option<String>,
    pub percent: Option<f64>,
    pub status: QuotaStatus,
}

/// Parses quota payload.
pub(crate) fn parse_quota_payload(
    rule: &Rule,
    payload: &Value,
) -> Result<ParsedQuotaSnapshot, String> {
    let remaining = evaluate_mapping_number(&rule.quota.response.remaining, payload)?;
    let total = evaluate_mapping_number(&rule.quota.response.total, payload)?;
    let unit = evaluate_mapping_string(&rule.quota.response.unit, payload)?;
    let reset_at = evaluate_mapping_string(&rule.quota.response.reset_at, payload)?;

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

    let low_threshold = if rule.quota.low_threshold_percent.is_finite()
        && rule.quota.low_threshold_percent >= 0.0
    {
        rule.quota.low_threshold_percent
    } else {
        10.0
    };

    let status = match normalized_remaining {
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

    Ok(ParsedQuotaSnapshot {
        remaining,
        total,
        unit,
        reset_at,
        percent: computed_percent,
        status,
    })
}

#[derive(Debug)]
enum MappingSpec {
    Empty,
    Path(String),
    Expr(String),
    Literal(Value),
}

/// Parses mapping spec.
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

/// Returns whether expression string is true.
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

/// Performs evaluate mapping number.
fn evaluate_mapping_number(source: &Value, payload: &Value) -> Result<Option<f64>, String> {
    match parse_mapping_spec(source) {
        MappingSpec::Empty => Ok(None),
        MappingSpec::Path(path) => Ok(extract_json_path(payload, &path).and_then(value_to_f64)),
        MappingSpec::Expr(expr) => Ok(Some(evaluate_expression(&expr, payload)?)),
        MappingSpec::Literal(value) => Ok(value_to_f64(&value)),
    }
}

/// Performs evaluate mapping string.
fn evaluate_mapping_string(source: &Value, payload: &Value) -> Result<Option<String>, String> {
    match parse_mapping_spec(source) {
        MappingSpec::Empty => Ok(None),
        MappingSpec::Path(path) => Ok(extract_json_path(payload, &path).and_then(value_to_string)),
        MappingSpec::Expr(expr) => Ok(Some(format!("{}", evaluate_expression(&expr, payload)?))),
        MappingSpec::Literal(value) => Ok(value_to_string(&value)),
    }
}

/// Performs value to f64.
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

/// Performs value to string.
fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => Some(text.to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// Performs evaluate expression.
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

/// Normalizes expression for this module's workflow.
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
                if current.is_whitespace() || matches!(current, '+' | '-' | '*' | '/' | '(' | ')') {
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
    /// Performs new.
    fn new(input: &'a str, payload: &'a Value) -> Self {
        Self {
            input,
            pos: 0,
            payload,
        }
    }

    /// Parses expression.
    fn parse_expression(&mut self) -> Result<f64, String> {
        self.parse_add_sub()
    }

    /// Parses add sub.
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

    /// Parses mul div.
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

    /// Parses unary.
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

    /// Parses primary.
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

    /// Parses number.
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

    /// Parses string literal.
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

    /// Performs skip whitespace.
    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() {
                self.pos += ch.len_utf8();
            } else {
                break;
            }
        }
    }

    /// Performs peek IDentifier.
    fn peek_identifier(&self, expected: &str) -> bool {
        self.input
            .get(self.pos..)
            .map(|rest| rest.starts_with(expected))
            .unwrap_or(false)
    }

    /// Performs consume char.
    fn consume_char(&mut self, expected: char) -> bool {
        if self.peek_char() == Some(expected) {
            self.pos += expected.len_utf8();
            true
        } else {
            false
        }
    }

    /// Performs peek char.
    fn peek_char(&self) -> Option<char> {
        self.input.get(self.pos..)?.chars().next()
    }
}

/// Extracts a JSON value by a simplified JSONPath expression.
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
    use super::{evaluate_expression, extract_json_path, parse_quota_payload};
    use crate::models::{
        default_quota_low_threshold_percent, default_rule_cost_config, default_rule_quota_config,
        QuotaStatus, QuotaUnitType,
        RuleQuotaResponseMapping,
    };
    use crate::models::{Rule, RuleProtocol};
    use serde_json::{json, Value};
    use std::collections::HashMap;

    /// Builds rule with mapping.
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
            cost: default_rule_cost_config(),
        }
    }

    #[test]
    /// Extracts JSON path reads nested and array paths for this module's workflow.
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
    /// Extracts JSON path returns none for missing path and bad index for this module's workflow.
    fn extract_json_path_returns_none_for_missing_path_and_bad_index() {
        let payload = json!({"data": {"items": [1]}});
        assert!(extract_json_path(&payload, "$.data.missing").is_none());
        assert!(extract_json_path(&payload, "$.data.items[99]").is_none());
    }

    #[test]
    /// Performs evaluate expression supports inline path references.
    fn evaluate_expression_supports_inline_path_references() {
        let payload = json!({
            "data": {
                "remaining_balance": 25,
                "remaining_total": 100
            }
        });
        let result =
            evaluate_expression("$.data.remaining_balance/$.data.remaining_total", &payload)
                .expect("expression should parse");
        assert!((result - 0.25).abs() < 1e-8);
    }

    #[test]
    /// Performs evaluate expression supports path function.
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
    /// Performs evaluate expression rejects invalid token and division by zero.
    fn evaluate_expression_rejects_invalid_token_and_division_by_zero() {
        let payload = json!({"x": 1});
        let err = evaluate_expression("1 + foo", &payload).expect_err("must fail");
        assert!(err.contains("expected number") || err.contains("unexpected token"));

        let err = evaluate_expression("1 / 0", &payload).expect_err("must fail");
        assert!(err.contains("division by zero"));
    }

    #[test]
    /// Performs evaluate expression rejects non finite result.
    fn evaluate_expression_rejects_non_finite_result() {
        let payload = json!({});
        let huge_number = format!("1{}", "0".repeat(400));
        let err = evaluate_expression(&huge_number, &payload).expect_err("must fail");
        assert!(err.contains("not finite"));
    }

    #[test]
    /// Parses quota payload supports path mapping.
    fn parse_quota_payload_supports_path_mapping() {
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

        let parsed = parse_quota_payload(&rule, &payload).expect("mapping should succeed");
        assert_eq!(parsed.remaining, Some(20.0));
        assert_eq!(parsed.total, Some(100.0));
        assert_eq!(parsed.unit.as_deref(), Some("USD"));
        assert_eq!(parsed.status, QuotaStatus::Ok);
        assert_eq!(parsed.percent, Some(20.0));
    }

    #[test]
    /// Parses quota payload supports expr mapping.
    fn parse_quota_payload_supports_expr_mapping() {
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

        let parsed = parse_quota_payload(&rule, &payload).expect("mapping should succeed");
        assert_eq!(parsed.remaining, Some(0.05));
        assert_eq!(parsed.percent, Some(5.0));
        assert_eq!(parsed.status, QuotaStatus::Low);
    }

    #[test]
    /// Parses quota payload parses percentage and comma numbers.
    fn parse_quota_payload_parses_percentage_and_comma_numbers() {
        let mut rule = build_rule_with_mapping(RuleQuotaResponseMapping {
            remaining: json!("$.quota.remaining"),
            unit: Value::Null,
            total: Value::Null,
            reset_at: Value::Null,
        });
        rule.quota.unit_type = QuotaUnitType::Amount;
        let payload = json!({
            "quota": {
                "remaining": "1,234.5%"
            }
        });

        let parsed = parse_quota_payload(&rule, &payload).expect("mapping should succeed");
        assert_eq!(parsed.remaining, Some(1234.5));
    }

    #[test]
    /// Parses quota payload status by unit type.
    fn parse_quota_payload_status_by_unit_type() {
        let mut percentage_rule = build_rule_with_mapping(RuleQuotaResponseMapping {
            remaining: json!("$.quota.remaining"),
            unit: Value::Null,
            total: Value::Null,
            reset_at: Value::Null,
        });
        percentage_rule.quota.unit_type = QuotaUnitType::Percentage;
        percentage_rule.quota.low_threshold_percent = 10.0;

        let parsed = parse_quota_payload(&percentage_rule, &json!({"quota": {"remaining": 0.05}}))
            .expect("mapping should succeed");
        assert_eq!(parsed.status, QuotaStatus::Low);

        let mut amount_rule = percentage_rule.clone();
        amount_rule.quota.unit_type = QuotaUnitType::Amount;
        let parsed = parse_quota_payload(&amount_rule, &json!({"quota": {"remaining": 20}}))
            .expect("mapping should succeed");
        assert_eq!(parsed.status, QuotaStatus::Ok);

        let mut tokens_rule = percentage_rule.clone();
        tokens_rule.quota.unit_type = QuotaUnitType::Tokens;
        let parsed = parse_quota_payload(&tokens_rule, &json!({"quota": {"remaining": 5}}))
            .expect("mapping should succeed");
        assert_eq!(parsed.status, QuotaStatus::Low);
    }

    #[test]
    /// Parses quota payload threshold boundaries.
    fn parse_quota_payload_threshold_boundaries() {
        let mut rule = build_rule_with_mapping(RuleQuotaResponseMapping {
            remaining: json!("$.quota.remaining"),
            unit: Value::Null,
            total: Value::Null,
            reset_at: Value::Null,
        });

        rule.quota.low_threshold_percent = -1.0;
        let parsed = parse_quota_payload(&rule, &json!({"quota": {"remaining": 5}}))
            .expect("mapping should succeed");
        assert_eq!(parsed.status, QuotaStatus::Low);

        rule.quota.low_threshold_percent = 0.0;
        let parsed = parse_quota_payload(&rule, &json!({"quota": {"remaining": 0.0001}}))
            .expect("mapping should succeed");
        assert_eq!(parsed.status, QuotaStatus::Ok);

        let parsed = parse_quota_payload(&rule, &json!({"quota": {"remaining": 0}}))
            .expect("mapping should succeed");
        assert_eq!(parsed.status, QuotaStatus::Empty);
    }

    #[test]
    /// Runs a unit test for the expected behavior contract.
    fn contract_parse_quota_payload_snapshot() {
        let payload: Value = serde_json::from_str(include_str!(
            "../contract_fixtures/quota/parse_quota_payload.payload.json"
        ))
        .expect("contract payload must be valid json");
        let expected: Value = serde_json::from_str(include_str!(
            "../contract_fixtures/quota/parse_quota_payload.expected.json"
        ))
        .expect("contract expected must be valid json");

        let mut rule = build_rule_with_mapping(RuleQuotaResponseMapping {
            remaining: json!("$.quota.remaining_ratio"),
            unit: json!("$.quota.unit"),
            total: Value::Null,
            reset_at: json!("$.quota.resetAt"),
        });
        rule.quota.unit_type = QuotaUnitType::Percentage;
        rule.quota.low_threshold_percent = 10.0;

        let parsed = parse_quota_payload(&rule, &payload).expect("mapping should succeed");
        let actual = json!({
            "remaining": parsed.remaining,
            "total": parsed.total,
            "unit": parsed.unit,
            "reset_at": parsed.reset_at,
            "percent": parsed.percent,
            "status": format!("{:?}", parsed.status),
        });
        assert_eq!(actual, expected);
    }
}
