//! The GSK cache (`hkpGskCache`) and the closest-point queries that use it.

use glam::Vec3;

use super::solver::Gsk;
use crate::collide::convex::{ConvexRef, inverse_rotate, rotate, transform_point};
use crate::math::Transform;

/// The features of the last GSK query of a pair (`hkpGskCache`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GskCache {
    /// The vertex ids of A, then the vertex ids of B (`m_vertices`).
    pub(crate) vertices: [u16; 4],
    /// The number of vertices of A (`m_dimA`).
    pub(crate) dim_a: u8,
    /// The number of vertices of B (`m_dimB`).
    pub(crate) dim_b: u8,
    /// The vertex count limit of A (`m_maxDimA`).
    pub(crate) max_dim_a: u8,
    /// The vertex count limit of B (`m_maxDimB`).
    pub(crate) max_dim_b: u8,
    /// The agent flags (`m_gskFlags`), for example the cylinder flags.
    pub(crate) flags: u8,
}

/// The result of a closest-point query (`0x140DCA870`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct ClosestPoint {
    /// The world unit normal from B to A.
    pub(crate) normal: Vec3,
    /// The distance of the surfaces (the core distance minus both convex radii).
    pub(crate) distance: f32,
    /// The world contact position on the surface of B, if the distance is below the tolerance.
    pub(crate) position: Option<Vec3>,
}

impl GskCache {
    /// Makes the cache of a new pair (`hkpGskCache::init`, `0x140DD8F80`).
    ///
    /// The start feature is a vertex pair: the support vertex of A toward vertex 0 of B, and
    /// the support vertex of B back toward that vertex of A.
    pub(crate) fn new(a: &ConvexRef<'_>, b: &ConvexRef<'_>, b_to_a: &Transform) -> Self {
        let first_a = a.vertex(0);
        let first_b = transform_point(b_to_a, b.vertex(0));
        let (support_a, id_a) = a.support(first_b - first_a);
        let (_, id_b) = b.support(-inverse_rotate(b_to_a, first_b - support_a));
        Self {
            vertices: [id_a, id_b, 0, 0],
            dim_a: 1,
            dim_b: 1,
            max_dim_a: a.max_dim(),
            max_dim_b: b.max_dim(),
            flags: 0,
        }
    }

    fn solver<'a>(&self, a: ConvexRef<'a>, b: ConvexRef<'a>, b_to_a: Transform) -> Gsk<'a> {
        Gsk::new(
            a,
            b,
            b_to_a,
            self.vertices,
            (self.dim_a, self.dim_b),
            (self.max_dim_a, self.max_dim_b),
        )
    }

    /// Writes the features of a query back (`0x140DCB180`). Each feature turns so that its
    /// largest id comes first; the cyclic order stays.
    fn store(&mut self, gsk: &Gsk<'_>) {
        self.dim_a = u8::try_from(gsk.dim_a).unwrap_or(1);
        self.dim_b = u8::try_from(gsk.dim_b).unwrap_or(1);
        let ids_a =
            rotate_largest_first(&gsk.a[..gsk.dim_a].iter().map(|v| v.id).collect::<Vec<_>>());
        let ids_b =
            rotate_largest_first(&gsk.b[..gsk.dim_b].iter().map(|v| v.id).collect::<Vec<_>>());
        for (slot, id) in self.vertices.iter_mut().zip(ids_a.iter().chain(&ids_b)) {
            *slot = *id;
        }
    }

    /// Returns the separating normal in world space with the surface distance in `w`, and the
    /// solver for the position (`0x140DC4F80` and the first part of `0x140DCA870`).
    fn run<'a>(
        &mut self,
        a: ConvexRef<'a>,
        b: ConvexRef<'a>,
        world_a: &Transform,
        world_b: &Transform,
    ) -> (Gsk<'a>, Vec3, f32, f32) {
        let b_to_a = world_a.inverse() * *world_b;
        let mut gsk = self.solver(a, b, b_to_a);
        let (normal, core_distance) = gsk.closest_feature();
        if gsk.feature_change {
            self.store(&gsk);
        }
        let distance = (core_distance - a.radius()) - b.radius();
        (gsk, rotate(world_a, normal), distance, core_distance)
    }

    /// Returns the separating normal of the shapes (`hkPredGskAgent3` sepNormal function,
    /// `0x140DC4F80`).
    pub(crate) fn separating_normal(
        &mut self,
        a: ConvexRef<'_>,
        b: ConvexRef<'_>,
        world_a: &Transform,
        world_b: &Transform,
    ) -> (Vec3, f32) {
        let (_, normal, distance, _) = self.run(a, b, world_a, world_b);
        (normal, distance)
    }

    /// Returns the closest point of the shapes (`0x140DCA870`). The position is `None` if the
    /// distance is at least `tolerance`.
    pub(crate) fn closest_point(
        &mut self,
        a: ConvexRef<'_>,
        b: ConvexRef<'_>,
        world_a: &Transform,
        world_b: &Transform,
        tolerance: f32,
    ) -> ClosestPoint {
        let (gsk, normal, distance, core_distance) = self.run(a, b, world_a, world_b);
        let position = (distance < tolerance).then(|| {
            let radius_b = b.radius();
            let local_normal = gsk.dir;
            let local = if gsk.dim_a == 1 {
                local_normal * (radius_b - core_distance) + gsk.a[0].pos
            } else if gsk.dim_b == 1 {
                local_normal * radius_b + gsk.b[0].pos
            } else {
                local_normal * (radius_b - core_distance) + gsk.closest_on_a
            };
            transform_point(world_a, local)
        });
        ClosestPoint {
            normal,
            distance,
            position,
        }
    }
}

/// Turns a feature so that its largest id comes first and keeps the cyclic order
/// (`0x140DCB180`).
fn rotate_largest_first(ids: &[u16]) -> Vec<u16> {
    let start = ids
        .iter()
        .enumerate()
        .fold(0, |best, (i, id)| if *id > ids[best] { i } else { best });
    ids[start..].iter().chain(&ids[..start]).copied().collect()
}

#[cfg(test)]
mod tests {
    use super::rotate_largest_first;

    #[test]
    fn the_export_turns_a_triangle_and_keeps_its_winding() {
        assert_eq!(rotate_largest_first(&[1, 7, 3]), [7, 3, 1]);
        assert_eq!(rotate_largest_first(&[1, 3, 7]), [7, 1, 3]);
        assert_eq!(rotate_largest_first(&[2, 9]), [9, 2]);
    }
}
