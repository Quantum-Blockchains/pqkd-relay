use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;
use std::collections::HashMap;
use std::{error, fs, path::PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Pqkd {
    port: u16,
    sae_id: String,
    remote_sae_id: String,
    remote_proxy_address: String,
    kme_address: String,
    ca_cert: Option<PathBuf>,
    client_cert: Option<PathBuf>,
    client_key: Option<PathBuf>,
}

impl Pqkd {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn sae_id(&self) -> &str {
        &self.sae_id
    }

    pub fn kme_address(&self) -> &str {
        &self.kme_address
    }

    pub fn remote_sae_id(&self) -> &str {
        &self.remote_sae_id
    }

    pub fn remote_proxy_address(&self) -> &str {
        &self.remote_proxy_address
    }

    pub fn ca_cert(&self) -> &Option<PathBuf> {
        &self.ca_cert
    }

    pub fn client_cert(&self) -> &Option<PathBuf> {
        &self.client_cert
    }

    pub fn client_key(&self) -> &Option<PathBuf> {
        &self.client_key
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    id: String,
    port: u16,
    pqkds: Vec<Pqkd>,
}

impl Config {
    pub fn build(config_path: PathBuf) -> Result<Config, Box<dyn error::Error>> {
        let data = fs::read(config_path)?;
        let text = String::from_utf8(data)?;
        let config: Config = toml::from_str(&text)?;
        Ok(config)
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn pqkds(&self) -> &Vec<Pqkd> {
        &self.pqkds
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Relay {
    id: String,
    pqkds: Vec<String>,
}

impl Relay {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn pqkds(&self) -> &Vec<String> {
        &self.pqkds
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Connection {
    first: String,
    second: String,
}

impl Connection {
    pub fn first(&self) -> &str {
        &self.first
    }

    pub fn second(&self) -> &str {
        &self.second
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Hypercube {
    dimension: usize,
    n: usize,
    relay: Vec<Relay>,
    connection: Vec<Connection>,
}

impl Hypercube {
    pub fn build(path: PathBuf) -> Result<Hypercube, Box<dyn error::Error>> {
        let data = fs::read(path)?;
        let text = String::from_utf8(data)?;
        let hypercube: Hypercube = toml::from_str(&text)?;
        Ok(hypercube)
    }

    pub fn dimension(&self) -> usize {
        self.dimension
    }

    pub fn n(&self) -> usize {
        self.n
    }

    pub fn relay(&self) -> &Vec<Relay> {
        &self.relay
    }

    pub fn connection(&self) -> &Vec<Connection> {
        &self.connection
    }

    pub fn find_relay(&self, sae_id: &str) -> Option<&str> {
        for r in self.relay.iter() {
            let p = r.pqkds.iter().find(|p| p == &sae_id);
            if p.is_some() {
                return Some(r.id());
            }
        }
        None
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MeshTopology {
    n: usize,
    relay: Vec<Relay>,
    connection: Vec<Connection>,
}

impl MeshTopology {
    pub fn build(path: PathBuf) -> Result<MeshTopology, Box<dyn error::Error>> {
        let data = fs::read(path)?;
        let text = String::from_utf8(data)?;
        let mesh: MeshTopology = toml::from_str(&text)?;
        Ok(mesh)
    }

    pub fn n(&self) -> usize {
        self.n
    }

    pub fn relay(&self) -> &Vec<Relay> {
        &self.relay
    }

    pub fn connection(&self) -> &Vec<Connection> {
        &self.connection
    }

    pub fn find_relay(&self, sae_id: &str) -> Option<&str> {
        for r in self.relay.iter() {
            let p = r.pqkds.iter().find(|p| p == &sae_id);
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
    ArticulationPoint(String),
}

impl std::fmt::Display for MeshValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MeshValidationError::DeadEnd(n) => write!(f, "node '{}' has fewer than 2 connections", n),
            MeshValidationError::TooManyConnections(n) => write!(f, "node '{}' has more than 5 connections", n),
            MeshValidationError::ArticulationPoint(n) => write!(f, "node '{}' is an articulation point", n),
        }
    }
}

impl std::error::Error for MeshValidationError {}

#[derive(Eq, PartialEq)]
pub struct Path {
    cost: usize,
    nodes: Vec<String>,
}

impl Ord for Path {
    fn cmp(&self, other: &Self) -> Ordering {
        other.cost.cmp(&self.cost)
    }
}

impl PartialOrd for Path {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub fn hamming_distance(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).filter(|(c1, c2)| c1 != c2).count()
}

pub fn build_hypercube(dim: usize) -> HashMap<String, Vec<String>> {
    let nodes: Vec<String> = (0..1 << dim)
        .map(|i| format!("{:0width$b}", i, width = dim))
        .collect();

    let mut graph = HashMap::new();
    for (a, b) in nodes.iter().tuple_combinations() {
        if hamming_distance(a, b) == 1 {
            graph
                .entry(a.clone())
                .or_insert_with(Vec::new)
                .push(b.clone());
            graph
                .entry(b.clone())
                .or_insert_with(Vec::new)
                .push(a.clone());
        }
    }
    graph
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
    if let Some(ap) = find_articulation_point(graph) {
        return Err(MeshValidationError::ArticulationPoint(ap));
    }
    Ok(())
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

pub fn find_n_shortest_paths(
    graph: &HashMap<String, Vec<String>>,
    start: &str,
    end: &str,
    n: usize,
) -> Vec<Vec<String>> {
    let mut heap = BinaryHeap::new();
    let mut paths = Vec::new();

    heap.push(Path {
        cost: 0,
        nodes: vec![start.to_string()],
    });

    while let Some(Path { cost, nodes }) = heap.pop() {
        let current = nodes.last().unwrap();

        if current == end {
            paths.push(nodes.clone());
            if paths.len() == n {
                break;
            }
            continue;
        }

        if let Some(neighbors) = graph.get(current) {
            for neighbor in neighbors {
                if !nodes.contains(neighbor) {
                    let mut new_path = nodes.clone();
                    new_path.push(neighbor.clone());
                    heap.push(Path {
                        cost: cost + 1,
                        nodes: new_path,
                    });
                }
            }
        }
    }

    paths
}

#[cfg(test)]
mod tests {
    use super::{
        build_hypercube, build_mesh, find_n_shortest_paths, find_two_disjoint_paths,
        hamming_distance, validate_mesh, Connection, Hypercube, MeshTopology, Relay,
    };
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
    fn hamming_distance_counts_different_bits() {
        assert_eq!(hamming_distance("1010", "1111"), 2);
        assert_eq!(hamming_distance("000", "000"), 0);
    }

    #[test]
    fn build_hypercube_for_dim_2_has_expected_neighbors() {
        let graph = build_hypercube(2);

        assert_eq!(graph.len(), 4);
        assert_eq!(graph["00"].len(), 2);
        assert_eq!(graph["01"].len(), 2);
        assert!(graph["00"].contains(&"01".to_string()));
        assert!(graph["00"].contains(&"10".to_string()));
        assert!(graph["01"].contains(&"11".to_string()));
        assert!(graph["10"].contains(&"11".to_string()));
    }

    #[test]
    fn find_n_shortest_paths_returns_two_shortest_routes_in_dim_2() {
        let graph = build_hypercube(2);
        let paths = find_n_shortest_paths(&graph, "00", "11", 2);

        assert_eq!(paths.len(), 2);
        assert!(paths.iter().all(|p| p.len() == 3));
        assert!(paths
            .iter()
            .any(|p| p == &vec!["00".to_string(), "01".to_string(), "11".to_string()]));
        assert!(paths
            .iter()
            .any(|p| p == &vec!["00".to_string(), "10".to_string(), "11".to_string()]));
        assert!(paths.iter().all(|p| {
            let mut unique = p.clone();
            unique.sort();
            unique.dedup();
            unique.len() == p.len()
        }));
    }

    #[test]
    fn find_relay_returns_matching_relay_id_for_sae() {
        let hypercube = Hypercube {
            dimension: 2,
            n: 2,
            relay: vec![
                Relay {
                    id: "00".to_string(),
                    pqkds: vec!["Alice".to_string()],
                },
                Relay {
                    id: "10".to_string(),
                    pqkds: vec!["Bob".to_string()],
                },
            ],
            connection: vec![Connection {
                first: "00".to_string(),
                second: "10".to_string(),
            }],
        };

        assert_eq!(hypercube.find_relay("Alice"), Some("00"));
        assert_eq!(hypercube.find_relay("Unknown"), None);
    }

    #[test]
    fn mesh_find_relay_returns_matching_relay_id_for_sae() {
        let mesh = MeshTopology {
            n: 2,
            relay: vec![
                Relay {
                    id: "relay-a".to_string(),
                    pqkds: vec!["Alice".to_string()],
                },
                Relay {
                    id: "relay-b".to_string(),
                    pqkds: vec!["Bob".to_string()],
                },
            ],
            connection: vec![Connection {
                first: "relay-a".to_string(),
                second: "relay-b".to_string(),
            }],
        };

        assert_eq!(mesh.find_relay("Alice"), Some("relay-a"));
        assert_eq!(mesh.find_relay("Unknown"), None);
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
    fn suurballe_linear_chain_returns_none() {
        // A-B-C has only one path, no vertex-disjoint pair
        let g = mesh_from_edges(&[("A", "B"), ("B", "C")]);
        assert!(find_two_disjoint_paths(&g, "A", "C").is_none());
    }

    #[test]
    fn build_mesh_triangle_has_correct_neighbors() {
        let relays = vec![
            Relay { id: "A".to_string(), pqkds: vec![] },
            Relay { id: "B".to_string(), pqkds: vec![] },
            Relay { id: "C".to_string(), pqkds: vec![] },
        ];
        let connections = vec![
            Connection { first: "A".to_string(), second: "B".to_string() },
            Connection { first: "B".to_string(), second: "C".to_string() },
            Connection { first: "A".to_string(), second: "C".to_string() },
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
    fn build_mesh_isolated_relay_has_empty_neighbors() {
        let relays = vec![
            Relay { id: "A".to_string(), pqkds: vec![] },
            Relay { id: "B".to_string(), pqkds: vec![] },
        ];
        let connections = vec![];
        let graph = build_mesh(&relays, &connections);

        assert_eq!(graph.len(), 2);
        assert!(graph["A"].is_empty());
        assert!(graph["B"].is_empty());
    }
}
