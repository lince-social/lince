//! Place: Lince's first Instinct (blueprint IX) — a concept the engine ships
//! functions for. Pure math over passed-in data; map data loads elsewhere.

use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub lat: f64,
    pub lon: f64,
}

const EARTH_RADIUS_M: f64 = 6_371_000.0;

/// Haversine distance in meters.
pub fn distance(a: Place, b: Place) -> f64 {
    let (la1, la2) = (a.lat.to_radians(), b.lat.to_radians());
    let dlat = (b.lat - a.lat).to_radians();
    let dlon = (b.lon - a.lon).to_radians();
    let h = (dlat / 2.0).sin().powi(2) + la1.cos() * la2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().asin()
}

pub fn near(p: Place, center: Place, radius_m: f64) -> bool {
    distance(p, center) <= radius_m
}

/// A loaded routing graph (OSM extract or anything else): nodes are places,
/// edges are traversable segments with lengths in meters.
#[derive(Debug, Clone, Default)]
pub struct MapGraph {
    pub nodes: Vec<Place>,
    /// adjacency: node index -> [(neighbor index, meters)]
    pub edges: HashMap<usize, Vec<(usize, f64)>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    pub path: Vec<usize>,
    pub meters: f64,
}

impl Route {
    /// Seconds at an average speed (m/s) — `route_eta` in Karma/Protein.
    pub fn eta_seconds(&self, speed_m_s: f64) -> f64 {
        if speed_m_s <= 0.0 {
            f64::INFINITY
        } else {
            self.meters / speed_m_s
        }
    }
}

/// A* over the graph, haversine heuristic (admissible for ground travel).
pub fn route(graph: &MapGraph, from: usize, to: usize) -> Option<Route> {
    #[derive(PartialEq)]
    struct Open(f64, usize);
    impl Eq for Open {}
    impl PartialOrd for Open {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for Open {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            other.0.total_cmp(&self.0) // min-heap
        }
    }

    if from >= graph.nodes.len() || to >= graph.nodes.len() {
        return None;
    }
    let goal = graph.nodes[to];
    let mut best: HashMap<usize, f64> = HashMap::from([(from, 0.0)]);
    let mut prev: HashMap<usize, usize> = HashMap::new();
    let mut open = BinaryHeap::from([Open(distance(graph.nodes[from], goal), from)]);
    while let Some(Open(_, current)) = open.pop() {
        if current == to {
            let mut path = vec![to];
            let mut at = to;
            while let Some(&p) = prev.get(&at) {
                path.push(p);
                at = p;
            }
            path.reverse();
            return Some(Route {
                path,
                meters: best[&to],
            });
        }
        let g = best[&current];
        for &(next, meters) in graph.edges.get(&current).into_iter().flatten() {
            let candidate = g + meters;
            if best.get(&next).is_none_or(|&b| candidate < b) {
                best.insert(next, candidate);
                prev.insert(next, current);
                open.push(Open(candidate + distance(graph.nodes[next], goal), next));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(lat: f64, lon: f64) -> Place {
        Place { lat, lon }
    }

    #[test]
    fn haversine_sanity() {
        // ~111km per degree of latitude at the equator
        let d = distance(p(0.0, 0.0), p(1.0, 0.0));
        assert!((d - 111_195.0).abs() < 200.0, "got {d}");
        assert!(near(p(0.0, 0.0), p(0.001, 0.0), 200.0));
        assert!(!near(p(0.0, 0.0), p(0.01, 0.0), 200.0));
    }

    #[test]
    fn a_star_picks_the_shorter_road() {
        // 0 -> 1 -> 3 is shorter than 0 -> 2 -> 3
        let g = MapGraph {
            nodes: vec![p(0.0, 0.0), p(0.0, 0.001), p(0.002, 0.0), p(0.0, 0.002)],
            edges: HashMap::from([
                (0, vec![(1, 111.0), (2, 222.0)]),
                (1, vec![(3, 111.0)]),
                (2, vec![(3, 300.0)]),
            ]),
        };
        let r = route(&g, 0, 3).unwrap();
        assert_eq!(r.path, vec![0, 1, 3]);
        assert!((r.meters - 222.0).abs() < 1e-9);
        assert!((r.eta_seconds(1.0) - 222.0).abs() < 1e-9);
        assert!(route(&g, 3, 0).is_none(), "directed graph: no way back");
    }
}
