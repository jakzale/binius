// Copyright 2024 Ulvetanna Inc.

use super::simd_arithmetic::TowerSimdType;
use crate::{
	affine_transformation::{FieldAffineTransformation, Transformation},
	arch::{portable::packed::PackedPrimitiveType, GfniStrategy},
	arithmetic_traits::{TaggedInvertOrZero, TaggedMul},
	packed::PackedBinaryField,
	underlier::{UnderlierType, WithUnderlier},
	BinaryField, BinaryField16b, BinaryField32b, BinaryField64b, BinaryField8b, PackedField,
};
use std::{array, ops::Deref};

#[rustfmt::skip]
const TOWER_TO_AES_MAP: i64 = u64::from_le_bytes([
	0b00111110,
	0b10011000,
	0b01001110,
	0b10010110,
	0b11101010,
	0b01101010,
	0b01010000,
	0b00110001,
]) as i64;

#[rustfmt::skip]
const AES_TO_TOWER_MAP: i64 = u64::from_le_bytes([
	0b00001100,
	0b01110000,
	0b10100010,
	0b01110010,
	0b00111110,
	0b10000110,
	0b11101000,
	0b11010001,
]) as i64;

#[rustfmt::skip]
pub const IDENTITY_MAP: i64 = u64::from_le_bytes([
	0b10000000,
	0b01000000,
	0b00100000,
	0b00010000,
	0b00001000,
	0b00000100,
	0b00000010,
	0b00000001,
]) as i64;

pub type GfniBinaryTowerStrategy = GfniStrategy<TOWER_TO_AES_MAP, AES_TO_TOWER_MAP>;
pub type GfniAESTowerStrategy = GfniStrategy<IDENTITY_MAP, IDENTITY_MAP>;

pub(super) trait GfniType: Copy + TowerSimdType {
	fn gf2p8affine_epi64_epi8(x: Self, a: Self) -> Self;
	fn gf2p8mul_epi8(a: Self, b: Self) -> Self;
	fn gf2p8affineinv_epi64_epi8(x: Self, a: Self) -> Self;
}

#[inline(always)]
fn affine_transform<T: GfniType>(x: T, map: i64) -> T {
	let map = T::set_epi_64(map);
	T::gf2p8affine_epi64_epi8(x, map)
}

impl<
		const TO_AES_MAP: i64,
		const FROM_AES_MAP: i64,
		U: GfniType + UnderlierType,
		Scalar: BinaryField,
	> TaggedMul<GfniStrategy<TO_AES_MAP, FROM_AES_MAP>> for PackedPrimitiveType<U, Scalar>
{
	fn mul(self, rhs: Self) -> Self {
		let (lhs_gfni, rhs_gfni) = if TO_AES_MAP != IDENTITY_MAP {
			(
				affine_transform(self.to_underlier(), TO_AES_MAP),
				affine_transform(rhs.to_underlier(), TO_AES_MAP),
			)
		} else {
			(self.to_underlier(), rhs.to_underlier())
		};

		let prod_gfni = U::gf2p8mul_epi8(lhs_gfni, rhs_gfni);

		let prod_gfni = if FROM_AES_MAP != IDENTITY_MAP {
			affine_transform(prod_gfni, FROM_AES_MAP)
		} else {
			prod_gfni
		};

		prod_gfni.into()
	}
}

impl<
		const TO_AES_MAP: i64,
		const FROM_AES_MAP: i64,
		U: GfniType + UnderlierType,
		Scalar: BinaryField,
	> TaggedInvertOrZero<GfniStrategy<TO_AES_MAP, FROM_AES_MAP>> for PackedPrimitiveType<U, Scalar>
{
	fn invert_or_zero(self) -> Self {
		let val_gfni = if TO_AES_MAP != IDENTITY_MAP {
			affine_transform(self.to_underlier(), TO_AES_MAP)
		} else {
			self.to_underlier()
		};

		// Calculate inversion and affine transformation to the original field with a single instruction
		let identity = U::set_epi_64(FROM_AES_MAP);
		let inv_gfni = U::gf2p8affineinv_epi64_epi8(val_gfni, identity);

		inv_gfni.into()
	}
}

/// Transformation that uses `gf2p8affine_epi64_epi8` transformation to apply affine transformation to a
/// 8-bit packed field. It appeared that this dedicated implementation is more efficient than `GfniTransformationNxN<_, 1>`.
pub(super) struct GfniTransformation<OP>
where
	OP: WithUnderlier<Underlier: GfniType>,
{
	/// Value is filled with 64-bit affine transformation matrices
	bases_8x8: OP::Underlier,
}

/// Transpose i64 representing a 8x8 boolean matrix.
/// There may be a faster implementation for this but
/// it is used only during packed transformation creation, not at transformation itself.
fn transpose_8x8(mut matrix: i64) -> i64 {
	let mut result = 0;

	for i in 0..8 {
		for j in 0..8 {
			result |= (matrix & 1) << ((7 - j) * 8 + i);
			matrix >>= 1;
		}
	}

	result
}

impl<OP> GfniTransformation<OP>
where
	OP: WithUnderlier<Underlier: GfniType>
		+ PackedBinaryField<Scalar: WithUnderlier<Underlier = u8>>,
{
	pub(super) fn new<Data: Deref<Target = [OP::Scalar]>>(
		transformation: FieldAffineTransformation<OP::Scalar, Data>,
	) -> Self {
		debug_assert_eq!(OP::Scalar::N_BITS, 8);
		debug_assert_eq!(transformation.bases().len(), 8);
		let bases_8x8 =
			i64::from_le_bytes(array::from_fn(|i| transformation.bases()[i].to_underlier()));

		Self {
			bases_8x8: OP::Underlier::set_epi_64(transpose_8x8(bases_8x8)),
		}
	}
}

impl<IP, OP, U> Transformation<IP, OP> for GfniTransformation<OP>
where
	IP: PackedField + WithUnderlier<Underlier = U>,
	OP: PackedField + WithUnderlier<Underlier = U>,
	U: GfniType,
{
	fn transform(&self, data: &IP) -> OP {
		U::gf2p8affine_epi64_epi8(data.to_underlier(), self.bases_8x8).into()
	}
}

/// Implement packed transformation factory with GFNI instructions for 8-bit packed field
macro_rules! impl_transformation_with_gfni {
	($name:ty, $strategy:ty) => {
		impl<OP> $crate::affine_transformation::PackedTransformationFactory<OP> for $name
		where
			OP: $crate::packed::PackedBinaryField<
					Scalar: $crate::underlier::WithUnderlier<Underlier = u8>,
				> + $crate::underlier::WithUnderlier<
					Underlier = <$name as $crate::underlier::WithUnderlier>::Underlier,
				>,
		{
			fn make_packed_transformation<Data: std::ops::Deref<Target = [OP::Scalar]>>(
				transformation: $crate::affine_transformation::FieldAffineTransformation<
					OP::Scalar,
					Data,
				>,
			) -> impl $crate::affine_transformation::Transformation<Self, OP> {
				$crate::arch::x86_64::gfni::gfni_arithmetics::GfniTransformation::new(
					transformation,
				)
			}
		}
	};
}

pub(crate) use impl_transformation_with_gfni;

/// Value that can be converted to a little-endian byte array
pub(super) trait ToLEBytes<const N: usize> {
	fn to_le_bytes(self) -> [u8; N];
}

impl ToLEBytes<1> for u8 {
	fn to_le_bytes(self) -> [u8; 1] {
		self.to_le_bytes()
	}
}

impl ToLEBytes<2> for u16 {
	fn to_le_bytes(self) -> [u8; 2] {
		self.to_le_bytes()
	}
}

impl ToLEBytes<4> for u32 {
	fn to_le_bytes(self) -> [u8; 4] {
		self.to_le_bytes()
	}
}

impl ToLEBytes<8> for u64 {
	fn to_le_bytes(self) -> [u8; 8] {
		self.to_le_bytes()
	}
}

/// Affine transformation for packed scalars of size `BLOCKS*8`.
/// Splits elements itself and transformation matrix to 8-bit size blocks and uses `gf2p8affine_epi64_epi8`
/// to perform multiplications of those.
/// Transformation complexity is `BLOCKS^2`.
pub(super) struct GfniTransformationNxN<OP, const BLOCKS: usize>
where
	OP: WithUnderlier<Underlier: GfniType>,
{
	bases_8x8: [[OP::Underlier; BLOCKS]; BLOCKS],
}

impl<OP, const BLOCKS: usize> GfniTransformationNxN<OP, BLOCKS>
where
	OP: WithUnderlier<Underlier: GfniType>
		+ PackedBinaryField<Scalar: WithUnderlier<Underlier: ToLEBytes<BLOCKS>>>,
	[[OP::Underlier; BLOCKS]; BLOCKS]: Default,
{
	pub(super) fn new<Data: Deref<Target = [OP::Scalar]>>(
		transformation: FieldAffineTransformation<OP::Scalar, Data>,
	) -> Self {
		debug_assert_eq!(OP::Scalar::N_BITS, BLOCKS * 8);
		debug_assert_eq!(transformation.bases().len(), BLOCKS * 8);

		// Convert bases matrix into `BLOCKS`x`BLOCKS` matrix of 8x8 blocks.
		let mut bases_8x8 = <[[OP::Underlier; BLOCKS]; BLOCKS]>::default();
		for (i, row) in bases_8x8.iter_mut().enumerate() {
			for (j, matr) in row.iter_mut().enumerate() {
				let matrix8x8 = transpose_8x8(i64::from_le_bytes(array::from_fn(|k| {
					transformation.bases()[k + 8 * i]
						.to_underlier()
						.to_le_bytes()[j]
				})));
				*matr = OP::Underlier::set_epi_64(matrix8x8);
			}
		}

		Self { bases_8x8 }
	}
}

impl<IP, OP, U, const BLOCKS: usize> Transformation<IP, OP> for GfniTransformationNxN<OP, BLOCKS>
where
	IP: PackedField + WithUnderlier<Underlier = U>,
	OP: PackedField + WithUnderlier<Underlier = U>,
	U: GfniType + TowerSimdType + std::fmt::Debug,
{
	fn transform(&self, data: &IP) -> OP {
		let packed_values: [OP::Underlier; BLOCKS] = array::from_fn(|i| {
			(0..BLOCKS)
				.map(|j| {
					// move meaningful value to the `i` index
					shift_bytes(
						U::gf2p8affine_epi64_epi8(data.to_underlier(), self.bases_8x8[j][i]),
						i as i32 - j as i32,
					)
				})
				.reduce(U::xor)
				.expect("collection is never empty")
		});

		// Put `i`'s component of each value to the result
		blend_values(&packed_values).into()
	}
}

/// Shift `value` by `count` bytes, where `count` can be negative.
/// Positive `count` value corresponds to a left shift, negative - to the right.
#[inline(always)]
fn shift_bytes<T: GfniType>(value: T, count: i32) -> T {
	match count {
		0 => value,
		1 => value.bslli_epi128::<1>(),
		2 => value.bslli_epi128::<2>(),
		3 => value.bslli_epi128::<3>(),
		4 => value.bslli_epi128::<4>(),
		5 => value.bslli_epi128::<5>(),
		6 => value.bslli_epi128::<6>(),
		7 => value.bslli_epi128::<7>(),
		8 => value.bslli_epi128::<8>(),
		-1 => value.bsrli_epi128::<1>(),
		-2 => value.bsrli_epi128::<2>(),
		-3 => value.bsrli_epi128::<3>(),
		-4 => value.bsrli_epi128::<4>(),
		-5 => value.bsrli_epi128::<5>(),
		-6 => value.bsrli_epi128::<6>(),
		-7 => value.bsrli_epi128::<7>(),
		-8 => value.bsrli_epi128::<8>(),
		_ => panic!("unsupported byte shift"),
	}
}

/// Creates a packed value where
/// - components at index `0` are from `values[0]`
/// - components at index `1` are from `values[1]`
/// ...
/// - components at index `values.len() - 1` are from `values[values.len() - 1]`
#[inline(always)]
fn blend_values<T: TowerSimdType>(values: &[T]) -> T {
	match values.len() {
		1 => values[0],
		2 => T::blend_odd_even::<BinaryField8b>(values[1], values[0]),
		4 => T::blend_odd_even::<BinaryField16b>(
			blend_values(&values[2..4]),
			blend_values(&values[0..2]),
		),
		8 => T::blend_odd_even::<BinaryField32b>(
			blend_values(&values[4..8]),
			blend_values(&values[0..4]),
		),
		16 => T::blend_odd_even::<BinaryField64b>(
			blend_values(&values[8..16]),
			blend_values(&values[0..8]),
		),
		_ => panic!("unsupported height"),
	}
}

/// Implement packed transformation factory with GFNI instructions for scalars bigger than 8 bits
macro_rules! impl_transformation_with_gfni_nxn {
	($name:ty, $blocks:literal) => {
		impl<OP> $crate::affine_transformation::PackedTransformationFactory<OP> for $name where OP: $crate::packed::PackedBinaryField<Scalar: $crate::underlier::WithUnderlier<Underlier: $crate::arch::x86_64::gfni::gfni_arithmetics::ToLEBytes<$blocks>>> + $crate::underlier::WithUnderlier<Underlier = <$name as $crate::underlier::WithUnderlier>::Underlier> {
			fn make_packed_transformation<Data: std::ops::Deref<Target = [OP::Scalar]>>(transformation: $crate::affine_transformation::FieldAffineTransformation<OP::Scalar, Data>) -> impl $crate::affine_transformation::Transformation<Self, OP> {
				$crate::arch::x86_64::gfni::gfni_arithmetics::GfniTransformationNxN::<OP, $blocks>::new(transformation)
			}
		}
	};
}

pub(crate) use impl_transformation_with_gfni_nxn;

#[cfg(target_feature = "sse2")]
mod impl_128 {
	use super::*;
	use crate::arch::x86_64::m128::M128;
	use core::arch::x86_64::*;

	impl GfniType for M128 {
		#[inline(always)]
		fn gf2p8affine_epi64_epi8(x: Self, a: Self) -> Self {
			unsafe { _mm_gf2p8affine_epi64_epi8::<0>(x.0, a.0) }.into()
		}

		#[inline(always)]
		fn gf2p8mul_epi8(a: Self, b: Self) -> Self {
			unsafe { _mm_gf2p8mul_epi8(a.0, b.0) }.into()
		}

		#[inline(always)]
		fn gf2p8affineinv_epi64_epi8(x: Self, a: Self) -> Self {
			unsafe { _mm_gf2p8affineinv_epi64_epi8::<0>(x.0, a.0) }.into()
		}
	}
}

#[cfg(target_feature = "avx2")]
mod impl_256 {
	use super::*;
	use crate::arch::x86_64::m256::M256;
	use core::arch::x86_64::*;

	impl GfniType for M256 {
		#[inline(always)]
		fn gf2p8affine_epi64_epi8(x: Self, a: Self) -> Self {
			unsafe { _mm256_gf2p8affine_epi64_epi8::<0>(x.0, a.0) }.into()
		}

		#[inline(always)]
		fn gf2p8mul_epi8(a: Self, b: Self) -> Self {
			unsafe { _mm256_gf2p8mul_epi8(a.0, b.0) }.into()
		}

		#[inline(always)]
		fn gf2p8affineinv_epi64_epi8(x: Self, a: Self) -> Self {
			unsafe { _mm256_gf2p8affineinv_epi64_epi8::<0>(x.0, a.0) }.into()
		}
	}
}

#[cfg(target_feature = "avx512f")]
mod impl_512 {
	use super::*;
	use crate::arch::x86_64::m512::M512;
	use core::arch::x86_64::*;

	impl GfniType for M512 {
		#[inline(always)]
		fn gf2p8affine_epi64_epi8(x: Self, a: Self) -> Self {
			unsafe { _mm512_gf2p8affine_epi64_epi8::<0>(x.0, a.0) }.into()
		}

		#[inline(always)]
		fn gf2p8mul_epi8(a: Self, b: Self) -> Self {
			unsafe { _mm512_gf2p8mul_epi8(a.0, b.0) }.into()
		}

		#[inline(always)]
		fn gf2p8affineinv_epi64_epi8(x: Self, a: Self) -> Self {
			unsafe { _mm512_gf2p8affineinv_epi64_epi8::<0>(x.0, a.0) }.into()
		}
	}
}
