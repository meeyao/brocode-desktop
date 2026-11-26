//! `plugins/plugin.schema.json`: JSON Schema (draft-07) for plugin.toml, for TOML editors.

use serde_json::{Map, Value, json};

use super::model::{Contract, ManifestField};

pub fn manifest(c: &Contract) -> Result<String, String> {
    let mut schema = object(c, &c.manifest.fields)?;
    let head = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": format!("Brocode plugin manifest (contract {})", c.contract),
        "description": format!("{} Generated from crates/opencode-plugin-spec/contract.json.", c.manifest.doc),
    });
    if let (Value::Object(mut all), Value::Object(body)) = (head, schema.take()) {
        all.extend(body);
        let text = serde_json::to_string_pretty(&Value::Object(all)).map_err(|e| e.to_string())?;
        return Ok(format!("{text}\n"));
    }
    Err("manifest schema is not an object".to_owned())
}

fn object(c: &Contract, fields: &[ManifestField]) -> Result<Value, String> {
    let mut properties = Map::new();
    for f in fields {
        properties.insert(f.name.clone(), property(c, f)?);
    }
    let required: Vec<&str> = fields.iter().filter(|f| f.required).map(|f| f.name.as_str()).collect();
    Ok(json!({
        "type": "object",
        "required": required,
        "additionalProperties": false,
        "properties": properties,
    }))
}

fn property(c: &Contract, f: &ManifestField) -> Result<Value, String> {
    let enum_values = |name: &Option<String>| -> Result<Vec<String>, String> {
        let name = name.as_deref().ok_or_else(|| format!("manifest field `{}` needs `enum`", f.name))?;
        let e = c.enum_def(name).ok_or_else(|| format!("manifest field `{}` names a missing enum `{name}`", f.name))?;
        Ok(e.values.iter().map(|v| v.value.clone()).collect())
    };
    let mut p = match f.ty.as_str() {
        "string" => json!({ "type": "string" }),
        "string[]" => json!({ "type": "array", "items": { "type": "string" } }),
        "enum" => json!({ "type": "string", "enum": enum_values(&f.enum_name)? }),
        "enum[]" => json!({ "type": "array", "uniqueItems": true, "items": { "type": "string", "enum": enum_values(&f.enum_name)? } }),
        "table[]" => json!({ "type": "array", "items": object(c, &f.fields)? }),
        other => return Err(format!("manifest field `{}` has an unknown type `{other}`", f.name)),
    };
    if let Value::Object(map) = &mut p {
        map.insert("description".to_owned(), Value::String(f.doc.replace('`', "")));
        if let Some(pattern) = &f.pattern {
            map.insert("pattern".to_owned(), Value::String(pattern.clone()));
        }
        if let Some(default) = &f.default {
            map.insert("default".to_owned(), Value::String(default.clone()));
        }
        if let Some(format) = &f.format {
            map.insert("format".to_owned(), Value::String(format.clone()));
        }
    }
    Ok(p)
}
