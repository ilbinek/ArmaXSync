//! Canonical JSON of a deserialized graph, matching `tools/java-compat`'s
//! `axs.Dump` so Rust and Java views of the same file can be compared.

use axs_javaser::{ArrayData, Content, Graph, Handle, Value};
use serde_json::{json, Map, Value as J};

pub fn canonical_json(g: &Graph) -> J {
    conv_ref(g, g.root())
}

fn conv_ref(g: &Graph, h: Option<Handle>) -> J {
    match h {
        None => J::Null,
        Some(h) => conv(g, h),
    }
}

fn conv_value(g: &Graph, v: &Value) -> J {
    match v {
        Value::Byte(x) => json!(x),
        Value::Short(x) => json!(x),
        Value::Int(x) => json!(x),
        Value::Long(x) => json!(x),
        Value::Boolean(x) => json!(x),
        Value::Char(c) => json!(String::from_utf16_lossy(&[*c])),
        Value::Float(x) => json!(*x as f64),
        Value::Double(x) => json!(x),
        Value::Ref(r) => conv_ref(g, *r),
    }
}

fn conv(g: &Graph, h: Handle) -> J {
    let Ok(c) = g.get(h) else { return J::Null };
    match c {
        Content::String(s) => J::String(s.clone()),
        Content::Enum { constant, .. } => J::String(constant.clone()),
        Content::Array(a) => match &a.elements {
            ArrayData::Bytes(b) => json!({"$bytes": b.len()}),
            ArrayData::Values(v) => J::Array(v.iter().map(|x| conv_value(g, x)).collect()),
        },
        Content::Object(o) => {
            let class = g.class_name(h).unwrap_or("?");
            if let Ok(Some(items)) = g.collection(h) {
                let mut arr: Vec<J> = items.iter().map(|r| conv_ref(g, *r)).collect();
                if class.ends_with("Set") {
                    arr.sort_by_cached_key(|v| v.to_string());
                }
                return J::Array(arr);
            }
            if let Ok(Some(pairs)) = g.map(h) {
                let mut m = Map::new();
                for (k, v) in pairs {
                    let key = match k {
                        Some(k) => match conv(g, k) {
                            J::String(s) => s,
                            other => other.to_string(),
                        },
                        None => "null".into(),
                    };
                    m.insert(key, conv_ref(g, v));
                }
                return J::Object(m);
            }
            if let Ok(Some(ms)) = g.date_millis(h) {
                return json!({"$date": ms});
            }
            if class.starts_with("java.lang.") {
                if let Ok(Some(v)) = g.field(h, "value") {
                    return conv_value(g, v);
                }
            }
            let mut m = Map::new();
            m.insert("$class".into(), J::String(class.to_owned()));
            for cd in &o.class_data {
                for (name, v) in &cd.values {
                    let val = if name == "parent" {
                        match v.as_ref() {
                            Some(p) => g
                                .field(p, "name")
                                .ok()
                                .flatten()
                                .map(|n| conv_value(g, n))
                                .unwrap_or(J::Null),
                            None => J::Null,
                        }
                    } else {
                        conv_value(g, v)
                    };
                    m.entry(name.clone()).or_insert(val);
                }
            }
            J::Object(m)
        }
        _ => J::Null,
    }
}
