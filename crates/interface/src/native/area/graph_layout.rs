use serde::{Deserialize, Serialize};

/// A layout acts in the Area's plane. Distances are measured between card edges
/// in canvas units, so opening a card does not consume its requested gap.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    pub center: f64,
    pub repulsion: f64,
    pub link_spacing: f64,
    pub clearance: f64,
    pub cooling: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            center: 1.6,
            repulsion: 3200.0,
            link_spacing: 110.0,
            clearance: 24.0,
            cooling: 0.48,
        }
    }
}

impl Settings {
    pub fn valid(&self) -> bool {
        (0.0..=100.0).contains(&self.center)
            && (0.0..=100_000.0).contains(&self.repulsion)
            && (0.0..=10_000.0).contains(&self.link_spacing)
            && (0.0..=10_000.0).contains(&self.clearance)
            && (0.1..=10.0).contains(&self.cooling)
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Parameter {
    Center,
    Repulsion,
    LinkSpacing,
    Clearance,
    Cooling,
}

impl Parameter {
    pub(crate) const ALL: [Self; 5] = [
        Self::Center,
        Self::Repulsion,
        Self::LinkSpacing,
        Self::Clearance,
        Self::Cooling,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Center => "Center pull",
            Self::Repulsion => "Node repulsion",
            Self::LinkSpacing => "Link spacing",
            Self::Clearance => "Card clearance",
            Self::Cooling => "Cooling rate",
        }
    }

    pub(crate) fn description(self) -> &'static str {
        match self {
            Self::Center => "Pull toward the Area target on its plane (0 to 100).",
            Self::Repulsion => "Strength of local repulsion between nearby cards (0 to 100000).",
            Self::LinkSpacing => {
                "Preferred gap between the edges of linked cards, in canvas units (0 to 10000)."
            }
            Self::Clearance => {
                "Space to leave around neighboring cards, in canvas units (0 to 10000)."
            }
            Self::Cooling => {
                "How quickly layout forces fade after interaction. Higher values settle sooner (0.1 to 10)."
            }
        }
    }

    pub(crate) fn field(self, settings: &mut Settings) -> &mut f64 {
        match self {
            Self::Center => &mut settings.center,
            Self::Repulsion => &mut settings.repulsion,
            Self::LinkSpacing => &mut settings.link_spacing,
            Self::Clearance => &mut settings.clearance,
            Self::Cooling => &mut settings.cooling,
        }
    }

    pub(crate) fn value(self, mut settings: Settings) -> f64 {
        *self.field(&mut settings)
    }
}
