//! Order types matching vanilla BSimOrder.
//!
//! Orders are the internal representation of what entities should do.
//! Commands from the network are converted to orders.

/// Order types matching BSimOrder::cType* from SimOrder.h
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

impl OrderType {
    /// Convert from i32 (command ID).
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
