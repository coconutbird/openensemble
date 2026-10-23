# Movement and physics reverse-engineering map

This map records the ground-movement, formation, Warthog, and physics behavior
recovered from the Halo Wars Definitive Edition `xgameFinal.exe` IDA database.
Names are analyst-assigned from decompilation, vtable comparison, embedded source
diagnostics, database values, and call sites; they are not original debug symbols.

## Authoritative runtime flow

```text
base ERAs + scenario ERA
          │
          ▼
sim::load_scenario_from_game_dir
  ├─ parses the layered database
  ├─ resolves referenced .physics → .blueprint → .shp chains
  ├─ parses SCN players, starts, and objects
  ├─ creates player-owned initial base anchors
  └─ creates sim::World entities
          │
          ▼
render::ugx::UnitScene (entity IDs + decoded visuals)
          │
          ▼ every frame
UnitSceneRenderer reads position/facing from sim::World
```

The scenario ERA is pushed before `Database::load`, so a database table stored
in the scenario participates in the pipeline's last-loaded-wins resolution.
This is a mod extension: retail loads the database once at boot (`BGame_setup`
`0x140232EE0`), and no shipped scenario ERA has `data\*` files.
The sim then resolves supported vehicle physics through that same layered
source. A scenario-local `.physics`, `.blueprint`, or `.shp` therefore changes
the authoritative spawn configuration rather than only presentation data. The
renderer does not expand SCN objects or own a movement transform. It only
presents entity state owned by `sim::World`.

Player starts are assigned deterministically in SCN order. When leader data is
available, the sim materializes the leader starting unit's completed
`BuildOther` base; otherwise it uses the layered game-data
`SkirmishEmptyBaseObject` mapping. Either path creates a normal building entity
and registers it as the player's authoritative `Base` anchor.

Scenario objects, start positions, unit and rally start markers, design spheres and
design lines keep their authored `.scn` coordinates. The `.scn` frame is the world frame
of the simulation and of the XSD terrain. `BScenario_loadObject` @ `0x14037D6A0` and
`BScenario_loadPosition` @ `0x14037F430` read `Position`, `Forward` and `Right` with
`BXMLAttribute_getValueAsVector` @ `0x1406217A0` and only normalize the directions. A
ground object then takes the terrain height at its X and Z. The object matrix has the
rows right, up, forward and position, with `right = up × forward`
(`BWorld_createEntity` @ `0x1405C7D50`).

The opt-in real-asset integration test loads both the database and Blood Gulch
from an installed game, verifies all canonical database tables remain
resolvable beside the final scenario layer, and checks that SCN players and
initial bases reach the sim:

```powershell
$env:OPENENSEMBLE_GAME_DIR = 'C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE'
cargo test -p sim --test scenario-asset-loading -- --ignored
```

## Generic ground movement and Marine formation path

Stock Marines use the generic `BUnitActionMove` path. The relevant vtable is at
`off_1411922F0`.

| Address | IDA name | Recovered role |
| --- | --- | --- |
| `0x14053DC40` | `BUnitActionMove_start` | Attaches the action and initializes movement state. |
| `0x14053DD60` | `BUnitActionMove_stop` | Clears movement state, target ownership, and path data. |
| `0x14053DE00` | `BUnitActionMove_reset` | Resets target, velocity, opponent, timer, and path fields. |
| `0x14053DF30` | `BUnitActionMove_setMoveState` | Applies state and animation side effects. |
| `0x14053E0A0` | `BUnitActionMove_update` | Main ground-movement state machine. |
| `0x14053ECC0` | `BUnitActionMove_advancePath` | Advances waypoints and linked path nodes. |
| `0x14053F060` | `BUnitActionMove_requestPath` | Requests navigation data for a destination. |
| `0x14053F280` | `BUnitActionMove_refreshPath` | Retries path generation at successive path levels. |
| `0x14053FA10` | `BUnitActionMove_computeGroundStep` | Computes the next ground position and facing. |
| `0x1405401B0` | `BUnitActionMove_computeFormationStep` | Computes formation-following position and steering. |
| `0x1405403F0` | `BUnitActionMove_updateGroundMotion` | Virtual ground-motion update and transform application. |
| `0x140540920` | `BUnitActionMove_updateFormationMotion` | Formation-motion update wrapper. |
| `0x140540A40` | `BUnitActionMove_applyGroundTransform` | Applies position and an orthonormal facing basis. |
| `0x140540CA0` | `BUnitActionMove_resolveTargetPosition` | Resolves target/opponent/formation position. |
| `0x140540EA0` | `BUnitActionMove_resolveSteeringTarget` | Resolves steering target, distance, and travel time. |
| `0x1405411E0` | `BUnitActionMove_applyFormationSteering` | Adjusts waypoints for formation and avoidance. |
| `0x140541E70` | `BUnitActionMove_limitTurnDirection` | Limits facing change by the prototype turn rate. |

The generic action's `mPathMoveData` serialization is mapped as
`BPathMoveData_writeSavegame` (`0x140542AA0`) and
`BPathMoveData_readSavegame` (`0x140542C80`). The action itself serializes at
`BUnitActionMove_writeSavegame` (`0x1405422E0`) and reads at
`BUnitActionMove_readSavegame` (`0x1405426B0`). Embedded diagnostics confirm
fields including `mTarget`, `mVelocity`, `mOppID`, `mPathMoveData`, and the path
refresh/formation flags.

## Warthog vehicle path

| Address | IDA name | Recovered role |
| --- | --- | --- |
| `0x140545520` | `BUnitActionMoveWarthog_applyTimedImpulse` | Interpolates and applies one configured impulse. |
| `0x1405456C0` | `BUnitActionMoveWarthog_start` | Activates/configures the physics body and starts movement. |
| `0x140545750` | `BUnitActionMoveWarthog_stop` | Stops flight audio, restores motion type, and stops movement. |
| `0x140545800` | `BUnitActionMoveWarthog_reset` | Resets target, impulses, current speed, position, and flags. |
| `0x140545890` | `BUnitActionMoveWarthog_update` | Per-tick state, impulse, movement, and velocity synchronization. |
| `0x140545A10` | `BUnitActionMoveWarthog_updateMovement` | Advances the route and handles target/collision outcomes. |
| `0x140545CB0` | `BUnitActionMoveWarthog_computeTargetVelocity` | Accelerates/brakes at 60 units/s², caps speed, and selects waypoints. |
| `0x140546170` | `BUnitActionMoveWarthog_collectTerrainImpulses` | Converts nearby terrain responses into timed impulses. |
| `0x1405465C0` | `BUnitActionMoveWarthog_updateImpulses` | Applies and expires active impulses. |
| `0x140546650` | `BUnitActionMoveWarthog_updateSkiddingState` | Derives skid state from planar motion and emits skid events. |
| `0x140546950` | `BUnitActionMoveWarthog_writeSavegame` | Writes Warthog movement state. |
| `0x140546D70` | `BUnitActionMoveWarthog_readSavegame` | Restores Warthog movement state. |

The Warthog database profile selects `Vehicle="warthog"`; the Rust profile
keeps its database acceleration, maximum speed, obstruction extents, and turn
radius range rather than hard-coding renderer motion.

## Physics loading and runtime objects

| Address | IDA name | Recovered role |
| --- | --- | --- |
| `0x1402D3CD0` | `BPhysicsInfo_load` | Loads `.physics`, blueprint, vehicle, projectile, terrain, and death settings. |
| `0x1402D5920` | `PhysicsInfoManager_getOrCreateId` | Resolves or lazily creates a physics-info ID. |
| `0x1402D5B50` | `PhysicsInfoManager_getById` | Bounds-checks and lazily resolves a record. |
| `0x140547880` | `BUnitActionPhysics_update` | Synchronizes the physics transform and processes impacts/completion. |
| `0x140548480` | `BUnitActionPhysics_recordCollision` | Records the first collision position and surface. |
| `0x1405485E0` | `BUnitActionPhysics_tryAdjustTrajectoryForTerrain` | Revises a ballistic trajectory around terrain when required. |
| `0x1407B6DB0` | `PhysicsObject_applyImpulseAtPoint` | Activates a non-fixed body and applies an impulse at a point. |
| `0x1407B7060` | `PhysicsObject_addCollisionListener` | Registers backend callbacks and a listener. |
| `0x1407B70B0` | `PhysicsObject_removeCollisionListener` | Removes a listener and unregisters the final callback. |
| `0x1407B8C10` | `PhysicsObject_writeSavegame` | Writes body parameters, transform, velocity, state, and callback flags. |
| `0x1407B9460` | `PhysicsObject_readSavegame` | Restores body parameters, transform, velocity, state, and callback flags. |

The pipeline's lightweight runtime world intentionally does not resolve full
physics chains. `gameplay/vehicle_physics.rs` follows each layered
`PhysicsInfo` reference itself, reads the primary blueprint and Havok box
shape, and sanitizes its material, center offset, and half extents. Supported
profiles are copied into the world's checksummed prototype catalog before any
scenario objects spawn, so later training, trigger creation, and protection
systems receive the same body as initial placements.

Ghosts instantiate that body through `entities/units/vehicle.rs`. Their
zero-speed response uses the `cFwdK = 2.5` proportional term in the named
recovered `BPhysicsGhostAction::calcMovement` source; contacts remain owned by
the deterministic sim physics loop. The stock Warthog continues through its
specialized movement profile while the layered Warthog chain is retained in
the immutable catalog for subsequent controller parity work.

## Ground contact and orientation

### Retail rules

`BEntity` stores the frame as rows at `+0x20` (right), `+0x30` (up) and `+0x40` (forward).
The position is at `+0x10`. The flag byte `+0xC0` holds `mFlagTiesToGround` (`0x10`),
`mFlagUseMaxHeight` (`0x20`) and `mFlagPhysicsControl` (`0x40`). `BEntity_writeSavegame`
(`0x140207090`) gives the flag names and bits.

`BEntity_tieToGround` (`0x140202EB0`) does these steps:

1. Return if `mFlagTiesToGround` is clear or `mFlagPhysicsControl` is set.
2. For a flying entity with a maximum height, apply the flying height limits and return.
3. For a unit whose prototype has `OrientUnitWithGround` (`BProtoObject` data `+0x428 & 8`),
   call `BEntity_orientWithGround`. Then call `BEntity_filterOrientation` with weight 0.1 if
   the prototype has `FilterOrient` (`+0x42F & 1`) and the previous `up.y` is not exactly 1.
   `DoNotFilterOrient` clears `FilterOrient`.
4. Skip a squad that holds one Havok vehicle.
5. Otherwise set Y to the clamped terrain height (`BTerrainSimRep_getHeight` @ `0x140062EB0`).
   The up axis does not change.

`BEntity_orientWithGround` (`0x1402032A0`) samples the clamped terrain height at the four
corners `position ± 1.25 right ± 2.25 forward`. These constants are the same for all units.

- Y is the mean of the four heights.
- The diagonal with the higher mean height and half of the other diagonal give the normal
  (`normal = diagonal × other`, turned so that `normal.y ≥ 0`).
- The up axis is the normal. The forward axis is projected onto the terrain plane, and
  `right = up × forward`.

`BEntity_filterOrientation` (`0x1402035F0`) blends the right and forward axes as
`0.1 new + 0.9 old`. It then sets `up = forward × right` and `right = up × forward`.

`mFlagTiesToGround` is the inverse of `NoTieToGround` (`BProtoObject` `+181 & 8`). The unit
setup code (`0x1403E0130`, `0x1404DCC80`) ties a new unit unless that bit is set.
`BScenario_loadObject` also reads `NoTieToGround` on a scenario object. Retail calls the tie
after a position change, for example in `BSquad_setPositionAndSyncChildren` (`0x14039FC30`) for
the squad and each member without `mFlagPhysicsControl`.

In the shipped `objects.xml`, all ground vehicles have `OrientUnitWithGround` (Warthog, Ghost,
Scorpion, Wraith, Cobra, Grizzly, Wolverine, Elephant, Chopper, Gremlin, Locust). Infantry and
buildings do not have it, so they stay upright. The Flood infection forms also have
`DoNotFilterOrient`.

A Havok vehicle does not use the tie. `BTerrainSimRep_initPhysics` (`0x140063F20`) calls
`BTerrainPhysics_createHeightFieldBody` (`0x14041B500`). It makes a fixed body with a
`hkpSampledHeightFieldShape` (`0x140874D40`) over the simulation height grid, with a clamped
border of 10 samples. `BTerrainHeightFieldShape_getHeightAt` (`0x14041B3B0`) reads the same
half-float heights as the simulation queries. The Warthog box body
(`warthog.blueprint`, `warthog.shp`) collides with that shape. `BUnitActionPhysics_update`
(`0x140547C73`–`0x140547DC4`) reads the body position and rotation quaternion back. It writes
the entity position and its right, up and forward rows. The Warthog `.physics` file selects the
code vehicle `warthog` and has no suspension data.

### Simulation model

- `BaseEntity::up` is the up axis. `BaseEntity::forward` stays the heading. The renderer
  projects the heading onto the plane of the up axis.
- Before each substep, `World::sample_physics_ground` samples the terrain under each dynamic
  body with the `BEntity_orientWithGround` footprint. `PhysicsBody` collides with that plane
  for the step. A grounded body slides on the plane, so it climbs and descends with the
  terrain. It keeps contact while the terrain falls away by no more than the planar distance
  of the step. A steeper drop makes it airborne, and gravity acts on it. When grounded, the
  body's up axis is the plane normal. This replaces the Havok box and height-field contact.
- After the unit step and again after the squad member sync, `World::tie_units_to_ground`
  ties each ground unit with `mFlagTiesToGround`. The second pass also orients and filters
  units with `OrientUnitWithGround`. Other units keep world up. Physics-driven units, aircraft,
  buildings, sockets, contained, jumping and thrown units, and members of a pulled or jumping
  squad are not tied.
- Squads keep the plain terrain height and world up.
- The up axis is not in the sync checksum. It only changes the rendered frame, and no retail
  sync data for it was found. The ground-tie flags are in the checksum.

## Formation serialization

| Address | IDA name |
| --- | --- |
| `0x14003D040` | `BFormation2_writeSavegame` |
| `0x14003D600` | `BFormation2_readSavegame` |
| `0x14003DC50` | `BFormationPosition_writeSavegame` |
| `0x14003DF10` | `BFormationPosition_readSavegame` |

The serialized formation includes its position records, radii, type, line
ratio, children, prior forward vector, reassignment state, and dynamic-priority
flags. Each position includes its transform, line indices, priorities, entity
ID, flock velocity, position, and offset.

## Rust correspondence

| Behavior | Implementation |
| --- | --- |
| Deterministic fixed substeps and rigid-body state | `crates/sim/src/physics.rs` |
| Terrain plane contact and retail ground orientation | `crates/sim/src/physics/ground.rs` |
| Ground tie of units and physics bodies each substep | `crates/sim/src/world/terrain/ground_tie.rs` |
| Ground-tie flags of a unit | `entities/units/ground_tie.rs` |
| Vehicle acceleration, braking, yaw, turn radii | `entities/units/warthog.rs` plus `physics.rs` |
| Layered ground-vehicle physics chains | `gameplay/vehicle_physics.rs` |
| Spawned Ghost rigid-body configuration | `entities/units/vehicle.rs` |
| Marine velocity, acceleration, turn rate, and obstruction size | `entities/units/marine.rs` |
| Warthog squad physics anchor | `entities/squads/warthog.rs` and `world.rs` |
| Marine four-member Flock seed and local offsets | `entities/squads/marine.rs` and `entities/squads.rs` |
| Scenario/database-to-sim expansion | `scenario.rs` |
| Sim entity transforms (position, heading, up) to GPU presentation | `crates/render/src/ugx/scene.rs` |

Marine formation offsets are stored squad-local and rotated by the squad's live
facing. The stock database declares `Flock` but does not provide stable authored
random offsets, so the sim uses a deterministic centered 2-by-2 seed for its
four Marines. That seed is an implementation choice, not a recovered vanilla
constant.

Each non-physics member owns checksummed `BUnitActionMove`-style phase, interim
target, action velocity, and retry state. For a Move4 squad order, the sim follows
`BSquad::getChildInterimTarget_4`: it transforms the member's formation offset
at the current squad waypoint and orients that offset along the path. Members
inherit the squad action's accelerated velocity while it is working, then use
their own desired velocity to settle into the final formation.

The per-unit turn controller follows `BUnitActionMove::calcTurning`, including
the constant yaw limit, the 0.125/0.25/0.5/0.75 slowdown bands below
5.625/11.25/22.5/45 degrees, the turn-duration distance clamp, the 0.1-unit
Move4 completion epsilon, and the three-attempt Pathing retry guard. Ground
squad completion uses XZ distance, and the world samples the authoritative
scenario terrain for the moving squad center and each ground member. This
prevents elevation differences from leaving ground squads circling a target
they can never reach.

The complete inspected function set, including Warthog physics and air-move
comparison functions, is renamed and repeatably commented in the saved IDA
database beside `xgameFinal.exe`.

## Known gaps

- The sim has no pather. `Squad::update_movement_with_speed_limit` in
  `entities/squads.rs` steers each squad in a straight line to its current
  waypoint. Retail requests and refreshes multi-level paths in
  `BUnitActionMove_requestPath` and `BUnitActionMove_refreshPath`.
- The sim has no obstruction manager. Obstructions are circles and boxes in
  `physics.rs`. Dynamic repathing, wait ownership, and unit-to-unit avoidance
  are not implemented.
- The sim has no Havok rigid-body rotation. A physics vehicle takes its up axis from the
  terrain plane under it and does not pitch or roll in the air. The Warthog move action's
  terrain impulses (`BUnitActionMoveWarthog_collectTerrainImpulses`) are not modeled.
- Formation reassignment, formation priorities, and movement-animation
  controller transitions are not implemented.
- Renderer movement is a read-only projection of each unit's sim-owned action
  and transform.

See `simulation-coverage.md` for the full list of engine systems.
