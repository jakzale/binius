use std::{
	arch::x86_64::*,
	mem::transmute_copy,
	ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not, Shl, Shr},
};

use bytemuck::{must_cast, Pod, Zeroable};
use rand::{Rng, RngCore};
use subtle::{Choice, ConstantTimeEq};

use crate::field::{
	arch::portable::{
		packed::PackedPrimitiveType,
		packed_arithmetic::{interleave_mask_even, interleave_mask_odd, UnderlierWithBitConstants},
	},
	arithmetic_traits::Broadcast,
	underlier::{NumCast, Random, UnderlierType, WithUnderlier},
	BinaryField,
};

/// 256-bit value that is used for 256-bit SIMD operations
#[derive(Copy, Clone, Debug)]
pub struct M256(__m256i);

impl From<__m256i> for M256 {
	fn from(value: __m256i) -> Self {
		Self(value)
	}
}

impl From<[u128; 2]> for M256 {
	fn from(value: [u128; 2]) -> Self {
		Self(unsafe {
			_mm256_set_epi64x(
				(value[1] >> 64) as i64,
				value[1] as i64,
				(value[0] >> 64) as i64,
				value[0] as i64,
			)
		})
	}
}

impl From<u128> for M256 {
	fn from(value: u128) -> Self {
		Self::from([value, 0])
	}
}

impl From<u64> for M256 {
	fn from(value: u64) -> Self {
		Self::from(value as u128)
	}
}

impl From<u32> for M256 {
	fn from(value: u32) -> Self {
		Self::from(value as u128)
	}
}

impl From<u16> for M256 {
	fn from(value: u16) -> Self {
		Self::from(value as u128)
	}
}

impl From<u8> for M256 {
	fn from(value: u8) -> Self {
		Self::from(value as u128)
	}
}
impl From<M256> for [u128; 2] {
	fn from(value: M256) -> Self {
		let result: [u128; 2] = unsafe { transmute_copy(&value.0) };

		result
	}
}

impl From<M256> for __m256i {
	fn from(value: M256) -> Self {
		value.0
	}
}
impl<U: NumCast<u128>> NumCast<M256> for U {
	fn num_cast_from(val: M256) -> Self {
		let [low, _high] = val.into();
		Self::num_cast_from(low)
	}
}

impl Default for M256 {
	fn default() -> Self {
		Self(unsafe { _mm256_setzero_si256() })
	}
}

impl BitAnd for M256 {
	type Output = Self;

	fn bitand(self, rhs: Self) -> Self::Output {
		Self(unsafe { _mm256_and_si256(self.0, rhs.0) })
	}
}

impl BitAndAssign for M256 {
	fn bitand_assign(&mut self, rhs: Self) {
		*self = *self & rhs
	}
}

impl BitOr for M256 {
	type Output = Self;

	fn bitor(self, rhs: Self) -> Self::Output {
		Self(unsafe { _mm256_or_si256(self.0, rhs.0) })
	}
}

impl BitOrAssign for M256 {
	fn bitor_assign(&mut self, rhs: Self) {
		*self = *self | rhs
	}
}

impl BitXor for M256 {
	type Output = Self;

	fn bitxor(self, rhs: Self) -> Self::Output {
		Self(unsafe { _mm256_xor_si256(self.0, rhs.0) })
	}
}

impl BitXorAssign for M256 {
	fn bitxor_assign(&mut self, rhs: Self) {
		*self = *self ^ rhs;
	}
}

impl Not for M256 {
	type Output = Self;

	fn not(self) -> Self::Output {
		const ONES: __m256i = m256_from_u128s!(u128::MAX, u128::MAX,);

		self ^ Self(ONES)
	}
}

impl Shr<usize> for M256 {
	type Output = Self;

	/// TODO: this is unefficient implementation
	fn shr(self, rhs: usize) -> Self::Output {
		match rhs {
			rhs if rhs >= 256 => Self::ZERO,
			0 => self,
			rhs => {
				let [mut low, mut high]: [u128; 2] = self.into();
				if rhs >= 128 {
					low = high >> (rhs - 128);
					high = 0;
				} else {
					low = (low >> rhs) + (high << (128usize - rhs));
					high >>= rhs
				}
				[low, high].into()
			}
		}
	}
}
impl Shl<usize> for M256 {
	type Output = Self;

	/// TODO: this is unefficient implementation
	fn shl(self, rhs: usize) -> Self::Output {
		match rhs {
			rhs if rhs >= 256 => Self::ZERO,
			0 => self,
			rhs => {
				let [mut low, mut high]: [u128; 2] = self.into();
				if rhs >= 128 {
					high = low << (rhs - 128);
					low = 0;
				} else {
					high = (high << rhs) + (low >> (128usize - rhs));
					low <<= rhs
				}
				[low, high].into()
			}
		}
	}
}

impl PartialEq for M256 {
	fn eq(&self, other: &Self) -> bool {
		unsafe {
			let pcmp = _mm256_cmpeq_epi32(self.0, other.0);
			let bitmask = _mm256_movemask_epi8(pcmp) as u32;
			bitmask == 0xffffffff
		}
	}
}

impl Eq for M256 {}

impl ConstantTimeEq for M256 {
	fn ct_eq(&self, other: &Self) -> Choice {
		unsafe {
			let pcmp = _mm256_cmpeq_epi32(self.0, other.0);
			let bitmask = _mm256_movemask_epi8(pcmp) as u32;
			bitmask.ct_eq(&0xffffffff)
		}
	}
}

impl Random for M256 {
	fn random(mut rng: impl RngCore) -> Self {
		let val: [u128; 2] = rng.gen();
		val.into()
	}
}

impl std::fmt::Display for M256 {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let data: [u128; 2] = (*self).into();
		write!(f, "{data:02X?}")
	}
}

#[repr(align(32))]
pub struct AlignedData(pub [u128; 2]);

macro_rules! m256_from_u128s {
    ($($values:expr,)+) => {{
        let aligned_data = $crate::field::arch::x86_64::m256::AlignedData([$($values,)*]);
        unsafe {* (aligned_data.0.as_ptr() as *const __m256i)}
    }};
}

pub(super) use m256_from_u128s;

impl UnderlierType for M256 {
	const LOG_BITS: usize = 8;

	const ONE: Self = { Self(m256_from_u128s!(0, 1,)) };

	const ZERO: Self = { Self(m256_from_u128s!(0, 0,)) };

	fn fill_with_bit(val: u8) -> Self {
		Self(unsafe { _mm256_set1_epi8(val.wrapping_neg() as i8) })
	}
}

unsafe impl Zeroable for M256 {}

unsafe impl Pod for M256 {}

unsafe impl Send for M256 {}

unsafe impl Sync for M256 {}

impl<Scalar: BinaryField> From<__m256i> for PackedPrimitiveType<M256, Scalar> {
	fn from(value: __m256i) -> Self {
		PackedPrimitiveType::from(M256::from(value))
	}
}

impl<Scalar: BinaryField> From<[u128; 2]> for PackedPrimitiveType<M256, Scalar> {
	fn from(value: [u128; 2]) -> Self {
		PackedPrimitiveType::from(M256::from(value))
	}
}

impl<Scalar: BinaryField> From<PackedPrimitiveType<M256, Scalar>> for __m256i {
	fn from(value: PackedPrimitiveType<M256, Scalar>) -> Self {
		value.to_underlier().into()
	}
}

impl<Scalar: BinaryField + WithUnderlier> Broadcast<Scalar> for PackedPrimitiveType<M256, Scalar>
where
	u128: From<Scalar::Underlier>,
{
	fn broadcast(scalar: Scalar) -> Self {
		let tower_level = Scalar::N_BITS.ilog2() as usize;
		let mut value = u128::from(scalar.to_underlier());
		for n in tower_level..3 {
			value |= value << (1 << n);
		}

		match tower_level {
			0..=3 => unsafe { _mm256_broadcastb_epi8(must_cast(value)).into() },
			4 => unsafe { _mm256_broadcastw_epi16(must_cast(value)).into() },
			5 => unsafe { _mm256_broadcastd_epi32(must_cast(value)).into() },
			6 => unsafe { _mm256_broadcastq_epi64(must_cast(value)).into() },
			7 => [value, value].into(),
			_ => unreachable!(),
		}
	}
}

const fn from_equal_u128s(val: u128) -> M256 {
	unsafe { transmute_copy(&[val, val]) }
}

// TODO: Add efficient interleave specialization for 256 values
impl UnderlierWithBitConstants for M256 {
	const INTERLEAVE_EVEN_MASK: &'static [Self] = &[
		from_equal_u128s(interleave_mask_even!(u128, 0)),
		from_equal_u128s(interleave_mask_even!(u128, 1)),
		from_equal_u128s(interleave_mask_even!(u128, 2)),
		from_equal_u128s(interleave_mask_even!(u128, 3)),
		from_equal_u128s(interleave_mask_even!(u128, 4)),
		from_equal_u128s(interleave_mask_even!(u128, 5)),
		from_equal_u128s(interleave_mask_even!(u128, 6)),
	];

	const INTERLEAVE_ODD_MASK: &'static [Self] = &[
		from_equal_u128s(interleave_mask_odd!(u128, 0)),
		from_equal_u128s(interleave_mask_odd!(u128, 1)),
		from_equal_u128s(interleave_mask_odd!(u128, 2)),
		from_equal_u128s(interleave_mask_odd!(u128, 3)),
		from_equal_u128s(interleave_mask_odd!(u128, 4)),
		from_equal_u128s(interleave_mask_odd!(u128, 5)),
		from_equal_u128s(interleave_mask_odd!(u128, 6)),
	];
}

#[cfg(test)]
mod tests {
	use proptest::{arbitrary::any, proptest};

	use super::*;

	fn check_roundtrip<T>(val: M256)
	where
		T: From<M256>,
		M256: From<T>,
	{
		assert_eq!(M256::from(T::from(val)), val);
	}

	#[test]
	fn test_constants() {
		assert_eq!(M256::default(), M256::ZERO);
		assert_eq!(M256::from(0u128), M256::ZERO);
		assert_eq!(M256::from([0u128, 1u128]), M256::ONE);
	}

	#[derive(Default)]
	struct ByteData([u128; 2]);

	impl ByteData {
		fn get_bit(&self, i: usize) -> u8 {
			if self.0[i / 128] & (1u128 << (i % 128)) == 0 {
				0
			} else {
				1
			}
		}

		fn set_bit(&mut self, i: usize, val: u8) {
			self.0[i / 128] &= !(1 << (i % 128));
			self.0[i / 128] |= (val as u128) << (i % 128);
		}
	}

	impl From<ByteData> for M256 {
		fn from(value: ByteData) -> Self {
			let vals: [u128; 2] = unsafe { std::mem::transmute(value) };
			vals.into()
		}
	}

	impl From<[u128; 2]> for ByteData {
		fn from(value: [u128; 2]) -> Self {
			unsafe { std::mem::transmute(value) }
		}
	}

	impl Shl<usize> for ByteData {
		type Output = Self;

		fn shl(self, rhs: usize) -> Self::Output {
			let mut result = Self::default();
			for i in 0..256 {
				if i >= rhs {
					result.set_bit(i, self.get_bit(i - rhs));
				}
			}

			result
		}
	}

	impl Shr<usize> for ByteData {
		type Output = Self;

		fn shr(self, rhs: usize) -> Self::Output {
			let mut result = Self::default();
			for i in 0..256 {
				if i + rhs < 256 {
					result.set_bit(i, self.get_bit(i + rhs));
				}
			}

			result
		}
	}

	proptest! {
		#[test]
		fn test_conversion(a in any::<u128>(), b in any::<u128>()) {
			check_roundtrip::<[u128; 2]>([a, b].into());
			check_roundtrip::<__m256i>([a, b].into());
		}

		#[test]
		fn test_binary_bit_operations([a, b, c, d] in any::<[u128;4]>()) {
			assert_eq!(M256::from([a & b, c & d]), M256::from([a, c]) & M256::from([b, d]));
			assert_eq!(M256::from([a | b, c | d]), M256::from([a, c]) | M256::from([b, d]));
			assert_eq!(M256::from([a ^ b, c ^ d]), M256::from([a, c]) ^ M256::from([b, d]));
		}

		#[test]
		fn test_negate(a in any::<u128>(), b in any::<u128>()) {
			assert_eq!(M256::from([!a, ! b]), !M256::from([a, b]))
		}

		#[test]
		fn test_shifts(a in any::<[u128; 2]>(), rhs in 0..255usize) {
			assert_eq!(M256::from(a) << rhs, M256::from(ByteData::from(a) << rhs));
			assert_eq!(M256::from(a) >> rhs, M256::from(ByteData::from(a) >> rhs));
		}
	}

	#[test]
	fn test_fill_with_bit() {
		assert_eq!(M256::fill_with_bit(1), M256::from([u128::MAX, u128::MAX]));
		assert_eq!(M256::fill_with_bit(0), M256::from(0u128));
	}

	#[test]
	fn test_eq() {
		let a = M256::from(0u128);
		let b = M256::from(42u128);
		let c = M256::from(u128::MAX);
		let d = M256::from([u128::MAX, u128::MAX]);

		assert_eq!(a, a);
		assert_eq!(b, b);
		assert_eq!(c, c);
		assert_eq!(d, d);

		assert_ne!(a, b);
		assert_ne!(a, c);
		assert_ne!(a, d);
		assert_ne!(b, c);
		assert_ne!(b, d);
		assert_ne!(c, d);
	}
}
