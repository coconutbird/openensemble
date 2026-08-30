# Persistent healing

The simulation owns retail's persistent unit `Heal` action. The renderer reads the resulting
`World` state (`Unit::heal_phase` and `Unit::heal_target`) and does not reproduce healing rules.

## Layered data

`GameplayCatalog` builds `HealActionProfile` values only for `Heal` actions named by an object's
`PersistentAction` tactic list. Scenario ERAs are mounted before the database and tactic catalog
are parsed, so scenario-local database rows and tactic files take precedence. The ignored
`scenario-heal` integration test loads Blood Gulch from an installed game, pins the Medic and
Monitor profiles, spawns a Medic from that same layered database, and executes its heal action.

## Runtime contract

- A connected action begins in `Waiting`. Once its target is eligible, it enters `Working` without
  restoring hit points until the next fixed action update.
- Ordinary Medic healing targets the unit's parent squad. `HealTarget` follows the parent squad's
  Join target and falls back to the parent when no live target exists.
- The target must have remained idle, undamaged, and not attacking for `MinIdleDuration`.
- `WorkRate` is hit points per second and receives player action-work-rate technology effects. The
  general unit work-rate scalar is deliberately not applied.
- `AllowReinforce` controls whether missing members from the authored proto squad are recreated.
- Disabling a working action freezes it in place, matching the retail early return; enabling it
  resumes the opportunity.

The implementation follows the named 2008 source in `unitactionheal.cpp`, `squadactionattack.cpp`,
and `squad.cpp`. The animation opportunity is represented by the authoritative `Working` phase;
generic retail opportunity arbitration and exact animation selection remain future infrastructure.
