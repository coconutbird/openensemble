# Targeted squad repair (`RepairOther`)

`sim` owns targeted repair as authoritative game state. Commands, contextual trigger work, and retail idle opportunities all connect the same squad order; renderers only consume the resulting `World`, including repair effect objects.

## Scenario-layered inputs

`GameplayCatalog` collects every tactic action whose type is `RepairOther` after the scenario ERA has been mounted. A `RepairOtherActionProfile` retains:

- action name, `WorkRate`, and `WorkRange`;
- `AllowReinforce` and `StartDisabled`;
- `AutoRepairIdleTime`, `AutoRepairThreshold`, and `AutoRepairSearchDistance`;
- the optional effect `ProtoObject` and source bone name.

Retail defaults are `WorkRate = 0`, `WorkRange = 0.1`, auto-repair idle time/search distance `0`, and threshold `1`. Player technology and live action enablement are evaluated during selection and execution. Technology modifies the squad action's work rate; the per-unit work-rate scalar is intentionally not applied.

The ignored `scenario-repair-other` integration test loads installed Blood Gulch and pins scenario-layered values that are absent from the root Engineer tactic: rate `0.15`, range `3`, auto-repair `1000 ms / 0.99 / 45`, and `fx_covhealbeam`. It then spawns an Engineer and damaged Wraith from that database, heals the Wraith through the real order, and verifies effect creation and cleanup in `World`.

## Execution contract

- An entity target resolves to a live squad directly or through a unit's parent squad.
- A source cannot repair itself or a full-health target. Authored target rules and current action enablement must select a `RepairOther` action.
- The source approaches until obstruction-surface XZ distance is strictly less than `WorkRange`, stops, then restores `WorkRate * elapsed` combat value.
- Existing combat-value repair restores live members first and reproduces retail's bug where `AllowReinforce` is ignored and eligible multi-member squads are reinforced anyway.
- Positive excess combat value completes the action. A missing/dead target or a target that changes to another team also ends it.
- Move, attack, gather, capture, garrison, join, hitch, mines, and detonate orders cancel targeted repair and remove its effects.
- Auto-repair uses the tactic action literally named `RepairOther`, requires the authored idle duration, scans visible same-team squads under the health threshold and within search distance, confirms target rules, and deterministically chooses the nearest candidate.

## Renderer-facing effects

An explicit action effect creates a class-zero beam owned by the target player, with its secondary endpoint in authoritative world state. Authored `BeamHead` and `BeamTail` prototype objects are created alongside it. If the action omits a prototype, `fx_repairing` is attached to the target leader. All objects are checksummed and removed when the order ends or is canceled.

The sim currently uses source and target simulation centers. Crashing aircraft leaders are rejected
as repair sources. Exact skeletal bone placement, target visual-bounds shortening, healing
animation opportunities, and repair-driven attacker reveal bookkeeping await their underlying
general-purpose systems; they do not require renderer-side game logic.

## Source basis

Behavior was recovered from the named 2008 source files `squadactionrepairother.cpp`, `opportunity.cpp`, `squad.cpp`, and the tactic/action definitions. No additional IDA functions were inspected for this slice.
