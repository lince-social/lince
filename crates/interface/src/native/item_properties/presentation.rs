use super::*;
use crate::host::performance::{self, Count, Stage};

pub(super) struct Refresh {
    inspectors: QueryState<(Entity, &'static Inspector)>,
    pages: QueryState<Entity, With<Page>>,
    prepared_inspectors: Vec<(Entity, Entity, Entity)>,
    prepared_pages: Vec<Entity>,
}

impl FromWorld for Refresh {
    fn from_world(world: &mut World) -> Self {
        Self {
            inspectors: world.query(),
            pages: world.query_filtered(),
            prepared_inspectors: Vec::new(),
            prepared_pages: Vec::new(),
        }
    }
}

impl Refresh {
    pub(super) fn refresh(&mut self, world: &mut World) {
        let _refresh = performance::Span::new(Stage::ItemPropertyRefresh);
        {
            let _metadata = performance::Span::new(Stage::ItemPropertyMetadata);
            self.prepared_inspectors.clear();
            self.prepared_inspectors.extend(
                self.inspectors
                    .iter(world)
                    .map(|(entity, inspector)| (entity, inspector.root, inspector.target)),
            );
            if self.prepared_inspectors.is_empty() {
                self.prepared_inspectors.shrink_to_fit();
            }
            for (owner, root, target) in self.prepared_inspectors.drain(..) {
                if world.get_entity(target).is_err() || world.get::<Workspaces>(root).is_none() {
                    world.despawn(owner);
                    continue;
                }
                let changed = metadata_change(
                    world,
                    target,
                    world.get::<crate::native::full_record::window::Metadata>(owner),
                );
                if let Some(metadata) = changed {
                    world.entity_mut(owner).insert(metadata);
                }
            }
        }
        let _feedback = performance::Span::new(Stage::ItemPropertyFeedback);
        self.prepared_pages.clear();
        self.prepared_pages.extend(self.pages.iter(world));
        if self.prepared_pages.is_empty() {
            self.prepared_pages.shrink_to_fit();
        }
        for owner in self.prepared_pages.drain(..) {
            page(world, owner);
        }
    }
}

fn page(world: &mut World, owner: Entity) {
    let page = world.get::<Page>(owner).unwrap();
    let attachment = world.get::<Attachment>(page.target);
    let value = attachment.map_or("none", |attachment| attachment.uid.as_str());
    const PREFIX: &str = "Record properties: ";
    let summary = world
        .get::<Text>(page.summary)
        .filter(|current| current.0.strip_prefix(PREFIX) != Some(value))
        .map(|_| {
            performance::count(Count::ItemPropertyLabelsPrepared);
            format!("{PREFIX}{value}")
        });
    let value = if page.message.is_empty() {
        attachments::status(world, page.target)
    } else {
        page.message.as_str()
    };
    let message = world.get::<Text>(page.status).and_then(|current| {
        let current = if page.status == page.summary {
            summary.as_deref().unwrap_or(current.0.as_str())
        } else {
            current.0.as_str()
        };
        (current != value).then(|| {
            performance::count(Count::ItemPropertyLabelsPrepared);
            value.to_owned()
        })
    });
    let fields = (page.summary, page.status, page.detach, page.open);
    let missing = attachment.is_none() || !eligible(world, page.root, page.target);
    if let Some(summary) = summary {
        input_settings::text(world, fields.0, summary);
    }
    if let Some(message) = message {
        input_settings::text(world, fields.1, message);
    }
    input_settings::disable(world, fields.2, missing);
    input_settings::disable(world, fields.3, missing);
}
