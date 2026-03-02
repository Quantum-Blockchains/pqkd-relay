use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
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
        build_hypercube, find_n_shortest_paths, hamming_distance, Connection, Hypercube, Relay,
    };

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
                first: "Alice".to_string(),
                second: "Bob".to_string(),
            }],
        };

        assert_eq!(hypercube.find_relay("Alice"), Some("00"));
        assert_eq!(hypercube.find_relay("Unknown"), None);
    }
}
