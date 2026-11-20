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

fn fields(area: &InfluenceArea) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    match serde_json::to_value(area).map_err(|error| error.to_string())? {
        serde_json::Value::Object(object) => Ok(object),
        _ => Err("Area settings must encode as a JSON object.".into()),
    }
}

fn configuration(area: &InfluenceArea) -> Result<serde_json::Value, String> {
    let mut value = fields(area)?;
    for key in OWNED {
        value.remove(*key);
    }
    Ok(serde_json::Value::Object(value))
}

fn encode_settings(
    area: &InfluenceArea,
    configuration: &serde_json::Value,
) -> Result<BTreeMap<String, serde_json::Value>, String> {
    Ok(BTreeMap::from([
        (
            "configuration".into(),
            snapshot::json_text("Area settings", configuration)?,
        ),
        (
            "activate".into(),
            serde_json::json!(area.enabled && !area.paused),
        ),
    ]))
}

pub(super) fn settings(
    area: &InfluenceArea,
) -> Result<BTreeMap<String, serde_json::Value>, String> {
    encode_settings(area, &configuration(area)?)
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
    let mut value = fields(&area)?;
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
            if OWNED.contains(&key.as_str()) || !value.contains_key(key) {
                return Err(format!(
                    "Area field {key} is unknown or controlled by placement/Record ownership."
                ));
            }
            value.insert(key.clone(), entry.clone());
        }
    }
    area = serde_json::from_value(serde_json::Value::Object(value))
        .map_err(|error| error.to_string())?;
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

pub(super) fn component(area: &InfluenceArea) -> Result<ContentKind, String> {
    let value = configuration(area)?;
    Ok(ContentKind::Native {
        kind: "area".into(),
        settings: encode_settings(area, &value)?,
        bindings: record_references(&value),
    })
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
    let captured = self::component(&area)?;
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
