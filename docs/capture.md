# Capture

Capture is authoritative simulation state. The selected scenario ERA is
mounted before database parsing, so its `GameData`, object prototypes, and
tactics determine capture behavior in the same `sim::World` consumed by the
renderer. Rendering can project owner, health, phase, active state, and
progress from the live world; it does not calculate capture work or ownership.

## Recovered retail contract

The implementation was recovered from the named 2008
`unitactioncapture.cpp`, `squadactioncapture.cpp`, `unit.cpp`, `squad.cpp`,
`tactic.cpp`, and database sources. No additional IDA functions were inspected
for this slice, so there are no new inspected functions requiring names.

- Unit actions progress through moving, working, done, and failed states.
- Range uses horizontal obstruction-surface distance. An explicit order range
  wins; otherwise the tactic action range is used.
- Each in-range unit contributes `WorkRate * elapsed`, including player
  technology and live unit work-rate modifiers.
- A target's `BuildPoints` is its capture threshold. Work against another
  player's positive progress removes that progress before a new player starts
  accumulating points.
- Only one source squad may actively work a target. The target's active unit
  references are the authoritative `BeingCaptured` state.
- A squad pays applicable `CaptureCost` entries once per player. Additional
  same-player squads share the link; the cost is refunded only when the final
  link disconnects before completion.
- With linked orders but no active workers, progress decays by the scenario's
  `GameData/CaptureDecayRate`.
- Completion transfers ownership, uses the actor's co-op partner when retail's
  co-op rule applies, restores all target-squad members to full health, and
  puts that squad in Lockdown mode. `DieOnBuilt` kills completing workers.
- Retail permits capture when the prototype is `Capturable` and the target is
  Gaia-owned, or when its live target state is also invulnerable.

The source-backed omitted-field defaults are zero capture points per second
and a `0.1` work range. Scenario layers normally supply playable values. Blood
Gulch, for example, overrides the root Marine Capture action to a `0.167` work
rate and `4.0` range.

## Implemented authoritative behavior

- `GameplayCatalog` resolves enabled Capture actions from the fully layered
  tactic set, including authored target rules and scenario-local overrides.
- Scenario/runtime object construction projects `Capturable`, `Invulnerable`,
  `BuildPoints`, and civilization-specific `CaptureCost` data into world state.
- `World::issue_capture_order` validates the player, squad, target, tactic,
  ownership rule, and payment before connecting squad and per-unit actions.
- Fixed-step updates handle approach movement, one retail second-chance
  approach after losing working range, deterministic unit work, exclusivity,
  contested progress, decay, completion, cancellation, and lifecycle cleanup.
- Capture cancels conflicting simulation orders through the same shared order
  paths used by move, attack, gather, garrison, join, mines, detonate, and
  hitching.
- `WorkCommand::capture_squads` and contextual scenario `Work` effects both
  converge on `World::issue_capture_order`. Scenario ticks provide that path
  with the same layered database and gameplay catalog used during loading.
- Squad/unit phases, targets, progress, payment links, and active workers all
  participate in deterministic world checksums.
- Public unit and squad queries expose capture target, phase, action name,
  points, maximum points, percentage, owner of progress, and active status for
  renderer/UI projection.

## Validation

Focused tests cover action parsing and target rules, ownership transfer and
health restoration, shared payment/refund, opposing progress, idle decay,
single-squad exclusivity, player command dispatch, and contextual trigger
routing. Strict Clippy runs across all `sim` targets.

The opt-in `scenario-capture` integration test loads installed Blood Gulch,
asserts the scenario-layered Marine work rate/range and capture decay rate,
spawns the shipped Marine squad and `for_bldg_factory_01`, then observes the
factory transfer to the Marine player through authoritative fixed-step updates.

```powershell
$env:OPENENSEMBLE_GAME_DIR='C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE'
cargo test -p sim --test scenario-capture -- --ignored
```

## Deliberate parity boundaries

- Retail multiplies capture work by 30 while the session QuickBuild flag is
  active. The simulation does not yet model that session flag, so it cannot
  apply this branch without inventing duplicate state.
- Capture animation-controller ownership, sounds, localized notifications, AI
  target selection, and retail path-obstruction-manager details remain
  presentation or future systems. They do not have parallel renderer-side game
  logic.
