//! Condition evaluator for gateway routing.
//!
//! Evaluates simple expressions like `"x == 5"`, `"amount > 100"`, or
//! bare `"flag"` (truthy check) against a variable map.

use std::cmp::Ordering;
use std::collections::HashMap;

use serde_json::Value;

/// Evaluates a simple condition expression against token variables.
///
/// Supported forms:
/// - `"variable == value"` / `"variable != value"`
/// - `"variable > value"` / `"variable < value"` / `"variable >= value"` / `"variable <= value"`
/// - `"variable"` (truthy check: non-null, non-false, non-zero, non-empty-string)
///
/// Returns `false` if the variable is missing or the expression is malformed.
pub fn evaluate_condition(expr: &str, variables: &HashMap<String, Value>) -> bool {
    let expr = expr.trim();
    if expr.is_empty() {
        return false;
    }

    // Try comparison operators (longest first to avoid prefix conflicts)
    for op in ["==", "!=", ">=", "<=", ">", "<"] {
        if let Some(idx) = expr.find(op) {
            let var_name = expr[..idx].trim();
            let rhs_str = expr[idx + op.len()..].trim();

            let lhs = match variables.get(var_name) {
                Some(v) => v,
                None => return false,
            };

            // Parse RHS as a JSON value for comparison
            let rhs = parse_rhs(rhs_str);

            return match op {
                "==" => values_eq(lhs, &rhs),
                "!=" => !values_eq(lhs, &rhs),
                ">" => values_cmp(lhs, &rhs) == Some(Ordering::Greater),
                "<" => values_cmp(lhs, &rhs) == Some(Ordering::Less),
                ">=" => values_cmp(lhs, &rhs).is_some_and(|o| o != Ordering::Less),
                "<=" => values_cmp(lhs, &rhs).is_some_and(|o| o != Ordering::Greater),
                _ => false,
            };
        }
    }

    // Fallback: truthy check on a single variable name
    match variables.get(expr) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Null) | None => false,
        // Arrays and objects are truthy
        Some(_) => true,
    }
}

/// Parses a right-hand-side string into a `serde_json::Value`.
fn parse_rhs(s: &str) -> Value {
    // Strip surrounding quotes (single or double) for string comparison
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        return Value::String(s[1..s.len() - 1].to_string());
    }
    // Boolean literals
    if s == "true" {
        return Value::Bool(true);
    }
    if s == "false" {
        return Value::Bool(false);
    }
    // Null
    if s == "null" {
        return Value::Null;
    }
    // Try number
    if let Ok(n) = s.parse::<i64>() {
        return Value::Number(n.into());
    }
    if let Ok(n) = s.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(n) {
            return Value::Number(n);
        }
    }
    // Fallback: treat as plain string
    Value::String(s.to_string())
}

/// Equality comparison for JSON values.
fn values_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            a.as_f64().zip(b.as_f64()).is_some_and(|(x, y)| (x - y).abs() < f64::EPSILON)
        }
        _ => a == b,
    }
}

/// Ordering comparison for JSON values (numbers only).
fn values_cmp(a: &Value, b: &Value) -> Option<Ordering> {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let fa = a.as_f64()?;
            let fb = b.as_f64()?;
            fa.partial_cmp(&fb)
        }
        _ => None,
    }
}
