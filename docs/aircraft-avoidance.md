# Aircraft avoidance and lethal crash lifecycle

`sim` owns aircraft separation, lethal crash state, movement, collision, damage, and impact
requests. The renderer projects that authoritative `World`; it does not select crash targets or
re-run flight and combat rules.

## Scenario-layered inputs

The scenario ERA is mounted before database tables and tactics are parsed. `GameplayCatalog`
therefore compiles each persistent `AvoidCollisionAir` action from the same layered content used to
spawn the scenario. An `AirAvoidanceActionProfile` retains:

- action name, authored start-disabled state, `Stationary`, `AvoidOnly`, and `DetonateOnDeath`;
- hover-altitude offset and maximum attack-depression angle, with retail's 50-degree default;
- the optional kamikaze weapon's damage, range, weapon type, area-damage distribution, and impact
  effect.

The same layered pass resolves every referenced `.physics` file once and records the retail
movement-action selection. Hawk, Hornet, Vulture, Banshee, Vampire, and Sentinel physics types use
physics hover; a prototype with no `PhysicsInfo` uses `MoveAir` for `AirMovement`, or for `Flying`
when the synchronized `EnableFlight` config is defined. A prototype that has `PhysicsInfo` never
falls back to either flag. This immutable rule is copied into checksummed world prototype state and
applied by the shared scenario, trigger, training, and runtime spawn path.

The installed-data test pins both shipped Banshee profiles and its squad's 45-unit leash,
15-unit deadzone, and 2,500 ms recall delay. Its normal profile is enabled without a weapon;
`cov_banshee_upgrade3` disables that profile and enables `KamikazeOnDeath`, whose layered
`KamikazeDive` weapon supplies 1,500 damage, 65 range, a 6-unit area, and the `Tankshell` impact.

## Authoritative runtime contract

- Action enablement combines authored defaults, player technology, and live per-unit overrides.
  Switching profiles preserves live flight state instead of creating a second renderer-side mode.
- Every enabled aircraft participates in deterministic claimed-air-spot separation. `Stationary`
  and `AvoidOnly` retain their authored meanings, exact overlaps consume synchronized randomness,
  playable bounds are inset by the aircraft obstruction radius, and proposed avoidance positions
  reject live `ObstructsAir` prototypes.
- New aircraft cap their parent squad at 8 units per second until the strict retail
  `birth_timer > 1.5` transition. Attack steering honors the profile's maximum target-depression
  angle. Units with an authored zero reverse speed, including the shipped Banshee, strafe laterally
  when too steep; absent reverse speed uses retail's `-1` maximum-speed fallback and backs away.
- Hover flight samples the three retail 4-unit chassis probes and a rotating 0.2-to-1.0-second
  future probe. Its checksummed five-height ring, vertical velocity, authored hover offset,
  16-unit terrain fallback, 13-unit minimum clearance, slower descent, and collision-avoidance
  altitude offset produce the authoritative squad height. Ground-space order Y never competes with
  that controller.
- Non-physics `MoveAir` units retain independent checksummed action state instead of being snapped
  back onto generic formation transforms. They turn toward the moving squad center with retail's
  turn-rate acceleration, ease toward 60 percent of maximum speed, and select terrain-relative
  altitude increments every four seconds. Flood aircraft use the retail one-second randomized
  70-to-100-percent speed goal and two-second altitude interval. Authored `FlightLevel` (including
  negative values) is projected from the layered prototype, with retail's 10-unit default.
- `MoveAir` initialization is separate from squad `FlyIn` birth. `FlyIn` raises an aircraft leader
  100 units above the trainer's ordinary placement and consumes no landing pad. A `MoveAir` member
  requests one of the controller's eight pads only when its squad has an authored train-limit link
  back to that building. It captures the base, pad, and facing in checksummed state, teleports to
  the pad, refills to maximum ammunition on every parked update, and waits for an explicit launch.
  Conjured or ordinary unlinked aircraft instead pass through the source None/Pathing states and
  launch automatically. Launch does not free the pad; retail releases it when `MoveAir`
  disconnects.
- `MoveAir` also owns its checksummed combat-flight cycle. Navigate blocks the shared ranged
  executor and sends an attacker toward the midpoint between its target and squad; Strafe retains
  that run until the source timer expires; LaunchHover permits firing for two seconds and requests
  zero Flood goal speed; ReturnToSquad blocks firing for two seconds. A unit farther than the
  greater of 35 units or half its squad leash is forced back for at least one second. The existing
  combat executor remains responsible for target rules, weapon selection, timing, ammunition, and
  damage.
- The legacy trigger carpet-bomb action is also simulation-owned. It sets retail's persistent
  `IgnoreLeash` squad flag, builds up to 100 evenly spaced ground targets around the authored run
  center, explicitly launches every `MoveAir` child, and feeds each position through that same
  ranged executor. The lead aircraft drags the squad center within the source's 40-unit attack
  envelope; after the last run it drags toward its captured base within 100 units and sets the
  checksummed return-to-base flag. The named source's landing block is commented out, so the sim
  does not invent pad release, landing, or ammunition refill during return.
- Collision correction drags the squad leash without changing its anchor. Once the authored leash
  distance plus deadzone is exceeded, an idle aircraft returns to that anchor only when no friendly
  claimed air spot occupies it; ordinary movement advances both positions together.
- Lethal damage configures the current layered profile before mutating health. Eligible aircraft
  stop at one hit point, become unselectable, record the responsible unit/player/team, and enter a
  checksummed pending-crash phase. `DetonateOnDeath` and externally suppressed aircraft die
  normally.
- Crash targeting considers only alive, visible, non-flying enemies within weapon range, rejects
  units attached to the aircraft, and chooses the positive forward-dot candidate closest to its
  nose. With no target it chooses the retail randomized forward/right terrain point 20 units below
  ground.
- The first crash update clears attack orders, makes the aircraft untargetable, and rolls the retail
  2,500-to-4,999 ms fallback deadline; rolls below 3,000 detonate immediately. A live target is
  pursued at three times normal speed.
- Swept simulation bounds and terrain contact trigger the authored weapon damage, including live
  damage scalars, armor type, and area distribution. The sim then applies retail's separate final
  10 damage to the aircraft using the original killer, banks the remaining hit-point bounty, and
  retains killer entity/player/team and weapon type in checksummed unit death state before the
  force-kill. The sim emits the impact request from that same lifecycle.
- `RepairOther`, ordinary operational-owner checks, and Wave cooperate with this state. Repair
  rejects crashing leaders, while Wave disables kamikaze during pull and restores it on release;
  that suppression also survives first-time profile reconciliation.

## Renderer projection

`render::ugx::UnitScene` derives each placement from `sim::World`. A targeted crashing aircraft
selects the authored `Kamikaze` presentation animation, uses the simulation clock, and consumes the
unit's full three-dimensional forward vector so the model pitches with the authoritative dive.
Authored animation attachments, including the Banshee kamikaze fire particle, follow the selected
clip. Target choice, pitch calculation, impact timing, and damage remain exclusively in `sim`.

## Validation

The normal unit suite uses synthetic tactics to cover profile compilation, birth speed, terrain
hover, reverse/strafe choice, air obstructions, anchor/leash recovery, exact-overlap determinism,
visibility filtering, Wave suppression, crash targeting, collision, area damage, and impact
emission. The opt-in installed tests exercise the same path through Blood Gulch, the
scenario-layered database, real Banshee/Marine spawns, authored leash metadata, terrain hover,
technology, the shipped four-member Flood swarm's independent `MoveAir` motion, VIS animation data,
the ranged Strafe/LaunchHover firing gate, source-exact air-base layouts, the shipped swarm's lack
of a train-limit pad reservation, and the renderer:

```powershell
$env:OPENENSEMBLE_GAME_DIR = 'C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE'
cargo test -p sim --test scenario-air-avoidance -- --ignored
cargo test -p sim --test scenario-air-traffic-control -- --ignored
cargo test -p render --test scenario-air-avoidance -- --ignored
```

## Remaining parity boundaries

The terrain fallback and vertical controller are authoritative, but the sim does not yet load the
retail flight-height mesh or reproduce Havok's mass/inertia impulses, chassis yaw/pitch/roll springs,
pivoting engines, or physics-shape refinement used for building impacts. Squad stasis counters are
not yet modeled, so stasis cannot force the fallback deadline to zero. Death audio,
selection-manager notification, shockwave instances, rumble, and camera shake also await their
shared authoritative presentation systems. The named source's return-to-base landing block is
commented out, and its
Extend/Dip/Climb/KamikazeDive and roll/Immelman branches have no live caller or state assignment;
they are documented dormant code, not retail behavior to invent in the sim.

Behavior was recovered from the named 2008 source in `unit.cpp`, `unitactionmoveair.cpp`,
`unitactionavoidcollisionair.cpp`, `physicshoverflightaction.cpp`, `object.cpp`, `protoobject.cpp`,
`unitactionairtrafficcontrol.cpp`, `unitactionbuilding.cpp`, `powerwave.cpp`,
`unitactionrepairother.cpp`, `powercleansing.cpp`, and `powermanager.cpp`, plus the scenario-layered
shipped data. No additional IDA functions were inspected for this slice.
