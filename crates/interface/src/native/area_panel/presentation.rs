use super::*;

pub(super) struct Values {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub hole: bool,
    pub color: [u8; 3],
    pub opacity: f32,
    pub has_filter: bool,
    pub match_all: bool,
    pub rules: Vec<PropertyRule>,
    pub attraction_enabled: bool,
    pub changes_enabled: bool,
    pub reach: crate::native::area::Reach,
    pub target: AttractionTarget,
    pub shape: ShapeKind,
    pub center: [f64; 2],
    pub size: [f64; 2],
    pub depth: f64,
}

impl From<&InfluenceArea> for Values {
    fn from(area: &InfluenceArea) -> Self {
        Self {
            id: area.id.clone(),
            name: area.name.clone(),
            enabled: area.enabled,
            hole: area.hole,
            color: area.color,
            opacity: area.opacity,
            has_filter: area.filter.is_some(),
            match_all: area.match_all,
            rules: if area.filter.is_none() {
                area.rules.clone()
            } else {
                Vec::new()
            },
            attraction_enabled: area.attraction_enabled,
            changes_enabled: area.changes_enabled,
            reach: area.reach,
            target: area.target,
            shape: area.shape.kind(),
            center: area.center,
            size: area.size,
            depth: area.depth,
        }
    }
}
