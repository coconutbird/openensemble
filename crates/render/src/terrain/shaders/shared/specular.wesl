// Shared reciprocal specular response from the freshly decompiled PC shaders.

/// Evaluate `response / (power - power * response + response)`.
///
/// `response` is the saturated dot product between the reflected camera ray
/// and the light direction. Invalid reciprocal results in the PC shader become
/// zero; the denominator guard provides the same finite result here.
fn specular_schlick(response: f32, power: f32) -> f32 {
    let denominator = power - power * response + response;
    if denominator < 0.0001 {
        return 0.0;
    }
    return response / denominator;
}
