//! Fuzz target: History Diff Calculation
//!
//! Fuzzes `calculate_diff` and `calculate_diff_from_snapshot` with structured
//! input generated via the `arbitrary` crate.
//!
//! Attack vectors:
//! - Strings near the 1024-byte truncation boundary (including multi-byte UTF-8)
//! - Arrays near the 128-element boundary
//! - Deeply nested JSON objects
//! - File-reference-shaped JSON (`{"type":"file","filename":"...","size_bytes":N}`)
//! - All `InstanceState` variants

#![no_main]
use arbitrary::Unstructured;
use engine_core::history::{DiffSnapshot, calculate_diff, calculate_diff_from_snapshot};
use engine_core::runtime::{InstanceState, ProcessInstance};
use engine_core::domain::Token;
use libfuzzer_sys::fuzz_target;
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

/// Generates a JSON Value from fuzzer-driven choices.
fn arb_value(u: &mut Unstructured<'_>, depth: u8) -> arbitrary::Result<Value> {
    if depth > 4 {
        // Prevent unbounded recursion
        return Ok(Value::Null);
    }

    let choice: u8 = u.int_in_range(0..=7)?;
    match choice {
        0 => Ok(Value::Null),
        1 => Ok(Value::Bool(u.arbitrary()?)),
        2 => Ok(Value::Number(serde_json::Number::from(u.arbitrary::<i64>()?))),
        3 => Ok(Value::Number(
            serde_json::Number::from_f64(u.arbitrary::<f64>()?)
                .unwrap_or_else(|| serde_json::Number::from(0)),
        )),
        4 => {
            // Regular string
            let s: String = u.arbitrary()?;
            Ok(Value::String(s))
        }
        5 => {
            // String near the 1024-char truncation boundary — including multi-byte chars
            let base_len: u16 = u.int_in_range(1000..=1100)?;
            let use_multibyte: bool = u.arbitrary()?;
            let s: String = if use_multibyte {
                // Mix of ASCII and multi-byte chars (e.g. 'ä', '€', '🦀')
                let chars = ['a', 'ä', '€', '🦀', 'x', 'ö', '日', '本'];
                (0..base_len)
                    .map(|_| {
                        let idx = u.int_in_range(0..=7).unwrap_or(0);
                        chars[idx as usize]
                    })
                    .collect()
            } else {
                "A".repeat(base_len as usize)
            };
            Ok(Value::String(s))
        }
        6 => {
            // Array near the 128-element boundary
            let len: u8 = u.int_in_range(120..=140)?;
            let arr: Vec<Value> = (0..len)
                .map(|_| arb_value(u, depth + 1).unwrap_or(Value::Null))
                .collect();
            Ok(Value::Array(arr))
        }
        7 => {
            // Object — sometimes file-reference-shaped
            let is_file_ref: bool = u.arbitrary()?;
            if is_file_ref {
                let filename: String = u.arbitrary()?;
                let size: u64 = u.arbitrary()?;
                let mut map = serde_json::Map::new();
                map.insert("type".into(), Value::String("file".into()));
                map.insert("filename".into(), Value::String(filename));
                map.insert(
                    "size_bytes".into(),
                    Value::Number(serde_json::Number::from(size)),
                );
                Ok(Value::Object(map))
            } else {
                let field_count: u8 = u.int_in_range(0..=5)?;
                let mut map = serde_json::Map::new();
                for _ in 0..field_count {
                    let k: String = u.arbitrary()?;
                    let v = arb_value(u, depth + 1)?;
                    map.insert(k, v);
                }
                Ok(Value::Object(map))
            }
        }
        _ => Ok(Value::Null),
    }
}

/// Generates a random set of variables (key → JSON value).
fn arb_variables(u: &mut Unstructured<'_>) -> arbitrary::Result<HashMap<String, Value>> {
    let count: u8 = u.int_in_range(0..=20)?;
    let mut vars = HashMap::new();
    for _ in 0..count {
        let key: String = u.arbitrary()?;
        let val = arb_value(u, 0)?;
        vars.insert(key, val);
    }
    Ok(vars)
}

/// Generates a random InstanceState variant from fuzzer input.
fn arb_instance_state(u: &mut Unstructured<'_>) -> arbitrary::Result<InstanceState> {
    let variant: u8 = u.int_in_range(0..=10)?;
    match variant {
        0 => Ok(InstanceState::Running),
        1 => Ok(InstanceState::WaitingOnUserTask {
            task_id: Uuid::from_u128(u.arbitrary()?),
        }),
        2 => Ok(InstanceState::WaitingOnServiceTask {
            task_id: Uuid::from_u128(u.arbitrary()?),
        }),
        3 => Ok(InstanceState::WaitingOnTimer {
            timer_id: Uuid::from_u128(u.arbitrary()?),
        }),
        4 => Ok(InstanceState::WaitingOnMessage {
            message_id: Uuid::from_u128(u.arbitrary()?),
        }),
        5 => Ok(InstanceState::WaitingOnEventBasedGateway),
        6 => Ok(InstanceState::ParallelExecution {
            active_token_count: u.int_in_range(0..=100)?,
        }),
        7 => Ok(InstanceState::Completed),
        8 => Ok(InstanceState::CompletedWithError {
            error_code: u.arbitrary()?,
        }),
        9 => {
            // Suspended wraps another state — limit depth to avoid infinite recursion
            let inner = arb_instance_state(u).unwrap_or(InstanceState::Running);
            Ok(InstanceState::Suspended {
                previous_state: Box::new(inner),
            })
        }
        _ => Ok(InstanceState::WaitingOnCallActivity {
            sub_instance_id: Uuid::from_u128(u.arbitrary()?),
            token: Token::new(&u.arbitrary::<String>().unwrap_or_default()),
        }),
    }
}

fuzz_target!(|data: &[u8]| {
    if data.is_empty() || data.len() > 1024 * 64 {
        return;
    }

    let mut u = Unstructured::new(data);

    // Generate two variable sets with some overlap
    let vars_old = match arb_variables(&mut u) {
        Ok(v) => v,
        Err(_) => return,
    };

    // Build second set: start from subset of old, then add/change some keys
    let mut vars_new = HashMap::new();
    let keep_ratio: u8 = u.int_in_range(0..=10).unwrap_or(5);
    for (i, (k, v)) in vars_old.iter().enumerate() {
        if (i as u8 % 10) < keep_ratio {
            // Sometimes keep the value, sometimes mutate it
            let mutate: bool = u.arbitrary().unwrap_or(false);
            if mutate {
                let new_val = arb_value(&mut u, 0).unwrap_or(Value::Null);
                vars_new.insert(k.clone(), new_val);
            } else {
                vars_new.insert(k.clone(), v.clone());
            }
        }
        // else: key is "removed" from new
    }
    // Add some entirely new keys
    let extra: u8 = u.int_in_range(0..=8).unwrap_or(0);
    for _ in 0..extra {
        if let Ok(k) = u.arbitrary::<String>() {
            let v = arb_value(&mut u, 0).unwrap_or(Value::Null);
            vars_new.insert(k, v);
        }
    }

    let old_state = arb_instance_state(&mut u).unwrap_or(InstanceState::Running);
    let new_state = arb_instance_state(&mut u).unwrap_or(InstanceState::Completed);
    let old_node: String = u.arbitrary().unwrap_or_else(|_| "start".into());
    let new_node: String = u.arbitrary().unwrap_or_else(|_| "end".into());

    // Build full ProcessInstances for calculate_diff
    let old_instance = ProcessInstance {
        id: Uuid::nil(),
        definition_key: Uuid::nil(),
        business_key: String::new(),
        parent_instance_id: None,
        state: old_state.clone(),
        current_node: old_node.clone(),
        audit_log: vec![],
        variables: vars_old.clone(),
        tokens: HashMap::new(),
        active_tokens: vec![],
        join_barriers: HashMap::new(),
        multi_instance_state: HashMap::new(),
        compensation_log: vec![],
        started_at: None,
        completed_at: None,
    };

    let mut new_instance = old_instance.clone();
    new_instance.variables = vars_new;
    new_instance.state = new_state;
    new_instance.current_node = new_node.clone();

    // Test 1: calculate_diff (full ProcessInstance → ProcessInstance)
    let _diff = calculate_diff(&old_instance, &new_instance);

    // Test 2: calculate_diff_from_snapshot (lightweight path)
    let snapshot = DiffSnapshot {
        state: old_instance.state.clone(),
        current_node: old_node,
        variables: vars_old,
    };
    let _diff2 = calculate_diff_from_snapshot(&snapshot, &new_instance);
});
