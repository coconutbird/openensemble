//! Trigger effects - actions that modify game state.

mod ids;
mod lookup;

pub use ids::EffectType;

use super::{VarBinding, VarId};

/// An effect that modifies game state when executed.
#[derive(Debug, Clone)]
pub struct Effect {
    /// Unique ID within the parent trigger.
    pub id: i32,

    /// The type of effect to execute.
    pub effect_type: EffectType,

    /// Original effect DBID, including IDs not implemented by this build.
    pub raw_type: u16,

    /// Input signature slots and their sparse script-variable IDs.
    pub inputs: Vec<VarBinding>,

    /// Output signature slots and their sparse script-variable IDs.
    pub outputs: Vec<VarBinding>,

    /// Version for compatibility.
    pub version: u8,
}

impl Effect {
    /// Create a new effect.
    #[must_use]
    pub fn new(id: i32, effect_type: EffectType) -> Self {
        Self {
            id,
            effect_type,
            raw_type: effect_type as u16,
            inputs: Vec::new(),
            outputs: Vec::new(),
            version: 0,
        }
    }

    /// Add an input variable reference.
    #[must_use]
    pub fn with_input(mut self, var_id: VarId) -> Self {
        let signature_id = u16::try_from(self.inputs.len() + 1).unwrap_or(u16::MAX);
        self.inputs.push(VarBinding::new(signature_id, var_id));
        self
    }

    /// Add an input variable at an explicit retail signature slot.
    #[must_use]
    pub fn with_input_at(mut self, signature_id: u16, var_id: VarId) -> Self {
        self.inputs.push(VarBinding::new(signature_id, var_id));
        self
    }

    /// Add an output variable reference.
    #[must_use]
    pub fn with_output(mut self, var_id: VarId) -> Self {
        let signature_id = u16::try_from(self.outputs.len() + 1).unwrap_or(u16::MAX);
        self.outputs.push(VarBinding::new(signature_id, var_id));
        self
    }

    /// Add an output variable at an explicit retail signature slot.
    #[must_use]
    pub fn with_output_at(mut self, signature_id: u16, var_id: VarId) -> Self {
        self.outputs.push(VarBinding::new(signature_id, var_id));
        self
    }

    /// Resolve a variable ID by its one-based retail signature slot.
    #[must_use]
    pub fn variable_id(&self, signature_id: u16) -> Option<VarId> {
        self.inputs
            .iter()
            .chain(&self.outputs)
            .find(|binding| binding.signature_id == signature_id)
            .map(|binding| binding.variable_id)
    }
}
