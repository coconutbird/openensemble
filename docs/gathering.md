# Resource gathering

Resource gathering is authoritative simulation state. Scenario loading mounts
the selected scenario ERA before parsing the database, so scenario-local game
data, objects, and tactics define the resource table, gatherable nodes, and
unit actions used by the sim. Renderer and UI code project the resulting world
state; they do not calculate income or depletion independently.

## Recovered retail contract

The implementation was cross-checked against the read-only 2008
`BUnitActionGather` source and installed Halo Wars DE data:

- A squad order connects one per-unit `Gather` action to an explicit target.
- A target is valid while it is owned by Gaia or the gathering player and
  exposes the resource requested by the selected action.
- Working range is the horizontal distance between obstruction surfaces, not
  the distance between entity origins.
- Each working unit contributes its action work rate multiplied by elapsed
  simulation time. Player technology and live unit work-rate modifiers apply
  before resource transfer.
- Finite targets clamp the final transfer to their remaining amount. Unlimited
  targets grant income without changing their stored amount.
- `GathererLimit` caps concurrent per-unit work controllers. Stable entity
  order resolves contention deterministically.
- A depleted target completes connected orders, and
  `DieAtZeroResources` routes the target through the normal unit lifecycle.
- `TeamShare` divides each transfer among playing members of the owner's team.

## Implemented authoritative behavior

- `GameplayCatalog` resolves all `Gather` actions from the scenario-layered
  tactics and maps their resource names to runtime `GameData/Resources` slots.
- Units retain finite/unlimited resource payloads loaded from
  `ResourceAmount`, `Resource_*`/`Collectable` object types,
  `UnlimitedResources`, `DieAtZeroResources`, and `GathererLimit`.
- `World::issue_gather_order` validates ownership and squad control state,
  selects enabled actions per member, and cancels conflicting movement,
  combat, containment, and incoming transport work through the shared sim
  order path.
- Gathering moves through checksummed `Moving`, `Working`, `Done`, and `Failed`
  phases on both the squad and participating units. Movement approaches the
  target; only members actually within their action range earn resources.
- Income updates both the player's spendable balance and lifetime resource
  total through `Player::add_resource`.
- Target amount, selected action, phases, and resource configuration all
  participate in deterministic world checksums.

## Validation

Unit tests cover authored catalog fields, shipped shorthand fallbacks, finite
and unlimited nodes, clamped final transfers, terminal lifecycle behavior,
movement, gatherer limits, team sharing, and resource-state checksums. The
opt-in installed-data regression loads Blood Gulch and its layered database,
resolves the shipped Marine supply/collectable actions, spawns the shipped
30-supply crate, and proves that the authoritative order increases player
supplies while depleting the resource node. The synthetic scenario-layering
suite separately proves that scenario-local resource/rate tables and object
definitions reach the loaded simulation.

## Deliberate parity boundaries

- Installed HWDE Marine tactics omit explicit `Resource`, `WorkRate`, and
  `WorkRange` fields. The catalog currently maps the three shipped shorthand
  action names to their resource names and uses a one-unit-per-second rate plus
  the source-backed 0.1 default range. The executable's exact implicit work
  rate still needs recovery.
- Retail collectable reporting, localized UI notifications, gather sounds,
  animation-controller ownership, AI target selection, and path-obstruction
  manager behavior are not yet implemented.
- Campaign co-op has additional sharing/reporting branches beyond authored
  `TeamShare`; those need a dedicated source-backed pass.
