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
`techeffect.cpp`, `actionmanager.cpp`, `unitactionbubbleshield.cpp`, and
`unitactionplasmashieldgen.cpp`, `unitactionjoin.cpp`, `tactic.cpp`,
`protosquad.cpp`, and `squad.cpp` in the recovered Halo Wars source tree. The
Join work used those recovered named sources; it did not inspect additional IDA
functions beyond the renamed functions listed above. Veterancy and XP recovery
likewise used the named `squad.cpp`, `unit.cpp`, `protosquad.cpp`,
`squadactionattack.cpp`, and `triggereffect.cpp` sources, so this work introduced
no additional inspected IDA functions requiring a rename.

## Implemented retail contracts

- Scenario ERAs are mounted before database parsing. Scenario-local database,
  tactic, visual, and UAX files therefore participate in last-loaded-wins
  resolution. The typed pipeline database, raw gameplay tables, and
  authoritative sim are all built from that same already-layered source.
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
- Player-targeted `DamageModifier` technology effects mutate one authored
  weapon-type/damage-type pair without creating missing table entries. Direct
  hits, non-directional AOE, and AI attack-rating reconstruction all read the
  attacking player's value; other players retain the layered database base.
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
- Ordinary ranged attacks now launch from and aim at retail's synchronized
  simulation-bounding-box centers. Ground objects add their scenario-layered
  Y obstruction radius to the entity origin; flying objects retain their
  origin. Tracking retains that center offset as the target moves, projectile
  and AOE intersections share the same simulation bounds, and ballistic target
  radius uses the retail maximum of the X/Z obstruction radii. Target locations
  are clamped above the loaded scenario XSD terrain before flight begins.
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
- Scenario-layered `ExternalShield` prototype flags select retail's special
  shield volume instead of the ordinary object AABB. Its collision radius is
  derived from the X/Z obstruction radii and clipped by the Y radius. These
  shields intercept projectiles even without `ProjectileObstructable`; a
  projectile whose immutable launch point is already inside a non-wall shield
  may escape, while `_WallShield` and `_BaseShield` retain the retail exception
  that catches such shots. External-shield impacts preserve the projectile's
  post-step position rather than applying ordinary 0.01-unit penetration.
- XSD height queries now reproduce retail's bilinear `getHeightRaycast`
  interpolation. Projectile terrain checks preserve the short-segment
  8-unit/0.25-height early-outs, intersect the two authored triangles in each
  crossed height tile, and place ground zero 0.25 units above the surface.
  Unit hits are resolved first; a terrain or targets-foot impact has no direct
  primary target and therefore supplies the full base-damage pool to AOE.
- Impact damage applies attacker damage, optional height bonus, the attacking
  player's weapon type versus damage type, and the target's live damage-taken
  multiplier.
  Direct projectile and instant-hit vectors select retail's six front/side/back
  armor sectors. A matching secondary squad mode (notably Cover) uses its
  authored sector and falls back to Normal only when that sector is absent.
  Authored AOE damage remains non-directional, so it deliberately uses front
  armor regardless of the projectile travel vector.
- Positive `AOERadius` weapons use retail's shared damage-pool contract for
  both instant hits and projectile impacts. The primary factor is applied
  first, the remaining pool is capped before weapon-type modifiers, nonlinear
  damage is normalized across eligible simulation bounds, and linear damage
  consumes base damage nearest-target-first. The two-interval distance falloff,
  vertical-axis ignore, friendly-fire filtering, attacker exclusion, and
  uncapped non-Cover Gaia damage all come from the scenario-layered tactic.
- Squads retain retail's synchronized damage-proxy reference. Raw damage and
  weapon hits intended for a member follow live proxy squads to child zero,
  including valid proxy chains; dead, empty, missing, and cyclic references
  terminate deterministically. The proxy owns shields, construction state,
  incoming-damage scalar, hit points, revival, and damage notifications, while
  the originally requested unit still supplies weapon-vs-armor type and squad
  mode, matching `BDamageHelper`'s target-ID quirk. Removing a proxy squad
  clears every reference immediately, and proxy state participates in the
  world checksum.
- Scenario-layered persistent `PlasmaShieldGen` actions create an authoritative
  main base-shield squad and proxy the base anchor into it. Generator count
  applies retail's `1/N` incoming-damage scalar, primary ownership transfers
  deterministically, base hit-point percentage drives the shield object's hit
  points, and the authored action duration plus shield `BuildPoints` gate
  reconstruction after combat. Eligible socket buildings receive their
  authored `ShieldType` subshield squads; those proxies relay through the main
  shield and mirror its shield percentage. All generated squads and lifecycle
  timers are ordinary checksummed sim state.
- Work command order 9 now drives persistent Follow joins. The scenario-layered
  monitor tactic supplies its work range and `BubbleShield` action, while raw
  layered `ShieldBubbleTypes` data selects the default or target-specific
  shield squad. Once in range, the sim makes the monitor unselectable and
  invulnerable, creates the mapped bubble at the target transform with 0.1
  initial shield points, owns its damage proxy, and recreates it only after the
  strict player/leader shield delay. At connection the monitor adopts twice
  the protected squad's effective speed, matching retail Follow joins. Moving
  targets, target loss, source loss, and shield destruction are resolved
  entirely by authoritative squad state.
- FollowAttack uses the same persistent Join state and authored work range, but
  keeps its source operational and synchronized to the moving target rather
  than applying Follow's hidden/invulnerable connection contract.
- Raw scenario-layered `MergedSquads` entries create the retail
  `merged_{target}_{joining}` synthetic prototype identities after the typed
  squad table. A compatible Merge transfers the joining unit into the target,
  preserves population accounting, applies Join modifiers only to the original
  target members, and deterministically restores the source or target prototype
  when one side of the merged composition is eliminated. In
  `byCombatValue` mode the sim uses retail's prototype values: the joining
  object's combat value divided by the target proto-squad's summed member
  combat value. Damage becomes `1 + ratio * DamageBuffFactor`; positive
  `DamageTakenBuffFactor` becomes `1 / (1 + ratio / factor)`. A missing or
  nonpositive value leaves both layers at identity. Player-specific transformed
  target prototypes are resolved before this calculation for both Merge and
  Board.
- Board joins reserve their channel while approaching and during the authored
  timer, mark the target as being boarded and nonattackable, then transfer enemy
  ownership and force-contain the joining Spartan in the captured target. The
  contained source is invulnerable and unselectable; target loss releases it,
  clears Join modifiers, and applies the authored revert-health fraction only
  after a completed takeover. Ownership, containment, timers, modifier layers,
  and cleanup all live in checksummed sim state. `veterancyOverride` applies
  the Spartan prototype's earned-level damage, damage-taken, velocity, accuracy,
  work-rate, and weapon-range multipliers to the captured unit and retains the
  action's extra `levels` as an effective-level contribution. The authored
  `ProtoObject` is created as an attached class-zero sim entity and removed by
  normal target/Join cleanup. Omitted `ObjectClass` uses retail's class-zero
  `Object` default, which is required by the shipped `fx_hijacked` prototype.
- Scenario-layered object veterancy entries now build each proto squad's retail
  level thresholds by summing member XP times authored count. Squads retain
  checksummed committed XP, attack-action XP bank, and earned level. Thresholds
  use retail's strict `XP > required` comparison, and each earned level applies
  damage, damage-taken, velocity, accuracy, work-rate, and weapon-range factors
  to every live member one level at a time.
- Weapon damage awards the attacking unit's parent squad only the fraction of
  the target prototype's bounty represented by actual HP loss divided by base
  prototype hit points. Shield absorption earns no XP. Bounty remains banked
  during the attack action, is discarded when a different order replaces it,
  and is committed when the ordered unit or full squad is defeated.
- A vehicle containing a veterancy-override Spartan splits incoming XP between
  vehicle and Spartan in proportion to their scenario-layered proto-squad
  combat values. Both banks are then applied in retail order. Later Spartan
  levels update its own live member and apply that Spartan level's object
  modifiers to the captured vehicle; effective vehicle veterancy includes its
  own level, the Spartan's current level, and the Board action's authored extra
  levels. Trigger effect DBID 852 (`AddXP`) uses the retail scalar-before-list
  signature and preserves duplicate squad-list applications.
- The authoritative world retains a checksummed effective veterancy gate. The
  installed-scenario loader reads the raw root-level `AllowVeterancy` value
  before creating any entities, including retail's missing-element `false` and
  present-empty-element `true` defaults. A disabled scenario suppresses
  prototype starting levels and their unit scalars, combat and trigger XP,
  Board level inheritance, and later Spartan-to-vehicle propagation. The
  `ScenarioData`-only helper defaults the gate on because that lossy pipeline
  representation does not currently retain the raw flag.
- When an AOE primary target resolves through a proxy to an external shield,
  other candidate centers within the shield's authored X/Y volume are removed
  before damage-pool normalization. The original primary remains eligible for
  its retail splash pass, whose damage redirects back to the proxy.
- Retail `Shielded` damage-type entries are treated as shield coverage, not as
  an armor multiplier. `Full` shields absorb every hit; `FrontHalf` shields
  absorb non-directional damage and direct vectors traveling against the
  target's forward, while rear direct hits bypass them. Units spawn with empty
  shields and immediately request recharge. A covered hit drains shields first
  and sends same-hit overflow to hit points after existing damage scalars.
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
- The UGX scene roster is derived from live sim visual-object, unit, and
  projectile pools. This includes Board's attached `fx_hijacked` object; its
  root transform follows the target in the sim, while GPU presentation merely
  refreshes that transform by entity ID. The renderer contains no duplicate
  movement, targeting, firing, damage, Join, or attachment lifecycle logic.

## Deliberate next boundaries

This is the deterministic baseline, not a claim of complete combat parity.
Remaining retail systems include unsupported technology effect families,
ability ammunition, non-attack
recovery start events, tactic-state membership, lockdown minimum-range
behavior, remaining garrison and melee-attacker predicates, visual bone/animation
hardpoint overrides, targeted hit-zone offsets and oriented hit-zone/visual-mesh
projectile intersection, hit-zone shields, broader runtime `Unhittable` and
invulnerability controls, and destructible
non-unit AOE recipients, dodge/deflect, sticky visual-mesh/bone intersections,
timer damage reapplication, beam/needler behaviors, hero death/revival
presentation, death effects, and ranged-action savegame compatibility. Join
boundaries still include manual Board disconnect and fatality
animation/controller presentation. Automatic discovery of retail's separate
`cConfigVeterancy` runtime definition and veterancy presentation effects remain
to be reconstructed; scenario `AllowVeterancy` is authoritative today.

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
The catalog check also reads repeated raw `DamageType` nodes from the mounted
object table and verifies the shipped Jackal's `FrontHalf` shield plus its
Normal `Light` and Cover `LightInCover` armor modes. This guards against the
typed object schema collapsing those repeated nodes before they reach the sim.
It also spawns the shipped `env_generic_wallshield_01` from that layered table
and verifies that `ExternalShield` plus the authored `0.5/20/39` obstruction
radii reach the authoritative unit state used by collision and AOE queries.
The protection check resolves the shipped Covenant generator's 30-second
rebuild and 5-second attack wait, creates its 50,000-point base shield, and
verifies the real anchor proxy. It also resolves `for_air_monitor_04`'s Follow
join, spawns a real Scorpion and Protector monitor, selects
`sys_bubbleshield_med_01` from the raw layered squad table, and verifies bubble
creation, proxy ownership, and target-loss teardown in the sim.
It also resolves the shipped Spartan `InfantryJoin` through the raw
`MergedSquads` compatibility table and verifies a real Spartan/Marine Merge,
then resolves `VehicleTakeOver` against a Scorpion and verifies the authored
8-unit work range, 8-second Board timer, 0.5 revert fraction, `HijackIdle`
animation, one effective level, static `1.15/0.87` modifier factors, and
`fx_hijacked` attachment. The completed real takeover verifies ownership
transfer, containment, the live attached class-zero sim object, and release on
target death. It additionally verifies the shipped Spartan and Scorpion combat
values plus Spartan XP thresholds from the mounted database, then grants one XP
to the captured Scorpion and confirms the proportional Board split commits to
both authoritative squads without duplication.

The non-ignored `scenario-database-layering` test builds encrypted synthetic
`root.era` and scenario archives. It proves that scenario-local `gamedata.xml`
and `squads.xml` replace their base copies before the pipeline database is
parsed, that the resulting difficulty initializes the authoritative player,
and that the raw scenario-local `MergedSquads` mapping enters the same gameplay
catalog.

The same installed-data test now also spawns a shipped Barracks, issues the
retail building command for the first Marine upgrade, verifies its authored
resource cost and research duration, and observes activation through the
authoritative simulation. See `docs/research.md` for the recovered production
contract and remaining boundaries.

```powershell
$env:OPENENSEMBLE_GAME_DIR='C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE'
cargo test -p sim --test scenario-asset-loading -- --ignored
```
