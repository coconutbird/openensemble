//! Contact materials and the Havok 8-bit float (`hkUFloat8`).
//!
//! Retail mixes the materials of two bodies when the collision detection adds a contact point
//! (`hkpSimpleConstraintContactMgr_mixMaterial`, `0x14088CF40`). The friction is stored as an
//! `hkUFloat8` and the restitution as a byte in 1/128 steps.

/// The decode table of `hkUFloat8` (`hkUFloat8_intToReal`, Havok `hkBaseTypes.cpp:11-29`).
///
/// The first eight entries rise in steps of 0.01. The other entries rise exponentially to 1e6.
pub(crate) const UFLOAT8_TABLE: [f32; 256] = [
    0.000,
    0.010,
    0.020,
    0.030,
    0.040,
    0.050,
    0.060,
    0.070,
    0.075,
    0.080,
    0.085,
    0.091,
    0.098,
    0.104,
    0.111,
    0.119,
    0.127,
    0.136,
    0.145,
    0.155,
    0.166,
    0.177,
    0.190,
    0.203,
    0.217,
    0.231,
    0.247,
    0.264,
    0.282,
    0.302,
    0.323,
    0.345,
    0.368,
    0.394,
    0.421,
    0.450,
    0.481,
    0.514,
    0.549,
    0.587,
    0.627,
    0.670,
    0.716,
    0.765,
    0.818,
    0.874,
    0.934,
    0.998,
    1.067,
    1.140,
    1.218,
    1.302,
    1.391,
    1.487,
    1.589,
    1.698,
    1.815,
    1.939,
    2.072,
    2.215,
    2.367,
    2.529,
    2.703,
    2.889,
    3.087,
    3.299,
    3.526,
    3.768,
    4.027,
    4.304,
    4.599,
    4.915,
    5.253,
    5.613,
    5.999,
    6.411,
    6.851,
    7.322,
    7.825,
    8.362,
    8.937,
    9.551,
    10.21,
    10.91,
    11.66,
    12.46,
    13.31,
    14.23,
    15.20,
    16.25,
    17.37,
    18.56,
    19.83,
    21.20,
    22.65,
    24.21,
    25.87,
    27.65,
    29.55,
    31.57,
    33.74,
    36.06,
    38.54,
    41.18,
    44.01,
    47.04,
    50.27,
    53.72,
    57.41,
    61.35,
    65.57,
    70.07,
    74.88,
    80.03,
    85.52,
    91.40,
    97.68,
    104.4,
    111.6,
    119.2,
    127.4,
    136.2,
    145.5,
    155.5,
    166.2,
    177.6,
    189.8,
    202.8,
    216.8,
    231.7,
    247.6,
    264.6,
    282.7,
    302.2,
    322.9,
    345.1,
    368.8,
    394.1,
    421.2,
    450.1,
    481.1,
    514.1,
    549.4,
    587.2,
    627.5,
    670.6,
    716.6,
    765.9,
    818.5,
    874.7,
    934.8,
    999.0,
    1068.0,
    1141.0,
    1219.0,
    1303.0,
    1393.0,
    1488.0,
    1590.0,
    1700.0,
    1816.0,
    1941.0,
    2074.0,
    2217.0,
    2369.0,
    2532.0,
    2706.0,
    2892.0,
    3090.0,
    3303.0,
    3530.0,
    3772.0,
    4031.0,
    4308.0,
    4604.0,
    4920.0,
    5258.0,
    5619.0,
    6005.0,
    6418.0,
    6858.0,
    7329.0,
    7833.0,
    8371.0,
    8946.0,
    9560.0,
    10217.0,
    10919.0,
    11669.0,
    12470.0,
    13327.0,
    14242.0,
    15220.0,
    16266.0,
    17383.0,
    18577.0,
    19853.0,
    21217.0,
    22674.0,
    24231.0,
    25896.0,
    27674.0,
    29575.0,
    31607.0,
    33778.0,
    36098.0,
    38577.0,
    41227.0,
    44059.0,
    47085.0,
    50319.0,
    53775.0,
    57469.0,
    61416.0,
    65635.0,
    70143.0,
    74961.0,
    80110.0,
    85612.0,
    91493.0,
    97777.0,
    104_493.0,
    111_670.0,
    119_340.0,
    127_538.0,
    136_298.0,
    145_660.0,
    155_664.0,
    166_356.0,
    177_783.0,
    189_994.0,
    203_044.0,
    216_991.0,
    231_895.0,
    247_823.0,
    264_846.0,
    283_037.0,
    302_478.0,
    323_254.0,
    345_457.0,
    369_186.0,
    394_544.0,
    421_644.0,
    450_605.0,
    481_556.0,
    514_632.0,
    549_980.0,
    587_757.0,
    628_128.0,
    671_272.0,
    717_379.0,
    766_654.0,
    819_313.0,
    875_589.0,
    935_730.0,
    1_000_002.0,
];

/// Encodes a non-negative float as an `hkUFloat8` code (`hkUFloat8::operator=`).
///
/// Havok runs a binary search of seven steps over the table and then takes the upper
/// neighbour if the value is above the middle entry. The result is the first entry at or above
/// the value, except that 0 gives code 1 (0.01). A value above the last entry wraps to code 0,
/// like the `hkUint8` store of index 256.
#[must_use]
pub(crate) fn encode_ufloat8(value: f32) -> u8 {
    // Havok compares the float bits as signed integers. For non-negative floats this is the
    // same order as the float order.
    let bits = value.to_bits().cast_signed();
    let key = |index: usize| UFLOAT8_TABLE[index].to_bits().cast_signed();
    let mut min = 0_usize;
    let mut max = UFLOAT8_TABLE.len();
    let mut mid = UFLOAT8_TABLE.len() / 2;
    for _ in 0..7 {
        if bits > key(mid) {
            min = mid;
        } else {
            max = mid;
        }
        mid = usize::midpoint(min, max);
    }
    if bits > key(mid) {
        mid += 1;
    }
    u8::try_from(mid & 0xff).unwrap_or(0)
}

/// Decodes an `hkUFloat8` code.
#[must_use]
pub(crate) fn decode_ufloat8(code: u8) -> f32 {
    UFLOAT8_TABLE[usize::from(code)]
}

/// Mixes two frictions: `sqrt(fB · fA)`, stored as an `hkUFloat8` code.
#[must_use]
pub(crate) fn mix_friction(friction_a: f32, friction_b: f32) -> u8 {
    encode_ufloat8((friction_b * friction_a).sqrt())
}

/// Mixes two restitutions: `(u8)(sqrt(rB · rA) · 128)`, truncated toward zero.
///
/// Retail converts the product to an `int` and stores the low byte.
#[must_use]
pub(crate) fn mix_restitution(restitution_a: f32, restitution_b: f32) -> u8 {
    let scaled = (restitution_b * restitution_a).sqrt() * 128.0;
    if !scaled.is_finite() || scaled <= 0.0 || scaled >= 2_147_483_648.0 {
        // `cvttss2si` gives `i32::MIN` out of range, and its low byte is 0.
        return 0;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Rust has no float to integer conversion without `as`; the value is in 0..2^31"
    )]
    let truncated = scaled as u32;
    u8::try_from(truncated & 0xff).unwrap_or(0)
}

/// Decodes a restitution byte (`code / 128`).
#[must_use]
pub(crate) fn decode_restitution(code: u8) -> f32 {
    f32::from(code) * 0.007_812_5
}

#[cfg(test)]
mod tests {
    use super::{
        decode_restitution, decode_ufloat8, encode_ufloat8, mix_friction, mix_restitution,
    };

    #[test]
    fn encoding_rounds_up_to_the_next_table_entry() {
        assert_eq!(encode_ufloat8(0.0), 1, "zero encodes as the first step");
        assert_eq!(encode_ufloat8(0.01), 1);
        assert_eq!(encode_ufloat8(0.5), 37);
        assert!((decode_ufloat8(encode_ufloat8(0.5)) - 0.514).abs() < 1e-6);
        assert!((decode_ufloat8(encode_ufloat8(200.0)) - 202.8).abs() < 1e-3);
        assert!((decode_ufloat8(encode_ufloat8(1.0)) - 1.067).abs() < 1e-6);
    }

    #[test]
    fn terrain_material_mixes_like_retail() {
        // Terrain friction 1.9 against a blueprint friction of 2: sqrt(3.8) = 1.949.
        let friction = decode_ufloat8(mix_friction(2.0, 1.9));
        assert!((friction - 2.072).abs() < 1e-6, "{friction}");
        // Terrain restitution 1.0 against 0.5: sqrt(0.5) * 128 = 90.5, truncated to 90.
        assert_eq!(mix_restitution(0.5, 1.0), 90);
        assert!((decode_restitution(90) - 0.703_125).abs() < 1e-7);
        assert_eq!(mix_restitution(0.0, 1.0), 0);
    }
}
