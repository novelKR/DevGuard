//! Observed process-group scopes: cooperative CPU policy application with
//! readback, identity-checked membership tracking and termination. The logic
//! runs over `ProcessTable`, so the native table and scripted tests share it.
//!
//! A scope is the process group led by its root. Cooperative workloads are
//! assumed to stay in that group; a known member or child observed outside it
//! is a sticky escape, and any incomplete observation is sticky tracking loss.
//! Neither is ever cleared by a later ordinary observation.
//!
//! A group ID is trusted only while it provably still names this scope's
//! group: the root still holds its PID, or a known member is seen in the group
//! in the same pass. Once the group is confirmed empty with its root reaped,
//! its ID may be reused by an unrelated group and is never listed again.

use crate::process::Presence;
use devguard_contract::*;
use devguard_core::{BindingEvidence, Clock, ScopeObservation};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

/// Workload `nice` value applied by the authority before any payload runs.
pub const WORKLOAD_NICE: i32 = 10;
/// Highest scheduler priority of a utility-clamped task (XNU BASEPRI_UTILITY).
pub const UTILITY_PRIORITY_CEILING: i32 = 20;

/// Scheduler state read back from the kernel for one process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuReadback {
    pub nice: i32,
    /// `pti_priority`: the task's base priority. Without the clamp it reads
    /// 31 minus nice, so it alone cannot distinguish a clamp from a high nice.
    pub task_priority: i32,
    /// Highest `pth_maxpriority` over the threads that could be read: 20 under
    /// the utility clamp, 63 without it.
    pub max_thread_priority: i32,
    pub threads: u32,
}

impl CpuReadback {
    /// Utility QoS and nice +10 are both in effect for every observed thread.
    pub fn cooperative(&self) -> bool {
        self.threads > 0
            && self.nice >= WORKLOAD_NICE
            && self.task_priority <= UTILITY_PRIORITY_CEILING
            && self.max_thread_priority <= UTILITY_PRIORITY_CEILING
    }
}

/// Scheduler priorities of a process, without its `nice` value.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Priorities {
    pub task_priority: i32,
    pub max_thread_priority: i32,
    pub threads: u32,
}

/// The kernel operations a scope needs. Errors mean the observation or
/// operation failed, never that a process is absent.
pub(crate) trait ProcessTable: Send + Sync {
    fn presence(&self, pid: u32) -> Result<Presence>;
    fn group_members(&self, pgid: u32) -> Result<Vec<u32>>;
    fn children(&self, pid: u32) -> Result<Vec<u32>>;
    fn priorities(&self, pid: u32) -> Result<Priorities>;
    fn renice(&self, pid: u32, nice: i32) -> Result<()>;
    fn signal(&self, pid: u32, signal: i32) -> Result<()>;
    fn effective_uid(&self) -> u32;
    fn own_pid(&self) -> u32;
}

/// The result of establishing a scope, retained as application evidence.
#[derive(Debug, Clone, Serialize)]
pub struct Establishment {
    pub scope: ScopeIdentity,
    pub applied: AppliedResources,
    pub cpu: Option<CpuReadback>,
}

/// Processes that received a signal after their identity was rechecked.
/// `complete` is false when a read or delivery failed or the group could not
/// be proven to be the scope's; some targets may then remain unsignalled.
#[derive(Debug, Clone, Serialize)]
pub struct SignalReceipt {
    pub scope: ScopeIdentity,
    pub signal: i32,
    pub signalled: Vec<(u32, u64)>,
    pub complete: bool,
    pub observed_at: ObservationTime,
}

type Identity = (u32, u64);

/// One classified group listing.
#[derive(Default)]
struct Listing {
    present: usize,
    verified_known: bool,
    newcomers: Vec<Identity>,
}

/// A group listing bracketed by root checks: how many PIDs were listed, how
/// many still hold the group, and the root's state after the listing.
struct GroupListing {
    listed: usize,
    present: usize,
    held_after: bool,
    reaped_after: bool,
}

struct Tracked {
    key: AttemptKey,
    owner: InstanceIdentity,
    identity: ScopeIdentity,
    plan: ExecutionPlan,
    quantities: Budget,
    /// Member identities not yet confirmed gone. Gone identities are pruned:
    /// a (PID, start) pair can never return.
    known: BTreeSet<Identity>,
    /// Known identities observed outside the group, kept as signal targets.
    escaped: BTreeSet<Identity>,
    escape_seen: bool,
    tracking_lost: bool,
    /// The group was confirmed empty with its root reaped; its ID may since
    /// name an unrelated group and is never listed again.
    group_ended: bool,
}

pub(crate) struct ScopeTracker<T: ProcessTable> {
    table: T,
    scopes: Mutex<BTreeMap<String, Arc<Mutex<Tracked>>>>,
}

fn unsupported(message: &'static str) -> Error {
    Error::new(ErrorCode::ResourcePolicyUnsupported, message)
}

fn unavailable(message: &'static str) -> Error {
    Error::new(ErrorCode::ResourceControlUnavailable, message)
}

fn unknown_scope() -> Error {
    Error::new(
        ErrorCode::ReconciliationRequired,
        "scope is not tracked by this authority",
    )
}

fn poisoned() -> Error {
    unavailable("scope registry is unavailable")
}

impl<T: ProcessTable> ScopeTracker<T> {
    pub fn new(table: T) -> Self {
        Self {
            table,
            scopes: Mutex::new(BTreeMap::new()),
        }
    }

    /// Whether `pid` still names the process that started at `start`.
    fn same_process(&self, (pid, start): Identity) -> Result<bool> {
        Ok(match self.table.presence(pid)? {
            Presence::Live(snapshot) => snapshot.start_ticks == start,
            Presence::Exited { start_ticks } => start_ticks == start,
            Presence::Absent => false,
        })
    }

    /// Find a tracked scope without reading any clock or process state.
    fn tracked(&self, scope: &ScopeIdentity) -> Result<Arc<Mutex<Tracked>>> {
        let tracked = self
            .scopes
            .lock()
            .map_err(|_| poisoned())?
            .get(&scope.scope_id)
            .cloned()
            .ok_or_else(unknown_scope)?;
        if tracked.lock().map_err(|_| poisoned())?.identity != *scope {
            return Err(unknown_scope());
        }
        Ok(tracked)
    }

    /// The identity of a process presenting a launch grant. It must be a live
    /// process of this user whose parent is the attempt owner, and the owner
    /// must still be running as the registered identity: only the owner can
    /// have created the helper. The helper is read again after the owner
    /// check, so a PID reused in between yields no identity.
    pub fn helper(
        &self,
        pid: u32,
        owner: &ProcessIdentity,
        clock: &impl Clock,
    ) -> Result<ProcessIdentity> {
        let boot_id = clock.now().boot_id;
        let refused = |message: &'static str| Error::new(ErrorCode::Unauthorized, message);
        if pid == 0 || pid == self.table.own_pid() {
            return Err(refused("the authority cannot be a launch helper"));
        }
        if owner.boot_id != boot_id {
            return Err(refused("the attempt owner is from another boot"));
        }
        let snapshot = match self.table.presence(pid)? {
            Presence::Live(snapshot) => snapshot,
            _ => return Err(unavailable("launch helper is not running")),
        };
        if snapshot.uid != self.table.effective_uid() {
            return Err(refused("launch helper belongs to another user"));
        }
        let owner_running = matches!(
            self.table.presence(owner.pid)?,
            Presence::Live(current) if current.start_ticks == owner.start_ticks
        );
        if snapshot.ppid != owner.pid || !owner_running {
            return Err(refused(
                "launch helper was not created by the running attempt owner",
            ));
        }
        match self.table.presence(pid)? {
            Presence::Live(current)
                if current.start_ticks == snapshot.start_ticks && current.ppid == owner.pid => {}
            _ => return Err(unavailable("launch helper changed during observation")),
        }
        Ok(ProcessIdentity {
            boot_id,
            pid,
            start_ticks: snapshot.start_ticks,
        })
    }

    /// Establish the observed scope of a started root before any payload runs.
    /// The root must be alive, lead its own process group alone and belong to
    /// this user. The authority applies nice; the root must already carry the
    /// utility QoS clamp. Failed application is recorded, not hidden: the scope
    /// stays tracked so it can be terminated, and binding will refuse it.
    pub fn establish(
        &self,
        key: &AttemptKey,
        owner: &InstanceIdentity,
        root: &ProcessIdentity,
        plan: &ExecutionPlan,
        quantities: Budget,
        clock: &impl Clock,
    ) -> Result<Establishment> {
        key.validate()?;
        plan.validate()?;
        if plan.scope_kind != ScopeKind::ObservedProcessGroup {
            return Err(unsupported("macOS scopes are observed process groups"));
        }
        if root.boot_id != clock.now().boot_id {
            return Err(unsupported("scope root identity is from another boot"));
        }
        if root.pid == self.table.own_pid() {
            return Err(unsupported("the authority cannot be its own scope root"));
        }
        let snapshot = match self.table.presence(root.pid)? {
            Presence::Live(snapshot) if snapshot.start_ticks == root.start_ticks => snapshot,
            Presence::Live(_) => return Err(unsupported("scope root identity changed")),
            _ => return Err(unavailable("scope root is not running")),
        };
        if snapshot.uid != self.table.effective_uid() {
            return Err(Error::new(
                ErrorCode::Unauthorized,
                "scope root belongs to another user",
            ));
        }
        if snapshot.pgid != root.pid {
            return Err(unsupported("scope root must lead its own process group"));
        }
        if self.table.group_members(root.pid)? != [root.pid] {
            return Err(unsupported(
                "a scope must contain only its root before the payload starts",
            ));
        }
        let identity = ScopeIdentity {
            kind: ScopeKind::ObservedProcessGroup,
            scope_id: format!("pg-{}-{}", root.pid, root.start_ticks),
            root: root.clone(),
        };
        let inserted = {
            let mut scopes = self.scopes.lock().map_err(|_| poisoned())?;
            if let Some(existing) = scopes.get(&identity.scope_id) {
                let existing = existing.lock().map_err(|_| poisoned())?;
                if existing.key != *key
                    || existing.owner != *owner
                    || existing.plan != *plan
                    || existing.quantities != quantities
                {
                    return Err(Error::new(
                        ErrorCode::AttemptConflict,
                        "scope root is already bound to another attempt",
                    ));
                }
                false
            } else {
                scopes.insert(
                    identity.scope_id.clone(),
                    Arc::new(Mutex::new(Tracked {
                        key: key.clone(),
                        owner: owner.clone(),
                        identity: identity.clone(),
                        plan: plan.clone(),
                        quantities,
                        known: BTreeSet::from([(root.pid, root.start_ticks)]),
                        escaped: BTreeSet::new(),
                        escape_seen: false,
                        tracking_lost: false,
                        group_ended: false,
                    })),
                );
                true
            }
        };
        // Never lower an already higher nice value, and recheck the identity
        // immediately before changing it. A failure leaves the CPU policy
        // unapplied, which the readback below reports.
        if snapshot.nice < WORKLOAD_NICE
            && matches!(
                self.table.presence(root.pid),
                Ok(Presence::Live(current)) if current.start_ticks == root.start_ticks
            )
        {
            let _ = self.table.renice(root.pid, WORKLOAD_NICE);
        }
        match self.read_back(&identity, plan, quantities, clock) {
            Ok((applied, cpu)) => Ok(Establishment {
                scope: identity,
                applied,
                cpu,
            }),
            Err(error) => {
                // The root is gone before any readback: nothing was applied
                // and nothing is left to terminate.
                if inserted {
                    self.forget(&identity)?;
                }
                Err(error)
            }
        }
    }

    /// Stop tracking a scope that no attempt claimed, for example because the
    /// grant was refused after establishment. A claimed scope must instead stay
    /// tracked until its termination is observed.
    pub fn forget(&self, scope: &ScopeIdentity) -> Result<()> {
        let mut scopes = self.scopes.lock().map_err(|_| poisoned())?;
        if scopes
            .get(&scope.scope_id)
            .is_some_and(|tracked| tracked.lock().is_ok_and(|t| t.identity == *scope))
        {
            scopes.remove(&scope.scope_id);
        }
        Ok(())
    }

    fn read_back(
        &self,
        scope: &ScopeIdentity,
        plan: &ExecutionPlan,
        quantities: Budget,
        clock: &impl Clock,
    ) -> Result<(AppliedResources, Option<CpuReadback>)> {
        let root = &scope.root;
        let live = |presence: Result<Presence>| match presence? {
            Presence::Live(snapshot) if snapshot.start_ticks == root.start_ticks => Ok(snapshot),
            _ => Err(unavailable("scope root is no longer running")),
        };
        live(self.table.presence(root.pid))?;
        let priorities = self.table.priorities(root.pid);
        // The priorities belong to the root only if its identity is unchanged
        // afterwards; this snapshot also supplies the current nice and group.
        let snapshot = live(self.table.presence(root.pid))?;
        let cpu = priorities.ok().map(|priorities| CpuReadback {
            nice: snapshot.nice,
            task_priority: priorities.task_priority,
            max_thread_priority: priorities.max_thread_priority,
            threads: priorities.threads,
        });
        let state = |resource: &PlannedResource| match resource.method {
            // The authority itself accounts the reservation.
            ControlMethod::Accounting => ApplicationState::Applied,
            ControlMethod::QosAndPriority => match cpu {
                Some(cpu) if snapshot.pgid == root.pid && cpu.cooperative() => {
                    ApplicationState::Applied
                }
                _ => ApplicationState::Failed,
            },
            ControlMethod::CgroupV2 => ApplicationState::Unsupported,
        };
        Ok((
            AppliedResources {
                scope: scope.clone(),
                plan: plan.clone(),
                quantities,
                cpu: state(&plan.cpu),
                memory: state(&plan.memory),
                pids: state(&plan.pids),
                observed_at: clock.now(),
            },
            cpu,
        ))
    }

    /// Fresh application evidence for a tracked scope.
    pub fn binding(&self, scope: &ScopeIdentity, clock: &impl Clock) -> Result<BindingEvidence> {
        let (key, owner, plan, quantities) = {
            let tracked = self.tracked(scope)?;
            let tracked = tracked.lock().map_err(|_| poisoned())?;
            (
                tracked.key.clone(),
                tracked.owner.clone(),
                tracked.plan.clone(),
                tracked.quantities,
            )
        };
        let (applied, _) = self.read_back(scope, &plan, quantities, clock)?;
        Ok(BindingEvidence {
            key,
            owner,
            applied,
        })
    }

    /// Classify one group listing without adopting anything yet.
    fn classify_listing(
        &self,
        tracked: &Tracked,
        members: Vec<u32>,
        complete: &mut bool,
    ) -> Listing {
        let pgid = tracked.identity.root.pid;
        let mut listing = Listing::default();
        for pid in members {
            match self.table.presence(pid) {
                Ok(Presence::Live(snapshot)) if snapshot.pgid == pgid => {
                    listing.present += 1;
                    let identity = (pid, snapshot.start_ticks);
                    if tracked.known.contains(&identity) {
                        listing.verified_known = true;
                    } else {
                        listing.newcomers.push(identity);
                    }
                }
                // An unreaped member still holds the group.
                Ok(Presence::Exited { start_ticks }) => {
                    listing.present += 1;
                    listing.verified_known |= tracked.known.contains(&(pid, start_ticks));
                }
                // Listed in the group but now elsewhere: a known identity is
                // classified by the known-member pass; an unknown one cannot be
                // told apart from a reused PID.
                Ok(Presence::Live(snapshot)) => {
                    if !tracked.known.contains(&(pid, snapshot.start_ticks)) {
                        *complete = false;
                    }
                }
                Ok(Presence::Absent) => {}
                Err(_) => *complete = false,
            }
        }
        listing
    }

    /// Where the root stands now: (holds its PID, reaped). An observation
    /// error proves neither and clears `complete`.
    fn root_state(&self, root: Identity, complete: &mut bool) -> (bool, bool) {
        match self.table.presence(root.0) {
            Ok(Presence::Live(snapshot)) => {
                let same = snapshot.start_ticks == root.1;
                (same, !same)
            }
            Ok(Presence::Exited { start_ticks }) => {
                let same = start_ticks == root.1;
                (same, !same)
            }
            Ok(Presence::Absent) => (false, true),
            Err(_) => {
                *complete = false;
                (false, false)
            }
        }
    }

    /// List the group, bracketed by root checks. Newcomers are adopted only if
    /// the group was proven to be this scope's across the whole listing: the
    /// root held its PID before and after it, or a known member was listed.
    /// Otherwise they are tracking loss.
    fn list_group(
        &self,
        tracked: &mut Tracked,
        root_held_before: bool,
        complete: &mut bool,
    ) -> GroupListing {
        let root = (tracked.identity.root.pid, tracked.identity.root.start_ticks);
        let members = self.table.group_members(root.0);
        let (held_after, reaped_after) = self.root_state(root, complete);
        let Ok(members) = members else {
            *complete = false;
            return GroupListing {
                listed: 0,
                present: 0,
                held_after,
                reaped_after,
            };
        };
        let listed = members.len();
        let listing = self.classify_listing(tracked, members, complete);
        if (root_held_before && held_after) || listing.verified_known {
            tracked.known.extend(listing.newcomers);
        } else if !listing.newcomers.is_empty() {
            *complete = false;
        }
        GroupListing {
            listed,
            present: listing.present,
            held_after,
            reaped_after,
        }
    }

    /// Observe membership. Release evidence needs a reaped root, a group
    /// confirmed empty twice, every known identity gone, complete tracking
    /// and no escape, ever. The observation is timestamped when it completes.
    pub fn observe(&self, scope: &ScopeIdentity, clock: &impl Clock) -> Result<ScopeObservation> {
        let tracked = self.tracked(scope)?;
        let mut tracked = tracked.lock().map_err(|_| poisoned())?;
        let root = (scope.root.pid, scope.root.start_ticks);
        let pgid = scope.root.pid;
        let mut complete = true;
        // While the root holds its PID, no other group can use this ID.
        let (mut root_held, mut root_reaped) = self.root_state(root, &mut complete);
        let mut present = 0;
        if !tracked.group_ended {
            let listing = self.list_group(&mut tracked, root_held, &mut complete);
            present = listing.present;
            root_held &= listing.held_after;
            root_reaped = listing.reaped_after;
        }
        // Every known identity is still here, gone for good, or escaped.
        let mut live = Vec::new();
        let mut zombies = 0usize;
        let mut gone = Vec::new();
        let mut escaped = Vec::new();
        for &identity in tracked.known.iter() {
            match self.table.presence(identity.0) {
                Ok(Presence::Live(snapshot)) if snapshot.start_ticks == identity.1 => {
                    if snapshot.pgid != pgid {
                        escaped.push(identity);
                    }
                    live.push(identity);
                }
                Ok(Presence::Exited { start_ticks }) if start_ticks == identity.1 => zombies += 1,
                Ok(_) => gone.push(identity),
                Err(_) => complete = false,
            }
        }
        // A child of a live known member outside the group has escaped even
        // if it was never seen as a member. Parentage counts only if the parent
        // is unchanged after its children were listed; a child outside the
        // group whose parentage cannot be verified is tracking loss.
        for &(parent, parent_start) in &live {
            let children = match self.table.children(parent) {
                Ok(children) => children,
                Err(_) => {
                    complete = false;
                    continue;
                }
            };
            let parent_verified = self
                .same_process((parent, parent_start))
                .unwrap_or_else(|_| {
                    complete = false;
                    false
                });
            for child in children {
                match self.table.presence(child) {
                    Ok(Presence::Live(snapshot)) if parent_verified && snapshot.ppid == parent => {
                        let identity = (child, snapshot.start_ticks);
                        tracked.known.insert(identity);
                        if snapshot.pgid != pgid {
                            escaped.push(identity);
                        }
                    }
                    Ok(Presence::Live(snapshot)) => {
                        if snapshot.pgid != pgid {
                            complete = false;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => complete = false,
                }
            }
        }
        for identity in &gone {
            tracked.known.remove(identity);
            tracked.escaped.remove(identity);
        }
        if !escaped.is_empty() {
            tracked.escape_seen = true;
            tracked.escaped.extend(escaped);
        }
        // Confirm emptiness with a second listing so a member created during
        // this observation cannot be missed. A group that already ended is not
        // listed: its ID may name an unrelated group.
        let mut empty = complete && present == 0;
        if empty && !tracked.group_ended {
            // The confirming listing must be literally empty: a member listed
            // and reaped before its read may have forked one not listed yet.
            let confirming = self.list_group(&mut tracked, root_held, &mut complete);
            root_reaped = confirming.reaped_after;
            empty = complete && confirming.listed == 0;
            if empty && root_reaped {
                tracked.group_ended = true;
            }
        }
        if !complete {
            tracked.tracking_lost = true;
        }
        Ok(ScopeObservation {
            scope: scope.clone(),
            observed_at: clock.now(),
            root_reaped,
            empty,
            known_members_gone: complete && live.is_empty() && zombies == 0,
            tracking_complete: !tracked.tracking_lost,
            known_escape: tracked.escape_seen,
            prior_tracking_loss_resolved: false,
        })
    }

    /// Signal known identities, known escapees and, while the group provably
    /// is still this scope's, its current members, each rechecked immediately
    /// before its signal. An unprovable group or a failed read or delivery
    /// does not stop the verified identities from being signalled; it makes
    /// the receipt incomplete. Never signals a stale PID or a process group,
    /// and never implies release. A PID can still be reused between the
    /// recheck and the signal; macOS offers no process handle to close that.
    pub fn signal(
        &self,
        scope: &ScopeIdentity,
        signal: i32,
        clock: &impl Clock,
    ) -> Result<SignalReceipt> {
        if signal <= 0 {
            return Err(Error::new(
                ErrorCode::InvalidRequest,
                "a positive signal number is required",
            ));
        }
        let tracked = self.tracked(scope)?;
        let tracked = tracked.lock().map_err(|_| poisoned())?;
        let pgid = scope.root.pid;
        let root = (pgid, scope.root.start_ticks);
        let mut complete = true;
        let mut targets: BTreeSet<Identity> =
            tracked.known.union(&tracked.escaped).copied().collect();
        if !tracked.group_ended {
            let (held_before, _) = self.root_state(root, &mut complete);
            match self.table.group_members(pgid) {
                Ok(members) => {
                    let (held_after, _) = self.root_state(root, &mut complete);
                    let listing = self.classify_listing(&tracked, members, &mut complete);
                    if (held_before && held_after) || listing.verified_known {
                        targets.extend(listing.newcomers);
                    } else if !listing.newcomers.is_empty() {
                        // Members of an unproven group are not signalled.
                        complete = false;
                    }
                }
                Err(_) => complete = false,
            }
        }
        let mut signalled = Vec::new();
        for identity in targets {
            match self.table.presence(identity.0) {
                Ok(Presence::Live(snapshot)) if snapshot.start_ticks == identity.1 => {
                    if self.table.signal(identity.0, signal).is_ok() {
                        signalled.push(identity);
                    } else {
                        complete = false;
                    }
                }
                Ok(_) => {}
                Err(_) => complete = false,
            }
        }
        Ok(SignalReceipt {
            scope: scope.clone(),
            signal,
            signalled,
            complete,
            observed_at: clock.now(),
        })
    }

    /// Scheduler readback for any live process, for diagnostics and evidence.
    pub fn scheduler(&self, pid: u32) -> Result<CpuReadback> {
        let before = match self.table.presence(pid)? {
            Presence::Live(snapshot) => snapshot,
            _ => return Err(unavailable("process is not running")),
        };
        let priorities = self.table.priorities(pid)?;
        match self.table.presence(pid)? {
            Presence::Live(after) if after.start_ticks == before.start_ticks => Ok(CpuReadback {
                nice: after.nice,
                task_priority: priorities.task_priority,
                max_thread_priority: priorities.max_thread_priority,
                threads: priorities.threads,
            }),
            _ => Err(unavailable("process changed during readback")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::Snapshot;
    use devguard_core::Backend as _;
    use std::collections::VecDeque;

    const ROOT: u32 = 100;
    const UID: u32 = 501;

    #[derive(Clone)]
    struct Proc {
        ppid: u32,
        pgid: u32,
        uid: u32,
        nice: i32,
        start: u64,
        zombie: bool,
        task_priority: i32,
        max_thread_priority: i32,
    }

    type Edit = fn(&mut State);

    #[derive(Default)]
    struct State {
        procs: BTreeMap<u32, Proc>,
        fail_listing: bool,
        fail_presence: BTreeSet<u32>,
        /// Members that appear only on the next listing (a creation race).
        appear_on_next_listing: Vec<(u32, Proc)>,
        /// Mutations applied right after a priorities, children or group
        /// read, simulating PID reuse between two checks.
        after_priorities: Option<Edit>,
        after_children: Option<Edit>,
        after_listing: VecDeque<Edit>,
        /// A mutation applied right after the first presence read of a PID.
        after_presence_of: Option<(u32, Edit)>,
        /// Listings left before every further listing fails.
        listings_before_failure: Option<u32>,
        fail_children: bool,
        fail_priorities: bool,
        failing_signals: BTreeSet<u32>,
        signals: Vec<(u32, i32)>,
        renices: u32,
        renice_denied: bool,
    }

    #[derive(Default)]
    struct Scripted(Mutex<State>);

    impl Scripted {
        fn with(&self, edit: impl FnOnce(&mut State)) {
            edit(&mut self.0.lock().unwrap());
        }
    }

    impl ProcessTable for Scripted {
        fn presence(&self, pid: u32) -> Result<Presence> {
            let mut state = self.0.lock().unwrap();
            if state.fail_presence.contains(&pid) {
                return Err(unavailable("scripted refusal"));
            }
            let presence = match state.procs.get(&pid) {
                Some(p) if p.zombie => Presence::Exited {
                    start_ticks: p.start,
                },
                Some(p) => Presence::Live(Snapshot {
                    ppid: p.ppid,
                    pgid: p.pgid,
                    uid: p.uid,
                    nice: p.nice,
                    start_ticks: p.start,
                }),
                None => Presence::Absent,
            };
            if state
                .after_presence_of
                .is_some_and(|(target, _)| target == pid)
            {
                let (_, edit) = state.after_presence_of.take().unwrap();
                edit(&mut state);
            }
            Ok(presence)
        }
        fn group_members(&self, pgid: u32) -> Result<Vec<u32>> {
            let mut state = self.0.lock().unwrap();
            match state.listings_before_failure {
                Some(0) => state.fail_listing = true,
                Some(left) => state.listings_before_failure = Some(left - 1),
                None => {}
            }
            if state.fail_listing {
                return Err(unavailable("scripted listing failure"));
            }
            let members = state
                .procs
                .iter()
                .filter(|(_, p)| p.pgid == pgid)
                .map(|(pid, _)| *pid)
                .collect();
            for (pid, p) in std::mem::take(&mut state.appear_on_next_listing) {
                state.procs.insert(pid, p);
            }
            if let Some(edit) = state.after_listing.pop_front() {
                edit(&mut state);
            }
            Ok(members)
        }
        fn children(&self, pid: u32) -> Result<Vec<u32>> {
            let mut state = self.0.lock().unwrap();
            if state.fail_children {
                return Err(unavailable("scripted children failure"));
            }
            let children = state
                .procs
                .iter()
                .filter(|(_, p)| p.ppid == pid && !p.zombie)
                .map(|(child, _)| *child)
                .collect();
            if let Some(edit) = state.after_children.take() {
                edit(&mut state);
            }
            Ok(children)
        }
        fn priorities(&self, pid: u32) -> Result<Priorities> {
            let mut state = self.0.lock().unwrap();
            if state.fail_priorities {
                return Err(unavailable("scripted priorities failure"));
            }
            let p = state
                .procs
                .get(&pid)
                .ok_or_else(|| unavailable("no process"))?
                .clone();
            if let Some(edit) = state.after_priorities.take() {
                edit(&mut state);
            }
            Ok(Priorities {
                task_priority: p.task_priority,
                max_thread_priority: p.max_thread_priority,
                threads: 1,
            })
        }
        fn renice(&self, pid: u32, nice: i32) -> Result<()> {
            let mut state = self.0.lock().unwrap();
            state.renices += 1;
            if state.renice_denied {
                return Err(unavailable("scripted denial"));
            }
            state.procs.get_mut(&pid).unwrap().nice = nice;
            Ok(())
        }
        fn signal(&self, pid: u32, signal: i32) -> Result<()> {
            let mut state = self.0.lock().unwrap();
            if state.failing_signals.contains(&pid) {
                return Err(unavailable("scripted delivery failure"));
            }
            state.signals.push((pid, signal));
            Ok(())
        }
        fn effective_uid(&self) -> u32 {
            UID
        }
        fn own_pid(&self) -> u32 {
            7
        }
    }

    struct Fixed;
    impl Clock for Fixed {
        fn now(&self) -> ObservationTime {
            now()
        }
    }

    fn proc(ppid: u32, pgid: u32, start: u64) -> Proc {
        Proc {
            ppid,
            pgid,
            uid: UID,
            nice: 0,
            start,
            zombie: false,
            task_priority: UTILITY_PRIORITY_CEILING,
            max_thread_priority: UTILITY_PRIORITY_CEILING,
        }
    }

    fn now() -> ObservationTime {
        ObservationTime {
            boot_id: "boot".into(),
            monotonic_ms: 1_000,
        }
    }

    fn key() -> AttemptKey {
        AttemptKey {
            consumer_id: "consumer".into(),
            consumer_generation: "generation".into(),
            attempt_id: "attempt".into(),
        }
    }

    fn owner() -> InstanceIdentity {
        InstanceIdentity {
            instance_id: "instance".into(),
            process: ProcessIdentity {
                boot_id: "boot".into(),
                pid: 7,
                start_ticks: 7,
            },
        }
    }

    fn root() -> ProcessIdentity {
        ProcessIdentity {
            boot_id: "boot".into(),
            pid: ROOT,
            start_ticks: 1_000,
        }
    }

    fn plan() -> ExecutionPlan {
        crate::backend::macos_plan()
    }

    fn tracker() -> ScopeTracker<Scripted> {
        let table = Scripted::default();
        table.with(|s| {
            s.procs.insert(ROOT, proc(1, ROOT, 1_000));
        });
        ScopeTracker::new(table)
    }

    fn establish(tracker: &ScopeTracker<Scripted>) -> Result<Establishment> {
        tracker.establish(&key(), &owner(), &root(), &plan(), Budget::ZERO, &Fixed)
    }

    fn established(tracker: &ScopeTracker<Scripted>) -> ScopeIdentity {
        let established = establish(tracker).unwrap();
        assert_eq!(established.applied.cpu, ApplicationState::Applied);
        established.scope
    }

    fn observe(tracker: &ScopeTracker<Scripted>, scope: &ScopeIdentity) -> ScopeObservation {
        tracker.observe(scope, &Fixed).unwrap()
    }

    fn known(tracker: &ScopeTracker<Scripted>, scope: &ScopeIdentity) -> BTreeSet<Identity> {
        let tracked = tracker.tracked(scope).unwrap();
        let known = tracked.lock().unwrap().known.clone();
        known
    }

    #[test]
    fn establishment_applies_nice_and_requires_utility_priorities_by_readback() {
        let tracker = tracker();
        let established = establish(&tracker).unwrap();
        assert_eq!(established.cpu.unwrap().nice, WORKLOAD_NICE);
        assert!(established.applied.confirms(&plan(), Budget::ZERO));
        assert_eq!(established.scope.kind, ScopeKind::ObservedProcessGroup);
        // Without the utility clamp the CPU policy is not applied, including a
        // root whose high nice already lowers its task priority to the ceiling
        // (31 - 15 = 16) while its threads remain at 63. A root whose nice
        // cannot be raised has not applied it either.
        let unclamped: [Edit; 4] = [
            |s| s.procs.get_mut(&ROOT).unwrap().max_thread_priority = 63,
            |s| s.procs.get_mut(&ROOT).unwrap().task_priority = 31,
            |s| {
                let p = s.procs.get_mut(&ROOT).unwrap();
                (p.nice, p.task_priority, p.max_thread_priority) = (15, 16, 63);
            },
            |s| s.renice_denied = true,
        ];
        for edit in unclamped {
            let tracker = self::tracker();
            tracker.table.with(edit);
            let established = establish(&tracker).unwrap();
            assert_eq!(established.applied.cpu, ApplicationState::Failed);
            assert!(!established.applied.confirms(&plan(), Budget::ZERO));
            // Binding repeats the readback rather than trusting establishment.
            let binding = tracker.binding(&established.scope, &Fixed).unwrap();
            assert_eq!(binding.applied.cpu, ApplicationState::Failed);
        }
        // Kernel controls are unsupported on an observed process group.
        let mut kernel = plan();
        kernel.memory = PlannedResource {
            level: EnforcementLevel::Kernel,
            method: ControlMethod::CgroupV2,
        };
        assert_eq!(
            self::tracker()
                .establish(&key(), &owner(), &root(), &kernel, Budget::ZERO, &Fixed)
                .unwrap_err()
                .code,
            ErrorCode::ResourcePolicyUnsupported
        );
    }

    #[test]
    fn a_root_replaced_during_readback_yields_no_evidence() {
        let tracker = tracker();
        tracker.table.with(|s| {
            s.after_priorities = Some(|s| s.procs.get_mut(&ROOT).unwrap().start = 9_999);
        });
        assert_eq!(
            establish(&tracker).unwrap_err().code,
            ErrorCode::ResourceControlUnavailable
        );
    }

    #[test]
    fn establishment_refuses_unowned_shared_changed_or_authority_roots() {
        let cases: [(Edit, ErrorCode); 5] = [
            (
                |s| s.procs.get_mut(&ROOT).unwrap().pgid = 1,
                ErrorCode::ResourcePolicyUnsupported,
            ),
            (
                |s| {
                    s.procs.insert(101, proc(ROOT, ROOT, 1_001));
                },
                ErrorCode::ResourcePolicyUnsupported,
            ),
            (
                |s| s.procs.get_mut(&ROOT).unwrap().uid = 0,
                ErrorCode::Unauthorized,
            ),
            (
                |s| s.procs.get_mut(&ROOT).unwrap().start = 2_000,
                ErrorCode::ResourcePolicyUnsupported,
            ),
            (
                |s| {
                    s.procs.remove(&ROOT);
                },
                ErrorCode::ResourceControlUnavailable,
            ),
        ];
        for (edit, code) in cases {
            let tracker = tracker();
            tracker.table.with(edit);
            assert_eq!(establish(&tracker).unwrap_err().code, code);
            assert!(tracker.table.0.lock().unwrap().signals.is_empty());
        }
        struct Elsewhere;
        impl Clock for Elsewhere {
            fn now(&self) -> ObservationTime {
                ObservationTime {
                    boot_id: "other".into(),
                    monotonic_ms: 1,
                }
            }
        }
        assert!(tracker()
            .establish(&key(), &owner(), &root(), &plan(), Budget::ZERO, &Elsewhere)
            .is_err());
        // The authority process itself is never a scope root.
        let tracker = tracker();
        tracker.table.with(|s| {
            s.procs.insert(7, proc(1, 7, 7));
        });
        let own = ProcessIdentity {
            boot_id: "boot".into(),
            pid: 7,
            start_ticks: 7,
        };
        assert!(tracker
            .establish(&key(), &owner(), &own, &plan(), Budget::ZERO, &Fixed)
            .is_err());
        // The same root cannot be bound to another attempt or quantity.
        established(&tracker);
        let mut other = key();
        other.attempt_id = "other".into();
        for (key, quantities) in [
            (other, Budget::ZERO),
            (
                key(),
                Budget {
                    cpu_milli: 1,
                    memory_bytes: 1,
                    tasks: 1,
                },
            ),
        ] {
            assert_eq!(
                tracker
                    .establish(&key, &owner(), &root(), &plan(), quantities, &Fixed)
                    .unwrap_err()
                    .code,
                ErrorCode::AttemptConflict
            );
        }
    }

    #[test]
    fn a_reaped_root_does_not_release_surviving_members_or_zombies() {
        let tracker = tracker();
        let scope = established(&tracker);
        tracker.table.with(|s| {
            s.procs.insert(101, proc(ROOT, ROOT, 1_001));
        });
        let seen = observe(&tracker, &scope);
        assert!(!seen.root_reaped && !seen.empty && !seen.known_members_gone);
        // Root exits but is not reaped yet.
        tracker
            .table
            .with(|s| s.procs.get_mut(&ROOT).unwrap().zombie = true);
        let zombie = observe(&tracker, &scope);
        assert!(!zombie.root_reaped && !zombie.empty && !zombie.known_members_gone);
        // Reaped root, descendant still running.
        tracker.table.with(|s| {
            s.procs.remove(&ROOT);
        });
        let orphan = observe(&tracker, &scope);
        assert!(orphan.root_reaped && !orphan.empty && !orphan.known_members_gone);
        assert!(orphan.tracking_complete);
        assert!(!orphan.supports_release(&scope, &now()));
        // Descendant exited but unreaped: still present.
        tracker
            .table
            .with(|s| s.procs.get_mut(&101).unwrap().zombie = true);
        let unreaped = observe(&tracker, &scope);
        assert!(!unreaped.empty && !unreaped.known_members_gone);
        tracker.table.with(|s| {
            s.procs.remove(&101);
        });
        let done = observe(&tracker, &scope);
        assert!(done.supports_release(&scope, &now()));
        // Gone identities are pruned rather than rechecked forever.
        assert!(known(&tracker, &scope).is_empty());
    }

    #[test]
    fn escapes_are_detected_by_membership_and_by_parentage_and_stay_sticky() {
        // A known member moves to another group.
        let tracker = tracker();
        let scope = established(&tracker);
        tracker.table.with(|s| {
            s.procs.insert(101, proc(ROOT, ROOT, 1_001));
        });
        observe(&tracker, &scope);
        tracker
            .table
            .with(|s| s.procs.get_mut(&101).unwrap().pgid = 101);
        assert!(observe(&tracker, &scope).known_escape);
        // Everything exits later; the escape is never forgotten.
        tracker.table.with(|s| s.procs.clear());
        let later = observe(&tracker, &scope);
        assert!(later.known_escape && later.empty && later.root_reaped);
        assert!(!later.supports_release(&scope, &now()));

        // A child created directly in another group, never seen as a member.
        let tracker = self::tracker();
        let scope = established(&tracker);
        tracker.table.with(|s| {
            s.procs.insert(102, proc(ROOT, 102, 1_002));
        });
        assert!(observe(&tracker, &scope).known_escape);
        // Termination reaches the known escaped identity, rechecked first.
        let receipt = tracker.signal(&scope, 9, &Fixed).unwrap();
        assert!(receipt.signalled.contains(&(102, 1_002)));
        assert!(receipt.signalled.contains(&(ROOT, 1_000)));
    }

    #[test]
    fn newcomers_are_adopted_while_the_root_or_a_known_member_holds_the_group() {
        let tracker = tracker();
        let scope = established(&tracker);
        tracker.table.with(|s| {
            s.procs.insert(101, proc(ROOT, ROOT, 1_001));
        });
        observe(&tracker, &scope);
        assert!(known(&tracker, &scope).contains(&(101, 1_001)));
        // With the root reaped, the verified known member still proves the
        // group, so its new child is adopted.
        tracker.table.with(|s| {
            s.procs.remove(&ROOT);
            s.procs.insert(103, proc(101, ROOT, 1_003));
        });
        let observed = observe(&tracker, &scope);
        assert!(observed.tracking_complete && !observed.empty);
        assert!(known(&tracker, &scope).contains(&(103, 1_003)));
        // The newcomer leaving the group is then an escape, not an exit.
        tracker
            .table
            .with(|s| s.procs.get_mut(&103).unwrap().pgid = 103);
        assert!(observe(&tracker, &scope).known_escape);
    }

    #[test]
    fn children_of_a_reused_parent_pid_are_not_adopted() {
        let tracker = tracker();
        let scope = established(&tracker);
        // Between listing the root's children and rechecking the root, the
        // root is replaced by another process with the same PID.
        tracker.table.with(|s| {
            s.procs.insert(103, proc(ROOT, 103, 1_003));
            s.after_children = Some(|s| {
                s.procs.insert(ROOT, proc(1, ROOT, 5_000));
            });
        });
        let observed = observe(&tracker, &scope);
        assert!(!observed.known_escape);
        assert!(!known(&tracker, &scope).contains(&(103, 1_003)));
    }

    #[test]
    fn incomplete_observation_is_sticky_tracking_loss() {
        let tracker = tracker();
        let scope = established(&tracker);
        tracker.table.with(|s| s.fail_listing = true);
        let failed = observe(&tracker, &scope);
        assert!(!failed.tracking_complete && !failed.empty);
        tracker.table.with(|s| {
            s.fail_listing = false;
            s.procs.clear();
        });
        let later = observe(&tracker, &scope);
        assert!(later.empty && later.root_reaped && later.known_members_gone);
        assert!(!later.tracking_complete);
        assert!(!later.supports_release(&scope, &now()));
        // A refused identity read is also tracking loss.
        let tracker = self::tracker();
        let scope = established(&tracker);
        tracker.table.with(|s| {
            s.fail_presence.insert(ROOT);
        });
        assert!(!observe(&tracker, &scope).tracking_complete);
        // So is an unknown process listed in the group that has moved before
        // its identity read: it cannot be told apart from a reused PID.
        let tracker = self::tracker();
        let scope = established(&tracker);
        tracker.table.with(|s| {
            s.procs.insert(105, proc(1, ROOT, 1_005));
        });
        let members = tracker.table.group_members(ROOT).unwrap();
        tracker
            .table
            .with(|s| s.procs.get_mut(&105).unwrap().pgid = 105);
        let tracked = tracker.tracked(&scope).unwrap();
        let mut complete = true;
        tracker.classify_listing(&tracked.lock().unwrap(), members, &mut complete);
        assert!(!complete);
        // Failing children or confirming listings are tracking loss too.
        let tracker = self::tracker();
        let scope = established(&tracker);
        tracker.table.with(|s| s.fail_children = true);
        assert!(!observe(&tracker, &scope).tracking_complete);
        let tracker = self::tracker();
        let scope = established(&tracker);
        tracker.table.with(|s| {
            s.procs.remove(&ROOT);
            s.listings_before_failure = Some(1);
        });
        let unconfirmed = observe(&tracker, &scope);
        assert!(!unconfirmed.tracking_complete && !unconfirmed.empty);
    }

    #[test]
    fn a_root_replaced_while_listing_does_not_vouch_for_the_group() {
        let tracker = tracker();
        let scope = established(&tracker);
        // After the root check and during the listing, the root is reaped and
        // its PID reused by a foreign group leader with a member of its own.
        tracker.table.with(|s| {
            s.after_listing.push_back(|s| {
                s.procs.insert(ROOT, proc(1, ROOT, 50_000));
                s.procs.insert(300, proc(ROOT, ROOT, 50_001));
            });
        });
        let observed = observe(&tracker, &scope);
        assert!(!observed.tracking_complete && observed.root_reaped);
        let known = known(&tracker, &scope);
        assert!(!known.contains(&(ROOT, 50_000)) && !known.contains(&(300, 50_001)));
        let receipt = tracker.signal(&scope, 9, &Fixed).unwrap();
        assert!(receipt.signalled.is_empty());
        assert!(tracker.table.0.lock().unwrap().signals.is_empty());
    }

    #[test]
    fn children_whose_parentage_cannot_be_verified_are_tracking_loss() {
        // A known member exits and is reaped between the listing of its
        // children and its recheck, and its child has already left the group.
        let tracker = tracker();
        let scope = established(&tracker);
        tracker.table.with(|s| {
            s.procs.insert(101, proc(ROOT, ROOT, 1_001));
        });
        observe(&tracker, &scope);
        tracker.table.with(|s| {
            s.procs.insert(110, proc(101, 110, 1_010));
            s.after_children = Some(|s| {
                s.procs.remove(&101);
            });
        });
        let observed = observe(&tracker, &scope);
        assert!(!observed.tracking_complete);
        assert!(!observed.supports_release(&scope, &now()));
        // A listed child reparented before its read, now outside the group.
        let tracker = self::tracker();
        let scope = established(&tracker);
        tracker.table.with(|s| {
            s.procs.insert(111, proc(ROOT, 111, 1_011));
            s.after_children = Some(|s| s.procs.get_mut(&111).unwrap().ppid = 1);
        });
        assert!(!observe(&tracker, &scope).tracking_complete);
    }

    #[test]
    fn termination_reaches_verified_identities_even_when_the_group_is_unproven() {
        let tracker = tracker();
        let scope = established(&tracker);
        tracker.table.with(|s| {
            s.procs.insert(101, proc(ROOT, ROOT, 1_001));
        });
        observe(&tracker, &scope);
        // The root cannot be observed (as for a setuid root).
        tracker.table.with(|s| {
            s.fail_presence.insert(ROOT);
        });
        let receipt = tracker.signal(&scope, 15, &Fixed).unwrap();
        assert!(!receipt.complete);
        assert_eq!(receipt.signalled, vec![(101, 1_001)]);
        // The listing itself fails.
        tracker.table.with(|s| {
            s.fail_presence.clear();
            s.fail_listing = true;
        });
        let receipt = tracker.signal(&scope, 15, &Fixed).unwrap();
        assert!(!receipt.complete);
        assert_eq!(receipt.signalled, vec![(ROOT, 1_000), (101, 1_001)]);
        // A failed delivery is reported rather than hidden.
        tracker.table.with(|s| {
            s.fail_listing = false;
            s.failing_signals.insert(101);
        });
        let receipt = tracker.signal(&scope, 15, &Fixed).unwrap();
        assert!(!receipt.complete);
        assert_eq!(receipt.signalled, vec![(ROOT, 1_000)]);
        tracker.table.with(|s| s.failing_signals.clear());
        assert!(tracker.signal(&scope, 15, &Fixed).unwrap().complete);
    }

    #[test]
    fn readback_failures_are_never_applied_and_a_higher_nice_is_kept() {
        let tracker = tracker();
        tracker.table.with(|s| s.fail_priorities = true);
        let established = establish(&tracker).unwrap();
        assert_eq!(established.applied.cpu, ApplicationState::Failed);
        assert!(established.cpu.is_none());
        let tracker = self::tracker();
        tracker
            .table
            .with(|s| s.procs.get_mut(&ROOT).unwrap().nice = 15);
        let established = establish(&tracker).unwrap();
        assert_eq!(established.applied.cpu, ApplicationState::Applied);
        assert_eq!(established.cpu.unwrap().nice, 15);
        assert_eq!(tracker.table.0.lock().unwrap().renices, 0);
        assert_eq!(tracker.scheduler(ROOT).unwrap().nice, 15);
        assert!(tracker.scheduler(999).is_err());
    }

    #[test]
    fn a_member_listed_only_by_the_confirming_listing_prevents_release() {
        let tracker = tracker();
        let scope = established(&tracker);
        tracker.table.with(|s| {
            s.procs.remove(&ROOT);
            s.appear_on_next_listing = vec![(103, proc(1, ROOT, 1_003))];
        });
        let racing = observe(&tracker, &scope);
        assert!(!racing.empty);
        assert!(!racing.supports_release(&scope, &now()));
        // It cannot be proven to be this scope's member: tracking loss, not
        // an adopted member.
        assert!(!racing.tracking_complete);
        assert!(!known(&tracker, &scope).contains(&(103, 1_003)));
    }

    #[test]
    fn a_group_that_cannot_be_proven_ours_is_never_adopted_or_signalled() {
        // The root is reaped and no known member remains in the group, yet the
        // group lists an unknown process: it is neither adopted nor signalled.
        let tracker = tracker();
        let scope = established(&tracker);
        tracker.table.with(|s| {
            s.procs.remove(&ROOT);
            s.procs.insert(106, proc(1, ROOT, 1_006));
        });
        let unproven = observe(&tracker, &scope);
        assert!(!unproven.tracking_complete && !unproven.empty);
        assert!(!known(&tracker, &scope).contains(&(106, 1_006)));
        let receipt = tracker.signal(&scope, 15, &Fixed).unwrap();
        assert!(receipt.signalled.is_empty());
        assert!(!receipt.complete);
    }

    #[test]
    fn a_confirming_listing_must_be_empty_even_if_its_members_vanish() {
        // The root is reaped. Known member 101 is listed and then reaped after
        // forking 102; the confirming listing sees 102, which forks 103 and is
        // reaped before its read. 103 still runs in the group.
        let tracker = tracker();
        let scope = established(&tracker);
        tracker.table.with(|s| {
            s.procs.insert(101, proc(ROOT, ROOT, 1_001));
        });
        observe(&tracker, &scope);
        tracker.table.with(|s| {
            s.procs.remove(&ROOT);
            s.after_listing.push_back(|s| {
                s.procs.remove(&101);
                s.procs.insert(102, proc(1, ROOT, 1_002));
            });
            s.after_listing.push_back(|s| {
                s.procs.remove(&102);
                s.procs.insert(103, proc(1, ROOT, 1_003));
            });
        });
        let racing = observe(&tracker, &scope);
        assert!(!racing.empty);
        assert!(!racing.supports_release(&scope, &now()));
        // The group has not ended, so 103 is found; it cannot be proven to be
        // the scope's, which is tracking loss, never release.
        let later = observe(&tracker, &scope);
        assert!(!later.empty && !later.tracking_complete);
        assert!(!later.supports_release(&scope, &now()));
    }

    #[test]
    fn a_reused_group_id_after_the_scope_ended_is_never_listed_again() {
        let tracker = tracker();
        let scope = established(&tracker);
        tracker.table.with(|s| {
            s.procs.remove(&ROOT);
        });
        let ended = observe(&tracker, &scope);
        assert!(ended.supports_release(&scope, &now()));
        // PID wrap: an unrelated process reuses the root PID and leads a new
        // group with the same ID, with a member of its own.
        tracker.table.with(|s| {
            s.procs.insert(ROOT, proc(1, ROOT, 50_000));
            s.procs.insert(300, proc(ROOT, ROOT, 50_001));
        });
        let later = observe(&tracker, &scope);
        assert!(later.empty && later.root_reaped && later.known_members_gone);
        assert!(later.tracking_complete && !later.known_escape);
        assert!(known(&tracker, &scope).is_empty());
        let receipt = tracker.signal(&scope, 9, &Fixed).unwrap();
        assert!(receipt.signalled.is_empty());
        assert!(tracker.table.0.lock().unwrap().signals.is_empty());
    }

    #[test]
    fn a_reused_pid_is_neither_a_known_member_nor_a_signal_target() {
        let tracker = tracker();
        let scope = established(&tracker);
        tracker.table.with(|s| {
            s.procs.insert(101, proc(ROOT, ROOT, 1_001));
        });
        observe(&tracker, &scope);
        // Member 101 exits and its PID is reused by an unrelated process.
        tracker.table.with(|s| {
            s.procs.insert(101, proc(1, 1, 9_999));
        });
        let observed = observe(&tracker, &scope);
        assert!(!observed.known_escape);
        assert!(!known(&tracker, &scope).iter().any(|(pid, _)| *pid == 101));
        let receipt = tracker.signal(&scope, 15, &Fixed).unwrap();
        assert!(!receipt.signalled.iter().any(|(pid, _)| *pid == 101));
        assert!(!tracker
            .table
            .0
            .lock()
            .unwrap()
            .signals
            .iter()
            .any(|(pid, _)| *pid == 101));
        // Signal 0 is a probe, not a termination request.
        assert_eq!(
            tracker.signal(&scope, 0, &Fixed).unwrap_err().code,
            ErrorCode::InvalidRequest
        );
    }

    #[test]
    fn unknown_or_altered_scopes_have_no_evidence() {
        let tracker = tracker();
        let scope = established(&tracker);
        let mut altered = scope.clone();
        altered.root.start_ticks += 1;
        assert!(tracker.binding(&altered, &Fixed).is_err());
        assert!(tracker.observe(&altered, &Fixed).is_err());
        assert!(tracker.signal(&altered, 15, &Fixed).is_err());
        // The backend looks the scope up before reading any clock, so this
        // holds on every platform.
        let backend = crate::NativeBackend::new(crate::BootClock::for_tests("boot"));
        assert!(backend.binding(&altered).is_err());
        assert!(backend.observe_scope(&altered).is_err());
        assert!(backend.signal_scope(&altered, 15).is_err());
    }

    const OWNER: u32 = 50;

    fn owner_process() -> ProcessIdentity {
        ProcessIdentity {
            boot_id: "boot".into(),
            pid: OWNER,
            start_ticks: 500,
        }
    }

    /// The owner (PID 50) and its helper child (PID 100), each in its own group.
    fn with_helper() -> ScopeTracker<Scripted> {
        let table = Scripted::default();
        table.with(|s| {
            s.procs.insert(OWNER, proc(1, OWNER, 500));
            s.procs.insert(ROOT, proc(OWNER, ROOT, 1_000));
        });
        ScopeTracker::new(table)
    }

    #[test]
    fn launch_helper_must_be_a_live_child_of_the_running_owner() {
        let tracker = with_helper();
        assert_eq!(
            tracker.helper(ROOT, &owner_process(), &Fixed).unwrap(),
            root()
        );
        let refusals: [(Edit, ErrorCode); 7] = [
            // Created by another process.
            (
                |s| s.procs.get_mut(&ROOT).unwrap().ppid = 1,
                ErrorCode::Unauthorized,
            ),
            // The owner has exited, is a zombie, or its PID was reused.
            (
                |s| {
                    s.procs.remove(&OWNER);
                },
                ErrorCode::Unauthorized,
            ),
            (
                |s| s.procs.get_mut(&OWNER).unwrap().zombie = true,
                ErrorCode::Unauthorized,
            ),
            (
                |s| s.procs.get_mut(&OWNER).unwrap().start = 501,
                ErrorCode::Unauthorized,
            ),
            // Another user's process.
            (
                |s| s.procs.get_mut(&ROOT).unwrap().uid = UID + 1,
                ErrorCode::Unauthorized,
            ),
            // The helper is gone or unreaped.
            (
                |s| {
                    s.procs.remove(&ROOT);
                },
                ErrorCode::ResourceControlUnavailable,
            ),
            (
                |s| s.procs.get_mut(&ROOT).unwrap().zombie = true,
                ErrorCode::ResourceControlUnavailable,
            ),
        ];
        for (edit, code) in refusals {
            let tracker = with_helper();
            tracker.table.with(edit);
            assert_eq!(
                tracker
                    .helper(ROOT, &owner_process(), &Fixed)
                    .unwrap_err()
                    .code,
                code
            );
        }
        // An owner from another boot, the authority itself and PID 0.
        let mut previous = owner_process();
        previous.boot_id = "other".into();
        for (pid, owner) in [(ROOT, previous), (7, owner_process()), (0, owner_process())] {
            assert_eq!(
                tracker.helper(pid, &owner, &Fixed).unwrap_err().code,
                ErrorCode::Unauthorized
            );
        }
        // A refused observation is an error, never a missing helper.
        let tracker = with_helper();
        tracker.table.with(|s| {
            s.fail_presence.insert(OWNER);
        });
        assert_eq!(
            tracker
                .helper(ROOT, &owner_process(), &Fixed)
                .unwrap_err()
                .code,
            ErrorCode::ResourceControlUnavailable
        );
    }

    #[test]
    fn launch_helper_pid_reused_during_the_owner_check_yields_no_identity() {
        let tracker = with_helper();
        tracker.table.with(|s| {
            s.after_presence_of = Some((ROOT, |s| {
                s.procs.get_mut(&ROOT).unwrap().start = 2_000;
            }));
        });
        assert_eq!(
            tracker
                .helper(ROOT, &owner_process(), &Fixed)
                .unwrap_err()
                .code,
            ErrorCode::ResourceControlUnavailable
        );
    }

    #[test]
    fn a_root_gone_before_readback_leaves_nothing_tracked() {
        let tracker = tracker();
        tracker.table.with(|s| {
            s.after_listing.push_back(|s| {
                s.procs.remove(&ROOT);
            })
        });
        assert_eq!(
            establish(&tracker).unwrap_err().code,
            ErrorCode::ResourceControlUnavailable
        );
        let scope = ScopeIdentity {
            kind: ScopeKind::ObservedProcessGroup,
            scope_id: format!("pg-{ROOT}-1000"),
            root: root(),
        };
        assert_eq!(
            tracker.observe(&scope, &Fixed).unwrap_err().code,
            ErrorCode::ReconciliationRequired
        );
    }

    #[test]
    fn forgetting_removes_only_the_matching_unclaimed_scope() {
        let tracker = tracker();
        let scope = established(&tracker);
        let mut other = scope.clone();
        other.root.start_ticks += 1;
        tracker.forget(&other).unwrap();
        assert!(tracker.tracked(&scope).is_ok());
        tracker.forget(&scope).unwrap();
        assert_eq!(
            tracker.tracked(&scope).err().unwrap().code,
            ErrorCode::ReconciliationRequired
        );
    }
}
