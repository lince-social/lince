use super::*;
use crate::edit_mode::label;

pub(super) fn populate(world: &mut World, sand: Entity) {
    let body = panel::frame(world, sand, "Ontology");
    let tabs = panel::row(world, body);
    for (index, title) in ["Linguas", "Concepts and hierarchy", "Record assertions"]
        .iter()
        .enumerate()
    {
        panel::button(world, tabs, sand, title, Command::Tab(index));
    }
    panel::button(world, tabs, sand, "Refresh", Command::Refresh);
    let status = label(world, body, "Loading ontology…", 12.0);
    let scroll = panel::column(world, body);
    {
        let mut node = world.get_mut::<Node>(scroll).unwrap();
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
        node.overflow = Overflow::scroll_y();
    }
    crate::scroll_sand::attach(world, scroll);
    let editor = panel::column(world, scroll);
    let list = panel::column(world, scroll);
    let navigation = panel::row(world, body);
    panel::button(world, navigation, sand, "Previous", Command::Page(false));
    panel::button(world, navigation, sand, "Next", Command::Page(true));
    world.entity_mut(sand).insert(OntologySand {
        editor,
        list,
        status,
        tab: 0,
        page: 0,
        name: Entity::PLACEHOLDER,
        choices: [
            "g_local".into(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "private".into(),
        ],
        choice_pages: [0; 6],
        choosers: vec![],
        rows: Default::default(),
        ids: std::array::from_fn(|_| nucleus::new_uid("ontology")),
        requested: [false; 4],
        ready: [false; 4],
        pending: None,
        dirty: true,
    });
    self::editor(world, sand);
    render(world, sand);
}

fn chooser(world: &mut World, owner: Entity, parent: Entity, slot: usize, caption: &str) {
    label(world, parent, caption, 13.0);
    let holder = panel::column(world, parent);
    world
        .get_mut::<OntologySand>(owner)
        .unwrap()
        .choosers
        .push((holder, slot));
}

pub(super) fn editor(world: &mut World, owner: Entity) {
    let view = world.get::<OntologySand>(owner).unwrap();
    let (parent, tab) = (view.editor, view.tab);
    panel::clear(world, parent);
    world
        .get_mut::<OntologySand>(owner)
        .unwrap()
        .choosers
        .clear();
    if tab < 2 {
        chooser(world, owner, parent, 0, "Lingua");
        if tab == 1 {
            chooser(world, owner, parent, 1, "Concept");
        }
        let name = panel::field(world, parent, "Name (for create or rename)", "");
        world.get_mut::<OntologySand>(owner).unwrap().name = name;
        if tab == 0 {
            chooser(world, owner, parent, 5, "Visibility for new Lingua");
        }
        let controls = panel::row(world, parent);
        panel::button(world, controls, owner, "Create", Command::Create);
        panel::button(world, controls, owner, "Rename selected", Command::Rename);
        panel::button(world, controls, owner, "Delete selected", Command::Delete);
        if tab == 1 {
            let membership = panel::row(world, parent);
            panel::button(
                world,
                membership,
                owner,
                "Include in Lingua",
                Command::Include(true),
            );
            panel::button(
                world,
                membership,
                owner,
                "Remove from Lingua",
                Command::Include(false),
            );
            chooser(
                world,
                owner,
                parent,
                2,
                "Parent concept (selected concept is a…)",
            );
            let hierarchy = panel::row(world, parent);
            panel::button(
                world,
                hierarchy,
                owner,
                "Connect parent",
                Command::Parent(true),
            );
            panel::button(
                world,
                hierarchy,
                owner,
                "Disconnect parent",
                Command::Parent(false),
            );
        }
    } else {
        chooser(world, owner, parent, 3, "Subject Record");
        chooser(world, owner, parent, 1, "Predicate concept");
        chooser(world, owner, parent, 4, "Object Record (optional for tags)");
        let controls = panel::row(world, parent);
        panel::button(world, controls, owner, "Assert", Command::Assert);
        panel::button(world, controls, owner, "Set identity", Command::Identity);
        label(
            world,
            parent,
            "Set identity uses only the subject and concept.",
            12.0,
        );
    }
}

fn title(row: &Value) -> String {
    row["name"]
        .as_str()
        .or_else(|| row["head"].as_str().filter(|value| !value.is_empty()))
        .or_else(|| row["slug"].as_str())
        .or_else(|| row["uid"].as_str())
        .unwrap_or("Unnamed")
        .into()
}

fn reference(rows: &[Value], uid: &str) -> String {
    rows.iter()
        .find(|row| row["uid"] == uid)
        .map(title)
        .unwrap_or_else(|| uid.into())
}

fn references(row: &Value, key: &str, rows: &[Value]) -> String {
    row[key]
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(|uid| reference(rows, uid))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let view = world.get::<OntologySand>(owner).unwrap();
    let holders = view.choosers.clone();
    for (holder, slot) in holders {
        let view = world.get::<OntologySand>(owner).unwrap();
        let source = match slot {
            0 => 0,
            1 | 2 => 1,
            _ => 2,
        };
        let mut options: Vec<(String, String)> = if slot == 5 {
            ["private", "shared", "public"]
                .iter()
                .map(|value| (value.to_string(), value.to_string()))
                .collect()
        } else {
            view.rows[source]
                .iter()
                .filter_map(|row| row["uid"].as_str().map(|uid| (uid.into(), title(row))))
                .collect()
        };
        options.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
        if slot == 4 {
            options.insert(0, (String::new(), "No object — tag".into()));
        }
        let selected = options
            .iter()
            .find(|(uid, _)| uid == &view.choices[slot])
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| "Choose…".into());
        let total = options.len();
        let page = view.choice_pages[slot].min(total.saturating_sub(1) / PAGE);
        let choices = options
            .into_iter()
            .skip(page * PAGE)
            .take(PAGE)
            .map(|(uid, caption)| (caption, crate::actions![Command::Choose(slot, uid)]))
            .collect();
        world.get_mut::<OntologySand>(owner).unwrap().choice_pages[slot] = page;
        panel::clear(world, holder);
        let group = crate::dropdown::spawn(
            world,
            holder,
            owner,
            "Choose ontology entry",
            &selected,
            choices,
        );
        let menu = world
            .get::<Children>(group)
            .and_then(|children| children.get(1))
            .copied();
        if let Some(menu) = menu {
            let mut node = world.get_mut::<Node>(menu).unwrap();
            node.max_height = px(240);
            node.overflow = Overflow::scroll_y();
            crate::scroll_sand::attach(world, menu);
        }
        if total > PAGE {
            let navigation = panel::row(world, holder);
            panel::button(
                world,
                navigation,
                owner,
                "Previous choices",
                Command::ChoicePage(slot, false),
            );
            label(
                world,
                navigation,
                &format!(
                    "{}–{} of {total}",
                    page * PAGE + 1,
                    ((page + 1) * PAGE).min(total)
                ),
                12.0,
            );
            panel::button(
                world,
                navigation,
                owner,
                "Next choices",
                Command::ChoicePage(slot, true),
            );
        }
    }
    let view = world.get::<OntologySand>(owner).unwrap();
    let (list, tab) = (view.list, view.tab);
    let source = if tab == 2 { 3 } else { tab };
    let mut rows = view.rows[source].clone();
    rows.sort_by_key(title);
    let total = rows.len();
    let page = view.page.min(total.saturating_sub(1) / PAGE);
    let catalogs = view.rows.clone();
    panel::clear(world, list);
    label(
        world,
        list,
        &format!("{} entries · page {}", total, page + 1),
        12.0,
    );
    for row in rows.into_iter().skip(page * PAGE).take(PAGE) {
        let Some(uid) = row["uid"].as_str() else {
            continue;
        };
        let entry = panel::column(world, list);
        if tab < 2 {
            panel::button(
                world,
                entry,
                owner,
                &title(&row),
                Command::Choose(tab, uid.into()),
            );
            let meta = if tab == 0 {
                format!(
                    "{} · {} concepts{}",
                    row["visibility"].as_str().unwrap_or("private"),
                    row["concepts"].as_array().map_or(0, Vec::len),
                    if uid == "g_local" {
                        " · permanent"
                    } else {
                        ""
                    }
                )
            } else {
                let parents = references(&row, "parents", &catalogs[1]);
                format!(
                    "{} · Linguas: {}",
                    if parents.is_empty() {
                        "Root concept".into()
                    } else {
                        format!("Is a {parents}")
                    },
                    references(&row, "linguas", &catalogs[0])
                )
            };
            label(world, entry, &meta, 12.0);
        } else {
            let subject = reference(&catalogs[2], row["subject"].as_str().unwrap_or(""));
            let predicate = reference(&catalogs[1], row["predicate"].as_str().unwrap_or(""));
            let object = row["object"]
                .as_str()
                .map(|uid| format!(" [{}]", reference(&catalogs[2], uid)))
                .unwrap_or_default();
            label(
                world,
                entry,
                &format!("{subject} @{predicate}{object}"),
                14.0,
            );
            label(
                world,
                entry,
                row["role"].as_str().unwrap_or("assertion"),
                12.0,
            );
            panel::button(world, entry, owner, "Retract", Command::Retract(uid.into()));
        }
    }
    let mut view = world.get_mut::<OntologySand>(owner).unwrap();
    view.page = page;
    view.dirty = false;
}
