//! The support mapping of the convex shapes for the GSK agent (`hkpConvexShape`).
//!
//! Each convex shape has a core shape and a convex radius around it. GSK works on the core
//! shapes. A vertex of a core shape has a 16-bit vertex id. The id lets the agent find the
//! vertex again in a later step (`convertVertexIdsToVertices`).
//!
//! The rules follow the retail vtable slots (`+0x60` `getSupportingVertex`, `+0x68`
//! `convertVertexIdsToVertices`, `+0x78` `getFirstVertex`):
//!
//! | Shape | Support | Convert | Vertex ids |
//! |---|---|---|---|
//! | box | `0x140872B80` | `0x140872BC0` | the sign bits of the corner (bit 0 = negative X) |
//! | sphere | `0x140D91F60` | `0x140D91F70` | always 0, the centre |
//! | capsule | `0x140D934C0` | `0x140D93520` | 0 for `a`, 16 for `b` (byte offsets) |
//! | cylinder | `0x140D90C20` | `0x140D90EF0` | the virtual rim vertex code (see below) |
//! | convex vertices | `0x140D926C0` | `0x140D92850` | the vertex index, padded blocks of four |
//! | convex transform | `0x140E8A0E0` | `0x140E8A1E0` | the ids of the child |

use glam::Vec3;

use crate::math::Transform;
use crate::shape::{CylinderFrame, Shape};

/// The sign of each box corner, in the order of the retail table at `0x140F17920`.
const BOX_CORNER_SIGNS: [Vec3; 8] = [
    Vec3::new(1.0, 1.0, 1.0),
    Vec3::new(-1.0, 1.0, 1.0),
    Vec3::new(1.0, -1.0, 1.0),
    Vec3::new(-1.0, -1.0, 1.0),
    Vec3::new(1.0, 1.0, -1.0),
    Vec3::new(-1.0, 1.0, -1.0),
    Vec3::new(1.0, -1.0, -1.0),
    Vec3::new(-1.0, -1.0, -1.0),
];

/// The vertex id of the second capsule end (`m_vertexB` is 16 bytes after `m_vertexA`).
pub(crate) const CAPSULE_B_ID: u16 = 16;

/// `s_virtualTesselationParameter` (`0x1414A9AF0`). Retail never calls
/// `setNumberOfVirtualSideSegements` (`0x140D91CC0`), so the static value stays.
const CYLINDER_TESSELATION: f32 = 2.687_411_5;

/// `s_virtualTesselationParameterInv` (`0x1414A9AF4`).
const CYLINDER_TESSELATION_INV: f32 = 0.372_105_27;

/// `s_intRoundUpThreshold` (`0x1414A9AF8`) after the first cylinder constructor
/// (`0x140D90B20`): `1 - v`, where `v` is the first sum of `0.01` steps that truncates to 1.
const CYLINDER_ROUND_UP_THRESHOLD: f32 = f32::from_bits(3_156_464_768);

/// The constant that the cylinder support adds before the truncation (`0x1411F3C1C`).
const CYLINDER_ROUND_UP_OFFSET: f32 = 0.05;

/// The smallest squared length of the direction in the rim plane (`0x140D90C20`).
const CYLINDER_MIN_RIM_DIRECTION: f32 = 1.421_085_5e-14;

/// The largest value that the convex vertices support starts with (`0x14115F9B0`).
const CONVEX_VERTICES_START: f32 = f32::from_bits(0xFF7F_FFEE);

/// A convex shape for GSK: a shape of the body, or the capsule that the cylinder agent puts in
/// place of a cylinder.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ConvexRef<'a> {
    /// A convex shape of a body.
    Shape(&'a Shape),
    /// A temporary capsule (`hkpCapsuleShape` built by `hkPredGskCylinderAgent3`).
    Capsule {
        /// The first end.
        a: Vec3,
        /// The second end.
        b: Vec3,
        /// The radius.
        radius: f32,
    },
}

impl ConvexRef<'_> {
    /// Returns the convex radius (`hkpConvexShape::m_radius`, offset `+0x20`).
    pub(crate) fn radius(&self) -> f32 {
        match self {
            Self::Shape(shape) => shape_radius(shape),
            Self::Capsule { radius, .. } => *radius,
        }
    }

    /// Returns the GSK cache limit of the vertex count: the number of collision spheres
    /// (vtable slot `+0x48`) clipped to 1..3, or 15 for more than 3 (`hkpGskCache::init`,
    /// `0x140DD8F80`).
    pub(crate) fn max_dim(&self) -> u8 {
        let count = match self {
            Self::Shape(shape) => shape.num_collision_spheres(),
            Self::Capsule { .. } => 2,
        };
        match u8::try_from(count) {
            Ok(count) if count <= 3 => count,
            _ => 15,
        }
    }

    /// Returns the core vertex that is farthest along `direction`, with its id
    /// (`getSupportingVertex`).
    pub(crate) fn support(&self, direction: Vec3) -> (Vec3, u16) {
        match self {
            Self::Shape(shape) => shape_support(shape, direction),
            Self::Capsule { a, b, .. } => capsule_support(*a, *b, direction),
        }
    }

    /// Returns the core vertex of an id (`convertVertexIdsToVertices`).
    pub(crate) fn vertex(&self, id: u16) -> Vec3 {
        match self {
            Self::Shape(shape) => shape_vertex(shape, id),
            Self::Capsule { a, b, .. } => {
                if id == CAPSULE_B_ID {
                    *b
                } else {
                    *a
                }
            }
        }
    }
}

/// Returns `a · b` in the retail order: `(y + x) + z`.
pub(crate) fn dot(a: Vec3, b: Vec3) -> f32 {
    (a.y * b.y + a.x * b.x) + a.z * b.z
}

/// Returns the point `transform · point` in the retail order: `(y·c1 + x·c0) + (z·c2 + t)`.
pub(crate) fn transform_point(transform: &Transform, point: Vec3) -> Vec3 {
    let rotation = transform.rotation;
    (rotation.y_axis * point.y + rotation.x_axis * point.x)
        + (rotation.z_axis * point.z + transform.translation)
}

/// Returns `rotationᵀ · direction` in the retail order: `(c.x·d.x + c.y·d.y) + c.z·d.z`.
pub(crate) fn inverse_rotate(transform: &Transform, direction: Vec3) -> Vec3 {
    let rotation = transform.rotation;
    let row =
        |column: Vec3| (column.x * direction.x + column.y * direction.y) + column.z * direction.z;
    Vec3::new(
        row(rotation.x_axis),
        row(rotation.y_axis),
        row(rotation.z_axis),
    )
}

/// Returns the rotation of a direction in the retail order: `(y·c1 + x·c0) + z·c2`.
pub(crate) fn rotate(transform: &Transform, direction: Vec3) -> Vec3 {
    let rotation = transform.rotation;
    (rotation.y_axis * direction.y + rotation.x_axis * direction.x) + rotation.z_axis * direction.z
}

fn shape_radius(shape: &Shape) -> f32 {
    match shape {
        Shape::Box { radius, .. }
        | Shape::Sphere { radius }
        | Shape::Capsule { radius, .. }
        | Shape::ConvexVertices { radius, .. } => *radius,
        Shape::Cylinder { convex_radius, .. } => *convex_radius,
        // The convex transform shape copies the radius of its child.
        Shape::Transform { child, .. } => shape_radius(child),
        Shape::List { .. } | Shape::HeightField(_) => 0.0,
    }
}

fn shape_support(shape: &Shape, direction: Vec3) -> (Vec3, u16) {
    match shape {
        Shape::Box { half_extents, .. } => box_support(*half_extents, direction),
        Shape::Capsule { a, b, .. } => capsule_support(*a, *b, direction),
        Shape::Cylinder { a, b, radius, .. } => cylinder_support(*a, *b, *radius, direction),
        Shape::ConvexVertices { vertices, .. } => convex_vertices_support(vertices, direction),
        Shape::Transform { transform, child } => {
            let (vertex, id) = shape_support(child, inverse_rotate(transform, direction));
            (transform_point(transform, vertex), id)
        }
        Shape::Sphere { .. } | Shape::List { .. } | Shape::HeightField(_) => (Vec3::ZERO, 0),
    }
}

fn shape_vertex(shape: &Shape, id: u16) -> Vec3 {
    match shape {
        Shape::Box { half_extents, .. } => BOX_CORNER_SIGNS[usize::from(id & 7)] * *half_extents,
        Shape::Capsule { a, b, .. } => {
            if id == CAPSULE_B_ID {
                *b
            } else {
                *a
            }
        }
        Shape::Cylinder { a, b, radius, .. } => cylinder_vertex(*a, *b, *radius, id),
        Shape::ConvexVertices { vertices, .. } => padded_vertex(vertices, usize::from(id)),
        Shape::Transform { transform, child } => {
            transform_point(transform, shape_vertex(child, id))
        }
        Shape::Sphere { .. } | Shape::List { .. } | Shape::HeightField(_) => Vec3::ZERO,
    }
}

/// Box support (`0x140872B80`): the half extents with the sign bits of the direction.
fn box_support(half_extents: Vec3, direction: Vec3) -> (Vec3, u16) {
    let signed = |extent: f32, d: f32| {
        if d.is_sign_negative() {
            -extent
        } else {
            extent
        }
    };
    let vertex = Vec3::new(
        signed(half_extents.x, direction.x),
        signed(half_extents.y, direction.y),
        signed(half_extents.z, direction.z),
    );
    let id =
        u16::from(vertex.x < 0.0) | u16::from(vertex.y < 0.0) << 1 | u16::from(vertex.z < 0.0) << 2;
    (vertex, id)
}

/// Capsule support (`0x140D934C0`): `b` if `(b - a) · direction >= 0`, else `a`.
fn capsule_support(a: Vec3, b: Vec3, direction: Vec3) -> (Vec3, u16) {
    if dot(b - a, direction) >= 0.0 {
        (b, CAPSULE_B_ID)
    } else {
        (a, 0)
    }
}

/// Cylinder support (`0x140D90C20`) with the virtual rim vertices.
///
/// The id has the bits of `hkpCylinderShape::VertexIdEncoding`: bit 7 for the cap at `a`, bit 6
/// for a positive sine, bit 5 for a positive cosine, bit 4 if the cosine is the larger one, and
/// the tessellation step in bits 0 to 3.
fn cylinder_support(a: Vec3, b: Vec3, radius: f32, direction: Vec3) -> (Vec3, u16) {
    let frame = CylinderFrame::new(a, b);
    let cosine = dot(frame.perpendicular1, direction);
    let sine = dot(frame.perpendicular2, direction);
    let length_squared = sine * sine + cosine * cosine;
    let (sine, cosine) = if length_squared < CYLINDER_MIN_RIM_DIRECTION {
        (0.0, 1.0)
    } else {
        let inverse = 1.0 / length_squared.sqrt();
        (sine * inverse, cosine * inverse)
    };
    let cosine_larger = cosine.abs() >= sine.abs();
    let smaller = cosine.abs().min(sine.abs());
    let base_a = dot(direction, b - a) <= 0.0;
    let quadrant = u16::from(cosine >= 0.0) + 2 * (u16::from(sine >= 0.0) + 2 * u16::from(base_a));
    // Truncation toward zero, like `cvttss2si`.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the retail code truncates a small non-negative float to an integer"
    )]
    let step = (CYLINDER_TESSELATION * smaller - CYLINDER_ROUND_UP_THRESHOLD
        + CYLINDER_ROUND_UP_OFFSET) as u16;
    let id = step + 16 * (u16::from(cosine_larger) + 2 * quadrant);
    (cylinder_vertex_with_frame(a, b, radius, &frame, id), id)
}

/// Cylinder vertex of an id (`0x140D90EF0`).
fn cylinder_vertex(a: Vec3, b: Vec3, radius: f32, id: u16) -> Vec3 {
    cylinder_vertex_with_frame(a, b, radius, &CylinderFrame::new(a, b), id)
}

fn cylinder_vertex_with_frame(
    a: Vec3,
    b: Vec3,
    radius: f32,
    frame: &CylinderFrame,
    id: u16,
) -> Vec3 {
    let small = (f32::from(id & 0xF) + 0.5) * CYLINDER_TESSELATION_INV;
    let large = (1.0 - small * small).sqrt();
    let (mut along2, mut along1) = if id & 0x10 == 0 {
        (large, small)
    } else {
        (small, large)
    };
    if id & 0x40 == 0 {
        along2 = -along2;
    }
    if id & 0x20 == 0 {
        along1 = -along1;
    }
    let base = if id & 0x80 != 0 { a } else { b };
    (frame.perpendicular2 * along2 + frame.perpendicular1 * along1) * radius + base
}

/// Returns vertex `index` of the padded vertex blocks: indices past the end give the last
/// vertex (`0x140D92F70` fills the last block with copies of the last vertex).
fn padded_vertex(vertices: &[Vec3], index: usize) -> Vec3 {
    vertices
        .get(index)
        .or_else(|| vertices.last())
        .copied()
        .unwrap_or(Vec3::ZERO)
}

/// Convex vertices support (`0x140D926C0`).
///
/// Retail stores the vertices in blocks of four. Each of the four lanes keeps its first best
/// vertex. Then the lanes reduce in pairs, and a tie goes to the later lane.
fn convex_vertices_support(vertices: &[Vec3], direction: Vec3) -> (Vec3, u16) {
    let blocks = vertices.len().div_ceil(4);
    let mut best = [CONVEX_VERTICES_START; 4];
    let mut index = [0, 1, 2, 3];
    for block in 0..blocks {
        for lane in 0..4 {
            let candidate = 4 * block + lane;
            let vertex = padded_vertex(vertices, candidate);
            let projection =
                (vertex.y * direction.y + vertex.x * direction.x) + vertex.z * direction.z;
            if best[lane] < projection {
                best[lane] = projection;
                index[lane] = candidate;
            }
        }
    }
    let high = if best[2] <= best[3] { 3 } else { 2 };
    let low = usize::from(best[0] <= best[1]);
    let lane = if best[low] > best[high] { low } else { high };
    let chosen = index[lane];
    (
        padded_vertex(vertices, chosen),
        u16::try_from(chosen).unwrap_or(u16::MAX),
    )
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::{CYLINDER_ROUND_UP_THRESHOLD, ConvexRef};
    use crate::shape::Shape;

    #[test]
    fn the_round_up_threshold_matches_the_constructor_loop() {
        let mut sum = 0.0_f32;
        loop {
            #[expect(clippy::cast_possible_truncation, reason = "the retail loop truncates")]
            let whole = sum as i32;
            if whole != 0 {
                break;
            }
            sum += 0.01;
            if sum >= 1.1 {
                sum = 1.0;
                break;
            }
        }
        assert_eq!((1.0 - sum).to_bits(), CYLINDER_ROUND_UP_THRESHOLD.to_bits());
    }

    #[test]
    fn box_ids_are_the_corner_sign_bits_and_convert_back() {
        let shape = Shape::Box {
            half_extents: Vec3::new(1.0, 2.0, 3.0),
            radius: 0.05,
        };
        let convex = ConvexRef::Shape(&shape);
        let (vertex, id) = convex.support(Vec3::new(-1.0, 0.5, -0.1));
        assert_eq!(id, 0b101);
        assert_eq!(vertex, Vec3::new(-1.0, 2.0, -3.0));
        assert_eq!(convex.vertex(id), vertex);
    }

    #[test]
    fn the_padded_vertex_wins_a_tie_with_the_last_vertex() {
        let vertices: Vec<Vec3> = (0..6_u8)
            .map(|i| Vec3::new(f32::from(i), 0.0, 0.0))
            .collect();
        let shape = Shape::ConvexVertices {
            vertices,
            planes: Vec::new(),
            radius: 0.05,
        };
        let (vertex, id) = ConvexRef::Shape(&shape).support(Vec3::X);
        assert_eq!(vertex, Vec3::new(5.0, 0.0, 0.0));
        assert_eq!(id, 7);
    }

    #[test]
    fn cylinder_support_ids_convert_to_the_same_rim_vertex() {
        let shape = Shape::Cylinder {
            a: Vec3::new(0.0, -1.0, 0.0),
            b: Vec3::new(0.0, 1.0, 0.0),
            radius: 0.5,
            convex_radius: 0.05,
        };
        let convex = ConvexRef::Shape(&shape);
        for direction in [
            Vec3::new(1.0, -0.2, 0.3),
            Vec3::new(-0.3, 1.0, 0.8),
            Vec3::new(0.0, -1.0, 0.0),
        ] {
            let (vertex, id) = convex.support(direction);
            assert_eq!(convex.vertex(id), vertex);
            assert!(((vertex - Vec3::new(0.0, vertex.y, 0.0)).length() - 0.5).abs() < 1e-5);
            assert!((vertex.y.abs() - 1.0).abs() < 1e-6);
            assert_eq!(vertex.y < 0.0, id & 0x80 != 0);
        }
    }
}
