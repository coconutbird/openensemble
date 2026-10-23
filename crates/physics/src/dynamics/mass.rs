//! Mass properties of shapes (`hkpInertiaTensorComputer`).
//!
//! Retail calls `hkpInertiaTensorComputer_computeShapeVolumeMassProperties` (`0x140DED8F0`,
//! Havok `hkpInertiaTensorComputer.cpp:942-1235`) for every body. The function assumes a
//! uniform density, sums the volumes of all leaf shapes and then scales the result to the mass.

use glam::{Mat3, Quat, Vec3};

use crate::math::Transform;
use crate::shape::Shape;

/// The mass, volume, centre of mass and inertia tensor of a shape (`hkpMassProperties`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MassProperties {
    /// The volume.
    pub(crate) volume: f32,
    /// The mass.
    pub(crate) mass: f32,
    /// The centre of mass in shape space.
    pub(crate) center_of_mass: Vec3,
    /// The inertia tensor about the centre of mass, in shape space.
    pub(crate) inertia: Mat3,
}

impl MassProperties {
    const ZERO: Self = Self {
        volume: 0.0,
        mass: 0.0,
        center_of_mass: Vec3::ZERO,
        inertia: Mat3::ZERO,
    };
}

/// Computes the volume mass properties of a shape with the given total mass
/// (`computeShapeVolumeMassProperties`).
///
/// Returns `None` if the shape has no volume, for example the heightfield.
#[must_use]
pub(crate) fn shape_volume_mass_properties(shape: &Shape, mass: f32) -> Option<MassProperties> {
    let mut properties = MassProperties::ZERO;
    add_shape(shape, &Transform::IDENTITY, &mut properties);
    if properties.volume == 0.0 {
        return None;
    }
    scale(mass / properties.volume, &mut properties);
    Some(properties)
}

/// Adds the uniform-density mass properties of `shape`, placed by `transform`, to `out`
/// (`computeRecursiveShapeVolumeMassProperties`).
fn add_shape(shape: &Shape, transform: &Transform, out: &mut MassProperties) {
    let leaf = match shape {
        Shape::Sphere { radius } => sphere(*radius, 1.0),
        Shape::Box { half_extents, .. } => Some(box_volume(*half_extents, 1.0)),
        Shape::ConvexVertices {
            vertices, planes, ..
        } => vertex_hull(vertices, planes, 1.0),
        Shape::Capsule { a, b, radius } => capsule(*a, *b, *radius, 1.0),
        Shape::Cylinder { a, b, radius, .. } => cylinder(*a, *b, *radius, 1.0),
        Shape::Transform {
            transform: child_transform,
            child,
        } => {
            add_shape(child, &(*transform * *child_transform), out);
            return;
        }
        Shape::List { children } => {
            for child in children {
                add_shape(child, transform, out);
            }
            return;
        }
        Shape::HeightField(_) => None,
    };
    let Some(mut leaf) = leaf else {
        return;
    };
    if leaf.volume == 0.0 {
        return;
    }
    // Rescale the leaf to density 1.
    scale(leaf.volume, &mut leaf);
    if let Some(combined) = combine(&[(*out, Transform::IDENTITY), (leaf, *transform)]) {
        *out = combined;
    }
}

/// Multiplies the mass and the inertia (not the volume) by `factor`.
fn scale(factor: f32, properties: &mut MassProperties) {
    properties.mass *= factor;
    properties.inertia *= factor;
}

/// `computeSphereVolumeMassProperties`.
fn sphere(radius: f32, mass: f32) -> Option<MassProperties> {
    if mass <= 0.0 || radius <= 0.0 {
        return None;
    }
    let k = mass * radius * radius * 0.4;
    Some(MassProperties {
        volume: 4.0 / 3.0 * std::f32::consts::PI * radius * radius * radius,
        mass,
        center_of_mass: Vec3::ZERO,
        inertia: Mat3::from_diagonal(Vec3::splat(k)),
    })
}

/// `computeBoxVolumeMassProperties`. The convex radius of the box is ignored.
fn box_volume(half_extents: Vec3, mass: f32) -> MassProperties {
    let Vec3 { x, y, z } = half_extents;
    let k = mass * (1.0 / 3.0);
    MassProperties {
        volume: x * y * z * 8.0,
        mass,
        center_of_mass: Vec3::ZERO,
        inertia: Mat3::from_diagonal(Vec3::new(
            (y * y + z * z) * k,
            (x * x + z * z) * k,
            (x * x + y * y) * k,
        )),
    }
}

/// The transform from the canonical capsule space (axis along +Z, centred) to shape space.
fn canonical_axis_transform(a: Vec3, b: Vec3) -> (Transform, f32) {
    let axis = b - a;
    let height = axis.length();
    let mut transform = Transform::IDENTITY;
    if height > 0.0 {
        let axis = axis / height;
        let cosine = axis.z;
        if cosine.abs() < 1.0 - 1e-5 {
            // `hkQuaternion(rotAxis, acos(cosine))` with the half-angle identities, so that no
            // platform `acos` enters the result.
            let rot_axis = Vec3::Z.cross(axis).normalize();
            let half_cos = f32::midpoint(1.0, cosine).sqrt();
            let half_sin = ((1.0 - cosine) * 0.5).sqrt();
            let rotation = Quat::from_xyzw(
                rot_axis.x * half_sin,
                rot_axis.y * half_sin,
                rot_axis.z * half_sin,
                half_cos,
            );
            transform.rotation = Mat3::from_quat(rotation);
        }
    }
    transform.translation = (a + b) * 0.5;
    (transform, height)
}

/// `computeCapsuleVolumeMassProperties`: a cylinder and two hemispheres.
fn capsule(a: Vec3, b: Vec3, radius: f32, mass: f32) -> Option<MassProperties> {
    if mass <= 0.0 || radius <= 0.0 {
        return None;
    }
    let (to_shape, height) = canonical_axis_transform(a, b);
    let pi = std::f32::consts::PI;
    let caps_volume = 4.0 / 3.0 * pi * radius * radius * radius;
    let cylinder_volume = pi * radius * radius * height;
    let total_volume = caps_volume + cylinder_volume;
    let caps_mass = mass * caps_volume / total_volume;
    let cylinder_mass = mass * cylinder_volume / total_volume;

    let cylinder = cylinder_element(radius, height, cylinder_mass, cylinder_volume);
    let mut elements = vec![(cylinder, to_shape)];
    for sign in [1.0_f32, -1.0] {
        let mut cap_transform = to_shape;
        cap_transform.translation =
            to_shape.rotation * Vec3::new(0.0, 0.0, sign * (height / 2.0)) + to_shape.translation;
        let center_of_mass = Vec3::new(0.0, 0.0, sign * (3.0 / 8.0 * radius));
        let half_sphere = sphere(radius, caps_mass)?;
        let mut inertia = half_sphere.inertia * 0.5;
        shift_inertia_to_com(center_of_mass, caps_mass * 0.5, &mut inertia);
        elements.push((
            MassProperties {
                volume: caps_volume * 0.5,
                mass: caps_mass * 0.5,
                center_of_mass,
                inertia,
            },
            cap_transform,
        ));
    }
    combine(&elements)
}

/// The solid cylinder of Graphics Gems III p. 142, along +Z and centred.
fn cylinder_element(radius: f32, height: f32, mass: f32, volume: f32) -> MassProperties {
    let side = radius * radius * 0.25 + (height / 2.0 * height / 2.0) / 3.0;
    let inertia = Mat3::from_diagonal(Vec3::new(side, side, radius * radius * 0.5)) * mass;
    MassProperties {
        volume,
        mass,
        center_of_mass: Vec3::ZERO,
        inertia,
    }
}

/// `computeCylinderVolumeMassProperties`. The convex radius of the cylinder is ignored.
fn cylinder(a: Vec3, b: Vec3, radius: f32, mass: f32) -> Option<MassProperties> {
    if mass <= 0.0 || radius <= 0.0 || (b - a).length() <= 0.0 {
        return None;
    }
    let (to_shape, height) = canonical_axis_transform(a, b);
    let volume = std::f32::consts::PI * radius * radius * height;
    combine(&[(cylinder_element(radius, height, mass, volume), to_shape)])
}

/// `combineMassProperties`: moves every element to a common space and adds them.
fn combine(elements: &[(MassProperties, Transform)]) -> Option<MassProperties> {
    let mut group_center = Vec3::ZERO;
    let mut group_mass = 0.0;
    let mut group_volume = 0.0;
    for (properties, transform) in elements {
        let center = transform.transform_point(properties.center_of_mass);
        group_center += center * properties.mass;
        group_mass += properties.mass;
        group_volume += properties.volume;
    }
    if group_mass <= 0.0 {
        return None;
    }
    group_center *= 1.0 / group_mass;
    let mut inertia = Mat3::ZERO;
    for (properties, transform) in elements {
        let rotation = transform.rotation;
        let mut rotated = rotation * properties.inertia * rotation.transpose();
        let shift = transform.transform_point(properties.center_of_mass) - group_center;
        shift_inertia_from_com(shift, properties.mass, &mut rotated);
        inertia += rotated;
    }
    Some(MassProperties {
        volume: group_volume,
        mass: group_mass,
        center_of_mass: group_center,
        inertia,
    })
}

/// `shiftInertiaToCom`.
fn shift_inertia_to_com(shift: Vec3, mass: f32, inertia: &mut Mat3) {
    shift_inertia(shift, -mass, inertia);
}

/// `shiftInertiaFromCom` (the parallel axis theorem).
fn shift_inertia_from_com(shift: Vec3, mass: f32, inertia: &mut Mat3) {
    shift_inertia(shift, mass, inertia);
}

fn shift_inertia(shift: Vec3, mass: f32, inertia: &mut Mat3) {
    let Vec3 { x, y, z } = shift;
    inertia.x_axis.x += mass * (y * y + z * z);
    inertia.y_axis.y += mass * (z * z + x * x);
    inertia.z_axis.z += mass * (x * x + y * y);
    let xy = inertia.y_axis.x - mass * x * y;
    let yz = inertia.z_axis.y - mass * y * z;
    let zx = inertia.x_axis.z - mass * z * x;
    inertia.y_axis.x = xy;
    inertia.x_axis.y = xy;
    inertia.z_axis.y = yz;
    inertia.y_axis.z = yz;
    inertia.x_axis.z = zx;
    inertia.z_axis.x = zx;
}

/// `computeVertexHullVolumeMassProperties`: the polyhedron mass properties of Mirtich (1996)
/// over the hull of the vertices, with the degenerate-axis guard of
/// `computeGeometryVolumeMassProperties`.
fn vertex_hull(vertices: &[Vec3], planes: &[glam::Vec4], mass: f32) -> Option<MassProperties> {
    if mass <= 0.0 || vertices.is_empty() {
        return None;
    }
    let triangles = hull_triangles(vertices, planes);
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(-f32::MAX);
    for vertex in vertices {
        min = vertex.min(min);
        max = vertex.max(max);
    }
    let center = (min + max) * 0.5;
    // Havok passes the full extents where the box function expects half extents.
    let mut aabb = box_volume(max - min, mass);
    aabb.center_of_mass = center;

    let integrals = VolumeIntegrals::compute(&triangles, -center);
    let volume = integrals.t0;
    if volume <= 0.0 {
        return Some(aabb);
    }
    let density = mass / volume;
    let (relative_center, mut inertia) = integrals.inertia(mass, density);
    let mut center_of_mass = relative_center + center;
    for axis in 0..3 {
        let limit = 0.1 * aabb.inertia.col(axis)[axis];
        if inertia.col(axis)[axis] < limit {
            inertia.col_mut(axis)[axis] = limit;
            center_of_mass[axis] = center[axis];
            for other in 0..3 {
                if other != axis {
                    let value = inertia.col(axis)[other].max(0.0);
                    inertia.col_mut(axis)[other] = value;
                    inertia.col_mut(other)[axis] = value;
                }
            }
        }
    }
    Some(MassProperties {
        volume,
        mass,
        center_of_mass,
        inertia,
    })
}

/// Builds outward-wound triangles of a convex hull from its vertices and face planes.
fn hull_triangles(vertices: &[Vec3], planes: &[glam::Vec4]) -> Vec<[Vec3; 3]> {
    let scale = vertices
        .iter()
        .fold(0.0_f32, |acc, vertex| acc.max(vertex.abs().max_element()));
    let tolerance = 1e-4 * scale + 1e-5;
    let mut triangles = Vec::new();
    for plane in planes {
        let normal = plane.truncate();
        let face: Vec<Vec3> = vertices
            .iter()
            .copied()
            .filter(|vertex| (normal.dot(*vertex) + plane.w).abs() <= tolerance)
            .collect();
        let ordered = order_around(&face, normal);
        for index in 1..ordered.len().saturating_sub(1) {
            triangles.push([ordered[0], ordered[index], ordered[index + 1]]);
        }
    }
    triangles
}

/// Orders the vertices of a convex face counter-clockwise around `normal`.
fn order_around(face: &[Vec3], normal: Vec3) -> Vec<Vec3> {
    let mut remaining: Vec<Vec3> = face.to_vec();
    let mut ordered = Vec::with_capacity(face.len());
    let Some(first) = remaining.pop() else {
        return ordered;
    };
    ordered.push(first);
    let mut current = first;
    while !remaining.is_empty() {
        // The next vertex leaves every other vertex on the left of the edge.
        let mut best = 0;
        for candidate in 1..remaining.len() {
            let edge = remaining[best] - current;
            let side = edge.cross(remaining[candidate] - current).dot(normal);
            if side < 0.0 {
                best = candidate;
            }
        }
        current = remaining.swap_remove(best);
        ordered.push(current);
    }
    ordered
}

/// The volume integrals of Mirtich's algorithm (`InternalInertiaTensorComputer`).
struct VolumeIntegrals {
    t0: f32,
    t1: [f32; 3],
    t2: [f32; 3],
    tp: [f32; 3],
}

/// The face integrals of one triangle.
struct FaceIntegrals {
    fa: f32,
    fb: f32,
    fc: f32,
    faa: f32,
    fbb: f32,
    fcc: f32,
    faaa: f32,
    fbbb: f32,
    fccc: f32,
    faab: f32,
    fbbc: f32,
    fcca: f32,
}

impl VolumeIntegrals {
    /// `compVolumeIntegrals`.
    fn compute(triangles: &[[Vec3; 3]], shift: Vec3) -> Self {
        let mut integrals = Self {
            t0: 0.0,
            t1: [0.0; 3],
            t2: [0.0; 3],
            tp: [0.0; 3],
        };
        for triangle in triangles {
            let verts = [
                triangle[0] + shift,
                triangle[1] + shift,
                triangle[2] + shift,
            ];
            let mut normal = (verts[1] - verts[0]).cross(verts[2] - verts[0]);
            let area = normal.length();
            if area <= 0.0 {
                continue;
            }
            normal *= 1.0 / area;
            let abs = normal.abs();
            let c = if abs.x > abs.y && abs.x > abs.z {
                0
            } else if abs.y > abs.z {
                1
            } else {
                2
            };
            let a = (c + 1) % 3;
            let b = (a + 1) % 3;
            let face = face_integrals(&verts, normal, a, b, c);
            let pick_x = if a == 0 {
                face.fa
            } else if b == 0 {
                face.fb
            } else {
                face.fc
            };
            integrals.t0 += normal.x * pick_x;
            integrals.t1[a] += normal[a] * face.faa;
            integrals.t1[b] += normal[b] * face.fbb;
            integrals.t1[c] += normal[c] * face.fcc;
            integrals.t2[a] += normal[a] * face.faaa;
            integrals.t2[b] += normal[b] * face.fbbb;
            integrals.t2[c] += normal[c] * face.fccc;
            integrals.tp[a] += normal[a] * face.faab;
            integrals.tp[b] += normal[b] * face.fbbc;
            integrals.tp[c] += normal[c] * face.fcca;
        }
        for axis in 0..3 {
            integrals.t1[axis] /= 2.0;
            integrals.t2[axis] /= 3.0;
            integrals.tp[axis] /= 2.0;
        }
        integrals
    }

    /// `computeInertialTensorInternal`: the centre of mass (relative to the shift) and the
    /// inertia tensor about it.
    fn inertia(&self, mass: f32, density: f32) -> (Vec3, Mat3) {
        let r = Vec3::new(
            self.t1[0] / self.t0,
            self.t1[1] / self.t0,
            self.t1[2] / self.t0,
        );
        let xx = density * (self.t2[1] + self.t2[2]) - mass * (r.y * r.y + r.z * r.z);
        let yy = density * (self.t2[2] + self.t2[0]) - mass * (r.z * r.z + r.x * r.x);
        let zz = density * (self.t2[0] + self.t2[1]) - mass * (r.x * r.x + r.y * r.y);
        let xy = -density * self.tp[0] + mass * r.x * r.y;
        let yz = -density * self.tp[1] + mass * r.y * r.z;
        let zx = -density * self.tp[2] + mass * r.z * r.x;
        let inertia = Mat3::from_cols(
            Vec3::new(xx, xy, zx),
            Vec3::new(xy, yy, yz),
            Vec3::new(zx, yz, zz),
        );
        (r, inertia)
    }
}

/// `compFaceIntegrals` with `compProjectionIntegrals` for one triangle.
#[expect(
    clippy::many_single_char_names,
    clippy::similar_names,
    reason = "the names follow Mirtich's published algorithm"
)]
fn face_integrals(v: &[Vec3; 3], n: Vec3, a: usize, b: usize, c: usize) -> FaceIntegrals {
    let mut p1 = 0.0;
    let mut pa = 0.0;
    let mut pb = 0.0;
    let mut paa = 0.0;
    let mut pab = 0.0;
    let mut pbb = 0.0;
    let mut paaa = 0.0;
    let mut paab = 0.0;
    let mut pabb = 0.0;
    let mut pbbb = 0.0;
    for i in 0..3 {
        let a0 = v[i][a];
        let b0 = v[i][b];
        let a1 = v[(i + 1) % 3][a];
        let b1 = v[(i + 1) % 3][b];
        let da = a1 - a0;
        let db = b1 - b0;
        let a0_2 = a0 * a0;
        let a0_3 = a0_2 * a0;
        let a0_4 = a0_3 * a0;
        let b0_2 = b0 * b0;
        let b0_3 = b0_2 * b0;
        let b0_4 = b0_3 * b0;
        let a1_2 = a1 * a1;
        let a1_3 = a1_2 * a1;
        let b1_2 = b1 * b1;
        let b1_3 = b1_2 * b1;
        let c1 = a1 + a0;
        let ca = a1 * c1 + a0_2;
        let caa = a1 * ca + a0_3;
        let caaa = a1 * caa + a0_4;
        let cb = b1 * (b1 + b0) + b0_2;
        let cbb = b1 * cb + b0_3;
        let cbbb = b1 * cbb + b0_4;
        let cab = 3.0 * a1_2 + 2.0 * a1 * a0 + a0_2;
        let kab = a1_2 + 2.0 * a1 * a0 + 3.0 * a0_2;
        let caab = a0 * cab + 4.0 * a1_3;
        let kaab = a1 * kab + 4.0 * a0_3;
        let cabb = 4.0 * b1_3 + 3.0 * b1_2 * b0 + 2.0 * b1 * b0_2 + b0_3;
        let kabb = b1_3 + 2.0 * b1_2 * b0 + 3.0 * b1 * b0_2 + 4.0 * b0_3;
        p1 += db * c1;
        pa += db * ca;
        paa += db * caa;
        paaa += db * caaa;
        pb += da * cb;
        pbb += da * cbb;
        pbbb += da * cbbb;
        pab += db * (b1 * cab + b0 * kab);
        paab += db * (b1 * caab + b0 * kaab);
        pabb += da * (a1 * cabb + a0 * kabb);
    }
    p1 /= 2.0;
    pa /= 6.0;
    paa /= 12.0;
    paaa /= 20.0;
    pb /= -6.0;
    pbb /= -12.0;
    pbbb /= -20.0;
    pab /= 24.0;
    paab /= 60.0;
    pabb /= -60.0;

    let w = -n.x * v[0].x - n.y * v[0].y - n.z * v[0].z;
    let k1 = 1.0 / n[c];
    let k2 = k1 * k1;
    let k3 = k2 * k1;
    let k4 = k3 * k1;
    let (na, nb) = (n[a], n[b]);
    FaceIntegrals {
        fa: k1 * pa,
        fb: k1 * pb,
        fc: -k2 * (na * pa + nb * pb + w * p1),
        faa: k1 * paa,
        fbb: k1 * pbb,
        fcc: k3
            * (na * na * paa
                + 2.0 * na * nb * pab
                + nb * nb * pbb
                + w * (2.0 * (na * pa + nb * pb) + w * p1)),
        faaa: k1 * paaa,
        fbbb: k1 * pbbb,
        fccc: -k4
            * (na * na * na * paaa
                + 3.0 * na * na * nb * paab
                + 3.0 * na * nb * nb * pabb
                + nb * nb * nb * pbbb
                + 3.0 * w * (na * na * paa + 2.0 * na * nb * pab + nb * nb * pbb)
                + w * w * (3.0 * (na * pa + nb * pb) + w * p1)),
        faab: k1 * paab,
        fbbc: -k2 * (na * pabb + nb * pbbb + w * pbb),
        fcca: k3
            * (na * na * paaa
                + 2.0 * na * nb * paab
                + nb * nb * pabb
                + w * (2.0 * (na * paa + nb * pab) + w * pa)),
    }
}

#[cfg(test)]
mod tests;
