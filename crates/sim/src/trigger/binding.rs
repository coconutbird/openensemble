//! Signature-to-variable bindings used by retail trigger conditions and effects.

use super::{TriggerScript, TriggerValue, VarId};

/// A variable bound to one authored signature slot.
///
/// Retail trigger code addresses parameters by one-based `SigID`, not by XML
/// child order. Keeping that ID prevents optional parameters from shifting the
/// remaining inputs and outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VarBinding {
    /// One-based slot in the condition or effect signature.
    pub signature_id: u16,
    /// Sparse trigger-script variable ID stored in that slot.
    pub variable_id: VarId,
}

impl VarBinding {
    /// Create a binding for an authored signature slot.
    #[must_use]
    pub const fn new(signature_id: u16, variable_id: VarId) -> Self {
        Self {
            signature_id,
            variable_id,
        }
    }
}

/// Add a binding at the next automatic signature slot.
///
/// The slot is the number of earlier bindings in `bindings` plus one. It stops at `u16::MAX`.
pub(super) fn push_next(bindings: &mut Vec<VarBinding>, variable_id: VarId) {
    let signature_id = u16::try_from(bindings.len() + 1).unwrap_or(u16::MAX);
    bindings.push(VarBinding::new(signature_id, variable_id));
}

/// The borrowed input and output bindings of one condition or effect.
#[derive(Debug, Clone, Copy)]
pub(super) struct Bindings<'a> {
    inputs: &'a [VarBinding],
    outputs: &'a [VarBinding],
}

impl<'a> Bindings<'a> {
    /// Make a view of the input and output bindings.
    pub(super) const fn new(inputs: &'a [VarBinding], outputs: &'a [VarBinding]) -> Self {
        Self { inputs, outputs }
    }

    /// Resolve a variable ID by its one-based signature slot.
    ///
    /// The search reads the inputs first, then the outputs. The first match wins.
    pub(super) fn variable_id(self, signature_id: u16) -> Option<VarId> {
        self.inputs
            .iter()
            .chain(self.outputs)
            .find(|binding| binding.signature_id == signature_id)
            .map(|binding| binding.variable_id)
    }

    /// Resolve the variable ID in a signature slot if the variable exists and is not null.
    pub(super) fn used_variable_id(
        self,
        script: &TriggerScript,
        signature_id: u16,
    ) -> Option<VarId> {
        let variable_id = self.variable_id(signature_id)?;
        script
            .get_variable(variable_id)
            .is_some_and(|variable| !variable.is_null())
            .then_some(variable_id)
    }

    /// Borrow the value of the variable in a signature slot.
    ///
    /// Return `None` if the slot has no binding, the variable is missing or the variable is
    /// null.
    pub(super) fn value(self, script: &TriggerScript, signature_id: u16) -> Option<&TriggerValue> {
        let variable_id = self.variable_id(signature_id)?;
        script
            .get_variable(variable_id)
            .filter(|variable| !variable.is_null())
            .map(|variable| &variable.value)
    }
}

#[cfg(test)]
mod tests;
