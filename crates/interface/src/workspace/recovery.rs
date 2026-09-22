use super::*;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use std::{collections::BTreeMap, path::Path};

#[derive(Default)]
pub(super) struct Report {
    skipped: BTreeMap<&'static str, (usize, String)>,
    notes: Vec<String>,
    original: Option<Vec<u8>>,
}

impl Report {
    pub(super) fn changed(&self) -> bool {
        !self.skipped.is_empty() || !self.notes.is_empty()
    }

    fn skip(&mut self, section: &'static str, reason: impl ToString) {
        let entry = self
            .skipped
            .entry(section)
            .or_insert_with(|| (0, reason.to_string().chars().take(240).collect()));
        entry.0 += 1;
    }

    pub(super) fn skipped_snapshots(&mut self, count: usize) {
        self.notes.push(format!(
            "Skipped {count} unreadable newer snapshots; those files have been kept"
        ));
    }

    pub(super) fn summary(&self) -> String {
        let mut details: Vec<_> = self
            .skipped
            .iter()
            .map(|(section, (count, reason))| format!("{section}: {count} skipped ({reason})"))
            .collect();
        details.extend(self.notes.iter().cloned());
        format!(
            "Workspace restored with compatible items. {}",
            details.join(". ")
        )
    }

    pub(super) fn message(&self, backup: Option<&Path>) -> String {
        let mut message = format!("{}. Saving is enabled.", self.summary());
        if let Some(backup) = backup {
            message.push_str(&format!(
                " Original snapshot backed up at {}.",
                backup.display()
            ));
        }
        message
    }

    pub(super) fn preserve(&self, path: &Path) -> io::Result<Option<PathBuf>> {
        let Some(bytes) = &self.original else {
            return Ok(None);
        };
        let directory = path.with_extension("recovery");
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&directory) {
            Ok(()) => sync_parent(&directory)?,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if !std::fs::symlink_metadata(&directory)?.is_dir() {
                    return Err(io::Error::other("recovery path is not a directory"));
                }
            }
            Err(error) => return Err(error),
        }
        let mut backup = tempfile::Builder::new()
            .prefix("original-")
            .suffix(".json")
            .tempfile_in(&directory)?;
        backup.write_all(bytes)?;
        backup.as_file().sync_all()?;
        let (_, backup) = backup.keep().map_err(|error| error.error)?;
        sync_directory(&directory)?;
        Ok(Some(backup))
    }
}

fn entries<T: DeserializeOwned>(
    values: &mut Map<String, Value>,
    key: &str,
    section: &'static str,
    report: &mut Report,
    mut valid: impl FnMut(&T) -> bool,
) -> Vec<T> {
    let Some(value) = values.remove(key) else {
        return Vec::new();
    };
    let Value::Array(items) = value else {
        report.skip(section, "saved list could not be read");
        return Vec::new();
    };
    items
        .into_iter()
        .enumerate()
        .filter_map(|(index, item)| match serde_json::from_value::<T>(item) {
            Ok(item) if valid(&item) => Some(item),
            Ok(_) => {
                report.skip(
                    section,
                    format!("item {} has invalid settings or a missing owner", index + 1),
                );
                None
            }
            Err(error) => {
                report.skip(section, format!("item {}: {error}", index + 1));
                None
            }
        })
        .collect()
}

pub(super) fn read(path: &Path) -> io::Result<Option<Document>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut document = decode(&bytes)?;
    if document.recovery.changed() {
        document.recovery.original = Some(bytes);
    }
    Ok(Some(document))
}

fn decode(bytes: &[u8]) -> io::Result<Document> {
    let Value::Object(mut values) = serde_json::from_slice(bytes)? else {
        return Err(io::Error::other("workspace snapshot is not an object"));
    };
    let mut report = Report::default();
    let mut ids = HashSet::new();
    let workspaces = entries(
        &mut values,
        "workspaces",
        "Workspaces",
        &mut report,
        |space: &Workspace| space.valid() && ids.insert(space.id),
    );
    if workspaces.is_empty() {
        return Err(io::Error::other("no readable workspaces in snapshot"));
    }
    let active = match values.remove("active").and_then(|value| value.as_u64()) {
        Some(active) if ids.contains(&active) => active,
        _ => {
            report.notes.push("Opened the first remaining workspace because the active workspace could not be restored".into());
            workspaces[0].id
        }
    };
    let theme = match values.remove("theme") {
        None => Default::default(),
        Some(theme) => match serde_json::from_value::<crate::tokens::ThemeSettings>(theme) {
            Ok(theme) if theme.validate() => theme,
            _ => {
                report
                    .notes
                    .push("Theme settings could not be restored; using defaults".into());
                Default::default()
            }
        },
    };
    let mut area_ids = HashMap::new();
    let areas = entries(
        &mut values,
        "areas",
        "Areas",
        &mut report,
        |saved: &crate::area::SavedArea| {
            ids.contains(&saved.workspace)
                && saved.area.validate()
                && saved.placement.valid()
                && area_ids.len() < crate::area::MAX_AREAS
                && !area_ids.contains_key(&saved.area.id)
                && {
                    area_ids.insert(saved.area.id.clone(), saved.workspace);
                    true
                }
        },
    );
    let sands = entries(
        &mut values,
        "sands",
        "Sands",
        &mut report,
        |sand: &SavedSand| ids.contains(&sand.workspace) && sand.valid(),
    );
    let records = entries(
        &mut values,
        "records",
        "Record placements",
        &mut report,
        |record: &SavedRecord| ids.contains(&record.workspace) && record.valid(),
    );
    let imports = entries(
        &mut values,
        "imports",
        "Imported assets",
        &mut report,
        |saved: &crate::topology::assets::SavedAsset| {
            ids.contains(&saved.workspace)
                && saved.asset.valid()
                && saved.placement.valid()
                && valid_geometry(saved.position, saved.size)
        },
    );
    let layouts = entries(
        &mut values,
        "layouts",
        "Record layouts",
        &mut report,
        |saved: &crate::layout::records::Saved| {
            ids.contains(&saved.workspace)
                && saved.valid()
                && area_ids.get(saved.owner()) == Some(&saved.workspace)
        },
    );
    let kanbans = entries(
        &mut values,
        "kanbans",
        "Kanbans",
        &mut report,
        |saved: &crate::kanban::SavedKanban| {
            ids.contains(&saved.workspace)
                && saved.valid()
                && saved
                    .areas()
                    .all(|id| area_ids.get(id) == Some(&saved.workspace))
        },
    );
    macro_rules! castles {
        ($key:literal, $name:literal, $ty:ty) => {
            entries(&mut values, $key, $name, &mut report, |saved: &$ty| {
                ids.contains(&saved.workspace) && saved.valid()
            })
        };
    }
    let proteins = castles!(
        "proteins",
        "Protein Castles",
        crate::protein_castle::SavedProteinCastle
    );
    let karma_castles = castles!(
        "karma_castles",
        "Karma Castles",
        crate::karma_castle::SavedKarmaCastle
    );
    let recorders = castles!(
        "recorders",
        "Recorder Castles",
        crate::recorder_castle::SavedRecorder
    );
    let transfer_castles = castles!(
        "transfer_castles",
        "Transfer Castles",
        crate::transfer_castle::SavedTransferCastle
    );
    let frequency_castles = castles!(
        "frequency_castles",
        "Frequency Castles",
        crate::frequency_castle::SavedFrequencyCastle
    );
    let calendars = castles!("calendars", "Calendars", crate::calendar::SavedCalendar);
    let instincts = castles!("instincts", "Instincts", crate::instinct::SavedInstinct);
    let assertions = entries(
        &mut values,
        "assertions",
        "Assertion Castles",
        &mut report,
        |saved: &crate::assertion_castle::SavedAssertion| {
            ids.contains(&saved.frame.workspace) && saved.valid()
        },
    );
    let shaders = entries(
        &mut values,
        "shaders",
        "Shader Castles",
        &mut report,
        |saved: &crate::shader_castle::SavedShader| {
            ids.contains(&saved.0.workspace) && saved.valid()
        },
    );
    let document = Document {
        recovery: report,
        theme,
        active,
        workspaces,
        sands,
        records,
        areas,
        imports,
        layouts,
        kanbans,
        proteins,
        karma_castles,
        frequency_castles,
        transfer_castles,
        recorders,
        calendars,
        instincts,
        assertions,
        shaders,
    };
    if !document.validate() {
        return Err(io::Error::other("recovered workspace data is invalid"));
    }
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scene() -> Value {
        json!({"active": 1, "workspaces": Workspaces::default().entries, "sands": [], "records": []})
    }

    fn sand(text: &str) -> Value {
        json!({
            "kind": "Square", "workspace": 1, "position": [25.0, 50.0], "size": [248.0, 184.0],
            "texts": [{"area": crate::sand_text::SandText::new(false), "text": text}]
        })
    }

    fn load(value: &Value) -> Document {
        decode(&serde_json::to_vec(value).unwrap()).unwrap()
    }

    #[test]
    fn incompatible_items_are_skipped_without_losing_neighbors_or_positions() {
        let mut scene = scene();
        let kept = sand("Keep this note");
        let mut unknown = sand("Unknown kind");
        unknown["kind"] = json!("RemovedSandKind");
        let mut invalid = sand("Invalid geometry");
        invalid["size"] = json!([0, 100]);
        let mut orphan = sand("Missing workspace");
        orphan["workspace"] = json!(99);
        scene["sands"] = json!([kept, unknown, invalid, orphan, sand("Also kept")]);
        let area = crate::area::InfluenceArea::new(
            crate::area::AreaShape::Square,
            DVec2::ZERO,
            DVec2::splat(100.0),
        );
        let valid_area = json!({"workspace": 1, "area": area});
        let mut incompatible_area = valid_area.clone();
        incompatible_area["area"]
            .as_object_mut()
            .unwrap()
            .remove("enabled");
        scene["areas"] = json!([incompatible_area, valid_area.clone(), valid_area]);
        let recovered = load(&scene);
        assert!(recovered.validate());
        assert_eq!(recovered.sands.len(), 2);
        assert_eq!(recovered.sands[0].texts[0].text, "Keep this note");
        assert_eq!(recovered.sands[0].position, [25.0, 50.0]);
        assert_eq!(recovered.sands[1].texts[0].text, "Also kept");
        assert_eq!(recovered.areas.len(), 1);
        assert_eq!(recovered.recovery.skipped["Sands"].0, 3);
        assert_eq!(recovered.recovery.skipped["Areas"].0, 2);
        assert!(
            recovered
                .recovery
                .summary()
                .contains("missing field `enabled`")
        );
        assert!(
            !serde_json::to_value(&recovered)
                .unwrap()
                .as_object()
                .unwrap()
                .contains_key("recovery")
        );
        let restarted = load(&serde_json::to_value(recovered).unwrap());
        assert!(!restarted.recovery.changed());
        assert_eq!(restarted.sands.len(), 2);
    }

    #[test]
    fn invalid_workspaces_settings_and_lists_do_not_discard_other_sections() {
        let mut scene = scene();
        let mut second = scene["workspaces"][0].clone();
        second["id"] = json!(2);
        second["name"] = json!("");
        scene["workspaces"].as_array_mut().unwrap().push(second);
        scene["active"] = json!(2);
        scene["theme"] = json!({"scheme": "RemovedScheme"});
        scene["proteins"] = json!("not a list");
        scene["sands"] = json!([sand("Keep")]);
        let recovered = load(&scene);
        assert_eq!(recovered.active, 1);
        assert_eq!(recovered.workspaces.len(), 1);
        assert_eq!(recovered.sands.len(), 1);
        assert_eq!(recovered.theme, Default::default());
        assert!(recovered.recovery.summary().contains("Theme settings"));
        assert!(
            recovered
                .recovery
                .summary()
                .contains("Protein Castles: 1 skipped")
        );
        scene["workspaces"] = json!([]);
        assert!(decode(&serde_json::to_vec(&scene).unwrap()).is_err());
        assert!(decode(b"broken").is_err());
    }

    #[test]
    fn layouts_and_kanbans_with_missing_areas_are_skipped() {
        let mut scene = scene();
        scene["sands"] = json!([sand("Kept")]);
        scene["layouts"] = json!([{
            "workspace": 1, "owner": format!("{:032x}", 1), "source": "Local", "uid": "record",
            "layout": crate::layout::LayoutBox::new(Vec2::splat(100.0))
        }]);
        scene["kanbans"] = json!([{
            "workspace": 1,
            "board": {
                "source": format!("{:032x}", 0),
                "columns": (1..8).map(|index| json!({"area": format!("{index:032x}")})).collect::<Vec<_>>()
            },
            "position": [0, 0], "size": [400, 100],
            "placement": crate::sand_placement::Placement::default(), "tokens": {}
        }]);
        assert!(
            serde_json::from_value::<crate::layout::records::Saved>(scene["layouts"][0].clone())
                .unwrap()
                .valid()
        );
        assert!(
            serde_json::from_value::<crate::kanban::SavedKanban>(scene["kanbans"][0].clone())
                .unwrap()
                .valid()
        );
        let recovered = load(&scene);
        assert_eq!(recovered.sands.len(), 1);
        assert!(recovered.layouts.is_empty());
        assert!(recovered.kanbans.is_empty());
        assert_eq!(recovered.recovery.skipped["Record layouts"].0, 1);
        assert_eq!(recovered.recovery.skipped["Kanbans"].0, 1);
    }

    #[test]
    fn many_incompatible_items_produce_a_bounded_notice() {
        let mut scene = scene();
        scene["sands"] = Value::Array(
            (0..2_000)
                .map(|_| json!({"kind": "x".repeat(1000)}))
                .chain([sand("Kept")])
                .collect(),
        );
        let recovered = load(&scene);
        assert_eq!(recovered.sands.len(), 1);
        assert_eq!(recovered.recovery.skipped["Sands"].0, 2_000);
        assert!(recovered.recovery.summary().chars().count() < 400);
    }

    #[test]
    fn newest_partial_snapshot_wins_over_an_older_complete_snapshot_and_backups_are_kept() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let snapshots = path.with_extension("snapshots");
        std::fs::create_dir(&snapshots).unwrap();
        let mut older = scene();
        older["sands"] = json!([sand("Old note")]);
        std::fs::write(
            snapshots.join("00000000000000000001.json"),
            serde_json::to_vec(&older).unwrap(),
        )
        .unwrap();
        let mut newer = scene();
        newer["sands"] = json!([sand("Latest note"), {"kind": "Removed"}]);
        let bytes = serde_json::to_vec(&newer).unwrap();
        let newest = snapshots.join("00000000000000000002.json");
        std::fs::write(&newest, &bytes).unwrap();
        let recovered = storage::load(&path).unwrap().unwrap();
        assert_eq!(recovered.sands[0].texts[0].text, "Latest note");
        let backup = recovered.recovery.preserve(&path).unwrap().unwrap();
        assert_eq!(std::fs::read(&backup).unwrap(), bytes);
        assert_eq!(std::fs::read(&newest).unwrap(), bytes);
        let another = recovered.recovery.preserve(&path).unwrap().unwrap();
        assert_ne!(backup, another);
        let writer = storage::Writer::start(path.clone(), None, Default::default(), None).unwrap();
        writer.save(recovered, true).unwrap();
        drop(writer);
        let restarted = storage::load(&path).unwrap().unwrap();
        assert_eq!(restarted.sands[0].texts[0].text, "Latest note");
        assert!(!restarted.recovery.changed());
        assert_eq!(std::fs::read(backup).unwrap(), bytes);
    }

    #[test]
    fn unreadable_newer_snapshots_are_reported_when_an_older_one_is_restored() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let snapshots = path.with_extension("snapshots");
        std::fs::create_dir(&snapshots).unwrap();
        std::fs::write(
            snapshots.join("00000000000000000001.json"),
            serde_json::to_vec(&scene()).unwrap(),
        )
        .unwrap();
        let broken = snapshots.join("00000000000000000002.json");
        std::fs::write(&broken, b"broken").unwrap();
        let recovered = storage::load(&path).unwrap().unwrap();
        assert!(
            recovered
                .recovery
                .summary()
                .contains("1 unreadable newer snapshots")
        );
        assert_eq!(std::fs::read(broken).unwrap(), b"broken");
    }

    #[cfg(unix)]
    #[test]
    fn recovery_backups_are_private_and_do_not_follow_directory_symlinks() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let mut value = scene();
        value["sands"] = json!([sand("Kept"), {}]);
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let recovered = read(&path).unwrap().unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), path.with_extension("recovery")).unwrap();
        assert!(recovered.recovery.preserve(&path).is_err());
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
        std::fs::remove_file(path.with_extension("recovery")).unwrap();
        let backup = recovered.recovery.preserve(&path).unwrap().unwrap();
        assert_eq!(
            std::fs::metadata(backup).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(path.with_extension("recovery"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
}
