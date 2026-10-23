//! The contact manager of one body pair (`hkpSimpleConstraintContactMgr`).
//!
//! The agents add, update and remove the points of a manifold only through [`ContactSink`].
//! The bookkeeping follows the retail contact manager:
//!
//! - `addContactPoint` (`0x14088C150`) appends the new point at the end of the atom.
//! - `removeContactPoint` (`0x14088C4C0` → `0x14088BDE0` → `0x14088CE80`) moves the later
//!   points down, so the order of the other points stays the same.
//!
//! Retail gives each point the lowest free contact point id. The id value has no effect on the
//! simulation, so the sink gives out the keys in creation order and never reuses a key in a
//! manifold. Then a key that a contact listener removed can never name a newer point: the agent
//! sees that the key is gone and adds the point again on the next step, like retail does for a
//! rejected point.

use crate::contact::{ContactPoint, ContactProperties, Manifold, ManifoldPoint};

/// The largest number of points in one manifold. Retail maps the contact point ids to point
/// indices through a byte table in which `0xFF` marks a free id.
const MAX_POINTS: usize = 255;

/// Adds, updates and removes the points of one manifold for the collision agents.
///
/// A sink can be flipped for a symmetric agent (`hkpSymmetricAgent`, `0x140D9D590`). The agent
/// then works with its bodies swapped, and the sink turns each point back: the position moves
/// along the normal by the distance, and the normal changes its sign.
pub(crate) struct ContactSink<'a> {
    manifold: &'a mut Manifold,
    next_key: &'a mut u32,
    flipped: bool,
}

impl<'a> ContactSink<'a> {
    /// Makes a sink for a manifold, with the agent bodies in the manifold order. `next_key`
    /// is the key counter of the manifold.
    pub(crate) fn new(manifold: &'a mut Manifold, next_key: &'a mut u32) -> Self {
        Self {
            manifold,
            next_key,
            flipped: false,
        }
    }

    /// Returns a sink for an agent that has its two bodies swapped.
    pub(crate) fn flipped(&mut self) -> ContactSink<'_> {
        ContactSink {
            manifold: self.manifold,
            next_key: self.next_key,
            flipped: !self.flipped,
        }
    }

    /// Turns a point of the agent into a point of the manifold.
    fn orient(&self, contact: ContactPoint) -> ContactPoint {
        if self.flipped {
            ContactPoint {
                position: contact.position + contact.normal * contact.distance,
                normal: -contact.normal,
                distance: contact.distance,
            }
        } else {
            contact
        }
    }

    /// Adds a new point and returns its key (`addContactPoint`).
    ///
    /// The new point has default properties with `is_new` set. Returns `None` and adds nothing
    /// if the manifold is full, like the retail id `0xFFFF`.
    pub(crate) fn add_point(&mut self, contact: ContactPoint) -> Option<u32> {
        if self.manifold.points.len() >= MAX_POINTS {
            return None;
        }
        let key = *self.next_key;
        *self.next_key = key.wrapping_add(1);
        let contact = self.orient(contact);
        self.manifold.points.push(ManifoldPoint {
            key,
            contact,
            properties: ContactProperties {
                is_new: true,
                ..ContactProperties::default()
            },
        });
        Some(key)
    }

    /// Writes the new geometry of a point that the agent keeps. The solver data stays.
    ///
    /// Returns `false` if the point is gone because a contact listener removed it. The agent
    /// must then add the point again.
    pub(crate) fn update_point(&mut self, key: u32, contact: ContactPoint) -> bool {
        let contact = self.orient(contact);
        match self
            .manifold
            .points
            .iter_mut()
            .find(|point| point.key == key)
        {
            Some(point) => {
                point.contact = contact;
                true
            }
            None => false,
        }
    }

    /// Removes a point (`removeContactPoint`). The later points move down by one.
    pub(crate) fn remove_point(&mut self, key: u32) {
        if let Some(index) = self
            .manifold
            .points
            .iter()
            .position(|point| point.key == key)
        {
            self.manifold.points.remove(index);
        }
    }

    /// Returns the current geometry of a point as the agent sees it, or `None` for an unknown
    /// key.
    pub(crate) fn point(&self, key: u32) -> Option<ContactPoint> {
        self.manifold
            .points
            .iter()
            .find(|point| point.key == key)
            .map(|point| self.orient(point.contact))
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::ContactSink;
    use crate::body::BodyId;
    use crate::contact::{ContactPoint, Manifold, ManifoldId};

    fn contact(height: f32) -> ContactPoint {
        ContactPoint {
            position: Vec3::new(0.0, height, 0.0),
            normal: Vec3::Y,
            distance: 0.5,
        }
    }

    #[test]
    fn new_points_go_to_the_end_and_removal_keeps_the_order() {
        let mut manifold = Manifold::new(
            ManifoldId::from_raw(0),
            BodyId::from_raw(0),
            BodyId::from_raw(1),
        );
        let mut next_key = 0;
        let mut sink = ContactSink::new(&mut manifold, &mut next_key);
        assert_eq!(sink.add_point(contact(0.0)), Some(0));
        assert_eq!(sink.add_point(contact(1.0)), Some(1));
        assert_eq!(sink.add_point(contact(2.0)), Some(2));
        sink.remove_point(1);
        assert!(!sink.update_point(1, contact(5.0)));
        assert_eq!(sink.add_point(contact(3.0)), Some(3));
        let keys: Vec<u32> = manifold.points.iter().map(|point| point.key).collect();
        assert_eq!(keys, [0, 2, 3]);
        assert!(manifold.points.iter().all(|point| point.properties.is_new));
    }

    #[test]
    fn a_flipped_sink_moves_the_point_to_the_other_body_and_turns_the_normal() {
        let mut manifold = Manifold::new(
            ManifoldId::from_raw(0),
            BodyId::from_raw(0),
            BodyId::from_raw(1),
        );
        let mut next_key = 0;
        let mut sink = ContactSink::new(&mut manifold, &mut next_key);
        let key = sink.flipped().add_point(contact(1.0)).expect("room");
        assert_eq!(sink.flipped().point(key), Some(contact(1.0)));
        let stored = manifold.points[0].contact;
        assert_eq!(stored.position, Vec3::new(0.0, 1.5, 0.0));
        assert_eq!(stored.normal, -Vec3::Y);
        assert!((stored.distance - 0.5).abs() < f32::EPSILON);
    }
}
