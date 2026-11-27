use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use itertools::Itertools;
use parking_lot::Mutex;
use petgraph::algo::kosaraju_scc;
use petgraph::graph::{DiGraph, NodeIndex};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TaskId(usize);

impl TaskId {
    pub fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Default)]
pub struct TaskGraph {
    costs: Vec<u64>,
    continuations: Vec<Option<TaskId>>,
    dependencies: Vec<(TaskId, TaskId)>,
}

impl TaskGraph {
    pub fn add_task(&mut self, cost: u64) -> TaskId {
        let id = TaskId(self.costs.len());
        self.costs.push(cost);
        self.continuations.push(None);
        id
    }

    pub fn add_continuation(&mut self, task: TaskId, continuation: TaskId) {
        self.continuations[task.0] = Some(continuation);
        self.add_dependency(continuation, task);
    }

    pub fn add_dependency(&mut self, task: TaskId, dependency: TaskId) {
        self.dependencies.push((task, dependency));
    }
}

#[derive(Clone, Copy)]
struct ComponentId(usize);

struct Schedule<'a> {
    continuations: &'a [Option<TaskId>],
    components: Vec<Vec<TaskId>>,
    dependents: Vec<Vec<ComponentId>>,
    dependency_counts: Vec<usize>,
    priorities: Vec<u64>,
}

impl<'a> Schedule<'a> {
    fn new(graph: &'a TaskGraph) -> Schedule<'a> {
        let task_count = graph.costs.len();
        let mut dependency_graph =
            DiGraph::<(), (), u32>::with_capacity(task_count, graph.dependencies.len());
        for _ in 0..task_count {
            dependency_graph.add_node(());
        }
        for &(task, dependency) in &graph.dependencies {
            dependency_graph.add_edge(NodeIndex::new(task.0), NodeIndex::new(dependency.0), ());
        }

        let components = kosaraju_scc(&dependency_graph);
        let mut component_of = vec![0; task_count];
        for (component, tasks) in components.iter().enumerate() {
            for task in tasks {
                component_of[task.index()] = component;
            }
        }

        let mut dependents = vec![Vec::new(); task_count];
        let mut dependency_counts = vec![0; components.len()];
        for &(task, dependency) in &graph.dependencies {
            let component = component_of[task.0];
            if component != component_of[dependency.0] {
                dependents[dependency.0].push(ComponentId(component));
                dependency_counts[component] += 1;
            }
        }

        let mut priorities = vec![0; task_count];
        let mut component_priorities = vec![0; components.len()];
        for (component, tasks) in components.iter().enumerate().rev() {
            for task in tasks {
                let task = task.index();
                let downstream = dependents[task].iter();
                let downstream = downstream.map(|dependent| component_priorities[dependent.0]);
                let downstream = downstream.max().unwrap_or(0);
                priorities[task] = graph.costs[task].saturating_add(downstream);
            }
            let component_priority = tasks.iter().map(|task| priorities[task.index()]).max();
            component_priorities[component] = component_priority.unwrap_or(0);
        }

        let components = components.into_iter().map(|tasks| {
            let tasks = tasks.into_iter().map(|task| TaskId(task.index()));
            tasks.collect_vec()
        });
        let components = components.collect_vec();

        Schedule {
            continuations: &graph.continuations,
            components,
            dependents,
            dependency_counts,
            priorities,
        }
    }
}

struct Runner<'a, E, F> {
    schedule: &'a Schedule<'a>,
    remaining_dependencies: Vec<AtomicUsize>,
    ready: Mutex<BinaryHeap<(u64, Reverse<TaskId>)>>,
    execute: &'a F,
    failed: AtomicBool,
    error: Mutex<Option<E>>,
}

impl<'a, E, F> Runner<'a, E, F>
where
    E: Send,
    F: Fn(TaskId) -> Result<(), E> + Sync,
{
    fn release<'scope>(&'scope self, scope: &rayon::Scope<'scope>, tasks: &[TaskId]) {
        if tasks.is_empty() {
            return;
        }
        {
            let mut ready = self.ready.lock();
            for task in tasks {
                ready.push((self.schedule.priorities[task.0], Reverse(*task)));
            }
        }
        for _ in tasks {
            scope.spawn(move |scope| self.execute_next(scope));
        }
    }

    fn execute_next<'scope>(&'scope self, scope: &rayon::Scope<'scope>) {
        let next = self.ready.lock().pop();
        let (_, Reverse(task)) = next.expect("invariant violated: a released task is not ready");
        let mut next = Some(task);
        while let Some(task) = next {
            if self.failed.load(Ordering::Acquire) {
                return;
            }
            if let Err(task_error) = (self.execute)(task) {
                self.failed.store(true, Ordering::Release);
                let mut error = self.error.lock();
                error.get_or_insert(task_error);
                return;
            }
            let released = self.schedule.dependents[task.0].iter().filter(|dependent| {
                self.remaining_dependencies[dependent.0].fetch_sub(1, Ordering::AcqRel) == 1
            });
            let released = released.flat_map(|dependent| &self.schedule.components[dependent.0]);
            let mut released = released.copied().collect_vec();
            next = self.schedule.continuations[task.0].filter(|next| released.contains(next));
            released.retain(|released| Some(*released) != next);
            self.release(scope, &released);
        }
    }
}

pub fn execute_prioritized<E, F>(graph: &TaskGraph, execute: &F) -> Result<(), E>
where
    E: Send,
    F: Fn(TaskId) -> Result<(), E> + Sync,
{
    let schedule = Schedule::new(graph);
    let remaining_dependencies = schedule.dependency_counts.iter().copied().map(AtomicUsize::new);
    let remaining_dependencies = remaining_dependencies.collect_vec();
    let initial = schedule
        .dependency_counts
        .iter()
        .positions(|dependency_count| *dependency_count == 0)
        .flat_map(|component| &schedule.components[component]);
    let initial = initial.copied().collect_vec();
    let runner = Runner {
        schedule: &schedule,
        remaining_dependencies,
        ready: Mutex::new(BinaryHeap::new()),
        execute,
        failed: AtomicBool::new(false),
        error: Mutex::new(None),
    };

    rayon::scope(|scope| runner.release(scope, &initial));

    runner.error.into_inner().map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single_threaded<T: Send>(operation: impl FnOnce() -> T + Send) -> T {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap();
        pool.install(operation)
    }

    fn executed_order(graph: &TaskGraph) -> Vec<TaskId> {
        let executed = Mutex::new(Vec::new());
        execute_prioritized(graph, &|task| {
            executed.lock().push(task);
            Ok::<_, ()>(())
        })
        .unwrap();
        executed.into_inner()
    }

    fn position(executed: &[TaskId], task: TaskId) -> usize {
        let positions = executed.iter().positions(|executed| *executed == task).collect_vec();
        assert_eq!(positions.len(), 1, "{task:?} must execute exactly once");
        positions[0]
    }

    #[test]
    fn tasks_execute_once_after_their_dependencies() {
        let mut graph = TaskGraph::default();
        let root = graph.add_task(1);
        let left = graph.add_task(1);
        let right = graph.add_task(1);
        let join = graph.add_task(1);
        let independent = graph.add_task(1);
        graph.add_dependency(left, root);
        graph.add_dependency(right, root);
        graph.add_dependency(join, left);
        graph.add_dependency(join, right);
        graph.add_dependency(join, right);

        let executed = executed_order(&graph);

        assert_eq!(executed.len(), 5);
        assert!(position(&executed, root) < position(&executed, left));
        assert!(position(&executed, root) < position(&executed, right));
        assert!(position(&executed, left) < position(&executed, join));
        assert!(position(&executed, right) < position(&executed, join));
        position(&executed, independent);
    }

    #[test]
    fn the_task_heading_the_costliest_chain_executes_first() {
        let mut graph = TaskGraph::default();
        let cheap_head = graph.add_task(1);
        let lone = graph.add_task(50);
        let expensive_tail = graph.add_task(100);
        let cheap_tail = graph.add_task(1);
        graph.add_dependency(expensive_tail, cheap_head);
        graph.add_dependency(cheap_tail, cheap_head);

        let executed = single_threaded(|| executed_order(&graph));

        assert_eq!(executed, [cheap_head, expensive_tail, lone, cheap_tail]);
    }

    #[test]
    fn a_continuation_executes_directly_after_the_task_that_releases_it() {
        let mut graph = TaskGraph::default();
        let head = graph.add_task(200);
        let continuation = graph.add_task(1);
        let lone = graph.add_task(100);
        graph.add_continuation(head, continuation);

        let executed = single_threaded(|| executed_order(&graph));

        assert_eq!(executed, [head, continuation, lone]);
    }

    #[test]
    fn a_continuation_waits_for_its_other_dependencies() {
        let mut graph = TaskGraph::default();
        let head = graph.add_task(200);
        let continuation = graph.add_task(1);
        let lone = graph.add_task(100);
        graph.add_continuation(head, continuation);
        graph.add_dependency(continuation, lone);

        let executed = single_threaded(|| executed_order(&graph));

        assert_eq!(executed, [head, lone, continuation]);
    }

    #[test]
    fn a_cycle_is_released_together_at_the_priority_of_its_costliest_task() {
        let mut graph = TaskGraph::default();
        let head = graph.add_task(1);
        let continuation = graph.add_task(1);
        let sibling = graph.add_task(100);
        let lone = graph.add_task(50);
        graph.add_continuation(head, continuation);
        graph.add_dependency(continuation, sibling);
        graph.add_dependency(sibling, continuation);

        let executed = single_threaded(|| executed_order(&graph));

        assert_eq!(executed, [head, continuation, sibling, lone]);
    }

    #[test]
    fn cyclic_tasks_execute_once_between_their_dependencies_and_dependents() {
        let mut graph = TaskGraph::default();
        let upstream = graph.add_task(1);
        let first = graph.add_task(1);
        let second = graph.add_task(8);
        let reflexive = graph.add_task(1);
        let downstream = graph.add_task(1);
        graph.add_dependency(first, upstream);
        graph.add_dependency(first, second);
        graph.add_dependency(second, first);
        graph.add_dependency(reflexive, reflexive);
        graph.add_dependency(downstream, second);
        graph.add_dependency(downstream, reflexive);

        let executed = single_threaded(|| executed_order(&graph));

        assert_eq!(executed.len(), 5);
        assert!(position(&executed, upstream) < position(&executed, first));
        assert!(position(&executed, upstream) < position(&executed, second));
        assert!(position(&executed, second) < position(&executed, downstream));
        assert!(position(&executed, reflexive) < position(&executed, downstream));
    }

    #[test]
    fn long_dependency_chains_execute_in_dependency_order() {
        let mut graph = TaskGraph::default();
        let tasks = (0..100_000).map(|_| graph.add_task(1)).collect_vec();
        for (task, dependency) in tasks.iter().tuple_windows() {
            graph.add_dependency(*task, *dependency);
        }

        let executed = executed_order(&graph);

        assert!(executed.iter().rev().eq(tasks.iter()));
    }

    #[test]
    fn failed_tasks_stop_their_dependents_and_queued_tasks() {
        let mut graph = TaskGraph::default();
        let failing = graph.add_task(2);
        let dependent = graph.add_task(1);
        graph.add_task(1);
        graph.add_dependency(dependent, failing);
        let executed = Mutex::new(Vec::new());

        let result = single_threaded(|| {
            execute_prioritized(&graph, &|task| {
                executed.lock().push(task);
                if task == failing { Err("failed") } else { Ok(()) }
            })
        });

        assert_eq!(result, Err("failed"));
        assert_eq!(executed.into_inner(), [failing]);
    }
}
