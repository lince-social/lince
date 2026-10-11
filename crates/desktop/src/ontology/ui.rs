use super::*;
use crate::edit_mode::label;

pub(super) fn populate(world: &mut World, sand: Entity) {
    let body = panel::frame(world, sand, "Ontology");
    let tabs = panel::row(world, body);
    for (index, title) in [
        "Linguas",
        "Concepts and hierarchy",
        "Record assertions",
        "Terms and translations",
    ]
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
        language: Entity::PLACEHOLDER,
        definition: Entity::PLACEHOLDER,
        preferred: false,
        personal: false,
        term: String::new(),
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
    if tab == 3 {
        chooser(world, owner, parent, 0, "Source Vocabulary (Lingua)");
        chooser(world, owner, parent, 1, "Stable Concept");
        let name = panel::field(world, parent, "Term or original name", "");
        let language = panel::field(
            world,
            parent,
            "Language tag (en, pt-BR, el-Latn; empty for all languages)",
            "",
        );
        let definition = panel::field(world, parent, "Definition for a new source Concept", "");
        let mut view = world.get_mut::<OntologySand>(owner).unwrap();
        view.name = name;
        view.language = language;
        view.definition = definition;
        let controls = panel::row(world, parent);
        for (title, command) in [
            ("Create source Concept", Command::Create),
            ("Add alternative", Command::AddTerm),
            ("Edit selected term", Command::EditTerm),
            ("Remove selected term", Command::RetireTerm),
            ("Toggle preferred term", Command::Preferred),
            ("Toggle personal term", Command::Personal),
            ("Use this display language", Command::Language),
        ] {
            panel::button(world, controls, owner, title, command);
        }
        let bindings = panel::row(world, parent);
        panel::button(
            world,
            bindings,
            owner,
            "Accept for saved locations",
            Command::BindLocation(true, false),
        );
        panel::button(
            world,
            bindings,
            owner,
            "Use when linking addresses",
            Command::BindLocation(true, true),
        );
        panel::button(
            world,
            bindings,
            owner,
            "Remove location binding",
            Command::BindLocation(false, false),
        );
    } else if tab < 2 {
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
    let source = if tab == 2 {
        3
    } else if tab == 3 {
        1
    } else {
        tab
    };
    let mut rows = view.rows[source].clone();
    rows.sort_by_key(title);
    let total = rows.len();
    let page = view.page.min(total.saturating_sub(1) / PAGE);
    let catalogs = view.rows.clone();
    panel::clear(world, list);
    if tab == 3 {
        let view = world.get::<OntologySand>(owner).unwrap();
        let preferred = view.preferred;
        let personal = view.personal;
        let selected = view.rows[1]
            .iter()
            .find(|row| row["uid"] == view.choices[1])
            .cloned();
        label(
            world,
            list,
            &format!(
                "New term: {} · {}",
                if preferred {
                    "preferred"
                } else {
                    "alternative"
                },
                if personal {
                    "personal (signed-in Person)"
                } else {
                    "shared on this Lince"
                }
            ),
            13.0,
        );
        if let Some(row) = selected {
            let source = &row["source"];
            label(
                world,
                list,
                &format!(
                    "Source: #{} · {} · {}",
                    source["original_name"].as_str().unwrap_or(""),
                    source["original_language"]
                        .as_str()
                        .unwrap_or("all languages"),
                    row["uid"].as_str().unwrap_or("")
                ),
                14.0,
            );
            label(
                world,
                list,
                source["definition"].as_str().unwrap_or(""),
                13.0,
            );
            label(
                world,
                list,
                &format!(
                    "Saved location binding: {}",
                    if row["location_write"].as_bool().unwrap_or(false) {
                        "accepted and used for new links"
                    } else if row["saved_location"].as_bool().unwrap_or(false) {
                        "accepted"
                    } else {
                        "unbound"
                    }
                ),
                13.0,
            );
            let terms = row["terms"].as_array().map(Vec::as_slice).unwrap_or(&[]);
            let page = world
                .get::<OntologySand>(owner)
                .unwrap()
                .page
                .min(terms.len().saturating_sub(1) / PAGE);
            let selected_term = world.get::<OntologySand>(owner).unwrap().term.clone();
            if page > 0 {
                panel::button(world, list, owner, "Previous terms", Command::Page(false));
            }
            for term in terms.iter().skip(page * PAGE).take(PAGE) {
                if let Some(uid) = term["uid"].as_str() {
                    panel::button(
                        world,
                        list,
                        owner,
                        &format!(
                            "#{} · {}{}{}{}",
                            term["name"].as_str().unwrap_or(""),
                            term["language"]
                                .as_str()
                                .filter(|language| !language.is_empty())
                                .unwrap_or("all languages"),
                            if term["preferred"].as_bool().unwrap_or(false) {
                                " · preferred"
                            } else {
                                ""
                            },
                            if term["owner_uid"].is_string() {
                                " · personal"
                            } else {
                                ""
                            },
                            if uid == selected_term {
                                " · selected"
                            } else {
                                ""
                            },
                        ),
                        Command::Term(uid.into()),
                    );
                }
            }
            if (page + 1) * PAGE < terms.len() {
                panel::button(world, list, owner, "Next terms", Command::Page(true));
            }
        }
        world.get_mut::<OntologySand>(owner).unwrap().dirty = false;
        return;
    }
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
