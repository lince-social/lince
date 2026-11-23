// non-lean-yet
// Scope: fn project, fn visit, fn
//   curriculum_entrances_form_a_connected_skeleton_without_markdown_inventories, fn
//   an_outline_keeps_its_identity_and_context_after_export_and_relocation.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 62, 68, 109, 111, 114, 115, 116, 118, 122, 124, 168, 178.

#![cfg(feature = "publication")]

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use lince_resources::{Library, publication::Publication};

fn project(path: &Path) -> lingua::ProjectedDocument {
    let source = fs::read_to_string(path).expect("read the trail fixture");
    let (source, _) = lingua::ensure_uids(&source).expect("ensure the trail fixture has UIDs");
    let document = lingua::parse(&source).expect("parse the trail fixture");
    lingua::project(&document).expect("project the trail fixture")
}

fn visit(
    uid: &str,
    edges: &BTreeMap<String, BTreeSet<String>>,
    active: &mut BTreeSet<String>,
    done: &mut BTreeSet<String>,
) {
    assert!(!active.contains(uid), "branch cycle at {uid}");
    if done.contains(uid) {
        return;
    }
    active.insert(uid.to_owned());
    for child in edges.get(uid).into_iter().flatten() {
        visit(child, edges, active, done);
    }
    active.remove(uid);
    done.insert(uid.to_owned());
}

#[test]
fn curriculum_entrances_form_a_connected_skeleton_without_markdown_inventories() {
    let institute = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../institute")
        .canonicalize()
        .unwrap();
    let trails = institute.join("trails");
    assert!(!trails.join("ergon/connected-systems").exists());
    let root = "r_7A4AY51TXG87NVHR9J1XNZTJQ9";
    let entry = trails.join("Lince.lingua");
    let guidance = project(&entry);
    let main_record = |slug: &str| {
        guidance
            .records
            .iter()
            .find(|record| record.slug.as_deref() == Some(slug))
            .unwrap()
    };
    for (child, parent) in [
        ("tool", "philosophy"),
        ("institute", "philosophy"),
        ("instinct-handbook", "tool"),
        ("trails", "trail"),
    ] {
        assert!(main_record(child).assertions.iter().any(|assertion| {
            assertion.predicate == "part-of" && assertion.object_slug.as_deref() == Some(parent)
        }));
    }
    assert!(main_record("institute").assertions.iter().any(|assertion| {
        assertion.predicate == "contains" && assertion.object_slug.as_deref() == Some("trail")
    }));
    assert!(main_record("trails").body.contains("breadth first"));
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let navigation = Publication::load(&library, entry.to_str().unwrap(), &[], false).unwrap();
    assert_eq!(navigation.documents.len(), 1);
    let mut pending = vec![(entry, root.to_owned())];
    let mut records = BTreeMap::new();
    let mut edges = BTreeMap::new();
    while let Some((path, uid)) = pending.pop() {
        if records.contains_key(&uid) {
            continue;
        }
        let path = path.canonicalize().unwrap();
        assert!(path.starts_with(&trails));
        let record = project(&path)
            .records
            .into_iter()
            .find(|record| record.uid == uid)
            .unwrap();
        assert!(!record.slug.as_deref().unwrap().starts_with("plan-"));
        assert!(!record.assertions.iter().any(|a| a.predicate == "planned"));
        assert!(!record.body.trim().is_empty());
        let mut children = BTreeSet::new();
        for assertion in &record.assertions {
            if assertion.predicate != "contains" {
                continue;
            }
            if let Some(target) = &assertion.object_uri {
                let (relative, child) = target.rsplit_once('#').unwrap();
                children.insert(child.to_owned());
                pending.push((path.parent().unwrap().join(relative), child.to_owned()));
            }
        }
        edges.insert(uid.clone(), children);
        records.insert(uid, record);
    }
    let mut done = BTreeSet::new();
    visit(root, &edges, &mut BTreeSet::new(), &mut done);
    assert_eq!(done.len(), records.len());
    assert_eq!(
        edges[root],
        [
            "r_FE78A5A2D2A3261F711703DCC1",
            "r_E8E2D1B2D2994E0AD5F5D002D6",
            "r_C13F57992B4DED1C03FB1AF28D"
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    );
    assert_eq!(edges["r_00000000000000000000002001"].len(), 6);
    assert!(edges["r_0BDE857C410434BA79D017008C"].contains("r_3TKT81FRMZKC1D63FV1RHY8DCJ"));
    assert_eq!(records["r_3TKT81FRMZKC1D63FV1RHY8DCJ"].head, "Algebra");
    assert!(
        records["r_0BDE857C410434BA79D017008C"]
            .body
            .contains("Construct and explain a mathematical proof")
    );
    assert!(edges["r_2Y7YR9YHGPS7CWXQ7R4SAJVBR4"].contains("r_7D8SQ8BVQDZQBDBGC6DP0JAMZN"));
    assert!(
        records["r_7D8SQ8BVQDZQBDBGC6DP0JAMZN"]
            .body
            .contains("TODO — sensitive data")
    );
    let mut directories = vec![trails];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                directories.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "lingua")
            {
                let source = fs::read_to_string(&path).unwrap();
                assert!(
                    !source.contains("Trails.md") && !source.contains("Ergon.md"),
                    "{}",
                    path.display()
                );
                if source.contains("#outline") {
                    for record in project(&path).records {
                        if record.assertions.iter().any(|a| a.predicate == "outline") {
                            assert!(
                                done.contains(&record.uid),
                                "orphan outline {}",
                                path.display()
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn an_outline_keeps_its_identity_and_context_after_export_and_relocation() {
    let entry = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../institute/trails/ergon/hephaestus/mathematics/algebra/Algebra.lingua");
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let publication = Publication::load(&library, entry.to_str().unwrap(), &[], false).unwrap();
    assert_eq!(publication.documents.len(), 1);
    let original = &publication.documents[0].project.records[0];
    assert_eq!(original.uid, "r_3TKT81FRMZKC1D63FV1RHY8DCJ");
    let exported = temporary.path().join("export");
    let output = publication.export(&library, &exported, false).unwrap();
    let relative = output.strip_prefix(&exported).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(&exported, &moved).unwrap();
    let fresh = Library::new(temporary.path().join("fresh"));
    let reopened =
        Publication::load(&fresh, moved.join(relative).to_str().unwrap(), &[], false).unwrap();
    let record = &reopened.documents[0].project.records[0];
    assert_eq!(record.uid, original.uid);
    assert_eq!(record.head, original.head);
    assert!(record.assertions.iter().any(|a| a.predicate == "outline"));
    assert!(record.body.contains("groups, rings, fields, modules"));
    assert!(record.body.contains("r_0BDE857C410434BA79D017008C"));
    assert!(reopened.resources().unwrap().is_empty());
}
