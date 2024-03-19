// Copyright 2024 Ulvetanna Inc.

use super::{super::m128::M128, simd_arithmetic::SimdStrategy};
use crate::field::{
	aes_field::{
		AESTowerField128b, AESTowerField16b, AESTowerField32b, AESTowerField64b, AESTowerField8b,
	},
	arch::{
		portable::{
			packed::{
				impl_conversion, impl_packed_extension_field, packed_binary_field_tower,
				PackedPrimitiveType,
			},
			pairwise_arithmetic::PairwiseStrategy,
		},
		PackedStrategy,
	},
	arithmetic_traits::{
		impl_invert_with_strategy, impl_mul_alpha_with_strategy, impl_mul_with_strategy,
		impl_square_with_strategy,
	},
};
use std::{arch::x86_64::*, ops::Mul};

// Define 128 bit packed field types
pub type PackedAESBinaryField16x8b = PackedPrimitiveType<M128, AESTowerField8b>;
pub type PackedAESBinaryField8x16b = PackedPrimitiveType<M128, AESTowerField16b>;
pub type PackedAESBinaryField4x32b = PackedPrimitiveType<M128, AESTowerField32b>;
pub type PackedAESBinaryField2x64b = PackedPrimitiveType<M128, AESTowerField64b>;
pub type PackedAESBinaryField1x128b = PackedPrimitiveType<M128, AESTowerField128b>;

// Define conversion from type to underlier;
impl_conversion!(M128, PackedAESBinaryField16x8b);
impl_conversion!(M128, PackedAESBinaryField8x16b);
impl_conversion!(M128, PackedAESBinaryField4x32b);
impl_conversion!(M128, PackedAESBinaryField2x64b);
impl_conversion!(M128, PackedAESBinaryField1x128b);

// Define tower
packed_binary_field_tower!(
	PackedAESBinaryField16x8b
	< PackedAESBinaryField8x16b
	< PackedAESBinaryField4x32b
	< PackedAESBinaryField2x64b
	< PackedAESBinaryField1x128b
);

// Define extension fields
impl_packed_extension_field!(PackedAESBinaryField16x8b);
impl_packed_extension_field!(PackedAESBinaryField8x16b);
impl_packed_extension_field!(PackedAESBinaryField4x32b);
impl_packed_extension_field!(PackedAESBinaryField2x64b);
impl_packed_extension_field!(PackedAESBinaryField1x128b);

// Define multiplication
impl_mul_with_strategy!(PackedAESBinaryField8x16b, SimdStrategy);
impl_mul_with_strategy!(PackedAESBinaryField4x32b, SimdStrategy);
impl_mul_with_strategy!(PackedAESBinaryField2x64b, SimdStrategy);
impl_mul_with_strategy!(PackedAESBinaryField1x128b, SimdStrategy);

impl Mul for PackedAESBinaryField16x8b {
	type Output = Self;

	fn mul(self, rhs: Self) -> Self {
		unsafe { M128::from(_mm_gf2p8mul_epi8(self.0.into(), rhs.0.into())).into() }
	}
}

// TODO: use more optimal SIMD implementation
// Define square
impl_square_with_strategy!(PackedAESBinaryField16x8b, PairwiseStrategy);
impl_square_with_strategy!(PackedAESBinaryField8x16b, PackedStrategy);
impl_square_with_strategy!(PackedAESBinaryField4x32b, PackedStrategy);
impl_square_with_strategy!(PackedAESBinaryField2x64b, PackedStrategy);
impl_square_with_strategy!(PackedAESBinaryField1x128b, PackedStrategy);

// TODO: use more optimal SIMD implementation
// Define invert
impl_invert_with_strategy!(PackedAESBinaryField16x8b, PairwiseStrategy);
impl_invert_with_strategy!(PackedAESBinaryField8x16b, PairwiseStrategy);
impl_invert_with_strategy!(PackedAESBinaryField4x32b, PairwiseStrategy);
impl_invert_with_strategy!(PackedAESBinaryField2x64b, PairwiseStrategy);
impl_invert_with_strategy!(PackedAESBinaryField1x128b, PairwiseStrategy);

// TODO: use more optimal SIMD implementation
// Define multiply by alpha
impl_mul_alpha_with_strategy!(PackedAESBinaryField16x8b, PairwiseStrategy);
impl_mul_alpha_with_strategy!(PackedAESBinaryField8x16b, PairwiseStrategy);
impl_mul_alpha_with_strategy!(PackedAESBinaryField4x32b, PairwiseStrategy);
impl_mul_alpha_with_strategy!(PackedAESBinaryField2x64b, PairwiseStrategy);
impl_mul_alpha_with_strategy!(PackedAESBinaryField1x128b, PairwiseStrategy);
