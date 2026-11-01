//! Stable hashing and the keyed random streams.
//!
//! Every random choice in a tree draws from its own stream, seeded with
//! `stable_hash(account_id, key)`. Adding a language opens new streams and
//! leaves every other limb's stream untouched, which a single sequential
//! stream could not do.
//!
//! Both halves live in the crate on purpose: SipHash-1-3 with fixed keys for
//! the seed, SplitMix64 for the stream. `std`'s `DefaultHasher` is not
//! guaranteed stable across Rust versions, and a dependency bump must never
//! change anyone's tree.

/// The fixed SipHash keys: "gitbonsa" and "i trees!" as little-endian words.
const K0: u64 = 0x6173_6e6f_6274_6967;
const K1: u64 = 0x2173_6565_7274_2069;

/// The increment of SplitMix64: the golden ratio as a 64-bit fraction.
const GOLDEN_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// SipHash-1-3 of `bytes` under the crate's fixed keys.
pub fn siphash13(bytes: &[u8]) -> u64 {
    siphash(bytes, 1, 3)
}

/// SipHash-c-d: `c` rounds per word, `d` to finalize.
fn siphash(bytes: &[u8], c: usize, d: usize) -> u64 {
    let mut v0 = K0 ^ 0x736f_6d65_7073_6575;
    let mut v1 = K1 ^ 0x646f_7261_6e64_6f6d;
    let mut v2 = K0 ^ 0x6c79_6765_6e65_7261;
    let mut v3 = K1 ^ 0x7465_6462_7974_6573;

    let (words, tail) = bytes.as_chunks::<8>();
    for word in words {
        let m = u64::from_le_bytes(*word);
        v3 ^= m;
        for _ in 0..c {
            sipround(&mut v0, &mut v1, &mut v2, &mut v3);
        }
        v0 ^= m;
    }

    let mut last = [0u8; 8];
    last[..tail.len()].copy_from_slice(tail);
    #[allow(
        clippy::cast_possible_truncation,
        reason = "SipHash folds the length in modulo 256 by definition"
    )]
    {
        last[7] = bytes.len() as u8;
    }
    let b = u64::from_le_bytes(last);
    v3 ^= b;
    for _ in 0..c {
        sipround(&mut v0, &mut v1, &mut v2, &mut v3);
    }
    v0 ^= b;

    v2 ^= 0xff;
    for _ in 0..d {
        sipround(&mut v0, &mut v1, &mut v2, &mut v3);
    }
    v0 ^ v1 ^ v2 ^ v3
}

/// One SipRound.
fn sipround(v0: &mut u64, v1: &mut u64, v2: &mut u64, v3: &mut u64) {
    *v0 = v0.wrapping_add(*v1);
    *v1 = v1.rotate_left(13) ^ *v0;
    *v0 = v0.rotate_left(32);
    *v2 = v2.wrapping_add(*v3);
    *v3 = v3.rotate_left(16) ^ *v2;
    *v0 = v0.wrapping_add(*v3);
    *v3 = v3.rotate_left(21) ^ *v0;
    *v2 = v2.wrapping_add(*v1);
    *v1 = v1.rotate_left(17) ^ *v2;
    *v2 = v2.rotate_left(32);
}

/// The seed of the stream named `key` for one account.
pub fn stable_hash(account_id: u64, key: &str) -> u64 {
    let mut bytes = Vec::with_capacity(8 + key.len());
    bytes.extend_from_slice(&account_id.to_le_bytes());
    bytes.extend_from_slice(key.as_bytes());
    siphash13(&bytes)
}

/// A SplitMix64 stream (Steele, Lea and Flood).
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// The stream named `key` for the account `seed`.
    pub fn keyed(seed: u64, key: &str) -> Self {
        Self {
            state: stable_hash(seed, key),
        }
    }

    /// 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GOLDEN_GAMMA);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A double in `[0, 1)`: the 53 high bits, so every value is exact.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// A double in `[lo, hi)`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    /// An integer in `[0, n)`; `n` must be positive.
    pub fn below(&mut self, n: u32) -> u32 {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "the product of a u32 and a 32-bit fraction, shifted down 32 bits, fits a u32"
        )]
        let value = (((self.next_u64() >> 32) * u64::from(n)) >> 32) as u32;
        value
    }

    /// `1.0` or `-1.0`, evenly.
    pub fn sign(&mut self) -> f64 {
        if self.next_u64() >> 63 == 0 {
            1.0
        } else {
            -1.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pinned outputs: if these change, every tree in the world changes.
    #[test]
    fn the_hash_and_stream_never_change() {
        assert_eq!(siphash13(b""), siphash13(b""));
        let pinned = [
            stable_hash(583231, "trunk"),
            stable_hash(583231, "limb/Rust"),
            Rng::keyed(583231, "trunk").next_u64(),
        ];
        assert_eq!(pinned, PINNED);
    }

    const PINNED: [u64; 3] = [
        17832996917467008057,
        18090000154751447211,
        17305018135541851992,
    ];

    /// The same rounds with 2-4 parameters match std's SipHash-2-4, which
    /// checks the round function, the word order and the length padding.
    #[test]
    #[allow(
        deprecated,
        reason = "std's SipHasher is the only SipHash reference in std"
    )]
    fn the_rounds_match_std_siphash24() {
        use std::hash::Hasher;
        for len in 0..40u8 {
            let bytes: Vec<u8> = (0..len)
                .map(|i| i.wrapping_mul(37).wrapping_add(11))
                .collect();
            let mut std = std::hash::SipHasher::new_with_keys(K0, K1);
            std.write(&bytes);
            assert_eq!(siphash(&bytes, 2, 4), std.finish(), "length {len}");
        }
    }

    #[test]
    fn different_keys_and_accounts_give_different_streams() {
        assert_ne!(stable_hash(1, "limb/Rust"), stable_hash(1, "limb/Python"));
        assert_ne!(stable_hash(1, "limb/Rust"), stable_hash(2, "limb/Rust"));
        // Lengths across a word boundary hash differently.
        assert_ne!(siphash13(b"1234567"), siphash13(b"12345678"));
    }

    #[test]
    fn a_draw_stays_in_its_interval() {
        let mut rng = Rng::keyed(7, "test");
        for _ in 0..10_000 {
            let u = rng.unit();
            assert!((0.0..1.0).contains(&u));
            assert!(rng.below(5) < 5);
            let r = rng.range(-3.0, 4.0);
            assert!((-3.0..4.0).contains(&r));
        }
    }
}
