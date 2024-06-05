pub struct TopicAvailable(Vec<Topic>);
impl TopicAvailable {
    pub fn new() -> Self {
        Self(Vec::new())
    }
}

pub struct Topic {
    name: String,
    hash: u32
}

impl Topic {
    pub(super) fn new(name: String) -> Self {
        let hash = Self::murmurhash3_32(name.as_bytes(), 69);
        Self { name, hash }
    }

    fn murmurhash3_32(key: &[u8], seed: u32) -> u32 {
        let c1: u32 = 0xcc9e2d51;
        let c2: u32 = 0x1b873593;
        let r1: u32 = 15;
        let r2: u32 = 13;
        let m: u32 = 5;
        let n: u32 = 0xe6546b64;
    
        let mut hash = seed;
        let len = key.len() as u32;
    
        let mut i = 0;
        while i + 4 <= len {
            let idx = i as usize;
            let mut k = u32::from_le_bytes([key[idx], key[idx + 1], key[idx + 2], key[idx + 3]]);
            k = k.wrapping_mul(c1);
            k = k.rotate_left(r1);
            k = k.wrapping_mul(c2);
    
            hash ^= k;
            hash = hash.rotate_left(r2);
            hash = hash.wrapping_mul(m).wrapping_add(n);
    
            i += 4;
        }
    
        let remaining = len & 3;
        if remaining > 0 {
            let mut k = 0;
            let idx = i as usize;
            for j in (0..remaining).rev() {
                k ^= (key[idx + j as usize] as u32) << (j * 8);
            }
            k = k.wrapping_mul(c1);
            k = k.rotate_left(r1);
            k = k.wrapping_mul(c2);
            hash ^= k;
        }
    
        hash ^= len;
        hash = hash ^ (hash >> 16);
        hash = hash.wrapping_mul(0x85ebca6b);
        hash = hash ^ (hash >> 13);
        hash = hash.wrapping_mul(0xc2b2ae35);
        hash = hash ^ (hash >> 16);
    
        hash
    }
}