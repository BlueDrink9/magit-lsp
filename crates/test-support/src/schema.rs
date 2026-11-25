//! Narrow JSON Schema validation shared by the schema and example suites.
//!
//! The workspace has no JSON Schema validator dependency, so this deliberately
//! covers only the keywords the generated artifacts emit: `$ref`, `anyOf`,
//! `oneOf`, `type`, `const`, `enum`, `required`, `properties`, `items`,
//! `minimum` and `additionalProperties`. Unknown keywords are ignored, so
//! tolerated extra properties are not rejected.

use serde_json::Value;
use std::path::Path;

/// Read one committed artifact from `docs/schema/`.
pub fn read_schema(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/schema")
        .join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

/// Whether a JSON value satisfies one JSON Schema `type` name.
pub fn matches_type(kind: &str, value: &Value) -> bool {
    match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => value.as_f64().is_some_and(|number| number.fract() == 0.0),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => true,
    }
}

/// Resolve a local `#/...` JSON Pointer reference.
pub fn dereference<'a>(root: &'a Value, reference: &str) -> Option<&'a Value> {
    root.pointer(reference.strip_prefix('#')?)
}

/// Return every narrow-schema violation of `value` against `schema`.
pub fn schema_errors(root: &Value, schema: &Value, value: &Value, path: &str) -> Vec<String> {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        return match dereference(root, reference) {
            Some(target) => schema_errors(root, target, value, path),
            None => vec![format!("{path}: unresolved reference {reference}")],
        };
    }

    for keyword in ["anyOf", "oneOf"] {
        let Some(branches) = schema.get(keyword).and_then(Value::as_array) else {
            continue;
        };
        let matches = branches
            .iter()
            .filter(|branch| schema_errors(root, branch, value, path).is_empty())
            .count();
        let satisfied = if keyword == "oneOf" {
            matches == 1
        } else {
            matches >= 1
        };
        if !satisfied {
            return vec![format!(
                "{path}: matches {matches} {keyword} branches: {value}"
            )];
        }
        return Vec::new();
    }

    let mut errors = Vec::new();

    if let Some(kinds) = schema.get("type") {
        let satisfied = match kinds {
            Value::String(kind) => matches_type(kind, value),
            Value::Array(kinds) => kinds
                .iter()
                .filter_map(Value::as_str)
                .any(|kind| matches_type(kind, value)),
            _ => true,
        };
        if !satisfied {
            errors.push(format!("{path}: expected type {kinds}, found {value}"));
            return errors;
        }
    }

    if let Some(minimum) = schema.get("minimum").and_then(Value::as_f64) {
        if let Some(number) = value.as_f64() {
            if number < minimum {
                errors.push(format!(
                    "{path}: {value} is less than the minimum {minimum}"
                ));
            }
        }
    }

    if let Some(expected) = schema.get("const") {
        if value != expected {
            errors.push(format!("{path}: expected const {expected}, found {value}"));
        }
    }

    if let Some(allowed) = schema.get("enum").and_then(Value::as_array) {
        if !allowed.contains(value) {
            errors.push(format!("{path}: {value} is not one of {allowed:?}"));
        }
    }

    if let Some(object) = value.as_object() {
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for name in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(name) {
                    errors.push(format!("{path}: missing required property {name}"));
                }
            }
        }
        if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
            for (key, field) in object {
                match properties.get(key) {
                    Some(field_schema) => errors.extend(schema_errors(
                        root,
                        field_schema,
                        field,
                        &format!("{path}.{key}"),
                    )),
                    None => {
                        if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
                            errors.push(format!("{path}: unexpected property {key}"));
                        }
                    }
                }
            }
        }
    }

    if let (Some(items), Some(array)) = (schema.get("items"), value.as_array()) {
        for (index, item) in array.iter().enumerate() {
            errors.extend(schema_errors(
                root,
                items,
                item,
                &format!("{path}[{index}]"),
            ));
        }
    }

    errors
}
