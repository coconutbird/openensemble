# Building research reconstruction

Research is authoritative simulation state. Building commands, player
resources and technology status, per-building production queues, progress, and
technology activation all live in `sim`; renderer and other UI consumers only
read status and progress snapshots from the world.

## Retail evidence and renamed IDA functions

The wire layout and state-machine behavior were recovered from the shipping
source contracts and cross-checked against `xgameFinal.exe`. Every function
substantively inspected for this slice was renamed and commented in the saved
`xgameFinal.exe.i64` database.

| Address | IDA name | Recovered role |
| --- | --- | --- |
| `0x1401987D0` | `BBuildingCommand_processUnit` | Dispatches building work. Research uses a runtime technology-table ID; positive count starts and negative count cancels. |
| `0x140417190` | `BTechTree_save` | Persists normal and unique technology points, research-building IDs, statuses, uniqueness, and marker `10000`. |
| `0x1404176D0` | `BTechTree_load` | Remaps saved technology nodes and restores the saved research state and marker. |
| `0x140448700` | `BEventDefinitions_getGeneralEventEnum` | Maps `CommandResearch` and `CommandResearchCancel` to general-event IDs 70 and 71. |

The fixed `BBuildingCommand` payload after its base command is 28 bytes:
command subtype, target ID, three target-position floats, count, and socket
entity ID. A research target is its zero-based position in the live technology
table, not the authored DBID, because retail data can contain duplicate DBIDs.

## Implemented authoritative behavior

- Scenario ERAs are mounted before database parsing, so scenario-local object,
  technology, game-data, tactic, visual, and UAX records are part of the same
  layered database used to construct and tick the simulation.
- The lockstep packet dispatcher decodes `COMMAND_BUILDING`, queues the exact
  payload, and executes `Research` commands against recipient buildings.
  Positive counts enqueue exactly one item, as retail does; negative counts
  cancel the player-global assignment.
- A building must exist, be owned by the issuing player, and expose the target
  technology in its authored command list. Shipping object commands without an
  explicit `Type` are inferred as research when their target resolves in the
  technology table.
- Technology status is derived as `Unobtainable`, `Obtainable`, `Available`,
  `Researching`, or `Active`. Authored unobtainable/forbid state, active-tech
  prerequisites, `TypeCount` equality/`gt`/`lt`, and `OrPrereqs` participate in
  eligibility.
- Costs resolve through the database resource table and are paid in full when
  work is accepted. Insufficient funds and malformed or unknown resource data
  reject the command without partial mutation.
- Each building has one current item and a FIFO queue. Promotion consumes an
  update without adding work; subsequent deterministic updates add one research
  point per simulated second and reread the authored total while work is live.
- Cancellation fully refunds queued or partially completed work. Destroying a
  research building clears and refunds all of its outstanding items.
- Completion activates the existing simulation technology-effect pipeline.
  `Instant` technologies activate immediately after validation and payment.
- Player research assignments, points, building queues, costs, and active
  technology state participate in deterministic world checksums.
- `World::technology_status` and `World::research_progress` expose UI-ready
  state, including the authoritative building, current/total points, queued
  state, and completion fraction. Rendering does not advance or reinterpret
  research.

## Validation

The synthetic `building-research` integration suite covers the fixed wire
payload, packet dispatch, payment, promotion and progress timing, activation,
technology effects, dependent prerequisites, duplicate rejection,
cancellation, destruction refunds, `Instant`, `TypeCount`, and `OrPrereqs`.

The opt-in installed-data test mounts Blood Gulch and its scenario database,
spawns the shipped `unsc_bldg_barracks_01`, submits a building command for
`unsc_marine_upgrade1`, verifies its real 200 Supplies/1 Power cost and 40
research points, and observes the technology become active.

```powershell
$env:OPENENSEMBLE_GAME_DIR='C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE'
cargo test -p sim --test scenario-asset-loading -- --ignored
```

## Deliberate next boundaries

This slice implements normal player-global research, not every production
path. Remaining retail work includes per-unit `UniqueProtoUnitInstance`
technology state, automatic `Shadow` activation, cooperative per-player
research slots, quick-build and AI work-rate modifiers, repeated `Perpetual`
effects, research sound/events, research savegame compatibility, and the
non-research `BBuildingCommand` subtypes such as training and construction.
Unique and manual Shadow research are explicitly rejected until their correct
ownership semantics exist.
