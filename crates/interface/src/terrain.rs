use nucleus::terrain::{Preset, TopologyEffect};

#[derive(Clone, Debug)]
pub struct Draft {
    pub name: String,
    pub amplitude: String,
    pub radius_x: String,
    pub radius_y: String,
    pub rotation: String,
    pub falloff: String,
    pub formula: String,
}

impl From<&TopologyEffect> for Draft {
    fn from(effect: &TopologyEffect) -> Self {
        Self {
            name: effect.name.clone(),
            amplitude: effect.amplitude.to_string(),
            radius_x: effect.extent[0].to_string(),
            radius_y: effect.extent[1].to_string(),
            rotation: effect.rotation.to_degrees().to_string(),
            falloff: effect.falloff.to_string(),
            formula: effect.formula.clone(),
        }
    }
}

impl Draft {
    pub fn effect(&self, current: &TopologyEffect) -> Result<TopologyEffect, String> {
        let number = |source: &str| {
            source
                .trim()
                .parse::<f64>()
                .map_err(|_| "Enter a finite number.".to_owned())
        };
        let effect = TopologyEffect {
            name: self.name.trim().into(),
            enabled: current.enabled,
            preset: current.preset,
            amplitude: number(&self.amplitude)?,
            extent: [number(&self.radius_x)?, number(&self.radius_y)?],
            rotation: number(&self.rotation)?.to_radians(),
            falloff: number(&self.falloff)?,
            formula: self.formula.clone(),
        };
        effect.compile()?;
        Ok(effect)
    }

    pub fn preset_name(preset: Preset) -> &'static str {
        match preset {
            Preset::Hill => "Hill",
            Preset::Valley => "Valley",
            Preset::Slope => "Slope",
            Preset::Ring => "Ring",
            Preset::Formula => "Formula",
        }
    }
}
