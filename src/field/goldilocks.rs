//! Goldilocks prime field: p = 2^64 - 2^32 + 1.
//!
//! Used by: Triton VM, Miden VM, OpenVM, Plonky3.
//!
//! The Goldilocks prime has efficient reduction: since 2^64 ≡ 2^32 - 1 (mod p),
//! products in u128 can be reduced without general division.

use super::PrimeField;

/// Goldilocks prime: p = 2^64 - 2^32 + 1 = 0xFFFF_FFFF_0000_0001.
pub const MODULUS: u64 = 0xFFFF_FFFF_0000_0001;

/// A Goldilocks field element (u64 in [0, p)).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Goldilocks(pub u64);

impl Goldilocks {
    /// Reduce a u128 value modulo p using the identity 2^64 ≡ 2^32 - 1 (mod p).
    #[inline]
    fn reduce128(x: u128) -> Self {
        let lo = x as u64;
        let hi = (x >> 64) as u64;
        let hi_shifted = (hi as u128) * ((1u128 << 32) - 1);
        let sum = lo as u128 + hi_shifted;
        let lo2 = sum as u64;
        let hi2 = (sum >> 64) as u64;
        if hi2 == 0 {
            Self(if lo2 >= MODULUS { lo2 - MODULUS } else { lo2 })
        } else {
            let r = lo2 as u128 + (hi2 as u128) * ((1u128 << 32) - 1);
            let lo3 = r as u64;
            let hi3 = (r >> 64) as u64;
            if hi3 == 0 {
                Self(if lo3 >= MODULUS { lo3 - MODULUS } else { lo3 })
            } else {
                let v = lo3.wrapping_add(hi3.wrapping_mul(u32::MAX as u64));
                Self(if v >= MODULUS { v - MODULUS } else { v })
            }
        }
    }

    /// The Poseidon2 S-box for Goldilocks: x^7.
    #[inline]
    pub fn sbox(self) -> Self {
        let x2 = self.mul(self);
        let x3 = x2.mul(self);
        let x6 = x3.mul(x3);
        x6.mul(self)
    }
}

impl PrimeField for Goldilocks {
    const MODULUS: u128 = MODULUS as u128;
    const BITS: u32 = 64;
    const ZERO: Self = Self(0);
    const ONE: Self = Self(1);

    #[inline]
    fn from_u64(v: u64) -> Self {
        Self(v % MODULUS)
    }

    #[inline]
    fn to_u64(self) -> u64 {
        self.0
    }

    #[inline]
    fn add(self, rhs: Self) -> Self {
        let (sum, carry) = self.0.overflowing_add(rhs.0);
        if carry {
            let r = sum + (u32::MAX as u64);
            Self(if r >= MODULUS { r - MODULUS } else { r })
        } else {
            Self(if sum >= MODULUS { sum - MODULUS } else { sum })
        }
    }

    #[inline]
    fn sub(self, rhs: Self) -> Self {
        if self.0 >= rhs.0 {
            Self(self.0 - rhs.0)
        } else {
            Self(MODULUS - rhs.0 + self.0)
        }
    }

    #[inline]
    fn mul(self, rhs: Self) -> Self {
        Self::reduce128((self.0 as u128) * (rhs.0 as u128))
    }

    #[inline]
    fn neg(self) -> Self {
        if self.0 == 0 {
            Self(0)
        } else {
            Self(MODULUS - self.0)
        }
    }
}
