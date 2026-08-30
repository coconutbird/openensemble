#[derive(Clone, Copy, Default)]
pub(in crate::ugx) struct MeshVisibility<'a> {
    pub(in crate::ugx) only: &'a [String],
    pub(in crate::ugx) hidden: &'a [String],
    pub(in crate::ugx) section_overrides: &'a [Option<bool>],
}

impl MeshVisibility<'_> {
    pub(super) fn allows(self, section_index: usize, mesh_name: Option<&str>) -> bool {
        if self.section_overrides.get(section_index).copied().flatten() == Some(false) {
            return false;
        }
        let included = self.only.is_empty()
            || mesh_name.is_some_and(|name| {
                self.only
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(name))
            });
        included
            && !mesh_name.is_some_and(|name| {
                self.hidden
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(name))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::MeshVisibility;

    #[test]
    fn projects_exclusive_and_hidden_sim_masks() {
        let only = vec!["panel".to_owned()];
        let hidden = vec!["glass".to_owned()];
        let visibility = MeshVisibility {
            only: &only,
            hidden: &hidden,
            section_overrides: &[Some(true), Some(false)],
        };
        assert!(visibility.allows(0, Some("Panel")));
        assert!(!visibility.allows(0, Some("body")));
        assert!(!visibility.allows(0, None));
        assert!(!visibility.allows(1, Some("panel")));

        let hidden_only = MeshVisibility {
            only: &[],
            hidden: &hidden,
            section_overrides: &[],
        };
        assert!(hidden_only.allows(0, Some("body")));
        assert!(!hidden_only.allows(0, Some("GLASS")));
        assert!(hidden_only.allows(0, None));
    }
}
