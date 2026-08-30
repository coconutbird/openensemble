//! Renderer-side local-light payloads.
//!
//! The PC shaders consume at most twenty lights. Each light occupies thirty
//! meaningful `f32` values in an eight-`vec4` (128-byte) storage-buffer slot;
//! the final two values are padding. This module owns that ABI so terrain,
//! foliage, roads, particles, and UGX models can share one uploaded light set.

use glam::{Mat4, Vec2, Vec3, Vec4};
use num_traits::ToPrimitive;
use wgpu::util::DeviceExt;

use crate::terrain::LightingParams;

/// Maximum number of local lights evaluated by the shipped PC shaders.
pub const MAX_LOCAL_LIGHTS: usize = 20;

/// Camera state used to select and screen-fade visible local lights.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalLightView {
    /// Combined world-to-clip transform.
    pub view_projection: Mat4,
    /// World-to-view transform.
    pub world_to_view: Mat4,
    /// World-space camera position used for influence ordering.
    pub camera_position: Vec3,
    /// Target width and height in pixels.
    pub viewport_size: [u32; 2],
}

impl LocalLightView {
    /// Creates a checked projection descriptor for one rendered view.
    #[must_use]
    pub const fn new(
        view_projection: Mat4,
        world_to_view: Mat4,
        camera_position: Vec3,
        viewport_size: [u32; 2],
    ) -> Self {
        Self {
            view_projection,
            world_to_view,
            camera_position,
            viewport_size,
        }
    }

    /// Projects a world-space sphere to `[center_x, center_y, radius]` pixels.
    ///
    /// The retail renderer uses a low-accuracy tangent estimate here. This
    /// stable projection preserves its 15–30 pixel fade policy without copying
    /// the estimate's close-camera numerical defects.
    #[must_use]
    pub fn project_sphere(self, position: [f32; 3], radius: f32) -> Option<[f32; 3]> {
        let width = self.viewport_size[0].to_f32()?;
        let height = self.viewport_size[1].to_f32()?;
        if width <= 0.0 || height <= 0.0 || radius <= 0.0 || !radius.is_finite() {
            return None;
        }
        let center_view = self
            .world_to_view
            .transform_point3(Vec3::from_array(position));
        if !center_view.is_finite() {
            return None;
        }
        if center_view.length_squared() <= radius * radius {
            return Some([width * 0.5, height * 0.5, f32::INFINITY]);
        }
        let inverse_view = self.world_to_view.inverse();
        if !inverse_view.is_finite() {
            return None;
        }
        let projection = self.view_projection * inverse_view;
        let center = project_view_point(projection, center_view, width, height)?;
        let horizontal =
            project_view_point(projection, center_view + Vec3::X * radius, width, height)?;
        let vertical =
            project_view_point(projection, center_view + Vec3::Y * radius, width, height)?;
        let screen_radius = center.distance(horizontal).max(center.distance(vertical));
        if !screen_radius.is_finite()
            || center.x + screen_radius < 0.0
            || center.x - screen_radius > width
            || center.y + screen_radius < 0.0
            || center.y - screen_radius > height
        {
            None
        } else {
            Some([center.x, center.y, screen_radius])
        }
    }

    /// Tests the retail capped-cone proxy against the homogeneous view frustum.
    ///
    /// The retail renderer performs this narrower test after the light's
    /// bounding sphere succeeds, preventing an off-screen spot cone from
    /// consuming a direct-light or shadow slot.
    #[must_use]
    pub fn capped_cone_visible(
        self,
        position: [f32; 3],
        direction: [f32; 3],
        outer_cos: f32,
        radius: f32,
    ) -> bool {
        let Some(cone) = CappedCone::new(position, direction, outer_cos, radius) else {
            return false;
        };
        let points = cone.culling_points();
        let clips = points.map(|point| self.view_projection * point.extend(1.0));
        if clips.iter().any(|clip| !clip.is_finite()) {
            return false;
        }
        let outside = |distance: fn(Vec4) -> f32| clips.iter().all(|clip| distance(*clip) < 0.0);
        !outside(|clip| clip.x + clip.w)
            && !outside(|clip| clip.w - clip.x)
            && !outside(|clip| clip.y + clip.w)
            && !outside(|clip| clip.w - clip.y)
            && !outside(|clip| clip.z)
            && !outside(|clip| clip.w - clip.z)
    }

    /// Projects the retail capped-cone OBB and returns its equal-area circle radius.
    ///
    /// `None` means the proxy cannot be projected safely, in which case retail
    /// retains the bounding-sphere radius rather than dropping the light.
    #[must_use]
    pub fn project_capped_cone_radius(
        self,
        position: [f32; 3],
        direction: [f32; 3],
        outer_cos: f32,
        radius: f32,
    ) -> Option<f32> {
        let width = self.viewport_size[0].to_f32()?;
        let height = self.viewport_size[1].to_f32()?;
        let cone = CappedCone::new(position, direction, outer_cos, radius)?;
        if cone.contains_world_point(self.camera_position) {
            return None;
        }
        let points = cone
            .box_points()
            .into_iter()
            .map(|point| project_world_point(self.view_projection, point, width, height))
            .collect::<Option<Vec<_>>>()?;
        let area = convex_hull_area(points);
        let equivalent_radius = (area / std::f32::consts::PI).sqrt();
        (equivalent_radius.is_finite() && equivalent_radius > 0.0).then_some(equivalent_radius)
    }
}

struct CappedCone {
    apex: Vec3,
    axis: Vec3,
    u: Vec3,
    v: Vec3,
    radius: f32,
    sin_half_angle: f32,
    cos_half_angle: f32,
}

impl CappedCone {
    fn new(position: [f32; 3], direction: [f32; 3], outer_cos: f32, radius: f32) -> Option<Self> {
        let apex = Vec3::from_array(position);
        let axis = Vec3::from_array(direction).normalize_or_zero();
        let cos_half_angle = outer_cos.clamp(-1.0, 1.0);
        if !apex.is_finite()
            || !axis.is_finite()
            || axis == Vec3::ZERO
            || !radius.is_finite()
            || radius <= 0.0
            || cos_half_angle <= f32::EPSILON
        {
            return None;
        }
        let candidate = Vec3::Y.cross(axis);
        let u = if candidate.length_squared() > 0.000_012_5 {
            candidate.normalize()
        } else {
            stable_orthogonal(axis)
        };
        let v = axis.cross(u).normalize_or_zero();
        let sin_half_angle = (1.0 - cos_half_angle * cos_half_angle).max(0.0).sqrt();
        (u.is_finite() && v.is_finite() && v != Vec3::ZERO).then_some(Self {
            apex,
            axis,
            u,
            v,
            radius,
            sin_half_angle,
            cos_half_angle,
        })
    }

    fn culling_points(&self) -> [Vec3; 6] {
        let u = stable_orthogonal(self.axis);
        let v = self.axis.cross(u);
        let disk_radius = self.radius * self.sin_half_angle;
        let center = self.apex + self.axis * (self.radius * self.cos_half_angle);
        [
            self.apex,
            center - u * disk_radius - v * disk_radius,
            center + u * disk_radius - v * disk_radius,
            center + u * disk_radius + v * disk_radius,
            center - u * disk_radius + v * disk_radius,
            self.apex + self.axis * (self.radius / self.cos_half_angle),
        ]
    }

    fn box_points(&self) -> [Vec3; 8] {
        let extent = self.radius * self.sin_half_angle;
        let local_to_world =
            |x: f32, y: f32, z: f32| self.apex + self.u * x + self.v * y + self.axis * z;
        [
            local_to_world(-extent, -extent, 0.0),
            local_to_world(extent, -extent, 0.0),
            local_to_world(extent, extent, 0.0),
            local_to_world(-extent, extent, 0.0),
            local_to_world(-extent, -extent, self.radius),
            local_to_world(extent, -extent, self.radius),
            local_to_world(extent, extent, self.radius),
            local_to_world(-extent, extent, self.radius),
        ]
    }

    fn contains_world_point(&self, point: Vec3) -> bool {
        let local = point - self.apex;
        let local = Vec3::new(local.dot(self.u), local.dot(self.v), local.dot(self.axis));
        let extent = self.radius * self.sin_half_angle;
        local.x >= -extent
            && local.x <= extent
            && local.y >= -extent
            && local.y <= extent
            && local.z >= 0.0
            && local.z <= self.radius
    }
}

fn stable_orthogonal(axis: Vec3) -> Vec3 {
    let reference = if axis.x.abs() <= axis.z.abs() {
        Vec3::X
    } else {
        Vec3::Z
    };
    reference.cross(axis).normalize_or_zero()
}

fn project_world_point(projection: Mat4, point: Vec3, width: f32, height: f32) -> Option<Vec2> {
    let clip = projection * point.extend(1.0);
    if !clip.is_finite() || clip.w <= f32::EPSILON {
        return None;
    }
    let normalized = clip.truncate() / clip.w;
    Some(Vec2::new(
        (normalized.x + 1.0) * width * 0.5,
        (1.0 - normalized.y) * height * 0.5,
    ))
}

fn convex_hull_area(mut points: Vec<Vec2>) -> f32 {
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then_with(|| a.y.total_cmp(&b.y)));
    points.dedup();
    if points.len() < 3 {
        return 0.0;
    }
    let mut lower = Vec::with_capacity(points.len());
    for point in &points {
        while lower.len() >= 2
            && turn(lower[lower.len() - 2], lower[lower.len() - 1], *point) <= 0.0
        {
            lower.pop();
        }
        lower.push(*point);
    }
    let mut upper = Vec::with_capacity(points.len());
    for point in points.iter().rev() {
        while upper.len() >= 2
            && turn(upper[upper.len() - 2], upper[upper.len() - 1], *point) <= 0.0
        {
            upper.pop();
        }
        upper.push(*point);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
        .iter()
        .zip(lower.iter().cycle().skip(1))
        .map(|(a, b)| a.x * b.y - a.y * b.x)
        .sum::<f32>()
        .abs()
        * 0.5
}

fn turn(origin: Vec2, a: Vec2, b: Vec2) -> f32 {
    (a - origin).perp_dot(b - origin)
}

fn project_view_point(projection: Mat4, point: Vec3, width: f32, height: f32) -> Option<Vec2> {
    let clip = projection * Vec4::new(point.x, point.y, point.z, 1.0);
    if !clip.is_finite() || clip.w <= f32::EPSILON {
        return None;
    }
    let normalized = clip.truncate() / clip.w;
    Some(Vec2::new(
        (normalized.x + 1.0) * width * 0.5,
        (1.0 - normalized.y) * height * 0.5,
    ))
}

/// A local light's angular attenuation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LocalLightShape {
    /// Emits equally in every direction.
    Omni,
    /// Emits along `direction`, fading between the inner and outer cone.
    Spot {
        /// Unit direction from the light toward illuminated geometry.
        direction: [f32; 3],
        /// Cosine of the fully illuminated inner half-angle.
        inner_cos: f32,
        /// Cosine of the outer half-angle where illumination reaches zero.
        outer_cos: f32,
    },
}

/// Optional local-shadow metadata stored in the oracle light record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalShadow {
    /// Shadow texture-array index. Values below `-1` select dual-paraboloid
    /// omni-shadow addressing, matching the original shader.
    pub texture_index: f32,
    /// World-to-shadow transform rows.
    pub transform_rows: [[f32; 4]; 3],
    /// Atlas bounds preset in the range `0..=4`.
    pub bounds_preset: u32,
    /// Blend toward fully lit, where zero is the authored shadow and one is
    /// fully faded.
    pub fade: f32,
}

/// Renderer-facing local-light description.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalLight {
    /// World-space position.
    pub position: [f32; 3],
    /// Linear RGB radiance.
    pub color: [f32; 3],
    /// Radius at which smooth distance attenuation reaches zero.
    pub radius: f32,
    /// Fraction of `radius` at which the radial fade begins.
    pub far_attenuation_start: f32,
    /// Near-field inverse-distance decay scale.
    pub decay_distance: f32,
    /// Diffuse/specular angular shape.
    pub shape: LocalLightShape,
    /// Specular multiplier.
    pub specular_intensity: f32,
    /// Optional authored shadow projection.
    pub shadow: Option<LocalShadow>,
}

impl LocalLight {
    /// Creates an omnidirectional light without a local shadow.
    #[must_use]
    pub fn omni(position: [f32; 3], color: [f32; 3], radius: f32) -> Self {
        Self {
            position,
            color,
            radius,
            far_attenuation_start: 0.0,
            decay_distance: radius,
            shape: LocalLightShape::Omni,
            specular_intensity: 1.0,
            shadow: None,
        }
    }

    /// Creates a spot light without a local shadow.
    #[must_use]
    pub fn spot(
        position: [f32; 3],
        color: [f32; 3],
        radius: f32,
        direction: [f32; 3],
        inner_cos: f32,
        outer_cos: f32,
    ) -> Self {
        Self {
            position,
            color,
            radius,
            far_attenuation_start: 0.0,
            decay_distance: radius,
            shape: LocalLightShape::Spot {
                direction,
                inner_cos,
                outer_cos,
            },
            specular_intensity: 1.0,
            shadow: None,
        }
    }

    fn packed(self) -> PackedLocalLight {
        let safe_radius = self.radius.max(f32::EPSILON);
        let far_attenuation_start = self.far_attenuation_start.clamp(0.0, 0.999);
        let inverse_falloff_range = (1.0 - far_attenuation_start).recip();
        let omni_mul = -safe_radius.recip() * inverse_falloff_range;
        let omni_add = 1.0 + far_attenuation_start * inverse_falloff_range;
        let (spot_at, spot_mul, spot_add) = match self.shape {
            LocalLightShape::Omni => ([0.0, 1.0, 0.0], 0.0, 1.0),
            LocalLightShape::Spot {
                direction,
                inner_cos,
                outer_cos,
            } => {
                let direction = Vec3::from_array(direction).normalize_or_zero();
                let direction = if direction == Vec3::ZERO {
                    Vec3::Y
                } else {
                    direction
                };
                let inner_cos = inner_cos.clamp(-1.0, 1.0);
                let outer_cos = outer_cos.clamp(-1.0, inner_cos);
                let angular_range = (inner_cos - outer_cos).max(f32::EPSILON);
                (
                    direction.to_array(),
                    angular_range.recip(),
                    -outer_cos / angular_range,
                )
            }
        };
        let (shadow_index, shadow_rows, shadow_metadata) =
            self.shadow
                .map_or((-1.0, [[0.0; 4]; 3], [0.0; 4]), |shadow| {
                    (
                        shadow.texture_index,
                        shadow.transform_rows,
                        [
                            shadow.bounds_preset.min(4).to_f32().unwrap_or(4.0),
                            shadow.fade.clamp(0.0, 1.0),
                            0.0,
                            0.0,
                        ],
                    )
                });
        PackedLocalLight {
            position_and_omni: [
                self.position[0],
                self.position[1],
                self.position[2],
                omni_mul,
            ],
            color_and_omni: [self.color[0], self.color[1], self.color[2], omni_add],
            attenuation_and_shadow: [
                self.decay_distance.max(0.0),
                spot_mul,
                spot_add,
                shadow_index,
            ],
            spot_and_specular: [
                spot_at[0],
                spot_at[1],
                spot_at[2],
                self.specular_intensity.max(0.0),
            ],
            shadow_row0: shadow_rows[0],
            shadow_row1: shadow_rows[1],
            shadow_row2: shadow_rows[2],
            shadow_metadata,
        }
    }
}

/// A bounded local-light set and its shared shader settings.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalLightSet {
    lights: Vec<LocalLight>,
    /// Specular exponent used by the terrain/local-light oracle path.
    pub specular_power: f32,
    /// Whether local shadow-map sampling is enabled.
    pub shadows_enabled: bool,
}

impl Default for LocalLightSet {
    fn default() -> Self {
        Self {
            lights: Vec::new(),
            specular_power: 16.0,
            shadows_enabled: false,
        }
    }
}

impl LocalLightSet {
    /// Creates a checked light set.
    ///
    /// # Errors
    ///
    /// Returns [`LocalLightError::TooManyLights`] when more than the oracle's
    /// twenty-light limit is supplied.
    pub fn new(lights: Vec<LocalLight>) -> Result<Self, LocalLightError> {
        validate_count(lights.len())?;
        Ok(Self {
            lights,
            ..Self::default()
        })
    }

    /// Returns the active lights in upload order.
    #[must_use]
    pub fn lights(&self) -> &[LocalLight] {
        &self.lights
    }

    pub(crate) fn light(&self, index: usize) -> Option<&LocalLight> {
        self.lights.get(index)
    }

    pub(crate) fn set_shadow(&mut self, index: usize, shadow: Option<LocalShadow>) {
        if let Some(light) = self.lights.get_mut(index) {
            light.shadow = shadow;
        }
    }

    pub(crate) fn clear_shadows(&mut self) {
        for light in &mut self.lights {
            light.shadow = None;
        }
        self.shadows_enabled = false;
    }

    /// Appends one light without silently truncating the shader-visible set.
    ///
    /// # Errors
    ///
    /// Returns [`LocalLightError::TooManyLights`] at the twenty-light limit.
    pub fn push(&mut self, light: LocalLight) -> Result<(), LocalLightError> {
        validate_count(self.lights.len().saturating_add(1))?;
        self.lights.push(light);
        Ok(())
    }

    /// Removes all active lights.
    pub fn clear(&mut self) {
        self.lights.clear();
    }

    /// Replaces the active payload while retaining shared specular/shadow settings.
    ///
    /// # Errors
    ///
    /// Returns [`LocalLightError::TooManyLights`] when the replacement exceeds
    /// the shader-visible limit.
    pub fn replace(&mut self, lights: Vec<LocalLight>) -> Result<(), LocalLightError> {
        validate_count(lights.len())?;
        self.lights = lights;
        Ok(())
    }

    /// Applies count/specular/shadow switches to the shared lighting uniform.
    pub fn apply_to_lighting(&self, lighting: &mut LightingParams) {
        lighting.local_light_params = [
            self.lights.len().to_f32().unwrap_or(0.0),
            self.specular_power.max(f32::EPSILON),
            f32::from(self.shadows_enabled),
            f32::from(!self.lights.is_empty()),
        ];
    }
}

/// Persistent GPU storage for a [`LocalLightSet`].
#[derive(Clone)]
pub struct LocalLightBuffer {
    buffer: wgpu::Buffer,
}

impl LocalLightBuffer {
    /// Creates and initializes the fixed-size oracle light buffer.
    #[must_use]
    pub fn new(device: &wgpu::Device, lights: &LocalLightSet) -> Self {
        let packed = pack_lights(lights);
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Shared Local Lights"),
            contents: bytemuck::cast_slice(&packed),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        Self { buffer }
    }

    /// Creates the common zero-light buffer.
    #[must_use]
    pub fn empty(device: &wgpu::Device) -> Self {
        Self::new(device, &LocalLightSet::default())
    }

    /// Replaces the whole fixed-size payload.
    pub fn update(&self, queue: &wgpu::Queue, lights: &LocalLightSet) {
        let packed = pack_lights(lights);
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&packed));
    }

    /// Returns the storage buffer shared by renderer bind groups.
    #[must_use]
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }
}

/// Local-light validation error.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum LocalLightError {
    /// The fixed shader loop cannot address the requested count.
    #[error("local-light count {actual} exceeds the shader limit of {MAX_LOCAL_LIGHTS}")]
    TooManyLights {
        /// Requested light count.
        actual: usize,
    },
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct PackedLocalLight {
    position_and_omni: [f32; 4],
    color_and_omni: [f32; 4],
    attenuation_and_shadow: [f32; 4],
    spot_and_specular: [f32; 4],
    shadow_row0: [f32; 4],
    shadow_row1: [f32; 4],
    shadow_row2: [f32; 4],
    shadow_metadata: [f32; 4],
}

fn validate_count(count: usize) -> Result<(), LocalLightError> {
    if count > MAX_LOCAL_LIGHTS {
        Err(LocalLightError::TooManyLights { actual: count })
    } else {
        Ok(())
    }
}

fn pack_lights(lights: &LocalLightSet) -> [PackedLocalLight; MAX_LOCAL_LIGHTS] {
    let mut packed = [PackedLocalLight::default(); MAX_LOCAL_LIGHTS];
    for (destination, light) in packed.iter_mut().zip(lights.lights()) {
        *destination = light.packed();
    }
    packed
}

#[cfg(test)]
mod tests {
    use std::mem;

    use glam::{Mat4, Vec3};

    use super::{
        LocalLight, LocalLightError, LocalLightSet, LocalLightView, MAX_LOCAL_LIGHTS,
        PackedLocalLight, pack_lights,
    };
    use crate::terrain::LightingParams;

    #[test]
    fn packed_light_matches_oracle_stride() {
        assert_eq!(mem::size_of::<PackedLocalLight>(), 8 * 16);
    }

    #[test]
    fn omni_light_fades_at_its_radius() {
        let packed = LocalLight::omni([1.0, 2.0, 3.0], [4.0, 5.0, 6.0], 8.0).packed();
        assert_eq!(
            packed.position_and_omni.map(f32::to_bits),
            [1.0, 2.0, 3.0, -0.125].map(f32::to_bits)
        );
        assert_eq!(
            packed.color_and_omni.map(f32::to_bits),
            [4.0, 5.0, 6.0, 1.0].map(f32::to_bits)
        );
        assert_eq!(
            packed.attenuation_and_shadow[3].to_bits(),
            (-1.0_f32).to_bits()
        );
        assert_eq!(
            packed.spot_and_specular[0..3]
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            [0.0, 1.0, 0.0].map(f32::to_bits)
        );
    }

    #[test]
    fn spot_cone_maps_outer_to_zero_and_inner_to_one() {
        let packed = LocalLight::spot([0.0; 3], [1.0; 3], 10.0, [0.0, 0.0, 2.0], 0.9, 0.7).packed();
        assert_eq!(
            packed.spot_and_specular[0..3]
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            [0.0, 0.0, 1.0].map(f32::to_bits)
        );
        assert!((packed.attenuation_and_shadow[1] - 5.0).abs() < 1.0e-5);
        assert!((packed.attenuation_and_shadow[2] + 3.5).abs() < 1.0e-5);
    }

    #[test]
    fn oversized_sets_are_rejected_instead_of_truncated() {
        let lights = vec![LocalLight::omni([0.0; 3], [1.0; 3], 1.0); MAX_LOCAL_LIGHTS + 1];
        assert_eq!(
            LocalLightSet::new(lights),
            Err(LocalLightError::TooManyLights {
                actual: MAX_LOCAL_LIGHTS + 1
            })
        );
    }

    #[test]
    fn set_controls_shared_lighting_switches() {
        let mut set = LocalLightSet {
            specular_power: 32.0,
            shadows_enabled: true,
            ..LocalLightSet::default()
        };
        set.push(LocalLight::omni([0.0; 3], [1.0; 3], 5.0)).unwrap();
        let mut lighting = LightingParams::default();
        set.apply_to_lighting(&mut lighting);
        assert_eq!(
            lighting.local_light_params.map(f32::to_bits),
            [1.0, 32.0, 1.0, 1.0].map(f32::to_bits)
        );
        assert_eq!(pack_lights(&set)[0].position_and_omni[0].to_bits(), 0);
    }

    #[test]
    fn spot_proxy_reduces_projected_area_and_culls_an_offscreen_cone() {
        let projection = Mat4::orthographic_rh(-10.0, 10.0, -10.0, 10.0, 0.1, 100.0);
        let view = LocalLightView::new(projection, Mat4::IDENTITY, Vec3::ZERO, [200, 100]);
        let outer_cos = 30.0_f32.to_radians().cos();
        let radius = view
            .project_capped_cone_radius([0.0, 0.0, -10.0], [0.0, 0.0, -1.0], outer_cos, 2.0)
            .expect("front-facing cone proxy projects");

        assert!((radius - (200.0 / std::f32::consts::PI).sqrt()).abs() < 1.0e-4);
        assert!(view.capped_cone_visible([0.0, 0.0, -10.0], [0.0, 0.0, -1.0], outer_cos, 2.0,));
        assert!(!view.capped_cone_visible([50.0, 0.0, -10.0], [0.0, 0.0, -1.0], outer_cos, 2.0,));
    }
}
