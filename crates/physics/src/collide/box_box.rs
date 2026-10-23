//! The box–box agent (`hkpBoxBoxAgent`, `hkpBoxBoxCollisionDetection`).
//!
//! Retail runs box pairs through `hkpBoxBoxAgent::processCollision` (`0x140D956C0`) and
//! through `hkBoxBoxAgent3::process` (`0x140DC6860`, registered for the shape types 6/6 by
//! `0x140DC6B80` from `0x140D94830`). Both build the same detection context and call
//! `hkpBoxBoxCollisionDetection::calcManifold` (`0x140DCF3F0`) with a persistent
//! `hkpBoxBoxManifold`. This module follows `calcManifold`:
//!
//! 1. Check each kept point again. Remove a point whose feature left the tolerance.
//! 2. Compare the manifold normal with the stored normal. Stop if the normal changed by less
//!    than the 0.95 threshold and the manifold is complete.
//! 3. Look for a separating axis beyond the closest kept distance. Stop if one exists.
//! 4. Find the feature pair of the axis with the largest separation and add its point.
//! 5. If the first normal is near a face normal of both boxes (0.98), add the face vertices.
//!
//! The binary has no `m_attemptToFindAllEdges` branch: the completion uses only the face
//! vertices and the edge count of `checkCompleteness` (`0x140DD2580`).
//!
//! The box–box agent3 has no separating normal function (`+80` of its agent3 functions is
//! null, `0x140DC6BE0`). So `hkAgentNnMachine_createAgent` (`0x1408A35B0`) gives its entry the
//! type 2, and the stream machine calls `calcManifold` on each step without a TIM.

mod detect;
#[cfg(test)]
mod tests;

use glam::Vec3;

use crate::collide::agent::{CdBody, CollisionInput};
use crate::collide::sink::ContactSink;
use crate::contact::ContactPoint;
use crate::shape::Shape;

use detect::{
    Context, EPS, Feature, Found, apply, dot3, index_u8, negative_bits, rotate, rotate_inverse,
    sign_bit, vertex_signs,
};

/// The most points in a box–box manifold.
const MAX_POINTS: usize = 8;

/// A stable manifold normal keeps a complete manifold (`0x3F733333`).
const NORMAL_STABLE: f32 = 0.95;

/// A kept point stays only if its normal is near the new manifold normal (`0x3F733333`).
const NORMAL_KEEP: f32 = 0.95;

/// The face completion needs a normal this near to a face axis of both boxes
/// (`0x3F7AE148`).
const FACE_AXIS: f32 = 0.98;

/// The part of the tolerance below the closest kept distance that a new axis must reach.
const SEARCH_MARGIN: f32 = 0.05;

/// The vertex flips that give the other vertices of a face (`0x141164768`).
const FACE_FLIPS: [u8; 7] = [0x50, 0x30, 0x60, 0x10, 0x20, 0x40, 0x00];

/// The scale of the normal packing (`0x14115D640`).
const PACK_SCALE: u32 = 0x42E8_BA2E;

/// The offset of the normal packing (`0x14115D630`).
const PACK_OFFSET: u32 = 0x4840_2020;

/// The scale of the normal unpacking.
const UNPACK_SCALE: f32 = 0.008_593_75;

/// One point of the manifold.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Slot {
    /// The feature pair.
    feature: Feature,
    /// The key of the point in the contact sink.
    key: u32,
    /// The current geometry in world space.
    contact: ContactPoint,
}

/// The persistent box–box manifold of one pair (`hkpBoxBoxManifold`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct BoxBoxAgent {
    /// The points in manifold order. A removal moves the last point into the gap.
    slots: Vec<Slot>,
    /// The number of face–vertex points (`m_faceVertexFeatureCount`).
    face_vertex_count: u8,
    /// `true` if the manifold has all points of the contact (`m_isComplete`).
    is_complete: bool,
    /// `true` if `packed_normal` is valid (`m_manifoldNormalInitialized`).
    normal_initialized: bool,
    /// The manifold normal in the space of B, packed to bytes.
    packed_normal: [u8; 3],
    /// The manifold normal in the space of A.
    normal: Vec3,
}

/// Packs a normal into bytes (`hkVector4Util::packNormalInto4Bytes`, `0x140D84540`).
fn pack(normal: Vec3) -> [u8; 3] {
    let byte = |value: f32| {
        let bits = (value * f32::from_bits(PACK_SCALE) + f32::from_bits(PACK_OFFSET)).to_bits();
        u8::try_from((bits >> 6) & 0xFF).unwrap_or(0)
    };
    [byte(normal.x), byte(normal.y), byte(normal.z)]
}

/// Unpacks a normal that [`pack`] made.
fn unpack(bytes: [u8; 3]) -> Vec3 {
    let value = |byte: u8| (f32::from(byte) - 128.0) * UNPACK_SCALE;
    Vec3::new(value(bytes[0]), value(bytes[1]), value(bytes[2]))
}

/// Returns the half extents and radius of a box.
fn box_of(shape: &Shape) -> (Vec3, f32) {
    match shape {
        Shape::Box {
            half_extents,
            radius,
        } => (*half_extents, *radius),
        _ => (Vec3::ZERO, 0.0),
    }
}

impl BoxBoxAgent {
    /// Makes the agent with an empty manifold.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Updates the points of two boxes. Both shapes must be [`crate::Shape::Box`].
    pub(crate) fn process(
        &mut self,
        a: &CdBody<'_>,
        b: &CdBody<'_>,
        input: &CollisionInput,
        sink: &mut ContactSink<'_>,
    ) {
        let (half_a, radius_a) = box_of(a.shape);
        let (half_b, radius_b) = box_of(b.shape);
        let mut ctx = Context::new(
            &a.transform,
            &b.transform,
            half_a,
            radius_a,
            half_b,
            radius_b,
            input.tolerance,
        );
        self.calc_manifold(&mut ctx, sink);
        self.write_kept_points(sink);
    }

    /// Writes the geometry of all points to the sink. Adds again a point that a listener
    /// removed.
    fn write_kept_points(&mut self, sink: &mut ContactSink<'_>) {
        let mut index = 0;
        while index < self.slots.len() {
            let slot = self.slots[index];
            if !sink.update_point(slot.key, slot.contact) {
                if let Some(key) = sink.add_point(slot.contact) {
                    self.slots[index].key = key;
                } else {
                    self.remove_slot(index);
                    continue;
                }
            }
            index += 1;
        }
    }

    /// Removes a slot from the manifold (`hkpBoxBoxManifold::removePoint`, `0x140DAD4C0`).
    fn remove_slot(&mut self, index: usize) {
        if self.slots[index].feature.is_face_vertex() {
            self.face_vertex_count -= 1;
        }
        self.slots.swap_remove(index);
        self.is_complete = false;
    }

    /// Removes a point from the sink and from the manifold.
    fn drop_point(&mut self, index: usize, sink: &mut ContactSink<'_>) {
        sink.remove_point(self.slots[index].key);
        self.remove_slot(index);
    }

    /// Returns `true` if the manifold has a feature.
    fn has(&self, feature: Feature) -> bool {
        self.slots.iter().any(|slot| slot.feature == feature)
    }

    /// `calcManifold` (`0x140DCF3F0`).
    fn calc_manifold(&mut self, ctx: &mut Context, sink: &mut ContactSink<'_>) {
        self.check_kept_points(ctx, sink);
        let normal_changed = if self.slots.len() >= 2 {
            if self.normal_initialized {
                let normal = rotate(&ctx.r, unpack(self.packed_normal));
                let changed = dot3(normal, self.normal) < NORMAL_STABLE;
                if !changed && self.is_complete {
                    return;
                }
                changed
            } else {
                self.packed_normal = pack(rotate_inverse(&ctx.r, self.normal));
                self.normal_initialized = true;
                true
            }
        } else {
            self.normal_initialized = false;
            true
        };
        let threshold = match self
            .slots
            .iter()
            .map(|slot| slot.contact.distance)
            .reduce(f32::min)
        {
            Some(closest) => closest - ctx.tolerance * SEARCH_MARGIN,
            None => ctx.tolerance,
        };
        if ctx.separated(threshold) {
            return;
        }
        let Some(found) = ctx.find_closest(|feature| self.has(feature)) else {
            return;
        };
        if normal_changed {
            self.update_normal(ctx, &found, sink);
        }
        let Some(point) = found.point else {
            return;
        };
        let feature = found.feature;
        if feature.is_face_vertex() {
            let position = if feature.kind > 2 {
                apply(&ctx.wa, ctx.wa_t, point.pos_a)
            } else {
                apply(&ctx.wb, ctx.wb_t, point.pos_b)
            };
            if self.is_duplicate(ctx, position) {
                return;
            }
        }
        if self.slots.len() >= MAX_POINTS {
            return;
        }
        let contact = match feature.kind {
            0..=2 => ContactPoint {
                position: apply(&ctx.wb, ctx.wb_t, point.pos_b),
                normal: ctx.wa[usize::from(feature.kind)] * -found.sign,
                distance: point.distance,
            },
            3..=6 => ContactPoint {
                position: apply(&ctx.wa, ctx.wa_t, point.pos_a),
                normal: ctx.wb[usize::from(feature.kind - 4)] * -found.sign,
                distance: point.distance,
            },
            _ => ContactPoint {
                position: apply(&ctx.wa, ctx.wa_t, point.pos_a),
                normal: rotate(&ctx.wa, point.edge_normal),
                distance: point.distance,
            },
        };
        if !self.add(feature, contact, sink) {
            return;
        }
        self.complete_faces(ctx, sink);
    }

    /// Checks all kept points again and removes the points that left the tolerance.
    fn check_kept_points(&mut self, ctx: &Context, sink: &mut ContactSink<'_>) {
        let mut index = 0;
        while index < self.slots.len() {
            let feature = self.slots[index].feature;
            let contact = match feature.kind {
                0..=2 => face_a_point(ctx, feature),
                3..=6 => face_b_point(ctx, feature),
                _ => ctx.edge_edge(feature).map(|edge| ContactPoint {
                    position: apply(&ctx.wa, ctx.wa_t, edge.pos_a),
                    normal: rotate(&ctx.wa, edge.normal),
                    distance: edge.distance,
                }),
            };
            if let Some(contact) = contact {
                self.slots[index].contact = contact;
                index += 1;
            } else {
                self.drop_point(index, sink);
            }
        }
    }

    /// Sets the manifold normal from the found feature and removes the points with another
    /// normal.
    fn update_normal(&mut self, ctx: &Context, found: &Found, sink: &mut ContactSink<'_>) {
        let kind = found.feature.kind;
        match kind {
            0..=2 => {
                self.normal = Vec3::ZERO;
                self.normal[usize::from(kind)] = -found.sign;
            }
            3..=6 => self.normal = ctx.r[usize::from(kind - 4)] * -found.sign,
            _ => {
                if let Some(point) = found.point {
                    self.normal = point.edge_normal;
                }
            }
        }
        if !self.normal_initialized {
            return;
        }
        let world = rotate(&ctx.wa, self.normal);
        let mut index = 0;
        while index < self.slots.len() {
            if dot3(self.slots[index].contact.normal, world) >= NORMAL_KEEP {
                index += 1;
            } else {
                self.drop_point(index, sink);
            }
        }
        if self.slots.len() >= 2 {
            self.packed_normal = pack(rotate_inverse(&ctx.r, self.normal));
            self.normal_initialized = true;
        } else {
            self.normal_initialized = false;
        }
    }

    /// Returns `true` if a face–vertex point is within the tolerance of `position`.
    fn is_duplicate(&self, ctx: &Context, position: Vec3) -> bool {
        let limit = ctx.tolerance * ctx.tolerance + EPS;
        self.slots.iter().any(|slot| {
            let d = position - slot.contact.position;
            slot.feature.is_face_vertex() && limit >= dot3(d, d)
        })
    }

    /// Adds a new point to the manifold and to the sink (`addPoint`, `0x140DAD460`, then
    /// `addContactPoint`).
    ///
    /// Returns `false` only if the normal of the point points against the previous point.
    /// Retail then stops `calcManifold`.
    fn add(&mut self, feature: Feature, contact: ContactPoint, sink: &mut ContactSink<'_>) -> bool {
        if self.slots.len() >= MAX_POINTS || self.has(feature) {
            return true;
        }
        if let Some(previous) = self.slots.last()
            && dot3(previous.contact.normal, contact.normal) <= 0.0
        {
            self.is_complete = false;
            return false;
        }
        let Some(key) = sink.add_point(contact) else {
            self.is_complete = false;
            return true;
        };
        self.slots.push(Slot {
            feature,
            key,
            contact,
        });
        if feature.is_face_vertex() {
            self.face_vertex_count += 1;
        }
        true
    }

    /// Adds the other vertices of the two faces if the contact is face to face. Then updates
    /// `is_complete`.
    fn complete_faces(&mut self, ctx: &Context, sink: &mut ContactSink<'_>) {
        let Some(first) = self.slots.first() else {
            return;
        };
        let n = first.contact.normal;
        let normal_b = rotate_inverse(&ctx.wb, n);
        let normal_a = -rotate_inverse(&ctx.wa, n);
        let axis_a = dominant_axis(normal_a.abs());
        let axis_b = dominant_axis(normal_b.abs());
        if normal_a.abs()[axis_a] < FACE_AXIS || normal_b.abs()[axis_b] < FACE_AXIS {
            return;
        }
        let mask_a = 0x10_u8 << axis_a;
        let mask_b = 0x10_u8 << axis_b;
        let base_b = Feature {
            kind: index_u8(axis_a),
            vertex: sign_bit(ctx.t[axis_a]) | negative_bits(normal_b),
        };
        let base_a = Feature {
            kind: index_u8(axis_b + 4),
            vertex: sign_bit(ctx.rt_t[axis_b]) | negative_bits(normal_a),
        };
        let closest = self
            .slots
            .iter()
            .fold(1.0_f32, |closest, slot| closest.min(slot.contact.distance));
        for flip in FACE_FLIPS.into_iter().filter(|flip| flip & mask_b == 0) {
            let feature = Feature {
                vertex: base_b.vertex ^ flip,
                ..base_b
            };
            if let Some(contact) = face_a_point(ctx, feature) {
                self.add_face_vertex(ctx, feature, contact, closest, sink);
            }
        }
        for flip in FACE_FLIPS.into_iter().filter(|flip| flip & mask_a == 0) {
            let feature = Feature {
                vertex: base_a.vertex ^ flip,
                ..base_a
            };
            if let Some(contact) = face_b_point(ctx, feature) {
                self.add_face_vertex(ctx, feature, contact, closest, sink);
            }
        }
        if self.face_vertex_count < 4 {
            self.check_completeness(mask_a, mask_b);
        } else {
            self.is_complete = true;
        }
    }

    /// Adds a face vertex of the face completion if it is new, not deeper than the closest
    /// point and not near another point.
    fn add_face_vertex(
        &mut self,
        ctx: &Context,
        feature: Feature,
        contact: ContactPoint,
        closest: f32,
        sink: &mut ContactSink<'_>,
    ) {
        if self.has(feature)
            || contact.distance < closest - EPS
            || self.is_duplicate(ctx, contact.position)
        {
            return;
        }
        self.add(feature, contact, sink);
    }

    /// Sets `is_complete` if each edge of both faces touches no point or two points
    /// (`checkCompleteness`, `0x140DD2580`).
    fn check_completeness(&mut self, mask_a: u8, mask_b: u8) {
        if self.slots.len() < 3 {
            return;
        }
        if self.face_vertex_count >= 4 {
            self.is_complete = true;
            return;
        }
        let axis_of = |mask: u8| match mask >> 4 {
            1 => 0,
            2 => 1,
            _ => 2,
        };
        let (axis_a, axis_b) = (axis_of(mask_a), axis_of(mask_b));
        let mut edges_a = 0_u32;
        let mut edges_b = 0_u32;
        for slot in &self.slots {
            let Feature { kind, vertex } = slot.feature;
            match kind {
                0..=2 => count_vertex_edges(&mut edges_b, vertex, axis_b),
                3..=6 => count_vertex_edges(&mut edges_a, vertex, axis_a),
                _ => {
                    count_edge(&mut edges_a, edge_code(kind));
                    count_edge(&mut edges_b, edge_code(vertex));
                }
            }
        }
        self.is_complete = edges_a & 0x55_5555 == 0 && edges_b & 0x55_5555 == 0;
    }
}

/// Returns the index of the largest component, in the retail tie order.
fn dominant_axis(v: Vec3) -> usize {
    let axis = usize::from(v.x <= v.y);
    if v.z > v[axis] { 2 } else { axis }
}

/// Counts one more point on an edge. Each counter has two bits and stops at 2.
fn count_edge(counters: &mut u32, code: u8) {
    let bit = 1_u32 << (2 * u32::from(code));
    if (bit << 1) & *counters == 0 {
        *counters += bit;
    }
}

/// Returns the edge code (0–11) of an edge byte: the axis in the low bits, the signs in
/// bits 4–6.
fn edge_code(byte: u8) -> u8 {
    let positive = |bit: u8| u8::from(byte & bit == 0);
    let (x, y, z) = (positive(0x10), positive(0x20), positive(0x40));
    match byte & 0xF {
        0 => y + 2 * z,
        1 => x + 2 * (z + 2),
        _ => x + 2 * (y + 4),
    }
}

/// Counts the edges of a face vertex that do not run along the face axis.
fn count_vertex_edges(counters: &mut u32, vertex: u8, face_axis: usize) {
    let positive = |bit: u8| u8::from(vertex & bit == 0);
    let (x, y, z) = (positive(0x10), positive(0x20), positive(0x40));
    if face_axis != 0 {
        count_edge(counters, y + 2 * z);
    }
    if face_axis != 1 {
        count_edge(counters, x + 2 * (z + 2));
    }
    if face_axis != 2 {
        count_edge(counters, x + 2 * (y + 4));
    }
}

/// Returns the point of a vertex of B on a face of A, or `None` if the vertex is outside the
/// tolerance box of A.
fn face_a_point(ctx: &Context, feature: Feature) -> Option<ContactPoint> {
    let k = usize::from(feature.kind);
    let sign = feature.sign();
    let vertex_b = vertex_signs(feature.vertex) * ctx.ext_b;
    let pos_a = apply(&ctx.r, ctx.t, vertex_b);
    if !(0..3).all(|i| pos_a[i].abs() < ctx.ext_a_tol[i]) {
        return None;
    }
    Some(ContactPoint {
        position: apply(&ctx.wb, ctx.wb_t, vertex_b),
        normal: ctx.wa[k] * -sign,
        distance: sign * pos_a[k] - ctx.ext_a[k],
    })
}

/// Returns the point of a vertex of A on a face of B, or `None` if the vertex is outside the
/// tolerance box of B.
fn face_b_point(ctx: &Context, feature: Feature) -> Option<ContactPoint> {
    let k = usize::from(feature.kind - 4);
    let sign = feature.sign();
    let vertex_a = vertex_signs(feature.vertex) * ctx.ext_a;
    let pos_b = rotate_inverse(&ctx.r, vertex_a - ctx.t);
    if !(0..3).all(|i| pos_b[i].abs() < ctx.ext_b_tol[i]) {
        return None;
    }
    Some(ContactPoint {
        position: apply(&ctx.wa, ctx.wa_t, vertex_a),
        normal: ctx.wb[k] * -sign,
        distance: -(sign * pos_b[k]) - ctx.ext_b[k],
    })
}
