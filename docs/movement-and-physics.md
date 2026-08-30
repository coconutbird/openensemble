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
The sim then resolves supported vehicle physics through that same layered
source. A scenario-local `.physics`, `.blueprint`, or `.shp` therefore changes
the authoritative spawn configuration rather than only presentation data. The
renderer no longer expands SCN objects or owns a second movement transform; it
only presents entity state owned by `sim::World`.

Player starts are assigned deterministically in SCN order. When leader data is
available, the sim materializes the leader starting unit's completed
`BuildOther` base; otherwise it uses the layered game-data
`SkirmishEmptyBaseObject` mapping. Either path creates a normal building entity
and registers it as the player's authoritative `Base` anchor.

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

Ghosts now instantiate that body through `entities/units/vehicle.rs`. Their
zero-speed response uses the `cFwdK = 2.5` proportional term in the named
recovered `BPhysicsGhostAction::calcMovement` source; contacts remain owned by
the deterministic sim physics loop. The stock Warthog continues through its
specialized movement profile while the layered Warthog chain is retained in
the immutable catalog for subsequent controller parity work.

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
| Vehicle acceleration, braking, yaw, turn radii | `entities/units/warthog.rs` plus `physics.rs` |
| Layered ground-vehicle physics chains | `gameplay/vehicle_physics.rs` |
| Spawned Ghost rigid-body configuration | `entities/units/vehicle.rs` |
| Marine velocity, acceleration, turn rate, and obstruction size | `entities/units/marine.rs` |
| Warthog squad physics anchor | `entities/squads/warthog.rs` and `world.rs` |
| Marine four-member Flock seed and local offsets | `entities/squads/marine.rs` and `entities/squads/mod.rs` |
| Scenario/database-to-sim expansion | `scenario.rs` |
| Sim entity transforms to GPU presentation | `crates/render/src/ugx/scene.rs` |

Marine formation offsets are stored squad-local and rotated by the squad's live
facing. The stock database declares `Flock` but does not provide stable authored
random offsets, so the MVP uses a deterministic centered 2-by-2 seed for its
four Marines. That seed is an implementation choice, not a recovered vanilla
constant.

Non-physics members no longer inherit a teleported copy of the squad transform.
Each member owns checksummed `BUnitActionMove`-style phase, interim target,
action velocity, and retry state. For a Move4 squad order, the sim follows
`BSquad::getChildInterimTarget_4`: it transforms the member's formation offset
at the current squad waypoint and orients that offset along the path. Members
inherit the squad action's accelerated velocity while it is working, then use
their own desired velocity to settle into the final formation.

The per-unit turn controller follows `BUnitActionMove::calcTurning`, including
the constant yaw limit, the 0.125/0.25/0.5/0.75 slowdown bands below
5.625/11.25/22.5/45 degrees, the turn-duration distance clamp, the 0.1-unit
Move4 completion epsilon, and the three-attempt Pathing retry guard. Ground
squad completion uses XZ distance, and the world samples the authoritative
scenario terrain for the moving squad center and each independently moving
member. This prevents elevation differences from leaving ground squads circling
a target they can never reach.

The remaining generic-movement boundary is the retail multi-level pather and
obstruction-manager response: reduced paths, dynamic repathing, wait ownership,
unit-to-unit avoidance, formation reassignment, and movement-animation
controller transitions are not yet complete. Renderer movement remains a
read-only projection of each unit's sim-owned action and transform.

The complete inspected function set, including Warthog physics and air-move
comparison functions, is renamed and repeatably commented in the saved IDA
database beside `xgameFinal.exe`.
