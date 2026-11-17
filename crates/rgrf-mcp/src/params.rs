//! Tool parameters: one table per tool drives the JSON schema *and* the validation of the call,
//! so the two cannot drift apart.

use serde_json::{Map, Value, json};

#[derive(Debug, Clone, Copy)]
pub enum Ty {
    Str,
    Int,
    Bool,
    StrList,
    Enum(&'static [&'static str]),
}

#[derive(Debug, Clone, Copy)]
pub struct Param {
    pub name: &'static str,
    pub ty: Ty,
    pub required: bool,
    pub desc: &'static str,
}

pub const fn req(name: &'static str, ty: Ty, desc: &'static str) -> Param {
    Param {
        name,
        ty,
        required: true,
        desc,
    }
}

pub const fn opt(name: &'static str, ty: Ty, desc: &'static str) -> Param {
    Param {
        name,
        ty,
        required: false,
        desc,
    }
}

pub fn schema(params: &[Param]) -> Value {
    let mut props = Map::new();
    let mut required = Vec::new();
    for p in params {
        let mut s = match p.ty {
            Ty::Str => json!({"type": "string"}),
            Ty::Int => json!({"type": "integer", "minimum": 0}),
            Ty::Bool => json!({"type": "boolean"}),
            Ty::StrList => json!({"type": "array", "items": {"type": "string"}, "minItems": 1}),
            Ty::Enum(values) => json!({"type": "string", "enum": values}),
        };
        s["description"] = p.desc.into();
        props.insert(p.name.into(), s);
        if p.required {
            required.push(p.name);
        }
    }
    json!({
        "type": "object",
        "properties": props,
        "required": required,
        "additionalProperties": false,
    })
}

/// Arguments of a call after validation against the parameter table.
pub struct Args(Map<String, Value>);

impl Args {
    pub fn validate(params: &[Param], given: Option<&Value>) -> Result<Self, String> {
        let empty = Map::new();
        let map = match given {
            None | Some(Value::Null) => &empty,
            Some(Value::Object(m)) => m,
            Some(_) => return Err("arguments must be an object".into()),
        };
        for k in map.keys() {
            if !params.iter().any(|p| p.name == k) {
                return Err(format!("unknown argument '{k}'"));
            }
        }
        for p in params {
            let Some(v) = map.get(p.name) else {
                if p.required {
                    return Err(format!("missing argument '{}'", p.name));
                }
                continue;
            };
            let ok = match p.ty {
                Ty::Str => v.is_string(),
                Ty::Int => v.is_u64(),
                Ty::Bool => v.is_boolean(),
                Ty::StrList => v
                    .as_array()
                    .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string)),
                Ty::Enum(values) => v.as_str().is_some_and(|s| values.contains(&s)),
            };
            if !ok {
                return Err(match p.ty {
                    Ty::Enum(values) => {
                        format!("argument '{}' must be one of {}", p.name, values.join(", "))
                    }
                    Ty::Int => format!("argument '{}' must be a non-negative integer", p.name),
                    Ty::Bool => format!("argument '{}' must be true or false", p.name),
                    Ty::StrList => {
                        format!("argument '{}' must be a non-empty list of strings", p.name)
                    }
                    Ty::Str => format!("argument '{}' must be a string", p.name),
                });
            }
        }
        Ok(Self(map.clone()))
    }

    pub fn str(&self, name: &str) -> &str {
        self.opt_str(name).unwrap_or_default()
    }

    pub fn opt_str(&self, name: &str) -> Option<&str> {
        self.0.get(name).and_then(Value::as_str)
    }

    pub fn opt_u64(&self, name: &str) -> Option<u64> {
        self.0.get(name).and_then(Value::as_u64)
    }

    pub fn opt_bool(&self, name: &str) -> Option<bool> {
        self.0.get(name).and_then(Value::as_bool)
    }

    pub fn flag(&self, name: &str) -> bool {
        self.opt_bool(name).unwrap_or(false)
    }

    pub fn list(&self, name: &str) -> Vec<&str> {
        self.0
            .get(name)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: &[Param] = &[
        req("path", Ty::Str, "p"),
        opt("limit", Ty::Int, "l"),
        opt("mode", Ty::Enum(&["a", "b"]), "m"),
        opt("all", Ty::StrList, "x"),
    ];

    #[test]
    fn schema_lists_properties_and_required() {
        let s = schema(P);
        assert_eq!(s["required"], json!(["path"]));
        assert_eq!(s["properties"]["mode"]["enum"], json!(["a", "b"]));
        assert_eq!(s["additionalProperties"], false);
    }

    #[test]
    fn validation_rejects_unknown_missing_and_mistyped_arguments() {
        let bad = |v: Value| Args::validate(P, Some(&v)).err().unwrap();
        assert!(bad(json!({})).contains("missing"));
        assert!(bad(json!({"path": "x", "zzz": 1})).contains("unknown"));
        assert!(bad(json!({"path": 5})).contains("string"));
        assert!(bad(json!({"path": "x", "limit": -1})).contains("non-negative"));
        assert!(bad(json!({"path": "x", "limit": 1.5})).contains("non-negative"));
        assert!(bad(json!({"path": "x", "mode": "c"})).contains("one of a, b"));
        assert!(bad(json!({"path": "x", "all": []})).contains("non-empty"));
        let ok = Args::validate(P, Some(&json!({"path": "x", "limit": 3, "all": ["q"]}))).unwrap();
        assert_eq!((ok.str("path"), ok.opt_u64("limit")), ("x", Some(3)));
        assert_eq!(ok.list("all"), ["q"]);
    }
}
