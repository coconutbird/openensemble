// Shared TBN and normal helpers
//
// TBN reconstruction from a normal vector, and DXN/BC5 normal map unpacking.
// Matches GiveTBNFromNormal and DXN unpacking in the original HLSL.

/// Reconstruct TBN matrix from a normal vector.
/// Tangent and binormal are derived assuming the normal is mostly Y-up.
/// Returns mat3x3 where [0]=tangent, [1]=binormal, [2]=normal.
fn compute_tbn(normal: vec3<f32>) -> mat3x3<f32> {
    let binormal = normalize(vec3<f32>(0.0, normal.z, -normal.y));
    let tangent = cross(binormal, normal);
    return mat3x3<f32>(tangent, binormal, normal);
}

/// Unpack a DXN/BC5-encoded normal map sample to a unit normal.
/// DXN stores X and Y in .x and .y (0..1 range), Z is reconstructed.
fn unpack_dxn_normal(s: vec4<f32>) -> vec3<f32> {
    let nx = s.x * 2.0 - 1.0;
    let ny = s.y * 2.0 - 1.0;
    return vec3<f32>(nx, ny, sqrt(max(0.0, 1.0 - nx * nx - ny * ny)));
}
