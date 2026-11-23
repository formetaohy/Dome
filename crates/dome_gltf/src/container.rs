const GLB_MAGIC: u32 = 0x4654_6C67;
const GLB_VERSION: u32 = 2;
const JSON_CHUNK: u32 = 0x4E4F_534A;
const BINARY_CHUNK: u32 = 0x004E_4942;

pub(crate) struct Container {
    pub(crate) json: Vec<u8>,
    pub(crate) binary: Option<Vec<u8>>,
}

impl Container {
    pub(crate) fn open(bytes: &[u8]) -> Result<Self, String> {
        if word(bytes, 0) == Some(GLB_MAGIC) {
            return Self::binary(bytes);
        }
        if bytes.is_empty() {
            return Err(String::from("holds no text"));
        }
        Ok(Self {
            json: bytes.to_vec(),
            binary: None,
        })
    }

    fn binary(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 12 {
            return Err(String::from("ends inside its header"));
        }
        let version = word(bytes, 4).expect("a twelve byte header holds its version word");
        if version != GLB_VERSION {
            return Err(format!("holds the container version {version}"));
        }
        let length = word(bytes, 8).expect("a twelve byte header holds its length word") as usize;
        if length != bytes.len() {
            return Err(format!("names {length} bytes and holds {}", bytes.len()));
        }
        let mut json = None;
        let mut binary = None;
        let mut at = 12;
        while at < bytes.len() {
            let held = word(bytes, at).ok_or_else(|| String::from("ends inside a word"))? as usize;
            let kind = word(bytes, at + 4).ok_or_else(|| String::from("ends inside a word"))?;
            let start = at + 8;
            let end = start
                .checked_add(held)
                .ok_or_else(|| String::from("holds a chunk beyond its bytes"))?;
            if end > bytes.len() {
                return Err(String::from("ends inside a chunk"));
            }
            match kind {
                JSON_CHUNK => {
                    if json.is_some() || binary.is_some() {
                        return Err(String::from("holds a second JSON chunk"));
                    }
                    json = Some(bytes[start..end].to_vec());
                }
                BINARY_CHUNK => {
                    if json.is_none() {
                        return Err(String::from("holds a binary chunk before its JSON chunk"));
                    }
                    if binary.is_some() {
                        return Err(String::from("holds a second binary chunk"));
                    }
                    binary = Some(bytes[start..end].to_vec());
                }
                _ => return Err(format!("holds the unknown chunk {kind}")),
            }
            at = end;
        }
        Ok(Self {
            json: json.ok_or_else(|| String::from("holds no JSON chunk"))?,
            binary,
        })
    }
}

fn word(bytes: &[u8], at: usize) -> Option<u32> {
    let held = bytes.get(at..at + 4)?;
    Some(u32::from_le_bytes([held[0], held[1], held[2], held[3]]))
}
