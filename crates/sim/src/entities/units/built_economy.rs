//! Economy contributions active only while a retail object remains built.

use super::Unit;
use crate::sync::SyncChecksum;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct BuiltEconomyState {
    resource: Option<BuiltContribution>,
    rate: Option<BuiltContribution>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct BuiltContribution {
    id: u16,
    amount: f32,
}

impl BuiltEconomyState {
    pub(crate) fn new(resource: Option<(usize, f32)>, rate: Option<(usize, f32)>) -> Self {
        Self {
            resource: contribution(resource),
            rate: contribution(rate),
        }
    }

    pub(crate) fn resource(self) -> Option<(usize, f32)> {
        self.resource
            .map(|value| (usize::from(value.id), value.amount))
    }

    pub(crate) fn rate(self) -> Option<(usize, f32)> {
        self.rate.map(|value| (usize::from(value.id), value.amount))
    }

    pub(crate) const fn is_empty(self) -> bool {
        self.resource.is_none() && self.rate.is_none()
    }

    pub(crate) fn hash_state(self, checksum: &mut SyncChecksum) {
        hash_contribution(checksum, self.resource);
        hash_contribution(checksum, self.rate);
    }
}

impl Unit {
    pub(crate) fn take_built_economy_state(&mut self) -> BuiltEconomyState {
        std::mem::take(&mut self.built_economy)
    }
}

fn contribution(value: Option<(usize, f32)>) -> Option<BuiltContribution> {
    let (id, amount) = value?;
    if !amount.is_finite() || amount == 0.0 {
        return None;
    }
    Some(BuiltContribution {
        id: u16::try_from(id).ok()?,
        amount,
    })
}

fn hash_contribution(checksum: &mut SyncChecksum, contribution: Option<BuiltContribution>) {
    if let Some(contribution) = contribution {
        checksum.hash_u32(u32::from(contribution.id));
        checksum.hash_f32(contribution.amount);
    } else {
        checksum.hash_u32(u32::MAX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_and_zero_contributions_do_not_become_cleanup_state() {
        let state = BuiltEconomyState::new(Some((0, 0.0)), Some((1, f32::NAN)));
        assert!(state.is_empty());
    }
}
