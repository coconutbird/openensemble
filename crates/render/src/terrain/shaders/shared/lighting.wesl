// Shared SH fill lighting - matches computeSHFillLighting in shFillLighting.inc
//
// Second-order spherical harmonics irradiance evaluation.
// All SH coefficient vectors are passed as parameters so this function
// is independent of any particular uniform layout.

fn compute_sh_fill_lighting(
    n: vec3<f32>,
    sh_ar: vec4<f32>, sh_ag: vec4<f32>, sh_ab: vec4<f32>,
    sh_br: vec4<f32>, sh_bg: vec4<f32>, sh_bb: vec4<f32>,
    sh_c: vec4<f32>,
) -> vec3<f32> {
    let normal4 = vec4<f32>(n, 1.0);

    // Linear terms
    var linear_color: vec3<f32>;
    linear_color.x = dot(normal4, sh_ar);
    linear_color.y = dot(normal4, sh_ag);
    linear_color.z = dot(normal4, sh_ab);

    // First quadratic terms: r2 = normal.xyzz * normal.yzzx
    let r2 = vec4<f32>(n.x * n.y, n.y * n.z, n.z * n.z, n.z * n.x);

    var first_quad: vec3<f32>;
    first_quad.x = dot(r2, sh_br);
    first_quad.y = dot(r2, sh_bg);
    first_quad.z = dot(r2, sh_bb);

    // Second quadratic term: c * (nx^2 - ny^2)
    return linear_color + first_quad + sh_c.xyz * (n.x * n.x - n.y * n.y);
}
