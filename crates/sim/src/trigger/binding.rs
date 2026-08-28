//! Signature-to-variable bindings used by retail trigger conditions and effects.

use super::VarId;

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
