use super::*;
use nucleus::{
    trust_analysis::{Condition, Filter, Identity},
    visibility::Bound,
};

fn value(world: &World, key: &str, default: &str) -> String {
    world
        .resource::<Mobile>()
        .draft("trust-policy", key, default)
}

fn count(world: &World, name: &str, side: &str) -> Result<usize, String> {
    value(world, &format!("{name}-{side}-count"), "0")
        .parse::<usize>()
        .ok()
        .filter(|count| *count <= 32)
        .ok_or_else(|| "Use at most 32 conditions on each side".into())
}

pub(super) fn read(world: &World, name: &str) -> Result<Filter, String> {
    let mut filter = Filter::default();
    for side in ["include", "exclude"] {
        for index in 0..count(world, name, side)? {
            let prefix = format!("{name}-{side}-{index}");
            let field =
                |suffix: &str, default: &str| value(world, &format!("{prefix}-{suffix}"), default);
            let mut identities = Vec::new();
            for (key, organ) in [("organs", true), ("persons", false)] {
                for uid in field(key, "")
                    .split([',', '\n'])
                    .map(str::trim)
                    .filter(|uid| !uid.is_empty())
                {
                    let identity = if organ {
                        Identity::Organ(uid.into())
                    } else {
                        Identity::Person(uid.into())
                    };
                    identity.validate()?;
                    identities.push(identity);
                }
            }
            let bound = |key: &str| -> Result<Option<Bound>, String> {
                let raw = field(key, "");
                if raw.trim().is_empty() {
                    return Ok(None);
                }
                Ok(Some(Bound {
                    value: raw
                        .trim()
                        .parse()
                        .map_err(|_| "Proximity must be a whole number")?,
                    inclusive: field(&format!("{key}-inclusive"), "true") == "true",
                }))
            };
            let condition = Condition {
                identities,
                known: match field("known", "either").as_str() {
                    "known" => Some(true),
                    "unknown" => Some(false),
                    _ => None,
                },
                lower: bound("lower")?,
                upper: bound("upper")?,
            };
            if side == "include" {
                filter.include.push(condition);
            } else {
                filter.exclude.push(condition);
            }
        }
    }
    filter.validate()?;
    Ok(filter)
}

fn choices(
    world: &mut World,
    parent: Entity,
    label: &str,
    key: &str,
    default: &str,
    options: &[(&str, &str)],
) {
    let current = value(world, key, default);
    app::label(world, parent, &format!("{label}: {current}"), 15.0);
    for (caption, selected) in options {
        button(
            world,
            parent,
            caption,
            Intent::TrustValue(key.into(), (*selected).into()),
        );
    }
}

pub(super) fn render(world: &mut World, parent: Entity) {
    choices(
        world,
        parent,
        "Subject role",
        "role",
        "giver",
        &[("Contributing", "giver"), ("Receiving", "receiver")],
    );
    choices(
        world,
        parent,
        "Counterparty attribution",
        "basis",
        "organ",
        &[("Organs", "organ"), ("Consenting Persons", "person")],
    );
    app::label(world, parent, "Evidence included in the analysis", 18.0);
    for (key, title, default) in [
        ("need", "Need-linked receiver confirmation", "true"),
        ("receiver", "Receiver-only reports", "false"),
        ("giver", "Giver-only claims", "false"),
    ] {
        choices(
            world,
            parent,
            title,
            &format!("level-{key}"),
            default,
            &[("Include", "true"), ("Exclude", "false")],
        );
    }
    for (name, caption) in [
        ("endpoints", "Discovery endpoints"),
        ("issuers", "Reporting sources"),
        ("counterparties", "Transfer counterparties"),
        ("intermediaries", "Contact-path intermediaries"),
        ("targets", "Contact-path targets"),
    ] {
        button(world, parent, caption, Intent::TrustFilter(name.into()));
    }
    let name = world.resource::<State>().filter_editor.clone();
    if name.is_empty() {
        return;
    }
    app::label(
        world,
        parent,
        &format!("{} conditions", model::human(&name)),
        18.0,
    );
    app::label(
        world,
        parent,
        "Includes combine with OR; fields within a condition combine with AND. Exclusions win. Lower contact values mean closer; blank bounds are unlimited. An empty include list imposes no additional filter.",
        14.0,
    );
    for side in ["include", "exclude"] {
        let count = count(world, &name, side).unwrap_or_default();
        for index in 0..count {
            app::label(
                world,
                parent,
                &format!("{side} condition {}", index + 1),
                17.0,
            );
            let prefix = format!("{name}-{side}-{index}");
            for (key, label) in [
                ("organs", "Organ Records (comma-separated)"),
                ("persons", "Person Records (comma-separated)"),
                ("lower", "Minimum contact value"),
                ("upper", "Maximum contact value"),
            ] {
                app::input(
                    world,
                    parent,
                    "trust-policy",
                    &format!("{prefix}-{key}"),
                    label,
                    "",
                    false,
                );
            }
            choices(
                world,
                parent,
                "Known contact",
                &format!("{prefix}-known"),
                "either",
                &[
                    ("Either", "either"),
                    ("Known", "known"),
                    ("Unknown", "unknown"),
                ],
            );
            for key in ["lower", "upper"] {
                choices(
                    world,
                    parent,
                    &format!("{key} boundary"),
                    &format!("{prefix}-{key}-inclusive"),
                    "true",
                    &[("Inclusive", "true"), ("Strict", "false")],
                );
            }
        }
        if count < 32 {
            button(
                world,
                parent,
                &format!("Add {side} condition"),
                Intent::TrustValue(format!("{name}-{side}-count"), (count + 1).to_string()),
            );
        }
        if count > 0 {
            button(
                world,
                parent,
                &format!("Remove last {side} condition"),
                Intent::TrustValue(format!("{name}-{side}-count"), (count - 1).to_string()),
            );
        }
    }
    button(
        world,
        parent,
        "Close filter editor",
        Intent::TrustFilter(String::new()),
    );
}
