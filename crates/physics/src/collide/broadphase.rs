//! The broadphase: the pairs of bodies whose boxes overlap.
//!
//! Retail uses the Havok 3-axis sweep (`St3AxisSweep` in `0x140883BD0`). Its pair order depends
//! on the internal handle order, which a rebuild cannot follow. This sweep gives the same set of
//! pairs in `(BodyId, BodyId)` order.

use std::cmp::Ordering;

use crate::body::BodyId;
use crate::shape::Aabb;

/// Returns each pair of overlapping boxes once, as `(lower id, higher id)`, sorted.
pub(crate) fn overlapping_pairs(boxes: &[(BodyId, Aabb)]) -> Vec<(BodyId, BodyId)> {
    let mut order: Vec<usize> = (0..boxes.len()).collect();
    order.sort_by(|&i, &j| {
        let (id_i, box_i) = &boxes[i];
        let (id_j, box_j) = &boxes[j];
        box_i
            .min
            .x
            .total_cmp(&box_j.min.x)
            .then_with(|| id_i.cmp(id_j))
    });
    let mut pairs = Vec::new();
    for (position, &i) in order.iter().enumerate() {
        let (id_i, box_i) = &boxes[i];
        for &j in &order[position + 1..] {
            let (id_j, box_j) = &boxes[j];
            if box_j.min.x.total_cmp(&box_i.max.x) == Ordering::Greater {
                break;
            }
            if box_i.overlaps(box_j) {
                pairs.push(if id_i < id_j {
                    (*id_i, *id_j)
                } else {
                    (*id_j, *id_i)
                });
            }
        }
    }
    pairs.sort_unstable();
    pairs.dedup();
    pairs
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::overlapping_pairs;
    use crate::body::BodyId;
    use crate::shape::Aabb;

    fn cube(id: u32, x: f32) -> (BodyId, Aabb) {
        (
            BodyId::from_raw(id),
            Aabb::from_center_half_extents(Vec3::new(x, 0.0, 0.0), Vec3::ONE),
        )
    }

    #[test]
    fn pairs_come_sorted_by_body_id_whatever_the_box_order() {
        let boxes = [cube(5, 0.0), cube(2, 1.5), cube(9, 3.0), cube(1, 10.0)];
        let pairs = overlapping_pairs(&boxes);
        let raw: Vec<(u32, u32)> = pairs.iter().map(|(a, b)| (a.raw(), b.raw())).collect();
        assert_eq!(raw, [(2, 5), (2, 9)]);
    }

    #[test]
    fn touching_boxes_overlap() {
        let pairs = overlapping_pairs(&[cube(0, 0.0), cube(1, 2.0)]);
        assert_eq!(pairs.len(), 1);
    }
}
