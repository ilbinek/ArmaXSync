//! Arena representation of a parsed serialization stream.

use crate::{Error, Result};

/// Wire handle (starts at `0x7E0000`, assigned in stream order).
pub type Handle = u32;

/// A field or array element value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Byte(i8),
    Char(u16),
    Double(f64),
    Float(f32),
    Int(i32),
    Long(i64),
    Short(i16),
    Boolean(bool),
    /// Object reference; `None` is Java `null`.
    Ref(Option<Handle>),
}

impl Value {
    pub fn as_bool(&self) -> Option<bool> {
        match *self {
            Value::Boolean(b) => Some(b),
            _ => None,
        }
    }
    pub fn as_i32(&self) -> Option<i32> {
        match *self {
            Value::Int(v) => Some(v),
            Value::Short(v) => Some(v as i32),
            Value::Byte(v) => Some(v as i32),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match *self {
            Value::Long(v) => Some(v),
            Value::Int(v) => Some(v as i64),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            Value::Double(v) => Some(v),
            Value::Float(v) => Some(v as f64),
            _ => None,
        }
    }
    pub fn as_ref(&self) -> Option<Handle> {
        match *self {
            Value::Ref(h) => h,
            _ => None,
        }
    }
}

/// Field descriptor inside a class descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDesc {
    /// One of `B C D F I J S Z L [`.
    pub type_code: u8,
    pub name: String,
    /// JVM type signature for object/array fields, e.g. `Ljava/util/List;`.
    pub class_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassDesc {
    pub name: String,
    pub serial_version_uid: i64,
    pub flags: u8,
    pub fields: Vec<FieldDesc>,
    pub annotations: Vec<Annotation>,
    pub super_desc: Option<Handle>,
}

/// Content written by a class's `writeObject` (or class annotations).
#[derive(Debug, Clone, PartialEq)]
pub enum Annotation {
    Block(Vec<u8>),
    Content(Option<Handle>),
}

/// Data belonging to one class in an object's serializable hierarchy.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassData {
    pub desc: Handle,
    pub values: Vec<(String, Value)>,
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    pub desc: Handle,
    /// Ordered from the top-most serializable superclass to the concrete class.
    pub class_data: Vec<ClassData>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Array {
    pub desc: Handle,
    pub elements: ArrayData,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArrayData {
    Bytes(Vec<u8>),
    Values(Vec<Value>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    /// Placeholder for a handle slot that has been allocated but not filled yet.
    Pending,
    String(String),
    ClassDesc(ClassDesc),
    Object(Object),
    Array(Array),
    Enum {
        desc: Handle,
        constant: String,
    },
    Class {
        desc: Handle,
    },
}

/// A fully parsed stream.
#[derive(Debug, Clone, Default)]
pub struct Graph {
    pub(crate) items: Vec<Content>,
    /// Top-level contents in stream order (usually one).
    pub roots: Vec<Option<Handle>>,
}

impl Graph {
    pub fn root(&self) -> Option<Handle> {
        self.roots.first().copied().flatten()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn get(&self, h: Handle) -> Result<&Content> {
        h.checked_sub(crate::consts::BASE_WIRE_HANDLE)
            .and_then(|i| self.items.get(i as usize))
            .ok_or(Error::BadHandle(h))
    }

    pub fn string(&self, h: Handle) -> Result<&str> {
        match self.get(h)? {
            Content::String(s) => Ok(s),
            _ => Err(Error::WrongKind(h, "string")),
        }
    }

    pub fn class_desc(&self, h: Handle) -> Result<&ClassDesc> {
        match self.get(h)? {
            Content::ClassDesc(d) => Ok(d),
            _ => Err(Error::WrongKind(h, "class descriptor")),
        }
    }

    pub fn object(&self, h: Handle) -> Result<&Object> {
        match self.get(h)? {
            Content::Object(o) => Ok(o),
            _ => Err(Error::WrongKind(h, "object")),
        }
    }

    /// Class name of an object, enum or array.
    pub fn class_name(&self, h: Handle) -> Result<&str> {
        let desc = match self.get(h)? {
            Content::Object(o) => o.desc,
            Content::Array(a) => a.desc,
            Content::Enum { desc, .. } => *desc,
            Content::Class { desc } => *desc,
            Content::ClassDesc(d) => return Ok(&d.name),
            Content::String(_) => return Ok("java.lang.String"),
            Content::Pending => return Err(Error::WrongKind(h, "complete content")),
        };
        Ok(&self.class_desc(desc)?.name)
    }

    /// Enum constant name.
    pub fn enum_constant(&self, h: Handle) -> Result<&str> {
        match self.get(h)? {
            Content::Enum { constant, .. } => Ok(constant),
            _ => Err(Error::WrongKind(h, "enum")),
        }
    }

    /// Look up a field by name, searching from the most derived class upwards.
    pub fn field(&self, obj: Handle, name: &str) -> Result<Option<&Value>> {
        let o = self.object(obj)?;
        for cd in o.class_data.iter().rev() {
            if let Some((_, v)) = cd.values.iter().find(|(n, _)| n == name) {
                return Ok(Some(v));
            }
        }
        Ok(None)
    }

    /// Class data for a specific class name in an object's hierarchy.
    pub fn class_data(&self, obj: Handle, class: &str) -> Result<Option<&ClassData>> {
        let o = self.object(obj)?;
        for cd in &o.class_data {
            if self.class_desc(cd.desc)?.name == class {
                return Ok(Some(cd));
            }
        }
        Ok(None)
    }

    /// Resolve a `java.lang.Boolean`/`Integer`/`Long` box to its primitive.
    pub fn unbox(&self, h: Handle) -> Result<Option<Value>> {
        Ok(self.field(h, "value")?.cloned())
    }

    /// Elements of common `java.util` collections (`ArrayList`, `LinkedList`,
    /// `HashSet`, `LinkedHashSet`, `TreeSet`, `Vector`, `ArrayDeque`, unmodifiable
    /// and synchronized wrappers). Returns `None` for unrecognized classes.
    pub fn collection(&self, h: Handle) -> Result<Option<Vec<Option<Handle>>>> {
        let name = self.class_name(h)?.to_owned();
        match name.as_str() {
            "java.util.ArrayList" | "java.util.LinkedList" | "java.util.ArrayDeque" => {
                Ok(Some(self.annotation_refs(h, &name, 0)?))
            }
            "java.util.HashSet" | "java.util.LinkedHashSet" => {
                // LinkedHashSet's data lives in HashSet's writeObject.
                Ok(Some(self.annotation_refs(h, "java.util.HashSet", 0)?))
            }
            "java.util.TreeSet" => Ok(Some(self.annotation_refs(h, &name, 1)?)),
            "java.util.Vector" => {
                let data = self.field(h, "elementData")?.and_then(Value::as_ref);
                let count = self
                    .field(h, "elementCount")?
                    .and_then(Value::as_i32)
                    .unwrap_or(0);
                match data {
                    Some(arr) => match self.get(arr)? {
                        Content::Array(Array {
                            elements: ArrayData::Values(v),
                            ..
                        }) => Ok(Some(
                            v.iter()
                                .take(count.max(0) as usize)
                                .map(|v| v.as_ref())
                                .collect(),
                        )),
                        _ => Ok(Some(vec![])),
                    },
                    None => Ok(Some(vec![])),
                }
            }
            n if n.starts_with("java.util.Collections$") => {
                match self
                    .field(h, "c")?
                    .or(self.field(h, "list")?)
                    .and_then(Value::as_ref)
                {
                    Some(inner) => self.collection(inner),
                    None => Ok(Some(vec![])),
                }
            }
            _ => Ok(None),
        }
    }

    /// Key/value pairs of `HashMap`, `LinkedHashMap`, `TreeMap`, `Hashtable`.
    pub fn map(&self, h: Handle) -> Result<Option<Vec<(Option<Handle>, Option<Handle>)>>> {
        let name = self.class_name(h)?.to_owned();
        let refs = match name.as_str() {
            "java.util.HashMap" | "java.util.LinkedHashMap" => {
                self.annotation_refs(h, "java.util.HashMap", 0)?
            }
            "java.util.TreeMap" | "java.util.Hashtable" | "java.util.Properties" => {
                let class = if name == "java.util.TreeMap" {
                    "java.util.TreeMap"
                } else {
                    "java.util.Hashtable"
                };
                self.annotation_refs(h, class, 0)?
            }
            n if n.starts_with("java.util.Collections$") => {
                return match self.field(h, "m")?.and_then(Value::as_ref) {
                    Some(inner) => self.map(inner),
                    None => Ok(Some(vec![])),
                }
            }
            _ => return Ok(None),
        };
        Ok(Some(
            refs.chunks(2)
                .map(|c| (c[0], c.get(1).copied().flatten()))
                .collect(),
        ))
    }

    /// Millisecond timestamp of a `java.util.Date`.
    pub fn date_millis(&self, h: Handle) -> Result<Option<i64>> {
        let Some(cd) = self.class_data(h, "java.util.Date")? else {
            return Ok(None);
        };
        for a in &cd.annotations {
            if let Annotation::Block(b) = a {
                if b.len() >= 8 {
                    return Ok(Some(i64::from_be_bytes(b[..8].try_into().unwrap())));
                }
            }
        }
        Ok(None)
    }

    /// Raw byte array contents.
    pub fn byte_array(&self, h: Handle) -> Result<&[u8]> {
        match self.get(h)? {
            Content::Array(Array {
                elements: ArrayData::Bytes(b),
                ..
            }) => Ok(b),
            _ => Err(Error::WrongKind(h, "byte[]")),
        }
    }

    fn annotation_refs(&self, h: Handle, class: &str, skip: usize) -> Result<Vec<Option<Handle>>> {
        let Some(cd) = self.class_data(h, class)? else {
            return Ok(vec![]);
        };
        Ok(cd
            .annotations
            .iter()
            .filter_map(|a| match a {
                Annotation::Content(r) => Some(*r),
                Annotation::Block(_) => None,
            })
            .skip(skip)
            .collect())
    }
}
