use super::*;
use crate::i18n::Token as Tr;
use crate::native::edit_mode::label;

pub(super) fn populate(world: &mut World, sand: Entity) {
    const CREDITS: &[crate::native::credits::Attribution] = &[
        crate::native::credits::Attribution {
            name: "quick-xml",
            author: "The quick-xml contributors",
            license: include_str!("../../../licenses/native/knowledge-quick-xml-MIT.txt"),
        },
        crate::native::credits::Attribution {
            name: "reqwest",
            author: "Sean McArthur and contributors",
            license: include_str!("../../../licenses/native/knowledge-reqwest-MIT.txt"),
        },
        crate::native::credits::Attribution {
            name: "rfd",
            author: "Poly Meilex and contributors",
            license: include_str!("../../../licenses/native/document/rfd-MIT.txt"),
        },
    ];
    world.spawn((
        crate::native::sand_store::SandCredits(CREDITS),
        ChildOf(sand),
    ));
    let body = panel::frame(world, sand, Tr::Ontology.text());
    let tabs = panel::row(world, body);
    for (index, title) in [
        Tr::Vocabularies.text(),
        Tr::ConceptHierarchy.text(),
        Tr::RecordAssertions.text(),
        Tr::TermsTranslations.text(),
        Tr::RecordHistory.text(),
        Tr::KnowledgeSources.text(),
    ]
    .iter()
    .enumerate()
    {
        panel::button(world, tabs, sand, title, Command::Tab(index));
    }
    panel::button(world, tabs, sand, Tr::Refresh.text(), Command::Refresh);
    let status = label(world, body, Tr::LoadingOntology.text(), 12.0);
    let scroll = panel::column(world, body);
    {
        let mut node = world.get_mut::<Node>(scroll).unwrap();
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
        node.overflow = Overflow::scroll_y();
    }
    crate::native::scroll_sand::attach(world, scroll);
    let editor = panel::column(world, scroll);
    let list = panel::column(world, scroll);
    let navigation = panel::row(world, body);
    panel::button(
        world,
        navigation,
        sand,
        Tr::Previous.text(),
        Command::Page(false),
    );
    panel::button(
        world,
        navigation,
        sand,
        Tr::Next.text(),
        Command::Page(true),
    );
    world.entity_mut(sand).insert(OntologySand {
        editor,
        list,
        status,
        tab: 0,
        page: 0,
        name: Entity::PLACEHOLDER,
        language: Entity::PLACEHOLDER,
        definition: Entity::PLACEHOLDER,
        package: Entity::PLACEHOLDER,
        context: Entity::PLACEHOLDER,
        feature: "assignment".into(),
        knowledge_sources: vec![],
        knowledge_selected: vec!["test".into()],
        knowledge_results: vec![],
        knowledge_job: None,
        knowledge_poll: std::time::Instant::now(),
        knowledge_page: 0,
        knowledge_filter: String::new(),
        knowledge_item: None,
        knowledge_claim: None,
        knowledge_availability: String::new(),
        knowledge_preview: None,
        knowledge_review: None,
        knowledge_fields: vec![],
        knowledge_cached: vec![],
        knowledge_cache_bytes: 0,
        knowledge_file: None,
        output: None,
        package_page: 0,
        definition_page: 0,
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
            String::new(),
        ],
        choice_pages: [0; 7],
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
    if tab == 5 {
        let name = panel::field(
            world,
            parent,
            "Search local Records and selected knowledge sources",
            "",
        );
        world.get_mut::<OntologySand>(owner).unwrap().name = name;
        let controls = panel::row(world, parent);
        world.get_mut::<Node>(controls).unwrap().flex_wrap = FlexWrap::Wrap;
        for (title, command) in [
            ("List sources", Command::KnowledgeSources),
            ("Search selected sources", Command::KnowledgeSearch(0)),
            ("Cancel loading", Command::KnowledgeCancel),
            ("Clear my temporary content", Command::KnowledgeClear),
        ] {
            panel::button(world, controls, owner, title, command);
        }
        let package = panel::field(
            world,
            parent,
            "Archive, dump selection, or portable knowledge package",
            "",
        );
        world.get_mut::<OntologySand>(owner).unwrap().package = package;
        world
            .get_mut::<bevy::text::EditableText>(package)
            .unwrap()
            .max_characters = None;
        panel::button(
            world,
            parent,
            owner,
            "Open local knowledge file",
            Command::KnowledgeLoadFile,
        );
        panel::button(
            world,
            parent,
            owner,
            "Add offline source",
            Command::KnowledgeInstall,
        );
        let language = panel::field(world, parent, "Dump language (en, pt, ar, …)", "en");
        world.get_mut::<OntologySand>(owner).unwrap().definition = language;
        let imports = panel::row(world, parent);
        world.get_mut::<Node>(imports).unwrap().flex_wrap = FlexWrap::Wrap;
        for (title, command) in [
            (
                "Import Wikidata JSON selection",
                Command::KnowledgeDump("wikidata_json".into()),
            ),
            (
                "Import Wikipedia XML selection",
                Command::KnowledgeDump("wikipedia_xml".into()),
            ),
            (
                "Replace selected offline archive",
                Command::KnowledgeReplace,
            ),
            ("Import portable knowledge", Command::KnowledgeImport),
        ] {
            panel::button(world, imports, owner, title, command);
        }
        let optional = panel::field(
            world,
            parent,
            "Optional related item and asset IDs to save (comma separated)",
            "",
        );
        world.get_mut::<OntologySand>(owner).unwrap().context = optional;
        chooser(
            world,
            owner,
            parent,
            1,
            "Local Concept for an explicit source meaning mapping",
        );
        let external = panel::field(
            world,
            parent,
            "External meaning identifier (from the source item)",
            "",
        );
        world.get_mut::<OntologySand>(owner).unwrap().language = external;
        let mappings = panel::row(world, parent);
        panel::button(
            world,
            mappings,
            owner,
            "Map as a predicate",
            Command::KnowledgeMap(nucleus::knowledge::MeaningKind::Predicate),
        );
        panel::button(
            world,
            mappings,
            owner,
            "Map as a unit",
            Command::KnowledgeMap(nucleus::knowledge::MeaningKind::Unit),
        );
        chooser(
            world,
            owner,
            parent,
            6,
            "Organ for kept references and saved knowledge",
        );
        chooser(
            world,
            owner,
            parent,
            3,
            "Saved knowledge or reference Record",
        );
        let saving = panel::row(world, parent);
        world.get_mut::<Node>(saving).unwrap().flex_wrap = FlexWrap::Wrap;
        for (title, command) in [
            ("Keep a reference Record", Command::KnowledgeReference),
            ("Preview saving content", Command::KnowledgePreviewSave),
            ("Save reviewed content", Command::KnowledgeSave),
            ("Read saved content offline", Command::KnowledgeInspectSaved),
            (
                "Export knowledge with dependencies",
                Command::KnowledgeExport,
            ),
            ("Review latest source changes", Command::KnowledgeReview),
        ] {
            panel::button(world, saving, owner, title, command);
        }
        label(
            world,
            parent,
            "Choose changes to accept after reviewing the source",
            13.0,
        );
        let updates = panel::row(world, parent);
        world.get_mut::<Node>(updates).unwrap().flex_wrap = FlexWrap::Wrap;
        for (name, field) in [
            (Tr::Title.text(), nucleus::knowledge::UpdateField::Title),
            ("Content", nucleus::knowledge::UpdateField::Content),
            ("Claims", nucleus::knowledge::UpdateField::Claims),
            ("Attribution", nucleus::knowledge::UpdateField::Attribution),
            ("Assets", nucleus::knowledge::UpdateField::Assets),
        ] {
            panel::button(world, updates, owner, name, Command::KnowledgeField(field));
        }
        let accept = panel::row(world, parent);
        world.get_mut::<Node>(accept).unwrap().flex_wrap = FlexWrap::Wrap;
        panel::button(
            world,
            accept,
            owner,
            "Accept selected source changes",
            Command::KnowledgeAccept(false),
        );
        panel::button(
            world,
            accept,
            owner,
            "Create a separate local copy",
            Command::KnowledgeAccept(true),
        );
    } else if tab == 3 {
        let context = panel::field(world, parent, "Terminology context", "General");
        world.get_mut::<OntologySand>(owner).unwrap().context = context;
        let contexts = panel::row(world, parent);
        panel::button(world, contexts, owner, "Use context", Command::Context);
        panel::button(
            world,
            contexts,
            owner,
            "Inspect my choices",
            Command::InspectTerminology,
        );
        chooser(world, owner, parent, 0, "Source Vocabulary (Lingua)");
        chooser(world, owner, parent, 1, "Stable Concept");
        let name = panel::field(world, parent, Tr::TermName.text(), "");
        let language = panel::field(world, parent, Tr::LanguageTag.text(), "");
        label(world, parent, Tr::VocabularyLanguageHelp.text(), 13.0);
        let definition = panel::field(world, parent, Tr::DefinitionNew.text(), "");
        let mut view = world.get_mut::<OntologySand>(owner).unwrap();
        view.name = name;
        view.language = language;
        view.definition = definition;
        let controls = panel::row(world, parent);
        for (title, command) in [
            (Tr::CreateMeaning.text(), Command::Create),
            (Tr::ReviseDefinition.text(), Command::ReviseDefinition),
            (Tr::InspectMeaning.text(), Command::InspectMeaning(0)),
            (Tr::AddAlternative.text(), Command::AddTerm),
            (Tr::EditTerm.text(), Command::EditTerm),
            (Tr::RemoveTerm.text(), Command::RetireTerm),
            (Tr::PreferredTerm.text(), Command::Preferred),
            (Tr::PersonalTerm.text(), Command::Personal),
            (Tr::UseDisplayLanguage.text(), Command::Language),
            (Tr::UseContextWording.text(), Command::PreferTerm),
            (Tr::UseEnteredWording.text(), Command::UseWording),
            (Tr::HideSourceTerm.text(), Command::HideTerm(true)),
            (Tr::ShowSourceTerm.text(), Command::HideTerm(false)),
            (Tr::OriginalWording.text(), Command::RestoreOriginal),
            (Tr::AutomaticWording.text(), Command::ResetTerminology),
        ] {
            panel::button(world, controls, owner, title, command);
        }
        label(world, parent, Tr::FeatureMeanings.text(), 13.0);
        let features = panel::row(world, parent);
        world.get_mut::<Node>(features).unwrap().flex_wrap = FlexWrap::Wrap;
        for feature in store::vocabulary::FEATURES {
            let caption = feature.replace('_', " ");
            panel::button(
                world,
                features,
                owner,
                &caption,
                Command::Feature(feature.into()),
            );
        }
        let controls = panel::row(world, parent);
        for (title, command) in [
            (
                "Preview accepting this meaning",
                Command::PreviewFeature(true, false),
            ),
            (
                "Preview writing this meaning",
                Command::PreviewFeature(true, true),
            ),
            (
                "Preview removing this meaning",
                Command::PreviewFeature(false, false),
            ),
            ("Apply reviewed feature change", Command::ApplyFeature),
        ] {
            panel::button(world, controls, owner, title, command);
        }
    } else if tab < 2 {
        chooser(world, owner, parent, 0, "Vocabulary");
        if tab == 1 {
            chooser(world, owner, parent, 1, "Concept");
        }
        let name = panel::field(world, parent, "Name (for create or rename)", "");
        world.get_mut::<OntologySand>(owner).unwrap().name = name;
        if tab == 0 {
            chooser(world, owner, parent, 6, "Organ (create or adopt into)");
            label(
                world,
                parent,
                "New Vocabularies are private. Publish a saved collection to share it.",
                12.0,
            );
        }
        let controls = panel::row(world, parent);
        panel::button(world, controls, owner, "Create", Command::Create);
        panel::button(world, controls, owner, "Rename selected", Command::Rename);
        panel::button(world, controls, owner, "Delete selected", Command::Delete);
        if tab == 0 {
            let controls = panel::row(world, parent);
            for (title, command) in [
                ("Publish selected", Command::Publish),
                ("Inspect publication", Command::InspectPublication),
                ("Adopt into Organ", Command::Adopt),
                ("Withdraw publication", Command::Withdraw),
            ] {
                panel::button(world, controls, owner, title, command);
            }
            let package = panel::field(
                world,
                parent,
                "Published package (copy to share or paste to inspect)",
                "",
            );
            world.get_mut::<OntologySand>(owner).unwrap().package = package;
            let controls = panel::row(world, parent);
            panel::button(
                world,
                controls,
                owner,
                "Inspect pasted package",
                Command::PreviewPackage,
            );
            panel::button(
                world,
                controls,
                owner,
                "Adopt pasted package",
                Command::AdoptPackage,
            );
            for (title, command) in [
                (Tr::ExportTranslations.text(), Command::ExportTranslations),
                (Tr::InspectTranslations.text(), Command::PreviewTranslations),
                (Tr::ImportTranslations.text(), Command::ImportTranslations),
            ] {
                panel::button(world, controls, owner, title, command);
            }
        }
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
    } else if tab == 4 {
        chooser(world, owner, parent, 3, "Record");
        panel::button(
            world,
            parent,
            owner,
            "Inspect content history",
            Command::History(0),
        );
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
                .filter(|row| slot != 6 || row["kind"] == "organ")
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
            .map(|(uid, caption)| (caption, crate::native::actions![Command::Choose(slot, uid)]))
            .collect();
        world.get_mut::<OntologySand>(owner).unwrap().choice_pages[slot] = page;
        panel::clear(world, holder);
        let group = crate::native::dropdown::spawn(
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
            crate::native::scroll_sand::attach(world, menu);
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
    let source = if tab == 4 || tab == 5 {
        2
    } else if tab == 2 {
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
    if tab == 5 {
        let view = world.get::<OntologySand>(owner).unwrap();
        let sources = view.knowledge_sources.clone();
        let selected = view.knowledge_selected.clone();
        let results = view.knowledge_results.clone();
        let output = view.output.clone().unwrap_or_default();
        let page = view.knowledge_page;
        let item = view.knowledge_item.clone().unwrap_or_default();
        let claim = view.knowledge_claim.clone();
        let availability = view.knowledge_availability.clone();
        let preview = view.knowledge_preview.clone();
        let review = view.knowledge_review.clone();
        let fields = view.knowledge_fields.clone();
        let cached = view.knowledge_cached.clone();
        let cache_bytes = view.knowledge_cache_bytes;
        let query = panel::value(world, view.name)
            .unwrap_or_default()
            .to_lowercase();
        let local = view.rows[2]
            .iter()
            .filter(|record| {
                record["head"]
                    .as_str()
                    .is_some_and(|title| title.to_lowercase().contains(&query))
            })
            .take(PAGE)
            .cloned()
            .collect::<Vec<_>>();
        label(world, list, "Local Records", 14.0);
        for record in local {
            label(
                world,
                list,
                &format!(
                    "{} · {}",
                    record["head"].as_str().unwrap_or(""),
                    record["uid"].as_str().unwrap_or("")
                ),
                13.0,
            );
        }
        label(world, list, "Choose knowledge sources", 14.0);
        label(
            world,
            list,
            &format!(
                "Temporary storage: {cache_bytes} of {} bytes",
                nucleus::knowledge::CACHE_LIMIT
            ),
            12.0,
        );
        for cached in cached.iter().take(PAGE) {
            if let (Some(source), Some(id)) = (cached["source"].as_str(), cached["id"].as_str()) {
                panel::button(
                    world,
                    list,
                    owner,
                    &format!(
                        "Read cached: {} · {}",
                        cached["title"].as_str().unwrap_or(id),
                        cached["revision"]
                    ),
                    Command::KnowledgeCached(source.into(), id.into()),
                );
            }
        }
        label(
            world,
            list,
            &format!(
                "Selected source changes: {}",
                fields
                    .iter()
                    .map(|field| format!("{field:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            12.0,
        );
        if let Some(review) = review {
            label(
                world,
                list,
                &format!(
                    "Source update {} → {}",
                    review["review"]["saved_revision"], review["review"]["source_revision"]
                ),
                14.0,
            );
            label(
                world,
                list,
                &format!(
                    "Source changes: {}\nLocal changes: {}",
                    review["review"]["source_changed"], review["review"]["local_changed"]
                ),
                12.0,
            );
            for (caption, item) in [
                ("Saved content", &review["review"]["saved_item"]),
                ("Incoming source content", &review["review"]["source_item"]),
            ] {
                label(world, list, caption, 13.0);
                label(
                    world,
                    list,
                    &format!(
                        "{}\n{}",
                        item["title"].as_str().unwrap_or(""),
                        item["content"]
                            .as_str()
                            .unwrap_or("")
                            .chars()
                            .take(8000)
                            .collect::<String>()
                    ),
                    12.0,
                );
            }
        }
        for source in sources {
            if let Some(id) = source["id"].as_str() {
                panel::button(
                    world,
                    list,
                    owner,
                    &format!(
                        "{} · {}{}",
                        source["name"].as_str().unwrap_or(id),
                        if source["online"] == true {
                            "online source"
                        } else {
                            "offline source"
                        },
                        if selected.iter().any(|known| known == id) {
                            " · selected"
                        } else {
                            ""
                        }
                    ),
                    Command::KnowledgeSource(id.into()),
                );
                if id.starts_with("ks_") {
                    panel::button(
                        world,
                        list,
                        owner,
                        "Remove offline source (saved Records stay available)",
                        Command::KnowledgeRemove(id.into()),
                    );
                }
            }
        }
        for result in results {
            label(
                world,
                list,
                &format!(
                    "{} · {}",
                    result["source"].as_str().unwrap_or(""),
                    if result["complete"] == true {
                        "results complete"
                    } else {
                        "more results available"
                    }
                ),
                14.0,
            );
            for reference in result["items"].as_array().into_iter().flatten().take(PAGE) {
                if let (Some(source), Some(id)) =
                    (reference["source"].as_str(), reference["id"].as_str())
                {
                    panel::button(
                        world,
                        list,
                        owner,
                        reference["title"].as_str().unwrap_or(id),
                        Command::KnowledgeRead(source.into(), id.into()),
                    );
                }
            }
            if let Some(offset) = result["next_offset"].as_u64() {
                panel::button(
                    world,
                    list,
                    owner,
                    "More source results",
                    Command::KnowledgeSearch(offset as u32),
                );
            }
        }
        if let Some(review) = preview {
            let preview = &review["preview"];
            label(
                world,
                list,
                &format!(
                    "Save preview: {} bytes · {} required missing pieces · {}",
                    preview["estimated_bytes"],
                    preview["missing"].as_array().map_or(0, Vec::len),
                    if preview["can_save"] == true {
                        "ready to save"
                    } else {
                        "read the missing content first"
                    }
                ),
                14.0,
            );
            for entry in preview["entries"]
                .as_array()
                .into_iter()
                .flatten()
                .take(128)
            {
                label(
                    world,
                    list,
                    &format!(
                        "{} · source {} · revision {} · license {}",
                        entry["item"]["title"].as_str().unwrap_or(""),
                        entry["item"]["source"].as_str().unwrap_or(""),
                        entry["item"]["revision"].as_str().unwrap_or(""),
                        entry["item"]["license"]["name"]
                            .as_str()
                            .unwrap_or("not supplied")
                    ),
                    12.0,
                );
                for asset in entry["item"]["assets"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(PAGE)
                {
                    label(
                        world,
                        list,
                        &format!(
                            "Asset {} · license {}",
                            asset["id"].as_str().unwrap_or(""),
                            asset["license"]["name"].as_str().unwrap_or("not supplied")
                        ),
                        12.0,
                    );
                }
            }
            for missing in preview["missing"]
                .as_array()
                .into_iter()
                .flatten()
                .take(128)
            {
                if missing["kind"] == "content"
                    && let (Some(source), Some(id)) =
                        (missing["source"].as_str(), missing["id"].as_str())
                {
                    panel::button(
                        world,
                        list,
                        owner,
                        &format!("Read required item {id}"),
                        Command::KnowledgeRead(source.into(), id.into()),
                    );
                }
            }
        }
        if let Some(title) = item["title"].as_str() {
            label(world, list, title, 16.0);
            label(
                world,
                list,
                &format!(
                    "Source: {} · revision {} · {}",
                    item["source"].as_str().unwrap_or(""),
                    item["revision"].as_str().unwrap_or(""),
                    availability
                ),
                12.0,
            );
            label(world, list, item["uri"].as_str().unwrap_or(""), 12.0);
            for dependency in item["dependencies"]
                .as_array()
                .into_iter()
                .flatten()
                .take(128)
            {
                if let (Some(source), Some(id)) =
                    (dependency["source"].as_str(), dependency["id"].as_str())
                {
                    panel::button(
                        world,
                        list,
                        owner,
                        &format!(
                            "Read related: {}",
                            dependency["title"].as_str().unwrap_or(id)
                        ),
                        Command::KnowledgeRead(source.into(), id.into()),
                    );
                }
            }
            for credit in item["credits"].as_array().into_iter().flatten().take(PAGE) {
                label(
                    world,
                    list,
                    &format!(
                        "Attribution: {} · {}",
                        credit["name"].as_str().unwrap_or(""),
                        credit["uri"].as_str().unwrap_or("")
                    ),
                    12.0,
                );
            }
            for meaning in item["meanings"].as_array().into_iter().flatten().take(PAGE) {
                label(
                    world,
                    list,
                    &format!(
                        "Meaning {} · revision {} · {}",
                        meaning["id"].as_str().unwrap_or(""),
                        meaning["revision"].as_str().unwrap_or(""),
                        meaning["title"].as_str().unwrap_or("")
                    ),
                    13.0,
                );
                label(
                    world,
                    list,
                    meaning["definition"].as_str().unwrap_or(""),
                    12.0,
                );
            }
            label(
                world,
                list,
                &format!(
                    "License: {}",
                    item["license"]["name"].as_str().unwrap_or("not supplied")
                ),
                12.0,
            );
            let statement = claim
                .as_ref()
                .map(|claim| serde_json::to_string_pretty(claim).unwrap_or_default());
            let content = statement
                .as_deref()
                .unwrap_or_else(|| item["content"].as_str().unwrap_or(""));
            if statement.is_some() {
                panel::button(
                    world,
                    list,
                    owner,
                    "Return to source content",
                    Command::KnowledgeClaim(None),
                );
            }
            let (text, more) = lince_interface::text::reading_page(content, page, 16000);
            for paragraph in text.split_inclusive(['\n', '\u{2028}', '\u{2029}']) {
                let paragraph = paragraph.trim_end_matches(['\r', '\n', '\u{2028}', '\u{2029}']);
                label(
                    world,
                    list,
                    if paragraph.is_empty() { " " } else { paragraph },
                    13.0,
                );
            }
            let navigation = panel::row(world, list);
            if page > 0 {
                panel::button(
                    world,
                    navigation,
                    owner,
                    "Earlier content",
                    Command::KnowledgePage(false),
                );
            }
            if more
                || item["claims"]
                    .as_array()
                    .is_some_and(|claims| claims.len() > (page + 1) * PAGE)
            {
                panel::button(
                    world,
                    navigation,
                    owner,
                    "More content",
                    Command::KnowledgePage(true),
                );
            }
            for claim in item["claims"]
                .as_array()
                .into_iter()
                .flatten()
                .skip(page * PAGE)
                .take(PAGE)
            {
                label(
                    world,
                    list,
                    &format!(
                        "{} · {} · conditions {} · references {}",
                        claim["property"].as_str().unwrap_or(""),
                        claim["value"]
                            .to_string()
                            .chars()
                            .take(4000)
                            .collect::<String>(),
                        claim["conditions"]
                            .to_string()
                            .chars()
                            .take(4000)
                            .collect::<String>(),
                        claim["references"]
                            .to_string()
                            .chars()
                            .take(4000)
                            .collect::<String>()
                    ),
                    12.0,
                );
            }
            for (index, claim) in item["claims"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .skip(page * PAGE)
                .take(PAGE)
            {
                panel::button(
                    world,
                    list,
                    owner,
                    &format!(
                        "Read complete statement {}",
                        claim["id"].as_str().unwrap_or("")
                    ),
                    Command::KnowledgeClaim(Some(index)),
                );
            }
        }
        if let Some(message) = output["message"].as_str() {
            label(world, list, message, 13.0);
        }
        world.get_mut::<OntologySand>(owner).unwrap().dirty = false;
        return;
    }
    if tab == 4 {
        let history = world
            .get::<OntologySand>(owner)
            .unwrap()
            .output
            .clone()
            .unwrap_or_default();
        let history = &history["history"];
        if let Some(versions) = history["versions"].as_array() {
            label(
                world,
                list,
                &format!(
                    "{} saved content versions",
                    history["count"].as_u64().unwrap_or(0)
                ),
                13.0,
            );
            for version in versions {
                label(
                    world,
                    list,
                    &format!(
                        "Version {} · {}",
                        version["revision"],
                        version["saved_at"].as_str().unwrap_or("")
                    ),
                    14.0,
                );
                label(
                    world,
                    list,
                    version["content"]["head"].as_str().unwrap_or(""),
                    14.0,
                );
                label(
                    world,
                    list,
                    version["content"]["body"].as_str().unwrap_or(""),
                    13.0,
                );
                label(
                    world,
                    list,
                    &format!(
                        "Quantity: {} × 10^-{}",
                        version["content"]["quantity_mantissa"]
                            .as_str()
                            .unwrap_or("0"),
                        version["content"]["quantity_scale"]
                    ),
                    12.0,
                );
            }
            let offset = history["offset"].as_u64().unwrap_or(0) as u32;
            if offset > 0 {
                panel::button(
                    world,
                    list,
                    owner,
                    "Newer versions",
                    Command::History(offset.saturating_sub(30)),
                );
            }
            if u64::from(offset) + 30 < history["count"].as_u64().unwrap_or(0) {
                panel::button(
                    world,
                    list,
                    owner,
                    "Older versions",
                    Command::History(offset.saturating_add(30)),
                );
            }
        } else {
            label(
                world,
                list,
                "Choose a Record and inspect its saved content versions.",
                13.0,
            );
        }
        world.get_mut::<OntologySand>(owner).unwrap().dirty = false;
        return;
    }
    if tab == 0 {
        let output = world
            .get::<OntologySand>(owner)
            .unwrap()
            .output
            .clone()
            .unwrap_or_default();
        let package = &output["publication"];
        if let Some(entries) = output["translations_preview"]["entries"].as_array() {
            label(
                world,
                list,
                &format!(
                    "Translation package: {} meanings · {} missing source meanings",
                    entries.len(),
                    output["translations_preview"]["missing_meanings"]
                        .as_array()
                        .map_or(0, Vec::len)
                ),
                13.0,
            );
            for entry in entries.iter().take(PAGE) {
                label(
                    world,
                    list,
                    &format!(
                        "{} · {} added terms",
                        entry["concept_uid"].as_str().unwrap_or(""),
                        entry["new_terms"]
                    ),
                    12.0,
                );
                label(
                    world,
                    list,
                    entry["definition"].as_str().unwrap_or(""),
                    13.0,
                );
                for term in entry["terms"].as_array().into_iter().flatten().take(PAGE) {
                    label(
                        world,
                        list,
                        &format!(
                            "{} · {}",
                            term["name"].as_str().unwrap_or(""),
                            term["language"].as_str().unwrap_or("")
                        ),
                        12.0,
                    );
                }
            }
            label(
                world,
                list,
                "Importing adds shared terms. Saved definitions and feature meanings keep their selected identities.",
                12.0,
            );
        }
        if let Some(dictionaries) = package["dictionaries"].as_array() {
            label(
                world,
                list,
                &format!(
                    "Publication: {} · revision {} · {} Concepts",
                    package["name"].as_str().unwrap_or(""),
                    package["revision"],
                    dictionaries.len()
                ),
                14.0,
            );
            label(
                world,
                list,
                &format!(
                    "Source Organ: {}",
                    package["owner_organ"].as_str().unwrap_or("")
                ),
                12.0,
            );
            let page = world
                .get::<OntologySand>(owner)
                .unwrap()
                .package_page
                .min(dictionaries.len().saturating_sub(1) / PAGE);
            world.get_mut::<OntologySand>(owner).unwrap().package_page = page;
            for dictionary in dictionaries.iter().skip(page * PAGE).take(PAGE) {
                label(
                    world,
                    list,
                    dictionary["source"]["original_name"].as_str().unwrap_or(""),
                    14.0,
                );
                if let Some(version) = dictionary["definitions"]
                    .as_array()
                    .and_then(|versions| versions.last())
                {
                    label(
                        world,
                        list,
                        version["definition"].as_str().unwrap_or(""),
                        13.0,
                    );
                }
            }
            if page > 0 {
                panel::button(
                    world,
                    list,
                    owner,
                    "Previous published meanings",
                    Command::PackagePage(false),
                );
            }
            if (page + 1) * PAGE < dictionaries.len() {
                panel::button(
                    world,
                    list,
                    owner,
                    "Next published meanings",
                    Command::PackagePage(true),
                );
            }
        }
    }
    if tab == 3 {
        let view = world.get::<OntologySand>(owner).unwrap();
        let feature = view.feature.clone();
        let meaning = view
            .output
            .as_ref()
            .and_then(|output| output.get("meaning"))
            .cloned();
        let preview = view
            .output
            .as_ref()
            .and_then(|output| output.get("feature_preview"))
            .cloned();
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
            if let Some(meaning) = meaning.filter(|meaning| meaning["concept_uid"] == row["uid"]) {
                label(
                    world,
                    list,
                    &format!(
                        "Origin Organ: {} · {} visible uses{}",
                        meaning["origin_organ"]
                            .as_str()
                            .unwrap_or("Lince or this Organ"),
                        meaning["visible_uses"],
                        if meaning["limited"] == true {
                            " · more uses require a narrower search"
                        } else {
                            ""
                        }
                    ),
                    13.0,
                );
                for usage in meaning["uses"].as_array().into_iter().flatten() {
                    label(
                        world,
                        list,
                        &format!(
                            "{} · {}",
                            usage["head"].as_str().unwrap_or(""),
                            usage["uid"].as_str().unwrap_or("")
                        ),
                        13.0,
                    );
                }
                let offset = meaning["offset"].as_u64().unwrap_or(0) as u32;
                if offset > 0 {
                    panel::button(
                        world,
                        list,
                        owner,
                        "Previous uses",
                        Command::InspectMeaning(offset.saturating_sub(30)),
                    );
                }
                if u64::from(offset) + 30 < meaning["visible_uses"].as_u64().unwrap_or(0) {
                    panel::button(
                        world,
                        list,
                        owner,
                        "More uses",
                        Command::InspectMeaning(offset.saturating_add(30)),
                    );
                }
            }
            if let Some(binding) = row["feature_bindings"].as_array().and_then(|bindings| {
                bindings
                    .iter()
                    .find(|binding| binding["feature"] == feature)
            }) {
                label(
                    world,
                    list,
                    &format!(
                        "{} · revision {} · {} accepted meanings · new controls write {}",
                        feature,
                        binding["revision"],
                        binding["concept_uids"].as_array().map_or(0, Vec::len),
                        binding["write_concept_uid"].as_str().unwrap_or("")
                    ),
                    13.0,
                );
            }
            if let Some(preview) = preview {
                label(
                    world,
                    list,
                    &format!(
                        "Reviewed change: {} existing Assertions · {} incompatible · {}",
                        preview["active_assertions"],
                        preview["incompatible_assertions"],
                        if preview["can_apply"] == true {
                            "can apply"
                        } else {
                            "choose a compatible meaning"
                        }
                    ),
                    13.0,
                );
                label(
                    world,
                    list,
                    &format!("Accepted Concepts: {}", preview["after"]["concept_uids"]),
                    12.0,
                );
            }
            label(
                world,
                list,
                &format!(
                    "Context: {} · displayed as {}",
                    row["terminology_context"].as_str().unwrap_or("General"),
                    row["name"].as_str().unwrap_or("")
                ),
                13.0,
            );
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
            if let Some(versions) = row["definitions"].as_array() {
                let page = world
                    .get::<OntologySand>(owner)
                    .unwrap()
                    .definition_page
                    .min(versions.len().saturating_sub(1) / PAGE);
                world
                    .get_mut::<OntologySand>(owner)
                    .unwrap()
                    .definition_page = page;
                for version in versions.iter().rev().skip(page * PAGE).take(PAGE) {
                    label(
                        world,
                        list,
                        &format!(
                            "Definition version {} · {}",
                            version["revision"],
                            version["created_at"].as_str().unwrap_or("")
                        ),
                        12.0,
                    );
                    label(
                        world,
                        list,
                        version["definition"].as_str().unwrap_or(""),
                        13.0,
                    );
                }
                if page > 0 {
                    panel::button(
                        world,
                        list,
                        owner,
                        "Newer definitions",
                        Command::DefinitionPage(false),
                    );
                }
                if (page + 1) * PAGE < versions.len() {
                    panel::button(
                        world,
                        list,
                        owner,
                        "Older definitions",
                        Command::DefinitionPage(true),
                    );
                }
            }
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
                            "#{} · {}{}{}{}{}",
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
                            if row["hidden_terms"]
                                .as_array()
                                .is_some_and(|hidden| hidden.contains(&term["uid"]))
                            {
                                " · hidden in this context"
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
                    "{} · {} concepts · {} Organs{}",
                    row["visibility"].as_str().unwrap_or("private"),
                    row["concepts"].as_array().map_or(0, Vec::len),
                    row["organs"].as_array().map_or(0, Vec::len),
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
            if tab == 0 {
                if let Some(organs) = row["organs"].as_array() {
                    let names = organs
                        .iter()
                        .filter_map(|organ| organ["uid"].as_str())
                        .map(|uid| reference(&catalogs[2], uid))
                        .collect::<Vec<_>>()
                        .join(", ");
                    label(world, entry, &format!("Organs: {names}"), 12.0);
                }
                if let Some(publications) = row["publications"].as_array() {
                    for publication in publications.iter().rev().take(PAGE) {
                        if let Some(uid) = publication["uid"].as_str() {
                            panel::button(
                                world,
                                entry,
                                owner,
                                &format!(
                                    "Inspect publication {}{}",
                                    publication["revision"],
                                    if publication["withdrawn"] == true {
                                        " · withdrawn"
                                    } else {
                                        ""
                                    }
                                ),
                                Command::InspectPublicationAt(uid.into()),
                            );
                        }
                    }
                }
            }
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
                &format!(
                    "{} · definition version {}",
                    row["role"].as_str().unwrap_or("assertion"),
                    row["predicate_revision"]
                ),
                12.0,
            );
            panel::button(world, entry, owner, "Retract", Command::Retract(uid.into()));
        }
    }
    let mut view = world.get_mut::<OntologySand>(owner).unwrap();
    view.page = page;
    view.dirty = false;
}
