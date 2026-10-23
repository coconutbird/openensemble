//! The collision filter (`hkpGroupFilter`).
//!
//! Retail sets up the filter in `BPhysicsWorld_init` (`0x1407BB390`, 2008
//! `xphysics/physicsworld.cpp:117-134`). The rule is the Havok 5.1 rule in
//! `hkpGroupFilter.cpp:38-77`.

/// A collision layer of Halo Wars (2008 `xphysics/physicsworld.h:91-98`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Layer {
    /// Layer 0. It collides with every layer.
    Zero = 0,
    /// The terrain heightfield.
    Terrain = 1,
    /// Vehicles that collide with the terrain.
    Vehicles = 2,
    /// Phantoms.
    Phantoms = 3,
    /// Objects, for example debris that left its parent.
    Objects = 4,
    /// Buildings.
    Buildings = 5,
    /// Bodies that collide only with the terrain and with phantoms.
    Uncollidables = 6,
    /// Vehicles that do not collide with the terrain.
    NoTerrainVehicles = 7,
}

/// The collision filter information of a body (Havok `calcFilterInfo`).
///
/// The bits are `layer | subsystem_id << 5 | subsystem_dont_collide_with << 10 |
/// system_group << 16`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FilterInfo(u32);

impl FilterInfo {
    /// Packs the filter information. The layer and the two subsystem ids keep their low 5 bits.
    #[must_use]
    pub const fn new(
        layer: u8,
        system_group: u16,
        subsystem_id: u8,
        subsystem_dont_collide_with: u8,
    ) -> Self {
        Self(
            (layer as u32 & 0x1f)
                | ((subsystem_id as u32 & 0x1f) << 5)
                | ((subsystem_dont_collide_with as u32 & 0x1f) << 10)
                | ((system_group as u32) << 16),
        )
    }

    /// Makes filter information for a layer and a system group, with both subsystem ids 0.
    #[must_use]
    pub const fn for_layer(layer: Layer, system_group: u16) -> Self {
        Self::new(layer as u8, system_group, 0, 0)
    }

    /// Makes filter information from its raw bits.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw bits.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Returns the layer index (0 to 31).
    #[must_use]
    pub const fn layer(self) -> u8 {
        (self.0 & 0x1f) as u8
    }

    /// Returns the system group.
    #[must_use]
    pub const fn system_group(self) -> u16 {
        (self.0 >> 16) as u16
    }

    /// Returns the subsystem id.
    #[must_use]
    pub const fn subsystem_id(self) -> u8 {
        ((self.0 >> 5) & 0x1f) as u8
    }

    /// Returns the subsystem id that this body does not collide with.
    #[must_use]
    pub const fn subsystem_dont_collide_with(self) -> u8 {
        ((self.0 >> 10) & 0x1f) as u8
    }
}

/// The group collision filter: a layer table plus the system-group rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupFilter {
    collision_lookup: [u32; 32],
    next_free_system_group: u16,
}

impl Default for GroupFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl GroupFilter {
    /// The layer pairs that retail disables, in the order of `BPhysicsWorld_init`.
    pub const RETAIL_DISABLED_PAIRS: [(Layer, Layer); 12] = [
        (Layer::Phantoms, Layer::Phantoms),
        (Layer::Objects, Layer::Objects),
        (Layer::Vehicles, Layer::Buildings),
        (Layer::Vehicles, Layer::Objects),
        (Layer::NoTerrainVehicles, Layer::Buildings),
        (Layer::NoTerrainVehicles, Layer::Objects),
        (Layer::NoTerrainVehicles, Layer::Terrain),
        (Layer::NoTerrainVehicles, Layer::Vehicles),
        (Layer::Uncollidables, Layer::Objects),
        (Layer::Uncollidables, Layer::Vehicles),
        (Layer::Uncollidables, Layer::NoTerrainVehicles),
        (Layer::Uncollidables, Layer::Buildings),
    ];

    /// Makes a filter in which every layer collides with every layer.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            collision_lookup: [u32::MAX; 32],
            next_free_system_group: 0,
        }
    }

    /// Makes the filter of the retail world, with [`Self::RETAIL_DISABLED_PAIRS`] disabled.
    #[must_use]
    pub fn retail() -> Self {
        let mut filter = Self::new();
        for (a, b) in Self::RETAIL_DISABLED_PAIRS {
            filter.disable_collisions_between(a as u8, b as u8);
        }
        filter
    }

    /// Stops collisions between two layers. Layers above 31 use their low 5 bits.
    pub fn disable_collisions_between(&mut self, layer_a: u8, layer_b: u8) {
        let a = usize::from(layer_a & 0x1f);
        let b = usize::from(layer_b & 0x1f);
        self.collision_lookup[a] &= !(1 << b);
        self.collision_lookup[b] &= !(1 << a);
    }

    /// Allows collisions between two layers. Layers above 31 use their low 5 bits.
    pub fn enable_collisions_between(&mut self, layer_a: u8, layer_b: u8) {
        let a = usize::from(layer_a & 0x1f);
        let b = usize::from(layer_b & 0x1f);
        self.collision_lookup[a] |= 1 << b;
        self.collision_lookup[b] |= 1 << a;
    }

    /// Gives out a new system group (`getNewSystemGroup`). The first call returns 1.
    pub fn new_system_group(&mut self) -> u16 {
        self.next_free_system_group = self.next_free_system_group.wrapping_add(1);
        self.next_free_system_group
    }

    /// Returns `true` if two bodies with this filter information can collide.
    ///
    /// Two bodies in the same nonzero system group collide unless the subsystem id of one is the
    /// `subsystem_dont_collide_with` of the other. All other pairs use the layer table.
    #[must_use]
    pub fn is_collision_enabled(&self, a: FilterInfo, b: FilterInfo) -> bool {
        if a.system_group() == b.system_group() && a.system_group() != 0 {
            return a.subsystem_id() != b.subsystem_dont_collide_with()
                && b.subsystem_id() != a.subsystem_dont_collide_with();
        }
        self.collision_lookup[usize::from(a.layer())] & (1 << b.layer()) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::{FilterInfo, GroupFilter, Layer};

    #[test]
    fn retail_layer_table_matches_the_disabled_pairs() {
        let filter = GroupFilter::retail();
        let info = |layer| FilterInfo::for_layer(layer, 0);
        assert!(filter.is_collision_enabled(info(Layer::Vehicles), info(Layer::Terrain)));
        assert!(!filter.is_collision_enabled(info(Layer::Vehicles), info(Layer::Buildings)));
        assert!(!filter.is_collision_enabled(info(Layer::Buildings), info(Layer::Vehicles)));
        assert!(filter.is_collision_enabled(info(Layer::Objects), info(Layer::Buildings)));
        assert!(!filter.is_collision_enabled(info(Layer::Objects), info(Layer::Objects)));
        assert!(!filter.is_collision_enabled(info(Layer::NoTerrainVehicles), info(Layer::Terrain)));
        assert!(filter.is_collision_enabled(info(Layer::Uncollidables), info(Layer::Terrain)));
        assert!(filter.is_collision_enabled(info(Layer::Zero), info(Layer::Uncollidables)));
    }

    #[test]
    fn a_shared_system_group_stops_collisions_in_an_enabled_layer() {
        let mut filter = GroupFilter::retail();
        let group = filter.new_system_group();
        assert_eq!(group, 1);
        let vehicle = FilterInfo::for_layer(Layer::Vehicles, group);
        assert!(!filter.is_collision_enabled(vehicle, vehicle));

        let other = FilterInfo::for_layer(Layer::Vehicles, filter.new_system_group());
        assert!(filter.is_collision_enabled(vehicle, other));
    }

    #[test]
    fn subsystem_ids_decide_inside_a_system_group() {
        let filter = GroupFilter::retail();
        let pelvis = FilterInfo::new(Layer::Objects as u8, 7, 1, 2);
        let upper = FilterInfo::new(Layer::Objects as u8, 7, 2, 1);
        let lower = FilterInfo::new(Layer::Objects as u8, 7, 3, 1);
        assert!(!filter.is_collision_enabled(pelvis, upper));
        assert!(!filter.is_collision_enabled(lower, pelvis));
        assert!(filter.is_collision_enabled(upper, lower));
    }
}
