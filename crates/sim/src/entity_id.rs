//! Entity IDs matching the vanilla Halo Wars `BEntityID` layout.
//!
//! IDA analysis of `BObjectManager` shows that IDs are packed as:
//!
//! ```text
//! 31             28 27                    16 15                 0
//! +----------------+------------------------+--------------------+
//! | class (4 bits) | generation (12 bits)   | pool index (16)   |
//! +----------------+------------------------+--------------------+
//! ```
//!
//! Entity destruction increments the generation before the pool slot is reused,
//! allowing lookups to reject stale IDs.

const CLASS_SHIFT: u32 = 28;
const GENERATION_SHIFT: u32 = 16;
const INDEX_MASK: u32 = 0x0000_FFFF;
const GENERATION_MASK: u32 = 0x0FFF;

/// Entity class encoded in the high four bits of an [`EntityId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum EntityClass {
    Object = 0,
    Unit = 1,
    Squad = 2,
    Dopple = 3,
    Projectile = 4,
    Platoon = 5,
    Army = 6,
}

impl EntityClass {
    /// Decode a vanilla entity class value.
    #[must_use]
    pub fn from_raw(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Object),
            1 => Some(Self::Unit),
            2 => Some(Self::Squad),
            3 => Some(Self::Dopple),
            4 => Some(Self::Projectile),
            5 => Some(Self::Platoon),
            6 => Some(Self::Army),
            _ => None,
        }
    }
}

/// A 32-bit identifier matching vanilla `BEntityID`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct EntityId(pub u32);

impl EntityId {
    /// Vanilla sentinel for an unset entity ID.
    pub const INVALID: Self = Self(u32::MAX);

    /// Create an ID for generation zero.
    ///
    /// # Panics
    ///
    /// Panics when `index` does not fit the vanilla 16-bit pool index.
    #[must_use]
    pub fn new(class: EntityClass, index: u32) -> Self {
        let index = u16::try_from(index).expect("entity pool index exceeds 16 bits");
        Self::with_generation(class, index, 0)
    }

    /// Create an ID with an explicit pool index and generation.
    ///
    /// # Panics
    ///
    /// Panics when `generation` does not fit the vanilla 12-bit field.
    #[must_use]
    pub fn with_generation(class: EntityClass, index: u16, generation: u16) -> Self {
        assert!(
            u32::from(generation) <= GENERATION_MASK,
            "entity generation exceeds 12 bits"
        );
        Self(
            (u32::from(class as u8) << CLASS_SHIFT)
                | (u32::from(generation) << GENERATION_SHIFT)
                | u32::from(index),
        )
    }

    /// Get the entity class from the high four bits.
    #[must_use]
    pub fn class(self) -> Option<EntityClass> {
        EntityClass::from_raw((self.0 >> CLASS_SHIFT) as u8)
    }

    /// Get the low 16-bit pool index as a `u32`.
    #[must_use]
    pub fn index(self) -> u32 {
        self.0 & INDEX_MASK
    }

    /// Get the low 16-bit pool index.
    #[must_use]
    pub fn pool_index(self) -> u16 {
        (self.0 & INDEX_MASK) as u16
    }

    /// Get the 12-bit slot generation.
    #[must_use]
    pub fn generation(self) -> u16 {
        ((self.0 >> GENERATION_SHIFT) & GENERATION_MASK) as u16
    }

    /// Get the raw serialized value.
    #[must_use]
    pub fn as_u32(self) -> u32 {
        self.0
    }

    /// Create an ID from its serialized value.
    #[must_use]
    pub fn from_u32(value: u32) -> Self {
        Self(value)
    }

    /// Check whether this is the vanilla invalid sentinel.
    #[must_use]
    pub fn is_invalid(self) -> bool {
        self == Self::INVALID
    }
}

impl std::fmt::Debug for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_invalid() {
            return write!(f, "EntityId(INVALID)");
        }
        write!(
            f,
            "EntityId({:?}:{}@{})",
            self.class().unwrap_or(EntityClass::Object),
            self.index(),
            self.generation()
        )
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:08X}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_vanilla_fields() {
        let id = EntityId::with_generation(EntityClass::Squad, 0x1234, 0x0ABC);

        assert_eq!(id.as_u32(), 0x2ABC_1234);
        assert_eq!(id.class(), Some(EntityClass::Squad));
        assert_eq!(id.pool_index(), 0x1234);
        assert_eq!(id.generation(), 0x0ABC);
    }

    #[test]
    fn generation_zero_constructor_matches_pool_layout() {
        assert_eq!(EntityId::new(EntityClass::Unit, 7).as_u32(), 0x1000_0007);
    }
}
