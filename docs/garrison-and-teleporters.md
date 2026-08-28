# Garrison and teleporters

Containment is authoritative simulation state. Unit and squad implementations
live in their existing focused folders, commands mutate `World`, and the
renderer only projects visibility and transforms from the resulting state.

## State ownership

- Every `Unit` stores its authored container capability, inverse container ID,
  sorted contained-unit IDs, and object-type names.
- Every `Squad` stores its garrison/ungarrison phase and sorted contained-squad
  IDs. The phases are `Free`, `Garrisoning`, `Garrisoned`, and `Ungarrisoning`.
- Both layers participate in deterministic world checksums. Unit references are
  the physical source of truth; squad references retain retail's logical squad
  queries and one-squad containment behavior.
- Scenario and runtime spawns project `Contain`, `MaxContained`, `Teleporter`,
  `OneSquadContainment`, `TeleportPickup`, and `HotDropPickup` data from the
  active layered database into the unit state.

## Fixed-tick lifecycle

1. A `Garrison` work command resolves a target unit or parent squad, checks
   command ownership and authored capability, and starts squad movement.
2. Surface range uses the command override or the target's teleporter HotDrop
   `WorkRange`. A full teleporter waits; an ordinary invalid/full container
   rejects the order.
3. On entry, surviving passenger units gain inverse container references and
   leave ordinary movement, collision, combat targeting, and presentation.
4. A linked teleporter processes the first contained squad. It consumes the
   same two deterministic random angles as retail: the first belongs to the
   recovered unused exit-start calculation and the second selects the rally
   offset using HotDrop `WorkRange`.
5. The squad remains physically contained during the cached `Ungarrisoning`
   phase. On the following tick, both sides of every containment reference are
   removed, members appear at the exit transform, and the rally move begins.

Removing a passenger, container unit, or container squad repairs both reference
directions. Destroying a container emergency-unloads surviving passengers so
they cannot remain hidden or non-collidable behind a stale entity ID.

## Presentation contract

`simulation_unit_transform` returns no visible transform while a unit is
contained. Scene construction still decodes and retains that unit's visual, so
the normal per-frame world synchronization can reveal it after ungarrisoning
without rebuilding a second gameplay roster. No teleporter movement, placement,
or timing decision is made in the renderer.

## Verified coverage

- Synthetic tests cover command wiring, population and object-type capacity,
  deterministic RNG/checksums, emergency unload, trigger queries, and renderer
  hide/reveal behavior.
- The installed-game Blood Gulch test loads its scenario ERA and database,
  executes the authored 87→5 trigger link, spawns a database-backed Marine
  squad, and verifies complete source-pad-to-destination traversal.

## Remaining parity work

- Replace the deterministic first retail-style exit candidate with the complete
  terrain/obstruction-aware `findInstantiatePositions` search and growth loop.
- Reproduce per-unit entry/exit animations, containment reservation timing,
  notification events, and the retail ungarrison cooldown.
- Implement ordinary transports, partial per-unit containment, attachments,
  reverse hot-drop flows, and non-teleporter HotDrop presentation objects.
- Route teleporter beam, sound, alert, and selection changes to read-only
  presentation adapters.
