# Combat reconstruction

This document records the retail evidence behind the authoritative ranged-
combat and shield-recharge slices. The simulation owns target orders, pursuit,
authored attack timing, projectiles, hit points, shields, and recharge timing.
Rendering observes those entities and never reproduces combat decisions.

## Renamed IDA functions

Every function substantively inspected for this slice was renamed and
commented in the saved `xgameFinal.exe.i64` database.

| Address | IDA name | Recovered role |
| --- | --- | --- |
| `0x1400F0250` | `BUnitActionShieldRegen_init` | Initializes persistent unit recharge state and clears its end time. |
| `0x14010DBB0` | `BActionManager_createAction` | Allocates and initializes an action by authored action type. |
| `0x140112100` | `BActionManager_loadPtr` | Restores an action pointer by type and free-list index. |
| `0x140115FD0` | `BActionManager_preSave` | Writes action free-list headers and save markers. |
| `0x140122320` | `BActionManager_preLoad` | Restores action free-list headers and save markers. |
| `0x140131340` | `BActionManager_save` | Serializes allocated action instances and their runtime state. |
| `0x14013DA10` | `BActionManager_load` | Restores allocated action instances and their runtime state. |
| `0x1401AE270` | `BSquadActionShieldRegen_allocateFreeListPage` | Grows the squad-recharge action free list. |
| `0x1401B7C50` | `BSquadActionShieldRegen_allocateAndConstructPage` | Allocates a page and constructs its squad-recharge actions. |
| `0x1401F8BE0` | `BDatabase_getProtoActionTypeFromName` | Resolves authored action-type names. |
| `0x1401E0430` | `BUnitActionAttack_computeScaledDamage` | Computes attack damage from the proto action and live modifiers. |
| `0x1403D53F0` | `BSquadActionShieldRegen_init` | Makes squad recharge persistent and compatible with idle. |
| `0x1403D5410` | `BSquadActionShieldRegen_update` | Enforces the shared damage delay and starts child recharge actions. |
| `0x140439700` | `BSquadActionShieldRegen_getInstance` | Acquires a squad-recharge action from its free list. |
| `0x140554070` | `BUnitActionRangedAttack_start` | Starts target engagement and initial cooldown state. |
| `0x1405547C0` | `BUnitActionRangedAttack_stop` | Stops an active ranged action. |
| `0x140554C30` | `BUnitActionRangedAttack_reset` | Clears ranged-action runtime state. |
| `0x140554D70` | `BUnitActionRangedAttack_setAttackState` | Transitions the ranged-action state machine. |
| `0x140555280` | `BUnitActionRangedAttack_update` | Advances cooldown, animation, tags, reload, and target state. |
| `0x140556850` | `BUnitActionRangedAttack_fireWeapon` | Converts an `Attack` tag into an instant hit or projectile launch. |
| `0x140556A60` | `BUnitActionRangedAttack_applyInstantHit` | Applies non-projectile weapon damage immediately. |
| `0x14055C400` | `BUnitActionRangedAttack_launchPendingProjectiles` | Creates projectiles queued by the current attack event. |
| `0x14055CD40` | `BUnitActionRangedAttack_configureProjectileChain` | Configures chained projectile launch data. |
| `0x14055CE20` | `BUnitActionRangedAttack_createPendingProjectiles` | Materializes pending projectile instances. |
| `0x14055ECE0` | `BUnitActionRangedAttack_writeSavegame` | Serializes ranged-action runtime state. |
| `0x14055F9D0` | `BUnitActionRangedAttack_readSavegame` | Restores ranged-action runtime state. |
| `0x140566480` | `BUnitActionShieldRegen_connect` | Sets the fixed recharge-action end time from `ShieldRegenTime`. |
| `0x1405664B0` | `BUnitActionShieldRegen_update` | Applies authored shield rate and ends at the fixed action time. |
| `0x1405665F0` | `BUnitActionShieldRegen_save` | Serializes the recharge-action end time. |
| `0x140566670` | `BUnitActionShieldRegen_load` | Restores the recharge-action end time. |
| `0x1405C6CA0` | `BWorld_createProjectile` | Allocates and initializes a class-4 projectile. |
| `0x1405C7CC0` | `BWorld_getEntity` | Resolves a generational entity ID across typed pools. |

Recovered source corroboration comes from `unitactionrangedattack.cpp`,
`damagehelper.cpp`, `projectile.cpp`, `movementhelper.cpp`, `world.cpp`,
`TerrainSimRep.cpp`, `tactic.cpp`, `protovisual.cpp`, and `visualitem.cpp`, plus
`unit.cpp`, `unitactionshieldregen.cpp`, `squadactionshieldregen.cpp`,
`techeffect.cpp`, and `actionmanager.cpp` in the recovered Halo Wars source tree.

## Implemented retail contracts

- Scenario ERAs are mounted before database parsing. Scenario-local database,
  tactic, visual, and UAX files therefore participate in last-loaded-wins
  resolution.
- Tactic target rules are evaluated at runtime in authored order. The selector
  applies relation, current squad mode, manual/auto-target mode gates,
  `NoAutoTarget`, ability matching and fallback, target state, damage/object
  type, Gaia, and invulnerability predicates.
- Work-command squad-mode and ability bytes are retained in authoritative order
  state. Per-unit action enablement begins at `StartDisabled`, then applies the
  owning player's active `ActionEnable` technology effects and explicit unit
  overrides. Marine rifle, cover, grenade, and rocket selection therefore comes
  from the shipped rules rather than renderer state.
- Player technology state preserves activation order and implements retail
  `Absolute`, `BasePercent`, `Percent`, `Assign`, and `BasePercentAssign`
  scalar relativity. Weapon damage, range, accuracy/deviation, AOE primary
  factor, velocity lead, and ability recovery read that state at use time;
  trigger-time `ModifyProtoData` operations layer after active technologies.
  Hit-point effects rescale existing units by the new/old maximum ratio,
  while future units spawn with the modified maximum.
- `TransformProtoSquad` keeps the player's logical prototype ID stable, changes
  the effective definition for future spawns, and adds only newly introduced
  members to existing squads. As in retail, removing the technology does not
  reverse an already-applied squad transform.
- A requested database `Command` ability maps through each proto object's
  `AbilityCommand`. A successful authored attack cycle starts its declared
  recovery channel and duration; the command executor rejects another ability
  using that channel until it expires. Units use the normal tactic action while
  the squad recovers.
- VIS model selection and weighted UAX durations produce immutable attack
  profiles. Normalized `Attack` tags drive shot timing, including pre/post
  cooldowns, visual ammo, and reload duration.
- Squad and standalone attack orders retain generational entity targets,
  canonicalize clicked squad members to their parent squad, chase to authored
  range, and deterministically select a live concrete member when firing. Both
  pursuit and firing ask the same runtime selector, so a mode or ability cannot
  use one action's range and another action's damage.
- Class-4 projectile entities carry launch-time damage, weapon type, area-
  damage profile, target, lifespan, acceleration, tracking, turn rate, and
  gravity and perturbance state. Their pool and every unit combat timer
  participate in the world checksum. Active player technologies, squad
  recovery, and in-progress ability participation are also checksummed.
- Projectile acceleration is enabled only when the prototype has positive
  fuel. Tracking begins after its authored delay or once less than 0.4 seconds
  of current-speed travel remains, follows the live target position, and
  exhausts fuel into gravity/tumbling state as retail does. Flight retains the
  full velocity vector between ticks: straight shots integrate it directly,
  gravity shots use the old/new-velocity trapezoid, and tracking turns it with
  retail's dot-based angle capped by the authored turn rate.
- After a projectile clears its source unit's XZ obstruction radius, it consumes
  retail's synchronized perturbance-attempt roll on every inactive eligible
  update, even when its prototype has no perturbance data. Successful recurring
  and one-time starts consume duration plus three vector draws, use the authored
  half-sine envelope, and scale recurring velocity by current/desired speed.
  Perturbance changes the tracking movement step without being permanently
  added to stored velocity. Fuel exhaustion disables it, and the retail sticky
  intercept-distance state suppresses it for the rest of the flight.
- Tracking projectiles aimed at non-flying units retain their launch-point
  clearance above the scenario XSD simulation terrain, clamped to zero through
  five units. After movement and before collision, low projectiles are raised to
  that clearance while they remain outside retail's speed/turn-rate approach
  distance. Flying targets and the final approach skip the correction.
- `ExplodeOnTimer` and `ExpireOnTimer` projectiles now keep their authored
  lifespan after collision instead of being treated as ordinary impacts.
  Sticky unit hits retain a sim-local position/direction and follow the unit;
  terrain hits enter a fixed rest state. Plasma-grenade-style timer explosions
  defer all impact damage until expiration, while expire-only effects apply the
  initial unit hit once and disappear later without another damage event. The
  motion state, attachment target, and local transform are checksummed, and the
  renderer continues to consume only the resulting projectile transform.
- `MaxProjectileHeight` and effective weapon range drive retail's per-shot
  ballistic launch velocity and gravity, including distance-squared height
  scaling and target obstruction radius. Moving targets receive launch-time
  `MaxVelocityLead`; active tracking adds cosine-weighted interception inside
  the scenario-layered `TrackInterceptDistance`.
- Every projectile shot consumes retail's synchronized accuracy roll, including
  perfect shots. A miss consumes the deviation and rotation rolls in the same
  order, applies the authored two-interval distribution scaled by range, and
  retains that perpendicular offset as the target moves. Stationary versus
  moving values switch only at 90% of desired speed; live accuracy and dodge
  scalars multiply hit chance and inversely scale maximum deviation. Collision,
  rather than the originally requested target ID, decides whether that offset
  still produces a hit.
- Each projectile substep performs a deterministic swept query against live
  unit simulation bounds. The filter preserves `SelfDamage`,
  `ProjectileObstructable`, `TargetsFootOfUnit`, Cover, Gaia, diplomacy, and
  weapon friendly-fire rules. The nearest eligible intersection becomes the
  actual primary target, and the impact point advances 0.01 units inside its
  bounds so AOE distance is zero for the hit unit.
- XSD height queries now reproduce retail's bilinear `getHeightRaycast`
  interpolation. Projectile terrain checks preserve the short-segment
  8-unit/0.25-height early-outs, intersect the two authored triangles in each
  crossed height tile, and place ground zero 0.25 units above the surface.
  Unit hits are resolved first; a terrain or targets-foot impact has no direct
  primary target and therefore supplies the full base-damage pool to AOE.
- Impact damage applies attacker damage, optional height bonus, database weapon
  type versus damage type, and the target's live damage-taken multiplier.
- Positive `AOERadius` weapons use retail's shared damage-pool contract for
  both instant hits and projectile impacts. The primary factor is applied
  first, the remaining pool is capped before weapon-type modifiers, nonlinear
  damage is normalized across eligible simulation bounds, and linear damage
  consumes base damage nearest-target-first. The two-interval distance falloff,
  vertical-axis ignore, friendly-fire filtering, attacker exclusion, and
  uncapped non-Cover Gaia damage all come from the scenario-layered tactic.
- Retail `Shielded` damage-type entries are treated as full shield coverage,
  not as an armor multiplier. Units spawn with empty shields and immediately
  request recharge. A hit drains shields first and sends same-hit overflow to
  hit points after all existing damage scalars have been applied.
- Squad members share the squad's last-damaged recharge clock. Recharge begins
  only when elapsed time is strictly greater than the player delay multiplied
  by the leader unit's delay scalar; a zero initial timestamp starts the spawn
  recharge immediately. Standalone units use the same deterministic contract.
- A unit recharge action lasts the database `ShieldRegenTime` even when it
  reaches full early. Its per-second amount is maximum shields multiplied by
  the player's recharge rate and the unit's rate scalar. Subsequent hits do not
  cancel an already-running retail recharge action.
- Active `Shieldpoints`, player `ShieldRegenRate`/`ShieldRegenDelay`, and
  per-prototype recharge rate/delay technology effects update live units and
  are also applied to future spawns. Shield values, recharge actions, squad
  damage clocks, and the active technology state participate in checksums.
- Scenario-layered code-object types and tactics select immutable revival
  profiles. `_HeroDeath` units remain alive/down at one HP, regenerate from
  global game-data values, and clear `Down` only after reaching the authored HP
  threshold with another live, non-downed allied squad in range. A tactic
  `Revive` action keeps zero-HP units alive and hibernating, advances its two
  authored delays, and resumes its authored HP-per-second regeneration. A
  version-2 combat-damage revive override only takes effect when a later damage
  event finds that tactic-`Revive` unit already at zero HP, matching retail's
  pre-damage flag transition.
- Down and hibernating units cannot move, attack, receive combat damage, become
  combat targets, or start shield recharge. The flags remain independent of
  `UnitState`, participate in deterministic checksums, and drive the same
  unit/squad liveness predicates used by triggers.
- The UGX scene roster is derived from live sim unit and projectile pools. GPU
  presentation refreshes transforms by entity ID and contains no duplicate
  movement, targeting, firing, or damage logic.

## Deliberate next boundaries

This is the deterministic baseline, not a claim of complete combat parity.
Remaining retail systems include unsupported technology effect families,
ability ammunition, non-attack
recovery start events, tactic-state membership, lockdown minimum-range
behavior, merge/garrison/melee-attacker predicates, hardpoint and hit-zone
launch offsets, oriented hit-zone/visual-mesh projectile intersection,
directional damage and directional shield arcs, hit-zone shields, external
shields, damage proxies, runtime `Unhittable`, invulnerability and destructible
non-unit AOE recipients, dodge/deflect, sticky visual-mesh/bone intersections,
timer damage reapplication, beam/needler behaviors, hero death/revival
presentation, death effects, and ranged-action savegame compatibility. The
current database schema retains only
the shield marker, but every shipped `Shielded` object inspected so far omits a
direction attribute and therefore uses full coverage.

## Installed-data validation

The opt-in `scenario-asset-loading` integration test mounts Blood Gulch before
database parsing, verifies that canonical tables and scenario assets share one
layered source, and builds the authoritative simulation from that database. It
then activates the real Marine upgrade records, observes the grenade-to-rocket
action switch, completes a rocket volley, verifies the authored 20-second
`Ability` recovery, observes the authored four-unit-radius rocket splash
damaging multiple authoritative target members, and confirms that Command reuse is
rejected during it. The
same test spawns the shipped Spartan prototype, verifies its 5,000-point empty-
at-spawn shield, the database 5-second charge duration, same-hit absorption,
and the strict 20-second post-damage delay. It also resolves the shipped
`cpgn_inf_spartanRocket_01` `_HeroDeath` profile from the layered catalog and
verifies its 90-second regeneration time, 15-unit ally radius, and 50% revival
threshold. It also joins the real Marine rifle projectile to its layered
proto-object and verifies fuel-gated acceleration plus tracking, gravity,
tumbling, and self-damage flags before exercising projectile damage in the
loaded Blood Gulch world. The same catalog check joins the Marine grenade to
its positive `MaxProjectileHeight` and verifies that the simulation reads
`TrackInterceptDistance` from the active layered game-data table.
The Marine rifle catalog join also verifies its stationary/moving accuracy,
maximum deviations, distribution factors, and `MaxVelocityLead` directly
against the weapon in the scenario-layered tactic.
The Marine rocket join verifies the shipped tracking projectile's perturbance
chance, velocity, and duration interval directly against that same layered
database before combat runs it through the authoritative projectile pool.

The same installed-data test now also spawns a shipped Barracks, issues the
retail building command for the first Marine upgrade, verifies its authored
resource cost and research duration, and observes activation through the
authoritative simulation. See `docs/research.md` for the recovered production
contract and remaining boundaries.

```powershell
$env:OPENENSEMBLE_GAME_DIR='C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE'
cargo test -p sim --test scenario-asset-loading -- --ignored
```
