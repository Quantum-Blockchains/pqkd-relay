use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::{error, fs, path::PathBuf};
use crate::config::{Relay, Connection};

#[derive(Serialize, Deserialize, Debug)]
pub struct MeshTopology {
    relay: Vec<Relay>,
    connection: Vec<Connection>,
}

impl MeshTopology {
    pub fn build(path: PathBuf) -> Result<MeshTopology, Box<dyn error::Error>> {
        let data = fs::read(path)?;
        let text = String::from_utf8(data)?;
        let mesh: MeshTopology = toml::from_str(&text)?;
        check_relay_ids(&mesh.relay)?;
        Ok(mesh)
    }

    pub fn relay(&self) -> &Vec<Relay> {
        &self.relay
    }

    pub fn connection(&self) -> &Vec<Connection> {
        &self.connection
    }

    pub fn find_relay(&self, sae_id: &str) -> Option<&str> {
        for r in self.relay.iter() {
            let p = r.pqkds().iter().find(|p| p == &sae_id);
            if p.is_some() {
                return Some(r.id());
            }
        }
        None
    }
}

#[derive(Debug)]
pub enum MeshValidationError {
    DeadEnd(String),
    TooManyConnections(String),
    Disconnected,
    ArticulationPoint(String),
    InvalidRelayId(String),
}

impl std::fmt::Display for MeshValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MeshValidationError::DeadEnd(n) => write!(f, "node '{}' has fewer than 2 connections", n),
            MeshValidationError::TooManyConnections(n) => write!(f, "node '{}' has more than 5 connections", n),
            MeshValidationError::Disconnected => write!(f, "graph is not connected"),
            MeshValidationError::ArticulationPoint(n) => write!(f, "node '{}' is an articulation point", n),
            MeshValidationError::InvalidRelayId(n) => write!(f, "relay id '{}' must not end with '_in' or '_out' (reserved by Suurballe vertex splitting)", n),
        }
    }
}

impl std::error::Error for MeshValidationError {}

fn check_relay_ids(relays: &[Relay]) -> Result<(), MeshValidationError> {
    for relay in relays {
        if relay.id().ends_with("_in") || relay.id().ends_with("_out") {
            return Err(MeshValidationError::InvalidRelayId(relay.id().to_string()));
        }
    }
    Ok(())
}

pub fn build_mesh(relays: &[Relay], connections: &[Connection]) -> HashMap<String, Vec<String>> {
    let mut graph: HashMap<String, Vec<String>> =
        relays.iter().map(|r| (r.id().to_string(), vec![])).collect();

    for conn in connections {
        graph
            .entry(conn.first().to_string())
            .or_default()
            .push(conn.second().to_string());
        graph
            .entry(conn.second().to_string())
            .or_default()
            .push(conn.first().to_string());
    }
    graph
}

pub fn validate_mesh(graph: &HashMap<String, Vec<String>>) -> Result<(), MeshValidationError> {
    for (node, neighbors) in graph {
        if neighbors.len() < 2 {
            return Err(MeshValidationError::DeadEnd(node.clone()));
        }
        if neighbors.len() > 5 {
            return Err(MeshValidationError::TooManyConnections(node.clone()));
        }
    }
    if !is_connected(graph) {
        return Err(MeshValidationError::Disconnected);
    }
    if let Some(ap) = find_articulation_point(graph) {
        return Err(MeshValidationError::ArticulationPoint(ap));
    }
    Ok(())
}

fn is_connected(graph: &HashMap<String, Vec<String>>) -> bool {
    let start = match graph.keys().next() {
        None => return true,
        Some(s) => s,
    };
    let mut visited: HashSet<&str> = HashSet::new();
    let mut stack = vec![start.as_str()];
    while let Some(node) = stack.pop() {
        if visited.insert(node) {
            for neighbor in &graph[node] {
                if !visited.contains(neighbor.as_str()) {
                    stack.push(neighbor.as_str());
                }
            }
        }
    }
    visited.len() == graph.len()
}

fn find_articulation_point(graph: &HashMap<String, Vec<String>>) -> Option<String> {
    let nodes: Vec<String> = graph.keys().cloned().collect();
    let n = nodes.len();
    if n == 0 {
        return None;
    }
    let node_idx: HashMap<&str, usize> =
        nodes.iter().enumerate().map(|(i, s)| (s.as_str(), i)).collect();

    let mut disc = vec![usize::MAX; n];
    let mut low = vec![usize::MAX; n];
    let mut visited = vec![false; n];
    let mut is_ap = vec![false; n];
    let mut timer = 0usize;

    tarjan_ap(0, usize::MAX, &nodes, &node_idx, graph, &mut disc, &mut low, &mut visited, &mut is_ap, &mut timer);

    is_ap.iter().enumerate().find(|(_, &ap)| ap).map(|(i, _)| nodes[i].clone())
}

fn tarjan_ap(
    u: usize,
    parent: usize,
    nodes: &[String],
    node_idx: &HashMap<&str, usize>,
    graph: &HashMap<String, Vec<String>>,
    disc: &mut Vec<usize>,
    low: &mut Vec<usize>,
    visited: &mut Vec<bool>,
    is_ap: &mut Vec<bool>,
    timer: &mut usize,
) {
    visited[u] = true;
    disc[u] = *timer;
    low[u] = *timer;
    *timer += 1;

    let mut children = 0usize;
    let neighbors: Vec<usize> = graph[&nodes[u]]
        .iter()
        .filter_map(|v| node_idx.get(v.as_str()).copied())
        .collect();

    for v in neighbors {
        if !visited[v] {
            children += 1;
            tarjan_ap(v, u, nodes, node_idx, graph, disc, low, visited, is_ap, timer);
            low[u] = low[u].min(low[v]);
            if parent == usize::MAX && children > 1 {
                is_ap[u] = true;
            }
            if parent != usize::MAX && low[v] >= disc[u] {
                is_ap[u] = true;
            }
        } else if v != parent {
            low[u] = low[u].min(disc[v]);
        }
    }
}

pub fn find_two_disjoint_paths(
    graph: &HashMap<String, Vec<String>>,
    start: &str,
    end: &str,
) -> Option<(Vec<String>, Vec<String>)> {
    let in_of = |v: &str| -> String {
        if v == start || v == end {
            v.to_string()
        } else {
            format!("{}_in", v)
        }
    };
    let out_of = |v: &str| -> String {
        if v == start || v == end {
            v.to_string()
        } else {
            format!("{}_out", v)
        }
    };

    // Build split directed graph: v → v_in→v_out, edges become out(u)→in(v)
    let mut split: HashMap<String, Vec<(String, i64)>> = HashMap::new();
    for v in graph.keys() {
        split.entry(in_of(v)).or_default();
        split.entry(out_of(v)).or_default();
        if v != start && v != end {
            split
                .entry(format!("{}_in", v))
                .or_default()
                .push((format!("{}_out", v), 0));
        }
    }
    for (u, neighbors) in graph {
        for v in neighbors {
            split.entry(out_of(u)).or_default().push((in_of(v), 1));
        }
    }

    // Step 1: first Dijkstra
    let (dist, prev) = dijkstra_w(&split, start);
    if !dist.contains_key(end) {
        return None;
    }
    let path1_split = reconstruct_prev(&prev, start, end)?;

    // Step 2: Johnson reweighting + reverse P1 edges
    let mut modified = reweight_w(&split, &dist);
    for window in path1_split.windows(2) {
        let (u, v) = (&window[0], &window[1]);
        if let Some(edges) = modified.get_mut(u) {
            edges.retain(|(n, _)| n != v);
        }
        modified.entry(v.clone()).or_default().push((u.clone(), 0));
    }

    // Step 3: second Dijkstra on modified graph
    let (_, prev2) = dijkstra_w(&modified, start);
    let path2_split = reconstruct_prev(&prev2, start, end)?;

    // Step 4: merge edges from P1+P2, cancel opposing pairs
    let mut edge_bag: HashMap<(String, String), i32> = HashMap::new();
    for w in path1_split.windows(2) {
        *edge_bag.entry((w[0].clone(), w[1].clone())).or_default() += 1;
    }
    for w in path2_split.windows(2) {
        *edge_bag.entry((w[0].clone(), w[1].clone())).or_default() += 1;
    }
    let pairs: Vec<_> = edge_bag.keys().cloned().collect();
    for (u, v) in pairs {
        let rev = (v.clone(), u.clone());
        if edge_bag.contains_key(&rev) {
            let fwd = *edge_bag.get(&(u.clone(), v.clone())).unwrap();
            let bwd = *edge_bag.get(&rev).unwrap();
            let cancel = fwd.min(bwd);
            *edge_bag.get_mut(&(u.clone(), v.clone())).unwrap() -= cancel;
            *edge_bag.get_mut(&rev).unwrap() -= cancel;
        }
    }

    // Step 5: build adjacency from surviving edges, trace two paths
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for ((u, v), count) in &edge_bag {
        for _ in 0..*count {
            adj.entry(u.clone()).or_default().push(v.clone());
        }
    }
    let p1_split = trace_path(&mut adj, start, end)?;
    let p2_split = trace_path(&mut adj, start, end)?;

    // Convert split-graph node names back to original relay IDs
    let to_orig = |path: Vec<String>| -> Vec<String> {
        let mut result: Vec<String> = Vec::new();
        for node in path {
            let name = node
                .strip_suffix("_in")
                .or_else(|| node.strip_suffix("_out"))
                .map(str::to_string)
                .unwrap_or(node);
            if result.last() != Some(&name) {
                result.push(name);
            }
        }
        result
    };

    Some((to_orig(p1_split), to_orig(p2_split)))
}

fn dijkstra_w(
    graph: &HashMap<String, Vec<(String, i64)>>,
    start: &str,
) -> (HashMap<String, i64>, HashMap<String, String>) {
    let mut dist: HashMap<String, i64> = HashMap::new();
    let mut prev: HashMap<String, String> = HashMap::new();
    let mut heap: BinaryHeap<Reverse<(i64, String)>> = BinaryHeap::new();

    dist.insert(start.to_string(), 0);
    heap.push(Reverse((0, start.to_string())));

    while let Some(Reverse((cost, u))) = heap.pop() {
        if cost > *dist.get(&u).unwrap_or(&i64::MAX) {
            continue;
        }
        for (v, w) in graph.get(&u).into_iter().flatten() {
            let nc = cost + w;
            if nc < *dist.get(v).unwrap_or(&i64::MAX) {
                dist.insert(v.clone(), nc);
                prev.insert(v.clone(), u.clone());
                heap.push(Reverse((nc, v.clone())));
            }
        }
    }
    (dist, prev)
}

fn reconstruct_prev(prev: &HashMap<String, String>, start: &str, end: &str) -> Option<Vec<String>> {
    let mut path = vec![end.to_string()];
    let mut cur = end.to_string();
    while cur != start {
        let p = prev.get(&cur)?.clone();
        path.push(p.clone());
        cur = p;
    }
    path.reverse();
    Some(path)
}

fn reweight_w(
    graph: &HashMap<String, Vec<(String, i64)>>,
    dist: &HashMap<String, i64>,
) -> HashMap<String, Vec<(String, i64)>> {
    graph
        .iter()
        .map(|(u, neighbors)| {
            let new_neighbors = match dist.get(u) {
                None => vec![],
                Some(&du) => neighbors
                    .iter()
                    .filter_map(|(v, w)| dist.get(v).map(|&dv| (v.clone(), w + du - dv)))
                    .collect(),
            };
            (u.clone(), new_neighbors)
        })
        .collect()
}

fn trace_path(adj: &mut HashMap<String, Vec<String>>, start: &str, end: &str) -> Option<Vec<String>> {
    let mut path = vec![start.to_string()];
    let mut cur = start.to_string();
    while cur != end {
        let neighbors = adj.get_mut(&cur)?;
        if neighbors.is_empty() {
            return None;
        }
        let next = neighbors.remove(0);
        path.push(next.clone());
        cur = next;
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::{build_mesh, check_relay_ids, find_two_disjoint_paths, validate_mesh, Connection, MeshTopology, MeshValidationError, Relay};
    use std::collections::{HashMap, HashSet};

    fn mesh_from_edges(edges: &[(&str, &str)]) -> HashMap<String, Vec<String>> {
        let mut g: HashMap<String, Vec<String>> = HashMap::new();
        for (u, v) in edges {
            g.entry(u.to_string()).or_default().push(v.to_string());
            g.entry(v.to_string()).or_default().push(u.to_string());
        }
        g
    }

    #[test]
    fn find_relay_returns_matching_relay_id_for_sae() {
        let topology = MeshTopology {
            relay: vec![
                Relay::new("relay-a".to_string(), vec!["Alice".to_string()]),
                Relay::new("relay-b".to_string(), vec!["Bob".to_string()]),
            ],
            connection: vec![Connection::new(
                "relay-a".to_string(),
                "relay-b".to_string(),
                "Alice".to_string(),
                "Bob".to_string(),
            )],
        };

        assert_eq!(topology.find_relay("Alice"), Some("relay-a"));
        assert_eq!(topology.find_relay("Unknown"), None);
    }

    #[test]
    fn triangle_passes_validation() {
        let g = mesh_from_edges(&[("A", "B"), ("B", "C"), ("A", "C")]);
        assert!(validate_mesh(&g).is_ok());
    }

    #[test]
    fn chain_a_b_c_is_invalid() {
        let g = mesh_from_edges(&[("A", "B"), ("B", "C")]);
        assert!(validate_mesh(&g).is_err());
    }

    #[test]
    fn single_node_with_no_neighbors_is_invalid() {
        let mut g = HashMap::new();
        g.insert("A".to_string(), vec![]);
        assert!(validate_mesh(&g).is_err());
    }

    #[test]
    fn square_with_diagonal_passes_validation() {
        let g = mesh_from_edges(&[("A", "B"), ("B", "C"), ("C", "D"), ("D", "A"), ("A", "C")]);
        assert!(validate_mesh(&g).is_ok());
    }

    #[test]
    fn suurballe_square_finds_two_disjoint_paths() {
        let g = mesh_from_edges(&[("A", "B"), ("B", "D"), ("A", "C"), ("C", "D")]);
        let (p1, p2) = find_two_disjoint_paths(&g, "A", "D").unwrap();

        let mid1: HashSet<_> = p1[1..p1.len() - 1].iter().collect();
        let mid2: HashSet<_> = p2[1..p2.len() - 1].iter().collect();
        assert!(mid1.is_disjoint(&mid2));
        assert_eq!(p1.first().unwrap(), "A");
        assert_eq!(p1.last().unwrap(), "D");
        assert_eq!(p2.first().unwrap(), "A");
        assert_eq!(p2.last().unwrap(), "D");
    }

    #[test]
    fn suurballe_no_path_returns_none() {
        let g = mesh_from_edges(&[("A", "B"), ("C", "D")]);
        assert!(find_two_disjoint_paths(&g, "A", "D").is_none());
    }

    #[test]
    fn suurballe_reroutes_when_naive_paths_overlap() {
        // Naive BFS finds A→B→E and A→B→D→E (both through B).
        // Suurballe finds A→B→E and A→C→D→E (vertex-disjoint).
        let g = mesh_from_edges(&[("A", "B"), ("A", "C"), ("B", "D"), ("C", "D"), ("D", "E"), ("B", "E")]);
        let (p1, p2) = find_two_disjoint_paths(&g, "A", "E").unwrap();

        let mid1: HashSet<_> = p1[1..p1.len() - 1].iter().collect();
        let mid2: HashSet<_> = p2[1..p2.len() - 1].iter().collect();
        assert!(mid1.is_disjoint(&mid2));
        assert_eq!(p1.first().unwrap(), "A");
        assert_eq!(p1.last().unwrap(), "E");
        assert_eq!(p2.first().unwrap(), "A");
        assert_eq!(p2.last().unwrap(), "E");
    }

    #[test]
    fn suurballe_linear_chain_returns_none() {
        // A-B-C has only one path, no vertex-disjoint pair
        let g = mesh_from_edges(&[("A", "B"), ("B", "C")]);
        assert!(find_two_disjoint_paths(&g, "A", "C").is_none());
    }
    

    #[test]
    fn build_mesh_triangle_has_correct_neighbors() {
        let relays = vec![
            Relay::new("A".to_string(), vec![]),
            Relay::new("B".to_string(), vec![]),
            Relay::new("C".to_string(), vec![]),
        ];
        let connections = vec![
            Connection::new("A".to_string(), "B".to_string(), "a-b".to_string(), "b-a".to_string()),
            Connection::new("B".to_string(), "C".to_string(), "b-c".to_string(), "c-b".to_string()),
            Connection::new("A".to_string(), "C".to_string(), "a-c".to_string(), "c-a".to_string()),
        ];
        let graph = build_mesh(&relays, &connections);

        assert_eq!(graph.len(), 3);
        assert_eq!(graph["A"].len(), 2);
        assert!(graph["A"].contains(&"B".to_string()));
        assert!(graph["A"].contains(&"C".to_string()));
        assert_eq!(graph["B"].len(), 2);
        assert_eq!(graph["C"].len(), 2);
    }

    #[test]
    fn disconnected_graph_fails_with_disconnected_error() {
        // Two separate triangles — every node has degree 2, but graph is split in two components.
        // DeadEnd check passes; only the new connectivity check catches this.
        let g = mesh_from_edges(&[
            ("A", "B"), ("B", "C"), ("A", "C"),
            ("D", "E"), ("E", "F"), ("D", "F"),
        ]);
        assert!(matches!(validate_mesh(&g), Err(MeshValidationError::Disconnected)));
    }

    #[test]
    fn barbell_graph_has_articulation_point() {
        // Two triangles sharing node C — removing C disconnects A-B from D-E.
        // All nodes have degree ≥ 2 and the graph is connected, so only Tarjan catches this.
        let g = mesh_from_edges(&[
            ("A", "B"), ("B", "C"), ("A", "C"),
            ("C", "D"), ("D", "E"), ("C", "E"),
        ]);
        assert!(matches!(validate_mesh(&g), Err(MeshValidationError::ArticulationPoint(_))));
    }

    #[test]
    fn build_mesh_isolated_relay_has_empty_neighbors() {
        let relays = vec![
            Relay::new("A".to_string(), vec![]),
            Relay::new("B".to_string(), vec![]),
        ];
        let connections = vec![];
        let graph = build_mesh(&relays, &connections);

        assert_eq!(graph.len(), 2);
        assert!(graph["A"].is_empty());
        assert!(graph["B"].is_empty());
    }

    #[test]
    fn relay_id_ending_with_in_is_rejected() {
        let relays = vec![Relay::new("relay_in".to_string(), vec![])];
        assert!(matches!(
            check_relay_ids(&relays),
            Err(MeshValidationError::InvalidRelayId(id)) if id == "relay_in"
        ));
    }

    #[test]
    fn relay_id_ending_with_out_is_rejected() {
        let relays = vec![Relay::new("node_out".to_string(), vec![])];
        assert!(matches!(
            check_relay_ids(&relays),
            Err(MeshValidationError::InvalidRelayId(id)) if id == "node_out"
        ));
    }

    #[test]
    fn relay_ids_without_reserved_suffix_are_accepted() {
        let relays = vec![
            Relay::new("relay-a".to_string(), vec![]),
            Relay::new("relay_input".to_string(), vec![]),
            Relay::new("output_node".to_string(), vec![]),
        ];
        assert!(check_relay_ids(&relays).is_ok());
    }
}