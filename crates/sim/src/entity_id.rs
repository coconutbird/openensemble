//! Entity ID type matching vanilla Halo Wars format.
//!
//! Entity IDs are 4 bytes with the high 4 bits encoding the entity class:
//! - 0 = Object
//! - 1 = Unit
//! - 2 = Squad
//! - 3 = Dopple
//! - 4 = Projectile
//! - 5 = Platoon
//! - 6 = Army

/// Entity class encoded in the high 4 bits of an `EntityId`.
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

/// A 32-bit entity identifier matching vanilla `BEntityID`.
///
/// Format: High 4 bits = class, low 28 bits = index.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct EntityId(pub u32);

impl EntityId {
    pub const INVALID: EntityId = EntityId(u32::MAX);

    /// Create a new `EntityId` from class and index.
    #[must_use]
    pub fn new(class: EntityClass, index: u32) -> Self {
        debug_assert!(index < 0x0FFF_FFFF, "Index too large for EntityId");
        Self(((class as u32) << 28) | (index & 0x0FFF_FFFF))
    }

    /// Get the entity class from the high 4 bits.
    #[must_use]
    pub fn class(self) -> Option<EntityClass> {
        EntityClass::from_raw((self.0 >> 28) as u8)
    }

    /// Get the index from the low 28 bits.
    #[must_use]
    pub fn index(self) -> u32 {
        self.0 & 0x0FFF_FFFF
    }

    /// Get the raw u32 value (for serialization).
    #[must_use]
    pub fn as_u32(self) -> u32 {
        self.0
    }

    /// Create from raw u32 (for deserialization).
    #[must_use]
    pub fn from_u32(value: u32) -> Self {
        Self(value)
    }

    /// Check if this is an invalid/unset ID.
    #[must_use]
    pub fn is_invalid(self) -> bool {
        self == Self::INVALID
    }
}

impl std::fmt::Debug for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_invalid() {
            write!(f, "EntityId(INVALID)")
        } else {
            write!(
                f,
                "EntityId({:?}:{})",
                self.class().unwrap_or(EntityClass::Object),
                self.index()
            )
        }
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:08X}", self.0)
    }
}
