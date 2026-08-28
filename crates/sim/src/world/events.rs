//! Checksummed retail general events and renderer-facing presentation requests.

use std::collections::BTreeMap;

use glam::Vec3;

use super::World;
use crate::EntityId;
use crate::sync::SyncChecksum;
use crate::trigger::EntityFilterSet;

/// A retail `BEventDefinitions` identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum GeneralEventType {
    ControlTilt = 1,
    ControlZoom = 2,
    ControlRotate = 3,
    ControlPan = 4,
    ControlCircleSelect = 5,
    ControlCircleMultiSelect = 6,
    ControlClearAllSelections = 7,
    ControlModifierAction = 8,
    ControlModifierSpeed = 9,
    ControlResetCameraSettings = 10,
    ControlGotoRally = 11,
    ControlGotoBase = 12,
    ControlGotoScout = 13,
    ControlGotoNode = 14,
    ControlGotoHero = 15,
    ControlGotoAlert = 16,
    ControlGotoSelected = 17,
    ControlGroupSelect = 18,
    ControlGroupGoto = 19,
    ControlGroupAssign = 20,
    ControlGroupAddTo = 21,
    ControlScreenSelectMilitary = 22,
    ControlGlobalSelect = 23,
    ControlDoubleClickSelect = 24,
    ControlFindCrowdMilitary = 25,
    ControlFindCrowdVillager = 26,
    ControlSetRallyPoint = 27,
    Flare = 28,
    FlareHelp = 29,
    FlareMeet = 30,
    FlareAttack = 31,
    MenuShowCommand = 32,
    MenuCloseCommand = 33,
    MenuNavCommand = 34,
    MenuCommandHasFocus0 = 35,
    MenuCommandHasFocus1 = 36,
    MenuCommandHasFocus2 = 37,
    MenuCommandHasFocus3 = 38,
    MenuCommandHasFocus4 = 39,
    MenuCommandHasFocus5 = 40,
    MenuCommandHasFocus6 = 41,
    MenuCommandHasFocus7 = 42,
    MenuCommandHasFocusN = 43,
    MenuCommandClickmenuN = 44,
    MenuCommandIsMenuOpen = 45,
    MenuCommanndIsMenuNotOpen = 46,
    MenuShowPower = 47,
    MenuClosePower = 48,
    MenuPowerHasFocusN = 49,
    MenuPowerClickmenuN = 50,
    MenuPowerIsMenuOpen = 51,
    MenuPowerIsMenuNotOpen = 52,
    MenuShowSelectPower = 53,
    MenuShowAbility = 54,
    MenuShowTribute = 55,
    MenuShowObjectives = 56,
    GameEntityBuilt = 57,
    GameEntityKilled = 58,
    ChatShown = 59,
    ChatRemoved = 60,
    ChatCompleted = 61,
    CommandBowl = 62,
    CommandAbility = 63,
    CommandUnpack = 64,
    CommandDoWork = 65,
    CommandAttack = 66,
    CommandMove = 67,
    CommandTrainSquad = 68,
    CommandTrainSquadCancel = 69,
    CommandResearch = 70,
    CommandResearchCancel = 71,
    CommandBuildOther = 72,
    CommandRecycle = 73,
    CommandRecycleCancel = 74,
    CameraLookingAt = 75,
    SelectUnits = 76,
    CinematicCompleted = 77,
    FadeCompleted = 78,
    UsedPower = 79,
    Timer1Sec = 80,
    ControlCircleSelectFullyGrown = 81,
    PowerOrbitalComplete = 82,
    GameEntityRammed = 83,
    GameEntityJacked = 84,
    GameEntityKilledByNonProjectile = 85,
}

impl GeneralEventType {
    /// Convert a retail numeric event identifier.
    #[must_use]
    pub fn from_u16(value: u16) -> Option<Self> {
        if (1..=85).contains(&value) {
            // SAFETY: the retail event enumeration is contiguous from 1 to 85.
            Some(unsafe { std::mem::transmute::<u16, Self>(value) })
        } else {
            None
        }
    }

    /// Resolve an exact retail-authored event name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        GENERAL_EVENT_NAMES
            .iter()
            .position(|candidate| *candidate == name)
            .and_then(|index| u16::try_from(index + 1).ok())
            .and_then(Self::from_u16)
    }

    /// Return the retail-authored event name.
    #[must_use]
    pub fn name(self) -> &'static str {
        GENERAL_EVENT_NAMES[usize::from(self as u16) - 1]
    }
}

const GENERAL_EVENT_NAMES: [&str; 85] = [
    "ControlTilt",
    "ControlZoom",
    "ControlRotate",
    "ControlPan",
    "ControlCircleSelect",
    "ControlCircleMultiSelect",
    "ControlClearAllSelections",
    "ControlModifierAction",
    "ControlModifierSpeed",
    "ControlResetCameraSettings",
    "ControlGotoRally",
    "ControlGotoBase",
    "ControlGotoScout",
    "ControlGotoNode",
    "ControlGotoHero",
    "ControlGotoAlert",
    "ControlGotoSelected",
    "ControlGroupSelect",
    "ControlGroupGoto",
    "ControlGroupAssign",
    "ControlGroupAddTo",
    "ControlScreenSelectMilitary",
    "ControlGlobalSelect",
    "ControlDoubleClickSelect",
    "ControlFindCrowdMilitary",
    "ControlFindCrowdVillager",
    "ControlSetRallyPoint",
    "Flare",
    "FlareHelp",
    "FlareMeet",
    "FlareAttack",
    "MenuShowCommand",
    "MenuCloseCommand",
    "MenuNavCommand",
    "MenuCommandHasFocus0",
    "MenuCommandHasFocus1",
    "MenuCommandHasFocus2",
    "MenuCommandHasFocus3",
    "MenuCommandHasFocus4",
    "MenuCommandHasFocus5",
    "MenuCommandHasFocus6",
    "MenuCommandHasFocus7",
    "MenuCommandHasFocusN",
    "MenuCommandClickmenuN",
    "MenuCommandIsMenuOpen",
    "MenuCommanndIsMenuNotOpen",
    "MenuShowPower",
    "MenuClosePower",
    "MenuPowerHasFocusN",
    "MenuPowerClickmenuN",
    "MenuPowerIsMenuOpen",
    "MenuPowerIsMenuNotOpen",
    "MenuShowSelectPower",
    "MenuShowAbility",
    "MenuShowTribute",
    "MenuShowObjectives",
    "GameEntityBuilt",
    "GameEntityKilled",
    "ChatShown",
    "ChatRemoved",
    "ChatCompleted",
    "CommandBowl",
    "CommandAbility",
    "CommandUnpack",
    "CommandDoWork",
    "CommandAttack",
    "CommandMove",
    "CommandTrainSquad",
    "CommandTrainSquadCancel",
    "CommandResearch",
    "CommandResearchCancel",
    "CommandBuildOther",
    "CommandRecycle",
    "CommandRecycleCancel",
    "CameraLookingAt",
    "SelectUnits",
    "CinematicCompleted",
    "FadeCompleted",
    "UsedPower",
    "Timer1Sec",
    "ControlCircleSelectFullyGrown",
    "PowerOrbitalComplete",
    "GameEntityRammed",
    "GameEntityJacked",
    "GameEntityKilledByNonProjectile",
];

/// One synchronized occurrence offered to general-event subscribers.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneralEvent {
    pub event_type: GeneralEventType,
    pub player_id: i32,
    pub source: Option<EntityId>,
    pub target: Option<EntityId>,
    pub source_entities: Vec<EntityId>,
    pub target_entities: Vec<EntityId>,
    /// Terrain-intersection point at the center of the current camera view.
    pub camera_focus: Option<Vec3>,
}

impl GeneralEvent {
    /// Create an event with no entity or camera payload.
    #[must_use]
    pub fn new(event_type: GeneralEventType, player_id: i32) -> Self {
        Self {
            event_type,
            player_id,
            source: None,
            target: None,
            source_entities: Vec::new(),
            target_entities: Vec::new(),
            camera_focus: None,
        }
    }

    /// Attach singular source and target entities.
    #[must_use]
    pub fn with_entities(mut self, source: Option<EntityId>, target: Option<EntityId>) -> Self {
        self.source = source;
        self.target = target;
        self
    }

    /// Attach the camera's terrain focus point.
    #[must_use]
    pub fn with_camera_focus(mut self, camera_focus: Vec3) -> Self {
        self.camera_focus = Some(camera_focus);
        self
    }
}

/// Renderer-facing work authored by authoritative trigger effects.
#[derive(Debug, Clone, PartialEq)]
pub enum PresentationRequest {
    Chat(ChatRequest),
    Cinematic(CinematicRequest),
}

impl PresentationRequest {
    /// Stable synchronized request identifier used by completion input.
    #[must_use]
    pub const fn id(&self) -> u32 {
        match self {
            Self::Chat(request) => request.id,
            Self::Cinematic(request) => request.id,
        }
    }
}

/// A trigger-authored subtitle, voice cue, and talking-head request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatRequest {
    pub id: u32,
    pub sound_cue: String,
    pub queue_sound: bool,
    pub string_id: i32,
    pub duration_ms: u32,
    pub talking_head_id: Option<i32>,
}

/// A trigger-authored cinematic request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CinematicRequest {
    pub id: u32,
    pub cinematic_id: i32,
    pub possessed_squads: Vec<EntityId>,
    pub pre_rendered: bool,
}

#[derive(Debug, Clone)]
struct GeneralEventSubscriber {
    id: u32,
    fired: bool,
    fired_count: u32,
    fire_time: u32,
    use_count: bool,
    player_filter: Option<i32>,
    filters: Vec<GeneralEventFilter>,
}

#[derive(Debug, Clone)]
enum GeneralEventFilter {
    Entity {
        parameter: EventEntityParameter,
        filter_set: EntityFilterSet,
    },
    EntityList {
        parameter: EventEntityParameter,
        filter_set: EntityFilterSet,
        minimum_matches: u32,
        all_must_match: bool,
    },
    Camera {
        radius: f32,
        location: Option<Vec3>,
        entity: Option<EntityId>,
        invert: bool,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum EventEntityParameter {
    Source,
    Target,
}

#[derive(Debug, Default)]
pub(super) struct GeneralEventState {
    subscribers: BTreeMap<u16, Vec<GeneralEventSubscriber>>,
    chat_completed_subscriber: Option<u32>,
    cinematic_completed_subscriber: Option<u32>,
}

#[derive(Debug, Default)]
pub(super) struct PresentationState {
    next_request_id: u32,
    requests: BTreeMap<u32, PresentationRequest>,
    active_cinematic: Option<u32>,
}

impl World {
    /// Add a synchronized general-event subscriber and return its retail ID.
    pub fn subscribe_general_event(
        &mut self,
        event_type: GeneralEventType,
        player_filter: Option<i32>,
        use_count: bool,
    ) -> u32 {
        self.general_events
            .subscribe(event_type, player_filter, use_count)
    }

    /// Fire one synchronized event against the current subscriber/filter state.
    pub fn fire_general_event(&mut self, event: &GeneralEvent) -> usize {
        let mut events = std::mem::take(&mut self.general_events);
        let fired = events.fire(event, self);
        self.general_events = events;
        fired
    }

    /// Return whether a retail subscriber has fired.
    #[must_use]
    pub fn general_event_fired(&self, subscriber_id: u32) -> bool {
        self.general_events
            .get(subscriber_id)
            .is_some_and(|subscriber| subscriber.fired)
    }

    /// Return the outstanding count for a ref-counted retail subscriber.
    #[must_use]
    pub fn general_event_fire_count(&self, subscriber_id: u32) -> u32 {
        self.general_events
            .get(subscriber_id)
            .map_or(0, |subscriber| subscriber.fired_count)
    }

    /// Iterate UI work in stable request-ID order without duplicating game state.
    pub fn presentation_requests(&self) -> impl Iterator<Item = &PresentationRequest> {
        self.presentation.requests.values()
    }

    /// Apply a synchronized UI completion input to its authoritative request.
    pub fn acknowledge_presentation(&mut self, request_id: u32, player_id: i32) -> bool {
        let Some(request) = self.presentation.complete(request_id) else {
            return false;
        };
        let event_type = match request {
            PresentationRequest::Chat(_) => GeneralEventType::ChatCompleted,
            PresentationRequest::Cinematic(_) => GeneralEventType::CinematicCompleted,
        };
        self.fire_general_event(&GeneralEvent::new(event_type, player_id));
        true
    }

    pub(crate) fn request_chat(&mut self, request: ChatRequest) -> u32 {
        let subscriber_id = self.general_events.ensure_chat_completed_subscriber();
        self.general_events.reset_fired(subscriber_id, false);
        self.presentation.add_chat(request)
    }

    pub(crate) fn request_cinematic(&mut self, request: CinematicRequest) -> Option<u32> {
        if self.presentation.active_cinematic.is_some() {
            return None;
        }
        let subscriber_id = self.general_events.ensure_cinematic_completed_subscriber();
        self.general_events.reset_fired(subscriber_id, false);
        Some(self.presentation.add_cinematic(request))
    }

    pub(crate) fn chat_completed(&self) -> Option<(bool, u32)> {
        self.general_events
            .chat_completed_subscriber
            .and_then(|id| self.general_events.get(id))
            .map(|subscriber| (subscriber.fired, subscriber.fire_time))
    }

    pub(crate) fn cinematic_completed(&self) -> bool {
        self.general_events
            .cinematic_completed_subscriber
            .and_then(|id| self.general_events.get(id))
            .is_some_and(|subscriber| subscriber.fired)
    }

    pub(crate) fn reset_general_event(&mut self, subscriber_id: u32, consume_count: bool) {
        self.general_events
            .reset_fired(subscriber_id, consume_count);
    }

    pub(crate) fn clear_general_event_filters(&mut self, subscriber_id: u32) {
        if let Some(subscriber) = self.general_events.get_mut(subscriber_id) {
            subscriber.filters.clear();
        }
    }

    pub(crate) fn add_general_event_entity_filter(
        &mut self,
        subscriber_id: u32,
        parameter: EventEntityParameter,
        filter_set: EntityFilterSet,
    ) -> bool {
        let Some(subscriber) = self.general_events.get_mut(subscriber_id) else {
            return false;
        };
        subscriber.filters.push(GeneralEventFilter::Entity {
            parameter,
            filter_set,
        });
        true
    }

    pub(crate) fn add_general_event_entity_list_filter(
        &mut self,
        subscriber_id: u32,
        filter_set: EntityFilterSet,
        minimum_matches: u32,
        all_must_match: bool,
    ) -> bool {
        let Some(subscriber) = self.general_events.get_mut(subscriber_id) else {
            return false;
        };
        subscriber.filters.push(GeneralEventFilter::EntityList {
            parameter: EventEntityParameter::Source,
            filter_set,
            minimum_matches,
            all_must_match,
        });
        true
    }

    pub(crate) fn add_general_event_camera_filter(
        &mut self,
        subscriber_id: u32,
        radius: f32,
        location: Option<Vec3>,
        entity: Option<EntityId>,
        invert: bool,
    ) -> bool {
        let Some(subscriber) = self.general_events.get_mut(subscriber_id) else {
            return false;
        };
        subscriber.filters.push(GeneralEventFilter::Camera {
            radius,
            location,
            entity,
            invert,
        });
        true
    }
}

impl GeneralEventState {
    fn subscribe(
        &mut self,
        event_type: GeneralEventType,
        player_filter: Option<i32>,
        use_count: bool,
    ) -> u32 {
        let subscribers = self.subscribers.entry(event_type as u16).or_default();
        let index = u32::try_from(subscribers.len()).unwrap_or(u32::MAX >> 16);
        let id = (index << 16).wrapping_add(u32::from(event_type as u16));
        subscribers.push(GeneralEventSubscriber {
            id,
            fired: false,
            fired_count: 0,
            fire_time: 0,
            use_count,
            player_filter,
            filters: Vec::new(),
        });
        id
    }

    fn ensure_chat_completed_subscriber(&mut self) -> u32 {
        if let Some(id) = self.chat_completed_subscriber {
            return id;
        }
        let id = self.subscribe(GeneralEventType::ChatCompleted, None, false);
        self.chat_completed_subscriber = Some(id);
        id
    }

    fn ensure_cinematic_completed_subscriber(&mut self) -> u32 {
        if let Some(id) = self.cinematic_completed_subscriber {
            return id;
        }
        let id = self.subscribe(GeneralEventType::CinematicCompleted, None, false);
        self.cinematic_completed_subscriber = Some(id);
        id
    }

    fn get(&self, id: u32) -> Option<&GeneralEventSubscriber> {
        let event_type = u16::try_from(id & 0xFFFF).ok()?;
        let index = usize::try_from(id >> 16).ok()?;
        self.subscribers.get(&event_type)?.get(index)
    }

    fn get_mut(&mut self, id: u32) -> Option<&mut GeneralEventSubscriber> {
        let event_type = u16::try_from(id & 0xFFFF).ok()?;
        let index = usize::try_from(id >> 16).ok()?;
        self.subscribers.get_mut(&event_type)?.get_mut(index)
    }

    fn reset_fired(&mut self, id: u32, consume_count: bool) {
        let Some(subscriber) = self.get_mut(id) else {
            return;
        };
        if consume_count && subscriber.use_count {
            subscriber.fired_count = subscriber.fired_count.saturating_sub(1);
            if subscriber.fired_count == 0 {
                subscriber.fired = false;
            }
        } else {
            subscriber.fired = false;
        }
    }

    fn fire(&mut self, event: &GeneralEvent, world: &World) -> usize {
        let Some(subscribers) = self.subscribers.get_mut(&(event.event_type as u16)) else {
            return 0;
        };
        let mut fired = 0;
        for subscriber in subscribers {
            if subscriber.accepts(event, world) {
                subscriber.fired = true;
                subscriber.fire_time = world.game_time_ms;
                if subscriber.use_count {
                    subscriber.fired_count = subscriber.fired_count.saturating_add(1);
                }
                fired += 1;
            }
        }
        fired
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.subscribers.len()).unwrap_or(u32::MAX));
        for (event_type, subscribers) in &self.subscribers {
            checksum.hash_u32(u32::from(*event_type));
            checksum.hash_u32(u32::try_from(subscribers.len()).unwrap_or(u32::MAX));
            for subscriber in subscribers {
                subscriber.hash_state(checksum);
            }
        }
        hash_optional_u32(checksum, self.chat_completed_subscriber);
        hash_optional_u32(checksum, self.cinematic_completed_subscriber);
    }
}

impl GeneralEventSubscriber {
    fn accepts(&self, event: &GeneralEvent, world: &World) -> bool {
        if self.fired && !self.use_count {
            return false;
        }
        if self
            .player_filter
            .is_some_and(|player_id| player_id != event.player_id)
        {
            return false;
        }
        self.filters
            .iter()
            .all(|filter| filter.matches(event, world))
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.id);
        checksum.hash_u32(u32::from(self.fired));
        checksum.hash_u32(self.fired_count);
        checksum.hash_u32(self.fire_time);
        checksum.hash_u32(u32::from(self.use_count));
        checksum.hash_i32(self.player_filter.unwrap_or(-1));
        checksum.hash_u32(u32::try_from(self.filters.len()).unwrap_or(u32::MAX));
        for filter in &self.filters {
            filter.hash_state(checksum);
        }
    }
}

impl GeneralEventFilter {
    fn matches(&self, event: &GeneralEvent, world: &World) -> bool {
        match self {
            Self::Entity {
                parameter,
                filter_set,
            } => parameter
                .entity(event)
                .is_some_and(|entity_id| filter_set.matches_entity(entity_id, world)),
            Self::EntityList {
                parameter,
                filter_set,
                minimum_matches,
                all_must_match,
            } => {
                let entities = parameter.entities(event);
                if entities.is_empty() {
                    return false;
                }
                let passed = entities
                    .iter()
                    .filter(|entity_id| filter_set.matches_entity(**entity_id, world))
                    .count();
                if *all_must_match && (passed == 0 || passed != entities.len()) {
                    return false;
                }
                passed >= usize::try_from(*minimum_matches).unwrap_or(usize::MAX)
            }
            Self::Camera {
                radius,
                location,
                entity,
                invert,
            } => {
                let Some(focus) = event.camera_focus else {
                    return *invert;
                };
                let location_match =
                    location.is_some_and(|location| focus.distance(location) < *radius);
                let entity_match = entity
                    .and_then(|entity_id| world.entity_position(entity_id))
                    .is_some_and(|position| focus.distance(position) < *radius);
                (location_match || entity_match) != *invert
            }
        }
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        match self {
            Self::Entity {
                parameter,
                filter_set,
            } => {
                checksum.hash_u32(0);
                checksum.hash_u32(*parameter as u32);
                filter_set.hash_state(checksum);
            }
            Self::EntityList {
                parameter,
                filter_set,
                minimum_matches,
                all_must_match,
            } => {
                checksum.hash_u32(1);
                checksum.hash_u32(*parameter as u32);
                filter_set.hash_state(checksum);
                checksum.hash_u32(*minimum_matches);
                checksum.hash_u32(u32::from(*all_must_match));
            }
            Self::Camera {
                radius,
                location,
                entity,
                invert,
            } => {
                checksum.hash_u32(2);
                checksum.hash_f32(*radius);
                hash_optional_vec3(checksum, *location);
                hash_optional_entity(checksum, *entity);
                checksum.hash_u32(u32::from(*invert));
            }
        }
    }
}

impl EventEntityParameter {
    fn entity(self, event: &GeneralEvent) -> Option<EntityId> {
        match self {
            Self::Source => event.source,
            Self::Target => event.target,
        }
    }

    fn entities(self, event: &GeneralEvent) -> &[EntityId] {
        match self {
            Self::Source => &event.source_entities,
            Self::Target => &event.target_entities,
        }
    }
}

impl PresentationState {
    fn allocate_id(&mut self) -> u32 {
        self.next_request_id = self.next_request_id.wrapping_add(1);
        if self.next_request_id == 0 {
            self.next_request_id = 1;
        }
        self.next_request_id
    }

    fn add_chat(&mut self, mut request: ChatRequest) -> u32 {
        let id = self.allocate_id();
        request.id = id;
        self.requests.insert(id, PresentationRequest::Chat(request));
        id
    }

    fn add_cinematic(&mut self, mut request: CinematicRequest) -> u32 {
        let id = self.allocate_id();
        request.id = id;
        self.requests
            .insert(id, PresentationRequest::Cinematic(request));
        self.active_cinematic = Some(id);
        id
    }

    fn complete(&mut self, id: u32) -> Option<PresentationRequest> {
        let request = self.requests.remove(&id)?;
        if self.active_cinematic == Some(id) {
            self.active_cinematic = None;
        }
        Some(request)
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.next_request_id);
        checksum.hash_u32(u32::try_from(self.requests.len()).unwrap_or(u32::MAX));
        for request in self.requests.values() {
            match request {
                PresentationRequest::Chat(request) => request.hash_state(checksum),
                PresentationRequest::Cinematic(request) => request.hash_state(checksum),
            }
        }
        hash_optional_u32(checksum, self.active_cinematic);
    }
}

impl ChatRequest {
    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(0);
        checksum.hash_u32(self.id);
        checksum.hash_u32(u32::try_from(self.sound_cue.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(self.sound_cue.as_bytes());
        checksum.hash_u32(u32::from(self.queue_sound));
        checksum.hash_i32(self.string_id);
        checksum.hash_u32(self.duration_ms);
        checksum.hash_i32(self.talking_head_id.unwrap_or(-1));
    }
}

impl CinematicRequest {
    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(1);
        checksum.hash_u32(self.id);
        checksum.hash_i32(self.cinematic_id);
        checksum.hash_u32(u32::try_from(self.possessed_squads.len()).unwrap_or(u32::MAX));
        for squad_id in &self.possessed_squads {
            checksum.hash_u32(squad_id.as_u32());
        }
        checksum.hash_u32(u32::from(self.pre_rendered));
    }
}

fn hash_optional_u32(checksum: &mut SyncChecksum, value: Option<u32>) {
    checksum.hash_u32(value.unwrap_or(u32::MAX));
}

fn hash_optional_entity(checksum: &mut SyncChecksum, value: Option<EntityId>) {
    checksum.hash_u32(value.unwrap_or(EntityId::INVALID).as_u32());
}

fn hash_optional_vec3(checksum: &mut SyncChecksum, value: Option<Vec3>) {
    if let Some(value) = value {
        checksum.hash_u32(1);
        checksum.hash_vec3(value.x, value.y, value.z);
    } else {
        checksum.hash_u32(0);
    }
}

#[cfg(test)]
#[path = "events/tests.rs"]
mod tests;
