//! Stream parser producing a [`Graph`].

use crate::consts::*;
use crate::graph::*;
use crate::mutf8::decode_modified_utf8;
use crate::{Error, Result};

/// Resource limits for parsing untrusted input.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_depth: usize,
    pub max_handles: usize,
    pub max_array_len: usize,
    pub max_string_len: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_depth: 512,
            max_handles: 16_000_000,
            max_array_len: 256 * 1024 * 1024,
            max_string_len: 64 * 1024 * 1024,
        }
    }
}

/// Parse a complete stream (header + one or more top-level contents).
pub fn parse(data: &[u8]) -> Result<Graph> {
    parse_with_limits(data, Limits::default())
}

pub fn parse_with_limits(data: &[u8], limits: Limits) -> Result<Graph> {
    let mut p = Parser {
        data,
        pos: 0,
        graph: Graph::default(),
        limits,
        depth: 0,
    };
    let magic = p.u16()?;
    let version = p.u16()?;
    if magic != STREAM_MAGIC || version != STREAM_VERSION {
        return Err(Error::BadHeader(magic, version));
    }
    while p.pos < p.data.len() {
        let tag = p.peek()?;
        if tag == TC_RESET {
            p.pos += 1;
            p.reset()?;
            continue;
        }
        if tag == TC_BLOCKDATA || tag == TC_BLOCKDATALONG {
            // Top-level primitive data (writeInt etc. on the stream); skip.
            p.block_data()?;
            continue;
        }
        let root = p.content("top-level")?;
        p.graph.roots.push(root);
    }
    Ok(p.graph)
}

struct Parser<'a> {
    data: &'a [u8],
    pos: usize,
    graph: Graph,
    limits: Limits,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or(Error::Eof)?;
        if end > self.data.len() {
            return Err(Error::Eof);
        }
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn peek(&self) -> Result<u8> {
        self.data.get(self.pos).copied().ok_or(Error::Eof)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn utf(&mut self) -> Result<String> {
        let len = self.u16()? as usize;
        decode_modified_utf8(self.take(len)?)
    }
    fn long_utf(&mut self) -> Result<String> {
        let len = self.i64()?;
        if len < 0 || len as usize > self.limits.max_string_len {
            return Err(Error::Limit("string length"));
        }
        decode_modified_utf8(self.take(len as usize)?)
    }

    /// TC_RESET restarts wire handle numbering. Arena ids equal wire handles,
    /// so a reset is only accepted before any handle was assigned (legacy
    /// Arma3Sync files never contain resets).
    fn reset(&mut self) -> Result<()> {
        if self.graph.items.is_empty() {
            Ok(())
        } else {
            Err(Error::Unsupported("TC_RESET after handles were assigned"))
        }
    }

    fn alloc(&mut self, c: Content) -> Result<Handle> {
        if self.graph.items.len() >= self.limits.max_handles {
            return Err(Error::Limit("handle count"));
        }
        self.graph.items.push(c);
        Ok(BASE_WIRE_HANDLE + (self.graph.items.len() as u32 - 1))
    }

    fn set(&mut self, h: Handle, c: Content) {
        let i = (h - BASE_WIRE_HANDLE) as usize;
        self.graph.items[i] = c;
    }

    fn reference(&mut self) -> Result<Handle> {
        let h = self.i32()? as u32;
        self.graph.get(h)?; // validate
        Ok(h)
    }

    fn enter(&mut self) -> Result<()> {
        self.depth += 1;
        if self.depth > self.limits.max_depth {
            return Err(Error::Limit("nesting depth"));
        }
        Ok(())
    }

    /// Parse one `content`/`object` production; returns `None` for TC_NULL.
    fn content(&mut self, context: &'static str) -> Result<Option<Handle>> {
        self.enter()?;
        let r = self.content_inner(context);
        self.depth -= 1;
        r
    }

    fn content_inner(&mut self, context: &'static str) -> Result<Option<Handle>> {
        let offset = self.pos;
        let tag = self.u8()?;
        match tag {
            TC_NULL => Ok(None),
            TC_REFERENCE => Ok(Some(self.reference()?)),
            TC_STRING => {
                let s = self.utf()?;
                Ok(Some(self.alloc(Content::String(s))?))
            }
            TC_LONGSTRING => {
                let s = self.long_utf()?;
                Ok(Some(self.alloc(Content::String(s))?))
            }
            TC_CLASSDESC => Ok(Some(self.class_desc_body()?)),
            TC_PROXYCLASSDESC => Err(Error::Unsupported("proxy class descriptors")),
            TC_CLASS => {
                let desc = self
                    .class_desc("class")?
                    .ok_or(Error::Unsupported("null class desc"))?;
                Ok(Some(self.alloc(Content::Class { desc })?))
            }
            TC_ENUM => {
                let desc = self
                    .class_desc("enum")?
                    .ok_or(Error::Unsupported("null enum desc"))?;
                let h = self.alloc(Content::Pending)?;
                let name_h = self
                    .content("enum constant")?
                    .ok_or(Error::Unsupported("null enum name"))?;
                let constant = self.graph.string(name_h)?.to_owned();
                self.set(h, Content::Enum { desc, constant });
                Ok(Some(h))
            }
            TC_ARRAY => {
                let desc = self
                    .class_desc("array")?
                    .ok_or(Error::Unsupported("null array desc"))?;
                let h = self.alloc(Content::Pending)?;
                let len = self.i32()?;
                if len < 0 || len as usize > self.limits.max_array_len {
                    return Err(Error::Limit("array length"));
                }
                let len = len as usize;
                let elem_type = self
                    .graph
                    .class_desc(desc)?
                    .name
                    .as_bytes()
                    .get(1)
                    .copied()
                    .unwrap_or(b'L');
                let elements = if elem_type == b'B' {
                    ArrayData::Bytes(self.take(len)?.to_vec())
                } else {
                    // Guard against huge allocations from bogus lengths: elements
                    // need at least one byte each.
                    if len > self.data.len() - self.pos {
                        return Err(Error::Eof);
                    }
                    let mut v = Vec::with_capacity(len);
                    for _ in 0..len {
                        v.push(self.value(elem_type)?);
                    }
                    ArrayData::Values(v)
                };
                self.set(h, Content::Array(Array { desc, elements }));
                Ok(Some(h))
            }
            TC_OBJECT => {
                let desc = self
                    .class_desc("object")?
                    .ok_or(Error::Unsupported("null object desc"))?;
                let h = self.alloc(Content::Pending)?;
                let obj = self.object_data(desc)?;
                self.set(h, Content::Object(obj));
                Ok(Some(h))
            }
            TC_EXCEPTION => Err(Error::Unsupported("serialized exception")),
            TC_RESET => {
                self.reset()?;
                self.content_inner(context)
            }
            code => Err(Error::UnexpectedTag {
                code,
                offset,
                context,
            }),
        }
    }

    /// `classDesc` production: TC_CLASSDESC body, TC_NULL or TC_REFERENCE.
    fn class_desc(&mut self, context: &'static str) -> Result<Option<Handle>> {
        let offset = self.pos;
        match self.u8()? {
            TC_NULL => Ok(None),
            TC_REFERENCE => {
                let h = self.reference()?;
                self.graph.class_desc(h)?;
                Ok(Some(h))
            }
            TC_CLASSDESC => Ok(Some(self.class_desc_body()?)),
            TC_PROXYCLASSDESC => Err(Error::Unsupported("proxy class descriptors")),
            code => Err(Error::UnexpectedTag {
                code,
                offset,
                context,
            }),
        }
    }

    fn class_desc_body(&mut self) -> Result<Handle> {
        self.enter()?;
        let name = self.utf()?;
        let uid = self.i64()?;
        // Java assigns the descriptor handle before reading field type strings.
        let h = self.alloc(Content::Pending)?;
        let flags = self.u8()?;
        let count = self.u16()? as usize;
        let mut fields = Vec::with_capacity(count);
        for _ in 0..count {
            let type_code = self.u8()?;
            let fname = self.utf()?;
            let class_name = match type_code {
                b'L' | b'[' => {
                    let sh = self
                        .content("field type")?
                        .ok_or(Error::Unsupported("null type string"))?;
                    Some(self.graph.string(sh)?.to_owned())
                }
                b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' => None,
                c => return Err(Error::BadFieldType(c)),
            };
            fields.push(FieldDesc {
                type_code,
                name: fname,
                class_name,
            });
        }
        let annotations = self.annotations()?;
        let super_desc = self.class_desc("super class desc")?;
        self.set(
            h,
            Content::ClassDesc(ClassDesc {
                name,
                serial_version_uid: uid,
                flags,
                fields,
                annotations,
                super_desc,
            }),
        );
        self.depth -= 1;
        Ok(h)
    }

    /// Read content items and block data until TC_ENDBLOCKDATA.
    fn annotations(&mut self) -> Result<Vec<Annotation>> {
        let mut out = Vec::new();
        loop {
            match self.peek()? {
                TC_ENDBLOCKDATA => {
                    self.pos += 1;
                    return Ok(out);
                }
                TC_BLOCKDATA | TC_BLOCKDATALONG => {
                    let b = self.block_data()?;
                    // Merge consecutive block segments into one logical block.
                    if let Some(Annotation::Block(prev)) = out.last_mut() {
                        prev.extend_from_slice(&b);
                    } else {
                        out.push(Annotation::Block(b));
                    }
                }
                _ => out.push(Annotation::Content(self.content("annotation")?)),
            }
        }
    }

    fn block_data(&mut self) -> Result<Vec<u8>> {
        let len = match self.u8()? {
            TC_BLOCKDATA => self.u8()? as usize,
            TC_BLOCKDATALONG => {
                let l = self.i32()?;
                if l < 0 {
                    return Err(Error::Limit("block length"));
                }
                l as usize
            }
            _ => unreachable!(),
        };
        Ok(self.take(len)?.to_vec())
    }

    fn value(&mut self, type_code: u8) -> Result<Value> {
        Ok(match type_code {
            b'B' => Value::Byte(self.u8()? as i8),
            b'C' => Value::Char(self.u16()?),
            b'D' => Value::Double(f64::from_bits(self.i64()? as u64)),
            b'F' => Value::Float(f32::from_bits(self.i32()? as u32)),
            b'I' => Value::Int(self.i32()?),
            b'J' => Value::Long(self.i64()?),
            b'S' => Value::Short(self.u16()? as i16),
            b'Z' => Value::Boolean(self.u8()? != 0),
            b'L' | b'[' => Value::Ref(self.content("field value")?),
            c => return Err(Error::BadFieldType(c)),
        })
    }

    fn object_data(&mut self, desc: Handle) -> Result<Object> {
        // Build hierarchy chain: top-most superclass first.
        let mut chain = Vec::new();
        let mut cur = Some(desc);
        while let Some(d) = cur {
            if chain.len() > 64 {
                return Err(Error::Limit("class hierarchy depth"));
            }
            chain.push(d);
            cur = self.graph.class_desc(d)?.super_desc;
        }
        chain.reverse();
        let mut class_data = Vec::with_capacity(chain.len());
        for d in chain {
            let cd = self.graph.class_desc(d)?;
            let flags = cd.flags;
            let fields: Vec<(u8, String)> = cd
                .fields
                .iter()
                .map(|f| (f.type_code, f.name.clone()))
                .collect();
            if flags & SC_EXTERNALIZABLE != 0 {
                if flags & SC_BLOCK_DATA == 0 {
                    return Err(Error::Unsupported(
                        "externalizable without block data (protocol 1)",
                    ));
                }
                let annotations = self.annotations()?;
                class_data.push(ClassData {
                    desc: d,
                    values: vec![],
                    annotations,
                });
                continue;
            }
            let mut values = Vec::with_capacity(fields.len());
            for (tc, name) in fields {
                let v = self.value(tc)?;
                values.push((name, v));
            }
            let annotations = if flags & SC_WRITE_METHOD != 0 {
                self.annotations()?
            } else {
                vec![]
            };
            class_data.push(ClassData {
                desc: d,
                values,
                annotations,
            });
        }
        Ok(Object { desc, class_data })
    }
}
