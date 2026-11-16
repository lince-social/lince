use super::*;
use crate::host::performance::{self, Count, Span, Stage};
use crate::native::retained;
use nucleus::plugins::{BindingStatus, Delivery, Package};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct Rows(retained::Children<Key>);

#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    Diagnostic(usize),
    EmptyPackages,
    Package(String),
    Binding(String),
    Delivery(String),
}

#[derive(Clone, Copy, PartialEq)]
enum Row<'a> {
    Text(&'a str),
    Package(&'a Package),
    Binding(&'a BindingStatus),
    Delivery(&'a Delivery, bool),
}

#[derive(Component)]
struct StatusLabel(Entity);

fn binding_caption(item: &BindingStatus) -> String {
    format!(
        "{} · {} · {} pending · {} need attention",
        item.binding.name, item.status, item.pending, item.failed
    )
}

fn delivery_caption(delivery: &Delivery) -> String {
    format!(
        "Delivery {} · {} · {}",
        delivery.record, delivery.state, delivery.result
    )
}

fn can_resolve(delivery: &Delivery) -> bool {
    matches!(delivery.state.as_str(), "queued" | "failed" | "uncertain")
}

fn status_only(previous: Row<'_>, current: Row<'_>) -> Option<String> {
    match (previous, current) {
        (Row::Binding(before), Row::Binding(after)) if before.binding == after.binding => {
            Some(binding_caption(after))
        }
        (Row::Delivery(before, old_retry), Row::Delivery(after, retry))
            if before.uid == after.uid
                && old_retry == retry
                && can_resolve(before) == can_resolve(after) =>
        {
            Some(delivery_caption(after))
        }
        _ => None,
    }
}

fn items(catalog: &Catalog) -> Vec<(Key, Row<'_>)> {
    // Resolve cross-row action inputs once, retaining the original first-match
    // semantics. A delivery's retry control depends on its provider, not only
    // on the delivery row itself.
    let mut providers = HashMap::new();
    for package in &catalog.packages {
        for contribution in &package.manifest.contributions {
            providers
                .entry(format!("{}/{}", package.manifest.id, contribution.id()))
                .or_insert(matches!(
                    contribution,
                    Contribution::Sync {
                        replay_safe: true,
                        ..
                    }
                ));
        }
    }
    let mut bindings = HashMap::new();
    for item in &catalog.bindings {
        bindings
            .entry(item.binding.uid.as_str())
            .or_insert_with(|| {
                providers
                    .get(item.binding.provider.as_str())
                    .copied()
                    .unwrap_or(false)
            });
    }
    let mut rows = Vec::with_capacity(
        catalog.diagnostics.len()
            + catalog.packages.len()
            + catalog.bindings.len()
            + catalog.deliveries.len()
            + 1,
    );
    for (index, diagnostic) in catalog.diagnostics.iter().enumerate() {
        rows.push((Key::Diagnostic(index), Row::Text(diagnostic)));
    }
    if catalog.packages.is_empty() {
        rows.push((Key::EmptyPackages, Row::Text("Install with cargo xtask plugin-example, plugin-activitypub or plugin-irc <data-directory>, then reload packages here.")));
    }
    rows.extend(catalog.packages.iter().map(|package| {
        (
            Key::Package(package.manifest.id.clone()),
            Row::Package(package),
        )
    }));
    rows.extend(
        catalog
            .bindings
            .iter()
            .map(|item| (Key::Binding(item.binding.uid.clone()), Row::Binding(item))),
    );
    rows.extend(catalog.deliveries.iter().map(|delivery| {
        let retry = matches!(delivery.state.as_str(), "failed" | "uncertain")
            && bindings
                .get(delivery.binding.as_str())
                .copied()
                .unwrap_or(false);
        (
            Key::Delivery(delivery.uid.clone()),
            Row::Delivery(delivery, retry),
        )
    }));
    rows
}

impl Rows {
    pub(super) fn render(
        &mut self,
        world: &mut World,
        parent: Entity,
        owner: Entity,
        previous: Option<&Catalog>,
        catalog: &Catalog,
    ) {
        let _span = Span::new(Stage::NativePluginCatalogPresentation);
        let previous: HashMap<_, _> = previous.into_iter().flat_map(items).collect();
        let rows = items(catalog).into_iter().map(|(key, row)| {
            let before = previous.get(&key).copied();
            (key, (row, before))
        });
        let (mut created, mut retained, mut rebuilt) = (0, 0, 0);
        self.0.reconcile(
            world,
            parent,
            rows,
            |world, parent, &(row, _)| {
                created += 1;
                if let Row::Text(text) = row {
                    return label(world, parent, text);
                }
                let entity = panel::column(world, parent);
                fill(world, entity, owner, row);
                entity
            },
            |world, entity, &(row, before)| {
                retained += 1;
                if before == Some(row) {
                    return;
                }
                if let Row::Text(text) = row {
                    panel::status(world, entity, text);
                } else {
                    if let Some(text) = before.and_then(|before| status_only(before, row))
                        && let Some(header) =
                            world.get::<StatusLabel>(entity).map(|header| header.0)
                        && world.get::<Text>(header).is_some()
                    {
                        panel::status(world, header, text);
                        return;
                    }
                    rebuilt += 1;
                    panel::clear(world, entity);
                    fill(world, entity, owner, row);
                }
            },
        );
        performance::add(Count::SyncCatalogRowsCreated, created);
        performance::add(Count::SyncCatalogRowsRetained, retained);
        performance::add(Count::SyncCatalogRowsRebuilt, rebuilt);
    }
}

fn fill(world: &mut World, row: Entity, owner: Entity, item: Row<'_>) {
    match item {
        Row::Text(text) => {
            label(world, row, text);
        }
        Row::Package(package) => {
            let compatibility = &package.manifest.compatibility;
            label(
                world,
                row,
                &format!(
                    "Declared publisher: {} · Origin: {}",
                    compatibility.publisher, compatibility.origin
                ),
            );
            if !compatibility.dependencies.is_empty() {
                label(
                    world,
                    row,
                    &format!("Required packages: {:?}", compatibility.dependencies),
                );
            }
            panel::button(
                world,
                row,
                owner,
                "Remove package; keep saved data",
                Command::Run(Backend::Remove {
                    plugin: package.manifest.id.clone(),
                    digest: package.digest.clone(),
                }),
            );
            label(
                world,
                row,
                &format!(
                    "{} · version {} · {}",
                    package.manifest.name,
                    package.manifest.version,
                    if package.enabled {
                        "Enabled"
                    } else {
                        "Disabled"
                    }
                ),
            );
            panel::button(
                world,
                row,
                owner,
                if package.enabled {
                    "Disable package"
                } else {
                    "Enable trusted native package"
                },
                Command::Run(Backend::Enable {
                    plugin: package.manifest.id.clone(),
                    digest: package.digest.clone(),
                    enabled: !package.enabled,
                }),
            );
            panel::button(
                world,
                row,
                owner,
                "Licenses and credits",
                Command::Licenses(package.manifest.id.clone()),
            );
            for contribution in &package.manifest.contributions {
                let target = format!("{}/{}", package.manifest.id, contribution.id());
                match contribution {
                    Contribution::Schema { namespace, .. } => {
                        label(
                            world,
                            row,
                            &format!("Record schema: {} · {namespace}", contribution.name()),
                        );
                    }
                    _ if package.enabled => {
                        panel::button(
                            world,
                            row,
                            owner,
                            contribution.name(),
                            Command::Open(target, None),
                        );
                    }
                    _ => {
                        label(world, row, contribution.name());
                    }
                }
            }
        }
        Row::Binding(item) => {
            let header = label(world, row, &binding_caption(item));
            world.entity_mut(row).insert(StatusLabel(header));
            panel::button(
                world,
                row,
                owner,
                "Edit / start",
                Command::Open(item.binding.provider.clone(), Some(item.binding.clone())),
            );
            if item.binding.enabled {
                panel::button(
                    world,
                    row,
                    owner,
                    "Refresh now",
                    Command::Run(Backend::Run {
                        binding: item.binding.uid.clone(),
                    }),
                );
                panel::button(
                    world,
                    row,
                    owner,
                    "Stop Sync",
                    Command::Run(Backend::Stop {
                        binding: item.binding.uid.clone(),
                    }),
                );
            }
        }
        Row::Delivery(delivery, retry_allowed) => {
            let header = label(world, row, &delivery_caption(delivery));
            world.entity_mut(row).insert(StatusLabel(header));
            if retry_allowed {
                panel::button(
                    world,
                    row,
                    owner,
                    "Retry same prepared publication",
                    Command::Run(Backend::Retry {
                        delivery: delivery.uid.clone(),
                    }),
                );
            }
            if can_resolve(delivery) {
                panel::button(
                    world,
                    row,
                    owner,
                    "Resolve locally without resending",
                    Command::Run(Backend::ResolveDelivery {
                        delivery: delivery.uid.clone(),
                    }),
                );
            }
        }
    }
}
