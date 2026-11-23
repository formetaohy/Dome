const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Digest(u128);

impl Digest {
    pub fn of(bytes: &[u8]) -> Self {
        let mut builder = DigestBuilder {
            digest: Self(OFFSET),
        };
        builder.bytes(bytes);
        builder.finish()
    }

    pub fn builder() -> DigestBuilder {
        DigestBuilder {
            digest: Self(OFFSET),
        }
    }
}

#[derive(Clone, Copy)]
pub struct DigestBuilder {
    digest: Digest,
}

impl DigestBuilder {
    pub fn bytes(&mut self, bytes: &[u8]) -> &mut Self {
        for byte in bytes {
            self.digest.0 ^= u128::from(*byte);
            self.digest.0 = self.digest.0.wrapping_mul(PRIME);
        }
        self
    }

    pub fn number(&mut self, value: u64) -> &mut Self {
        self.bytes(&value.to_le_bytes())
    }

    pub fn text(&mut self, text: &str) -> &mut Self {
        self.number(text.len() as u64);
        self.bytes(text.as_bytes())
    }

    pub fn digest(&mut self, digest: Digest) -> &mut Self {
        self.bytes(&digest.0.to_le_bytes())
    }

    pub fn finish(&self) -> Digest {
        self.digest
    }
}
