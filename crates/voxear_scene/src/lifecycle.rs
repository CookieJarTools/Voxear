//! Behaviour lifecycle with a guaranteed order.
//!
//! Unity interleaves Awake and OnEnable per object and gives no ordering
//! between objects loaded together. Voxear guarantees instead:
//!
//! 1. Every behaviour sees Awake → OnEnable → Start, in that order, once each.
//! 2. Behaviours that arrive together (a scene load, an instantiated
//!    hierarchy) form a batch: Awake runs on the whole batch before OnEnable
//!    runs on any of it.
//! 3. No Start runs while any behaviour is still waiting for Awake or
//!    OnEnable, so Start always sees a fully initialised world.
//! 4. Within a phase, behaviours run by execution order, then creation order
//!    (hierarchy order for scene loads).
//!
//! Instantiating is the one exception to waiting for a flush, as in Unity:
//! [`activate`](Lifecycle::activate) runs Awake and OnEnable on the new batch
//! before it returns, and its Start still waits for the next flush.
//!
//! A behaviour on an Obj that is switched off (see [`crate::Inactive`]) is
//! not enabled however it asks to be: it gets OnDisable when its Obj goes
//! off and OnEnable when it comes back, as in Unity. It still gets its Awake
//! while off, which Unity holds back until the Obj comes on.

use std::collections::{HashMap, HashSet};

use voxear_ecs::Entity;

/// Index of a managed behaviour instance, assigned by the script host.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
#[repr(transparent)]
pub struct BehaviourHandle(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(i32)]
pub enum Phase {
    Awake = 0,
    OnEnable = 1,
    Start = 2,
    FixedUpdate = 3,
    Update = 4,
    LateUpdate = 5,
    OnDisable = 6,
    OnDestroy = 7,
    /// After animation and the engine's own posing, before Objs on bones
    /// follow them.
    PoseUpdate = 8,
    /// Its Obj's UI Panel has built a document: after Start, and at each
    /// frame's start, where scripts look whether the document is a new one,
    /// so the behaviour hears of each document once.
    OnUiBuilt = 9,
}

/// The phases a behaviour type overrides. Phases it does not override are
/// never dispatched, so empty callbacks cost nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(transparent)]
pub struct Callbacks(pub u32);

impl Callbacks {
    pub const NONE: Callbacks = Callbacks(0);
    pub const ALL: Callbacks = Callbacks(0x3ff);
    /// Not a callback but a mark beside them: a `[Predicted]` behaviour,
    /// whose FixedUpdate a networked player's machine runs again for its
    /// own Objs when the server disagrees.
    pub const PREDICTED: u32 = 1 << 16;

    pub const fn has(self, mark: u32) -> bool {
        self.0 & mark != 0
    }

    pub const fn of(phase: Phase) -> Self {
        Self(1 << phase as u32)
    }

    pub const fn contains(self, phase: Phase) -> bool {
        self.0 & (1 << phase as u32) != 0
    }

    pub const fn with(self, phase: Phase) -> Self {
        Self(self.0 | Self::of(phase).0)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Registration {
    pub handle: BehaviourHandle,
    pub entity: Entity,
    pub callbacks: Callbacks,
    /// `[ExecutionOrder]` value; lower runs first.
    pub order: i32,
    pub enabled: bool,
}

/// A change script code asked for while its callbacks were running.
#[derive(Clone, Copy, Debug)]
pub enum Request {
    /// A behaviour created mid-dispatch, to start with the next batch.
    Register(Registration),
    /// A behaviour created mid-dispatch that scripts already took through
    /// Awake and OnEnable themselves, such as an Obj instantiated in Update.
    Adopt(Registration, Adopted),
    SetEnabled(BehaviourHandle, bool),
    Destroy(BehaviourHandle),
}

/// How far scripts took an adopted behaviour. Awake has always run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Adopted {
    /// OnEnable ran.
    pub enabled: bool,
    /// It was destroyed during its own activation.
    pub destroyed: bool,
}

/// Runs behaviour callbacks; implemented by the C# script host.
pub trait BehaviourHost {
    fn invoke(&mut self, phase: Phase, behaviours: &[BehaviourHandle]);

    /// Frees destroyed behaviours after their last callback.
    fn release(&mut self, behaviours: &[BehaviourHandle]);

    /// Hands over the requests made during the last `invoke`.
    fn drain_requests(&mut self, _out: &mut Vec<Request>) {}
}

struct Record {
    entity: Entity,
    callbacks: Callbacks,
    order: i32,
    seq: u64,
    awake: bool,
    enabled: bool,
    wants_enabled: bool,
    /// Its Obj is on, and everything above it.
    obj_active: bool,
    started: bool,
    destroying: bool,
}

impl Record {
    /// Whether it should be enabled: it asks to be and its Obj is on.
    fn wants(&self) -> bool {
        self.wants_enabled && self.obj_active
    }
}

#[derive(Default)]
pub struct Lifecycle {
    records: HashMap<BehaviourHandle, Record>,
    pending: Vec<BehaviourHandle>,
    toggled: Vec<BehaviourHandle>,
    to_start: Vec<BehaviourHandle>,
    destroying: Vec<BehaviourHandle>,
    running: Vec<BehaviourHandle>,
    running_dirty: bool,
    next_seq: u64,
    requests: Vec<Request>,
    dispatch: Vec<BehaviourHandle>,
    /// The Objs switched off, as last followed.
    inactive: HashSet<Entity>,
}

impl Lifecycle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues a behaviour for its first Awake at the next [`flush`](Self::flush).
    pub fn register(&mut self, registration: Registration) {
        let record = Record {
            entity: registration.entity,
            callbacks: registration.callbacks,
            order: registration.order,
            seq: self.next_seq,
            awake: false,
            enabled: false,
            wants_enabled: registration.enabled,
            obj_active: !self.inactive.contains(&registration.entity),
            started: false,
            destroying: false,
        };
        self.next_seq += 1;
        let previous = self.records.insert(registration.handle, record);
        assert!(previous.is_none(), "{:?} registered twice", registration.handle);
        self.pending.push(registration.handle);
    }

    /// Takes effect at the next flush: OnDisable, or OnEnable (plus Start the
    /// first time).
    pub fn set_enabled(&mut self, handle: BehaviourHandle, enabled: bool) {
        let Some(record) = self.records.get_mut(&handle) else { return };
        if record.wants_enabled == enabled {
            return;
        }
        record.wants_enabled = enabled;
        if record.awake {
            self.toggled.push(handle);
        }
    }

    /// Deferred to [`end_of_frame`](Self::end_of_frame), like Unity's `Destroy`.
    /// The behaviour gets no further updates in the meantime.
    pub fn destroy(&mut self, handle: BehaviourHandle) {
        if let Some(record) = self.records.get_mut(&handle)
            && !record.destroying
        {
            record.destroying = true;
            self.destroying.push(handle);
            self.running_dirty = true;
        }
    }

    /// Takes over a behaviour that scripts activated themselves (see
    /// [`Request::Adopt`]). It starts at the next flush.
    pub fn adopt(&mut self, registration: Registration, adopted: Adopted) {
        self.register(registration);
        self.pending.pop();
        let handle = registration.handle;
        let record = self.record(handle);
        record.awake = true;
        record.enabled = adopted.enabled;
        let wants = record.wants();
        if adopted.enabled {
            self.to_start.push(handle);
        }
        // OnEnable may have disabled it again, or it never got enabled, or its Obj is off.
        if adopted.enabled != wants {
            self.toggled.push(handle);
        }
        if adopted.destroyed {
            self.destroy(handle);
        }
    }

    /// Runs Awake on these newly registered behaviours, then OnEnable, now
    /// rather than at the next flush. Start waits for the next flush.
    pub fn activate(&mut self, handles: &[BehaviourHandle], host: &mut dyn BehaviourHost) {
        self.pending.retain(|h| !handles.contains(h));
        self.awaken(handles.to_vec(), host);
    }

    pub fn apply(&mut self, request: Request) {
        match request {
            Request::Register(registration) => self.register(registration),
            Request::Adopt(registration, adopted) => self.adopt(registration, adopted),
            Request::SetEnabled(handle, enabled) => self.set_enabled(handle, enabled),
            Request::Destroy(handle) => self.destroy(handle),
        }
    }

    /// Follows the Objs switched off (see [`crate::inactive_objs`]): their
    /// behaviours get OnDisable at the next flush, and those of Objs back on
    /// OnEnable.
    pub fn follow_active(&mut self, inactive: HashSet<Entity>) {
        if inactive == self.inactive {
            return;
        }
        for (handle, record) in &mut self.records {
            let active = !inactive.contains(&record.entity);
            if record.obj_active != active {
                record.obj_active = active;
                if record.awake {
                    self.toggled.push(*handle);
                }
            }
        }
        self.inactive = inactive;
    }

    pub fn entity(&self, handle: BehaviourHandle) -> Option<Entity> {
        self.records.get(&handle).map(|r| r.entity)
    }

    pub fn is_enabled(&self, handle: BehaviourHandle) -> bool {
        self.records.get(&handle).is_some_and(|r| r.enabled)
    }

    /// Whether the behaviour is to be enabled, as its `Enabled` says, though
    /// its OnEnable or OnDisable may wait for the next flush.
    pub fn wants_enabled(&self, handle: BehaviourHandle) -> bool {
        self.records.get(&handle).is_some_and(|r| r.wants_enabled)
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Every behaviour, by execution order, with whether it is enabled and
    /// not on its way out: what a reload of the scripts gives OnDisable and
    /// then OnEnable.
    pub fn live(&self) -> Vec<(BehaviourHandle, bool)> {
        let mut handles: Vec<BehaviourHandle> = self.records.keys().copied().collect();
        sort_by_order(&self.records, &mut handles);
        handles.into_iter().map(|h| (h, self.records.get(&h).is_some_and(|r| r.enabled && !r.destroying))).collect()
    }

    /// A behaviour's class changed, as a reload of the scripts can change
    /// it: it may call other callbacks now, or run at another point in the
    /// order. Where it is in its life does not change.
    pub fn retype(&mut self, handle: BehaviourHandle, callbacks: Callbacks, order: i32) {
        let Some(record) = self.records.get_mut(&handle) else { return };
        record.callbacks = callbacks;
        record.order = order;
        self.running_dirty = true;
    }

    /// Forgets a behaviour at once, with no more callbacks, as when its
    /// class is gone after a reload of the scripts, which have already let
    /// it go.
    pub fn forget(&mut self, handle: BehaviourHandle) {
        if self.records.remove(&handle).is_none() {
            return;
        }
        for list in [&mut self.pending, &mut self.toggled, &mut self.to_start, &mut self.destroying] {
            list.retain(|h| *h != handle);
        }
        self.running_dirty = true;
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Brings every new and toggled behaviour up to date. See the module docs
    /// for the guarantees.
    pub fn flush(&mut self, host: &mut dyn BehaviourHost) {
        loop {
            if !self.pending.is_empty() {
                self.activate_batch(host);
            } else if !self.toggled.is_empty() {
                self.apply_toggles(host);
            } else if !self.to_start.is_empty() {
                self.start_waiting(host);
            } else {
                break;
            }
        }
    }

    /// Runs a per-frame phase on every started, enabled behaviour.
    pub fn run(&mut self, phase: Phase, host: &mut dyn BehaviourHost) {
        debug_assert!(matches!(
            phase,
            Phase::FixedUpdate | Phase::Update | Phase::PoseUpdate | Phase::LateUpdate | Phase::OnUiBuilt
        ));
        self.run_dirty();
        let running = std::mem::take(&mut self.running);
        self.dispatch(host, phase, &running);
        self.running = running;
        self.collect_requests(host);
    }

    /// [`run`](Self::run), but only for the behaviours `keep` keeps, by
    /// their Obj and callbacks.
    pub fn run_where(&mut self, phase: Phase, host: &mut dyn BehaviourHost, keep: &dyn Fn(Entity, Callbacks) -> bool) {
        self.run_dirty();
        let kept = self.filtered(&self.running, |r| keep(r.entity, r.callbacks));
        self.dispatch(host, phase, &kept);
        self.collect_requests(host);
    }

    /// Runs `phase` on `entity`'s running behaviours with `mark` in their
    /// callbacks, in their order.
    pub fn run_marked(&mut self, phase: Phase, entity: Entity, mark: u32, host: &mut dyn BehaviourHost) {
        self.run_where(phase, host, &|of, callbacks| of == entity && callbacks.has(mark));
    }

    /// Whether any of `entity`'s behaviours has `mark` in its callbacks.
    pub fn has_marked(&self, entity: Entity, mark: u32) -> bool {
        self.records.values().any(|r| r.entity == entity && !r.destroying && r.callbacks.has(mark))
    }

    fn run_dirty(&mut self) {
        if self.running_dirty {
            self.running.clear();
            self.running
                .extend(self.records.iter().filter(|(_, r)| r.started && r.enabled && !r.destroying).map(|(h, _)| *h));
            sort_by_order(&self.records, &mut self.running);
            self.running_dirty = false;
        }
    }

    /// Destroys everything queued with [`destroy`](Self::destroy): OnDisable
    /// for enabled behaviours, then OnDestroy for awoken ones, then release.
    pub fn end_of_frame(&mut self, host: &mut dyn BehaviourHost) {
        if self.destroying.is_empty() {
            return;
        }
        let mut batch = std::mem::take(&mut self.destroying);
        sort_by_order(&self.records, &mut batch);

        let disabling = self.filtered(&batch, |r| r.enabled);
        self.dispatch(host, Phase::OnDisable, &disabling);
        let destroying = self.filtered(&batch, |r| r.awake);
        self.dispatch(host, Phase::OnDestroy, &destroying);
        host.release(&batch);

        for handle in &batch {
            self.records.remove(handle);
        }
        let records = &self.records;
        for list in [&mut self.pending, &mut self.toggled, &mut self.to_start] {
            list.retain(|h| records.contains_key(h));
        }
        self.running_dirty = true;
        self.collect_requests(host);
    }

    fn activate_batch(&mut self, host: &mut dyn BehaviourHost) {
        let batch = std::mem::take(&mut self.pending);
        self.awaken(batch, host);
    }

    fn awaken(&mut self, mut batch: Vec<BehaviourHandle>, host: &mut dyn BehaviourHost) {
        sort_by_order(&self.records, &mut batch);

        let awakening = self.filtered(&batch, |r| !r.destroying);
        self.dispatch(host, Phase::Awake, &awakening);
        for handle in &awakening {
            self.record(*handle).awake = true;
        }
        self.collect_requests(host);

        let enabling = self.filtered(&awakening, |r| r.wants() && !r.destroying);
        self.dispatch(host, Phase::OnEnable, &enabling);
        for handle in &enabling {
            self.record(*handle).enabled = true;
        }
        self.to_start.extend_from_slice(&enabling);
        self.collect_requests(host);
    }

    fn apply_toggles(&mut self, host: &mut dyn BehaviourHost) {
        let mut toggled = std::mem::take(&mut self.toggled);
        toggled.sort_unstable();
        toggled.dedup();
        sort_by_order(&self.records, &mut toggled);

        let disabling = self.filtered(&toggled, |r| r.enabled && !r.wants());
        self.dispatch(host, Phase::OnDisable, &disabling);
        for handle in &disabling {
            self.record(*handle).enabled = false;
        }

        let enabling = self.filtered(&toggled, |r| !r.enabled && r.wants() && !r.destroying);
        self.dispatch(host, Phase::OnEnable, &enabling);
        for handle in &enabling {
            self.record(*handle).enabled = true;
        }
        let starting = self.filtered(&enabling, |r| !r.started);
        self.to_start.extend_from_slice(&starting);
        self.running_dirty = true;
        self.collect_requests(host);
    }

    fn start_waiting(&mut self, host: &mut dyn BehaviourHost) {
        let mut waiting = std::mem::take(&mut self.to_start);
        sort_by_order(&self.records, &mut waiting);
        let starting = self.filtered(&waiting, |r| r.enabled && !r.started && !r.destroying);
        self.dispatch(host, Phase::Start, &starting);
        for handle in &starting {
            self.record(*handle).started = true;
        }
        // Every Start in the batch first, so OnUiBuilt finds the others set up.
        self.collect_requests(host);
        let building = self.filtered(&starting, |r| r.enabled && !r.destroying);
        self.dispatch(host, Phase::OnUiBuilt, &building);
        self.running_dirty = true;
        self.collect_requests(host);
    }

    fn dispatch(&mut self, host: &mut dyn BehaviourHost, phase: Phase, handles: &[BehaviourHandle]) {
        self.dispatch.clear();
        let records = &self.records;
        self.dispatch.extend(handles.iter().filter(|h| records.get(h).is_some_and(|r| r.callbacks.contains(phase))));
        if !self.dispatch.is_empty() {
            host.invoke(phase, &self.dispatch);
        }
    }

    fn filtered(&self, handles: &[BehaviourHandle], keep: impl Fn(&Record) -> bool) -> Vec<BehaviourHandle> {
        handles.iter().copied().filter(|h| self.records.get(h).is_some_and(&keep)).collect()
    }

    /// Applies requests scripts made outside a dispatch, such as in an await
    /// continuation.
    pub fn apply_requests(&mut self, host: &mut dyn BehaviourHost) {
        self.collect_requests(host);
    }

    fn collect_requests(&mut self, host: &mut dyn BehaviourHost) {
        host.drain_requests(&mut self.requests);
        for request in std::mem::take(&mut self.requests) {
            self.apply(request);
        }
    }

    fn record(&mut self, handle: BehaviourHandle) -> &mut Record {
        self.records.get_mut(&handle).expect("behaviour is registered")
    }
}

fn sort_by_order(records: &HashMap<BehaviourHandle, Record>, handles: &mut [BehaviourHandle]) {
    handles.sort_by_key(|h| records.get(h).map_or((i32::MAX, u64::MAX), |r| (r.order, r.seq)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Recorder {
        log: Vec<(Phase, Vec<u32>)>,
        spawn_on_awake: HashMap<u32, Vec<Registration>>,
        requests: Vec<Request>,
        released: Vec<u32>,
    }

    impl BehaviourHost for Recorder {
        fn invoke(&mut self, phase: Phase, behaviours: &[BehaviourHandle]) {
            self.log.push((phase, behaviours.iter().map(|h| h.0).collect()));
            if phase == Phase::Awake {
                for handle in behaviours {
                    let spawned = self.spawn_on_awake.remove(&handle.0).unwrap_or_default();
                    self.requests.extend(spawned.into_iter().map(Request::Register));
                }
            }
        }

        fn release(&mut self, behaviours: &[BehaviourHandle]) {
            self.released.extend(behaviours.iter().map(|h| h.0));
        }

        fn drain_requests(&mut self, out: &mut Vec<Request>) {
            out.append(&mut self.requests);
        }
    }

    /// A behaviour with every callback but OnUiBuilt, which has a test of its own.
    fn reg(handle: u32, order: i32) -> Registration {
        Registration {
            handle: BehaviourHandle(handle),
            entity: Entity::NULL,
            callbacks: Callbacks(Callbacks::ALL.0 & !Callbacks::of(Phase::OnUiBuilt).0),
            order,
            enabled: true,
        }
    }

    #[test]
    fn on_ui_built_follows_every_start_in_the_batch() {
        let mut lifecycle = Lifecycle::new();
        let mut host = Recorder::default();
        lifecycle.register(Registration { callbacks: Callbacks::ALL, ..reg(0, 0) });
        lifecycle.register(Registration { callbacks: Callbacks::of(Phase::Start), ..reg(1, 0) });
        lifecycle.register(Registration { callbacks: Callbacks::of(Phase::OnUiBuilt), ..reg(2, 0) });
        lifecycle.flush(&mut host);
        assert_eq!(
            host.log,
            vec![
                (Phase::Awake, vec![0]),
                (Phase::OnEnable, vec![0]),
                (Phase::Start, vec![0, 1]),
                (Phase::OnUiBuilt, vec![0, 2]),
            ]
        );
        host.log.clear();
        lifecycle.run(Phase::OnUiBuilt, &mut host);
        assert_eq!(host.log, vec![(Phase::OnUiBuilt, vec![0, 2])], "and at each frame's start");
    }

    #[test]
    fn behaviours_on_an_obj_switched_off_are_disabled_until_it_comes_back() {
        let mut lifecycle = Lifecycle::new();
        let mut host = Recorder::default();
        let (on, off) = (Entity::from_bits(1 << 32), Entity::from_bits((1 << 32) | 1));
        lifecycle.register(Registration { entity: on, ..reg(0, 0) });
        lifecycle.register(Registration { entity: off, ..reg(1, 0) });
        lifecycle.follow_active(HashSet::from([off]));
        lifecycle.flush(&mut host);
        assert_eq!(host.log[1], (Phase::OnEnable, vec![0]));
        host.log.clear();
        lifecycle.run(Phase::Update, &mut host);
        assert_eq!(host.log, vec![(Phase::Update, vec![0])]);

        host.log.clear();
        lifecycle.follow_active(HashSet::from([on]));
        lifecycle.flush(&mut host);
        assert_eq!(host.log, vec![(Phase::OnDisable, vec![0]), (Phase::OnEnable, vec![1]), (Phase::Start, vec![1])]);
    }

    #[test]
    fn batch_runs_each_phase_on_everyone_before_the_next() {
        let mut lifecycle = Lifecycle::new();
        let mut host = Recorder::default();
        for i in 0..3 {
            lifecycle.register(reg(i, 0));
        }
        lifecycle.flush(&mut host);
        assert_eq!(
            host.log,
            vec![(Phase::Awake, vec![0, 1, 2]), (Phase::OnEnable, vec![0, 1, 2]), (Phase::Start, vec![0, 1, 2])]
        );
    }

    #[test]
    fn a_reload_retypes_and_forgets_behaviours_where_they_are_in_their_lives() {
        let mut lifecycle = Lifecycle::new();
        let mut host = Recorder::default();
        for i in 0..3 {
            lifecycle.register(reg(i, i as i32));
        }
        lifecycle.flush(&mut host);
        lifecycle.set_enabled(BehaviourHandle(1), false);
        lifecycle.flush(&mut host);
        let live = lifecycle.live();
        assert_eq!(live, [(BehaviourHandle(0), true), (BehaviourHandle(1), false), (BehaviourHandle(2), true)]);

        // The new build: 0 now runs last and has no Update, and 2's class is gone.
        lifecycle.retype(BehaviourHandle(0), Callbacks(Callbacks::ALL.0 & !(1 << Phase::Update as u32)), 10);
        lifecycle.forget(BehaviourHandle(2));
        lifecycle.set_enabled(BehaviourHandle(1), true);
        host.log.clear();
        lifecycle.flush(&mut host);
        lifecycle.run(Phase::Update, &mut host);
        lifecycle.run(Phase::LateUpdate, &mut host);
        assert_eq!(
            host.log,
            vec![(Phase::OnEnable, vec![1]), (Phase::Update, vec![1]), (Phase::LateUpdate, vec![1, 0])],
            "no Awake or Start again, 0 in its new place and without Update, and 2 gone"
        );
        assert_eq!(lifecycle.len(), 2);
    }

    #[test]
    fn execution_order_beats_creation_order() {
        let mut lifecycle = Lifecycle::new();
        let mut host = Recorder::default();
        lifecycle.register(reg(0, 10));
        lifecycle.register(reg(1, -5));
        lifecycle.register(reg(2, 0));
        lifecycle.flush(&mut host);
        lifecycle.run(Phase::Update, &mut host);
        assert_eq!(host.log[0], (Phase::Awake, vec![1, 2, 0]));
        assert_eq!(host.log.last().unwrap(), &(Phase::Update, vec![1, 2, 0]));
    }

    #[test]
    fn objects_created_during_awake_finish_initialising_before_any_start() {
        let mut lifecycle = Lifecycle::new();
        let mut host = Recorder::default();
        host.spawn_on_awake.insert(0, vec![reg(10, 0), reg(11, 0)]);
        lifecycle.register(reg(0, 0));
        lifecycle.register(reg(1, 0));
        lifecycle.flush(&mut host);
        assert_eq!(
            host.log,
            vec![
                (Phase::Awake, vec![0, 1]),
                (Phase::OnEnable, vec![0, 1]),
                (Phase::Awake, vec![10, 11]),
                (Phase::OnEnable, vec![10, 11]),
                (Phase::Start, vec![0, 1, 10, 11]),
            ]
        );
    }

    #[test]
    fn disabled_behaviours_awake_but_wait_to_enable_and_start() {
        let mut lifecycle = Lifecycle::new();
        let mut host = Recorder::default();
        lifecycle.register(Registration { enabled: false, ..reg(0, 0) });
        lifecycle.register(reg(1, 0));
        lifecycle.flush(&mut host);
        assert_eq!(host.log, vec![(Phase::Awake, vec![0, 1]), (Phase::OnEnable, vec![1]), (Phase::Start, vec![1])]);

        host.log.clear();
        lifecycle.set_enabled(BehaviourHandle(0), true);
        lifecycle.set_enabled(BehaviourHandle(1), false);
        lifecycle.flush(&mut host);
        lifecycle.run(Phase::Update, &mut host);
        assert_eq!(
            host.log,
            vec![
                (Phase::OnDisable, vec![1]),
                (Phase::OnEnable, vec![0]),
                (Phase::Start, vec![0]),
                (Phase::Update, vec![0]),
            ]
        );
    }

    #[test]
    fn destroy_is_deferred_and_skips_phases_never_reached() {
        let mut lifecycle = Lifecycle::new();
        let mut host = Recorder::default();
        lifecycle.register(reg(0, 0));
        lifecycle.register(Registration { enabled: false, ..reg(1, 0) });
        lifecycle.flush(&mut host);
        lifecycle.register(reg(2, 0));

        host.log.clear();
        for i in 0..3 {
            lifecycle.destroy(BehaviourHandle(i));
        }
        lifecycle.run(Phase::Update, &mut host);
        lifecycle.end_of_frame(&mut host);
        assert_eq!(host.log, vec![(Phase::OnDisable, vec![0]), (Phase::OnDestroy, vec![0, 1])]);
        assert_eq!(host.released, vec![0, 1, 2]);
        assert!(lifecycle.is_empty());
    }

    #[test]
    fn activate_runs_awake_and_on_enable_now_and_start_later() {
        let mut lifecycle = Lifecycle::new();
        let mut host = Recorder::default();
        lifecycle.register(reg(0, 0));
        lifecycle.register(reg(1, 5));
        lifecycle.register(reg(2, 0));
        lifecycle.activate(&[BehaviourHandle(1), BehaviourHandle(2)], &mut host);
        assert_eq!(host.log, vec![(Phase::Awake, vec![2, 1]), (Phase::OnEnable, vec![2, 1])]);

        host.log.clear();
        lifecycle.flush(&mut host);
        assert_eq!(
            host.log,
            vec![(Phase::Awake, vec![0]), (Phase::OnEnable, vec![0]), (Phase::Start, vec![0, 2, 1])],
            "the rest of the pending batch waits for the flush, and Start waits for everyone"
        );
    }

    #[test]
    fn adopted_behaviours_skip_to_start() {
        let mut lifecycle = Lifecycle::new();
        let mut host = Recorder::default();
        let enabled = Adopted { enabled: true, destroyed: false };
        host.requests.push(Request::Adopt(reg(0, 0), enabled));
        // Disabled in its own Awake, so OnEnable never ran.
        host.requests
            .push(Request::Adopt(Registration { enabled: false, ..reg(1, 0) }, Adopted { enabled: false, ..enabled }));
        // Disabled again in its own OnEnable.
        host.requests.push(Request::Adopt(Registration { enabled: false, ..reg(2, 0) }, enabled));
        host.requests.push(Request::Adopt(reg(3, 0), Adopted { destroyed: true, ..enabled }));
        lifecycle.run(Phase::Update, &mut host);
        lifecycle.flush(&mut host);
        lifecycle.run(Phase::Update, &mut host);
        lifecycle.end_of_frame(&mut host);
        assert_eq!(
            host.log,
            vec![
                (Phase::OnDisable, vec![2]),
                (Phase::Start, vec![0]),
                (Phase::Update, vec![0]),
                (Phase::OnDisable, vec![3]),
                (Phase::OnDestroy, vec![3]),
            ]
        );
        assert!(!lifecycle.is_enabled(BehaviourHandle(1)) && !lifecycle.is_enabled(BehaviourHandle(2)));
    }

    #[test]
    fn phases_a_type_does_not_override_are_skipped() {
        let mut lifecycle = Lifecycle::new();
        let mut host = Recorder::default();
        lifecycle.register(Registration { callbacks: Callbacks::of(Phase::Update), ..reg(0, 0) });
        lifecycle.register(reg(1, 0));
        lifecycle.flush(&mut host);
        lifecycle.run(Phase::Update, &mut host);
        assert_eq!(
            host.log,
            vec![
                (Phase::Awake, vec![1]),
                (Phase::OnEnable, vec![1]),
                (Phase::Start, vec![1]),
                (Phase::Update, vec![0, 1]),
            ]
        );
    }
}
