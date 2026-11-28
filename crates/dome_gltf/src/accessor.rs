use crate::document::{Accessor as AccessorDef, BufferView, Document};
use crate::uri;
use dome_math::{Mat4, Vec4};
use std::path::Path;

pub(crate) struct Store<'a> {
    buffers: Vec<Vec<u8>>,
    views: &'a [BufferView],
    accessors: &'a [AccessorDef],
    what: &'a str,
}

impl<'a> Store<'a> {
    pub(crate) fn new(
        document: &'a Document,
        base: &Path,
        binary: Option<Vec<u8>>,
        what: &'a str,
    ) -> Result<Self, String> {
        let mut binary = binary;
        let buffers = document
            .buffers
            .iter()
            .enumerate()
            .map(|(index, buffer)| {
                let bytes = match &buffer.uri {
                    Some(uri) => uri::read(uri, base, what)?,
                    None if index == 0 => binary
                        .take()
                        .ok_or_else(|| format!("{what} holds no binary chunk for its buffer"))?,
                    None => {
                        return Err(format!("{what} holds the buffer {index} without a source"));
                    }
                };
                if bytes.len() < buffer.byte_length {
                    return Err(format!(
                        "{what} holds {} bytes for a buffer of {}",
                        bytes.len(),
                        buffer.byte_length,
                    ));
                }
                Ok(bytes)
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            buffers,
            views: &document.buffer_views,
            accessors: &document.accessors,
            what,
        })
    }

    pub(crate) fn view(&self, index: usize) -> Result<(usize, usize), String> {
        let view = self
            .views
            .get(index)
            .ok_or_else(|| format!("{} holds no buffer view {index}", self.what))?;
        let buffer = self
            .buffers
            .get(view.buffer)
            .ok_or_else(|| format!("{} holds no buffer {}", self.what, view.buffer))?;
        let offset = view.byte_offset.unwrap_or(0);
        if offset
            .checked_add(view.byte_length)
            .is_none_or(|end| end > buffer.len())
        {
            return Err(format!(
                "{} holds the buffer view {index} beyond its buffer",
                self.what,
            ));
        }
        Ok((offset, view.byte_length))
    }

    pub(crate) fn view_bytes(&self, index: usize) -> Result<&[u8], String> {
        let (offset, length) = self.view(index)?;
        let buffer = self
            .views
            .get(index)
            .ok_or_else(|| format!("{} holds no buffer view {index}", self.what))?
            .buffer;
        Ok(&self.buffers[buffer][offset..offset + length])
    }

    pub(crate) fn accessor(&self, index: usize) -> Result<Accessor<'_>, String> {
        let held = self
            .accessors
            .get(index)
            .ok_or_else(|| format!("{} holds no accessor {index}", self.what))?;
        if held.sparse.is_some() {
            return Err(format!("{} holds the sparse accessor {index}", self.what));
        }
        let component = Component::of(held.component_type, self.what)?;
        let components = match held.kind.as_str() {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            "MAT4" => 16,
            other => {
                return Err(format!(
                    "{} holds the accessor {index} of type {other}",
                    self.what
                ));
            }
        };
        let element = component.size() * components;
        let (bytes, stride) = match held.buffer_view {
            Some(view_index) => {
                let view = self
                    .views
                    .get(view_index)
                    .ok_or_else(|| format!("{} holds no buffer view {view_index}", self.what))?;
                let stride = view.byte_stride.unwrap_or(element);
                if stride < element {
                    return Err(format!(
                        "{} holds the buffer view {view_index} of stride {stride} below {element}",
                        self.what,
                    ));
                }
                let offset = held.byte_offset.unwrap_or(0);
                let length = if held.count == 0 {
                    0
                } else {
                    stride
                        .checked_mul(held.count - 1)
                        .and_then(|bytes| bytes.checked_add(element))
                        .ok_or_else(|| {
                            format!(
                                "{} holds the accessor {index} beyond its buffer view",
                                self.what
                            )
                        })?
                };
                let (start, held_length) = self.view(view_index)?;
                if offset
                    .checked_add(length)
                    .is_none_or(|end| end > held_length)
                {
                    return Err(format!(
                        "{} holds the accessor {index} beyond its buffer view",
                        self.what,
                    ));
                }
                let source = &self.buffers[view.buffer];
                (
                    Some(&source[start + offset..start + offset + length]),
                    stride,
                )
            }
            None => (None, 0),
        };
        Ok(Accessor {
            bytes,
            count: held.count,
            component,
            normalized: held.normalized.unwrap_or(false),
            components,
            stride,
            index,
            what: self.what,
        })
    }
}

pub(crate) struct Accessor<'a> {
    bytes: Option<&'a [u8]>,
    count: usize,
    component: Component,
    normalized: bool,
    components: usize,
    stride: usize,
    index: usize,
    what: &'a str,
}

impl Accessor<'_> {
    pub(crate) fn count(&self) -> usize {
        self.count
    }

    pub(crate) fn scalar_at(&self, at: usize) -> Result<f32, String> {
        self.assert_kind(at, 1)?;
        if self.normalized || !matches!(self.component, Component::F32) {
            return Err(format!(
                "{} holds the accessor {} of scalars in another component",
                self.what, self.index,
            ));
        }
        Ok(self.scalar(at, 0))
    }

    pub(crate) fn vector2(&self, at: usize) -> Result<[f32; 2], String> {
        self.assert_kind(at, 2)?;
        Ok([self.scalar(at, 0), self.scalar(at, 1)])
    }

    pub(crate) fn vector3(&self, at: usize) -> Result<[f32; 3], String> {
        self.assert_kind(at, 3)?;
        Ok([self.scalar(at, 0), self.scalar(at, 1), self.scalar(at, 2)])
    }

    pub(crate) fn vector4(&self, at: usize) -> Result<[f32; 4], String> {
        self.assert_kind(at, 4)?;
        Ok([
            self.scalar(at, 0),
            self.scalar(at, 1),
            self.scalar(at, 2),
            self.scalar(at, 3),
        ])
    }

    pub(crate) fn joints(&self, at: usize) -> Result<[u32; 4], String> {
        self.assert_kind(at, 4)?;
        if self.normalized {
            return Err(format!(
                "{} holds a normalized accessor {} of joints",
                self.what, self.index,
            ));
        }
        if !matches!(
            self.component,
            Component::U8 | Component::U16 | Component::U32
        ) {
            return Err(format!(
                "{} holds the accessor {} of joints in another component",
                self.what, self.index,
            ));
        }
        Ok([
            self.scalar(at, 0) as u32,
            self.scalar(at, 1) as u32,
            self.scalar(at, 2) as u32,
            self.scalar(at, 3) as u32,
        ])
    }

    pub(crate) fn matrix4(&self, at: usize) -> Result<Mat4, String> {
        self.assert_kind(at, 16)?;
        if self.normalized || !matches!(self.component, Component::F32) {
            return Err(format!(
                "{} holds the accessor {} of matrices in another component",
                self.what, self.index,
            ));
        }
        Ok(Mat4::from_cols(
            Vec4::new(
                self.scalar(at, 0),
                self.scalar(at, 1),
                self.scalar(at, 2),
                self.scalar(at, 3),
            ),
            Vec4::new(
                self.scalar(at, 4),
                self.scalar(at, 5),
                self.scalar(at, 6),
                self.scalar(at, 7),
            ),
            Vec4::new(
                self.scalar(at, 8),
                self.scalar(at, 9),
                self.scalar(at, 10),
                self.scalar(at, 11),
            ),
            Vec4::new(
                self.scalar(at, 12),
                self.scalar(at, 13),
                self.scalar(at, 14),
                self.scalar(at, 15),
            ),
        ))
    }

    pub(crate) fn index(&self, at: usize) -> Result<u32, String> {
        if at >= self.count {
            return Err(format!(
                "{} reads the accessor {} past its {} elements",
                self.what, self.index, self.count,
            ));
        }
        if self.normalized {
            return Err(format!(
                "{} holds a normalized accessor {} of indices",
                self.what, self.index,
            ));
        }
        let Some(bytes) = self.bytes else {
            return Ok(0);
        };
        let held = &bytes[at * self.stride..];
        match self.component {
            Component::U8 => Ok(u32::from(held[0])),
            Component::U16 => Ok(u32::from(u16::from_le_bytes([held[0], held[1]]))),
            Component::U32 => Ok(u32::from_le_bytes([held[0], held[1], held[2], held[3]])),
            _ => Err(format!(
                "{} holds the accessor {} of indices in another component",
                self.what, self.index,
            )),
        }
    }

    pub(crate) fn scalar(&self, at: usize, channel: usize) -> f32 {
        let Some(bytes) = self.bytes else {
            return 0.0;
        };
        let offset = at * self.stride + channel * self.component.size();
        self.component.value(bytes, offset, self.normalized)
    }

    fn assert_kind(&self, at: usize, components: usize) -> Result<(), String> {
        if self.components != components {
            return Err(format!(
                "{} reads {} components from the accessor {} of {}",
                self.what, components, self.index, self.components,
            ));
        }
        if at >= self.count {
            return Err(format!(
                "{} reads the accessor {} past its {} elements",
                self.what, self.index, self.count,
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Component {
    I8,
    U8,
    I16,
    U16,
    U32,
    F32,
}

impl Component {
    fn of(component_type: u32, what: &str) -> Result<Self, String> {
        match component_type {
            5120 => Ok(Self::I8),
            5121 => Ok(Self::U8),
            5122 => Ok(Self::I16),
            5123 => Ok(Self::U16),
            5125 => Ok(Self::U32),
            5126 => Ok(Self::F32),
            other => Err(format!("{what} holds the accessor component {other}")),
        }
    }

    fn size(self) -> usize {
        match self {
            Self::I8 | Self::U8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::U32 | Self::F32 => 4,
        }
    }

    fn value(self, bytes: &[u8], at: usize, normalized: bool) -> f32 {
        match self {
            Self::I8 => {
                let value = bytes[at] as i8;
                if normalized {
                    (value as f32 / 127.0).max(-1.0)
                } else {
                    f32::from(value)
                }
            }
            Self::U8 => {
                let value = bytes[at];
                if normalized {
                    f32::from(value) / 255.0
                } else {
                    f32::from(value)
                }
            }
            Self::I16 => {
                let value = i16::from_le_bytes([bytes[at], bytes[at + 1]]);
                if normalized {
                    (value as f32 / 32767.0).max(-1.0)
                } else {
                    f32::from(value)
                }
            }
            Self::U16 => {
                let value = u16::from_le_bytes([bytes[at], bytes[at + 1]]);
                if normalized {
                    f32::from(value) / 65535.0
                } else {
                    f32::from(value)
                }
            }
            Self::U32 => {
                let value =
                    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
                if normalized {
                    value as f32 / 4294967295.0
                } else {
                    value as f32
                }
            }
            Self::F32 => {
                f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
            }
        }
    }
}
