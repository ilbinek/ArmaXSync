//! Debug dump of a graph as JSON (used by `armaxsync inspect`).

use std::collections::HashSet;

use serde_json::{json, Map, Value as J};

use crate::graph::*;

/// Convert the graph root to JSON. Back-references to objects already being
/// printed are rendered as `{"$ref": "0x7e00NN"}`.
pub fn to_json(g: &Graph) -> J {
    let mut seen = HashSet::new();
    match g.roots.as_slice() {
        [one] => conv_ref(g, *one, &mut seen),
        many => J::Array(many.iter().map(|r| conv_ref(g, *r, &mut seen)).collect()),
    }
}

fn conv_ref(g: &Graph, h: Option<Handle>, seen: &mut HashSet<Handle>) -> J {
    match h {
        None => J::Null,
        Some(h) => conv(g, h, seen),
    }
}

fn conv_value(g: &Graph, v: &Value, seen: &mut HashSet<Handle>) -> J {
    match v {
        Value::Byte(x) => json!(x),
        Value::Char(x) => json!(char::from_u32(*x as u32)
            .map(String::from)
            .unwrap_or_default()),
        Value::Double(x) => json!(x),
        Value::Float(x) => json!(x),
        Value::Int(x) => json!(x),
        Value::Long(x) => json!(x),
        Value::Short(x) => json!(x),
        Value::Boolean(x) => json!(x),
        Value::Ref(r) => conv_ref(g, *r, seen),
    }
}

fn conv(g: &Graph, h: Handle, seen: &mut HashSet<Handle>) -> J {
    let Ok(c) = g.get(h) else {
        return json!({"$bad_handle": h});
    };
    match c {
        Content::String(s) => J::String(s.clone()),
        Content::Enum { constant, .. } => {
            json!({"$enum": g.class_name(h).unwrap_or("?"), "value": constant})
        }
        Content::Class { .. } => json!({"$class": g.class_name(h).unwrap_or("?")}),
        Content::ClassDesc(d) => json!({"$classdesc": d.name}),
        Content::Pending => J::Null,
        Content::Array(a) => match &a.elements {
            ArrayData::Bytes(b) => json!({"$bytes": b.len()}),
            ArrayData::Values(v) => J::Array(v.iter().map(|x| conv_value(g, x, seen)).collect()),
        },
        Content::Object(o) => {
            if !seen.insert(h) {
                return json!({"$ref": format!("{h:#x}")});
            }
            let class = g.class_name(h).unwrap_or("?").to_owned();
            let out = if let Ok(Some(items)) = g.collection(h) {
                J::Array(items.iter().map(|r| conv_ref(g, *r, seen)).collect())
            } else if let Ok(Some(pairs)) = g.map(h) {
                let mut m = Map::new();
                for (k, v) in pairs {
                    let key = match k.and_then(|k| g.string(k).ok()) {
                        Some(s) => s.to_owned(),
                        None => format!("{}", conv_ref(g, k, seen)),
                    };
                    m.insert(key, conv_ref(g, v, seen));
                }
                J::Object(m)
            } else if let Ok(Some(ms)) = g.date_millis(h) {
                json!({"$date": ms})
            } else if class.starts_with("java.lang.")
                && o.class_data
                    .iter()
                    .any(|cd| cd.values.iter().any(|(n, _)| n == "value"))
            {
                g.field(h, "value")
                    .ok()
                    .flatten()
                    .map(|v| conv_value(g, v, seen))
                    .unwrap_or(J::Null)
            } else {
                let mut m = Map::new();
                m.insert("$class".into(), J::String(class));
                for cd in &o.class_data {
                    for (name, v) in &cd.values {
                        m.insert(name.clone(), conv_value(g, v, seen));
                    }
                    let ann: Vec<J> = cd
                        .annotations
                        .iter()
                        .map(|a| match a {
                            Annotation::Block(b) => json!({"$block": b.len()}),
                            Annotation::Content(r) => conv_ref(g, *r, seen),
                        })
                        .collect();
                    if !ann.is_empty() {
                        let key = format!(
                            "$annotations:{}",
                            g.class_desc(cd.desc)
                                .map(|d| d.name.as_str())
                                .unwrap_or("?")
                        );
                        m.insert(key, J::Array(ann));
                    }
                }
                J::Object(m)
            };
            seen.remove(&h);
            out
        }
    }
}
