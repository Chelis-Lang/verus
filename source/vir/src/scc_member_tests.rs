use super::*;
use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

// Keep the old accessors' exact map/vector operations and assertion expressions,
// including `self`, so their failure payloads are an independent oracle.
trait OldAccess<T> {
    fn old_rep(&self, t: &T) -> T;
    fn old_nodes(&self, t: &T) -> Vec<T>;
}
impl<T: Eq + Hash + Clone> OldAccess<T> for Graph<T> {
    fn old_rep(&self, t: &T) -> T {
        assert!(self.has_run);
        assert!(self.mapping.contains_key(&t));
        let id = self.mapping.get(&t).expect("key was present in the line above");
        self.nodes[self.sccs[*id].rep()].t.clone()
    }
    fn old_nodes(&self, t: &T) -> Vec<T> {
        assert!(self.has_run);
        assert!(self.mapping.contains_key(&t));
        let id = self.mapping[t];
        let scc = &self.sccs[id];
        scc.nodes.iter().map(|i| self.nodes[*i].t.clone()).collect()
    }
}

fn same_members<T: Eq + Hash + Clone + std::fmt::Debug>(graph: &Graph<T>) {
    for node in &graph.nodes {
        let expected = graph.old_nodes(&node.t);
        assert_eq!(graph.get_scc_nodes(&node.t), expected);
        assert_eq!(graph.get_scc_nodes_iter(&node.t).cloned().collect::<Vec<_>>(), expected);
        assert_eq!(graph.get_scc_rep(&node.t), graph.old_rep(&node.t));
        let id = graph.mapping[&node.t];
        for (borrowed, index) in graph.get_scc_nodes_iter(&node.t).zip(&graph.sccs[id].nodes) {
            assert!(std::ptr::eq(borrowed, &graph.nodes[*index].t));
        }
    }
}

#[test]
fn member_access_all_small_graphs_matches_old_vectors() {
    for order in [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
        for bits in 0u16..512 {
            for reverse in [false, true] {
                let mut graph = Graph::new();
                for node in order {
                    graph.add_node(node);
                }
                for step in 0..9 {
                    let edge = if reverse { 8 - step } else { step };
                    if bits & (1 << edge) != 0 {
                        graph.add_edge(edge / 3, edge % 3);
                        // Parallel edges must not change the returned vector.
                        if edge % 2 == 0 {
                            graph.add_edge(edge / 3, edge % 3);
                        }
                    }
                }
                graph.compute_sccs();
                same_members(&graph);
            }
        }
    }
}

#[derive(Debug)]
struct Counted {
    value: usize,
    hashes: Rc<Cell<usize>>,
    clones: Rc<Cell<usize>>,
}
impl Clone for Counted {
    fn clone(&self) -> Self {
        self.clones.set(self.clones.get() + 1);
        Self { value: self.value, hashes: self.hashes.clone(), clones: self.clones.clone() }
    }
}
impl PartialEq for Counted {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}
impl Eq for Counted {}
impl Hash for Counted {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hashes.set(self.hashes.get() + 1);
        self.value.hash(state);
    }
}

#[test]
fn member_access_hash_clone_and_reference_costs() {
    let hashes = Rc::new(Cell::new(0));
    let clones = Rc::new(Cell::new(0));
    let keys: Vec<_> = (0..4)
        .map(|value| Counted { value, hashes: hashes.clone(), clones: clones.clone() })
        .collect();
    let mut graph = Graph::new();
    for (source, target) in [(0, 1), (1, 2), (2, 0), (2, 3), (2, 3)] {
        graph.add_edge(keys[source].clone(), keys[target].clone());
    }
    graph.compute_sccs();
    for key in &keys {
        hashes.set(0);
        clones.set(0);
        let expected = graph.old_nodes(key);
        assert_eq!(hashes.get(), 2);
        assert_eq!(clones.get(), expected.len());
        hashes.set(0);
        clones.set(0);
        assert_eq!(graph.get_scc_nodes(key), expected);
        assert_eq!(hashes.get(), 1);
        assert_eq!(clones.get(), expected.len());
        hashes.set(0);
        clones.set(0);
        let borrowed: Vec<_> = graph.get_scc_nodes_iter(key).collect();
        assert_eq!(hashes.get(), 1);
        assert_eq!(clones.get(), 0);
        assert_eq!(borrowed.len(), expected.len());
        for (actual, expected) in borrowed.iter().zip(&expected) {
            assert_eq!(*actual, expected);
        }
        hashes.set(0);
        clones.set(0);
        let expected_rep = graph.old_rep(key);
        assert_eq!((hashes.get(), clones.get()), (2, 1));
        hashes.set(0);
        clones.set(0);
        assert_eq!(graph.get_scc_rep(key), expected_rep);
        assert_eq!((hashes.get(), clones.get()), (1, 1));
    }
}

fn failure(f: impl FnOnce()) -> String {
    let payload = catch_unwind(AssertUnwindSafe(f)).expect_err("expected original graph error");
    if let Some(message) = payload.downcast_ref::<&str>() {
        message.to_string()
    } else {
        payload.downcast_ref::<String>().expect("string panic payload").clone()
    }
}

#[test]
fn member_access_eager_errors_and_first_failure_order() {
    let mut graph = Graph::new();
    graph.add_node(90);
    for (source, target) in [(4, 2), (2, 7), (7, 4), (4, 2)] {
        graph.add_edge(source, target);
    }
    let before = failure(|| {
        graph.old_nodes(&4);
    });
    assert_eq!(before, "assertion failed: self.has_run");
    assert_eq!(
        failure(|| {
            graph.get_scc_nodes(&4);
        }),
        before
    );
    assert_eq!(
        failure(|| {
            graph.get_scc_rep(&4);
        }),
        failure(|| {
            graph.old_rep(&4);
        })
    );
    // Construct and immediately drop: validation must not become lazy.
    assert_eq!(
        failure(|| {
            drop(graph.get_scc_nodes_iter(&4));
        }),
        before
    );
    graph.compute_sccs();
    same_members(&graph);
    let missing = failure(|| {
        graph.old_nodes(&999);
    });
    assert_eq!(missing, "assertion failed: self.mapping.contains_key(&t)");
    assert_eq!(
        failure(|| {
            graph.get_scc_nodes(&999);
        }),
        missing
    );
    assert_eq!(
        failure(|| {
            graph.get_scc_rep(&999);
        }),
        failure(|| {
            graph.old_rep(&999);
        })
    );
    assert_eq!(
        failure(|| {
            drop(graph.get_scc_nodes_iter(&999));
        }),
        missing
    );
    assert_eq!(graph.get_scc_size(&999), 1);

    let old = graph.old_nodes(&4);
    assert_eq!(old, vec![7, 2, 4]);
    for reject in old.iter().copied().map(Some).chain(std::iter::once(None)) {
        let mut old_trace = Vec::new();
        let old_result = old.iter().try_for_each(|node| {
            old_trace.push(*node);
            if Some(*node) == reject { Err(*node) } else { Ok(()) }
        });
        let mut actual_trace = Vec::new();
        let actual_result = graph.get_scc_nodes_iter(&4).try_for_each(|node| {
            actual_trace.push(*node);
            if Some(*node) == reject { Err(*node) } else { Ok(()) }
        });
        assert_eq!(actual_result, old_result);
        assert_eq!(actual_trace, old_trace);
    }
}
