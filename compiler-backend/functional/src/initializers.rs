//! Dependency ordering and cycle detection for module initializers.
//!
//! Initializers are identified by their position in a dependency graph, where
//! `dependencies[position]` lists the positions that `position` refers to.
//! These algorithms use explicit work stacks and run in `O(V + E)` time, so a
//! module with a long chain of initializers neither exhausts the stack nor
//! spends quadratic time identifying cycles.

/// Visits every initializer in depth-first postorder, following dependencies
/// in the order they are listed and roots in increasing position order.
///
/// Emitting initializers in this order guarantees that an acyclic initializer
/// is defined after everything it depends on.
pub fn initializer_postorder(dependencies: &[Vec<usize>]) -> Vec<usize> {
    let mut visited = vec![false; dependencies.len()];
    let mut ordered = Vec::with_capacity(dependencies.len());
    let mut work_stack = Vec::new();

    for root in 0..dependencies.len() {
        if visited[root] {
            continue;
        }
        visited[root] = true;
        work_stack.push((root, 0));

        while let Some((position, next_dependency)) = work_stack.last_mut() {
            let Some(&dependency) = dependencies[*position].get(*next_dependency) else {
                ordered.push(*position);
                work_stack.pop();
                continue;
            };
            *next_dependency += 1;
            if !visited[dependency] {
                visited[dependency] = true;
                work_stack.push((dependency, 0));
            }
        }
    }

    ordered
}

/// Groups mutually dependent initializers, with dependencies before dependents.
/// The order within each strongly connected component is unspecified.
///
/// This is Kosaraju's algorithm: a depth-first search over the transposed
/// graph in reverse postorder visits exactly one strongly connected component
/// at a time.
pub fn initializer_components(dependencies: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut dependents = vec![Vec::new(); dependencies.len()];
    for (position, position_dependencies) in dependencies.iter().enumerate() {
        for &dependency in position_dependencies {
            dependents[dependency].push(position);
        }
    }

    let mut components = Vec::new();
    let mut assigned = vec![false; dependencies.len()];
    let mut work_stack = Vec::new();

    for &root in initializer_postorder(dependencies).iter().rev() {
        if assigned[root] {
            continue;
        }
        assigned[root] = true;
        let mut component = Vec::new();
        work_stack.push(root);

        while let Some(position) = work_stack.pop() {
            component.push(position);
            for &dependent in &dependents[position] {
                if !assigned[dependent] {
                    assigned[dependent] = true;
                    work_stack.push(dependent);
                }
            }
        }
        components.push(component);
    }

    components.reverse();
    components
}

/// Marks every initializer that participates in a cycle: each member of a
/// multi-node cycle and each initializer that depends on itself. Acyclic
/// initializers remain unmarked even when they depend on, or are depended
/// upon by, a cycle.
pub fn cyclic_initializers(dependencies: &[Vec<usize>]) -> Vec<bool> {
    let mut cyclic = vec![false; dependencies.len()];
    for component in initializer_components(dependencies) {
        let depends_on_itself = || dependencies[component[0]].contains(&component[0]);
        if component.len() > 1 || depends_on_itself() {
            for &position in &component {
                cyclic[position] = true;
            }
        }
    }

    cyclic
}

#[cfg(test)]
mod tests {
    use super::{cyclic_initializers, initializer_components, initializer_postorder};

    /// Builds a graph where each initializer depends on the next one.
    fn chain(length: usize) -> Vec<Vec<usize>> {
        let mut dependencies = vec![Vec::new(); length];
        for position in 1..length {
            dependencies[position - 1].push(position);
        }
        dependencies
    }

    /// Runs `test` on a thread whose stack is far too small for a recursive
    /// traversal of a long chain, so recursion fails the test instead of
    /// passing by virtue of the generous default test stack.
    fn with_small_stack(test: impl FnOnce() + Send + 'static) {
        let thread = std::thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn(test)
            .expect("failed to spawn small-stack thread");
        thread.join().expect("test panicked on small-stack thread");
    }

    #[test]
    fn postorder_defines_dependencies_first() {
        let dependencies = vec![vec![1, 2], vec![2], Vec::new(), vec![0]];

        assert_eq!(initializer_postorder(&dependencies), vec![2, 1, 0, 3]);
    }

    #[test]
    fn components_define_external_dependencies_before_recursive_groups() {
        let dependencies = vec![vec![1], vec![2], vec![1, 3], Vec::new(), vec![0]];
        let mut components = initializer_components(&dependencies);
        for component in &mut components {
            component.sort_unstable();
        }

        assert_eq!(components, vec![vec![3], vec![1, 2], vec![0], vec![4]]);
    }

    #[test]
    fn postorder_handles_long_chains_without_recursion() {
        with_small_stack(|| {
            let dependencies = chain(10_000);

            let ordered = initializer_postorder(&dependencies);

            assert_eq!(ordered, (0..10_000).rev().collect::<Vec<_>>());
        });
    }

    #[test]
    fn cycle_detection_handles_long_chains_without_recursion() {
        with_small_stack(|| {
            let dependencies = chain(10_000);

            assert_eq!(cyclic_initializers(&dependencies), vec![false; 10_000]);
        });
    }

    #[test]
    fn multi_node_cycles_mark_every_member() {
        let dependencies = vec![vec![1], vec![2], vec![0], vec![4], vec![3]];

        assert_eq!(cyclic_initializers(&dependencies), vec![true, true, true, true, true]);
    }

    #[test]
    fn self_edges_mark_a_single_initializer() {
        let dependencies = vec![vec![0], Vec::new(), vec![1]];

        assert_eq!(cyclic_initializers(&dependencies), vec![true, false, false]);
    }

    #[test]
    fn acyclic_initializers_connected_to_cycles_are_not_marked() {
        // 0 depends on the cycle {1, 2}, and the cycle depends on 3.
        let dependencies = vec![vec![1], vec![2], vec![1, 3], Vec::new()];

        assert_eq!(cyclic_initializers(&dependencies), vec![false, true, true, false]);
    }

    #[test]
    fn acyclic_graphs_have_no_cyclic_initializers() {
        let dependencies = vec![vec![1, 2], vec![2], Vec::new(), vec![0]];

        assert_eq!(cyclic_initializers(&dependencies), vec![false; 4]);
    }
}
