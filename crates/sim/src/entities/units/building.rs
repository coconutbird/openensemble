//! Deterministic production state owned by class-1 building units.

mod air_traffic_control;
mod trained_birth;

use crate::entity_id::EntityId;
use crate::player::{PlayerId, PopulationCost, Resources};
use crate::sync::SyncChecksum;

pub use air_traffic_control::{
    AIR_TRAFFIC_LANDING_SPOT_COUNT, AirTrafficControl, AirTrafficLandingSpot,
};
pub use trained_birth::TrainedSquadBirth;

/// Trigger variable notified when production work finishes or is canceled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TriggerCommandStateRef {
    pub(crate) script_id: u32,
    pub(crate) variable_id: u32,
}

impl TriggerCommandStateRef {
    pub(crate) const fn new(script_id: u32, variable_id: u32) -> Self {
        Self {
            script_id,
            variable_id,
        }
    }

    fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.script_id);
        checksum.hash_u32(self.variable_id);
    }
}

/// One paid technology item in a building's shared production queue.
#[derive(Debug, Clone)]
pub struct ResearchTask {
    pub(crate) player_id: PlayerId,
    pub(crate) technology_id: i32,
    pub(crate) technology_name: String,
    pub(crate) current_points: f32,
    pub(crate) total_points: f32,
    pub(crate) cost: Resources,
    pub(crate) trigger_state: Option<TriggerCommandStateRef>,
}

impl ResearchTask {
    /// Player who owns the research.
    #[must_use]
    pub fn player_id(&self) -> PlayerId {
        self.player_id
    }

    /// Runtime technology ID that the task researches.
    #[must_use]
    pub fn technology_id(&self) -> i32 {
        self.technology_id
    }

    /// Name of the technology that the task researches.
    #[must_use]
    pub fn technology_name(&self) -> &str {
        &self.technology_name
    }

    /// Research points collected so far.
    #[must_use]
    pub fn current_points(&self) -> f32 {
        self.current_points
    }

    /// Research points needed to complete the task.
    #[must_use]
    pub fn total_points(&self) -> f32 {
        self.total_points
    }

    /// Progress in the range 0 to 1. Returns 0 if `total_points` is not positive.
    #[must_use]
    pub fn fraction_complete(&self) -> f32 {
        fraction_complete(self.current_points, self.total_points)
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.player_id));
        checksum.hash_i32(self.technology_id);
        checksum.hash_string(&self.technology_name);
        checksum.hash_f32(self.current_points);
        checksum.hash_f32(self.total_points);
        hash_resources(checksum, &self.cost);
        hash_trigger_state(checksum, self.trigger_state);
    }
}

/// Runtime prototype table selected by a retail training command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TrainingKind {
    /// Standalone unit prototype (`TrainUnit`, wire value 1).
    Unit = 1,
    /// Squad prototype (`TrainSquad`, wire value 3).
    Squad = 3,
}

/// Construction command represented by a building worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConstructionKind {
    /// Construction performed by the unfinished target itself.
    Build = 2,
    /// Socket construction queued on another building's shared worker.
    BuildOther = 13,
}

impl ConstructionKind {
    /// Authored command type spelling used by object and technology data.
    #[must_use]
    pub const fn command_name(self) -> &'static str {
        match self {
            Self::Build => "Build",
            Self::BuildOther => "BuildOther",
        }
    }
}

impl TrainingKind {
    /// Authored command type spelling used by object and technology data.
    #[must_use]
    pub const fn command_name(self) -> &'static str {
        match self {
            Self::Unit => "TrainUnit",
            Self::Squad => "TrainSquad",
        }
    }
}

/// Per-building lockout created by retail `InstantTrainWithRecharge` training.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrainingRecharge {
    kind: TrainingKind,
    prototype_id: i32,
    time_remaining: f32,
}

impl TrainingRecharge {
    /// Kind of prototype that the lockout blocks.
    #[must_use]
    pub const fn kind(self) -> TrainingKind {
        self.kind
    }

    /// Runtime prototype ID that the lockout blocks.
    #[must_use]
    pub const fn prototype_id(self) -> i32 {
        self.prototype_id
    }

    /// Time in seconds until the lockout ends.
    #[must_use]
    pub const fn time_remaining(self) -> f32 {
        self.time_remaining
    }

    fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.kind as u32);
        checksum.hash_i32(self.prototype_id);
        checksum.hash_f32(self.time_remaining);
    }
}

/// One paid unit or squad item in a building's shared production queue.
#[derive(Debug, Clone)]
pub struct TrainingTask {
    pub(crate) player_id: PlayerId,
    pub(crate) kind: TrainingKind,
    pub(crate) prototype_id: i32,
    pub(crate) prototype_name: String,
    pub(crate) current_points: f32,
    pub(crate) total_points: f32,
    pub(crate) cost: Resources,
    pub(crate) population_costs: Vec<PopulationCost>,
    pub(crate) train_limit_bucket: Option<u8>,
    pub(crate) trigger_state: Option<TriggerCommandStateRef>,
}

impl TrainingTask {
    /// Player who owns the trained unit or squad.
    #[must_use]
    pub fn player_id(&self) -> PlayerId {
        self.player_id
    }

    /// Kind of prototype that the task trains.
    #[must_use]
    pub fn kind(&self) -> TrainingKind {
        self.kind
    }

    /// Runtime prototype ID that the task trains.
    #[must_use]
    pub fn prototype_id(&self) -> i32 {
        self.prototype_id
    }

    /// Name of the prototype that the task trains.
    #[must_use]
    pub fn prototype_name(&self) -> &str {
        &self.prototype_name
    }

    /// Training points collected so far.
    #[must_use]
    pub fn current_points(&self) -> f32 {
        self.current_points
    }

    /// Training points needed to complete the task.
    #[must_use]
    pub fn total_points(&self) -> f32 {
        self.total_points
    }

    /// Progress in the range 0 to 1. Returns 0 if `total_points` is not positive.
    #[must_use]
    pub fn fraction_complete(&self) -> f32 {
        fraction_complete(self.current_points, self.total_points)
    }

    /// Population that the task reserves.
    #[must_use]
    pub fn population_costs(&self) -> &[PopulationCost] {
        &self.population_costs
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.player_id));
        checksum.hash_u32(self.kind as u32);
        checksum.hash_i32(self.prototype_id);
        checksum.hash_string(&self.prototype_name);
        checksum.hash_f32(self.current_points);
        checksum.hash_f32(self.total_points);
        hash_resources(checksum, &self.cost);
        hash_population(checksum, &self.population_costs);
        checksum.hash_u32(self.train_limit_bucket.map_or(u32::MAX, u32::from));
        hash_trigger_state(checksum, self.trigger_state);
    }
}

/// One paid construction item in a building's shared production worker.
#[derive(Debug, Clone)]
pub struct ConstructionTask {
    pub(crate) player_id: PlayerId,
    pub(crate) purchasing_player_id: PlayerId,
    pub(crate) kind: ConstructionKind,
    pub(crate) prototype_id: i32,
    pub(crate) prototype_name: String,
    pub(crate) current_points: f32,
    pub(crate) total_points: f32,
    pub(crate) cost: Resources,
    pub(crate) population_costs: Vec<PopulationCost>,
    pub(crate) target_building_id: Option<EntityId>,
    pub(crate) manual: bool,
    pub(crate) train_limit_bucket: Option<u8>,
    pub(crate) trigger_state: Option<TriggerCommandStateRef>,
}

impl ConstructionTask {
    /// Player who owns the new building.
    #[must_use]
    pub fn player_id(&self) -> PlayerId {
        self.player_id
    }

    /// Player who paid for the building and gets a refund on cancel.
    #[must_use]
    pub fn purchasing_player_id(&self) -> PlayerId {
        self.purchasing_player_id
    }

    /// Kind of construction command.
    #[must_use]
    pub fn kind(&self) -> ConstructionKind {
        self.kind
    }

    /// Runtime prototype ID of the building.
    #[must_use]
    pub fn prototype_id(&self) -> i32 {
        self.prototype_id
    }

    /// Name of the building prototype.
    #[must_use]
    pub fn prototype_name(&self) -> &str {
        &self.prototype_name
    }

    /// Construction points collected so far.
    #[must_use]
    pub fn current_points(&self) -> f32 {
        self.current_points
    }

    /// Construction points needed to complete the task.
    #[must_use]
    pub fn total_points(&self) -> f32 {
        self.total_points
    }

    /// Progress in the range 0 to 1. Returns 0 if `total_points` is not positive.
    #[must_use]
    pub fn fraction_complete(&self) -> f32 {
        fraction_complete(self.current_points, self.total_points)
    }

    /// Unfinished building that the task constructs, once it exists.
    #[must_use]
    pub fn target_building_id(&self) -> Option<EntityId> {
        self.target_building_id
    }

    /// Returns `true` if the prototype has the `ManualBuild` flag.
    #[must_use]
    pub fn is_manual(&self) -> bool {
        self.manual
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.player_id));
        checksum.hash_u32(u32::from(self.purchasing_player_id));
        checksum.hash_u32(self.kind as u32);
        checksum.hash_i32(self.prototype_id);
        checksum.hash_string(&self.prototype_name);
        checksum.hash_f32(self.current_points);
        checksum.hash_f32(self.total_points);
        hash_resources(checksum, &self.cost);
        hash_population(checksum, &self.population_costs);
        checksum.hash_u32(
            self.target_building_id
                .map_or(EntityId::INVALID.as_u32(), EntityId::as_u32),
        );
        checksum.hash_u32(u32::from(self.manual));
        checksum.hash_u32(self.train_limit_bucket.map_or(u32::MAX, u32::from));
        hash_trigger_state(checksum, self.trigger_state);
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ProductionTask {
    Research(ResearchTask),
    Training(TrainingTask),
    Construction(ConstructionTask),
}

impl ProductionTask {
    pub(crate) fn trigger_state(&self) -> Option<TriggerCommandStateRef> {
        match self {
            Self::Research(task) => task.trigger_state,
            Self::Training(task) => task.trigger_state,
            Self::Construction(task) => task.trigger_state,
        }
    }

    pub(crate) fn as_research(&self) -> Option<&ResearchTask> {
        match self {
            Self::Research(task) => Some(task),
            Self::Training(_) | Self::Construction(_) => None,
        }
    }

    pub(crate) fn as_training(&self) -> Option<&TrainingTask> {
        match self {
            Self::Research(_) | Self::Construction(_) => None,
            Self::Training(task) => Some(task),
        }
    }

    pub(crate) fn as_construction(&self) -> Option<&ConstructionTask> {
        match self {
            Self::Research(_) | Self::Training(_) => None,
            Self::Construction(task) => Some(task),
        }
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        match self {
            Self::Research(task) => {
                checksum.hash_u32(0);
                task.hash_state(checksum);
            }
            Self::Training(task) => {
                checksum.hash_u32(1);
                task.hash_state(checksum);
            }
            Self::Construction(task) => {
                checksum.hash_u32(2);
                task.hash_state(checksum);
            }
        }
    }
}

/// Retail-like single-worker production queue attached to a building unit.
#[derive(Debug, Clone, Default)]
pub struct BuildingProduction {
    pub(crate) current_item: Option<ProductionTask>,
    pub(crate) queued_items: Vec<ProductionTask>,
    recharges: Vec<TrainingRecharge>,
    trained_squads: Vec<TrainedSquadBirth>,
    trained_squad_birth_time: f32,
    pub(crate) air_traffic_control: Option<AirTrafficControl>,
}

impl BuildingProduction {
    /// Active persistent air-traffic-control state, when this unit owns it.
    #[must_use]
    pub const fn air_traffic_control(&self) -> Option<&AirTrafficControl> {
        self.air_traffic_control.as_ref()
    }

    /// Research task that the worker runs now, if any.
    #[must_use]
    pub fn current_research(&self) -> Option<&ResearchTask> {
        self.current_item
            .as_ref()
            .and_then(ProductionTask::as_research)
    }

    /// Training task that the worker runs now, if any.
    #[must_use]
    pub fn current_training(&self) -> Option<&TrainingTask> {
        self.current_item
            .as_ref()
            .and_then(ProductionTask::as_training)
    }

    /// Construction task that the worker runs now, if any.
    #[must_use]
    pub fn current_construction(&self) -> Option<&ConstructionTask> {
        self.current_item
            .as_ref()
            .and_then(ProductionTask::as_construction)
    }

    /// Iterates the queued research tasks in queue order.
    pub fn queued_research(&self) -> impl Iterator<Item = &ResearchTask> {
        self.queued_items
            .iter()
            .filter_map(ProductionTask::as_research)
    }

    /// Iterates the queued training tasks in queue order.
    pub fn queued_training(&self) -> impl Iterator<Item = &TrainingTask> {
        self.queued_items
            .iter()
            .filter_map(ProductionTask::as_training)
    }

    /// Iterates the queued construction tasks in queue order.
    pub fn queued_construction(&self) -> impl Iterator<Item = &ConstructionTask> {
        self.queued_items
            .iter()
            .filter_map(ProductionTask::as_construction)
    }

    /// Number of queued research tasks.
    #[must_use]
    pub fn queued_research_count(&self) -> usize {
        self.queued_research().count()
    }

    /// Number of queued training tasks.
    #[must_use]
    pub fn queued_training_count(&self) -> usize {
        self.queued_training().count()
    }

    /// Number of queued construction tasks.
    #[must_use]
    pub fn queued_construction_count(&self) -> usize {
        self.queued_construction().count()
    }

    /// Number of queued tasks of all kinds.
    #[must_use]
    pub fn queued_item_count(&self) -> usize {
        self.queued_items.len()
    }

    /// Returns `true` if no task runs and the queue is empty.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.current_item.is_none() && self.queued_items.is_empty()
    }

    /// Return the active instant-training lockout for one prototype.
    #[must_use]
    pub fn training_recharge(
        &self,
        kind: TrainingKind,
        prototype_id: i32,
    ) -> Option<TrainingRecharge> {
        self.recharges
            .iter()
            .copied()
            .find(|recharge| recharge.kind == kind && recharge.prototype_id == prototype_id)
    }

    /// Iterate authoritative instant-training lockouts in insertion order.
    pub fn training_recharges(&self) -> impl Iterator<Item = TrainingRecharge> + '_ {
        self.recharges.iter().copied()
    }

    pub(crate) fn needs_update(&self) -> bool {
        !self.is_idle() || !self.recharges.is_empty() || self.has_trained_squad_birth_work()
    }

    pub(crate) fn set_training_recharge(
        &mut self,
        kind: TrainingKind,
        prototype_id: i32,
        time_remaining: f32,
    ) {
        let time_remaining = time_remaining.max(0.0);
        if let Some(recharge) = self
            .recharges
            .iter_mut()
            .find(|recharge| recharge.kind == kind && recharge.prototype_id == prototype_id)
        {
            recharge.time_remaining = time_remaining;
            return;
        }
        self.recharges.push(TrainingRecharge {
            kind,
            prototype_id,
            time_remaining,
        });
    }

    pub(crate) fn advance_training_recharges(&mut self, dt: f32) {
        for recharge in &mut self.recharges {
            recharge.time_remaining -= dt;
        }
        self.recharges
            .retain(|recharge| recharge.time_remaining > 0.0);
    }

    pub(crate) fn enqueue_research(&mut self, task: ResearchTask) {
        self.queued_items.push(ProductionTask::Research(task));
    }

    pub(crate) fn enqueue_training(&mut self, task: TrainingTask) {
        self.queued_items.push(ProductionTask::Training(task));
    }

    pub(crate) fn enqueue_construction(&mut self, task: ConstructionTask) {
        self.queued_items.push(ProductionTask::Construction(task));
    }

    pub(crate) fn start_construction(&mut self, task: ConstructionTask) -> bool {
        if self.current_item.is_some() || !self.queued_items.is_empty() {
            return false;
        }
        self.current_item = Some(ProductionTask::Construction(task));
        true
    }

    pub(crate) fn promote_next(&mut self) -> bool {
        if self.current_item.is_some() || self.queued_items.is_empty() {
            return false;
        }
        self.current_item = Some(self.queued_items.remove(0));
        true
    }

    pub(crate) fn cancel_research(
        &mut self,
        player_id: PlayerId,
        technology_id: i32,
    ) -> Option<ResearchTask> {
        if let Some(index) = self.queued_items.iter().rposition(|item| {
            item.as_research().is_some_and(|task| {
                task.player_id == player_id && task.technology_id == technology_id
            })
        }) {
            let ProductionTask::Research(task) = self.queued_items.remove(index) else {
                unreachable!("research predicate selected another production task")
            };
            return Some(task);
        }
        if self
            .current_research()
            .is_some_and(|task| task.player_id == player_id && task.technology_id == technology_id)
        {
            let Some(ProductionTask::Research(task)) = self.current_item.take() else {
                unreachable!("research predicate selected another production task")
            };
            return Some(task);
        }
        None
    }

    pub(crate) fn cancel_training(
        &mut self,
        player_id: PlayerId,
        kind: TrainingKind,
        prototype_id: i32,
        count: u32,
    ) -> Vec<TrainingTask> {
        let mut canceled = Vec::new();
        while u32::try_from(canceled.len()).unwrap_or(u32::MAX) < count {
            let Some(index) = self.queued_items.iter().rposition(|item| {
                item.as_training()
                    .is_some_and(|task| training_matches(task, player_id, kind, prototype_id))
            }) else {
                break;
            };
            let ProductionTask::Training(task) = self.queued_items.remove(index) else {
                unreachable!("training predicate selected another production task")
            };
            canceled.push(task);
        }
        if u32::try_from(canceled.len()).unwrap_or(u32::MAX) < count
            && self
                .current_training()
                .is_some_and(|task| training_matches(task, player_id, kind, prototype_id))
        {
            let Some(ProductionTask::Training(task)) = self.current_item.take() else {
                unreachable!("training predicate selected another production task")
            };
            canceled.push(task);
        }
        canceled
    }

    pub(crate) fn cancel_construction(
        &mut self,
        player_id: PlayerId,
        kind: ConstructionKind,
        prototype_id: i32,
    ) -> Option<ConstructionTask> {
        if let Some(index) = self.queued_items.iter().rposition(|item| {
            item.as_construction()
                .is_some_and(|task| construction_matches(task, player_id, kind, prototype_id))
        }) {
            let ProductionTask::Construction(task) = self.queued_items.remove(index) else {
                unreachable!("construction predicate selected another production task")
            };
            return Some(task);
        }
        if self
            .current_construction()
            .is_some_and(|task| construction_matches(task, player_id, kind, prototype_id))
        {
            let Some(ProductionTask::Construction(task)) = self.current_item.take() else {
                unreachable!("construction predicate selected another production task")
            };
            return Some(task);
        }
        None
    }

    pub(crate) fn research_task(
        &self,
        player_id: PlayerId,
        technology_id: i32,
    ) -> Option<(&ResearchTask, bool)> {
        if let Some(task) = self
            .current_research()
            .filter(|task| task.player_id == player_id && task.technology_id == technology_id)
        {
            return Some((task, false));
        }
        self.queued_research()
            .find(|task| task.player_id == player_id && task.technology_id == technology_id)
            .map(|task| (task, true))
    }

    pub(crate) fn training_task(
        &self,
        player_id: PlayerId,
        kind: TrainingKind,
        prototype_id: i32,
    ) -> Option<(&TrainingTask, bool)> {
        if let Some(task) = self
            .current_training()
            .filter(|task| training_matches(task, player_id, kind, prototype_id))
        {
            return Some((task, false));
        }
        self.queued_training()
            .find(|task| training_matches(task, player_id, kind, prototype_id))
            .map(|task| (task, true))
    }

    pub(crate) fn construction_task(
        &self,
        player_id: PlayerId,
        kind: ConstructionKind,
        prototype_id: i32,
    ) -> Option<(&ConstructionTask, bool)> {
        if let Some(task) = self
            .current_construction()
            .filter(|task| construction_matches(task, player_id, kind, prototype_id))
        {
            return Some((task, false));
        }
        self.queued_construction()
            .find(|task| construction_matches(task, player_id, kind, prototype_id))
            .map(|task| (task, true))
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        if let Some(task) = &self.current_item {
            checksum.hash_u32(1);
            task.hash_state(checksum);
        } else {
            checksum.hash_u32(0);
        }
        checksum.hash_u32(u32::try_from(self.queued_items.len()).unwrap_or(u32::MAX));
        for task in &self.queued_items {
            task.hash_state(checksum);
        }
        checksum.hash_u32(u32::try_from(self.recharges.len()).unwrap_or(u32::MAX));
        for recharge in &self.recharges {
            recharge.hash_state(checksum);
        }
        self.hash_trained_squad_birth_state(checksum);
        checksum.hash_u32(u32::from(self.air_traffic_control.is_some()));
        if let Some(control) = &self.air_traffic_control {
            control.hash_state(checksum);
        }
    }

    pub(crate) fn tasks(&self) -> impl Iterator<Item = &ProductionTask> {
        self.current_item.iter().chain(self.queued_items.iter())
    }

    pub(crate) fn take_tasks(&mut self) -> Vec<ProductionTask> {
        let mut tasks = Vec::with_capacity(self.queued_items.len() + 1);
        if let Some(current) = self.current_item.take() {
            tasks.push(current);
        }
        tasks.append(&mut self.queued_items);
        tasks
    }

    pub(crate) fn training_tasks(&self) -> impl Iterator<Item = &TrainingTask> {
        self.tasks().filter_map(ProductionTask::as_training)
    }

    pub(crate) fn construction_tasks(&self) -> impl Iterator<Item = &ConstructionTask> {
        self.tasks().filter_map(ProductionTask::as_construction)
    }
}

/// Snapshot exposed to renderer/UI consumers without duplicating sim logic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResearchProgress {
    /// Building that runs or queues the research.
    pub building_id: EntityId,
    /// Research points collected so far.
    pub current_points: f32,
    /// Research points needed to complete the research.
    pub total_points: f32,
    /// `true` if the research waits in the queue; `false` if it runs now.
    pub queued: bool,
}

impl ResearchProgress {
    /// Progress in the range 0 to 1. Returns 0 if `total_points` is not positive.
    #[must_use]
    pub fn fraction_complete(self) -> f32 {
        fraction_complete(self.current_points, self.total_points)
    }
}

/// Authoritative training progress exposed to renderer/UI consumers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrainingProgress {
    /// Building that runs or queues the training.
    pub building_id: EntityId,
    /// Kind of prototype that the building trains.
    pub kind: TrainingKind,
    /// Runtime prototype ID that the building trains.
    pub prototype_id: i32,
    /// Training points collected so far.
    pub current_points: f32,
    /// Training points needed to complete the training.
    pub total_points: f32,
    /// `true` if the training waits in the queue; `false` if it runs now.
    pub queued: bool,
}

impl TrainingProgress {
    /// Progress in the range 0 to 1. Returns 0 if `total_points` is not positive.
    #[must_use]
    pub fn fraction_complete(self) -> f32 {
        fraction_complete(self.current_points, self.total_points)
    }
}

/// Authoritative construction progress exposed to renderer/UI consumers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConstructionProgress {
    /// Unit whose production worker owns the task.
    pub worker_id: EntityId,
    /// Unfinished building that the task constructs, once it exists.
    pub building_id: Option<EntityId>,
    /// Kind of construction command.
    pub kind: ConstructionKind,
    /// Runtime prototype ID of the building.
    pub prototype_id: i32,
    /// Construction points collected so far.
    pub current_points: f32,
    /// Construction points needed to complete the building.
    pub total_points: f32,
    /// `true` if the construction waits in the queue; `false` if it runs now.
    pub queued: bool,
}

impl ConstructionProgress {
    /// Progress in the range 0 to 1. Returns 0 if `total_points` is not positive.
    #[must_use]
    pub fn fraction_complete(self) -> f32 {
        fraction_complete(self.current_points, self.total_points)
    }
}

fn training_matches(
    task: &TrainingTask,
    player_id: PlayerId,
    kind: TrainingKind,
    prototype_id: i32,
) -> bool {
    task.player_id == player_id && task.kind == kind && task.prototype_id == prototype_id
}

fn construction_matches(
    task: &ConstructionTask,
    player_id: PlayerId,
    kind: ConstructionKind,
    prototype_id: i32,
) -> bool {
    task.purchasing_player_id == player_id && task.kind == kind && task.prototype_id == prototype_id
}

fn fraction_complete(current_points: f32, total_points: f32) -> f32 {
    if total_points > 0.0 {
        (current_points / total_points).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn hash_resources(checksum: &mut SyncChecksum, resources: &Resources) {
    for amount in resources.amounts {
        checksum.hash_f32(amount);
    }
}

fn hash_population(checksum: &mut SyncChecksum, costs: &[PopulationCost]) {
    checksum.hash_u32(u32::try_from(costs.len()).unwrap_or(u32::MAX));
    for cost in costs {
        checksum.hash_u32(u32::try_from(cost.population_type).unwrap_or(u32::MAX));
        checksum.hash_f32(cost.amount);
    }
}

fn hash_trigger_state(checksum: &mut SyncChecksum, trigger_state: Option<TriggerCommandStateRef>) {
    if let Some(trigger_state) = trigger_state {
        checksum.hash_u32(1);
        trigger_state.hash_state(checksum);
    } else {
        checksum.hash_u32(0);
    }
}
