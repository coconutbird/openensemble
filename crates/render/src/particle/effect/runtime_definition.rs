//! Authored emitter data used by the retail particle update path.

use num_traits::ToPrimitive;
use pipeline::xmb::Node;

use super::{
    ParticleEffectError, ParticleEmitterKind, child, child_text, packed_color_child, parse_bool,
    unsupported,
};
use crate::particle::ParticleGeometry;

mod flags;
#[cfg(test)]
mod tests;

use flags::{ParticleForceFlags, ParticleTimingFlags};

/// A base value with symmetric multiplicative variance.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ParticleVarying {
    /// Authored base value.
    pub value: f32,
    /// Fractional variance used by `value * (1 + random * variance)`.
    pub variance: f32,
}

impl ParticleVarying {
    fn from_children(node: &Node, value: &str, variance: &str, default: f32) -> Self {
        Self {
            value: number(node, value, default),
            variance: number(node, variance, 0.0),
        }
    }

    /// Applies the retail multiplicative variance equation.
    #[must_use]
    pub fn sample(self, signed_random: f32) -> f32 {
        self.value * signed_random.mul_add(self.variance, 1.0)
    }
}

/// Trail emission scheduling used by `eTrail` and `eTrailCross`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ParticleTrailEmission {
    /// Add control points as the emitter moves.
    #[default]
    ByLength,
    /// Add control points at the regular emission rate.
    ByTime,
}

/// How trail U coordinates are generated.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ParticleTrailUv {
    /// Stretch one texture over the entire live trail.
    #[default]
    Stretch,
    /// Map one texture face to every segment.
    FaceMap,
}

/// Retail emitter timing and motion constants.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleEmitterTiming {
    flags: ParticleTimingFlags,
    /// Radius used by the retail update-priority system.
    pub update_radius: f32,
    /// Fractional variance applied to the authored particle cap.
    pub max_particles_variance: f32,
    /// Lifetime of each emitted particle, in seconds.
    pub particle_life: ParticleVarying,
    /// Legacy global fade-in interval retained by the data format.
    pub global_fade_in: ParticleVarying,
    /// Legacy global fade-out interval retained by the data format.
    pub global_fade_out: ParticleVarying,
    /// Particles emitted per second.
    pub emission_rate: ParticleVarying,
    /// Delay before the first active interval, in seconds.
    pub start_delay: ParticleVarying,
    /// Amount of particle history simulated on first update, in seconds.
    pub initial_update: ParticleVarying,
    /// Length of one active interval, in seconds.
    pub emission_time: ParticleVarying,
    /// Delay between looping active intervals, in seconds.
    pub loop_delay: ParticleVarying,
    /// Initial displacement along the emission velocity.
    pub initial_distance: ParticleVarying,
    /// Initial particle speed.
    pub velocity: ParticleVarying,
    /// Acceleration along the current velocity vector.
    pub acceleration: ParticleVarying,
    /// Distance between length-emitted trail control points.
    pub trail_segment_length: f32,
    /// Retained retail emitter-attraction parameter.
    pub emitter_attraction: ParticleVarying,
    /// Fraction of velocity lost on terrain collision.
    pub collision_energy_loss: ParticleVarying,
    /// Terrain collision height offset.
    pub collision_offset: f32,
    /// Trail scheduling family.
    pub trail_emission: ParticleTrailEmission,
    /// Trail texture-coordinate family.
    pub trail_uv: ParticleTrailUv,
    /// Number of beam subdivisions requested by the asset.
    pub beam_tessellation: u32,
    /// First authored beam tangent.
    pub beam_tangent_1: [f32; 3],
    /// Second authored beam tangent.
    pub beam_tangent_2: [f32; 3],
    /// Terrain-patch tessellation level, clamped to the retail range.
    pub terrain_tessellation: f32,
    /// Terrain-patch vertical offset.
    pub terrain_y_offset: f32,
}

impl Default for ParticleEmitterTiming {
    fn default() -> Self {
        Self {
            flags: ParticleTimingFlags::default(),
            update_radius: 10.0,
            max_particles_variance: 0.0,
            particle_life: constant_varying(1.0),
            global_fade_in: constant_varying(0.0),
            global_fade_out: constant_varying(0.0),
            emission_rate: constant_varying(100.0),
            start_delay: constant_varying(0.0),
            initial_update: constant_varying(0.0),
            emission_time: constant_varying(1.0),
            loop_delay: constant_varying(0.0),
            initial_distance: constant_varying(0.0),
            velocity: constant_varying(1.0),
            acceleration: constant_varying(0.0),
            trail_segment_length: 0.0,
            emitter_attraction: constant_varying(1.0),
            collision_energy_loss: constant_varying(0.0),
            collision_offset: 0.0,
            trail_emission: ParticleTrailEmission::ByLength,
            trail_uv: ParticleTrailUv::Stretch,
            beam_tessellation: 1,
            beam_tangent_1: [0.0; 3],
            beam_tangent_2: [0.0; 3],
            terrain_tessellation: 1.0,
            terrain_y_offset: 0.125,
        }
    }
}

impl ParticleEmitterTiming {
    fn from_node(node: &Node, kind: &ParticleEmitterKind) -> Result<Self, ParticleEffectError> {
        let mut timing = Self::default();
        timing.read_switches(node);
        timing.read_lifetime(node);
        timing.read_motion(node);
        timing.read_beam(node);
        timing.trail_emission = parse_trail_emission(node)?;
        timing.trail_uv = parse_trail_uv(node)?;
        if matches!(
            kind,
            ParticleEmitterKind::Render(ParticleGeometry::Trail | ParticleGeometry::TrailCross)
        ) {
            timing.particle_life.value = timing.particle_life.value.max(0.11);
        }
        Ok(timing)
    }

    fn read_switches(&mut self, node: &Node) {
        self.flags.set(
            ParticleTimingFlags::TIED_TO_EMITTER,
            boolean(node, "TiedToEmitter", false),
        );
        self.flags.set(
            ParticleTimingFlags::IGNORE_ROTATION,
            boolean(node, "IgnoreRotation", false),
        );
        self.flags
            .set(ParticleTimingFlags::LOOPING, boolean(node, "Loop", false));
        self.flags.set(
            ParticleTimingFlags::ALWAYS_ACTIVE,
            boolean(node, "AlwaysActive", false),
        );
        self.flags.set(
            ParticleTimingFlags::ALWAYS_RENDER,
            boolean(node, "AlwaysRender", false),
        );
        self.flags.set(
            ParticleTimingFlags::KILL_IMMEDIATELY_ON_RELEASE,
            boolean(node, "KillImmediatelyOnRelease", false),
        );
        self.flags.set(
            ParticleTimingFlags::COLLISION_DETECTION_TERRAIN,
            boolean(node, "CollisionDetectionTerrain", false),
        );
        self.flags.set(
            ParticleTimingFlags::FILL_OPTIMIZED,
            boolean(node, "FillOptimized", false),
        );
    }

    fn read_lifetime(&mut self, node: &Node) {
        self.update_radius = number(node, "UpdateRadius", 10.0);
        self.max_particles_variance = number(node, "MaxParticlesVar", 0.0);
        self.particle_life = varying_children(node, "ParticleLife", 1.0);
        self.global_fade_in = varying_children(node, "GlobalFadeIn", 0.0);
        self.global_fade_out = varying_children(node, "GlobalFadeOut", 0.0);
        self.emission_rate = varying_children(node, "EmissionRate", 100.0);
        self.start_delay = varying_children(node, "StartDelay", 0.0);
        self.initial_update = varying_children(node, "InitialUpdate", 0.0);
        self.emission_time = varying_children(node, "EmissionTime", 1.0);
        self.loop_delay = varying_children(node, "LoopDelay", 0.0);
    }

    fn read_motion(&mut self, node: &Node) {
        self.initial_distance = varying_children(node, "InitialDistance", 0.0);
        self.velocity = varying_children(node, "Velocity", 1.0);
        self.acceleration = varying_children(node, "Acceleration", 0.0);
        self.trail_segment_length = number(node, "TrailSegmentLength", 0.0);
        self.emitter_attraction = varying_children(node, "EmitterAttraction", 1.0);
        self.collision_energy_loss = varying_children(node, "CollisionEnergyLoss", 0.0);
        self.collision_offset = number(node, "CollisionOffset", 0.0);
        self.terrain_tessellation =
            number(node, "TerrainDecalTesselation", 1.0_f32).clamp(1.0, 15.0);
        self.terrain_y_offset = number(node, "TerrainDecalYOffset", 0.125);
    }

    fn read_beam(&mut self, node: &Node) {
        self.beam_tessellation = number(node, "BeamTesselation", 1_u32);
        self.beam_tangent_1 = trailing_axis_vector(node, "BeamTangent1");
        self.beam_tangent_2 = trailing_axis_vector(node, "BeamTangent2");
        self.flags.set(
            ParticleTimingFlags::BEAM_COLOR_BY_LENGTH,
            boolean(node, "BeamColorByLength", false),
        );
        self.flags.set(
            ParticleTimingFlags::BEAM_OPACITY_BY_LENGTH,
            boolean(node, "BeamOpacityByLength", false),
        );
        self.flags.set(
            ParticleTimingFlags::BEAM_INTENSITY_BY_LENGTH,
            boolean(node, "BeamIntensityByLength", false),
        );
    }
}

/// Volume used to choose a particle's initial position.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ParticleEmitterShapeKind {
    /// One point at the authored offset.
    #[default]
    Point,
    /// Axis-aligned box, optionally surface-only.
    Box,
    /// Vertical cylinder.
    Cylinder,
    /// Sphere.
    Sphere,
    /// Upper hemisphere.
    HalfSphere,
    /// Horizontal rectangle perimeter.
    Rectangle,
    /// Horizontal circle.
    Circle,
}

/// Initial-position and trajectory-cone data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParticleEmitterShape {
    /// Shape family.
    pub kind: ParticleEmitterShapeKind,
    /// Authored X/Y/Z size values.
    pub size: [f32; 3],
    /// Local emission offset.
    pub offset: [f32; 3],
    /// Inner trajectory angle, in degrees.
    pub trajectory_inner_angle: f32,
    /// Outer trajectory angle, in degrees.
    pub trajectory_outer_angle: f32,
    /// Pitch/yaw/bank applied to both shape and trajectory, in degrees.
    pub trajectory_rotation: [f32; 3],
    /// Legacy surface-radius value retained by the format.
    pub emit_from_surface_radius: f32,
    /// Restricts emission to the shape surface.
    pub emit_from_surface: bool,
}

impl ParticleEmitterShape {
    fn from_node(node: Option<&Node>) -> Result<Self, ParticleEffectError> {
        let Some(node) = node else {
            return Ok(Self::default());
        };
        let kind = match child_text(node, "ShapeType").as_deref().unwrap_or("ePoint") {
            "ePoint" => ParticleEmitterShapeKind::Point,
            "eBox" => ParticleEmitterShapeKind::Box,
            "eCylinder" => ParticleEmitterShapeKind::Cylinder,
            "eSphere" => ParticleEmitterShapeKind::Sphere,
            "eHalfSphere" => ParticleEmitterShapeKind::HalfSphere,
            "eRectangle" => ParticleEmitterShapeKind::Rectangle,
            "eCircle" => ParticleEmitterShapeKind::Circle,
            value => return unsupported("shape type", value),
        };
        let inner = number(node, "TrajectoryInnerAngle", 0.0_f32);
        let outer = number(node, "TrajectoryOuterAngle", 0.0_f32);
        Ok(Self {
            kind,
            size: leading_axis_vector(node, "Size"),
            offset: leading_axis_vector(node, "PosOffset"),
            trajectory_inner_angle: inner.min(outer),
            trajectory_outer_angle: outer.max(inner),
            trajectory_rotation: [
                number(node, "TrajectoryPitch", 0.0),
                number(node, "TrajectoryYaw", 0.0),
                number(node, "TrajectoryBank", 0.0),
            ],
            emit_from_surface_radius: number(node, "EmitFromSurfaceRadius", 0.0),
            emit_from_surface: boolean(node, "EmitFromSurface", false),
        })
    }
}

/// One scalar key in a lifetime progression.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ParticleScalarKey {
    /// Normalized lifetime location.
    pub alpha: f32,
    /// Key value.
    pub value: f32,
    /// Per-key multiplicative variance.
    pub variance: f32,
}

/// Piecewise-linear scalar progression.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParticleScalarProgression {
    /// Keys in authored order.
    pub keys: Vec<ParticleScalarKey>,
    /// Number of repeats when looping.
    pub cycles: f32,
    /// Repeats the progression over normalized particle life.
    pub looping: bool,
}

impl ParticleScalarProgression {
    /// Samples the retail piecewise-linear progression.
    #[must_use]
    pub fn sample(&self, alpha: f32, signed_random: f32) -> f32 {
        let alpha = progression_alpha(alpha, self.looping, self.cycles);
        let Some((left, right)) = progression_span(&self.keys, alpha, |key| key.alpha) else {
            return 1.0;
        };
        let left_value = varying_key(left, signed_random);
        let right_value = varying_key(right, signed_random);
        lerp_span(left.alpha, right.alpha, alpha, left_value, right_value)
    }
}

/// Scalar base value plus optional lifetime progression.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParticleScalarProperty {
    /// Constant multiplier and per-particle variance.
    pub base: ParticleVarying,
    /// Whether the progression modulates the constant.
    pub use_progression: bool,
    /// Lifetime progression.
    pub progression: ParticleScalarProgression,
}

impl ParticleScalarProperty {
    fn from_node(node: Option<&Node>) -> Self {
        let Some(node) = node else {
            return Self::default();
        };
        Self {
            base: ParticleVarying::from_children(node, "Value", "ValueVariance", 0.0),
            use_progression: boolean(node, "UseProgression", false),
            progression: child(node, "Progression")
                .map_or_else(ParticleScalarProgression::default, parse_scalar_progression),
        }
    }

    /// Evaluates the complete authored scalar value.
    #[must_use]
    pub fn sample(&self, alpha: f32, signed_random: f32) -> f32 {
        self.sample_with_progression_random(alpha, signed_random, 0.0)
    }

    /// Evaluates with independent per-particle and progression variance samples.
    #[must_use]
    pub fn sample_with_progression_random(
        &self,
        alpha: f32,
        signed_random: f32,
        progression_random: f32,
    ) -> f32 {
        let base = self.base.sample(signed_random);
        if self.use_progression {
            base * self.progression.sample(alpha, progression_random)
        } else {
            base
        }
    }
}

/// Three-axis base values and optional per-axis progressions.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParticleVectorProperty {
    /// X/Y/Z and uniform base multipliers.
    pub value: [f32; 4],
    /// X/Y/Z and uniform multiplicative variances.
    pub variance: [f32; 4],
    /// Progression enable switches for X/Y/Z.
    pub use_progression: [bool; 3],
    /// Progressions for X/Y/Z.
    pub progression: [ParticleScalarProgression; 3],
}

impl ParticleVectorProperty {
    fn from_node(node: Option<&Node>) -> Self {
        let Some(node) = node else {
            return Self::default();
        };
        let progression = child(node, "Progression");
        Self {
            value: [
                number(node, "ValueX", 1.0),
                number(node, "ValueY", 1.0),
                number(node, "ValueZ", 1.0),
                number(node, "UniformValue", 1.0),
            ],
            variance: [
                number(node, "ValueXVariance", 0.0),
                number(node, "ValueYVariance", 0.0),
                number(node, "ValueZVariance", 0.0),
                number(node, "UniformValueVariance", 0.0),
            ],
            use_progression: [
                boolean(node, "UseXProgression", false),
                boolean(node, "UseYProgression", false),
                boolean(node, "UseZProgression", false),
            ],
            progression: ["XProgression", "YProgression", "ZProgression"].map(|name| {
                progression
                    .and_then(|progression| child(progression, name))
                    .map_or_else(ParticleScalarProgression::default, parse_scalar_progression)
            }),
        }
    }

    /// Samples X/Y/Z, including the authored uniform multiplier.
    #[must_use]
    pub fn sample(&self, alpha: f32, signed_randoms: [f32; 4]) -> [f32; 3] {
        self.sample_with_progression_random(alpha, signed_randoms, 0.0)
    }

    /// Samples X/Y/Z with independent per-particle and progression variance.
    #[must_use]
    pub fn sample_with_progression_random(
        &self,
        alpha: f32,
        signed_randoms: [f32; 4],
        progression_random: f32,
    ) -> [f32; 3] {
        let uniform = sample_varying(self.value[3], self.variance[3], signed_randoms[3]);
        std::array::from_fn(|axis| {
            let base = sample_varying(self.value[axis], self.variance[axis], signed_randoms[axis]);
            let progression = if self.use_progression[axis] {
                self.progression[axis].sample(alpha, progression_random)
            } else {
                1.0
            };
            base * uniform * progression
        })
    }
}

/// Color source selected by the PFX `ColorData/Type` field.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ParticleColorKind {
    /// One constant color.
    #[default]
    Single,
    /// Randomly selects from an authored palette.
    Palette,
    /// Samples a lifetime color gradient.
    Progression,
}

/// One color-gradient key.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ParticleColorKey {
    /// Normalized lifetime location.
    pub alpha: f32,
    /// Linear RGBA decoded from A8R8G8B8.
    pub color: [f32; 4],
}

/// Piecewise-linear color progression.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParticleColorProgression {
    /// Keys in authored order.
    pub keys: Vec<ParticleColorKey>,
    /// Number of repeats when looping.
    pub cycles: f32,
    /// Repeats the gradient over normalized particle life.
    pub looping: bool,
}

impl ParticleColorProgression {
    /// Samples the authored RGBA gradient.
    #[must_use]
    pub fn sample(&self, alpha: f32) -> [f32; 4] {
        let alpha = progression_alpha(alpha, self.looping, self.cycles);
        let Some((left, right)) = progression_span(&self.keys, alpha, |key| key.alpha) else {
            return [1.0; 4];
        };
        std::array::from_fn(|channel| {
            lerp_span(
                left.alpha,
                right.alpha,
                alpha,
                left.color[channel],
                right.color[channel],
            )
        })
    }
}

/// One authored palette entry.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ParticlePaletteEntry {
    /// Palette RGB/A value.
    pub color: [f32; 4],
    /// Editor selection weight; retained even though the retail shader indexes uniformly.
    pub weight: f32,
}

/// Base/progression/palette color data and external tint switches.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleColorDefinition {
    /// Color source family.
    pub kind: ParticleColorKind,
    /// Constant source color.
    pub color: [f32; 4],
    /// Lifetime color gradient.
    pub progression: ParticleColorProgression,
    /// Authored random palette.
    pub palette: Vec<ParticlePaletteEntry>,
    /// Multiplies by the owning player's tint color.
    pub player_color: bool,
    /// RGB multiplier applied with player color.
    pub player_color_intensity: f32,
    /// Multiplies by the active light-set sun color.
    pub sun_color: bool,
    /// RGB multiplier applied with sun color.
    pub sun_color_intensity: f32,
}

impl Default for ParticleColorDefinition {
    fn default() -> Self {
        Self {
            kind: ParticleColorKind::Single,
            color: [0.0; 4],
            progression: ParticleColorProgression::default(),
            palette: Vec::new(),
            player_color: false,
            player_color_intensity: 1.0,
            sun_color: false,
            sun_color_intensity: 1.0,
        }
    }
}

impl ParticleColorDefinition {
    fn from_node(node: Option<&Node>) -> Result<Self, ParticleEffectError> {
        let Some(node) = node else {
            return Ok(Self::default());
        };
        let kind = match child_text(node, "Type")
            .as_deref()
            .unwrap_or("eSingleColor")
        {
            "eSingleColor" => ParticleColorKind::Single,
            "ePalletteColor" => ParticleColorKind::Palette,
            "eProgression" => ParticleColorKind::Progression,
            value => return unsupported("color type", value),
        };
        let progression = child(node, "ColorProgression")
            .map_or_else(ParticleColorProgression::default, parse_color_progression);
        let palette = child(node, "ColorPallette")
            .map(|palette| {
                palette
                    .children
                    .iter()
                    .map(|entry| ParticlePaletteEntry {
                        color: packed_color_child(Some(entry), "Color").unwrap_or([0.0; 4]),
                        weight: number(entry, "Weight", 0.0),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            kind,
            color: packed_color_child(Some(node), "Color").unwrap_or([0.0; 4]),
            progression,
            palette,
            player_color: boolean(node, "PlayerColor", false),
            player_color_intensity: number(node, "PlayerColorIntensity", 1.0_f32).clamp(0.0, 1.0),
            sun_color: boolean(node, "SunColor", false),
            sun_color_intensity: number(node, "SunColorIntensity", 1.0_f32).clamp(0.0, 1.0),
        })
    }

    /// Samples the source color before player/sun tint and opacity.
    #[must_use]
    pub fn sample(&self, alpha: f32, signed_random: f32) -> [f32; 4] {
        match self.kind {
            ParticleColorKind::Single => self.color,
            ParticleColorKind::Progression => self.progression.sample(alpha),
            ParticleColorKind::Palette => self.palette_color(signed_random),
        }
    }

    fn palette_color(&self, signed_random: f32) -> [f32; 4] {
        if self.palette.is_empty() {
            return [1.0; 4];
        }
        let unit = signed_random.mul_add(0.5, 0.5).clamp(0.0, 1.0);
        let count = self.palette.len().min(8);
        let max_index = (count - 1).to_f32().unwrap_or(7.0);
        let index = unit
            .mul_add(max_index, 0.5)
            .floor()
            .to_usize()
            .unwrap_or(count - 1)
            .min(count - 1);
        self.palette[index].color
    }
}

/// Internal gravity, wind, and tumble controls.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParticleForceDefinition {
    flags: ParticleForceFlags,
    /// Minimum angular velocity in degrees per second.
    pub min_angular_velocity: f32,
    /// Maximum angular velocity in degrees per second.
    pub max_angular_velocity: f32,
    /// Internal gravity and fractional variance.
    pub internal_gravity: ParticleVarying,
    /// Internal wind direction in degrees.
    pub internal_wind_direction: ParticleVarying,
    /// Internal wind speed.
    pub internal_wind_speed: ParticleVarying,
    /// Internal wind delay in seconds.
    pub internal_wind_delay: ParticleVarying,
}

impl ParticleForceDefinition {
    fn from_node(node: Option<&Node>) -> Self {
        let Some(node) = node else {
            return Self::default();
        };
        let mut flags = ParticleForceFlags::default();
        flags.set(
            ParticleForceFlags::RANDOM_ORIENTATION,
            boolean(node, "RandomOrientation", false),
        );
        flags.set(
            ParticleForceFlags::TUMBLE,
            boolean(node, "UseTumble", false),
        );
        flags.set(
            ParticleForceFlags::TUMBLE_BOTH_DIRECTIONS,
            boolean(node, "TumbleBothDirections", false),
        );
        flags.set(
            ParticleForceFlags::USE_INTERNAL_GRAVITY,
            boolean(node, "UseInternalGravity", false),
        );
        flags.set(
            ParticleForceFlags::USE_INTERNAL_WIND,
            boolean(node, "UseInternalWind", false),
        );
        Self {
            flags,
            min_angular_velocity: number(node, "MinAngularTumbleVelocity", 0.0),
            max_angular_velocity: number(node, "MaxAngularTumbleVelocity", 0.0),
            internal_gravity: ParticleVarying::from_children(
                node,
                "InternalGravity",
                "InternalGravityVar",
                0.0,
            ),
            internal_wind_direction: ParticleVarying::from_children(
                node,
                "InternalWindDirection",
                "InternalWindDirectionVar",
                0.0,
            ),
            internal_wind_speed: ParticleVarying::from_children(
                node,
                "InternalWindSpeed",
                "InternalWindSpeedVar",
                0.0,
            ),
            internal_wind_delay: ParticleVarying::from_children(
                node,
                "InternalWindDelay",
                "InternalWindDelayVar",
                0.0,
            ),
        }
    }
}

/// Magnet volume family.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ParticleMagnetKind {
    /// Spherical influence volume.
    #[default]
    Sphere,
    /// Cylindrical influence volume.
    Cylinder,
}

/// One authored particle magnet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParticleMagnetDefinition {
    /// Magnet volume family.
    pub kind: ParticleMagnetKind,
    /// Local position offset.
    pub offset: [f32; 3],
    /// Pitch/yaw/roll in degrees.
    pub rotation: [f32; 3],
    /// Radial force.
    pub force: f32,
    /// Tangential force.
    pub rotational_force: f32,
    /// Influence radius.
    pub radius: f32,
    /// Cylinder height.
    pub height: f32,
    /// Random force component.
    pub turbulence: f32,
    /// Velocity dampening.
    pub dampening: f32,
}

impl ParticleMagnetDefinition {
    fn from_node(node: &Node) -> Result<Self, ParticleEffectError> {
        let kind = match child_text(node, "MagnetType")
            .as_deref()
            .unwrap_or("eSphere")
        {
            "eSphere" => ParticleMagnetKind::Sphere,
            "eCylinder" => ParticleMagnetKind::Cylinder,
            value => return unsupported("magnet type", value),
        };
        Ok(Self {
            kind,
            offset: leading_axis_vector(node, "PosOffset"),
            rotation: leading_axis_vector(node, "Rotation"),
            force: number(node, "Force", 0.0),
            rotational_force: number(node, "RotationalForce", 0.0),
            radius: number(node, "Radius", 0.0),
            height: number(node, "Height", 0.0),
            turbulence: number(node, "Turbulence", 0.0),
            dampening: number(node, "Dampening", 0.0),
        })
    }
}

/// Complete non-material data needed to run one emitter.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParticleRuntimeDefinition {
    /// Emitter timing and motion constants.
    pub timing: ParticleEmitterTiming,
    /// Initial position/trajectory volume.
    pub shape: ParticleEmitterShape,
    /// Color source and tint switches.
    pub color: ParticleColorDefinition,
    /// Opacity value and progression.
    pub opacity: ParticleScalarProperty,
    /// Scale value and progressions.
    pub scale: ParticleVectorProperty,
    /// Per-axis motion multiplier and progressions.
    pub speed: ParticleVectorProperty,
    /// HDR intensity value and progression.
    pub intensity: ParticleScalarProperty,
    /// Gravity, wind, and tumble controls.
    pub force: ParticleForceDefinition,
    /// Authored magnet volumes.
    pub magnets: Vec<ParticleMagnetDefinition>,
}

impl ParticleRuntimeDefinition {
    pub(super) fn from_node(
        node: &Node,
        emitter_data: &Node,
        kind: &ParticleEmitterKind,
    ) -> Result<Self, ParticleEffectError> {
        let magnets = node
            .children
            .iter()
            .filter(|child| child.name.eq_ignore_ascii_case("ParticleMagnet"))
            .map(ParticleMagnetDefinition::from_node)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            timing: ParticleEmitterTiming::from_node(emitter_data, kind)?,
            shape: ParticleEmitterShape::from_node(child(node, "ShapeData"))?,
            color: ParticleColorDefinition::from_node(child(node, "ColorData"))?,
            opacity: ParticleScalarProperty::from_node(child(node, "OpacityData")),
            scale: ParticleVectorProperty::from_node(child(node, "ScaleData")),
            speed: ParticleVectorProperty::from_node(child(node, "SpeedData")),
            intensity: ParticleScalarProperty::from_node(child(node, "IntensityData")),
            force: ParticleForceDefinition::from_node(child(node, "ForceData")),
            magnets,
        })
    }
}

fn constant_varying(value: f32) -> ParticleVarying {
    ParticleVarying {
        value,
        variance: 0.0,
    }
}

fn parse_trail_emission(node: &Node) -> Result<ParticleTrailEmission, ParticleEffectError> {
    match child_text(node, "TrailEmissionType")
        .as_deref()
        .unwrap_or("eEmitByLength")
    {
        "eEmitByLength" => Ok(ParticleTrailEmission::ByLength),
        "eEmitByTime" => Ok(ParticleTrailEmission::ByTime),
        value => unsupported("trail emission type", value),
    }
}

fn parse_trail_uv(node: &Node) -> Result<ParticleTrailUv, ParticleEffectError> {
    match child_text(node, "TrailUVType")
        .as_deref()
        .unwrap_or("eStretch")
    {
        "eStretch" => Ok(ParticleTrailUv::Stretch),
        "eFaceMap" => Ok(ParticleTrailUv::FaceMap),
        value => unsupported("trail UV type", value),
    }
}

fn parse_scalar_progression(node: &Node) -> ParticleScalarProgression {
    let keys = child(node, "Stages")
        .map(|stages| {
            stages
                .children
                .iter()
                .map(|stage| ParticleScalarKey {
                    alpha: number(stage, "Alpha", 0.0),
                    value: number(stage, "Value", 0.0),
                    variance: number(stage, "ValueVariance", 0.0),
                })
                .collect()
        })
        .unwrap_or_default();
    ParticleScalarProgression {
        keys,
        cycles: number(node, "Cycles", 0.0),
        looping: boolean(node, "Loop", false),
    }
}

fn parse_color_progression(node: &Node) -> ParticleColorProgression {
    let keys = child(node, "Stages")
        .map(|stages| {
            stages
                .children
                .iter()
                .map(|stage| ParticleColorKey {
                    alpha: number(stage, "Alpha", 0.0),
                    color: packed_color_child(Some(stage), "Color").unwrap_or([0.0; 4]),
                })
                .collect()
        })
        .unwrap_or_default();
    ParticleColorProgression {
        keys,
        cycles: number(node, "Cycles", 0.0),
        looping: boolean(node, "Loop", false),
    }
}

fn progression_span<T>(keys: &[T], alpha: f32, key_alpha: impl Fn(&T) -> f32) -> Option<(&T, &T)> {
    for pair in keys.windows(2) {
        if key_alpha(&pair[1]) >= alpha {
            return Some((&pair[0], &pair[1]));
        }
    }
    None
}

fn progression_alpha(alpha: f32, looping: bool, cycles: f32) -> f32 {
    let alpha = alpha.clamp(0.0, 1.0);
    if !looping {
        return alpha;
    }
    let cycles = if cycles > 0.000_01 { cycles } else { 1.0 };
    (alpha * cycles) % 1.0
}

fn varying_key(key: &ParticleScalarKey, signed_random: f32) -> f32 {
    sample_varying(key.value, key.variance, signed_random)
}

fn sample_varying(value: f32, variance: f32, signed_random: f32) -> f32 {
    value * signed_random.mul_add(variance, 1.0)
}

fn lerp_span(left_alpha: f32, right_alpha: f32, alpha: f32, left: f32, right: f32) -> f32 {
    let length = right_alpha - left_alpha;
    let local = if length.abs() > f32::EPSILON {
        (alpha - left_alpha) / length
    } else {
        0.0
    };
    left + (right - left) * local
}

fn leading_axis_vector(node: &Node, stem: &str) -> [f32; 3] {
    ["X", "Y", "Z"].map(|axis| number(node, &format!("{axis}{stem}"), 0.0))
}

fn trailing_axis_vector(node: &Node, stem: &str) -> [f32; 3] {
    ["X", "Y", "Z"].map(|axis| number(node, &format!("{stem}{axis}"), 0.0))
}

fn varying_children(node: &Node, stem: &str, default: f32) -> ParticleVarying {
    ParticleVarying::from_children(node, stem, &format!("{stem}Var"), default)
}

fn boolean(node: &Node, name: &str, default: bool) -> bool {
    child_text(node, name)
        .and_then(|value| parse_bool(&value))
        .unwrap_or(default)
}

fn number<T>(node: &Node, name: &str, default: T) -> T
where
    T: std::str::FromStr,
{
    child_text(node, name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}
