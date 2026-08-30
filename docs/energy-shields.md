# Persistent energy-shield presentation

The simulation owns both integral shield gameplay and the persistent tactic actions that present
those shields. The renderer consumes the resulting `World`: external shield shells are ordinary
class-zero attachment objects, while infantry component visibility comes from
`UnitVisualMeshMask`. Presentation does not decide when a shield raises, reacts to damage, or
disappears.

## Layered data

`GameplayCatalog` creates `EnergyShieldActionProfile` values only for `EnergyShield` and
`InfantryEnergyShield` actions named by a unit's `PersistentAction` list. External profiles retain
the action name, start-disabled state, referenced proto-object database ID, and authored bone.
Infantry profiles retain the action duration and retail's named `Shield` visual component.

Scenario ERAs are mounted before database tables and tactics are parsed. Shield attachment
`Idle`, `Incoming`, and `Death` clips are then resolved from the same layered visual and animation
assets. The ignored `scenario-energy-shields` integration test loads Blood Gulch from an installed
game, pins Prophet, Ghost, Locust, Wraith, and Elite Commando profiles, and executes both shield
action types using that database stack.

## Runtime contract

- Integral shield points, directional coverage, damage absorption, recharge delay, and recharge
  rate remain authoritative in `UnitShields` and the world shield system.
- Every persistent action has separate live state. This preserves the Prophet's second,
  start-disabled shield action and lets player technology or a unit action override enable it.
- An enabled external action raises when its live unit has positive shield points. The sim creates
  the referenced class-zero attachment and selects its `Idle` animation.
- A damage event with shield points remaining selects `Incoming`. Depleting the shield selects
  `Death`; the attachment is removed only after the scenario-loaded clip duration completes.
- Disabling or disconnecting an external action removes its owned attachment. Unit removal uses the
  normal authoritative attachment cleanup path.
- The infantry action hides the recursive visual component named `Shield`, starts its authored hit
  timer when a damage event leaves positive shield points, and returns to its ready phase when that
  timer expires.
- Action phase, attachment ID, remaining transition time, component visibility, and animation state
  participate in synchronized world state or checksum data exposed to presentation.

The implementation follows the named 2008 source in `unitactionenergyshield.cpp`,
`unitactioninfantryenergyshield.cpp`, `unit.cpp`, and `database.cpp`. The infantry source writes
`false` to the component's visible flag both when hit and when its timer expires; the simulation
preserves that behavior instead of inferring a visible hit flash.

## Known placement gap

The attachment profile retains the authored bone name, but the current general attachment system
follows the parent root transform because it does not yet expose bone-local simulation transforms.
The renderer still receives the authoritative attachment entity and animation. Adding a shared
bone-anchor transform is a future attachment-system improvement, not renderer-owned shield logic.
