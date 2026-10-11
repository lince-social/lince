use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub lat: f64,
    pub lon: f64,
}

const EARTH_RADIUS_M: f64 = 6_371_000.0;

pub const MIN_AREA_RADIUS_METRES: u32 = 100;
pub const MAX_AREA_RADIUS_METRES: u32 = 100_000;

pub fn valid_area_radius(radius: u32) -> bool {
    (MIN_AREA_RADIUS_METRES..=MAX_AREA_RADIUS_METRES).contains(&radius)
}

pub fn approximate(point: Place, radius_metres: u32) -> Option<Place> {
    if !valid_area_radius(radius_metres)
        || !point.lat.is_finite()
        || !point.lon.is_finite()
        || !(-90.0..=90.0).contains(&point.lat)
        || !(-180.0..=180.0).contains(&point.lon)
    {
        return None;
    }
    let step = (f64::from(radius_metres) / EARTH_RADIUS_M).to_degrees();
    let center = |value: f64, span: f64| {
        let cells = (span / step).ceil();
        let width = span / cells;
        let index = ((value + span / 2.0) / width)
            .floor()
            .clamp(0.0, cells - 1.0);
        -span / 2.0 + (index + 0.5) * width
    };
    Some(Place {
        lat: center(point.lat, 180.0),
        lon: center((point.lon + 180.0).rem_euclid(360.0) - 180.0, 360.0),
    })
}

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

#[derive(Debug, Clone, Default)]
pub struct MapGraph {
    pub nodes: Vec<Place>,
    pub edges: HashMap<usize, Vec<(usize, f64)>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    pub path: Vec<usize>,
    pub meters: f64,
}

impl Route {
    pub fn eta_seconds(&self, speed_m_s: f64) -> f64 {
        if speed_m_s <= 0.0 {
            f64::INFINITY
        } else {
            self.meters / speed_m_s
        }
    }
}

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
            other.0.total_cmp(&self.0)
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
        let d = distance(p(0.0, 0.0), p(1.0, 0.0));
        assert!((d - 111_195.0).abs() < 200.0, "got {d}");
        assert!(near(p(0.0, 0.0), p(0.001, 0.0), 200.0));
        assert!(!near(p(0.0, 0.0), p(0.01, 0.0), 200.0));
    }

    #[test]
    fn a_star_picks_the_shorter_road() {
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
