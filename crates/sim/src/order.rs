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
    /// No order.
    #[default]
    None = 0,
    /// Move to a position.
    Move = 1,
    /// Attack a target.
    Attack = 2,
    /// Gather a resource.
    Gather = 3,
    /// Repair self.
    Repair = 4,
    /// Repair another entity.
    RepairOther = 5,
    /// Ask another entity for a repair.
    RequestRepair = 6,
    /// Build a building.
    Build = 7,
    /// Capture a target.
    Capture = 8,
    /// Join another squad.
    Join = 9,
    /// Enter a container.
    Garrison = 10,
    /// Leave a container.
    Ungarrison = 11,
    /// Detonate self.
    Detonate = 12,
    /// Honk the horn.
    Honk = 13,
    /// Guard a position or target.
    Guard = 14,
    /// Change the squad mode.
    ChangeMode = 15,
    /// Place mines.
    Mines = 16,
    /// Play an animation that blocks other orders.
    PlayBlockingAnimation = 17,
    /// Move to a rally point.
    RallyPoint = 18,
    /// Stay idle.
    Idle = 19,
    /// Unpack before an attack or deploy.
    Unpack = 20,
    /// Attach to a carrier.
    Hitch = 21,
    /// Detach from a carrier.
    Unhitch = 22,
    /// Transport squads to a position.
    Transport = 23,
    /// Wander around.
    Wander = 24,
    /// Turn on the cloak.
    Cloak = 25,
    /// Jump to a world location.
    Jump = 26,
    /// Jump beside a resource, then gather.
    JumpGather = 27,
    /// Jump beside a container, then garrison.
    JumpGarrison = 28,
    /// Jump toward an attack target.
    JumpAttack = 29,
    /// Attack a target at point-blank range.
    PointBlankAttack = 30,
    /// Turn on the energy shield.
    EnergyShield = 31,
    /// Involuntary pull that the charge system implements.
    JumpPull = 32,
    /// Turn on the infantry energy shield.
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
