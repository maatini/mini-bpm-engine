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
    // Strip surrounding quotes (single or double) for string comparison.
    // Use char boundaries to avoid panics on multi-byte UTF-8 input.
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        if s.len() >= 2 {
            let start = s.char_indices().nth(1).map(|(i, _)| i).unwrap_or(1);
            let end = s.char_indices().next_back().map(|(i, _)| i).unwrap_or(s.len());
            if start <= end {
                return Value::String(s[start..end].to_string());
            }
        }
        return Value::String(String::new());
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
        // Rust clippy warning let_and_return fix
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
        (Value::Number(a), Value::Number(b)) => a
            .as_f64()
            .zip(b.as_f64())
            .is_some_and(|(x, y)| (x - y).abs() < f64::EPSILON),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_equality() {
        let mut vars = HashMap::new();
        vars.insert("status".to_string(), json!("approved"));
        vars.insert("count".to_string(), json!(5));

        assert!(evaluate_condition("status == 'approved'", &vars));
        assert!(evaluate_condition("status == \"approved\"", &vars));
        assert!(!evaluate_condition("status == 'rejected'", &vars));
        assert!(evaluate_condition("status != 'rejected'", &vars));

        assert!(evaluate_condition("count == 5", &vars));
        assert!(!evaluate_condition("count == 6", &vars));
    }

    #[test]
    fn test_numeric_comparisons() {
        let mut vars = HashMap::new();
        vars.insert("amount".to_string(), json!(150.5));
        vars.insert("score".to_string(), json!(10));

        assert!(evaluate_condition("amount > 100", &vars));
        assert!(evaluate_condition("amount >= 150.5", &vars));
        assert!(evaluate_condition("amount < 200", &vars));
        assert!(evaluate_condition("amount <= 150.5", &vars));
        assert!(!evaluate_condition("amount > 200", &vars));

        assert!(evaluate_condition("score > 5", &vars));
        assert!(evaluate_condition("score < 15", &vars));
    }

    #[test]
    fn test_values_eq_numbers_vs_default() {
        // Catches: delete match arm (Number, Number), replace < with <=
        // Numbers use epsilon-based comparison, not default ==
        let a = json!(1.0);
        let b = json!(1);
        assert!(values_eq(&a, &b));

        let a = json!(0.1 + 0.2);
        let b = json!(0.3);
        // f64 epsilon comparison should catch near-equal floats
        assert!(values_eq(&a, &b));

        // Different numbers must not be equal
        let a = json!(1.0);
        let b = json!(2.0);
        assert!(!values_eq(&a, &b));
    }

    #[test]
    fn test_parse_rhs_quoted_strings() {
        // Catches: replace && with || in parse_rhs (line 66)
        // Both start AND end quote must match for stripping
        let val = parse_rhs("\"hello\"");
        assert_eq!(val, json!("hello"));

        let val = parse_rhs("'world'");
        assert_eq!(val, json!("world"));

        // Mismatched quotes: should NOT strip
        let val = parse_rhs("\"hello'");
        assert_eq!(val, json!("\"hello'"));

        let val = parse_rhs("'hello\"");
        assert_eq!(val, json!("'hello\""));
    }

    #[test]
    fn test_truthy() {
        let mut vars = HashMap::new();
        vars.insert("is_valid".to_string(), json!(true));
        vars.insert("is_empty".to_string(), json!(false));
        vars.insert("null_val".to_string(), json!(null));
        vars.insert("name".to_string(), json!("Bob"));
        vars.insert("empty_str".to_string(), json!(""));
        vars.insert("num".to_string(), json!(1));
        vars.insert("zero".to_string(), json!(0));

        assert!(evaluate_condition("is_valid", &vars));
        assert!(!evaluate_condition("is_empty", &vars));
        assert!(!evaluate_condition("null_val", &vars));
        assert!(evaluate_condition("name", &vars));
        assert!(!evaluate_condition("empty_str", &vars));
        assert!(evaluate_condition("num", &vars));
        assert!(!evaluate_condition("zero", &vars));
        assert!(!evaluate_condition("missing_var", &vars));
    }
}
