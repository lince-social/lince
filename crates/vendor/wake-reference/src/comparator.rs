use std::cmp;

pub struct MfccComparator {
    score_ref: f32,
    band_size: u16,
}
impl MfccComparator {
    pub fn new(score_ref: f32, band_size: u16) -> Self {
        MfccComparator {
            score_ref,
            band_size,
        }
    }
    pub fn calculate_distance(ax: &[f32], bx: &[f32]) -> f32 {
        1. - cosine_similarity(ax, bx)
    }
    pub fn compare(&self, a: &[&[f32]], b: &[&[f32]]) -> f32 {
        if a.is_empty() || b.is_empty() { return 0.0; }
        let window = usize::from(self.band_size).max(a.len().abs_diff(b.len()));
        let mut previous = vec![f32::INFINITY; b.len() + 1];
        let mut current = previous.clone();
        previous[0] = 0.0;
        for (index, frame) in a.iter().enumerate() {
            current.fill(f32::INFINITY);
            let row = index + 1;
            let start = row.saturating_sub(window).max(1);
            let end = (row + window).min(b.len());
            for column in start..=end {
                current[column] = Self::calculate_distance(frame, b[column - 1])
                    + previous[column].min(current[column - 1]).min(previous[column - 1]);
            }
            std::mem::swap(&mut previous, &mut current);
        }
        let cost = previous[b.len()];
        let normalized_cost = cost / (a.len() + b.len()) as f32;
        self.compute_probability(normalized_cost)
    }
    fn compute_probability(&self, cost: f32) -> f32 {
        1. / (1. + ((cost - self.score_ref) / self.score_ref).exp())
    }
}
pub fn cosine_similarity(vector_a: &[f32], vector_b: &[f32]) -> f32 {
    let dimensionality = cmp::min(vector_a.len(), vector_b.len());
    let mut dot_ab = 0.;
    let mut dot_a = 0.;
    let mut dot_b = 0.;
    let mut dimension = 0;
    while dimension < dimensionality {
        let component_a = vector_a[dimension];
        let component_b = vector_b[dimension];
        dot_ab += component_a * component_b;
        dot_a += component_a * component_a;
        dot_b += component_b * component_b;
        dimension += 1;
    }
    let magnitude = f32::sqrt(dot_a * dot_b);
    if magnitude == 0. {
        0.
    } else {
        dot_ab / magnitude
    }
}
