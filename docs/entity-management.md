# Entity management reverse-engineering map

This map records the entity-management behavior recovered from the Halo Wars
Definitive Edition `xgameFinal.exe` IDA database. The names below are
analyst-assigned names backed by decompilation, call sites, pool geometry, and
save/load ordering; they are not original debug symbols.

## Vanilla contract used by the MVP

- `BEntityID` is packed as `class:4 | generation:12 | pool index:16`.
- Destroying an entity increments the slot generation before reuse. Lookups
  validate the class, slot bounds, occupancy, and generation.
- Entity classes are separate pools: Object `0`, Unit `1`, Squad `2`, Dopple
  `3`, Projectile `4`, Platoon `5`, and Army `6`.
- Units and buildings both occupy the class-1 `BUnit` pool. A building is a
  behavioral/prototype distinction, not another entity-ID class.
- A base is associated with buildings by `BUnit::mBaseNumber`; it is not an
  eighth entity-ID class. Prototype flag `KBCreatesBase` identifies base
  anchors during scenario loading.
- Vanilla save loading rejects pool high-water marks above 20,000. The known
  serialized entity order is Units, Squads, Projectiles, Armies, Platoons,
  Dopples, Objects, then AirSpots.
- Observed allocation sizes include 1,120 bytes per `BUnit` and 752 bytes per
  `BSquad`. The Rust MVP models behavior rather than reproducing those in-memory
  layouts.

## Renamed functions

### Object-manager lifetime, allocation, and lookup

| Address | IDA name | Evidence-backed role |
| --- | --- | --- |
| `0x1402B6AE0` | `BObjectManager_releaseAirSpot` | Releases an AirSpot pool entry. |
| `0x1402B6B30` | `BObjectManager_ctor` | Initializes manager fields and pool state. |
| `0x1402B6D70` | `BObjectManager_scalarDeletingDtor` | Compiler scalar-deleting destructor wrapper. |
| `0x1402B6DB0` | `BObjectManager_dtor` | Clears and releases manager-owned pools. |
| `0x1402B6FD0` | `BObjectManager_initialize` | Allocates/initializes typed pools and tracking data. |
| `0x1402B7080` | `BObjectManager_clear` | Destroys active entities and resets manager state. |
| `0x1402B76B0` | `BObjectManager_createEntity` | Dispatches creation by the class encoded in an ID. |
| `0x1402B77C0` | `BObjectManager_allocateEntity` | Allocates a typed-pool slot and constructs its ID. |
| `0x1402B78E0` | `BObjectManager_allocateEntityWithId` | Reserves an explicit ID, used by restoration/loading. |
| `0x1402B79C0` | `BObjectManager_clearEntityPlayerTracking` | Clears per-player tracking for a removed entity. |
| `0x1402B7A70` | `BObjectManager_destroyEntity` | Runs class-specific destruction, advances generation, and frees the slot. |
| `0x1402B7D30` | `BObjectManager_getEntity` | Validates and resolves a packed ID through its typed pool. |
| `0x1402B8030` | `BObjectManager_isValidEntityId` | Performs class/index/generation validity checks. |
| `0x1402B8160` | `BObjectManager_getNextObject` | Iterates active class-0 objects by pool index. |
| `0x1402B8210` | `BObjectManager_getFirstUpdateObject` | Finds the first object included in the update set. |
| `0x1402B8B40` | `BObjectManager_assertEmpty` | Verifies that managed entity pools are empty. |
| `0x1402B8CE0` | `BObjectManager_getAirSpotAtOrAfter` | Finds an occupied AirSpot at or after an index. |
| `0x1402B8D60` | `BObjectManager_setObjectUpdateExcluded` | Changes the secondary object-update membership bitset. |

### Typed pool accessors

| Address | IDA name |
| --- | --- |
| `0x1402B82B0` | `BObjectManager_getDopple` |
| `0x1402B8360` | `BObjectManager_getNextDopple` |
| `0x1402B8420` | `BObjectManager_getProjectile` |
| `0x1402B84D0` | `BObjectManager_getNextProjectile` |
| `0x1402B8590` | `BObjectManager_getUnit` |
| `0x1402B8640` | `BObjectManager_getNextUnit` |
| `0x1402B8700` | `BObjectManager_getSquad` |
| `0x1402B87B0` | `BObjectManager_getNextSquad` |
| `0x1402B8870` | `BObjectManager_getPlatoon` |
| `0x1402B8920` | `BObjectManager_getNextPlatoon` |
| `0x1402B89E0` | `BObjectManager_getArmy` |
| `0x1402B8A80` | `BObjectManager_getNextArmy` |

### Scenario and save/load boundaries

| Address | IDA name | Evidence-backed role |
| --- | --- | --- |
| `0x140377BE0` | `BScenario_load` | Loads scenario-level players and placed objects. |
| `0x14037D6A0` | `BScenario_loadObject` | Resolves and creates an individual placed scenario object. |
| `0x1404E5B30` | `BUnit_writeSavegame` | Serializes unit/building state, including base association. |
| `0x1404E7A60` | `BUnit_readSavegame` | Restores unit/building state and references. |
| `0x1405D47E0` | `BWorld_writeSavegame` | Serializes typed entity pools in deterministic class order. |
| `0x1405D8530` | `BWorld_readSavegame` | Restores pool high-water marks, explicit IDs, and entity records. |

## Rust MVP correspondence

| Vanilla behavior | Rust implementation |
| --- | --- |
| Packed `BEntityID` fields | `entity_id.rs`: `EntityId` and `EntityClass` |
| Separate generational typed pools | `entity.rs`: `EntityManager<T>` |
| Class-1 mobile units and buildings | `entities/unit.rs`: `Unit` and `UnitKind` |
| Class-2 squads and unit membership | `entities/squad.rs`: `Squad::unit_ids` |
| Building-associated base numbers | `entities/base_site.rs`: `BaseId` and `Base` |
| World ownership and cleanup | `world.rs`: unit, squad, and base lifecycle APIs |
| Placed proto-object and squad expansion | `scenario.rs` |
| Deterministic state comparison | `world.rs`: unit/squad/base checksum helpers |

The MVP intentionally defers class-0 generic objects, Dopples, projectiles,
platoons, armies, AirSpots, construction/economy rules, combat, powers, and full
binary savegame compatibility. Those systems can be added as separate typed
pools without changing the packed-ID contract.
