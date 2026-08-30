//! RFC 8785 JSON Canonicalization for release documents.
//!
//! ADR-012 requires the release manifest to have a reproducible SHA-256, which means one
//! serialization has to be authoritative. Object keys are sorted by UTF-16 code unit, there is no
//! insignificant whitespace, and strings use the minimal escaping RFC 8785 prescribes.

use crate::domain::runtime::RuntimeError;
use serde::Serialize;

pub fn to_canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, RuntimeError> {
    let value = serde_json::to_value(value).map_err(|error| {
        RuntimeError::Manifest(format!("document is not serializable: {error}"))
    })?;
    let mut output = String::new();
    write_value(&value, &mut output)?;
    Ok(output.into_bytes())
}

fn write_value(value: &serde_json::Value, output: &mut String) -> Result<(), RuntimeError> {
    match value {
        serde_json::Value::Null => output.push_str("null"),
        serde_json::Value::Bool(true) => output.push_str("true"),
        serde_json::Value::Bool(false) => output.push_str("false"),
        serde_json::Value::Number(number) => {
            // Release documents use only integers. Refusing a float here keeps the canonical form
            // free of the ECMAScript number formatting rules nothing in this schema needs.
            let integer = number
                .as_u64()
                .map(|value| value.to_string())
                .or_else(|| number.as_i64().map(|value| value.to_string()));
            let Some(integer) = integer else {
                return Err(RuntimeError::Manifest(
                    "canonical release documents contain only integer numbers".to_owned(),
                ));
            };
            output.push_str(&integer);
        }
        serde_json::Value::String(string) => write_string(string, output),
        serde_json::Value::Array(items) => {
            output.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_value(item, output)?;
            }
            output.push(']');
        }
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by_key(|key| key.encode_utf16().collect::<Vec<u16>>());
            output.push('{');
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_string(key, output);
                output.push(':');
                write_value(&map[key], output)?;
            }
            output.push('}');
        }
    }
    Ok(())
}

fn write_string(value: &str, output: &mut String) {
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0c}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if (character as u32) < 0x20 => {
                output.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => output.push(character),
        }
    }
    output.push('"');
}

/// Lowercase hexadecimal SHA-256 of arbitrary bytes, for maintainer-facing pin reporting.
pub fn sha256_hex(bytes: &[u8]) -> String {
    crate::adapters::runtime_integrity::sha256_bytes(bytes).to_hex()
}

#[cfg(test)]
mod tests {
    use super::to_canonical_json;

    #[test]
    fn keys_are_sorted_and_whitespace_is_absent() {
        let value = serde_json::json!({ "b": 1, "a": [true, null, "x"], "A": 2 });
        assert_eq!(
            to_canonical_json(&value).unwrap(),
            br#"{"A":2,"a":[true,null,"x"],"b":1}"#.to_vec()
        );
    }

    #[test]
    fn control_characters_and_quotes_are_escaped() {
        let value = serde_json::json!({ "k": "a\"b\\c\nd\u{1}" });
        assert_eq!(
            String::from_utf8(to_canonical_json(&value).unwrap()).unwrap(),
            r#"{"k":"a\"b\\c\nd\u0001"}"#
        );
    }

    #[test]
    fn the_same_document_always_canonicalizes_to_the_same_bytes() {
        let first = serde_json::json!({ "z": 1, "a": { "n": 2, "m": 3 } });
        let second = serde_json::json!({ "a": { "m": 3, "n": 2 }, "z": 1 });
        assert_eq!(
            to_canonical_json(&first).unwrap(),
            to_canonical_json(&second).unwrap()
        );
    }

    #[test]
    fn a_non_integer_number_is_refused_rather_than_silently_reformatted() {
        let value = serde_json::json!({ "k": 1.5 });
        assert!(to_canonical_json(&value).is_err());
    }
}
