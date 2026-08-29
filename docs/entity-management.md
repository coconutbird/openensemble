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
- Scenario object creation defaults `VisualVariationIndex` to `-1` and passes
  an explicit placement value through to every unit created for a squad. The
  renderer consumes that checksummed `BObject` state; it does not reread SCN
  placements or choose an independent model variation.
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
| Class-1 mobile units and buildings | `entities/units/mod.rs`: `Unit` and `UnitKind` |
| Stock Warthog and Marine unit profiles | `entities/units/warthog.rs` and `entities/units/marine.rs` |
| Scenario-layered Ghost body configuration | `gameplay/vehicle_physics.rs` and `entities/units/vehicle.rs` |
| Persistent per-unit ammunition, tactic state, Ram, Detonate, and death-replacement lifecycle | `entities/units/ammunition.rs`: `UnitAmmunition`; `entities/units/tactic_state.rs`: checksummed current state/revision; `entities/units/collision_attack.rs`: checksummed collision state; `entities/units/detonate.rs`: checksummed action triggers/phases; `entities/units/physics_replacement.rs` and `entities/units/death_replacement.rs`: replacement lifetimes |
| Class-2 squads and unit membership | `entities/squads/mod.rs`: `Squad::unit_ids` |
| Stock Warthog and Marine squad profiles | `entities/squads/warthog.rs` and `entities/squads/marine.rs` |
| Mines target and per-member work lifecycle | `entities/squads/mines.rs`: checksummed order/progress; `world/mines.rs`: range, ammo, placement, and database-backed spawn |
| Targeted and passive Detonate lifecycle | `entities/squads/detonate.rs`: checksummed moving/glowing/attacking state; `world/detonate.rs` and `world/detonate/`: action selection, countdown/death/proximity/physics triggers, AOE, replacement, self-kill, and recovery |
| Database- and technology-authored death squads | `world/death_spawns.rs`: atomic pre-cleanup spawn, cap and position checks, Gaia ownership, and hero-down handoff; `player/technology.rs`: player-specific `DeathSpawn` assignment |
| Database-authored in-place death replacements | `world/death_replacements.rs`: same-ID prototype transform, squad retention, Gaia transfer, damaged-repair state, and immediate-destroy bypass |
| Class-4 combat projectiles | `entities/projectiles/mod.rs`: `Projectile` and launch state |
| Shared `BObject` visual requests | `entities/object_state.rs`: `ObjectState` |
| Building-associated base numbers | `entities/base_site.rs`: `BaseId` and `Base` |
| Building production and research state | `entities/units/building.rs`, `world/research.rs`, and `player/research.rs` |
| World ownership and cleanup | `world.rs`: unit, squad, and base lifecycle APIs |
| Layered scenario/database loading and entity expansion | `scenario.rs`, `scenario/coordinates.rs`, `scenario/starts.rs`, and `scenario/technology.rs` |
| Target pursuit, firing, ammunition, Mines, Detonate, collision attacks, and damage | `world/combat.rs`, `world/mines.rs`, `world/detonate.rs`, `world/collision_attacks.rs`, `world/ammunition.rs`, `entities/units/combat.rs`, and `gameplay.rs` |
| Ground/vehicle integration, layered prototype bodies, and obstruction contacts | `physics.rs`, `gameplay/vehicle_physics.rs`, and `entities/units/vehicle.rs` |
| Deterministic state comparison | `world/checksum.rs`: unit/squad/projectile/base state |

Class-0 visual, icon, and control objects now use their own generational pool.
Dopples, platoons, armies, AirSpots, and full binary savegame compatibility
remain separate parity slices that can be added without changing the packed-ID
contract.

Unit-specific state remains under `entities/units/`, squad-specific state under
`entities/squads/`, and orchestration under focused `world/` modules. A spawned
mine is an ordinary authoritative class-1 unit created from the scenario-
layered database. Renderer/UI code discovers it through the same live-world
roster used for every other object and does not own a parallel placement,
ammunition, ownership, or lifecycle model.

Detonate follows the same separation: the squad order lives in
`entities/squads/detonate.rs`, each member's targeted or passive action lives in
`entities/units/detonate.rs`, and `world/detonate.rs` is the only orchestration
layer. Tactic state, HitAndRun mode, contact, countdown, proximity, collision,
death notification, AOE damage, physical replacement, self-death, recovery,
and cleanup are authoritative and checksummed; renderer/UI clients do not
repeat any of those decisions.

Supported `PhysicsInfo` chains are likewise resolved from the already-mounted
scenario source and copied into the checksummed world prototype catalog before
spawn. Unit runtime configuration stays under `entities/units/`; neither the
renderer nor scenario presentation content owns a duplicate vehicle body.

The ordinary unit-death path resolves `<DeathSpawnSquad>` from that same
scenario-layered database immediately before the dead unit leaves the class-1
pool. It preserves the dying unit's transform, applies `MaxPopCount` against
the dying player's live squads, honors `ForceToGaiaPlayer` on target members,
and routes `_HeroDeath` squads into the existing downed-hero lifecycle. The
shipped `DeathSpawn` technology subtype uses the same player-owned prototype
state, including `skull_02` Grunt confetti. Immediate destroy deliberately
bypasses this path, matching retail's kill/destroy distinction. Spawned squads
and members enter the normal authoritative pools, so renderer/UI clients only
project the resulting world roster.

For the optional `CheckPos` flag, the current terrain layer rejects non-finite
and outside-terrain positions. Loading retail's per-cell land-path invalid mask
remains part of pathing parity; none of the shipped static death-spawn entries
currently author `CheckPos`.

Static `<DeathReplacement>` runs earlier than Detonate and death-spawn actions.
For an ordinary death, the existing class-1 entity changes to the target
prototype in place, retains its transform and squad relationship, and remains
in the authoritative pool with retail's dead flag. A target
`ForceToGaiaPlayer` transfers its single-member squad and restores Normal mode;
`DamagedDeathReplacement` starts at one hit point, remains invulnerable while
repairing, and clears that transient state when fully healed. Immediate destroy
bypasses replacement. The prototype swap also advances sim-owned presentation
state, allowing renderer/UI projections to reload the new visual without
implementing death logic.

Retail `SquadCryo` state is authoritative under `entities/squads/cryo.rs`, with
member effects under `entities/units/cryo.rs`. Scenario-layered `CryoPoints`
and global freeze/thaw settings control the checksummed Freezing, Frozen,
Thawing, and inactive phases. Freezing and thawing apply the authored movement
and incoming-damage modifiers; Frozen immobilizes the squad, blocks attacks,
and sets each member's transient `ShatterOnDeath` flag. Newly attached members
inherit the current effect, while detached members clear it.

Native Cryo casts are also owned by the simulation under
`world/powers/cryo.rs`. `InvokePower2` resolves the selected level from the
scenario-layered `powers.xml`, validates ownership/cost/technology/population,
consumes and recharges the player's authoritative power entry, and schedules
the recovered fixed ticks. Each power update selects at most one nearest,
not-yet-hit leader matching `FilterType`; the shipped implementation applies
the target squad's full Cryo resistance despite loading `CryoAmountPerTick`
and computing distance falloff. `DiesWhenFrozen` uses the strict remaining-HP
budget, while transporters always take the forced frozen kill. A cast creates
its bomber and released effect as class-0 visual objects in `World::objects`;
the simulation moves and expires them using authored prototype timing. They
therefore enter the renderer through its existing simulation roster instead of
requiring a second renderer-owned representation of power state.

Native Disruption casts share that same authoritative power infrastructure.
The sim owns the bomber, field, and pulse-attachment objects, the recovered
delayed activation and increasing pulse cadence, and the field's 2.5-second
death state. Active fields reject disruptable powers with retail's strict
planar-radius test even for `NO_COST` commands; `<NotDisruptable>` powers bypass
the test. `InvokePower2` dispatches from the scenario-layered `PowerType`, so
Cryo and Disruption no longer require command-specific routing in the renderer
or executor.

Native Repair uses the same scenario-layered dispatch. The power owns a
class-zero field plus a leader attachment, while squads retain the overlapping
regen reference count that controls attachment lifetime. Scheduled ticks query
the authored leader `FilterType` and relation once per outer update, apply the
recovered recent-damage cooldown, and distribute combat-value repair with
retail's excess spill order. Existing members heal before missing authored
members are reinforced at partial HP; the retail implementation's quirk that
`repairCombatValue` always permits reinforcement is preserved. Repair checks
Disruption only when a tick is due, so activation tears down the field and
shared attachment state without any renderer-owned gameplay path. Offset
attachments retain their sim-owned parent-local transform as leaders move and
turn.

That live flag now selects `ShatterDeathReplacement` for the two shipped Flood
prototypes. The source changes to its shatter target under the same entity ID,
but—unlike a static death replacement—continues through normal death actions,
desquadding, and cleanup. A frozen flag is captured across deferred cleanup so
an owning squad beginning to thaw cannot retroactively change a death that
already occurred. The installed-data test exercises the shipped Flood tentacle
pair; ordinary non-frozen deaths still skip the shatter-only target.
