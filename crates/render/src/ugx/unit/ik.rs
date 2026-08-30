//! Retail `GroundIK` and `SweetSpot` pose correction driven by read-only sim state.

use glam::{Mat4, Vec3};
use pipeline::database::hw1::ProtoObject;
use pipeline::database::hw1::visual::VisualTag;
use sim::{EntityId, World as SimWorld};

use super::animation_tracks::RenderedAnimationTrack;
use super::{RenderedUnitInstance, UnitAnimationEvent, UnitRenderer};

const TRANSITION_EPSILON: f32 = 1.0e-6;

#[derive(Clone, Debug, Default)]
pub(in crate::ugx) struct UnitIkProfile {
    ground: Vec<GroundIkProfile>,
    sweet_spot: Vec<SweetSpotIkProfile>,
}

impl UnitIkProfile {
    pub(in crate::ugx) fn from_proto(proto: &ProtoObject) -> Self {
        let ground = proto
            .ground_ik
            .iter()
            .filter(|node| !node.bone.trim().is_empty())
            .map(|node| GroundIkProfile {
                bone: node.bone.clone(),
                link_count: usize::from(node.link_count.unwrap_or_default()),
                maximum_range: finite_nonnegative(node.ik_range).unwrap_or_default(),
            })
            .collect();
        let sweet_spot = proto
            .sweet_spot_ik
            .iter()
            .filter(|node| !node.bone.trim().is_empty())
            .map(|node| SweetSpotIkProfile {
                bone: node.bone.clone(),
                link_count: usize::from(node.link_count.unwrap_or_default()),
            })
            .collect();
        Self { ground, sweet_spot }
    }
}

#[derive(Clone, Debug)]
struct GroundIkProfile {
    bone: String,
    link_count: usize,
    maximum_range: f32,
}

#[derive(Clone, Debug)]
struct SweetSpotIkProfile {
    bone: String,
    link_count: usize,
}

#[derive(Clone, Copy)]
pub(in crate::ugx) struct UnitAnimationFrame<'frame> {
    pub(in crate::ugx) simulation_position: Option<f32>,
    pub(in crate::ugx) presentation_phase: f32,
    pub(in crate::ugx) unit_transform: Mat4,
    pub(in crate::ugx) source_owner_id: u64,
    pub(in crate::ugx) world: &'frame SimWorld,
    pub(in crate::ugx) entity_id: EntityId,
    pub(in crate::ugx) movement_animation: Option<&'frame str>,
}

impl UnitAnimationFrame<'_> {
    fn movement_is_idle(&self) -> bool {
        self.movement_animation.is_none_or(|animation| {
            !["Walk", "Jog", "Run", "TurnLeft", "TurnRight"]
                .iter()
                .any(|kind| animation.eq_ignore_ascii_case(kind))
        })
    }

    fn sweet_spot_target(&self) -> Option<Vec3> {
        let unit = self.world.get_unit(self.entity_id)?;
        if let Some(position) = unit.combat.engaged_position() {
            return position.is_finite().then_some(position);
        }
        let target_id = unit.combat.engaged_target().or(unit.attack_target)?;
        self.world
            .entity_position(target_id)
            .filter(|position| position.is_finite())
    }
}

#[derive(Clone, Debug)]
struct GroundIkState {
    profile: GroundIkProfile,
    instance_index: Option<usize>,
    anchor: Vec3,
    target: Vec3,
    start: f32,
    end: f32,
    flags: GroundIkFlags,
}

#[derive(Clone, Copy, Debug, Default)]
struct GroundIkFlags(u8);

impl GroundIkFlags {
    const HAS_ANCHOR: u8 = 1 << 0;
    const ACTIVE: u8 = 1 << 1;
    const LOCK_COMPLETE: u8 = 1 << 2;
    const IDLE_TRANSITIONING: u8 = 1 << 3;
    const IDLE_LOCK_STARTED: u8 = 1 << 4;
    const INITIALIZED: u8 = 1 << 5;

    const fn contains(self, flag: u8) -> bool {
        self.0 & flag != 0
    }

    fn set(&mut self, flag: u8, enabled: bool) {
        if enabled {
            self.0 |= flag;
        } else {
            self.0 &= !flag;
        }
    }
}

impl GroundIkState {
    fn new(profile: GroundIkProfile, instance_index: Option<usize>) -> Self {
        Self {
            profile,
            instance_index,
            anchor: Vec3::ZERO,
            target: Vec3::ZERO,
            start: 0.0,
            end: 0.0,
            flags: GroundIkFlags::default(),
        }
    }

    fn prepare_frame(&mut self, movement_is_idle: bool) {
        if movement_is_idle {
            if !self.flags.contains(GroundIkFlags::IDLE_TRANSITIONING)
                && !self.flags.contains(GroundIkFlags::IDLE_LOCK_STARTED)
            {
                self.anchor = self.target;
                self.flags.set(GroundIkFlags::IDLE_TRANSITIONING, true);
            }
        } else {
            self.flags.set(GroundIkFlags::IDLE_TRANSITIONING, false);
            self.flags.set(GroundIkFlags::IDLE_LOCK_STARTED, false);
        }
    }

    fn lock_from_tag(&mut self, tag: &VisualTag, movement_is_idle: bool) {
        if movement_is_idle
            && self.flags.contains(GroundIkFlags::INITIALIZED)
            && !self.flags.contains(GroundIkFlags::IDLE_TRANSITIONING)
        {
            return;
        }
        let lock = tag.lock_to_ground.unwrap_or(false);
        let start = finite_unit(tag.position).unwrap_or_default();
        let end = finite_unit(tag.end).unwrap_or(start);
        if self.flags.contains(GroundIkFlags::HAS_ANCHOR) != lock {
            self.flags.set(GroundIkFlags::HAS_ANCHOR, lock);
            self.flags
                .set(GroundIkFlags::LOCK_COMPLETE, end - start <= 0.0);
        }
        self.anchor = self.target;
        self.start = start;
        self.end = end;
    }

    fn transition_factor(&mut self, position: f32, locking: bool) -> (f32, bool) {
        let was_complete = self.flags.contains(GroundIkFlags::LOCK_COMPLETE);
        let span = self.end - self.start;
        let mut factor = 1.0;
        if !was_complete
            && position >= self.start
            && position <= self.end
            && span > TRANSITION_EPSILON
        {
            let normalized = ((position - self.start) / span).clamp(0.0, 1.0);
            factor = if locking {
                normalized * normalized
            } else {
                1.0 - (normalized - 1.0) * (normalized - 1.0)
            };
            if locking && self.flags.contains(GroundIkFlags::IDLE_TRANSITIONING) {
                self.flags.set(GroundIkFlags::IDLE_LOCK_STARTED, true);
            }
        } else {
            self.flags.set(GroundIkFlags::LOCK_COMPLETE, true);
        }
        (factor, was_complete)
    }
}

#[derive(Clone, Debug)]
struct SweetSpotIkState {
    profile: SweetSpotIkProfile,
    instance_index: Option<usize>,
    target_world: Vec3,
    start: f32,
    sweet_spot: f32,
    end: f32,
    active: bool,
}

impl SweetSpotIkState {
    fn new(profile: SweetSpotIkProfile, instance_index: Option<usize>) -> Self {
        Self {
            profile,
            instance_index,
            target_world: Vec3::ZERO,
            start: 0.0,
            sweet_spot: 0.0,
            end: 0.0,
            active: false,
        }
    }

    fn activate(&mut self, tag: &VisualTag, target_world: Vec3) {
        let sweet_spot = finite_unit(tag.position).unwrap_or_default();
        let start = finite_unit(tag.start).unwrap_or(sweet_spot);
        let end = finite_unit(tag.end).unwrap_or(sweet_spot);
        self.target_world = target_world;
        self.start = start;
        self.sweet_spot = sweet_spot;
        self.end = end;
        self.active = true;
    }

    fn weight(&mut self, position: f32) -> Option<f32> {
        if !self.active || position < self.start || position > self.end {
            self.active = false;
            return None;
        }
        let weight = if position <= self.sweet_spot {
            unit_ratio(position - self.start, self.sweet_spot - self.start)
        } else {
            1.0 - unit_ratio(position - self.sweet_spot, self.end - self.sweet_spot)
        };
        Some(weight)
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct UnitIkRuntime {
    ground: Vec<GroundIkState>,
    sweet_spot: Vec<SweetSpotIkState>,
}

impl UnitIkRuntime {
    fn new(profile: UnitIkProfile, instances: &[RenderedUnitInstance]) -> Self {
        let resolve = |bone: &str| {
            instances
                .iter()
                .position(|instance| instance.model.bone_to_model(bone).is_some())
        };
        Self {
            ground: profile
                .ground
                .into_iter()
                .map(|node| {
                    let instance_index = resolve(&node.bone);
                    GroundIkState::new(node, instance_index)
                })
                .collect(),
            sweet_spot: profile
                .sweet_spot
                .into_iter()
                .map(|node| {
                    let instance_index = resolve(&node.bone);
                    SweetSpotIkState::new(node, instance_index)
                })
                .collect(),
        }
    }

    fn prepare_frame(&mut self, movement_is_idle: bool) {
        for node in &mut self.ground {
            node.prepare_frame(movement_is_idle);
        }
    }

    fn initialize_ground(
        &mut self,
        instances: &[RenderedUnitInstance],
        frame: &UnitAnimationFrame<'_>,
    ) {
        for node in &mut self.ground {
            if node.flags.contains(GroundIkFlags::INITIALIZED) {
                continue;
            }
            let Some(instance_index) = node.instance_index else {
                continue;
            };
            let Some(instance) = instances.get(instance_index) else {
                continue;
            };
            let Some(animated) = instance
                .model
                .posed_bone_to_model(&instance.pose, &node.profile.bone)
                .map(|matrix| matrix.w_axis.truncate())
            else {
                continue;
            };
            let instance_world = frame.unit_transform * instance.local_transform;
            let world_position = instance_world.transform_point3(animated);
            let height = frame
                .world
                .terrain_height(world_position, true)
                .unwrap_or(world_position.y);
            let mut grounded = world_position;
            grounded.y = height;
            let Some(target) = transform_point_inverse(instance_world, grounded) else {
                continue;
            };
            node.anchor = target;
            node.target = target;
            node.flags.set(GroundIkFlags::HAS_ANCHOR, true);
            node.flags.set(GroundIkFlags::ACTIVE, true);
            node.flags.set(GroundIkFlags::LOCK_COMPLETE, true);
            node.flags.set(GroundIkFlags::INITIALIZED, true);
        }
    }

    fn handle_tag(
        &mut self,
        tag: &VisualTag,
        frame: &UnitAnimationFrame<'_>,
        movement_is_idle: bool,
    ) -> bool {
        if tag.tag_type.eq_ignore_ascii_case("GroundIK") {
            if let Some(bone) = tag.to_bone.as_deref() {
                for node in &mut self.ground {
                    if node.profile.bone.eq_ignore_ascii_case(bone) {
                        node.lock_from_tag(tag, movement_is_idle);
                    }
                }
            }
            return true;
        }
        if tag.tag_type.eq_ignore_ascii_case("SweetSpot") {
            if let (Some(bone), Some(target)) = (tag.to_bone.as_deref(), frame.sweet_spot_target())
            {
                for node in &mut self.sweet_spot {
                    if node.profile.bone.eq_ignore_ascii_case(bone) {
                        node.activate(tag, target);
                    }
                }
            }
            return true;
        }
        false
    }

    fn apply_instance(
        &mut self,
        instance_index: usize,
        instance: &mut RenderedUnitInstance,
        frame: &UnitAnimationFrame<'_>,
        movement_position: f32,
        action_position: f32,
    ) {
        let animated_pose = instance.pose.clone();
        let instance_world = frame.unit_transform * instance.local_transform;
        for node in &mut self.ground {
            if node.instance_index != Some(instance_index)
                || !node.flags.contains(GroundIkFlags::ACTIVE)
                || sweet_spot_owns_bone(&self.sweet_spot, instance_index, &node.profile.bone)
            {
                continue;
            }
            let Some(animated) = instance
                .model
                .posed_bone_to_model(&animated_pose, &node.profile.bone)
                .map(|matrix| matrix.w_axis.truncate())
            else {
                continue;
            };
            let Some(target) = ground_target(
                node,
                animated,
                instance_world,
                frame.world,
                movement_position,
            ) else {
                continue;
            };
            instance.model.apply_ccd_ik(
                &mut instance.pose,
                &node.profile.bone,
                node.profile.link_count,
                target,
            );
        }
        for node in &mut self.sweet_spot {
            if node.instance_index != Some(instance_index) {
                continue;
            }
            let Some(weight) = node.weight(action_position) else {
                continue;
            };
            let Some(animated) = instance
                .model
                .posed_bone_to_model(&animated_pose, &node.profile.bone)
                .map(|matrix| matrix.w_axis.truncate())
            else {
                continue;
            };
            let Some(target) = transform_point_inverse(instance_world, node.target_world) else {
                continue;
            };
            instance.model.apply_ccd_ik(
                &mut instance.pose,
                &node.profile.bone,
                node.profile.link_count,
                animated.lerp(target, weight),
            );
        }
    }
}

impl UnitRenderer {
    pub(in crate::ugx) fn configure_ik(&mut self, profile: UnitIkProfile) {
        self.ik = UnitIkRuntime::new(profile, &self.instances);
    }

    pub(in crate::ugx) fn inherit_ik_from(&mut self, previous: &Self) {
        self.ik.clone_from(&previous.ik);
    }

    pub(in crate::ugx) fn update_animation(
        &mut self,
        queue: &wgpu::Queue,
        frame: UnitAnimationFrame<'_>,
    ) -> Vec<UnitAnimationEvent> {
        self.unit_transform = frame.unit_transform;
        let (action_position, movement_position, crossed) = sample_tracks(
            &mut self.instances,
            frame.simulation_position,
            frame.presentation_phase,
        );
        self.active_action_animation = self.instances.first().and_then(|instance| {
            instance
                .action_animation
                .active_animation_type()
                .map(str::to_owned)
        });
        self.active_movement_animation = self.instances.first().and_then(|instance| {
            instance
                .movement_animation
                .as_ref()
                .and_then(RenderedAnimationTrack::active_animation_type)
                .map(str::to_owned)
        });
        let movement_is_idle = frame.movement_is_idle();
        self.ik.prepare_frame(movement_is_idle);
        self.ik.initialize_ground(&self.instances, &frame);

        let mut presentation_tags = Vec::new();
        for (instance_index, tag) in crossed {
            if !self.ik.handle_tag(&tag, &frame, movement_is_idle) {
                presentation_tags.push((instance_index, tag));
            }
        }
        for (instance_index, instance) in self.instances.iter_mut().enumerate() {
            self.ik.apply_instance(
                instance_index,
                instance,
                &frame,
                movement_position.unwrap_or(action_position.unwrap_or_default()),
                action_position.unwrap_or_default(),
            );
        }
        let simulation_unit = frame.world.get_unit(frame.entity_id);
        if let Some(unit) = simulation_unit {
            for (bone, transform) in unit.combat.hardpoint_bone_transforms() {
                for instance in &mut self.instances {
                    if instance
                        .model
                        .apply_single_bone_ik(&mut instance.pose, bone, transform)
                    {
                        break;
                    }
                }
            }
        }
        for instance in &self.instances {
            instance
                .renderer
                .update_joints(queue, instance.pose.joint_matrices());
        }
        super::instance_attachment::update_transforms(&mut self.instances, |name| {
            simulation_unit.and_then(|unit| unit.combat.hardpoint_attachment_transform(name))
        });
        super::instance_attachment::update_anchors(&self.instances, &mut self.attachments);
        presentation_tags
            .into_iter()
            .filter_map(|(instance_index, tag)| {
                self.animation_event(instance_index, &tag, frame.source_owner_id)
            })
            .collect()
    }
}

fn sample_tracks(
    instances: &mut [RenderedUnitInstance],
    simulation_position: Option<f32>,
    presentation_phase: f32,
) -> (Option<f32>, Option<f32>, Vec<(usize, VisualTag)>) {
    let mut action_position = None;
    let mut movement_position = None;
    let mut crossed = Vec::new();
    for (instance_index, instance) in instances.iter_mut().enumerate() {
        let model = &instance.model;
        let action = instance.action_animation.sample(
            |bone| model.bind_local_transform(bone),
            simulation_position,
            presentation_phase,
            instance.visible,
        );
        let movement = instance.movement_animation.as_mut().map(|track| {
            track.sample(
                |bone| model.bind_local_transform(bone),
                None,
                presentation_phase,
                instance.visible,
            )
        });
        action_position = action_position.or(action.normalized_position);
        movement_position = movement_position.or_else(|| {
            movement
                .as_ref()
                .and_then(|sample| sample.normalized_position)
        });
        instance.pose = instance.model.pose_tracks(
            action.pose.as_ref(),
            movement.as_ref().and_then(|sample| sample.pose.as_ref()),
        );
        crossed.extend(
            action
                .crossed_tags
                .into_iter()
                .chain(movement.into_iter().flat_map(|sample| sample.crossed_tags))
                .map(|tag| (instance_index, tag)),
        );
    }
    (action_position, movement_position, crossed)
}

fn ground_target(
    node: &mut GroundIkState,
    animated: Vec3,
    instance_world: Mat4,
    world: &SimWorld,
    animation_position: f32,
) -> Option<Vec3> {
    let animated_world = instance_world.transform_point3(animated);
    let height = world
        .terrain_height(animated_world, true)
        .unwrap_or(animated_world.y);
    let mut animated_ground = animated_world;
    animated_ground.y = height;
    let anchor_world = instance_world.transform_point3(node.anchor);
    let has_anchor = node.flags.contains(GroundIkFlags::HAS_ANCHOR);
    let (factor, was_complete) = node.transition_factor(animation_position, has_anchor);
    let idle_transitioning = node.flags.contains(GroundIkFlags::IDLE_TRANSITIONING);
    let idle_lock_started = node.flags.contains(GroundIkFlags::IDLE_LOCK_STARTED);
    let target = if has_anchor {
        if idle_transitioning {
            if idle_lock_started {
                transform_point_inverse(instance_world, anchor_world.lerp(animated_ground, factor))?
            } else {
                node.anchor
            }
        } else {
            let mut target_world = animated_world;
            target_world.y = animated_world.y + (height - animated_world.y) * factor;
            transform_point_inverse(instance_world, target_world)?
        }
    } else if idle_transitioning {
        let mut goal = (anchor_world + animated_ground) * 0.5;
        let planar_distance = (goal - anchor_world).with_y(0.0).length();
        goal.y =
            (anchor_world.y + planar_distance).clamp(height, height + node.profile.maximum_range);
        transform_point_inverse(instance_world, anchor_world.lerp(goal, factor))?
    } else {
        let mut target = animated;
        target.y = node.anchor.y + (animated.y - node.anchor.y) * factor;
        target
    };
    if idle_lock_started && was_complete {
        node.flags.set(GroundIkFlags::IDLE_TRANSITIONING, false);
    }
    node.target = target;
    Some(target)
}

fn sweet_spot_owns_bone(nodes: &[SweetSpotIkState], instance_index: usize, bone: &str) -> bool {
    nodes.iter().any(|node| {
        node.active
            && node.instance_index == Some(instance_index)
            && node.profile.bone.eq_ignore_ascii_case(bone)
    })
}

fn transform_point_inverse(transform: Mat4, point: Vec3) -> Option<Vec3> {
    let determinant = transform.determinant();
    if !determinant.is_finite() || determinant.abs() <= f32::EPSILON {
        return None;
    }
    let transformed = transform.inverse().transform_point3(point);
    transformed.is_finite().then_some(transformed)
}

fn finite_nonnegative(value: Option<f32>) -> Option<f32> {
    value.filter(|value| value.is_finite() && *value >= 0.0)
}

fn finite_unit(value: Option<f32>) -> Option<f32> {
    value
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(0.0, 1.0))
}

fn unit_ratio(numerator: f32, denominator: f32) -> f32 {
    if denominator.abs() <= TRANSITION_EPSILON {
        1.0
    } else {
        (numerator / denominator).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use pipeline::database::hw1::ProtoObject;
    use pipeline::database::hw1::objects::{GroundIk, SweetSpotIk};

    use super::UnitIkProfile;

    #[test]
    fn profile_preserves_authored_bones_ranges_and_chain_lengths() {
        let profile = UnitIkProfile::from_proto(&ProtoObject {
            ground_ik: vec![GroundIk {
                bone: "foot".to_owned(),
                ik_range: Some(5.0),
                link_count: Some(2),
                ..GroundIk::default()
            }],
            sweet_spot_ik: vec![SweetSpotIk {
                bone: "tentacle".to_owned(),
                link_count: Some(8),
            }],
            ..ProtoObject::default()
        });

        assert_eq!(profile.ground.len(), 1);
        assert_eq!(profile.ground[0].bone, "foot");
        assert_eq!(profile.ground[0].link_count, 2);
        assert!((profile.ground[0].maximum_range - 5.0).abs() <= f32::EPSILON);
        assert_eq!(profile.sweet_spot.len(), 1);
        assert_eq!(profile.sweet_spot[0].link_count, 8);
    }
}
