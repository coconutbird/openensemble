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

The Warthog database profile selects `Vehicle="warthog"`. The stock profile in
`entities/units/warthog.rs` keeps the database maximum speed, the obstruction extents, the turn
radius range and the 60 units/s² acceleration. The Warthog body comes from the `warthog` physics
info, as for every other vehicle (see [Physics world](#physics-world)).

## Physics loading

| Address | IDA name | Recovered role |
| --- | --- | --- |
| `0x1401F2340` | `BDatabase_preloadPhysicsFiles` | Loads every physics file at startup. |
| `0x1402D3CD0` | `BPhysicsInfo_load` | Loads `.physics`, blueprint, vehicle, projectile, terrain, and death settings. |
| `0x1402D5920` | `PhysicsInfoManager_getOrCreateId` | Resolves or lazily creates a physics-info ID. |
| `0x1402D5B50` | `PhysicsInfoManager_getById` | Bounds-checks and lazily resolves a record. |
| `0x1407BD0C0` | `BShape_load` | Loads a `.shp` shape. |
| `0x1407B6DB0` | `PhysicsObject_applyImpulseAtPoint` | Activates a non-fixed body and applies an impulse at a point. |
| `0x1407B7060` | `PhysicsObject_addCollisionListener` | Registers backend callbacks and a listener. |
| `0x1407B70B0` | `PhysicsObject_removeCollisionListener` | Removes a listener and unregisters the final callback. |
| `0x1407B8C10` | `PhysicsObject_writeSavegame` | Writes body parameters, transform, velocity, state, and callback flags. |
| `0x1407B9460` | `PhysicsObject_readSavegame` | Restores body parameters, transform, velocity, state, and callback flags. |

`gameplay/physics_data.rs` makes one body template for each physics info of the layered
`PhysicsLibrary`. Each proto object then gets its `<PhysicsInfo>` and `<PhysicsReplacementInfo>`
templates (`BProtoObject_load`, `0x140341620`). `physics/template.rs` holds the body data of a
template: the blueprint shape, mass, center of mass offset, material, damping, `CenterOffset`,
vehicle type and vehicle info. `physics/shape.rs` maps each loaded `.shp` shape to one shape of
the physics world. A blueprint without a shape gets the box of its half extents. A scenario-local
`.physics`, `.blueprint` or `.shp` file therefore changes the body of later spawns.

The physics float text follows the engine `atof`: the text becomes a double, then rounds to a
float (`convertTokenToVector`, `cvtsd2ss` at `0x140687E2D`). The `ensemble-assets` database
does this in `text::atof`.

## Physics world

Retail steps an `hkpWorld` (Havok 5.1) inside the simulation. The `crates/physics` crate
rebuilds that world. It has no game rules. The `sim` module `crates/sim/src/physics.rs` holds the
game rules around it. `SimPhysics` (`physics/state.rs`) is the physics of one sim `World`
(`BPhysicsWorld`, `BWorld+0x250`).

### Constants

`WorldConfig::default` (`crates/physics/src/world.rs`) has the retail values:

| Value | Retail |
| --- | --- |
| Gravity | (0, −21.7, 0) |
| Collision tolerance | 0.1 |
| Solver | 4 iterations, tau 0.6, damping 1.0 |
| Deactivation | on, reference distance 0.02 |
| Longest substep (`MAX_SUBSTEP_SECONDS`) | 0.0125 s |
| Default convex radius (`DEFAULT_CONVEX_RADIUS`) | 0.05 |
| Velocity limit of a body | `hkUFloat8` of 200, which is 202.8 |
| Broadphase world | cube of edge 2850 about the origin (`setBroadPhaseWorldSize`, `0x1407BB414`) |

`SimPhysics::new` follows `BPhysicsWorld_init` (`0x1407BB390`):

1. Make the world with the retail settings and the retail layer filter
   (`GroupFilter::RETAIL_DISABLED_PAIRS`, `crates/physics/src/filter.rs`).
2. Make the five shared filter infos in retail order: Vehicles (`+0x58`), `NoTerrainVehicles`
   (`+0x5C`), Phantoms (`+0x60`), Buildings (`+0x64`) and Uncollidables (`+0x68`). Each has its
   own system group, so bodies that share an info do not collide with each other.
3. Make the shared 22-vertex vehicle hull (`BPhysicsWorld+0x10`, `physics/vehicle_hull.rs`).

The retail filter disables these layer pairs: Phantoms–Phantoms, Objects–Objects,
Vehicles–Buildings, Vehicles–Objects, `NoTerrainVehicles` with Buildings, Objects, Terrain and
Vehicles, and Uncollidables with Objects, Vehicles, `NoTerrainVehicles` and Buildings. Thus a
vehicle never touches a building, and body-to-body contacts occur mainly between objects and
buildings.

The world keeps the default `BROADPHASE_BORDER_ASSERT` border. A body whose broadphase box
reaches a face of the broadphase world becomes fixed: it loses its velocity and sleeps
(`hkpBroadPhaseBorder_maxPositionExceededCallback`, `0x14088AFD0`). The physics world does
this when it adds the body and at the end of each step. Thus a physics replacement that is
outside the world, or that falls out of it, counts as asleep, and its cleanup removes it.

### Step rule and update order

`World::update_entities_internal` (`world/update.rs`) follows `BWorld_update` (`0x1405C5690`):

1. `create_pending_physics_objects` makes the queued bodies.
2. `resolve_vehicle_terrain_penetrations` runs the terrain penetration queue.
3. The entity updates run in entity steps of at most 0.05 s. In each entity step:
   1. The squads move. `update_physics_vehicle_squads` then applies the physics vehicle squad
      rules, and `update_squad_turn_radius` moves the turn-radius point of each squad.
   2. `read_back_physics` reads the bodies back into their units.
   3. `update_death_physics` runs the clamshell, rocket and part filter rules. A finished rocket
      death removes its unit.
   4. `finish_unit_throws` ends the throws that landed.
   5. The unit updates, the ground tie and the other unit systems run.
4. `read_back_object_physics` reads the bodies of class-0 objects back.
5. The power manager runs (`update_active_powers`). Retail runs it after the units, objects and
   projectiles and before the physics step (DE `0x140314390`; 2008 `world.cpp:3331-3368`).
6. `step_physics` steps the physics world with the full update time.

`step_physics` (`physics/step.rs`) follows `BPhysicsWorld_update` (`0x1407BBB40`).
`World::substeps` splits the time into equal substeps of at most 0.0125 s. The count is
`n = (int)(t / 0.0125)`, plus one if a remainder is left. A time above 10 s or NaN gives one
substep of 0.0125 s. A 0.05 s update thus gives four substeps. Each substep does these steps:

1. `apply_physics_actions` applies the Havok actions of the active bodies in action creation
   order: the vehicle actions and the gravity ball actions.
2. `step_delta_time` steps the world.
3. The contact listener gives the new contact points to their units.

The readback runs in the next update, before the next step. A unit therefore shows the body
state of the last step.

### Terrain body

`SimPhysics::set_terrain` (`physics/terrain.rs`) follows `BTerrainPhysics_createHeightFieldBody`
(`0x14041B500`, 2008 `terrainphysicsheightfield.cpp:50-109`):

- The shape is a `HeightField` of the XSD sim heights (chunk `0x2222`), the decoded half floats.
- The shape adds a skirt of 10 vertices on each side that repeats the edge heights
  (`HeightField::SKIRT_VERTICES`).
- The body is fixed, at `HeightField::body_position`, so that the shape lines up with the world.
- The material is friction 1.9 and restitution 1.0.
- The filter is layer Terrain in its own system group.

The terrain shape and the sim height queries read the same heights.

### Bodies

`physics/bodies.rs` follows `BObject_initFromProto` (`0x14029C300`, 2008 `object.cpp:3656-3669`)
and `BEntity_createPhysicsObject` (`0x140204EF0`, 2008 `entity.cpp:2129-2301`):

- Every unit and class-0 object with a `PhysicsInfo` gets a body. Retail checks no object class.
  Only the create parameter `mNoPhysics` (bit `0x40` of parms byte `+0x66`) skips the body. In
  the sim, only the trigger `CreateObject` effect sets it.
- A physics replacement (`mPhysicsReplacement`, bit `0x02`) gets the `PhysicsReplacementInfo`
  body with `CompleteOnInactivePhysics` (`World::spawn_physics_replacement`, 2008
  `object.cpp:6735-6769`).
- The world queues a new body (`queue_physics_object`). The body is made at the start of the next
  update, when the entity has its final spawn transform.
- The body origin is the entity position plus the rotated `CenterOffset`.

Each body gets a motion type and a filter:

| Info | Motion type | Filter |
| --- | --- | --- |
| `MotionType fixed` | `Fixed`, no readback | The shared Buildings info |
| Any other info | `BoxInertia`, mass `max(mass, 1)`, zero velocities | A new Objects info |
| A unit with a vehicle type | `BoxInertia` | The shared Vehicles or `NoTerrainVehicles` info |

A clamshell info makes the pelvis body and the upper and lower bodies
(`BClamshellPhysicsObject_ctor`, `0x1407B7720`). Two limited hinges about the right axis join
them at the pelvis origin, with friction 3.0. The upper limits are [−π/3, π/8] and the lower
limits are [−π/8, π/4].

A unit with a vehicle type then gets its vehicle setup (`BEntity_setupVehiclePhysics`,
`0x1402055D0`, 2008 `entity.cpp:2399-2507`). A class-0 object gets none: the function returns for
any class other than Unit (`0x14020560C`).

1. The vehicle type selects the action (see [Vehicle actions](#vehicle-actions)).
2. The action constructor sets the inertia:
   - Warthog, chopper, ghost and aircraft: `diag(m, m, 0.4·m)` (`set_vehicle_inertia`).
   - The ground family: `diag(pitch·m, yaw·m, roll·m)` from the vehicle info
     (`setVehicleInfo`, `0x1402CEEF0`).
3. The body gets the shared Vehicles filter, or `NoTerrainVehicles` for the ground family.
4. Every vehicle except the aircraft gets the shared 22-vertex hull as its shape. The vertices
   are in DE at `0x1411F47A0` to `0x1411F4980` (2008 `physicsworld.cpp:150-200`).

`World::release_physics_object` removes a body with its clamshell parts and hinges. It also
cancels a queued body. `remove_object` and `remove_unit` release the body.

### Readback

`World::read_back_physics` (`physics/readback.rs`) runs the `BUnitActionPhysics` update
(`0x140547880`, 2008 `unitactionphysics.cpp:187-625`) of each unit with a dynamic body, in entity
id order. `read_back_object_physics` does the same for class-0 objects.

- An active body moves its entity. The position is `body position − R·CenterOffset`. The forward
  axis is the body Z axis, and the up axis is the body Y axis. The entity also takes the body
  velocity.
- An early terminate request of the vehicle action stops and deactivates the body.
- An inactive body clears the early terminate flags. With `EndMoveOnInactivePhysics`, the move
  of the unit ends. With `CompleteOnInactivePhysics`, the action goes to its wait state and the
  readback stops.

`mFlagPhysicsControl` units (`Unit::is_physics_driven`, a non-fixed body) skip the ground tie.
The world checksum hashes the state of each body in body id order: transform, velocities and
the active flag. It also hashes the body queue and the action state.

### Vehicle actions

`physics/vehicle_actions.rs` holds the four retail action classes. Each one is an
`hkpUnaryAction` that runs before each substep. A keyframed body skips its action.
`world_input::build` gives each action its unit, squad and air input for the substep.

| Vehicle types | Action | Code |
| --- | --- | --- |
| Scorpion, Grizzly, Elephant, Rhino, Reactor, Cobra, Gremlin, Wolverine, Ground | `BPhysicsGroundVehicleAction` (`0x1402CF050`) | `vehicle_actions/ground.rs` |
| Warthog, Chopper | `BPhysicsWarthogAction` (`0x1402D77B0`, `calcMovement` `0x1402D8190`) | `vehicle_actions/warthog.rs` |
| Ghost | `BPhysicsGhostAction` (`0x1402CCC00`, `calcMovement` `0x1402CD370`) | `vehicle_actions/ghost.rs` |
| Hawk, Hornet, Vulture, Banshee, Vampire, Sentinel | `BPhysicsHoverFlightAction` (`0x1402D0E20`, `calcMovement` `0x1402D1BD0`) | `vehicle_actions/hover.rs` |

- **Suspension** (`vehicle_actions/suspension.rs`): the ground, warthog and ghost actions cast
  a ray from each hardpoint to the terrain heightfield. Each wheel spring pushes the body up.
  `mFlagInAir` is set when no wheel touches the terrain.
- **Reorient** (`vehicle_actions/reorient.rs`): each action turns the body toward world up, as
  `hkpReorientAction::applyAction` does (ground `0x1402D05E0`, warthog `0x1402D8B50`, ghost
  `0x1402CDC60`, hover `0x1402D29C0`).
- **Ground family**: the action has no drive force. A teleport impulse moves the body toward the
  turn-radius point of its squad, and a yaw-match impulse turns it to the turn-radius forward
  axis. The family feels three times the world gravity. The filter turns off the Havok terrain
  contact, so the suspension holds the body above the terrain.
- **Warthog and chopper**: while a wheel touches the terrain, spring accelerations drive the body
  along its forward axis toward the squad position. Side friction holds it on its line, and a yaw
  spring turns it. The constants are hardcoded (2008 `physicswarthogaction.cpp:39-101`).
- **Ghost**: the ghost hovers on the warthog suspension with softer springs. It drives and
  strafes toward the squad position, also with no ground contact, and turns toward its attack
  target (`cFwdK = 2.5`, `cRightK = 2.5`, `cTurnK = 10`).
- **Hover**: the action cancels gravity and climbs toward the highest flight height under and
  ahead of the aircraft. It writes the squad height. In the plane, springs drive the aircraft
  toward the squad position and turn, pitch and roll it. A crashing aircraft dives through its
  crash position.

The kinematic hover controller (`world/air_avoidance/hover.rs`) and the crash movement
(`world/air_avoidance/crash.rs`) skip a unit with a hover action. Its
crash impact test uses the segment from its previous position to the read-back position. The
ground test is `y − 1.2·obstruction Y ≤ terrain` (`BUnitActionAvoidCollisionAir_update`,
`0x1404EEBB0`).

### Physics vehicle squads and the turn-radius point

A squad with one unit that has a vehicle action is a physics vehicle squad
(`BSquad::isSquadAPhysicsVehicle`). `World::update_physics_vehicle_squads`
(`physics/readback.rs`) applies its move rules (2008 `squadactionmove.cpp:967-981`, `:1027-1040`):

- The squad moves itself. Its unit gets no move target, only the moving state.
- At the move start, the body loses its deactivation and wakes.
- At the move end, the deactivation comes back, and the move of the unit ends when the body
  sleeps.

The squad move of a unit with a hover action keeps the squad height, because the hover action
owns it (2008 `squadactionmove.cpp:2693-2731`).

`entities/squads/turn_radius.rs` keeps the turn-radius point of a ground physics vehicle squad
(DE squad `+0x1D0` and `+0x1E0`, 2008 `squad.cpp:8490-8741`). The point follows the squad position
at a limited turn rate. It catches up at up to twice the speed (`cMaxSpeedupFactor`).
`World::update_squad_turn_radius` (`world/turn_radius.rs`) runs `BSquad_updateTurning`
(`0x1403A70E0`) once for each squad after its move. `BSquad_calcTurning` (`0x1403A73E0`) limits
the turn. Aircraft get no point.

### Contact listeners

`StepListener` (`physics/step.rs`) receives each new contact point of a step:

- `gVehicleCollisionListener` (2008 `physicsCollision.cpp:84-126`): when a vehicle body touches a
  body on the Objects layer, the vehicle side gets an inverse mass factor of 0. The vehicle keeps
  its motion.
- `BUnitActionPhysics::collision`: each unit records its first collision, the count of new
  points, body contacts and impact speeds. A new terrain point while the body moves down sets the
  landed flag.

Contacts are recorded only for units. Object contacts are not recorded.

### Terrain penetration queue

A ground action queues a terrain fix when a hardpoint goes below the terrain
(`BWorld_addPhysicsVehicleTerrainCollisionAsync`, `0x1405D2E20`).
`World::resolve_vehicle_terrain_penetrations` (`physics/step.rs`) runs at the start of the next
update (`BWorld_resolvePhysicsVehicleTerrainCollisions`, `0x1405D4090`). DE sorts the queue by
entity id (`0x1405D4030`); the sim visits the units in entity id order. The fix
(`mainThreadTerrainCollisionResolution`, `0x1402D0480`) does these steps:

1. Stop the vertical velocity.
2. Lift the body by the depth plus the wheel radius.
3. Push the body up by the old downward speed times the restitution of the body material.

### Gravity ball action

The Wave power gives a captured body a `BPhysicsGravityBallPullAction`
(`BPowerWave_addObjectToBall`, `0x140331D40`; `BPowerWave_grabFakeObject`, `0x1403324B0`).
`physics/gravity_ball.rs` runs it before each substep with the vehicle actions, in action
creation order (`applyAction`, `0x1402CE1A0`):

1. Apply the anti-gravity force `−m·g` to the body and to each extra body. The extra bodies are
   the upper and lower bodies of a clamshell, and they use the mass of the main body.
2. Apply the spring force along the ball direction: `F = (len − rest)·k + relVel·damping`. The
   spring pulls only when the body is farther than the rest length.
3. If the lateral speed is below the minimum, apply a point impulse of the lateral velocity
   times the mass at the center of mass.

A NaN or infinite ball position, ball velocity or lateral velocity counts as zero. The Wave fixes
the rest length when it makes the action.

### Throws, deaths and replacements

Other slices still change these mechanics. This list gives the current code only.

- `World::begin_unit_throw` (`entities/units/thrown.rs`) gives a body its throw velocity. The Wave
  calls it. `finish_unit_throws` ends a throw when the body lands. A body that exists only for the
  throw goes, and another body stops.
- `World::update_death_physics` (`physics/death.rs`) runs the death part of
  `BUnitActionPhysics_update`: the clamshell readback (`getClamshellData`, `0x1407B8520`), the
  rocket on death and the filter switch of a thrown part.
- `World::throw_killed_unit` (`world/death_throw.rs`) makes and throws the physics replacement of
  a unit that a weapon kills (`BUnit_createAndThrowPhysicsReplacement`, `0x1404DEA30`). No weapon
  damage gives it throw data yet, so no weapon kill throws a unit.
- Detonate, jump and Wave replacements use `World::spawn_physics_replacement`.

### Installed tests

These opt-in tests load Blood Gulch from an installed game:

- `scenario-physics-world.rs`: the Warthog body (hull, inertia `(m, m, 0.4m)`, Vehicles filter),
  the fixed building bodies, the terrain heights, a body that rests at the convex radius gap, and
  an exact readback.
- `scenario-ground-follow.rs`: the Warthog drives over the hill on its wheels and stops within
  1.5 of the goal.
- `scenario-object-physics.rs`: the bodies of the class-0 macblast rocks.

```powershell
$env:OPENENSEMBLE_GAME_DIR = 'C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE'
cargo test -p sim --test scenario-physics-world --test scenario-ground-follow --test scenario-object-physics -- --ignored
```

### Physics deaths

| Address | IDA name | Recovered role |
| --- | --- | --- |
| `0x1404DEA30` | `BUnit_createAndThrowPhysicsReplacement` | Replaces a killed unit with its `PhysicsReplacementInfo` body and throws it. |
| `0x1407B8300` | `BClamshellPhysicsObject_applyImpulse` | Applies the impulse to the pelvis, then to the upper and lower bodies. |
| `0x1411E4120` | `BClamshellPhysicsObject` vtable | Slots 10 to 17 and 25 act on all three clamshell bodies. |
| `0x1402A6190` | `BObject_explodeMethane` | Plays the `MethaneExplosion` impact effect (size large) at the end of a rocket death. |
| `0x1404EC0C0` | `BUnit_doDeathSpawnSquad` | Spawns the death spawn squad of a unit. |

A weapon damage carries the throw values of its weapon (`ThrowUnits`, `PhysicsForce*`,
`PhysicsLaunch*`, `FlailThrownUnits`) from the attack profile through the fire event and
the projectile to `World::apply_attack_damage` (`world/combat/area_damage.rs`). The force
origin is the launch position of the projectile, or the attacker position for a hit without a
projectile. An area damage uses its ground zero. The distance factor is 1 for a direct hit,
`PrimaryTargetFactor` for the area primary target, the capped damage divided by the attack
damage for a capped area target, 1 for a linear area target, and the area factor (with a zero
direction) for a Gaia target (`BDamageHelper::doAreaEffectDamage`).

A kill with these values throws the physics replacement (`world/death_throw.rs`). The dead
replacement is a corpse: the dead-unit pass (death replacements, passive detonations, death
spawns, removal) skips it. It goes when its physics action waits, that is when its body
sleeps (`CompleteOnInactivePhysics`). The sim does not model the fade of the corpse manager.
A rocket death ends at its timer with the methane explosion, the death spawn squad and the
removal of the unit.

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
the squad and each member without `mFlagPhysicsControl`. At scenario load, only squads take
the terrain height (`BSquad::settle`). A class-zero object keeps its authored height, because
`BEntity_update` ties an entity only while it moves. The scenario flag `IncludeInSimRep` only
sets the `appearsBelowDecals` create parameter (`0x14037DE9F`), a render decal order, so it
has no simulation effect.

In the shipped `objects.xml`, all ground vehicles have `OrientUnitWithGround` (Warthog, Ghost,
Scorpion, Wraith, Cobra, Grizzly, Wolverine, Elephant, Chopper, Gremlin, Locust). Infantry and
buildings do not have it, so they stay upright. The Flood infection forms also have
`DoNotFilterOrient`.

A Havok vehicle does not use the tie. `BTerrainSimRep_initPhysics` (`0x140063F20`) calls
`BTerrainPhysics_createHeightFieldBody` (`0x14041B500`). It makes a fixed body with a
`hkpSampledHeightFieldShape` (`0x140874D40`) over the simulation height grid, with a clamped
border of 10 samples. `BTerrainHeightFieldShape_getHeightAt` (`0x14041B3B0`) reads the same
half-float heights as the simulation queries. The Warthog body has the shared vehicle hull, and
its action casts the suspension rays to that shape. `BUnitActionPhysics_update`
(`0x140547C73`–`0x140547DC4`) reads the body position and rotation quaternion back. It writes
the entity position and its right, up and forward rows. The Warthog `.physics` file selects the
code vehicle `warthog` and has no suspension data. The action uses hardcoded suspension values.

### Simulation model

- `BaseEntity::up` is the up axis. `BaseEntity::forward` stays the heading. The renderer
  projects the heading onto the plane of the up axis.
- A unit with a dynamic body does not use the tie. Its body collides with the terrain body of the
  physics world, or its vehicle suspension holds it above the terrain. The readback gives the
  unit its position and its forward and up axes (see [Readback](#readback)).
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
| Physics world: shapes, filter, contacts, solver, hinges, deactivation | `crates/physics` |
| Physics of a sim `World`: filters, hull, body queue, checksum | `crates/sim/src/physics/state.rs` |
| Terrain body | `crates/sim/src/physics/terrain.rs` |
| Body creation and release | `crates/sim/src/physics/bodies.rs` |
| Body templates of the physics infos | `crates/sim/src/physics/template.rs`, `gameplay/physics_data.rs` |
| Substeps, contact listeners, terrain penetration queue | `crates/sim/src/physics/step.rs` |
| Readback and physics vehicle squads | `crates/sim/src/physics/readback.rs` |
| Vehicle actions | `crates/sim/src/physics/vehicle_actions.rs` and `vehicle_actions/` |
| Gravity ball action and the action order | `crates/sim/src/physics/gravity_ball.rs` |
| Clamshell, rocket and part filter on death | `crates/sim/src/physics/death.rs` |
| Body access of the gameplay code (impulses, velocities, damping) | `crates/sim/src/physics/access.rs` |
| Turn-radius point of a squad | `entities/squads/turn_radius.rs`, `world/turn_radius.rs` |
| Ground tie of units without a dynamic body | `crates/sim/src/world/terrain/ground_tie.rs` |
| Ground-tie flags of a unit | `entities/units/ground_tie.rs` |
| Warthog unit and squad profiles | `entities/units/warthog.rs`, `entities/squads/warthog.rs` |
| Marine velocity, acceleration, turn rate, and obstruction size | `entities/units/marine.rs` |
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

The squad move has no turn rate. As `BSquadActionMove_doWorkUpdateInternal_4` (`0x1403C4220`)
does, each step goes straight toward the target and the squad faces the step. Only the
turn-radius point of a ground physics vehicle squad turns at a limited rate. A flying join
follower therefore flies straight to its target and does not circle it.

The complete inspected function set, including Warthog physics and air-move
comparison functions, is renamed and repeatably commented in the saved IDA
database beside `xgameFinal.exe`.

## Known gaps

- The sim has no pather. `Squad::update_movement_with_speed_limit` in
  `entities/squads.rs` steers each squad in a straight line to its current
  waypoint. Retail requests and refreshes multi-level paths in
  `BUnitActionMove_requestPath` and `BUnitActionMove_refreshPath`.
- The sim has no obstruction manager. Dynamic repathing, wait ownership, and unit-to-unit
  avoidance are not implemented.
- Warthog jump lines are not implemented (`BUnitActionPhysics_tryAdjustTrajectoryForTerrain`,
  `0x1405485E0`). The `ensemble-assets` `DesignLine` has no `Data` values, and the sim keeps the
  design lines by id, not in document order.
- The visual parts of the ground family (treads, wheels) are not modeled. The derived ground
  actions differ only in these parts.
- The vehicle action input has approximations (`vehicle_actions/world_input.rs`):
  - The sim has no fatality, stasis, move-action Wait state or flight-height mesh. The input uses
    false, 1.0, never and terrain + 16.
  - The move-action target is the squad `move_target`. The future positions follow a straight
    line to it.
  - `mDesiredPos` starts at the body position, not at the unit position.
  - The ground terrain check uses `HeightField::surface_at`, not `floorf` cells.
- The physics crate has approximations (`crates/physics`):
  - The islands are made again in each step. Retail splits islands lazily, so the timing of
    merges and splits can differ.
  - The manifolds are solved in id order, then the hinges. Retail uses the constraint list of
    each entity.
  - An active keyframed body does not wake a sleeping dynamic body through a contact.
  - The wake rule (reset the inactive counts, keep the velocities) is not confirmed in IDA.
- Contacts of class-0 objects are not recorded.
- Retail sets the acceleration to 2 × `Velocity` when a proto has no `Acceleration`
  (`BProtoObject_load`, `0x14034241B`). The sim uses 0
  ([#37](https://github.com/coconutbird/openensemble/issues/37)).
- The Warthog move action's terrain impulses
  (`BUnitActionMoveWarthog_collectTerrainImpulses`) are not modeled.
- Formation reassignment, formation priorities, and movement-animation
  controller transitions are not implemented.
- Renderer movement is a read-only projection of each unit's sim-owned action
  and transform.

See `simulation-coverage.md` for the full list of engine systems.
