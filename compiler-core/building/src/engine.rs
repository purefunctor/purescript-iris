//! Implements the core build system for the query-based compiler
//!
//! Our implementation is inspired by the verifying step traces described in
//! the [Build systems à la carte: Theory and practice] paper. However, the
//! implementation has two key differences: we only retain the latest step
//! trace for any given query; and more significantly, we use structural
//! equality instead of hashing to compare cached and fresh values.
//!
//! Unlike traditional phase-based compilation, query-based compilers are
//! designed to have its intermediate states be observed directly using a
//! convenient API.
//!
//! The build system is designed to be pure and hermetic—the current state of
//! the workspace e.g. file contents are stored in-memory to make dependency
//! tracking easier to manage.
//!
//! Our implementation also borrows a few techniques used by [salsa] such as
//! using global query lock for ordering query reads and input writes, and
//! future-promise-based work deduplication. These techniques enable parallel
//! computation with cancellation and work deduplication!
//!
//! [Build systems à la carte: Theory and practice]: https://www.cambridge.org/core/journals/journal-of-functional-programming/article/build-systems-a-la-carte-theory-and-practice/097CE52C750E69BD16B78C318754C7A4
//! [salsa]: https://github.com/salsa-rs/salsa

mod graph;
mod promise;

use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::hash::{BuildHasher, Hash};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use building_types::{
    ModuleNameId, ModuleNameInterner, QueryError, QueryKey, QueryProxy, QueryResult,
};
use checking::CheckedModule;
use documenting::DocumentedModule;
use files::{FileId, ForeignFileCandidates, ForeignFileId};
use foreign_javascript::{ForeignModule, ForeignValidation};
use graph::SnapshotGraph;
use indexing::IndexedModule;
use lock_api::{RawRwLock, RawRwLockRecursive};
use lowering::{GroupedModule, LoweredModule};
use parking_lot::{Mutex, RwLock, RwLockUpgradableReadGuard};
use parsing::FullParsedModule;
use promise::{Future, Promise};
use resolving::{ExportedModule, ResolvedModule};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use stabilizing::StabilizedModule;
use thread_local::ThreadLocal;

#[derive(Debug, Clone, Copy)]
struct Trace {
    /// Timestamp of when the query was last called.
    built: usize,
    /// Timestamp of when the query was last recomputed.
    changed: usize,
}

#[derive(Debug, Default)]
enum DerivedState<T> {
    #[default]
    NotComputed,
    InProgress {
        id: SnapshotId,
        waiters: Mutex<Vec<Waiter<T>>>,
    },
    Computed {
        computed: T,
        trace: Trace,
        dependencies: Arc<[QueryKey]>,
    },
}

impl<T> DerivedState<T> {
    fn in_progress(id: SnapshotId) -> DerivedState<T> {
        DerivedState::InProgress { id, waiters: Mutex::default() }
    }
}

#[derive(Debug)]
struct Waiter<T> {
    id: SnapshotId,
    promise: Promise<T>,
}

#[derive(Debug)]
struct InputState<T> {
    value: T,
    changed: usize,
}

const SHARDS: usize = 16;
const SHARD_MASK: usize = SHARDS - 1;

/// A [`SHARDS`]-way sharded [`FxHashMap`] with individual [`RwLock`].
struct Shards<K, V> {
    inner: [RwLock<FxHashMap<K, V>>; SHARDS],
}

impl<K, V> Default for Shards<K, V> {
    fn default() -> Shards<K, V> {
        Shards { inner: std::array::from_fn(|_| RwLock::new(FxHashMap::default())) }
    }
}

impl<K, V> Shards<K, V>
where
    K: Hash,
{
    fn shard(&self, key: &K) -> &RwLock<FxHashMap<K, V>> {
        let hash = FxBuildHasher.hash_one(key);
        &self.inner[(hash as usize) & SHARD_MASK]
    }

    fn remove(&self, key: &K) -> Option<V>
    where
        K: Eq,
    {
        self.shard(key).write().remove(key)
    }
}

impl<K, V> Shards<K, V>
where
    K: Send + 'static,
    V: Send + 'static,
{
    fn drop_in(&mut self, scope: &rayon::Scope<'_>) {
        for shard in &mut self.inner {
            let shard = std::mem::take(shard.get_mut());
            scope.spawn(move |_| drop(shard));
        }
    }
}

#[derive(Default)]
struct InputStorage {
    content: Shards<FileId, InputState<Arc<str>>>,
    foreign: Shards<FileId, InputState<ForeignFileCandidates>>,
    foreign_content: Shards<ForeignFileId, InputState<Arc<str>>>,
    module: Shards<ModuleNameId, InputState<Option<FileId>>>,
}

#[derive(Default)]
struct DerivedStorage {
    foreign_module: Shards<ForeignFileId, DerivedState<Option<Arc<ForeignModule>>>>,
    foreign_validation: Shards<FileId, DerivedState<Arc<ForeignValidation>>>,
    parsed: Shards<FileId, DerivedState<FullParsedModule>>,
    stabilized: Shards<FileId, DerivedState<Arc<StabilizedModule>>>,
    indexed: Shards<FileId, DerivedState<Arc<IndexedModule>>>,
    lowered: Shards<FileId, DerivedState<Arc<LoweredModule>>>,
    grouped: Shards<FileId, DerivedState<Arc<GroupedModule>>>,
    resolved: Shards<FileId, DerivedState<Arc<ResolvedModule>>>,
    exported: Shards<FileId, DerivedState<Arc<ExportedModule>>>,
    bracketed: Shards<FileId, DerivedState<Arc<sugar::Bracketed>>>,
    sectioned: Shards<FileId, DerivedState<Arc<sugar::Sectioned>>>,
    checked_core: Shards<(), DerivedState<Arc<checking::context::CheckedCore>>>,
    checked: Shards<FileId, DerivedState<Arc<CheckedModule>>>,
    documented: Shards<FileId, DerivedState<Arc<DocumentedModule>>>,
    functional:
        Shards<FileId, DerivedState<functional::ModuleResult<Arc<functional::tree::Module>>>>,
    javascript: Shards<FileId, DerivedState<javascript::ModuleResult<Arc<javascript::Module>>>>,
}

impl Drop for DerivedStorage {
    fn drop(&mut self) {
        // Derived values are trees of many small allocations; releasing every
        // module's values on one thread dominates teardown of a large build.
        rayon::scope(|scope| {
            let DerivedStorage {
                foreign_module,
                foreign_validation,
                parsed,
                stabilized,
                indexed,
                lowered,
                grouped,
                resolved,
                exported,
                bracketed,
                sectioned,
                checked_core,
                checked,
                documented,
                functional,
                javascript,
            } = self;
            foreign_module.drop_in(scope);
            foreign_validation.drop_in(scope);
            parsed.drop_in(scope);
            stabilized.drop_in(scope);
            indexed.drop_in(scope);
            lowered.drop_in(scope);
            grouped.drop_in(scope);
            resolved.drop_in(scope);
            exported.drop_in(scope);
            bracketed.drop_in(scope);
            sectioned.drop_in(scope);
            checked_core.drop_in(scope);
            checked.drop_in(scope);
            documented.drop_in(scope);
            functional.drop_in(scope);
            javascript.drop_in(scope);
        });
    }
}

#[derive(Default)]
struct InternedStorage {
    module: ModuleNameInterner,
    checking: checking::CoreInterners,
}

fn query_references_file(query: QueryKey, file_id: FileId) -> bool {
    match query {
        QueryKey::Content(id)
        | QueryKey::Foreign(id)
        | QueryKey::ForeignValidation(id)
        | QueryKey::Parsed(id)
        | QueryKey::Stabilized(id)
        | QueryKey::Indexed(id)
        | QueryKey::Lowered(id)
        | QueryKey::Grouped(id)
        | QueryKey::Resolved(id)
        | QueryKey::Exported(id)
        | QueryKey::Bracketed(id)
        | QueryKey::Sectioned(id)
        | QueryKey::Checked(id)
        | QueryKey::Documented(id)
        | QueryKey::Functional(id)
        | QueryKey::JavaScript(id) => id == file_id,
        QueryKey::ForeignContent(_)
        | QueryKey::ForeignModule(_)
        | QueryKey::Module(_)
        | QueryKey::CheckedCore => false,
    }
}

fn state_references_removed_file<T>(
    state: &DerivedState<T>,
    file_id: FileId,
    removed_modules: &FxHashSet<ModuleNameId>,
) -> bool {
    let DerivedState::Computed { dependencies, .. } = state else {
        return false;
    };
    let mut dependencies = dependencies.iter();
    dependencies.any(|dependency| {
        query_references_file(*dependency, file_id)
            || matches!(dependency, QueryKey::Module(module) if removed_modules.contains(module))
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SnapshotId(u32);

#[derive(Default)]
struct GlobalState {
    /// An atomic token that determines if query execution had been cancelled.
    cancelled: AtomicBool,
    /// A global read-write lock for enforcing the order of reads and writes.
    query_lock: RwLock<()>,
    /// A counter that tracks the current revision of the query engine.
    revision: AtomicUsize,
    /// A counter that tracks the next [`SnapshotId`],
    snapshot: AtomicU32,
    /// A graph that tracks dependencies between [`SnapshotId`]
    graph: Mutex<SnapshotGraph>,
}

impl GlobalState {
    fn next_snapshot(&self) -> SnapshotId {
        SnapshotId(self.snapshot.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Default)]
struct LocalState {
    inner: ThreadLocal<RefCell<LocalStateInner>>,
}

impl LocalState {
    fn with_query<T>(&self, query: QueryKey, f: impl FnOnce(&RefCell<LocalStateInner>) -> T) -> T {
        let local = self.inner.get_or_default();
        {
            let mut inner = local.borrow_mut();
            if let Some(parent) = inner.frames.last_mut() {
                parent.dependencies.insert(query);
            }
            inner.frames.push(QueryFrame::new(query));
        }
        let result = f(local);
        {
            let mut inner = local.borrow_mut();
            let frame = inner.frames.pop().expect("invariant violated: expected query frame");
            debug_assert_eq!(frame.query, query);
        }
        result
    }

    fn with_dependency(&self, dependency: QueryKey) {
        let mut inner = self.inner.get_or_default().borrow_mut();
        if let Some(frame) = inner.frames.last_mut() {
            frame.dependencies.insert(dependency);
        }
    }

    fn dependencies(local: &RefCell<LocalStateInner>) -> Arc<[QueryKey]> {
        let inner = local.borrow();
        let dependencies = inner
            .frames
            .last()
            .expect("invariant violated: expected query frame")
            .dependencies
            .iter()
            .copied();
        dependencies.collect()
    }

    fn clear_dependencies(local: &RefCell<LocalStateInner>) {
        let mut inner = local.borrow_mut();
        let frame = inner.frames.last_mut().expect("invariant violated: expected query frame");
        frame.dependencies.clear();
    }

    fn stack(local: &RefCell<LocalStateInner>) -> Arc<[QueryKey]> {
        let inner = local.borrow();
        let stack = inner.frames.iter().map(|frame| frame.query);
        stack.collect()
    }

    fn mark_in_progress(local: &RefCell<LocalStateInner>) {
        let mut inner = local.borrow_mut();
        let frame = inner.frames.last_mut().expect("invariant violated: expected query frame");
        frame.in_progress = true;
    }

    fn is_in_progress(local: &RefCell<LocalStateInner>) -> bool {
        let inner = local.borrow();
        let frame = inner.frames.last().expect("invariant violated: expected query frame");
        frame.in_progress
    }

    #[cfg(test)]
    fn contains_in_progress(&self, query: QueryKey) -> bool {
        let inner = self.inner.get_or_default().borrow();
        inner.frames.iter().any(|frame| frame.query == query && frame.in_progress)
    }
}

#[derive(Debug, Default)]
struct LocalStateInner {
    frames: Vec<QueryFrame>,
}

#[derive(Debug)]
struct QueryFrame {
    query: QueryKey,
    in_progress: bool,
    dependencies: FxHashSet<QueryKey>,
}

impl QueryFrame {
    fn new(query: QueryKey) -> QueryFrame {
        QueryFrame { query, in_progress: false, dependencies: FxHashSet::default() }
    }
}

/// Custom guard that acquires a read lock from the [`GlobalState::query_lock`]
/// and releases it when dropped, effectively tying it to the lifetime of the
/// [`QueryControl`] it belongs to.
struct QueryControlGuard {
    global: Arc<GlobalState>,
}

impl QueryControlGuard {
    fn new(global: &Arc<GlobalState>) -> QueryControlGuard {
        // SAFETY: QueryControlGuard::drop
        unsafe { global.query_lock.raw().lock_shared_recursive() };
        QueryControlGuard { global: Arc::clone(global) }
    }
}

impl Drop for QueryControlGuard {
    fn drop(&mut self) {
        // SAFETY: QueryControlGuard::new
        unsafe { self.global.query_lock.raw().unlock_shared() }
    }
}

struct QueryControl {
    _guard: Option<QueryControlGuard>,
    id: SnapshotId,
    local: Arc<LocalState>,
    global: Arc<GlobalState>,
}

impl QueryControl {
    fn snapshot(&self) -> QueryControl {
        let _guard = Some(QueryControlGuard::new(&self.global));
        let local = Arc::new(LocalState::default());
        let global = Arc::clone(&self.global);
        let id = global.next_snapshot();
        QueryControl { _guard, id, local, global }
    }
}

impl Default for QueryControl {
    fn default() -> QueryControl {
        let _guard = None;
        let local = Arc::new(LocalState::default());
        let global = Arc::new(GlobalState::default());
        let id = global.next_snapshot();
        QueryControl { _guard, id, local, global }
    }
}

#[derive(Default)]
pub struct QueryEngine {
    input: Arc<InputStorage>,
    derived: Arc<DerivedStorage>,
    interned: Arc<InternedStorage>,
    control: QueryControl,
}

impl QueryEngine {
    /// Creates a snapshot of the [`QueryEngine`].
    ///
    /// Snapshots are read locks over the [`QueryEngine`] that must
    /// be sent across threads to perform query execution.
    ///
    /// As with read locks, keeping snapshots alive indefinitely is
    /// a logic error and will cause a deadlock on mutation or on a
    /// [cancellation request].
    ///
    /// [cancellation request]: QueryEngine::request_cancel
    pub fn snapshot(&self) -> QueryEngine {
        let input = self.input.clone();
        let derived = self.derived.clone();
        let interned = self.interned.clone();
        let control = self.control.snapshot();
        QueryEngine { input, derived, interned, control }
    }

    /// Creates a cancellation request for queries.
    ///
    /// Query cancellation is cooperative. A cancellation flag is read
    /// at some point during query execution. This function also waits
    /// for all snapshots to be dropped, as in the expected consequence
    /// of cancelling all queries running across all threads.
    pub fn request_cancel(&self) {
        self.control.global.cancelled.store(true, Ordering::Relaxed);
        let _query_lock = self.control.global.query_lock.write();
        self.control.global.cancelled.store(false, Ordering::Relaxed);
    }
}

impl QueryEngine {
    fn query<K, V, ShardsFn, ComputeFn>(
        &self,
        query: QueryKey,
        key: K,
        shards: ShardsFn,
        compute: ComputeFn,
    ) -> QueryResult<V>
    where
        K: Hash + Eq + Copy,
        ShardsFn: Fn(&DerivedStorage) -> &Shards<K, DerivedState<V>>,
        ComputeFn: Fn(&QueryEngine) -> QueryResult<V>,
        V: Eq + Clone,
    {
        self.control.local.with_query(query, |local| {
            // If query execution fails at any given point, clean up the state.
            self.query_core(key, &shards, &compute, local).inspect_err(|_| {
                if LocalState::is_in_progress(local) {
                    let shard = shards(&self.derived).shard(&key);
                    let mut guard = shard.write();
                    if let Entry::Occupied(o) = guard.entry(key) {
                        if let DerivedState::InProgress { id, waiters } = o.remove() {
                            let waiters = waiters.into_inner();
                            self.remove_waiter_edges(id, &waiters);
                            drop(guard);
                            drop(waiters);
                        } else {
                            unreachable!("invariant violated: expected InProgress");
                        }
                    }
                }
            })
        })
    }

    /// Fulfills the promises of an [`DerivedState::InProgress`] query and
    /// replaces it with a [`DerivedState::Computed`] result in the store.
    fn fulfill_and_store<K, V, ShardsFn>(
        &self,
        key: K,
        shards: &ShardsFn,
        computed: V,
        trace: Trace,
        dependencies: Arc<[QueryKey]>,
    ) where
        K: Hash + Eq + Copy,
        ShardsFn: Fn(&DerivedStorage) -> &Shards<K, DerivedState<V>>,
        V: Clone,
    {
        let shard = shards(&self.derived).shard(&key);
        let mut guard = shard.write();
        if let Entry::Occupied(o) = guard.entry(key) {
            if let DerivedState::InProgress { id, waiters } = o.remove() {
                let waiters = waiters.into_inner();
                self.remove_waiter_edges(id, &waiters);
                waiters.into_iter().for_each(|waiter| {
                    let computed = V::clone(&computed);
                    waiter.promise.fulfill(computed);
                });
            } else {
                unreachable!("invariant violated: expected InProgress");
            }
        }

        let state = DerivedState::Computed { computed, trace, dependencies };
        guard.insert(key, state);
    }

    fn remove_waiter_edges<T>(&self, to_id: SnapshotId, waiters: &[Waiter<T>]) {
        if waiters.is_empty() {
            return;
        }

        let mut graph = self.control.global.graph.lock();
        for waiter in waiters {
            graph.remove_edge(waiter.id, to_id);
        }
    }

    fn compute_core<K, V, ShardsFn, ComputeFn>(
        &self,
        key: K,
        shards: &ShardsFn,
        compute: &ComputeFn,
        revision: usize,
        previous: Option<(V, Trace)>,
        local: &RefCell<LocalStateInner>,
    ) -> QueryResult<V>
    where
        K: Hash + Eq + Copy,
        ShardsFn: Fn(&DerivedStorage) -> &Shards<K, DerivedState<V>>,
        ComputeFn: Fn(&QueryEngine) -> QueryResult<V>,
        V: Eq + Clone,
    {
        if self.control.global.cancelled.load(Ordering::Relaxed) {
            return Err(QueryError::Cancelled);
        }

        let computed = compute(self)?;

        // If the computed result is equal to the cached one, the changed
        // timestamp does not need to be updated. Likewise, we also insert
        // the previous value back into the cache. The latter is a niche,
        // but useful optimisation for when V = Arc<T>, since it enables
        // pointer equality.
        match previous {
            Some((previous, trace)) if computed == previous => {
                let trace = Trace { built: revision, changed: trace.changed };
                let dependencies = LocalState::dependencies(local);
                self.fulfill_and_store(key, shards, V::clone(&previous), trace, dependencies);
                Ok(previous)
            }
            _ => {
                let trace = Trace { built: revision, changed: revision };
                let dependencies = LocalState::dependencies(local);
                self.fulfill_and_store(key, shards, V::clone(&computed), trace, dependencies);
                Ok(computed)
            }
        }
    }

    /// Verifies the given dependencies by executing them, returning the
    /// timestamp of the most latest change.
    fn verify_core(&self, dependencies: &[QueryKey]) -> QueryResult<usize> {
        let mut latest = 0;

        macro_rules! input_changed {
            ($field:ident, $key:expr) => {{
                let shard = self.input.$field.shard($key).read();
                if let Some(InputState { changed, .. }) = shard.get($key) {
                    latest = latest.max(*changed);
                }
            }};
        }

        macro_rules! derived_changed {
            ($field:ident, $key:expr) => {{
                let _ = self.$field(*$key)?;
                let shard = self.derived.$field.shard($key).read();
                if let Some(DerivedState::Computed { trace, .. }) = shard.get($key) {
                    latest = latest.max(trace.changed);
                }
            }};
        }

        for dependency in dependencies {
            match dependency {
                QueryKey::Content(k) => input_changed!(content, k),
                QueryKey::Foreign(k) => input_changed!(foreign, k),
                QueryKey::ForeignContent(k) => input_changed!(foreign_content, k),
                QueryKey::ForeignModule(k) => derived_changed!(foreign_module, k),
                QueryKey::ForeignValidation(k) => derived_changed!(foreign_validation, k),
                QueryKey::Module(k) => input_changed!(module, k),
                QueryKey::Parsed(k) => derived_changed!(parsed, k),
                QueryKey::Stabilized(k) => derived_changed!(stabilized, k),
                QueryKey::Indexed(k) => derived_changed!(indexed, k),
                QueryKey::Lowered(k) => derived_changed!(lowered, k),
                QueryKey::Grouped(k) => derived_changed!(grouped, k),
                QueryKey::Resolved(k) => derived_changed!(resolved, k),
                QueryKey::Exported(k) => derived_changed!(exported, k),
                QueryKey::Bracketed(k) => derived_changed!(bracketed, k),
                QueryKey::Sectioned(k) => derived_changed!(sectioned, k),
                QueryKey::CheckedCore => {
                    self.checked_core()?;
                    let shard = self.derived.checked_core.shard(&()).read();
                    if let Some(DerivedState::Computed { trace, .. }) = shard.get(&()) {
                        latest = latest.max(trace.changed);
                    }
                }
                QueryKey::Checked(k) => derived_changed!(checked, k),
                QueryKey::Documented(k) => derived_changed!(documented, k),
                QueryKey::Functional(k) => derived_changed!(functional, k),
                QueryKey::JavaScript(k) => derived_changed!(javascript, k),
            }
        }

        Ok(latest)
    }

    fn create_future<T>(
        &self,
        to_id: SnapshotId,
        waiters: &Mutex<Vec<Waiter<T>>>,
        local: &RefCell<LocalStateInner>,
    ) -> QueryResult<Future<T>> {
        {
            let mut graph = self.control.global.graph.lock();
            if !graph.add_edge(self.control.id, to_id) {
                let stack = LocalState::stack(local);
                return Err(QueryError::Cycle { stack });
            }
        }

        let (future, promise) = Future::new();
        let waiter = Waiter { id: self.control.id, promise };
        waiters.lock().push(waiter);
        Ok(future)
    }

    fn query_core<K, V, ShardsFn, ComputeFn>(
        &self,
        key: K,
        shards: &ShardsFn,
        compute: &ComputeFn,
        local: &RefCell<LocalStateInner>,
    ) -> QueryResult<V>
    where
        K: Hash + Eq + Copy,
        ShardsFn: Fn(&DerivedStorage) -> &Shards<K, DerivedState<V>>,
        ComputeFn: Fn(&QueryEngine) -> QueryResult<V>,
        V: Eq + Clone,
    {
        if self.control.global.cancelled.load(Ordering::Relaxed) {
            return Err(QueryError::Cancelled);
        }

        let revision = self.control.global.revision.load(Ordering::Relaxed);
        let shard = shards(&self.derived).shard(&key);

        // Certain query states can be checked with only a read lock, and this
        // is an extremely useful optimisation because it allows threads to
        // skip their turn on acquiring an upgradable read lock.
        //
        // For computed queries, we can skip dependency verification if the
        // cached value was built during the current revision.
        //
        // For in-progress queries, we can simply push to the internally mutable
        // vector of waiters and then wait on the future.
        {
            let guard = shard.read();
            match guard.get(&key).unwrap_or(&DerivedState::NotComputed) {
                DerivedState::Computed { computed, trace, .. } if trace.built == revision => {
                    return Ok(V::clone(computed));
                }
                DerivedState::InProgress { id, waiters } => {
                    let future = self.create_future(*id, waiters, local)?;

                    // Remember that Future::wait blocks the current thread!
                    drop(guard);

                    return future.wait().ok_or(QueryError::Cancelled);
                }
                _ => (),
            }
        }

        // Otherwise, we will have to perform computation or cache verification.
        // Instead of a write lock, we use an upgradable read lock for two reasons:
        // we want to ensure that only a single thread can observe the NotComputed
        // state for any given query while allowing read locks to be acquired for
        // the optimisation above.
        {
            let guard = shard.upgradable_read();
            match guard.get(&key).unwrap_or(&DerivedState::NotComputed) {
                DerivedState::NotComputed => {
                    // At the end of this block, threads waiting to acquire the
                    // upgradable read lock should read that the query is InProgress.
                    {
                        let mut guard = RwLockUpgradableReadGuard::upgrade(guard);
                        guard.insert(key, DerivedState::in_progress(self.control.id));
                        LocalState::mark_in_progress(local);
                    }

                    self.compute_core(key, shards, compute, revision, None, local)
                }
                DerivedState::InProgress { id, waiters } => {
                    let future = self.create_future(*id, waiters, local)?;

                    // Remember that Future::wait blocks the current thread!
                    drop(guard);

                    future.wait().ok_or(QueryError::Cancelled)
                }
                DerivedState::Computed { computed, trace, dependencies } => {
                    let computed = V::clone(computed);
                    let trace = *trace;
                    let dependencies = Arc::clone(dependencies);

                    // If the cached value was built during the current revision
                    // we can skip dependency verification entirely. This is also
                    // checked at the start of the query_core with a read lock.
                    if trace.built == revision {
                        return Ok(computed);
                    }

                    // Same as NotComputed, see comment above.
                    {
                        let mut guard = RwLockUpgradableReadGuard::upgrade(guard);
                        guard.insert(key, DerivedState::in_progress(self.control.id));
                        LocalState::mark_in_progress(local);
                    }

                    let latest = self.verify_core(&dependencies)?;

                    // If the cached value was built more recently the the
                    // latest change, we can update its built timestamp to
                    // the current revision. This allows the query to hit
                    // the fastest path if it's called in the same revision.
                    if trace.built >= latest {
                        let trace = Trace { built: revision, ..trace };
                        self.fulfill_and_store(
                            key,
                            shards,
                            V::clone(&computed),
                            trace,
                            dependencies,
                        );
                        return Ok(computed);
                    }

                    LocalState::clear_dependencies(local);
                    self.compute_core(
                        key,
                        shards,
                        compute,
                        revision,
                        Some((computed, trace)),
                        local,
                    )
                }
            }
        }
    }

    fn set_input<K, V, F>(&self, key: K, shards: F, value: V)
    where
        K: Hash + Eq + Copy,
        F: FnOnce(&InputStorage) -> &Shards<K, InputState<V>>,
        V: Eq,
    {
        let shard = shards(&self.input).shard(&key);
        {
            let guard = shard.read();
            if guard.get(&key).is_some_and(|state| state.value == value) {
                return;
            }
        }

        self.set_input_transaction(key, shard, value);
    }

    fn set_input_transaction<K, V>(
        &self,
        key: K,
        shard: &RwLock<FxHashMap<K, InputState<V>>>,
        value: V,
    ) where
        K: Hash + Eq + Copy,
        V: Eq,
    {
        self.control.global.cancelled.store(true, Ordering::Relaxed);
        let _query_lock = self.control.global.query_lock.write();

        let mut guard = shard.write();
        if guard.get(&key).is_some_and(|state| state.value == value) {
            self.control.global.cancelled.store(false, Ordering::Relaxed);
            return;
        }

        let changed = self.control.global.revision.fetch_add(1, Ordering::Relaxed);
        let state = InputState { value, changed: changed + 1 };
        guard.insert(key, state);

        self.control.global.cancelled.store(false, Ordering::Relaxed);
    }

    fn get_input<K, V, F>(&self, query: QueryKey, key: K, shards: F) -> Option<V>
    where
        K: Hash + Eq,
        F: FnOnce(&InputStorage) -> &Shards<K, InputState<V>>,
        V: Clone,
    {
        self.control.local.with_dependency(query);
        let shard = shards(&self.input).shard(&key);
        let guard = shard.read();
        guard.get(&key).map(|state| V::clone(&state.value))
    }
}

impl QueryEngine {
    pub fn set_content(&self, id: FileId, content: impl Into<Arc<str>>) {
        self.set_input(id, |input| &input.content, content.into());
    }

    pub fn content(&self, id: FileId) -> QueryResult<Arc<str>> {
        self.get_input(QueryKey::Content(id), id, |input| &input.content)
            .ok_or(QueryError::MissingContent { file_id: id })
    }

    fn remove_file_queries<T>(
        &self,
        file_id: FileId,
        removed_modules: &FxHashSet<ModuleNameId>,
        queries: &Shards<FileId, DerivedState<T>>,
    ) {
        for shard in &queries.inner {
            let mut queries = shard.write();
            let removed_query_file_ids = queries.iter().filter_map(|(query_file_id, state)| {
                let remove = matches!(state, DerivedState::InProgress { .. })
                    || *query_file_id == file_id
                    || state_references_removed_file(state, file_id, removed_modules);
                remove.then_some(*query_file_id)
            });
            let removed_query_file_ids = removed_query_file_ids.collect::<Vec<_>>();
            for query_file_id in removed_query_file_ids {
                let state = queries
                    .remove(&query_file_id)
                    .expect("invariant violated: expected removable query state");
                self.discard_query_state(state);
            }
        }
    }

    fn discard_query_state<T>(&self, state: DerivedState<T>) {
        if let DerivedState::InProgress { id, waiters } = state {
            let waiters = waiters.into_inner();
            self.remove_waiter_edges(id, &waiters);
        }
    }

    pub fn remove_file(&self, file_id: FileId) {
        self.control.global.cancelled.store(true, Ordering::Relaxed);
        let _query_lock = self.control.global.query_lock.write();

        self.control.global.revision.fetch_add(1, Ordering::Relaxed);
        self.input.content.remove(&file_id);
        self.input.foreign.remove(&file_id);

        let mut removed_modules = FxHashSet::default();
        for shard in &self.input.module.inner {
            let mut modules = shard.write();
            modules.retain(|module, state| {
                if state.value != Some(file_id) {
                    return true;
                }
                removed_modules.insert(*module);
                false
            });
        }

        macro_rules! remove_file_queries {
            ($($field:ident),* $(,)?) => {
                $(self.remove_file_queries(file_id, &removed_modules, &self.derived.$field);)*
            };
        }
        remove_file_queries!(
            foreign_validation,
            parsed,
            stabilized,
            indexed,
            lowered,
            grouped,
            resolved,
            exported,
            bracketed,
            sectioned,
            checked,
            documented,
            functional,
            javascript,
        );

        for shard in &self.derived.checked_core.inner {
            let mut queries = shard.write();
            let remove = queries.get(&()).is_some_and(|state| {
                matches!(state, DerivedState::InProgress { .. })
                    || state_references_removed_file(state, file_id, &removed_modules)
            });
            if remove {
                let state = queries
                    .remove(&())
                    .expect("invariant violated: expected removable checked core state");
                self.discard_query_state(state);
            }
        }

        self.control.global.cancelled.store(false, Ordering::Relaxed);
    }

    pub fn set_foreign_content(&self, id: ForeignFileId, content: impl Into<Arc<str>>) {
        self.set_input(id, |input| &input.foreign_content, content.into());
    }

    pub fn foreign_content(&self, id: ForeignFileId) -> Option<Arc<str>> {
        self.get_input(QueryKey::ForeignContent(id), id, |input| &input.foreign_content)
    }

    pub fn set_foreign_file(&self, source_id: FileId, foreign_id: ForeignFileId) {
        let mut candidates = self.foreign_files(source_id);
        candidates.insert(foreign_id);
        self.set_input(source_id, |input| &input.foreign, candidates);
    }

    pub fn remove_foreign_file(&self, foreign_id: ForeignFileId) {
        self.control.global.cancelled.store(true, Ordering::Relaxed);
        let _query_lock = self.control.global.query_lock.write();

        let changed = self.control.global.revision.fetch_add(1, Ordering::Relaxed) + 1;
        for shard in &self.input.foreign.inner {
            let mut associations = shard.write();
            for state in associations.values_mut() {
                if state.value.remove(foreign_id) {
                    state.changed = changed;
                }
            }
        }

        self.input.foreign_content.remove(&foreign_id);
        self.derived.foreign_module.remove(&foreign_id);
        let dependency = QueryKey::ForeignModule(foreign_id);
        for shard in &self.derived.foreign_validation.inner {
            let mut validations = shard.write();
            validations.retain(|_, state| {
                let DerivedState::Computed { dependencies, .. } = state else {
                    return true;
                };
                !dependencies.contains(&dependency)
            });
        }

        self.control.global.cancelled.store(false, Ordering::Relaxed);
    }

    pub fn foreign_file(&self, source_id: FileId) -> Option<ForeignFileId> {
        self.foreign_files(source_id).unique()
    }

    pub fn foreign_files(&self, source_id: FileId) -> ForeignFileCandidates {
        self.get_input(QueryKey::Foreign(source_id), source_id, |input| &input.foreign)
            .unwrap_or_default()
    }

    pub fn foreign_module(&self, id: ForeignFileId) -> QueryResult<Option<Arc<ForeignModule>>> {
        self.query(
            QueryKey::ForeignModule(id),
            id,
            |derived| &derived.foreign_module,
            |this| {
                let module = this
                    .foreign_content(id)
                    .map(|content| Arc::new(foreign_javascript::parse_module(id.kind(), &content)));
                Ok(module)
            },
        )
    }

    pub fn foreign_validation(&self, id: FileId) -> QueryResult<Arc<ForeignValidation>> {
        self.query(
            QueryKey::ForeignValidation(id),
            id,
            |derived| &derived.foreign_validation,
            |this| {
                let indexed = this.indexed(id)?;
                let candidates = this.foreign_files(id);
                let foreign = if let Some(foreign_id) = candidates.unique() {
                    this.foreign_module(foreign_id)?
                } else {
                    None
                };
                let validation =
                    foreign_javascript::validate_module(&indexed, candidates, foreign.as_deref());
                Ok(Arc::new(validation))
            },
        )
    }

    pub fn set_module_file(&self, name: &str, file_id: FileId) {
        let id = self.interned.module.intern(name);
        self.set_input(id, |input| &input.module, Some(file_id));
    }

    pub fn remove_module_file(&self, name: &str, file_id: FileId) {
        let Some(id) = self.interned.module.lookup(name) else {
            return;
        };

        let current = self.get_input(QueryKey::Module(id), id, |input| &input.module);
        if current != Some(Some(file_id)) {
            return;
        }

        self.set_input(id, |input| &input.module, None);
    }

    pub fn module_file(&self, name: &str) -> Option<FileId> {
        let id = self.interned.module.intern(name);
        self.get_input(QueryKey::Module(id), id, |input| &input.module).flatten()
    }

    pub fn parsed(&self, id: FileId) -> QueryResult<FullParsedModule> {
        self.query(
            QueryKey::Parsed(id),
            id,
            |derived| &derived.parsed,
            |this| {
                let content = this.content(id)?;

                let lexed = lexing::lex(&content);
                let tokens = lexing::layout(&lexed);
                let parsed = parsing::parse(&lexed, &tokens);

                Ok(parsed)
            },
        )
    }

    pub fn stabilized(&self, id: FileId) -> QueryResult<Arc<StabilizedModule>> {
        self.query(
            QueryKey::Stabilized(id),
            id,
            |derived| &derived.stabilized,
            |this| {
                let (parsed, _) = this.parsed(id)?;
                let node = parsed.syntax_node();
                Ok(Arc::new(stabilizing::stabilize_module(&node)))
            },
        )
    }

    pub fn indexed(&self, id: FileId) -> QueryResult<Arc<IndexedModule>> {
        self.query(
            QueryKey::Indexed(id),
            id,
            |derived| &derived.indexed,
            |this| {
                let content = this.content(id)?;
                let (parsed, _) = this.parsed(id)?;
                let stabilized = this.stabilized(id)?;

                let module = parsed.cst();
                let indexed = indexing::index_module(&content, &module, &stabilized);

                Ok(Arc::new(indexed))
            },
        )
    }

    pub fn lowered(&self, id: FileId) -> QueryResult<Arc<LoweredModule>> {
        self.query(
            QueryKey::Lowered(id),
            id,
            |derived| &derived.lowered,
            |this| {
                let content = this.content(id)?;
                let (parsed, _) = this.parsed(id)?;

                let prim = {
                    let prim_id = this.prim_id();
                    this.resolved(prim_id)?
                };

                let stabilized = this.stabilized(id)?;
                let indexed = this.indexed(id)?;
                let resolved = this.resolved(id)?;

                let module = parsed.cst();
                let lowered = lowering::lower_module(
                    id,
                    &content,
                    &module,
                    &prim,
                    &stabilized,
                    &indexed,
                    &resolved,
                );

                Ok(Arc::new(lowered))
            },
        )
    }

    pub fn grouped(&self, id: FileId) -> QueryResult<Arc<GroupedModule>> {
        self.query(
            QueryKey::Grouped(id),
            id,
            |derived| &derived.grouped,
            |this| {
                let lowered = this.lowered(id)?;
                let indexed = this.indexed(id)?;
                let groups = lowering::group_module(&indexed, &lowered);
                Ok(Arc::new(groups))
            },
        )
    }

    pub fn resolved(&self, id: FileId) -> QueryResult<Arc<ResolvedModule>> {
        self.query(
            QueryKey::Resolved(id),
            id,
            |derived| &derived.resolved,
            |this| {
                let resolved = resolving::resolve_module(this, id)?;
                Ok(Arc::new(resolved))
            },
        )
    }

    pub fn exported(&self, id: FileId) -> QueryResult<Arc<ExportedModule>> {
        self.query(
            QueryKey::Exported(id),
            id,
            |derived| &derived.exported,
            |this| {
                let resolved = this.resolved(id)?;
                let exported = resolving::export_module(&resolved);
                Ok(Arc::new(exported))
            },
        )
    }

    pub fn bracketed(&self, id: FileId) -> QueryResult<Arc<sugar::Bracketed>> {
        self.query(
            QueryKey::Bracketed(id),
            id,
            |derived| &derived.bracketed,
            |this| {
                let lowered = this.lowered(id)?;
                let bracketed = sugar::bracketed(this, &lowered)?;
                Ok(Arc::new(bracketed))
            },
        )
    }

    pub fn sectioned(&self, id: FileId) -> QueryResult<Arc<sugar::Sectioned>> {
        self.query(
            QueryKey::Sectioned(id),
            id,
            |derived| &derived.sectioned,
            |this| {
                let lowered = this.lowered(id)?;
                let sectioned = sugar::sectioned(&lowered);
                Ok(Arc::new(sectioned))
            },
        )
    }

    pub fn checked_core(&self) -> QueryResult<Arc<checking::context::CheckedCore>> {
        self.query(
            QueryKey::CheckedCore,
            (),
            |derived| &derived.checked_core,
            |this| {
                let core = checking::context::CheckedCore::new(this)?;
                Ok(Arc::new(core))
            },
        )
    }

    pub fn checked(&self, id: FileId) -> QueryResult<Arc<CheckedModule>> {
        self.query(
            QueryKey::Checked(id),
            id,
            |derived| &derived.checked,
            |this| {
                let checked = checking::check_module(this, id)?;
                Ok(Arc::new(checked))
            },
        )
    }

    pub fn functional(
        &self,
        id: FileId,
    ) -> QueryResult<functional::ModuleResult<Arc<functional::tree::Module>>> {
        self.query(
            QueryKey::Functional(id),
            id,
            |derived| &derived.functional,
            |this| {
                let converted = functional::convert_module(this, id)?;
                Ok(converted.map(Arc::new))
            },
        )
    }

    pub fn javascript(
        &self,
        id: FileId,
    ) -> QueryResult<javascript::ModuleResult<Arc<javascript::Module>>> {
        self.query(
            QueryKey::JavaScript(id),
            id,
            |derived| &derived.javascript,
            |this| {
                let converted = javascript::convert_module(this, id)?;
                Ok(converted.map(Arc::new))
            },
        )
    }

    pub fn documented(&self, id: FileId) -> QueryResult<Arc<DocumentedModule>> {
        self.query(
            QueryKey::Documented(id),
            id,
            |derived| &derived.documented,
            |this| {
                let content = this.content(id)?;
                let (parsed, _) = this.parsed(id)?;
                let stabilized = this.stabilized(id)?;
                let indexed = this.indexed(id)?;
                Ok(documenting::document_module(&content, &parsed, &stabilized, &indexed))
            },
        )
    }
}

impl QueryEngine {
    pub fn prim_id(&self) -> FileId {
        self.module_file("Prim").expect("invariant violated: prim::configure")
    }
}

impl QueryProxy for QueryEngine {
    type Parsed = FullParsedModule;

    type Stabilized = Arc<StabilizedModule>;

    type Indexed = Arc<IndexedModule>;

    type Lowered = Arc<LoweredModule>;

    type Grouped = Arc<GroupedModule>;

    type Resolved = Arc<ResolvedModule>;

    type Exported = Arc<ExportedModule>;

    type Bracketed = Arc<sugar::Bracketed>;

    type Sectioned = Arc<sugar::Sectioned>;

    type Checked = Arc<checking::CheckedModule>;

    type Documented = Arc<documenting::DocumentedModule>;

    fn content(&self, id: FileId) -> QueryResult<Arc<str>> {
        QueryEngine::content(self, id)
    }

    fn parsed(&self, id: FileId) -> QueryResult<Self::Parsed> {
        QueryEngine::parsed(self, id)
    }

    fn stabilized(&self, id: FileId) -> QueryResult<Self::Stabilized> {
        QueryEngine::stabilized(self, id)
    }

    fn indexed(&self, id: FileId) -> QueryResult<Self::Indexed> {
        QueryEngine::indexed(self, id)
    }

    fn lowered(&self, id: FileId) -> QueryResult<Self::Lowered> {
        QueryEngine::lowered(self, id)
    }

    fn grouped(&self, id: FileId) -> QueryResult<Self::Grouped> {
        QueryEngine::grouped(self, id)
    }

    fn resolved(&self, id: FileId) -> QueryResult<Self::Resolved> {
        QueryEngine::resolved(self, id)
    }

    fn exported(&self, id: FileId) -> QueryResult<Self::Exported> {
        QueryEngine::exported(self, id)
    }

    fn bracketed(&self, id: FileId) -> QueryResult<Self::Bracketed> {
        QueryEngine::bracketed(self, id)
    }

    fn sectioned(&self, id: FileId) -> QueryResult<Self::Sectioned> {
        QueryEngine::sectioned(self, id)
    }

    fn checked(&self, id: FileId) -> QueryResult<Arc<checking::CheckedModule>> {
        QueryEngine::checked(self, id)
    }

    fn documented(&self, id: FileId) -> QueryResult<Arc<documenting::DocumentedModule>> {
        QueryEngine::documented(self, id)
    }

    fn prim_id(&self) -> FileId {
        QueryEngine::prim_id(self)
    }

    fn module_file(&self, name: &str) -> Option<FileId> {
        QueryEngine::module_file(self, name)
    }
}

impl functional::ExternalQueries for QueryEngine {
    fn functional(
        &self,
        file_id: FileId,
    ) -> QueryResult<functional::ModuleResult<Arc<functional::tree::Module>>> {
        QueryEngine::functional(self, file_id)
    }
}

impl javascript::ExternalQueries for QueryEngine {
    fn foreign_file(&self, file_id: FileId) -> QueryResult<Option<ForeignFileId>> {
        Ok(QueryEngine::foreign_file(self, file_id))
    }
}

impl javascript::ModuleQueries for QueryEngine {
    fn javascript(
        &self,
        file_id: FileId,
    ) -> QueryResult<javascript::ModuleResult<Arc<javascript::Module>>> {
        QueryEngine::javascript(self, file_id)
    }
}

impl checking::PrettyQueries for QueryEngine {
    #[inline]
    fn lookup_type(&self, id: checking::TypeId) -> &checking::Type {
        self.interned.checking.lookup_type(id)
    }

    #[inline]
    fn lookup_forall_binder(
        &self,
        id: checking::core::ForallBinderId,
    ) -> checking::core::ForallBinder {
        self.interned.checking.lookup_forall_binder(id)
    }

    #[inline]
    fn lookup_row_type(&self, id: checking::core::RowTypeId) -> &checking::core::RowType {
        self.interned.checking.lookup_row_type(id)
    }

    #[inline]
    fn lookup_smol_str(&self, id: checking::core::SmolStrId) -> &smol_str::SmolStr {
        self.interned.checking.lookup_smol_str(id)
    }
}

impl checking::ExternalQueries for QueryEngine {
    fn checked_core(&self) -> QueryResult<Arc<checking::context::CheckedCore>> {
        QueryEngine::checked_core(self)
    }

    fn intern_type(&self, t: checking::Type) -> checking::TypeId {
        self.interned.checking.intern_type(t)
    }

    #[inline]
    fn lookup_type_flags(&self, id: checking::TypeId) -> checking::core::TypeFlags {
        self.interned.checking.lookup_type_flags(id)
    }

    fn intern_forall_binder(
        &self,
        binder: checking::core::ForallBinder,
    ) -> checking::core::ForallBinderId {
        self.interned.checking.intern_forall_binder(binder)
    }

    fn intern_row_type(&self, row: checking::core::RowType) -> checking::core::RowTypeId {
        self.interned.checking.intern_row_type(row)
    }

    fn intern_smol_str(&self, s: smol_str::SmolStr) -> checking::core::SmolStrId {
        self.interned.checking.intern_smol_str(s)
    }
}

impl resolving::ExternalQueries for QueryEngine {}

impl sugar::ExternalQueries for QueryEngine {}

impl foreign_javascript::ForeignQueries for QueryEngine {
    fn foreign_module(&self, id: ForeignFileId) -> QueryResult<Option<Arc<ForeignModule>>> {
        QueryEngine::foreign_module(self, id)
    }

    fn foreign_validation(&self, id: FileId) -> QueryResult<Arc<ForeignValidation>> {
        QueryEngine::foreign_validation(self, id)
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};

    use building_types::{QueryError, QueryResult};
    use files::{FileId, Files, ForeignFiles, ForeignSourceKind};
    use foreign_javascript::ForeignError;
    use parking_lot::Mutex;
    use resolving::ResolvedModule;

    use crate::prim;

    use super::promise::Future;
    use super::{DerivedState, QueryEngine, QueryKey, SnapshotId, Waiter};

    #[derive(Debug)]
    struct Trace<'a> {
        built: usize,
        changed: usize,
        dependencies: &'a [QueryKey],
    }

    struct ShowTrace<'a, T>(&'a DerivedState<T>);

    impl<'a, T> Debug for ShowTrace<'a, T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match &self.0 {
                DerivedState::NotComputed => write!(f, "NotComputed"),
                DerivedState::InProgress { .. } => write!(f, "InProgress {{ .. }}"),
                DerivedState::Computed { trace, dependencies, .. } => f
                    .debug_struct("Trace")
                    .field("built", &trace.built)
                    .field("changed", &trace.changed)
                    .field("dependencies", dependencies)
                    .finish(),
            }
        }
    }

    impl<'a, 'b, T> PartialEq<Trace<'b>> for ShowTrace<'a, T> {
        fn eq(&self, other: &Trace<'b>) -> bool {
            match self.0 {
                DerivedState::NotComputed => false,
                DerivedState::InProgress { .. } => false,
                DerivedState::Computed { trace, dependencies, .. } => {
                    trace.built == other.built
                        && trace.changed == other.changed
                        && dependencies.len() == other.dependencies.len()
                        && dependencies
                            .iter()
                            .all(|dependency| other.dependencies.contains(dependency))
                }
            }
        }
    }

    #[test]
    fn test_equal_input_write_cost() {
        const SOURCE: FileId = FileId::new(0);
        const QUERY: FileId = FileId::new(1);

        let engine = QueryEngine::default();
        let initial_content: Arc<str> = Arc::from("module Main where\n\nvalue = 42");
        let equal_content: Arc<str> = Arc::from("module Main where\n\nvalue = 42");
        assert!(!Arc::ptr_eq(&initial_content, &equal_content));

        let recomputations = AtomicUsize::new(0);
        let compute = |engine: &QueryEngine| {
            recomputations.fetch_add(1, Ordering::Relaxed);
            engine.content(SOURCE)?;
            engine.parsed(SOURCE)
        };

        engine.set_content(SOURCE, initial_content);
        engine.query(QueryKey::Parsed(QUERY), QUERY, |derived| &derived.parsed, compute).unwrap();

        let revision_before = engine.control.global.revision.load(Ordering::Relaxed);
        let recomputations_before = recomputations.load(Ordering::Relaxed);

        engine.set_content(SOURCE, equal_content);
        engine.query(QueryKey::Parsed(QUERY), QUERY, |derived| &derived.parsed, compute).unwrap();

        let revision_after = engine.control.global.revision.load(Ordering::Relaxed);
        let recomputations_after = recomputations.load(Ordering::Relaxed);
        assert_eq!(
            (revision_after - revision_before, recomputations_after - recomputations_before),
            (0, 0)
        );

        engine.set_content(SOURCE, "module Main where\n\nvalue = 43");
        engine.query(QueryKey::Parsed(QUERY), QUERY, |derived| &derived.parsed, compute).unwrap();

        let revision_after_change = engine.control.global.revision.load(Ordering::Relaxed);
        let recomputations_after_change = recomputations.load(Ordering::Relaxed);
        assert_eq!(
            (
                revision_after_change - revision_after,
                recomputations_after_change - recomputations_after
            ),
            (1, 1)
        );
    }

    #[test]
    fn test_equal_input_write_does_not_cancel_live_snapshot() {
        const SOURCE: FileId = FileId::new(0);

        let engine = QueryEngine::default();
        let initial_content: Arc<str> = Arc::from("module Main where");
        let equal_content: Arc<str> = Arc::from("module Main where");
        assert!(!Arc::ptr_eq(&initial_content, &equal_content));

        engine.set_content(SOURCE, Arc::clone(&initial_content));
        let revision_before = engine.control.global.revision.load(Ordering::Relaxed);
        let snapshot = engine.snapshot();
        engine.set_content(SOURCE, equal_content);
        drop(snapshot);

        let revision_after = engine.control.global.revision.load(Ordering::Relaxed);
        let stored_content = engine.content(SOURCE).unwrap();
        assert_eq!(revision_after, revision_before);
        assert!(!engine.control.global.cancelled.load(Ordering::Relaxed));
        assert!(Arc::ptr_eq(&stored_content, &initial_content));
    }

    #[test]
    fn test_concurrent_input_writes_advance_revision_once() {
        const SOURCE: FileId = FileId::new(0);

        let engine = QueryEngine::default();
        engine.set_content(SOURCE, "module Main where\n\nvalue = 1");

        let revision_before = engine.control.global.revision.load(Ordering::Relaxed);
        let snapshot = engine.snapshot();
        let barrier = Barrier::new(3);
        let updated_a: Arc<str> = Arc::from("module Main where\n\nvalue = 2");
        let updated_b: Arc<str> = Arc::from("module Main where\n\nvalue = 2");
        assert!(!Arc::ptr_eq(&updated_a, &updated_b));

        std::thread::scope(|scope| {
            let setter_a = scope.spawn(|| {
                {
                    let shard = engine.input.content.shard(&SOURCE).read();
                    assert_eq!(
                        shard.get(&SOURCE).unwrap().value.as_ref(),
                        "module Main where\n\nvalue = 1"
                    );
                }
                barrier.wait();
                let shard = engine.input.content.shard(&SOURCE);
                engine.set_input_transaction(SOURCE, shard, updated_a);
            });
            let setter_b = scope.spawn(|| {
                {
                    let shard = engine.input.content.shard(&SOURCE).read();
                    assert_eq!(
                        shard.get(&SOURCE).unwrap().value.as_ref(),
                        "module Main where\n\nvalue = 1"
                    );
                }
                barrier.wait();
                let shard = engine.input.content.shard(&SOURCE);
                engine.set_input_transaction(SOURCE, shard, updated_b);
            });

            barrier.wait();
            drop(snapshot);
            setter_a.join().unwrap();
            setter_b.join().unwrap();
        });

        let revision_after = engine.control.global.revision.load(Ordering::Relaxed);
        let shard = engine.input.content.shard(&SOURCE).read();
        let state = shard.get(&SOURCE).unwrap();
        assert_eq!(revision_after, revision_before + 1);
        assert_eq!(state.changed, revision_after);
        assert_eq!(state.value.as_ref(), "module Main where\n\nvalue = 2");
    }

    #[test]
    fn test_equal_module_input_write_preserves_revision() {
        const MODULE_FILE: FileId = FileId::new(0);
        const REPLACEMENT_FILE: FileId = FileId::new(1);

        let engine = QueryEngine::default();
        engine.set_module_file("Main", MODULE_FILE);

        let module_name = engine.interned.module.lookup("Main").unwrap();
        let changed_before = {
            let shard = engine.input.module.shard(&module_name).read();
            shard.get(&module_name).unwrap().changed
        };
        let revision_before = engine.control.global.revision.load(Ordering::Relaxed);

        engine.set_module_file("Main", MODULE_FILE);

        let changed_after = {
            let shard = engine.input.module.shard(&module_name).read();
            shard.get(&module_name).unwrap().changed
        };
        let revision_after = engine.control.global.revision.load(Ordering::Relaxed);
        assert_eq!(changed_after, changed_before);
        assert_eq!(revision_after, revision_before);

        engine.set_module_file("Main", REPLACEMENT_FILE);

        let changed_after_replacement = {
            let shard = engine.input.module.shard(&module_name).read();
            shard.get(&module_name).unwrap().changed
        };
        let revision_after_replacement = engine.control.global.revision.load(Ordering::Relaxed);
        assert_eq!(changed_after_replacement, revision_after_replacement);
        assert_eq!(revision_after_replacement - revision_after, 1);
    }

    #[test]
    fn test_module_registration_invalidates_unresolved_import() {
        let engine = QueryEngine::default();
        let mut files = Files::default();

        let main = files.insert("Main.purs", "module Main where\n\nimport Library");
        let library = files.insert("Library.purs", "module Library where");

        engine.set_content(main, files.content(main));
        engine.set_content(library, files.content(library));

        let unresolved = engine.resolved(main).unwrap();
        assert!(!unresolved.unqualified.contains_key("Library"));

        engine.set_module_file("Library", library);

        let resolved = engine.resolved(main).unwrap();
        assert!(resolved.unqualified.contains_key("Library"));
    }

    #[test]
    fn test_remove_module_file() {
        let engine = QueryEngine::default();
        let mut files = Files::default();

        let main = files.insert("Main.purs", "module Main where\n\nimport Old\n\nvalue = imported");
        let library = files.insert("Library.purs", "module Old where\n\nimported = 42");
        let replacement = files.insert("Replacement.purs", "module Old where");

        engine.set_content(main, files.content(main));
        engine.set_content(library, files.content(library));
        engine.set_content(replacement, files.content(replacement));

        engine.set_module_file("Old", library);

        let resolved = engine.resolved(main).unwrap();
        assert!(resolved.unqualified.contains_key("Old"));

        engine.remove_module_file("Old", library);
        engine.set_module_file("New", library);

        assert_eq!(engine.module_file("Old"), None);
        assert_eq!(engine.module_file("New"), Some(library));

        let resolved = engine.resolved(main).unwrap();
        assert!(!resolved.unqualified.contains_key("Old"));

        engine.set_module_file("Old", replacement);
        engine.remove_module_file("Old", library);

        assert_eq!(engine.module_file("Old"), Some(replacement));
    }

    #[test]
    fn test_remove_file() {
        let engine = QueryEngine::default();
        let mut files = Files::default();

        let main = files.insert("Main.purs", "module Main where\n\nimport Library\n\nvalue = life");
        let library = files.insert("Library.purs", "module Library where\n\nlife = 42");
        engine.set_content(main, files.content(main));
        engine.set_content(library, files.content(library));
        engine.set_module_file("Main", main);
        engine.set_module_file("Library", library);

        let resolved = engine.resolved(main).unwrap();
        assert!(resolved.unqualified.contains_key("Library"));

        engine.remove_file(library);
        assert_eq!(engine.module_file("Library"), None);
        assert_eq!(engine.parsed(library), Err(QueryError::MissingContent { file_id: library }));

        let resolved = engine.resolved(main).unwrap();
        assert!(!resolved.unqualified.contains_key("Library"));

        assert_eq!(files.remove("Library.purs"), Some(library));
        let replacement = files.insert("Library.purs", "module Library where\n\nlife = 43");
        assert_ne!(replacement, library);
        engine.set_content(replacement, files.content(replacement));
        engine.set_module_file("Library", replacement);

        let resolved = engine.resolved(main).unwrap();
        assert!(resolved.unqualified.contains_key("Library"));
    }

    #[test]
    fn test_remove_file_discards_in_progress_waiter_edges() {
        let engine = QueryEngine::default();
        let file_id = FileId::new(0);
        engine.set_content(file_id, "module Main where");

        let computing = SnapshotId(1);
        let waiting = SnapshotId(2);
        let (_, promise) = Future::new();
        let waiter = Waiter { id: waiting, promise };
        let state = DerivedState::InProgress { id: computing, waiters: Mutex::new(vec![waiter]) };
        engine.derived.parsed.shard(&file_id).write().insert(file_id, state);
        assert!(engine.control.global.graph.lock().add_edge(waiting, computing));

        engine.remove_file(file_id);

        assert!(engine.control.global.graph.lock().add_edge(computing, waiting));
    }

    #[test]
    fn test_remove_files_discards_query_state() {
        let engine = QueryEngine::default();
        let mut files = Files::default();

        for number in 0..32 {
            let path = format!("Temporary{number}.purs");
            let module_name = format!("Temporary{number}");
            let content = format!("module {module_name} where");
            let file_id = files.insert(path.as_str(), content.as_str());
            engine.set_content(file_id, content);
            engine.set_module_file(&module_name, file_id);
            engine.parsed(file_id).unwrap();

            engine.remove_file(file_id);
            assert_eq!(files.remove(&path), Some(file_id));
        }

        let content_states = engine.input.content.inner.iter().map(|shard| shard.read().len());
        let module_states = engine.input.module.inner.iter().map(|shard| shard.read().len());
        let parsed_states = engine.derived.parsed.inner.iter().map(|shard| shard.read().len());
        assert_eq!(content_states.sum::<usize>(), 0);
        assert_eq!(module_states.sum::<usize>(), 0);
        assert_eq!(parsed_states.sum::<usize>(), 0);
    }

    #[test]
    fn test_pointer_equality() {
        let mut engine = QueryEngine::default();
        let mut files = Files::default();
        prim::configure(&mut engine, &mut files);

        let id = files.insert("./src/Main.purs", "module Main where\n\nlife = 42");
        let content = files.content(id);

        engine.set_content(id, content);
        let index_a = engine.indexed(id).unwrap();
        let index_b = engine.indexed(id).unwrap();
        assert!(Arc::ptr_eq(&index_a, &index_b));

        let id = files.insert("./src/Main.purs", "module Main where\n\nlife = 42\n\n");
        let content = files.content(id);

        engine.set_content(id, content);
        let index_a = engine.indexed(id).unwrap();
        let index_b = engine.indexed(id).unwrap();
        assert!(Arc::ptr_eq(&index_a, &index_b));
    }

    #[test]
    fn test_foreign_inputs() {
        let engine = QueryEngine::default();
        let mut files = Files::default();
        let mut foreign_files = ForeignFiles::default();

        let source = "module Main where\n\nforeign import life :: Int";
        let source_id = files.insert("src/Main.purs", source);
        let first_id = foreign_files.insert(
            ForeignSourceKind::JavaScript,
            "src/Main.js",
            "export const life = 1;",
        );
        let second_id = foreign_files.insert(
            ForeignSourceKind::JavaScript,
            "src/Other.js",
            "export const life = 2;",
        );
        engine.set_content(source_id, source);

        assert_eq!(engine.foreign_file(source_id), None);

        engine.set_foreign_content(first_id, foreign_files.content(first_id));
        engine.set_foreign_file(source_id, first_id);
        assert_eq!(engine.foreign_file(source_id), Some(first_id));
        assert_eq!(engine.foreign_content(first_id).as_deref(), Some("export const life = 1;"));
        assert!(engine.foreign_validation(source_id).unwrap().errors.is_empty());

        engine.set_foreign_content(second_id, foreign_files.content(second_id));
        engine.set_foreign_file(source_id, second_id);
        engine.remove_foreign_file(first_id);
        assert_eq!(engine.foreign_file(source_id), Some(second_id));
        assert_eq!(engine.foreign_content(first_id), None);
        assert_eq!(engine.foreign_module(first_id).unwrap(), None);
        assert!(
            !engine.derived.foreign_validation.shard(&source_id).read().contains_key(&source_id)
        );
        assert!(engine.foreign_validation(source_id).unwrap().errors.is_empty());

        engine.remove_foreign_file(second_id);
        assert_eq!(engine.foreign_file(source_id), None);
        let validation = engine.foreign_validation(source_id).unwrap();
        assert!(matches!(validation.errors.as_ref(), [ForeignError::MissingModule { .. }]));
    }

    #[test]
    fn test_foreign_validation_invalidation() {
        let engine = QueryEngine::default();
        let mut files = Files::default();
        let mut foreign_files = ForeignFiles::default();

        let source_id =
            files.insert("src/Main.purs", "module Main where\n\nforeign import life :: Int");
        engine.set_content(source_id, files.content(source_id));

        let missing_module = engine.foreign_validation(source_id).unwrap();
        assert!(matches!(
            missing_module.errors.as_ref(),
            [ForeignError::MissingModule { name, .. }] if name == "life"
        ));

        let foreign_id = foreign_files.insert(
            ForeignSourceKind::JavaScript,
            "src/Main.js",
            "export const other = 42;",
        );
        engine.set_foreign_content(foreign_id, foreign_files.content(foreign_id));
        engine.set_foreign_file(source_id, foreign_id);

        let missing_implementation = engine.foreign_validation(source_id).unwrap();
        assert!(matches!(
            missing_implementation.errors.as_ref(),
            [ForeignError::MissingImplementation { name, .. }] if name == "life"
        ));

        engine.set_foreign_content(foreign_id, "export const life = 42;");
        let valid = engine.foreign_validation(source_id).unwrap();
        assert!(valid.errors.is_empty());

        engine.set_foreign_content(
            foreign_id,
            "// The implementation changed.\nexport const life = 43;",
        );
        let body_changed = engine.foreign_validation(source_id).unwrap();
        assert!(Arc::ptr_eq(&valid, &body_changed));

        engine.remove_foreign_file(foreign_id);
        assert_eq!(foreign_files.remove("src/Main.js"), Some(foreign_id));

        for number in 0..32 {
            let path = format!("src/Main-{number}.js");
            let foreign_id = foreign_files.insert(
                ForeignSourceKind::JavaScript,
                path.as_str(),
                "export const life = 42;",
            );
            engine.set_foreign_content(foreign_id, foreign_files.content(foreign_id));
            engine.set_foreign_file(source_id, foreign_id);

            let valid = engine.foreign_validation(source_id).unwrap();
            assert!(valid.errors.is_empty());

            engine.remove_foreign_file(foreign_id);
            assert_eq!(foreign_files.remove(&path), Some(foreign_id));
            assert!(
                !engine.input.foreign_content.shard(&foreign_id).read().contains_key(&foreign_id)
            );
            assert!(
                !engine.derived.foreign_module.shard(&foreign_id).read().contains_key(&foreign_id)
            );
            assert!(
                !engine
                    .derived
                    .foreign_validation
                    .shard(&source_id)
                    .read()
                    .contains_key(&source_id)
            );

            let missing_module = engine.foreign_validation(source_id).unwrap();
            assert!(matches!(
                missing_module.errors.as_ref(),
                [ForeignError::MissingModule { name, .. }] if name == "life"
            ));
        }
    }

    #[test]
    fn test_foreign_validation_rejects_javascript_and_jsx_candidates() {
        let engine = QueryEngine::default();
        let mut files = Files::default();
        let mut foreign_files = ForeignFiles::default();

        let source_id =
            files.insert("src/Main.purs", "module Main where\n\nforeign import life :: Int");
        engine.set_content(source_id, files.content(source_id));
        for kind in ForeignSourceKind::ALL {
            let path = format!("src/Main.{}", kind.extension());
            let foreign_id = foreign_files.insert(kind, path, "export const life = 42;");
            engine.set_foreign_content(foreign_id, foreign_files.content(foreign_id));
            engine.set_foreign_file(source_id, foreign_id);
        }

        assert_eq!(engine.foreign_file(source_id), None);
        let validation = engine.foreign_validation(source_id).unwrap();
        assert!(matches!(validation.errors.as_ref(), [ForeignError::AmbiguousModule { .. }]));
    }

    #[test]
    fn test_unparseable_foreign_modules_skip_export_validation() {
        let engine = QueryEngine::default();
        let mut files = Files::default();
        let mut foreign_files = ForeignFiles::default();

        let source = "module Main where\n\nforeign import _null :: Int\nforeign import registerVersionProvider :: Int";
        let source_id = files.insert("src/Main.purs", source);
        engine.set_content(source_id, files.content(source_id));

        let malformed_modules =
            ["export _null = null;", "export const registerVersionProvider u => u;"];
        for (index, content) in malformed_modules.into_iter().enumerate() {
            let path = format!("src/Main-{index}.js");
            let foreign_id = foreign_files.insert(ForeignSourceKind::JavaScript, path, content);
            engine.set_foreign_content(foreign_id, foreign_files.content(foreign_id));
            engine.set_foreign_file(source_id, foreign_id);

            let validation = engine.foreign_validation(source_id).unwrap();
            assert!(!validation.errors.is_empty());
            assert!(
                validation.errors.iter().all(|error| matches!(error, ForeignError::Parse { .. }))
            );
        }
    }

    #[test]
    fn test_backend_queries_cache_and_invalidate() {
        let mut engine = QueryEngine::default();
        let mut files = Files::default();
        prim::configure(&mut engine, &mut files);

        let main = files.insert("Main.purs", "module Main where\n\nlife = 42");
        engine.set_content(main, files.content(main));
        engine.set_module_file("Main", main);

        let functional_initial = engine.functional(main).unwrap().unwrap();
        let javascript_initial = engine.javascript(main).unwrap().unwrap();
        let functional_repeated = engine.functional(main).unwrap().unwrap();
        let javascript_repeated = engine.javascript(main).unwrap().unwrap();
        assert!(Arc::ptr_eq(&functional_initial, &functional_repeated));
        assert!(Arc::ptr_eq(&javascript_initial, &javascript_repeated));

        {
            let shard = engine.derived.functional.shard(&main).read();
            let DerivedState::Computed { dependencies, .. } = shard.get(&main).unwrap() else {
                unreachable!("invariant violated: expected computed query");
            };
            assert!(dependencies.contains(&QueryKey::Exported(main)));
            assert!(dependencies.contains(&QueryKey::Indexed(main)));
            assert!(dependencies.contains(&QueryKey::Lowered(main)));
            assert!(dependencies.contains(&QueryKey::Grouped(main)));
            assert!(dependencies.contains(&QueryKey::Checked(main)));
        }
        {
            let shard = engine.derived.javascript.shard(&main).read();
            let DerivedState::Computed { dependencies, .. } = shard.get(&main).unwrap() else {
                unreachable!("invariant violated: expected computed query");
            };
            assert_eq!(
                dependencies.as_ref(),
                &[QueryKey::Functional(main), QueryKey::Foreign(main)]
            );
        }

        let unrelated = files.insert("Unrelated.purs", "module Unrelated where\n\nvalue = 1");
        engine.set_content(unrelated, files.content(unrelated));
        engine.set_module_file("Unrelated", unrelated);

        let functional_after_unrelated = engine.functional(main).unwrap().unwrap();
        let javascript_after_unrelated = engine.javascript(main).unwrap().unwrap();
        assert!(Arc::ptr_eq(&functional_initial, &functional_after_unrelated));
        assert!(Arc::ptr_eq(&javascript_initial, &javascript_after_unrelated));

        engine.set_content(main, "module Main where\n\nlife = 43");

        let functional_changed = engine.functional(main).unwrap().unwrap();
        let javascript_changed = engine.javascript(main).unwrap().unwrap();
        assert!(!Arc::ptr_eq(&functional_initial, &functional_changed));
        assert!(!Arc::ptr_eq(&javascript_initial, &javascript_changed));
        assert_ne!(functional_initial, functional_changed);
        assert_ne!(javascript_initial, javascript_changed);
    }

    #[test]
    fn test_backend_queries_invalidate_conversion_errors() {
        let mut engine = QueryEngine::default();
        let mut files = Files::default();
        prim::configure(&mut engine, &mut files);

        let main = files.insert(
            "Main.purs",
            "module Main where\n\nimport Iris.StyleX (Props, Style, props)\n\npartialProps :: Style -> Props\npartialProps = props",
        );
        engine.set_content(main, files.content(main));
        engine.set_module_file("Main", main);

        let functional_error = engine.functional(main).unwrap().unwrap_err();
        assert!(matches!(functional_error, functional::ModuleError::Unsupported { .. }));

        let javascript_error = engine.javascript(main).unwrap().unwrap_err();
        assert!(matches!(
            javascript_error,
            javascript::ModuleError::Functional(functional::ModuleError::Unsupported { .. })
        ));

        engine.set_content(main, "module Main where\n\nlife = 42");

        engine.functional(main).unwrap().unwrap();
        engine.javascript(main).unwrap().unwrap();
    }

    #[test]
    fn test_checked_core_sharing_and_invalidation() {
        let mut engine = QueryEngine::default();
        let mut files = Files::default();
        prim::configure(&mut engine, &mut files);

        let main = files.insert(
            "Main.purs",
            "module Main where\n\nimport Data.Eq (class Eq)\n\ndata Value = Value\n\nderive instance Eq Value",
        );
        engine.set_content(main, files.content(main));
        engine.set_module_file("Main", main);

        let checked_initial = engine.checked(main).unwrap();
        let core_initial = engine.checked_core().unwrap();
        let core_repeated = engine.checked_core().unwrap();
        assert!(Arc::ptr_eq(&core_initial, &core_repeated));

        {
            let snapshot = engine.snapshot();
            let core_snapshot = snapshot.checked_core().unwrap();
            assert!(Arc::ptr_eq(&core_initial, &core_snapshot));
        }

        let unrelated = files.insert("Unrelated.purs", "module Unrelated where\n\nvalue = 2");
        engine.set_content(unrelated, files.content(unrelated));
        engine.set_module_file("Unrelated", unrelated);

        let core_after_unrelated = engine.checked_core().unwrap();
        let checked_after_unrelated = engine.checked(main).unwrap();
        assert!(Arc::ptr_eq(&core_initial, &core_after_unrelated));
        assert!(Arc::ptr_eq(&checked_initial, &checked_after_unrelated));

        {
            let data_eq_name = engine.interned.module.lookup("Data.Eq").unwrap();
            let unrelated_name = engine.interned.module.lookup("Unrelated").unwrap();
            let shard = engine.derived.checked_core.shard(&()).read();
            let DerivedState::Computed { dependencies, .. } = shard.get(&()).unwrap() else {
                unreachable!("invariant violated: expected computed query");
            };
            assert!(dependencies.contains(&QueryKey::Module(data_eq_name)));
            assert!(dependencies.contains(&QueryKey::Indexed(core_initial.prim.prim_id)));
            assert!(dependencies.contains(&QueryKey::Resolved(core_initial.prim.prim_id)));
            assert!(!dependencies.contains(&QueryKey::Module(unrelated_name)));
            assert!(!dependencies.contains(&QueryKey::Content(unrelated)));
        }

        let data_eq = files.insert(
            "Data.Eq.purs",
            "module Data.Eq where\n\nclass Eq a where\n  eq :: a -> a -> Boolean",
        );
        engine.set_content(data_eq, files.content(data_eq));
        engine.set_module_file("Data.Eq", data_eq);

        let core_with_eq = engine.checked_core().unwrap();
        assert!(!Arc::ptr_eq(&core_after_unrelated, &core_with_eq));
        assert!(core_with_eq.known_types.eq.is_some());
        assert!(core_with_eq.known_terms.eq.is_some());

        let checked_with_eq = engine.checked(main).unwrap();
        assert!(!Arc::ptr_eq(&checked_initial, &checked_with_eq));
        assert_ne!(checked_initial, checked_with_eq);

        let checked_trace_with_eq = {
            let shard = engine.derived.checked.shard(&main).read();
            let DerivedState::Computed { trace, dependencies, .. } = shard.get(&main).unwrap()
            else {
                unreachable!("invariant violated: expected computed query");
            };
            assert!(dependencies.contains(&QueryKey::CheckedCore));
            *trace
        };

        engine.set_content(
            data_eq,
            "module Data.Eq where\n\nclass Eq a where\n  eq :: a -> a -> Boolean\n\nclass Eq1 f where\n  eq1 :: forall a. Eq a => f a -> f a -> Boolean",
        );

        let checked_with_eq1 = engine.checked(main).unwrap();
        let (core_with_eq1, core_trace_with_eq1) = {
            let shard = engine.derived.checked_core.shard(&()).read();
            let DerivedState::Computed { computed, trace, .. } = shard.get(&()).unwrap() else {
                unreachable!("invariant violated: expected computed query");
            };
            (Arc::clone(computed), *trace)
        };
        let revision = engine.control.global.revision.load(Ordering::Relaxed);
        assert_eq!(core_trace_with_eq1.built, revision);
        assert!(!Arc::ptr_eq(&core_with_eq, &core_with_eq1));
        assert!(core_with_eq1.known_types.eq1.is_some());
        assert!(core_with_eq1.known_terms.eq1.is_some());

        let core_with_eq1_repeated = engine.checked_core().unwrap();
        assert!(Arc::ptr_eq(&core_with_eq1, &core_with_eq1_repeated));
        assert!(Arc::ptr_eq(&checked_with_eq, &checked_with_eq1));

        let checked_trace_with_eq1 = {
            let shard = engine.derived.checked.shard(&main).read();
            let DerivedState::Computed { trace, .. } = shard.get(&main).unwrap() else {
                unreachable!("invariant violated: expected computed query");
            };
            *trace
        };
        assert!(checked_trace_with_eq1.built > checked_trace_with_eq.built);
        assert_eq!(checked_trace_with_eq1.changed, checked_trace_with_eq.changed);
    }

    #[test]
    fn test_indexed_depends_on_source_text() {
        let mut engine = QueryEngine::default();
        let mut files = Files::default();
        prim::configure(&mut engine, &mut files);

        let id = files.insert("./src/Main.purs", "module Main where\n\nlife = 42");
        engine.set_content(id, files.content(id));
        let indexed = engine.indexed(id).unwrap();
        assert!(indexed.names.terms.lookup("life").is_some());

        let id = files.insert("./src/Main.purs", "module Main where\n\ntime = 42");
        engine.set_content(id, files.content(id));
        let indexed = engine.indexed(id).unwrap();
        assert!(indexed.names.terms.lookup("life").is_none());
        assert!(indexed.names.terms.lookup("time").is_some());
    }

    #[test]
    fn test_text_edit_preserves_structural_query_traces() {
        let mut engine = QueryEngine::default();
        let mut files = Files::default();
        prim::configure(&mut engine, &mut files);

        macro_rules! assert_trace {
            ($engine:expr, $field:ident($id:expr) => $trace:expr) => {{
                let shard = $engine.derived.$field.shard(&$id);
                let guard = shard.read();
                assert_eq!(ShowTrace(guard.get(&$id).unwrap()), $trace);
            }};
        }

        let id = files.insert("./src/Main.purs", "module Main where\n\nlife = 42");
        engine.set_content(id, files.content(id));
        let stabilized_a = engine.stabilized(id).unwrap();
        let indexed_a = engine.indexed(id).unwrap();

        assert_trace!(engine, parsed(id) => Trace {
            built: 25,
            changed: 25,
            dependencies: &[QueryKey::Content(id)]
        });
        assert_trace!(engine, stabilized(id) => Trace {
            built: 25,
            changed: 25,
            dependencies: &[QueryKey::Parsed(id)]
        });
        assert_trace!(engine, indexed(id) => Trace {
            built: 25,
            changed: 25,
            dependencies: &[QueryKey::Content(id), QueryKey::Parsed(id), QueryKey::Stabilized(id)]
        });

        let id = files.insert("./src/Main.purs", "module Main where\n\ntime = 42");
        engine.set_content(id, files.content(id));
        let stabilized_b = engine.stabilized(id).unwrap();
        let indexed_b = engine.indexed(id).unwrap();

        assert_trace!(engine, parsed(id) => Trace {
            built: 26,
            changed: 25,
            dependencies: &[QueryKey::Content(id)]
        });
        assert_trace!(engine, stabilized(id) => Trace {
            built: 26,
            changed: 25,
            dependencies: &[QueryKey::Parsed(id)]
        });
        assert_trace!(engine, indexed(id) => Trace {
            built: 26,
            changed: 26,
            dependencies: &[QueryKey::Content(id), QueryKey::Parsed(id), QueryKey::Stabilized(id)]
        });

        assert!(Arc::ptr_eq(&stabilized_a, &stabilized_b));
        assert!(!Arc::ptr_eq(&indexed_a, &indexed_b));
    }

    #[test]
    fn test_verifying_step_traces() {
        let mut engine = QueryEngine::default();
        let mut files = Files::default();
        prim::configure(&mut engine, &mut files);

        macro_rules! assert_trace {
            ($engine:expr, $field:ident($id:expr) => $trace:expr) => {{
                let shard = $engine.derived.$field.shard(&$id);
                let guard = shard.read();
                assert_eq!(ShowTrace(guard.get(&$id).unwrap()), $trace);
            }};
        }

        let id = files.insert("./src/Main.purs", "module Main where\n\nlife = 42");
        let content = files.content(id);

        engine.set_content(id, content);
        let indexed_a = engine.indexed(id).unwrap();
        let lowered_a = engine.lowered(id).unwrap();
        let resolved_a = engine.resolved(id).unwrap();

        assert_trace!(engine, parsed(id) => Trace {
            built: 25,
            changed: 25,
            dependencies: &[QueryKey::Content(id)]
        });
        assert_trace!(engine, indexed(id) => Trace {
            built: 25,
            changed: 25,
            dependencies: &[QueryKey::Content(id), QueryKey::Parsed(id), QueryKey::Stabilized(id)]
        });
        assert_trace!(engine, resolved(id) => Trace {
            built: 25,
            changed: 25,
            dependencies: &[QueryKey::Indexed(id)]
        });

        let id = files.insert("./src/Main.purs", "module Main where\n\n\n\nlife = 42");
        let content = files.content(id);

        engine.set_content(id, content);
        let indexed_b = engine.indexed(id).unwrap();
        let lowered_b = engine.lowered(id).unwrap();
        let resolved_b = engine.resolved(id).unwrap();

        assert_trace!(engine, parsed(id) => Trace {
            built: 26,
            changed: 26,
            dependencies: &[QueryKey::Content(id)]
        });
        assert_trace!(engine, indexed(id) => Trace {
            built: 26,
            changed: 25,
            dependencies: &[QueryKey::Content(id), QueryKey::Parsed(id), QueryKey::Stabilized(id)]
        });
        assert_trace!(engine, resolved(id) => Trace {
            built: 26,
            changed: 25,
            dependencies: &[QueryKey::Indexed(id)]
        });

        let id = files.insert("./src/Main.purs", "module Main where\n\n\n\nlife = 42\n\n");
        let content = files.content(id);

        engine.set_content(id, content);
        let indexed_c = engine.indexed(id).unwrap();
        let lowered_c = engine.lowered(id).unwrap();
        let resolved_c = engine.resolved(id).unwrap();

        assert_trace!(engine, parsed(id) => Trace {
            built: 27,
            changed: 27,
            dependencies: &[QueryKey::Content(id)]
        });
        assert_trace!(engine, indexed(id) => Trace {
            built: 27,
            changed: 25,
            dependencies: &[QueryKey::Content(id), QueryKey::Parsed(id), QueryKey::Stabilized(id)]
        });
        assert_trace!(engine, resolved(id) => Trace {
            built: 27,
            changed: 25,
            dependencies: &[QueryKey::Indexed(id)]
        });

        assert!(Arc::ptr_eq(&indexed_a, &indexed_b));
        assert!(Arc::ptr_eq(&indexed_b, &indexed_c));

        assert!(Arc::ptr_eq(&lowered_a, &lowered_b));
        assert!(Arc::ptr_eq(&lowered_b, &lowered_c));

        assert!(Arc::ptr_eq(&resolved_a, &resolved_b));
        assert!(Arc::ptr_eq(&resolved_b, &resolved_c));
    }

    #[test]
    fn test_local_state_cleanup() {
        let mut engine = QueryEngine::default();
        let mut files = Files::default();
        prim::configure(&mut engine, &mut files);

        let id = files.insert("./src/Main.purs", "module Main where\n\n\n\nlife = 42");
        let content = files.content(id);

        engine.set_content(id, content);
        let key = QueryKey::Parsed(id);

        let indexed_a = engine.indexed(id).unwrap();
        assert!(!engine.control.local.contains_in_progress(key));

        let indexed_b = engine.indexed(id).unwrap();
        assert!(!engine.control.local.contains_in_progress(key));

        assert_eq!(indexed_a, indexed_b);
    }

    #[test]
    fn test_nested_dependencies_are_deduplicated_and_isolated() {
        let mut engine = QueryEngine::default();
        let mut files = Files::default();
        prim::configure(&mut engine, &mut files);

        let parent = files.insert("./src/Parent.purs", "module Parent where");
        let child = files.insert("./src/Child.purs", "module Child where");
        engine.set_content(child, files.content(child));

        let parsed = engine
            .query(
                QueryKey::Parsed(parent),
                parent,
                |derived| &derived.parsed,
                |engine| {
                    let parsed = engine.parsed(child)?;
                    engine.parsed(child)?;
                    Ok(parsed)
                },
            )
            .unwrap();
        assert_eq!(parsed, engine.parsed(child).unwrap());

        let shard = engine.derived.parsed.shard(&parent);
        let guard = shard.read();
        assert_eq!(
            ShowTrace(guard.get(&parent).unwrap()),
            Trace { built: 25, changed: 25, dependencies: &[QueryKey::Parsed(child)] }
        );
    }

    #[test]
    fn test_recomputation_replaces_dependencies() {
        let mut engine = QueryEngine::default();
        let mut files = Files::default();
        prim::configure(&mut engine, &mut files);

        let parent = files.insert("./src/Parent.purs", "module Parent where");
        let child_a = files.insert("./src/ChildA.purs", "module ChildA where\n\nvalue = 1");
        let child_b = files.insert("./src/ChildB.purs", "module ChildB where\n\nvalue = 2");
        engine.set_content(child_a, files.content(child_a));
        engine.set_content(child_b, files.content(child_b));

        engine
            .query(
                QueryKey::Parsed(parent),
                parent,
                |derived| &derived.parsed,
                |engine| engine.parsed(child_a),
            )
            .unwrap();

        engine.set_content(child_a, "module ChildA where\n\nvalue = 3\nother = 4");
        let parsed_b = engine
            .query(
                QueryKey::Parsed(parent),
                parent,
                |derived| &derived.parsed,
                |engine| engine.parsed(child_b),
            )
            .unwrap();

        {
            let shard = engine.derived.parsed.shard(&parent);
            let guard = shard.read();
            let Some(DerivedState::Computed { dependencies, .. }) = guard.get(&parent) else {
                panic!("invariant violated: expected computed parent query");
            };
            assert_eq!(dependencies.as_ref(), &[QueryKey::Parsed(child_b)]);
        }

        engine.set_content(child_a, "module ChildA where\n\nvalue = 4\nother = 5\nthird = 6");
        let cached = engine
            .query(
                QueryKey::Parsed(parent),
                parent,
                |derived| &derived.parsed,
                |_| unreachable!("removed dependency should not cause recomputation"),
            )
            .unwrap();
        assert_eq!(cached, parsed_b);
    }

    #[test]
    fn test_query_completion_preserves_unrelated_waiter_edges() {
        let mut engine = QueryEngine::default();
        let mut files = Files::default();
        prim::configure(&mut engine, &mut files);

        let parent = files.insert("./src/Parent.purs", "module Parent where");
        let child = files.insert("./src/Child.purs", "module Child where");
        engine.set_content(child, files.content(child));
        let computed = engine.parsed(child).unwrap();

        let computing = SnapshotId(1);
        let waiting = SnapshotId(2);
        let shard = engine.derived.parsed.shard(&parent);
        shard.write().insert(parent, DerivedState::in_progress(computing));

        assert!(engine.control.global.graph.lock().add_edge(waiting, computing));
        engine.fulfill_and_store(
            parent,
            &|derived| &derived.parsed,
            computed,
            super::Trace { built: 19, changed: 19 },
            Arc::from([]),
        );
        assert!(!engine.control.global.graph.lock().add_edge(computing, waiting));
    }

    #[test]
    fn test_cancellation_cleanup() {
        let mut engine = QueryEngine::default();
        let mut files = Files::default();
        prim::configure(&mut engine, &mut files);

        let id = files.insert("./src/Main.purs", "module Main where\n\n\n\nlife = 42");
        let key = QueryKey::Indexed(id);

        let result =
            engine.query(key, id, |derived| &derived.indexed, |_| Err(QueryError::Cancelled));

        assert_eq!(result, Err(QueryError::Cancelled));

        // Observe that the storage has been edited.
        {
            let shard = engine.derived.indexed.shard(&id);
            assert!(!shard.read().contains_key(&id));
        }
    }

    #[test]
    fn test_cancellation_no_cleanup() {
        let mut engine = QueryEngine::default();
        let mut files = Files::default();
        prim::configure(&mut engine, &mut files);

        let id = files.insert("./src/Main.purs", "module Main where\n\n\n\nlife = 42");
        let key = QueryKey::Indexed(id);

        // Simulate the current thread starting a computation.
        {
            let shard = engine.derived.indexed.shard(&id);
            shard.write().insert(id, DerivedState::in_progress(engine.control.id));
        }

        // Finally, enable cancellation and run the query on another thread.
        engine.control.global.cancelled.store(true, Ordering::Relaxed);
        let result = std::thread::scope(|scope| {
            let runtime = engine.snapshot();
            let thread = scope.spawn(move || {
                runtime.query(key, id, |derived| &derived.indexed, |_| unreachable!("impossible."))
            });
            thread.join().unwrap()
        });

        assert_eq!(result, Err(QueryError::Cancelled));

        // Observe that the storage is not edited.
        {
            let shard = engine.derived.indexed.shard(&id);
            assert!(shard.read().contains_key(&id));
        }
    }

    #[test]
    fn test_cycle_detection() {
        const ID: FileId = FileId::new(0);
        const KEY: QueryKey = QueryKey::Resolved(ID);

        fn fake_query_a(engine: &QueryEngine) -> QueryResult<Arc<ResolvedModule>> {
            engine.query(QueryKey::Resolved(ID), ID, |derived| &derived.resolved, fake_query_a)
        }

        let engine = QueryEngine::default();
        let result = fake_query_a(&engine);
        assert_eq!(result, Err(QueryError::Cycle { stack: [KEY, KEY].into() }));
    }

    #[test]
    fn test_cycle_recovery() {
        const ID: FileId = FileId::new(0);

        fn fake_query_a(engine: &QueryEngine) -> QueryResult<Arc<ResolvedModule>> {
            engine.query(
                QueryKey::Resolved(ID),
                ID,
                |derived| &derived.resolved,
                |engine| fake_query_a(engine).map_err(|_| QueryError::Cancelled),
            )
        }

        let engine = QueryEngine::default();
        let result = fake_query_a(&engine);
        assert!(matches!(result, Err(QueryError::Cancelled)));
    }

    #[test]
    fn test_snapshot_cycle_detection() {
        const ID_A: FileId = FileId::new(0);
        const ID_B: FileId = FileId::new(1);

        fn fake_query_a(engine: &QueryEngine) -> QueryResult<Arc<ResolvedModule>> {
            engine.query(QueryKey::Resolved(ID_A), ID_A, |derived| &derived.resolved, fake_query_b)
        }

        fn fake_query_b(engine: &QueryEngine) -> QueryResult<Arc<ResolvedModule>> {
            engine.query(QueryKey::Resolved(ID_B), ID_B, |derived| &derived.resolved, fake_query_a)
        }

        let engine = QueryEngine::default();

        let snapshot = engine.snapshot();
        let thread = std::thread::spawn(move || fake_query_b(&snapshot));

        let result_a = fake_query_a(&engine);
        let result_b = thread.join().unwrap();

        assert!(result_a.is_err());
        assert!(result_b.is_err());

        // Either result can return `Cancelled`, but at least one of should be `Cycle`
        assert!(
            [result_a, result_b]
                .iter()
                .any(|result| matches!(result, Err(QueryError::Cycle { .. })))
        );
    }

    #[test]
    fn test_snapshot_cycle_recovery() {
        const ID_A: FileId = FileId::new(0);
        const ID_B: FileId = FileId::new(1);

        fn fake_query_a(engine: &QueryEngine) -> QueryResult<Arc<ResolvedModule>> {
            engine.query(
                QueryKey::Resolved(ID_A),
                ID_A,
                |derived| &derived.resolved,
                |engine| fake_query_b(engine).map_err(|_| QueryError::Cancelled),
            )
        }

        fn fake_query_b(engine: &QueryEngine) -> QueryResult<Arc<ResolvedModule>> {
            engine.query(
                QueryKey::Resolved(ID_B),
                ID_B,
                |derived| &derived.resolved,
                |engine| fake_query_a(engine).map_err(|_| QueryError::Cancelled),
            )
        }

        let engine = QueryEngine::default();

        let snapshot = engine.snapshot();
        let thread = std::thread::spawn(move || fake_query_b(&snapshot));

        let result_a = fake_query_a(&engine);
        let result_b = thread.join().unwrap();

        assert!(matches!(result_a, Err(QueryError::Cancelled)));
        assert!(matches!(result_b, Err(QueryError::Cancelled)));
    }

    #[test]
    fn test_resolving_cycle() {
        let mut engine = QueryEngine::default();
        let mut files = Files::default();
        prim::configure(&mut engine, &mut files);

        let main = files.insert("Main.purs", "module Main where\n\nimport Lib (b)\n\na = 123");
        let library = files.insert("Lib.purs", "module Lib where\n\nimport Main (a)\n\nb = 123");

        engine.set_content(main, files.content(main));
        engine.set_content(library, files.content(library));
        engine.set_module_file("Main", main);
        engine.set_module_file("Lib", library);

        let result_a = engine.resolved(main);
        assert_eq!(
            result_a,
            Err(QueryError::Cycle {
                stack: [
                    QueryKey::Resolved(main),
                    QueryKey::Resolved(library),
                    QueryKey::Resolved(main)
                ]
                .into()
            })
        );

        let result_b = engine.resolved(library);
        assert_eq!(
            result_b,
            Err(QueryError::Cycle {
                stack: [
                    QueryKey::Resolved(library),
                    QueryKey::Resolved(main),
                    QueryKey::Resolved(library)
                ]
                .into()
            })
        );
    }

    #[test]
    fn test_grouped_identity() {
        let mut engine = QueryEngine::default();
        let mut files = Files::default();
        prim::configure(&mut engine, &mut files);

        let id = files.insert("./src/Main.purs", "module Main where\n\nx = y\ny = 1");
        let content = files.content(id);
        engine.set_content(id, content);

        let groups_a = engine.grouped(id).unwrap();
        let groups_b = engine.grouped(id).unwrap();
        assert!(Arc::ptr_eq(&groups_a, &groups_b));
    }

    #[test]
    fn test_exported_identity() {
        let mut engine = QueryEngine::default();
        let mut files = Files::default();
        prim::configure(&mut engine, &mut files);

        let id = files.insert("./src/Main.purs", "module Main (x) where\n\nx = 1\ny = 2");
        let content = files.content(id);
        engine.set_content(id, content);

        let exported_a = engine.exported(id).unwrap();
        let exported_b = engine.exported(id).unwrap();
        assert!(Arc::ptr_eq(&exported_a, &exported_b));
        let indexed = engine.indexed(id).unwrap();
        let x = indexed.names.terms.lookup("x").unwrap();
        assert_eq!(exported_a.local.as_ref(), &[x]);
        assert!(exported_a.indirect.is_empty());

        let shard = engine.derived.exported.shard(&id).read();
        let DerivedState::Computed { dependencies, .. } = shard.get(&id).unwrap() else {
            unreachable!("invariant violated: expected computed query");
        };
        assert_eq!(dependencies.as_ref(), &[QueryKey::Resolved(id)]);
    }

    #[test]
    fn test_lowered_identity() {
        let mut engine = QueryEngine::default();
        let mut files = Files::default();
        prim::configure(&mut engine, &mut files);

        let id = files.insert("./src/Main.purs", "module Main where\n\nx = 1");
        let content = files.content(id);
        engine.set_content(id, content);

        let lowered_a = engine.lowered(id).unwrap();
        let lowered_b = engine.lowered(id).unwrap();
        assert!(Arc::ptr_eq(&lowered_a, &lowered_b));
    }

    #[test]
    fn test_grouped_stable() {
        let mut engine = QueryEngine::default();
        let mut files = Files::default();
        prim::configure(&mut engine, &mut files);

        let id = files.insert("./src/Main.purs", "module Main where\n\nx = 1");
        engine.set_content(id, files.content(id));
        let groups_a = engine.grouped(id).unwrap();

        let id = files.insert("./src/Main.purs", "module Main where\n\n\n\nx = 1");
        engine.set_content(id, files.content(id));
        let groups_b = engine.grouped(id).unwrap();

        assert_eq!(groups_a.term_scc, groups_b.term_scc);
        assert_eq!(groups_a.type_scc, groups_b.type_scc);
    }
}
