//! Order types matching vanilla `BSimOrder`.
//!
//! Orders are the internal representation of what entities should do.
//! Commands from the network are converted to orders.

use crate::entity_id::EntityId;
use glam::Vec3;

/// Order types matching `BSimOrder::cType`* from SimOrder.h
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(i32)]
pub enum OrderType {
    #[default]
    None = 0,
    Move = 1,
    Attack = 2,
    Gather = 3,
    Repair = 4,
    RepairOther = 5,
    RequestRepair = 6,
    Build = 7,
    Capture = 8,
    Join = 9,
    Garrison = 10,
    Ungarrison = 11,
    Detonate = 12,
    Honk = 13,
    Guard = 14,
    ChangeMode = 15,
    Mines = 16,
    PlayBlockingAnimation = 17,
    RallyPoint = 18,
    Idle = 19,
    Unpack = 20,
    Hitch = 21,
    Unhitch = 22,
    Transport = 23,
    Wander = 24,
    Cloak = 25,
    Jump = 26,
    JumpGather = 27,
    JumpGarrison = 28,
    JumpAttack = 29,
    PointBlankAttack = 30,
    EnergyShield = 31,
    JumpPull = 32,
    InfantryEnergyShield = 33,
}

/// Retail subtype carried by the five squad Jump orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum JumpOrderType {
    /// Jump to a world location.
    #[default]
    Jump = 0,
    /// Jump beside a resource and gather after landing.
    Gather = 1,
    /// Jump beside a container and garrison after landing.
    Garrison = 2,
    /// Jump toward an attack target, stopping at half weapon range.
    Attack = 3,
    /// Involuntary Brute Chief pull, implemented by the charge system.
    Pull = 4,
}

/// Inputs needed to issue one authoritative voluntary Jump-family order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JumpOrderRequest {
    kind: JumpOrderType,
    target_id: Option<EntityId>,
    target_position: Vec3,
    requested_ability_id: Option<u8>,
}

impl JumpOrderRequest {
    /// Build a location-targeted Jump request.
    #[must_use]
    pub const fn location(
        kind: JumpOrderType,
        target_position: Vec3,
        requested_ability_id: Option<u8>,
    ) -> Self {
        Self {
            kind,
            target_id: None,
            target_position,
            requested_ability_id,
        }
    }

    /// Build an entity-targeted Jump-family request.
    #[must_use]
    pub const fn entity(
        kind: JumpOrderType,
        target_id: EntityId,
        requested_ability_id: Option<u8>,
    ) -> Self {
        Self {
            kind,
            target_id: Some(target_id),
            target_position: Vec3::ZERO,
            requested_ability_id,
        }
    }

    pub(crate) const fn kind(self) -> JumpOrderType {
        self.kind
    }

    pub(crate) const fn target_id(self) -> Option<EntityId> {
        self.target_id
    }

    pub(crate) const fn target_position(self) -> Vec3 {
        self.target_position
    }

    pub(crate) const fn requested_ability_id(self) -> Option<u8> {
        self.requested_ability_id
    }
}

impl OrderType {
    /// Convert from i32 (command ID).
    #[must_use]
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Move),
            2 => Some(Self::Attack),
            3 => Some(Self::Gather),
            4 => Some(Self::Repair),
            5 => Some(Self::RepairOther),
            6 => Some(Self::RequestRepair),
            7 => Some(Self::Build),
            8 => Some(Self::Capture),
            9 => Some(Self::Join),
            10 => Some(Self::Garrison),
            11 => Some(Self::Ungarrison),
            12 => Some(Self::Detonate),
            13 => Some(Self::Honk),
            14 => Some(Self::Guard),
            15 => Some(Self::ChangeMode),
            16 => Some(Self::Mines),
            17 => Some(Self::PlayBlockingAnimation),
            18 => Some(Self::RallyPoint),
            19 => Some(Self::Idle),
            20 => Some(Self::Unpack),
            21 => Some(Self::Hitch),
            22 => Some(Self::Unhitch),
            23 => Some(Self::Transport),
            24 => Some(Self::Wander),
            25 => Some(Self::Cloak),
            26 => Some(Self::Jump),
            27 => Some(Self::JumpGather),
            28 => Some(Self::JumpGarrison),
            29 => Some(Self::JumpAttack),
            30 => Some(Self::PointBlankAttack),
            31 => Some(Self::EnergyShield),
            32 => Some(Self::JumpPull),
            33 => Some(Self::InfantryEnergyShield),
            _ => None,
        }
    }
}

impl JumpOrderType {
    /// Convert a Jump-family simulation order into its retail subtype.
    #[must_use]
    pub const fn from_order_type(order_type: OrderType) -> Option<Self> {
        match order_type {
            OrderType::Jump => Some(Self::Jump),
            OrderType::JumpGather => Some(Self::Gather),
            OrderType::JumpGarrison => Some(Self::Garrison),
            OrderType::JumpAttack => Some(Self::Attack),
            OrderType::JumpPull => Some(Self::Pull),
            _ => None,
        }
    }

    /// Return the synchronized order ID corresponding to this subtype.
    #[must_use]
    pub const fn order_type(self) -> OrderType {
        match self {
            Self::Jump => OrderType::Jump,
            Self::Gather => OrderType::JumpGather,
            Self::Garrison => OrderType::JumpGarrison,
            Self::Attack => OrderType::JumpAttack,
            Self::Pull => OrderType::JumpPull,
        }
    }
}
