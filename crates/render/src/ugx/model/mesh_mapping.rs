//! Mapping cached UGX sections back to named Granny meshes.

use pipeline::ugx::{GrannyBone, UgxGeom};
use std::collections::HashSet;

pub(super) fn section_mesh_names(geometry: &UgxGeom) -> Vec<Option<String>> {
    if geometry.granny_meshes.is_empty() {
        return vec![None; geometry.sections.len()];
    }
    let mesh_bones = geometry
        .granny_meshes
        .iter()
        .map(|mesh| {
            mesh.bone_bindings
                .iter()
                .map(|binding| binding.bone_name.as_str())
                .collect::<HashSet<_>>()
        })
        .collect::<Vec<_>>();
    let mut usage = vec![0; geometry.granny_meshes.len()];
    (0..geometry.sections.len())
        .map(|section_index| {
            let section_bones = section_bone_names(geometry, section_index, &geometry.granny_bones);
            let mesh_index = best_mesh(&section_bones, &mesh_bones, &usage);
            if let Some(count) = usage.get_mut(mesh_index) {
                *count += 1;
            }
            geometry
                .granny_meshes
                .get(mesh_index)
                .map(|mesh| mesh.name.clone())
        })
        .collect()
}

fn section_bone_names(
    geometry: &UgxGeom,
    section_index: usize,
    bones: &[GrannyBone],
) -> HashSet<String> {
    let mut names = HashSet::new();
    let Some(section) = geometry.sections.get(section_index) else {
        return names;
    };
    if let Ok(vertices) = geometry.unpack_section_vertices(section_index) {
        for vertex in vertices {
            for influence in 0..4 {
                if vertex.bone_weights[influence] <= 0.0 {
                    continue;
                }
                let local = usize::from(vertex.bone_indices[influence]);
                let global = section
                    .bone_remap
                    .get(local)
                    .map_or(local, |index| usize::from(*index));
                if let Some(bone) = bones.get(global) {
                    names.insert(bone.name.clone());
                }
            }
        }
    }
    if names.is_empty()
        && let Ok(index) = usize::try_from(section.rigid_bone_index)
        && let Some(bone) = bones.get(index)
    {
        names.insert(bone.name.clone());
    }
    names
}

fn best_mesh(
    section_bones: &HashSet<String>,
    mesh_bones: &[HashSet<&str>],
    usage: &[usize],
) -> usize {
    if section_bones.is_empty() {
        return least_used(usage);
    }
    let mut best = None;
    for (index, candidate) in mesh_bones.iter().enumerate() {
        let overlap = section_bones
            .iter()
            .filter(|bone| candidate.contains(bone.as_str()))
            .count();
        let superset = overlap == section_bones.len();
        let score = (
            u8::from(superset),
            overlap,
            usize::MAX - usage.get(index).copied().unwrap_or_default(),
            usize::MAX - candidate.len(),
        );
        if best.is_none_or(|(_, best_score)| score > best_score) {
            best = Some((index, score));
        }
    }
    match best {
        Some((_, (_, 0, _, _))) | None => least_used(usage),
        Some((index, _)) => index,
    }
}

fn least_used(usage: &[usize]) -> usize {
    usage
        .iter()
        .enumerate()
        .min_by_key(|(_, count)| *count)
        .map_or(0, |(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_bone_match_wins_over_partial_overlap() {
        let section = HashSet::from(["body".to_owned(), "panel".to_owned()]);
        let meshes = vec![HashSet::from(["body"]), HashSet::from(["body", "panel"])];
        assert_eq!(best_mesh(&section, &meshes, &[0, 0]), 1);
    }

    #[test]
    fn empty_bone_sets_distribute_to_least_used_mesh() {
        assert_eq!(
            best_mesh(
                &HashSet::new(),
                &[HashSet::new(), HashSet::new(), HashSet::new()],
                &[2, 0, 1]
            ),
            1
        );
    }
}
