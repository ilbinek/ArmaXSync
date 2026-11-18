//! Stream writer driven by static class specifications.
//!
//! The writer mirrors `java.io.ObjectOutputStream` closely enough that streams
//! are accepted by the JDK, and for the class shapes used by Arma3Sync they are
//! byte-identical to what Java produces:
//! * class descriptors are written once and referenced afterwards;
//! * field type strings are shared (Java interns them);
//! * enum constants are written once and referenced afterwards;
//! * block data is segmented in chunks of at most 1024 bytes.

use std::collections::HashMap;

use crate::consts::*;
use crate::graph::Handle;
use crate::mutf8::encode_modified_utf8;

/// Field descriptor; fields must be listed in Java's canonical order
/// (primitives first, then objects, each group sorted by name).
#[derive(Debug, Clone, Copy)]
pub struct FieldSpec {
    pub type_code: u8,
    pub name: &'static str,
    pub class_name: Option<&'static str>,
}

impl FieldSpec {
    pub const fn prim(type_code: u8, name: &'static str) -> Self {
        Self {
            type_code,
            name,
            class_name: None,
        }
    }
    pub const fn obj(name: &'static str, class_name: &'static str) -> Self {
        Self {
            type_code: b'L',
            name,
            class_name: Some(class_name),
        }
    }
}

#[derive(Debug)]
pub struct ClassSpec {
    pub name: &'static str,
    pub uid: i64,
    pub flags: u8,
    pub fields: &'static [FieldSpec],
    pub super_spec: Option<&'static ClassSpec>,
}

#[derive(Debug)]
pub struct EnumSpec {
    pub name: &'static str,
}

/// `java.lang.Enum` descriptor (super of every enum descriptor).
pub static ENUM_BASE: ClassSpec = ClassSpec {
    name: "java.lang.Enum",
    uid: 0,
    flags: SC_SERIALIZABLE | SC_ENUM,
    fields: &[],
    super_spec: None,
};

pub static ARRAY_LIST: ClassSpec = ClassSpec {
    name: "java.util.ArrayList",
    uid: 8683452581122892189,
    flags: SC_SERIALIZABLE | SC_WRITE_METHOD,
    fields: &[FieldSpec::prim(b'I', "size")],
    super_spec: None,
};

pub static HASH_MAP: ClassSpec = ClassSpec {
    name: "java.util.HashMap",
    uid: 362498820763181265,
    flags: SC_SERIALIZABLE | SC_WRITE_METHOD,
    fields: &[
        FieldSpec::prim(b'F', "loadFactor"),
        FieldSpec::prim(b'I', "threshold"),
    ],
    super_spec: None,
};

pub static HASH_SET: ClassSpec = ClassSpec {
    name: "java.util.HashSet",
    uid: -5024744406713321676,
    flags: SC_SERIALIZABLE | SC_WRITE_METHOD,
    fields: &[],
    super_spec: None,
};

pub static DATE: ClassSpec = ClassSpec {
    name: "java.util.Date",
    uid: 7523967970034938905,
    flags: SC_SERIALIZABLE | SC_WRITE_METHOD,
    fields: &[],
    super_spec: None,
};

pub static BOOLEAN: ClassSpec = ClassSpec {
    name: "java.lang.Boolean",
    uid: -3665804199014368530,
    flags: SC_SERIALIZABLE,
    fields: &[FieldSpec::prim(b'Z', "value")],
    super_spec: None,
};

pub struct Writer {
    buf: Vec<u8>,
    block: Vec<u8>,
    next: u32,
    descs: HashMap<&'static str, Handle>,
    type_strings: HashMap<&'static str, Handle>,
    enums: HashMap<(&'static str, String), Handle>,
    booleans: [Option<Handle>; 2],
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}

impl Writer {
    pub fn new() -> Self {
        let mut buf = Vec::with_capacity(4096);
        buf.extend_from_slice(&STREAM_MAGIC.to_be_bytes());
        buf.extend_from_slice(&STREAM_VERSION.to_be_bytes());
        Self {
            buf,
            block: Vec::new(),
            next: BASE_WIRE_HANDLE,
            descs: HashMap::new(),
            type_strings: HashMap::new(),
            enums: HashMap::new(),
            booleans: [None, None],
        }
    }

    /// Finish the stream and return its bytes.
    pub fn finish(mut self) -> Vec<u8> {
        self.flush_block();
        self.buf
    }

    fn assign(&mut self) -> Handle {
        let h = self.next;
        self.next += 1;
        h
    }

    fn flush_block(&mut self) {
        if self.block.is_empty() {
            return;
        }
        let block = std::mem::take(&mut self.block);
        for chunk in block.chunks(1024) {
            if chunk.len() <= 0xFF {
                self.buf.push(TC_BLOCKDATA);
                self.buf.push(chunk.len() as u8);
            } else {
                self.buf.push(TC_BLOCKDATALONG);
                self.buf
                    .extend_from_slice(&(chunk.len() as i32).to_be_bytes());
            }
            self.buf.extend_from_slice(chunk);
        }
    }

    fn raw(&mut self, bytes: &[u8]) {
        self.flush_block();
        self.buf.extend_from_slice(bytes);
    }

    fn raw_utf(&mut self, s: &str) {
        let enc = encode_modified_utf8(s);
        assert!(enc.len() <= 0xFFFF, "class/field name too long");
        self.raw(&(enc.len() as u16).to_be_bytes());
        self.raw(&enc);
    }

    // ----- references and simple contents -------------------------------

    pub fn null(&mut self) {
        self.raw(&[TC_NULL]);
    }

    pub fn reference(&mut self, h: Handle) {
        self.raw(&[TC_REFERENCE]);
        self.raw(&h.to_be_bytes());
    }

    /// Write a new string object and return its handle.
    pub fn string(&mut self, s: &str) -> Handle {
        let enc = encode_modified_utf8(s);
        if enc.len() <= 0xFFFF {
            self.raw(&[TC_STRING]);
            self.raw(&(enc.len() as u16).to_be_bytes());
        } else {
            self.raw(&[TC_LONGSTRING]);
            self.raw(&(enc.len() as i64).to_be_bytes());
        }
        self.raw(&enc);
        self.assign()
    }

    /// Write a nullable string.
    pub fn opt_string(&mut self, s: Option<&str>) {
        match s {
            Some(s) => {
                self.string(s);
            }
            None => self.null(),
        }
    }

    fn type_string(&mut self, s: &'static str) {
        if let Some(&h) = self.type_strings.get(s) {
            self.reference(h);
        } else {
            let h = self.string(s);
            self.type_strings.insert(s, h);
        }
    }

    /// Write (or reference) a class descriptor chain.
    pub fn class_desc(&mut self, spec: &'static ClassSpec) {
        if let Some(&h) = self.descs.get(spec.name) {
            self.reference(h);
            return;
        }
        self.raw(&[TC_CLASSDESC]);
        self.raw_utf(spec.name);
        self.raw(&spec.uid.to_be_bytes());
        let h = self.assign();
        self.descs.insert(spec.name, h);
        self.raw(&[spec.flags]);
        self.raw(&(spec.fields.len() as u16).to_be_bytes());
        for f in spec.fields {
            self.raw(&[f.type_code]);
            self.raw_utf(f.name);
            if let Some(cn) = f.class_name {
                self.type_string(cn);
            }
        }
        self.raw(&[TC_ENDBLOCKDATA]);
        match spec.super_spec {
            Some(s) => self.class_desc(s),
            None => self.null(),
        }
    }

    /// Write an enum constant (referenced if already written).
    pub fn enum_const(&mut self, spec: &'static EnumSpec, constant: &str) -> Handle {
        if let Some(&h) = self.enums.get(&(spec.name, constant.to_owned())) {
            self.reference(h);
            return h;
        }
        self.raw(&[TC_ENUM]);
        if let Some(&h) = self.descs.get(spec.name) {
            self.reference(h);
        } else {
            self.raw(&[TC_CLASSDESC]);
            self.raw_utf(spec.name);
            self.raw(&0i64.to_be_bytes());
            let h = self.assign();
            self.descs.insert(spec.name, h);
            self.raw(&[SC_SERIALIZABLE | SC_ENUM]);
            self.raw(&0u16.to_be_bytes());
            self.raw(&[TC_ENDBLOCKDATA]);
            self.class_desc(&ENUM_BASE);
        }
        let h = self.assign();
        self.string(constant);
        self.enums.insert((spec.name, constant.to_owned()), h);
        h
    }

    /// Begin an object: writes TC_OBJECT and its class descriptor and returns
    /// the object's handle. The caller then writes class data for each class in
    /// the hierarchy (super-most first) in descriptor field order.
    pub fn begin_object(&mut self, spec: &'static ClassSpec) -> Handle {
        self.raw(&[TC_OBJECT]);
        self.class_desc(spec);
        self.assign()
    }

    // ----- field values (written raw) -----------------------------------

    pub fn f_bool(&mut self, v: bool) {
        self.raw(&[v as u8]);
    }
    pub fn f_i32(&mut self, v: i32) {
        self.raw(&v.to_be_bytes());
    }
    pub fn f_i64(&mut self, v: i64) {
        self.raw(&v.to_be_bytes());
    }
    pub fn f_f32(&mut self, v: f32) {
        self.raw(&v.to_bits().to_be_bytes());
    }
    pub fn f_f64(&mut self, v: f64) {
        self.raw(&v.to_bits().to_be_bytes());
    }

    // ----- block data (inside writeObject) -------------------------------

    pub fn b_i32(&mut self, v: i32) {
        self.block.extend_from_slice(&v.to_be_bytes());
    }
    pub fn b_i64(&mut self, v: i64) {
        self.block.extend_from_slice(&v.to_be_bytes());
    }
    pub fn b_f32(&mut self, v: f32) {
        self.block.extend_from_slice(&v.to_bits().to_be_bytes());
    }

    /// End the custom data of a `writeObject` class.
    pub fn end_block(&mut self) {
        self.raw(&[TC_ENDBLOCKDATA]);
    }

    // ----- JDK helpers ---------------------------------------------------

    /// Begin a `java.util.ArrayList` with `len` elements. The caller writes
    /// exactly `len` objects and then calls [`Writer::end_block`].
    pub fn begin_array_list(&mut self, len: usize) -> Handle {
        let h = self.begin_object(&ARRAY_LIST);
        self.f_i32(len as i32);
        self.b_i32(len as i32);
        h
    }

    /// Write a complete `ArrayList<String>`.
    pub fn string_list<S: AsRef<str>>(&mut self, items: &[S]) -> Handle {
        let h = self.begin_array_list(items.len());
        for s in items {
            self.string(s.as_ref());
        }
        self.end_block();
        h
    }

    /// Begin a `java.util.HashMap` with `len` entries. The caller writes
    /// `len` key/value object pairs and then calls [`Writer::end_block`].
    pub fn begin_hash_map(&mut self, len: usize) -> Handle {
        let h = self.begin_object(&HASH_MAP);
        let (buckets, threshold) = hash_capacity(len);
        self.f_f32(0.75);
        self.f_i32(threshold);
        self.b_i32(buckets);
        self.b_i32(len as i32);
        h
    }

    /// Begin a `java.util.HashSet` with `len` elements. The caller writes
    /// `len` objects and then calls [`Writer::end_block`].
    pub fn begin_hash_set(&mut self, len: usize) -> Handle {
        let h = self.begin_object(&HASH_SET);
        let (buckets, _) = hash_capacity(len);
        self.b_i32(buckets);
        self.b_f32(0.75);
        self.b_i32(len as i32);
        h
    }

    /// Write a `java.util.Date`.
    pub fn date(&mut self, millis: i64) -> Handle {
        let h = self.begin_object(&DATE);
        self.b_i64(millis);
        self.end_block();
        h
    }

    /// Write a boxed `java.lang.Boolean` (shared like `Boolean.TRUE/FALSE`).
    pub fn boxed_bool(&mut self, v: bool) -> Handle {
        if let Some(h) = self.booleans[v as usize] {
            self.reference(h);
            return h;
        }
        let h = self.begin_object(&BOOLEAN);
        self.f_bool(v);
        self.booleans[v as usize] = Some(h);
        h
    }
}

/// Table capacity (power of two, minimum 16) and resize threshold for a Java
/// 8 `HashMap` holding `len` entries at load factor 0.75.
fn hash_capacity(len: usize) -> (i32, i32) {
    let mut cap: i64 = 16;
    while (cap as f64) * 0.75 < len as f64 {
        cap *= 2;
    }
    let cap = cap.min(1 << 30) as i32;
    (cap, (cap as f32 * 0.75) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse, Content};

    static POINT: ClassSpec = ClassSpec {
        name: "test.Point",
        uid: 42,
        flags: SC_SERIALIZABLE,
        fields: &[
            FieldSpec::prim(b'I', "x"),
            FieldSpec::obj("label", "Ljava/lang/String;"),
            FieldSpec::obj("next", "Ltest/Point;"),
        ],
        super_spec: None,
    };

    #[test]
    fn roundtrip_cycle_and_collections() {
        let mut w = Writer::new();
        let a = w.begin_object(&POINT);
        w.f_i32(7);
        w.string("ä€🙂");
        // next -> self (cycle)
        w.reference(a);
        let bytes = w.finish();
        let g = parse(&bytes).unwrap();
        let root = g.root().unwrap();
        assert_eq!(g.class_name(root).unwrap(), "test.Point");
        assert_eq!(g.field(root, "x").unwrap().unwrap().as_i32(), Some(7));
        let label = g.field(root, "label").unwrap().unwrap().as_ref().unwrap();
        assert_eq!(g.string(label).unwrap(), "ä€🙂");
        assert_eq!(g.field(root, "next").unwrap().unwrap().as_ref(), Some(root));
    }

    #[test]
    fn list_map_set_date() {
        let mut w = Writer::new();
        let _ = w.begin_array_list(4);
        w.string_list(&["a", "b"]);
        let _ = w.begin_hash_map(2);
        w.string("k1");
        w.boxed_bool(true);
        w.string("k2");
        w.boxed_bool(true);
        w.end_block();
        let _ = w.begin_hash_set(1);
        w.string("s");
        w.end_block();
        w.date(1_600_000_000_123);
        w.end_block();
        let g = parse(&w.finish()).unwrap();
        let root = g.root().unwrap();
        let items = g.collection(root).unwrap().unwrap();
        assert_eq!(items.len(), 4);
        let inner = g.collection(items[0].unwrap()).unwrap().unwrap();
        assert_eq!(g.string(inner[1].unwrap()).unwrap(), "b");
        let map = g.map(items[1].unwrap()).unwrap().unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(map[0].1, map[1].1, "Boolean.TRUE shared");
        let v = g.unbox(map[0].1.unwrap()).unwrap().unwrap();
        assert_eq!(v.as_bool(), Some(true));
        let set = g.collection(items[2].unwrap()).unwrap().unwrap();
        assert_eq!(g.string(set[0].unwrap()).unwrap(), "s");
        assert_eq!(
            g.date_millis(items[3].unwrap()).unwrap(),
            Some(1_600_000_000_123)
        );
        assert!(matches!(
            g.get(items[0].unwrap()).unwrap(),
            Content::Object(_)
        ));
    }

    static COLOR: EnumSpec = EnumSpec { name: "test.Color" };

    #[test]
    fn enums_are_shared() {
        let mut w = Writer::new();
        let _ = w.begin_array_list(3);
        let r = w.enum_const(&COLOR, "RED");
        let r2 = w.enum_const(&COLOR, "RED");
        w.enum_const(&COLOR, "BLUE");
        w.end_block();
        assert_eq!(r, r2);
        let g = parse(&w.finish()).unwrap();
        let items = g.collection(g.root().unwrap()).unwrap().unwrap();
        assert_eq!(g.enum_constant(items[0].unwrap()).unwrap(), "RED");
        assert_eq!(items[0], items[1]);
        assert_eq!(g.enum_constant(items[2].unwrap()).unwrap(), "BLUE");
        assert_eq!(g.class_name(items[2].unwrap()).unwrap(), "test.Color");
    }

    #[test]
    fn large_block_segments() {
        let mut w = Writer::new();
        let _ = w.begin_object(&DATE);
        for i in 0..400 {
            w.b_i32(i);
        }
        w.end_block();
        let g = parse(&w.finish()).unwrap();
        let cd = g
            .class_data(g.root().unwrap(), "java.util.Date")
            .unwrap()
            .unwrap();
        match &cd.annotations[0] {
            crate::Annotation::Block(b) => assert_eq!(b.len(), 1600),
            _ => panic!(),
        }
    }
}
