use super::*;
use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

// The exact old map-based edge walk is an independent ordering oracle.
fn map_sort<T: Eq + Hash + Clone>(graph: &Graph<T>) -> Vec<T> {
    fn visit<T: Eq + Hash + Clone>(
        graph: &Graph<T>,
        visited: &mut Vec<bool>,
        sorted: &mut Vec<T>,
        s: SccId,
    ) {
        if visited[s] {
            return;
        }
        visited[s] = true;
        for v in graph.sccs[s].nodes.iter() {
            for w in graph.nodes[*v].edges.iter() {
                visit(graph, visited, sorted, graph.mapping[&graph.nodes[*w].t]);
            }
        }
        sorted.push(graph.nodes[graph.sccs[s].rep()].t.clone());
    }
    let mut visited = vec![false; graph.sccs.len()];
    let mut sorted = Vec::new();
    for s in 0..graph.sccs.len() {
        visit(graph, &mut visited, &mut sorted, s);
    }
    sorted
}

fn map_reach<T: Eq + Hash + Clone>(
    graph: &Graph<T>,
    source: &T,
    target: &T,
    sorted_order: Option<&HashMap<T, usize>>,
) -> bool {
    assert!(graph.has_run);
    assert!(graph.mapping.contains_key(source));
    let src = graph.mapping[source];
    let tgt = graph.mapping[target];
    let tgt_sort_index = sorted_order.map(|order| order[&graph.get_scc_rep(target)]);
    let mut reached: HashSet<SccId> = HashSet::new();
    let mut worklist: Vec<SccId> = vec![src];
    while let Some(scc) = worklist.pop() {
        if scc == tgt {
            return true;
        }
        if reached.contains(&scc) {
            continue;
        }
        reached.insert(scc);
        if let Some(tgt_sort_index) = tgt_sort_index {
            let scc_sort_index = sorted_order.unwrap()[&graph.nodes[graph.sccs[scc].rep()].t];
            if scc_sort_index < tgt_sort_index {
                continue;
            }
        }
        for v in graph.sccs[scc].nodes.iter() {
            for w in graph.nodes[*v].edges.iter() {
                worklist.push(graph.mapping[&graph.nodes[*w].t]);
            }
        }
    }
    false
}

fn equivalent<T: Eq + Hash + Clone + std::fmt::Debug>(graph: &Graph<T>) {
    for node in graph.nodes.iter() {
        assert_eq!(node.scc, graph.mapping[&node.t]);
        assert!(graph.sccs[node.scc].nodes.iter().any(|i| graph.nodes[*i].t == node.t));
    }
    let old = map_sort(graph);
    assert_eq!(graph.sort_sccs(), old);
    assert_eq!(graph.clone().sort_sccs(), old);
    let sorted_order: HashMap<T, usize> =
        old.iter().enumerate().map(|(i, t)| (t.clone(), i)).collect();
    for source in graph.nodes.iter() {
        for target in graph.nodes.iter() {
            for order in [None, Some(&sorted_order)] {
                assert_eq!(
                    graph.can_reach(&source.t, &target.t, order),
                    map_reach(graph, &source.t, &target.t, order)
                );
            }
        }
    }
}

#[test]
fn numeric_scc_all_small_graphs_preserve_map_walk() {
    equivalent(&{
        let mut graph = Graph::<usize>::new();
        graph.compute_sccs();
        graph
    });
    // Every directed three-node graph, all six declaration orders, and both
    // edge insertion directions. This includes self-loops and disconnected SCCs.
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
                    }
                }
                graph.compute_sccs();
                equivalent(&graph);
            }
        }
    }
}

#[test]
fn numeric_scc_duplicates_cycles_and_implicit_nodes_preserve_order() {
    let mut graph = Graph::new();
    graph.add_node(90);
    graph.add_node(90);
    // Add edges before declaring their nodes, including repeated parallel
    // edges, multiple SCCs, a self-loop and an isolated declared node.
    for (source, target) in [(4, 2), (2, 4), (4, 2), (2, 7), (7, 7), (7, 3), (3, 5), (5, 3)] {
        graph.add_edge(source, target);
    }
    graph.compute_sccs();
    equivalent(&graph);
    assert_eq!(graph.get_scc_nodes(&4), vec![2, 4]);
    assert_eq!(graph.get_scc_nodes(&3), vec![5, 3]);
    assert_eq!(graph.sort_sccs(), vec![90, 5, 7, 2]);
    assert_eq!(graph.get_scc_size(&999), 1);
}

#[derive(Clone, Debug)]
struct CountedNode {
    value: usize,
    hashes: Rc<Cell<usize>>,
}
impl PartialEq for CountedNode {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}
impl Eq for CountedNode {}
impl Hash for CountedNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hashes.set(self.hashes.get() + 1);
        self.value.hash(state);
    }
}

#[test]
fn numeric_scc_internal_edge_walks_avoid_key_hashing() {
    let hashes = Rc::new(Cell::new(0));
    let keys: Vec<_> = (0..5).map(|value| CountedNode { value, hashes: hashes.clone() }).collect();
    let mut graph = Graph::new();
    for key in &keys {
        graph.add_node(key.clone());
    }
    for (source, target) in [(0, 1), (1, 0), (1, 2), (1, 2), (2, 3), (3, 2)] {
        graph.add_edge(keys[source].clone(), keys[target].clone());
    }
    graph.compute_sccs();
    hashes.set(0);
    let actual = graph.sort_sccs();
    assert_eq!(hashes.get(), 0);
    let expected = map_sort(&graph);
    assert_eq!(actual, expected);
    assert_eq!(hashes.get(), 6);

    // Searching for an isolated target traverses every reachable edge. The
    // three external-key queries remain; only internal edge lookups disappear.
    hashes.set(0);
    assert!(!graph.can_reach(&keys[0], &keys[4], None));
    assert_eq!(hashes.get(), 3);
    hashes.set(0);
    assert!(!map_reach(&graph, &keys[0], &keys[4], None));
    assert_eq!(hashes.get(), 9);
}
