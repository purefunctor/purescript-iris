use itertools::Itertools;

use super::*;

fn compute_covering_sets(
    functional_dependencies: &[Fd],
    argument_count: usize,
) -> Vec<FxHashSet<usize>> {
    let all_argument_positions: FxHashSet<_> = (0..argument_count).collect();

    let all_covering_sets =
        argument_position_subsets(argument_count).into_iter().filter(|argument_positions| {
            let determined_positions = compute_closure(functional_dependencies, argument_positions);
            determined_positions == all_argument_positions
        });

    let all_covering_sets = all_covering_sets.collect_vec();

    let mut minimal_covering_sets = all_covering_sets.clone();
    minimal_covering_sets.retain(|covering_set| {
        !all_covering_sets.iter().any(|other_covering_set| {
            other_covering_set != covering_set && other_covering_set.is_subset(covering_set)
        })
    });

    minimal_covering_sets
}

fn argument_position_subsets(argument_count: usize) -> Vec<FxHashSet<usize>> {
    let mut subsets = vec![FxHashSet::default()];

    for argument_position in (0..argument_count).rev() {
        for index in 0..subsets.len() {
            let mut subset = subsets[index].clone();
            subset.insert(argument_position);
            subsets.push(subset);
        }
    }

    subsets
}

#[test]
fn test_no_fundeps() {
    let initial = FxHashSet::from_iter([0, 1]);
    let result = compute_closure(&[], &initial);
    assert_eq!(result, initial);
}

#[test]
fn test_single_fundep() {
    let fundeps = vec![Fd::new([0], [1])];
    let initial = FxHashSet::from_iter([0]);
    let result = compute_closure(&fundeps, &initial);
    assert_eq!(result, FxHashSet::from_iter([0, 1]));
}

#[test]
fn test_fundep_not_triggered() {
    let fundeps = vec![Fd::new([0], [1])];
    let initial = FxHashSet::from_iter([1]);
    let result = compute_closure(&fundeps, &initial);
    assert_eq!(result, FxHashSet::from_iter([1]));
}

#[test]
fn test_transitive_fundeps() {
    let fundeps = vec![Fd::new([0], [1]), Fd::new([1], [2])];
    let initial = FxHashSet::from_iter([0]);
    let result = compute_closure(&fundeps, &initial);
    assert_eq!(result, FxHashSet::from_iter([0, 1, 2]));
}

#[test]
fn test_multi_determiner() {
    let fundeps = vec![Fd::new([0, 1], [2])];

    let initial = FxHashSet::from_iter([0]);
    let result = compute_closure(&fundeps, &initial);
    assert_eq!(result, FxHashSet::from_iter([0]));

    let initial = FxHashSet::from_iter([0, 1]);
    let result = compute_closure(&fundeps, &initial);
    assert_eq!(result, FxHashSet::from_iter([0, 1, 2]));
}

#[test]
fn test_multiple_determined() {
    let fundeps = vec![Fd::new([0], [1, 2])];
    let initial = FxHashSet::from_iter([0]);
    let result = compute_closure(&fundeps, &initial);
    assert_eq!(result, FxHashSet::from_iter([0, 1, 2]));
}

#[test]
fn test_empty_determiners() {
    let fundeps = vec![Fd::new([], [0])];
    let initial: FxHashSet<usize> = FxHashSet::default();
    let result = compute_closure(&fundeps, &initial);
    assert_eq!(result, FxHashSet::from_iter([0]));
}

#[test]
fn test_all_determined_no_fundeps() {
    let result = get_all_determined(&[]);
    assert_eq!(result, FxHashSet::default());
}

#[test]
fn test_all_determined_single_fundep() {
    let fundeps = vec![Fd::new([0], [1])];
    let result = get_all_determined(&fundeps);
    assert_eq!(result, FxHashSet::from_iter([1]));
}

#[test]
fn test_all_determined_multiple_fundeps() {
    let fundeps = vec![Fd::new([0], [1]), Fd::new([1], [2])];
    let result = get_all_determined(&fundeps);
    assert_eq!(result, FxHashSet::from_iter([1, 2]));
}

#[test]
fn test_all_determined_overlapping() {
    let fundeps = vec![Fd::new([0], [1, 2]), Fd::new([3], [1])];
    let result = get_all_determined(&fundeps);
    assert_eq!(result, FxHashSet::from_iter([1, 2]));
}

#[test]
fn test_all_determined_empty_determiners() {
    let fundeps = vec![Fd::new([], [0])];
    let result = get_all_determined(&fundeps);
    assert_eq!(result, FxHashSet::from_iter([0]));
}

#[test]
fn test_covering_sets_no_fundeps() {
    let result = compute_covering_sets(&[], 2);
    assert_eq!(result, vec![FxHashSet::from_iter([0, 1])]);
}

#[test]
fn test_covering_sets_single_fundep() {
    let fundeps = vec![Fd::new([0], [1])];
    let result = compute_covering_sets(&fundeps, 2);
    assert_eq!(result, vec![FxHashSet::from_iter([0])]);
}

#[test]
fn test_covering_sets_symmetric_fundeps() {
    let fundeps = vec![Fd::new([0], [1]), Fd::new([1], [0])];
    let result = compute_covering_sets(&fundeps, 2);

    assert_eq!(result.len(), 2);
    assert!(result.contains(&FxHashSet::from_iter([0])));
    assert!(result.contains(&FxHashSet::from_iter([1])));
}

#[test]
fn test_covering_sets_empty_determiner() {
    let fundeps = vec![Fd::new([], [0])];
    let result = compute_covering_sets(&fundeps, 1);
    assert_eq!(result, vec![FxHashSet::default()]);
}

#[test]
fn closure_decision_matches_covering_sets() {
    for argument_count in 0..=3 {
        let all_positions = FxHashSet::from_iter(0..argument_count);
        let mut possible_dependencies = Vec::new();

        for determined in 0..argument_count {
            possible_dependencies.push(Fd::new([], [determined]));
            for determiner in 0..argument_count {
                possible_dependencies.push(Fd::new([determiner], [determined]));
            }
        }

        for dependency_mask in 0..(1 << possible_dependencies.len()) {
            let functional_dependencies = possible_dependencies
                .iter()
                .enumerate()
                .filter(|(index, _)| dependency_mask & (1 << index) != 0)
                .map(|(_, dependency)| dependency.clone());
            let functional_dependencies = functional_dependencies.collect_vec();
            let covering_sets = compute_covering_sets(&functional_dependencies, argument_count);

            for non_apart_mask in 0..(1 << argument_count) {
                let non_apart_positions =
                    (0..argument_count).filter(|position| non_apart_mask & (1 << position) != 0);
                let non_apart_positions = FxHashSet::from_iter(non_apart_positions);
                let expected = covering_sets
                    .iter()
                    .any(|covering_set| covering_set.is_subset(&non_apart_positions));
                let actual = positions_cover_all(
                    &functional_dependencies,
                    &non_apart_positions,
                    &all_positions,
                );

                assert_eq!(actual, expected);
            }
        }
    }
}

#[test]
fn closure_decision_preserves_complex_dependency_semantics() {
    let argument_count = 4;
    let all_positions = FxHashSet::from_iter(0..argument_count);
    let cases = [
        vec![Fd::new([0, 1], [2, 3]), Fd::new([2], [0])],
        // The out-of-range position preserves exact closure-equality behavior for malformed dependencies.
        vec![Fd::new([0, 1], [2, 4]), Fd::new([], [3])],
    ];

    for functional_dependencies in cases {
        let covering_sets = compute_covering_sets(&functional_dependencies, argument_count);
        for non_apart_mask in 0..(1 << argument_count) {
            let non_apart_positions =
                (0..argument_count).filter(|position| non_apart_mask & (1 << position) != 0);
            let non_apart_positions = FxHashSet::from_iter(non_apart_positions);
            let expected = covering_sets
                .iter()
                .any(|covering_set| covering_set.is_subset(&non_apart_positions));
            let actual =
                positions_cover_all(&functional_dependencies, &non_apart_positions, &all_positions);

            assert_eq!(actual, expected);
        }
    }
}
