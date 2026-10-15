use super::*;
use crate::native::area::InfluenceArea;

const OWNED: &[&str] = &[
    "id",
    "center",
    "size",
    "records",
    "backend_components",
    "enabled",
    "paused",
];

pub(super) fn settings(area: &InfluenceArea) -> BTreeMap<String, serde_json::Value> {
    let mut value = serde_json::to_value(area).unwrap();
    for key in OWNED {
        value.as_object_mut().unwrap().remove(*key);
    }
    BTreeMap::from([
        (
            "configuration".into(),
            serde_json::json!(serde_json::to_string(&value).unwrap()),
        ),
        (
            "activate".into(),
            serde_json::json!(area.enabled && !area.paused),
        ),
    ])
}

pub(super) fn parse(
    settings: &BTreeMap<String, serde_json::Value>,
    source: Option<&InfluenceArea>,
    geometry: &api::Geometry,
) -> Result<InfluenceArea, String> {
    let mut area = source.cloned().unwrap_or_else(|| {
        InfluenceArea::new(
            crate::native::component_push::rectangle(),
            DVec2::from_array(geometry.position),
            DVec2::from_array(geometry.size),
        )
    });
    let mut value = serde_json::to_value(&area).unwrap();
    if let Some(configuration) = settings.get("configuration") {
        let configuration: serde_json::Value = serde_json::from_str(
            configuration
                .as_str()
                .ok_or("Area configuration is JSON text.")?,
        )
        .map_err(|error| error.to_string())?;
        let object = configuration
            .as_object()
            .ok_or("Area configuration needs an object.")?;
        for (key, entry) in object {
            if OWNED.contains(&key.as_str()) || !value.as_object().unwrap().contains_key(key) {
                return Err(format!(
                    "Area field {key} is unknown or controlled by placement/Record ownership."
                ));
            }
            value[key] = entry.clone();
        }
    }
    area = serde_json::from_value(value).map_err(|error| error.to_string())?;
    area.center = geometry.position;
    area.size = geometry.size;
    area.paused = !settings
        .get("activate")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    area.enabled = !area.paused;
    if !area.validate() {
        return Err("Invalid influence area configuration.".into());
    }
    Ok(area)
}

pub(super) fn component(area: &InfluenceArea) -> ContentKind {
    let settings = settings(area);
    let value: serde_json::Value =
        serde_json::from_str(settings["configuration"].as_str().unwrap()).unwrap();
    ContentKind::Native {
        kind: "area".into(),
        settings,
        bindings: record_references(&value),
    }
}

pub(super) fn is_area(component: &ContentKind) -> bool {
    matches!(api::authoring::base(component), ContentKind::Native { kind, .. } if kind == "area")
}

pub(super) fn validate(
    world: &World,
    entity: Entity,
    component: &ContentKind,
    geometry: &api::Geometry,
) -> Result<InfluenceArea, String> {
    let ContentKind::Native {
        settings, bindings, ..
    } = api::authoring::base(component)
    else {
        return Err("Expected native area settings.".into());
    };
    let existing = world
        .get::<InfluenceArea>(entity)
        .ok_or("The native area is missing.")?;
    let area = parse(settings, Some(existing), geometry)?;
    let captured = self::component(&area);
    if captured
        .records()
        .iter()
        .any(|uid| !bindings.iter().any(|binding| binding == uid))
    {
        return Err("List the area's referenced Record UIDs in bindings.".into());
    }
    if existing.backend_components
        || world.get::<composition::Generated>(entity).is_some()
        || world
            .get::<crate::native::component_push::composition::Generated>(entity)
            .is_some()
    {
        return Err(
            "Generated hosts keep their protected isolation; edit their content parts.".into(),
        );
    }
    Ok(area)
}

pub(super) fn apply(
    world: &mut World,
    entity: Entity,
    area: InfluenceArea,
    component: &ContentKind,
) {
    crate::native::area_mutation::disarm(
        world,
        entity,
        "Area configuration changed through native authoring; review property changes again.",
    );
    world
        .entity_mut(entity)
        .insert((area, Content(component.clone())));
}
