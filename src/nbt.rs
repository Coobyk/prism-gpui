//! Minimal NBT reader (big-endian, gzip/zlib/raw), enough for `level.dat` and
//! `servers.dat`. Mirrors the parts of libnbtplusplus the launcher uses.

use flate2::read::GzDecoder;
use std::collections::HashMap;
use std::io::Read;

#[derive(Debug, Clone, PartialEq)]
pub enum Nbt {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<u8>),
    String(String),
    List(Vec<Nbt>),
    Compound(HashMap<String, Nbt>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Nbt {
    /// Look up a nested compound value by `/`-separated path.
    pub fn get(&self, path: &str) -> Option<&Nbt> {
        let mut cur = self;
        for key in path.split('/') {
            match cur {
                Nbt::Compound(map) => cur = map.get(key)?,
                _ => return None,
            }
        }
        Some(cur)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Nbt::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Nbt::Byte(v) => Some(*v as i64),
            Nbt::Short(v) => Some(*v as i64),
            Nbt::Int(v) => Some(*v as i64),
            Nbt::Long(v) => Some(*v),
            _ => None,
        }
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.pos + n > self.data.len() {
            return None;
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Some(s)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    fn i16(&mut self) -> Option<i16> {
        self.take(2).map(|b| i16::from_be_bytes([b[0], b[1]]))
    }
    fn i32(&mut self) -> Option<i32> {
        self.take(4)
            .map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn i64(&mut self) -> Option<i64> {
        self.take(8).map(|b| {
            i64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
        })
    }
    fn f32(&mut self) -> Option<f32> {
        self.i32().map(|v| f32::from_bits(v as u32))
    }
    fn f64(&mut self) -> Option<f64> {
        self.i64().map(|v| f64::from_bits(v as u64))
    }
    fn string(&mut self) -> Option<String> {
        let len = self.i16()? as usize;
        let bytes = self.take(len)?;
        Some(String::from_utf8_lossy(bytes).to_string())
    }

    /// Read a payload of the given tag id. `named` controls whether a name
    /// prefix is present (named for root/compound entries, unnamed in lists).
    fn payload(&mut self, id: u8, depth: usize) -> Option<Nbt> {
        if depth > 64 {
            return None;
        }
        Some(match id {
            0 => Nbt::Byte(0),
            1 => Nbt::Byte(self.u8()? as i8),
            2 => Nbt::Short(self.i16()?),
            3 => Nbt::Int(self.i32()?),
            4 => Nbt::Long(self.i64()?),
            5 => Nbt::Float(self.f32()?),
            6 => Nbt::Double(self.f64()?),
            7 => {
                let len = self.i32()? as usize;
                Nbt::ByteArray(self.take(len)?.to_vec())
            }
            8 => Nbt::String(self.string()?),
            9 => {
                let elem = self.u8()?;
                let len = self.i32()?;
                let mut items = Vec::new();
                for _ in 0..len.max(0) {
                    items.push(self.payload(elem, depth + 1)?);
                }
                Nbt::List(items)
            }
            10 => {
                let mut map = HashMap::new();
                loop {
                    let id = self.u8()?;
                    if id == 0 {
                        break;
                    }
                    let name = self.string()?;
                    let value = self.payload(id, depth + 1)?;
                    map.insert(name, value);
                }
                Nbt::Compound(map)
            }
            11 => {
                let len = self.i32()? as usize;
                let mut items = Vec::with_capacity(len.min(1024));
                for _ in 0..len {
                    items.push(self.i32()?);
                }
                Nbt::IntArray(items)
            }
            12 => {
                let len = self.i32()? as usize;
                let mut items = Vec::with_capacity(len.min(1024));
                for _ in 0..len {
                    items.push(self.i64()?);
                }
                Nbt::LongArray(items)
            }
            _ => return None,
        })
    }
}

/// Parse an NBT document, transparently decompressing gzip/zlib framing.
pub fn parse(bytes: &[u8]) -> Option<Nbt> {
    let decompressed = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        GzDecoder::new(bytes).read_to_end(&mut out).ok()?;
        out
    } else if bytes.len() >= 2 && bytes[0] == 0x78 {
        let mut out = Vec::new();
        flate2::read::ZlibDecoder::new(bytes)
            .read_to_end(&mut out)
            .ok()?;
        out
    } else {
        bytes.to_vec()
    };

    let mut r = Reader::new(&decompressed);
    let id = r.u8()?;
    if id != 10 {
        return None;
    }
    let _root_name = r.string()?;
    r.payload(10, 0)
}

/// Read a `level.dat` file and return `(LevelName, LastPlayed)`.
pub fn read_level_dat(path: &std::path::Path) -> Option<(String, i64)> {
    let bytes = std::fs::read(path).ok()?;
    let nbt = parse(&bytes)?;
    let name = nbt.get("Data/LevelName").and_then(|v| v.as_str())?.to_string();
    let played = nbt.get("Data/LastPlayed").and_then(|v| v.as_i64()).unwrap_or(0);
    Some((name, played))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Build a tiny gzipped level.dat-like document.
    fn sample() -> Vec<u8> {
        let mut raw = Vec::new();
        raw.push(10u8); // root compound
        raw.extend_from_slice(&0u16.to_be_bytes()); // root name ""
        // Data: compound
        raw.push(10u8);
        raw.extend_from_slice(&4u16.to_be_bytes());
        raw.extend_from_slice(b"Data");
        // LevelName: string
        raw.push(8u8);
        raw.extend_from_slice(&9u16.to_be_bytes());
        raw.extend_from_slice(b"LevelName");
        let name = b"My World";
        raw.extend_from_slice(&(name.len() as u16).to_be_bytes());
        raw.extend_from_slice(name);
        // LastPlayed: long
        raw.push(4u8);
        raw.extend_from_slice(&10u16.to_be_bytes());
        raw.extend_from_slice(b"LastPlayed");
        raw.extend_from_slice(&1234567i64.to_be_bytes());
        raw.push(0u8); // end Data
        raw.push(0u8); // end root

        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&raw).unwrap();
        gz.finish().unwrap()
    }

    #[test]
    fn parses_level_name_and_last_played() {
        let nbt = parse(&sample()).unwrap();
        assert_eq!(nbt.get("Data/LevelName").unwrap().as_str(), Some("My World"));
        assert_eq!(nbt.get("Data/LastPlayed").unwrap().as_i64(), Some(1234567));
    }

    #[test]
    fn reads_level_dat_file() {
        let td = tempfile::tempdir().unwrap();
        let p = td.path().join("level.dat");
        std::fs::write(&p, sample()).unwrap();
        let (name, played) = read_level_dat(&p).unwrap();
        assert_eq!(name, "My World");
        assert_eq!(played, 1234567);
    }
}
