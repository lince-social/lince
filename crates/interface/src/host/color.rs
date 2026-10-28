use serde::{Deserialize, Serialize};

mod hdr;
pub use hdr::{Mastering, nits_to_pq, pq_to_nits};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeEncoding {
    #[default]
    Srgb,
    LinearRelative,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ColorMode {
    #[default]
    Srgb,
    Hdr10,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OutputColor {
    pub mode: ColorMode,
    pub peak_nits: u32,
}

impl Default for OutputColor {
    fn default() -> Self { Self { mode: ColorMode::Srgb, peak_nits: 1000 } }
}

impl ColorMode {
    pub fn label(self) -> &'static str {
        match self { Self::Srgb => "SDR sRGB", Self::Hdr10 => "HDR10" }
    }

    pub fn output_uniforms(self, peak_nits: u32) -> Option<[[f32; 4]; 4]> {
        if !(203..=10000).contains(&peak_nits) { return None; }
        static TO_BT2020: std::sync::OnceLock<[[f64; 3]; 3]> = std::sync::OnceLock::new();
        let matrix = if self == Self::Hdr10 { *TO_BT2020.get_or_init(|| inverse(Description::hdr10().matrix())) }
            else { Description::default().matrix() };
        let row = |values: [f64; 3]| [values[0] as f32, values[1] as f32, values[2] as f32, 0.0];
        Some([row(matrix[0]), row(matrix[1]), row(matrix[2]),
            [if self == Self::Hdr10 { 1.0 } else { 0.0 }, peak_nits as f32 / 80.0, 203.0 / 80.0, 0.0]])
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Primaries {
    #[default]
    Srgb,
    DisplayP3,
    Bt2020,
}

impl Primaries {
    pub fn chromaticities(self) -> [[i32; 2]; 4] {
        match self {
            Self::Srgb => [[640000, 330000], [300000, 600000], [150000, 60000], [312700, 329000]],
            Self::DisplayP3 => [[680000, 320000], [265000, 690000], [150000, 60000], [312700, 329000]],
            Self::Bt2020 => [[708000, 292000], [170000, 797000], [131000, 46000], [312700, 329000]],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Transfer {
    #[default]
    Srgb,
    Gamma22,
    Pq,
    Linear,
}

impl Transfer {
    pub fn decode(self, value: f64) -> Option<f64> {
        if !value.is_finite() || if self == Self::Linear { value.abs() > 65504.0 } else { !(0.0..=1.0).contains(&value) } {
            return None;
        }
        Some(match self {
            Self::Srgb => if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) },
            Self::Gamma22 => value.powf(2.2),
            Self::Pq => pq_to_nits(value)? / 80.0,
            Self::Linear => value,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Description {
    pub primaries: Primaries,
    pub transfer: Transfer,
    pub minimum_luminance: u32,
    pub maximum_luminance: u32,
    pub reference_luminance: u32,
    pub mastering: Mastering,
}

impl Default for Description {
    fn default() -> Self {
        Self { primaries: Primaries::Srgb, transfer: Transfer::Srgb,
            minimum_luminance: 2000, maximum_luminance: 80, reference_luminance: 80, mastering: Default::default() }
    }
}

impl Description {
    pub fn valid(self) -> bool {
        u64::from(self.minimum_luminance) < u64::from(self.maximum_luminance) * 10000
            && u64::from(self.minimum_luminance) < u64::from(self.reference_luminance) * 10000
    }

    pub fn supported_sdr(self) -> bool {
        matches!(self.transfer, Transfer::Srgb | Transfer::Gamma22)
            && matches!(self.primaries, Primaries::Srgb | Primaries::DisplayP3)
            && self.supported()
    }

    pub fn supported(self) -> bool {
        self.valid() && self.reference_luminance <= self.maximum_luminance && self.maximum_luminance <= 10000
            && (self.transfer != Transfer::Pq || self.maximum_luminance == 10000)
            && self.mastering.valid(self)
    }

    pub fn hdr10() -> Self {
        Self { primaries: Primaries::Bt2020, transfer: Transfer::Pq,
            minimum_luminance: 50, maximum_luminance: 10000, reference_luminance: 203,
            mastering: Default::default() }
    }

    pub fn linear_working() -> Self {
        Self { transfer: Transfer::Linear, minimum_luminance: 0, maximum_luminance: 10000,
            ..Default::default() }
    }

    fn matrix(self) -> [[f64; 3]; 3] {
        static P3: std::sync::OnceLock<[[f64; 3]; 3]> = std::sync::OnceLock::new();
        static BT2020: std::sync::OnceLock<[[f64; 3]; 3]> = std::sync::OnceLock::new();
        match self.primaries {
            Primaries::Srgb => [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            Primaries::DisplayP3 => *P3.get_or_init(|| multiply(inverse(rgb_to_xyz(Primaries::Srgb)), rgb_to_xyz(Primaries::DisplayP3))),
            Primaries::Bt2020 => *BT2020.get_or_init(|| multiply(inverse(rgb_to_xyz(Primaries::Srgb)), rgb_to_xyz(Primaries::Bt2020))),
        }
    }

    fn gain(self) -> f64 {
        if matches!(self.transfer, Transfer::Pq | Transfer::Linear) { return 1.0; }
        let minimum = f64::from(self.minimum_luminance) / 10000.0;
        (f64::from(self.maximum_luminance) - minimum) / (f64::from(self.reference_luminance) - minimum)
    }

    pub fn uniforms(self) -> [[f32; 4]; 4] {
        let matrix = self.matrix();
        let transfer = match self.transfer {
            Transfer::Srgb => 0.0, Transfer::Gamma22 => 1.0, Transfer::Pq => 2.0, Transfer::Linear => 3.0,
        };
        let row = |values: [f64; 3]| [values[0] as f32, values[1] as f32, values[2] as f32, 0.0];
        [row(matrix[0]), row(matrix[1]), row(matrix[2]), [transfer, self.gain() as f32, 0.0, 0.0]]
    }

    pub fn native_uniforms(self) -> [[f32; 4]; 4] {
        let mut uniforms = self.uniforms();
        if matches!(self.transfer, Transfer::Pq | Transfer::Linear) { uniforms[3][1] *= 80.0 / 203.0; }
        uniforms
    }

    pub fn linear_srgb(self, encoded: [f64; 3]) -> Option<[f64; 3]> {
        if !self.supported() { return None; }
        let values = [self.transfer.decode(encoded[0])?, self.transfer.decode(encoded[1])?, self.transfer.decode(encoded[2])?];
        let matrix = self.matrix();
        Some(std::array::from_fn(|axis| (0..3).map(|index| matrix[axis][index] * values[index]).sum::<f64>() * self.gain()))
    }
}

fn rgb_to_xyz(primaries: Primaries) -> [[f64; 3]; 3] {
    let chromaticities = primaries.chromaticities().map(|xy| xy.map(|value| f64::from(value) / 1_000_000.0));
    let columns = [0, 1, 2].map(|index| { let xy = chromaticities[index]; [xy[0] / xy[1], 1.0, (1.0 - xy[0] - xy[1]) / xy[1]] });
    let matrix = std::array::from_fn(|row| std::array::from_fn(|column| columns[column][row]));
    let white = chromaticities[3];
    let white = [white[0] / white[1], 1.0, (1.0 - white[0] - white[1]) / white[1]];
    let inverted = inverse(matrix);
    let scale: [f64; 3] = std::array::from_fn(|row| (0..3).map(|column| inverted[row][column] * white[column]).sum());
    std::array::from_fn(|row| std::array::from_fn(|column| matrix[row][column] * scale[column]))
}

fn multiply(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    std::array::from_fn(|row| std::array::from_fn(|column| (0..3).map(|index| a[row][index] * b[index][column]).sum()))
}

fn inverse(matrix: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let [[a, b, c], [d, e, f], [g, h, i]] = matrix;
    let determinant = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    [[e * i - f * h, c * h - b * i, b * f - c * e],
        [f * g - d * i, a * i - c * g, c * d - a * f],
        [d * h - e * g, b * g - a * h, a * e - b * d]].map(|row| row.map(|value| value / determinant))
}

#[cfg(test)]
mod tests;
