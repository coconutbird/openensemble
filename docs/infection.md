# Persistent infection

The simulation owns retail's persistent unit `Infect` action and victim conversion lifecycle. The
renderer reads the resulting `World`: exposure attachments are ordinary authoritative objects, and
`Unit::infection_phase`, ownership, prototype, health, and squad membership describe conversion.
No infection or transformation rule is duplicated in presentation code.

## Layered data

`GameplayCatalog` materializes `InfectActionProfile` only for `Infect` actions named by a unit's
`PersistentAction` tactic list. Work rate, radius, delay, invalid targets, enablement, and the
infection attachment all come from the active tactics. Victim forms come from `GameData`'s
`InfectionMap`, while `NumConversions` comes from the live infector prototype.

Scenario ERAs are mounted before database tables and tactics are parsed. The ignored
`scenario-infect` integration test loads Blood Gulch from an installed game, pins the shipped spore
cloud profile and Marine map entry, spawns both squads from that same layered database, and executes
the conversion.

## Runtime contract

- Each enabled infector scans every 0.5 seconds and retains newly found enemy, non-Gaia squads.
- A squad qualifies when at least one current child has an infection-map entry, is not already
  converting, and is not rejected by the action's invalid-target list.
- The visual proto-object is attached to every child in an exposed squad. Leaving the scan radius
  does not cancel an existing exposure.
- After `MinIdleDuration`, `WorkRate` banks combat value independently for each exposed squad.
  Multiple children can be marked in authored member order when enough work is available.
- Disabling the action freezes its scan timer, exposures, work banks, and attachments. Disconnecting
  or removing the infector destroys all retained visuals.
- A marked victim reaches zero hit points first. On the following fixed substep, cover produces an
  ordinary death; otherwise the same unit entity takes the mapped infected prototype and joins a
  newly created mapped single-unit squad under Gaia.
- On the next fixed substep that squad transfers to the infecting player. This staged state lets the
  renderer present the authoritative death/birth transition without owning gameplay decisions.
- A positive `NumConversions` kills the infector's parent squad when its conversion count reaches
  the authored limit.

The implementation follows the named 2008 source in `unitactioninfect.cpp`,
`unitactioninfectdeath.cpp`, and `unit.cpp`. Fatality opportunity flags and exact animation-event
durations are not yet generalized; infection still preserves their source-backed ordering and the
retail victim unit ID.
