use super::*;
use lince_interface::markup::{Block, Run};

#[derive(PartialEq)]
struct Scope {
    owner: Entity,
    source: Source,
    base: Option<String>,
    record: Option<String>,
    root: Entity,
    ancestors: Vec<String>,
}

impl Scope {
    fn current(world: &World, parent: Entity, context: &Context) -> Self {
        let mut scope = Self {
            owner: context.owner,
            source: context.source.clone(),
            base: None,
            record: None,
            root: transclusion::root(world, parent),
            ancestors: transclusion::ancestors(world, parent),
        };
        let mut cursor = Some(context.owner);
        while let Some(owner) = cursor {
            if scope.base.is_none() {
                scope.base = world
                    .get::<crate::native::source_view::ReferenceBase>(owner)
                    .map(|base| base.0.clone());
            }
            if scope.record.is_none() {
                scope.record = world
                    .get::<crate::native::protein_area::RecordBinding>(owner)
                    .map(|binding| binding.uid.clone());
            }
            cursor = world.get::<ChildOf>(owner).map(ChildOf::parent);
        }
        scope
    }
}

struct View {
    block: Block,
    roots: Vec<Entity>,
}

impl View {
    fn belongs_to(&self, world: &World, parent: Entity) -> bool {
        self.roots.iter().all(|root| {
            world
                .get::<ChildOf>(*root)
                .is_some_and(|child| child.parent() == parent)
                && world.get::<live::SubscriptionRefused>(*root).is_none()
        }) && (self.block.transclusion.is_none()
            || self.roots.len() == 1
                && transclusion::is_view(world, self.roots[0])
                && live::watched(world, self.roots[0]))
    }

    fn can_patch(&self, world: &World, block: &Block) -> bool {
        if self.block.image.is_some()
            || block.image.is_some()
            || self.block.transclusion.is_some()
            || block.transclusion.is_some()
            || self.block.heading != block.heading
            || self.block.quote != block.quote
            || self.roots.len() != 1
        {
            return false;
        }
        if let Some(language) = self.block.code.as_deref() {
            return self.block.code == block.code
                && !language.trim().eq_ignore_ascii_case("wgsl")
                && !language.trim().eq_ignore_ascii_case("mermaid")
                && world.get::<Text>(self.roots[0]).is_some();
        }
        if block.code.is_some() || self.block.runs.len() != block.runs.len() {
            return false;
        }
        let Some(children) = world.get::<Children>(self.roots[0]) else {
            return false;
        };
        children.len() == block.runs.len()
            && children
                .iter()
                .all(|child| world.get::<TextSpan>(child).is_some())
            && self
                .block
                .runs
                .iter()
                .zip(&block.runs)
                .all(|(old, new)| same_style(old, new))
    }

    fn patch(&self, world: &mut World, block: &Block) {
        if block.code.is_some() {
            let mut text = world.get_mut::<Text>(self.roots[0]).unwrap();
            if let [run] = block.runs.as_slice() {
                if text.0 != run.text {
                    text.0.clone_from(&run.text);
                }
            } else {
                let value: String = block.runs.iter().map(|run| run.text.as_str()).collect();
                if text.0 != value {
                    text.0 = value;
                }
            }
            return;
        }
        let children = world.get::<Children>(self.roots[0]).unwrap().to_vec();
        for ((old, new), child) in self.block.runs.iter().zip(&block.runs).zip(children) {
            if old.text != new.text {
                world
                    .get_mut::<TextSpan>(child)
                    .unwrap()
                    .0
                    .clone_from(&new.text);
            }
        }
    }
}

fn same_style(old: &Run, new: &Run) -> bool {
    old.bold == new.bold
        && old.italic == new.italic
        && old.code == new.code
        && old.strike == new.strike
        && old.link == new.link
        && old.literal == new.literal
}

#[derive(Component)]
struct Retained {
    scope: Scope,
    blocks: Vec<View>,
}

pub(in crate::native::description) fn render(
    world: &mut World,
    parent: Entity,
    source: &str,
    context: &Context,
) {
    world.init_resource::<Fonts>();
    let blocks = parse(source);
    let scope = Scope::current(world, parent, context);
    let previous = world.entity_mut(parent).take::<Retained>();
    let same_scope = previous
        .as_ref()
        .is_some_and(|previous| previous.scope == scope);
    let old = previous.map_or_else(Vec::new, |previous| previous.blocks);
    let common = old.len().min(blocks.len());
    let equal = |before: usize, after: usize| {
        same_scope && old[before].block == blocks[after] && old[before].belongs_to(world, parent)
    };
    let prefix = (0..common)
        .take_while(|index| equal(*index, *index))
        .count();
    let suffix = (0..common - prefix)
        .take_while(|index| equal(old.len() - 1 - index, blocks.len() - 1 - index))
        .count();
    let mut matches = vec![None; blocks.len()];
    for (index, matched) in matches.iter_mut().take(prefix).enumerate() {
        *matched = Some(index);
    }
    for offset in 0..suffix {
        matches[blocks.len() - 1 - offset] = Some(old.len() - 1 - offset);
    }
    if same_scope {
        for (index, block) in blocks
            .iter()
            .enumerate()
            .take(blocks.len() - suffix)
            .skip(prefix)
        {
            if index < old.len() - suffix
                && old[index].belongs_to(world, parent)
                && (old[index].block == *block || old[index].can_patch(world, block))
            {
                matches[index] = Some(index);
            }
        }
    }
    let mut old: Vec<_> = old.into_iter().map(Some).collect();
    let retained: Vec<_> = matches
        .into_iter()
        .map(|index| index.and_then(|index| old[index].take()))
        .collect();
    let mut shaders = std::collections::VecDeque::new();
    for view in old.into_iter().flatten() {
        for root in view.roots {
            if world
                .get::<ChildOf>(root)
                .is_none_or(|child| child.parent() != parent)
            {
                continue;
            }
            if world.get::<shader::Preview>(root).is_some() {
                world.entity_mut(root).remove::<ChildOf>();
                shaders.push_back(root);
            } else {
                world.despawn(root);
            }
        }
    }
    let mut views = Vec::with_capacity(blocks.len());
    for (block, retained) in blocks.into_iter().zip(retained) {
        let roots = if let Some(view) = retained {
            if view.block == block {
                crate::host::performance::count(
                    crate::host::performance::Count::DescriptionBlocksRetained,
                );
            } else {
                view.patch(world, &block);
                crate::host::performance::count(
                    crate::host::performance::Count::DescriptionBlocksPatched,
                );
            }
            view.roots
        } else {
            let start = world
                .get::<Children>(parent)
                .map_or(0, |children| children.len());
            let shader = block
                .code
                .as_deref()
                .is_some_and(|language| language.trim().eq_ignore_ascii_case("wgsl"))
                .then(|| shaders.pop_front())
                .flatten();
            spawn_block(
                world,
                parent,
                &block,
                context,
                scope.base.as_deref(),
                shader,
            );
            crate::host::performance::count(
                crate::host::performance::Count::DescriptionBlocksRebuilt,
            );
            world
                .get::<Children>(parent)
                .map_or_else(Vec::new, |children| children[start..].to_vec())
        };
        views.push(View { block, roots });
    }
    for shader in shaders {
        world.despawn(shader);
    }
    let ordered = views
        .iter()
        .flat_map(|view| view.roots.iter().copied())
        .collect();
    crate::native::retained::order(world, parent, ordered);
    world.entity_mut(parent).insert(Retained {
        scope,
        blocks: views,
    });
}
