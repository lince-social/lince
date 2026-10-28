use super::*;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Mastering {
    pub primaries: Option<[[i32; 2]; 4]>,
    pub luminances: Option<[u32; 2]>,
    pub max_cll: Option<u32>,
    pub max_fall: Option<u32>,
}

impl Mastering {
    pub fn valid(self, description: Description) -> bool {
        let [minimum, maximum] = self.luminances.unwrap_or([description.minimum_luminance, description.maximum_luminance]);
        if minimum < description.minimum_luminance || maximum > description.maximum_luminance
            || u64::from(minimum) >= u64::from(maximum) * 10000 {
            return false;
        }
        if [self.max_cll, self.max_fall].into_iter().flatten().any(|value| {
            value > maximum || u64::from(value) * 10000 <= u64::from(minimum)
        }) || self.max_cll.zip(self.max_fall).is_some_and(|(cll, fall)| fall > cll) {
            return false;
        }
        let Some(primaries) = self.primaries else { return true; };
        let container = description.primaries.chromaticities();
        if primaries.iter().any(|point| point[0] < 0 || point[1] <= 0 || i64::from(point[0]) + i64::from(point[1]) > 1_000_000) {
            return false;
        }
        let triangle = [primaries[0], primaries[1], primaries[2]];
        contains(triangle, primaries[3]) && primaries.iter().all(|point| contains([container[0], container[1], container[2]], *point))
    }
}

fn contains(triangle: [[i32; 2]; 3], point: [i32; 2]) -> bool {
    let cross = |a: [i32; 2], b: [i32; 2], p: [i32; 2]| {
        (i64::from(b[0]) - i64::from(a[0])) * (i64::from(p[1]) - i64::from(a[1]))
            - (i64::from(b[1]) - i64::from(a[1])) * (i64::from(p[0]) - i64::from(a[0]))
    };
    let area = cross(triangle[0], triangle[1], triangle[2]);
    if area == 0 { return false; }
    (0..3).all(|index| {
        let side = cross(triangle[index], triangle[(index + 1) % 3], point);
        side == 0 || side.signum() == area.signum()
    })
}

pub fn pq_to_nits(value: f64) -> Option<f64> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) { return None; }
    let power = value.powf(32.0 / 2523.0);
    let numerator = (power - 3424.0 / 4096.0).max(0.0);
    let denominator = 2413.0 / 128.0 - (2392.0 / 128.0) * power;
    Some(10000.0 * (numerator / denominator).powf(16384.0 / 2610.0))
}

pub fn nits_to_pq(value: f64) -> Option<f64> {
    if !value.is_finite() || !(0.0..=10000.0).contains(&value) { return None; }
    if value == 0.0 { return Some(0.0); }
    let power = (value / 10000.0).powf(2610.0 / 16384.0);
    Some(((3424.0 / 4096.0 + (2413.0 / 128.0) * power) / (1.0 + (2392.0 / 128.0) * power)).powf(2523.0 / 32.0))
}
