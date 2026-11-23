//! The B39 fixture uses two grades of Steel and two revisions of one Bracket identity.
use nucleus::{
    canvas::scene::{DefinitionKind, DocumentId, Part, Pin, RootId, Scene, Slot},
    karma::{DecimalValue, canonical_json_bytes},
};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use store::workspace_scene::{
    self as storage,
    library::{Catalog, Status},
};

fn sample() -> Scene {
    #[derive(Deserialize)]
    struct Sample {
        document: DocumentId,
        root: RootId,
        parts: Vec<Part>,
        slots: Vec<Slot>,
    }
    let data: Sample = serde_json::from_slice(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../interface/fixtures/showoff-parts.json"
    )))
    .unwrap();
    let scene = Scene {
        document: data.document,
        root: data.root,
        parts: data.parts.into(),
        slots: data.slots.into(),
        canvas: None,
    };
    scene.validate().unwrap();
    scene
}
fn body(dir: &Path, pin: &Pin) -> PathBuf {
    dir.join("pages")
        .join(format!("{}.json", &pin.revision.as_str()[7..]))
}
fn destination() -> Scene {
    let mut s = Scene::default();
    let a = s.add_part("Bracket").unwrap();
    let b = s.place_again(&a).unwrap();
    s.edit(
        &b,
        [5, 0, 2].map(DecimalValue::from_integer),
        Some(nucleus::new_uid("r")),
    )
    .unwrap();
    s
}

#[test]
fn bounded_import_preserves_versions_rights_and_existing_occurrences() {
    let temp = tempfile::tempdir().unwrap();
    let src = temp.path().join("source");
    let dest = temp.path().join("dest");
    let source = sample();
    storage::save(&src, &source, None).unwrap();
    let pins: Vec<_> = source
        .slots
        .iter()
        .map(|p| p.pin(&source.document))
        .collect();
    let spare = source
        .parts
        .iter()
        .find(|p| p.name == "Spacer")
        .unwrap()
        .pin(&source.document)
        .unwrap();
    fs::remove_file(body(&src, &spare)).unwrap();
    // The catalogue can import the intended roots even when unrelated data cannot be opened.
    assert!(storage::open(&src).is_err());
    let catalog = Catalog::open(&src).unwrap();
    assert_eq!(catalog.available().count(), 7);
    let mut scene = destination();
    let original = scene.clone();
    for (i, pin) in pins.iter().enumerate() {
        let r = catalog.preview(pin, &scene).unwrap();
        assert!(r.complete());
        assert_eq!(r.items.len(), 3);
        let root = r.items.last().unwrap();
        assert_eq!(&root.pin, pin);
        assert_eq!(root.name.as_deref(), Some("Bracket"));
        assert!(!root.conflicts.is_empty());
        assert_eq!(r.items[0].name.as_deref(), Some("Millimetre"));
        assert_eq!(
            r.items[0].status,
            if i == 0 {
                Status::BringIn
            } else {
                Status::Reuse
            }
        );
        assert_eq!(
            r.items[1].intrinsic,
            Some(DefinitionKind::Material {
                grade: if i == 0 { "A36" } else { "304" }.into()
            })
        );
        assert_eq!(catalog.import(pin, &mut scene, &r).unwrap(), 3 - i);
        assert!(Arc::ptr_eq(&original.slots, &scene.slots));
        let again = catalog.preview(pin, &scene).unwrap();
        assert!(again.items.iter().all(|r| r.status == Status::Reuse));
        let before = scene.clone();
        assert_eq!(catalog.import(pin, &mut scene, &again).unwrap(), 0);
        assert!(Arc::ptr_eq(&before.parts, &scene.parts));
    }
    assert_eq!(scene.parts.len(), 6);
    assert_eq!(scene.slots, original.slots);
    for pin in &pins {
        let handle = scene.place_definition(pin).unwrap();
        let copy = scene.place_again(&handle).unwrap();
        let slot = scene.slot(&copy).unwrap();
        assert_eq!(&slot.pin(&scene.document), pin);
        assert_eq!(slot.metres, [DecimalValue::from_integer(0); 3]);
        assert_eq!(slot.record, None);
    }
    storage::save(&dest, &scene, None).unwrap();
    let reopened = storage::open(&dest).unwrap().scene;
    assert_eq!(scene, reopened);
    assert_eq!(pins[0].part, pins[1].part);
    assert_ne!(pins[0].revision, pins[1].revision);
    let third = temp.path().join("third");
    let catalogue = Catalog::open(&dest).unwrap();
    let mut relay = Scene::default();
    let r = catalogue.preview(&pins[1], &relay).unwrap();
    assert_eq!(catalogue.import(&pins[1], &mut relay, &r).unwrap(), 3);
    storage::save(&third, &relay, None).unwrap();
    assert!(
        storage::open(&third)
            .unwrap()
            .scene
            .definition(&pins[1])
            .is_some()
    );
}

#[test]
fn revoked_private_and_missing_requirements_never_partially_adopt_or_disclose_a_private_body() {
    let temp = tempfile::tempdir().unwrap();
    let src = temp.path();
    let mut source = sample();
    let head = storage::save(src, &source, None).unwrap();
    let catalog = Catalog::open(src).unwrap();
    let mut scene = destination();
    let before = scene.clone();
    let first = source.slots[0].pin(&source.document);
    let grant = catalog.preview(&first, &scene).unwrap();
    let steel = source.definition(&first).unwrap().requires[0].clone();
    source.set_local_only(&steel, true).unwrap();
    storage::save(src, &source, Some(&head)).unwrap();
    assert!(
        catalog
            .import(&first, &mut scene, &grant)
            .unwrap_err()
            .to_string()
            .contains("permissions changed")
    );
    assert_eq!(scene, before);
    let catalog = Catalog::open(src).unwrap();
    let unreadable = body(src, &steel);
    fs::write(&unreadable, b"this must never be opened").unwrap();
    let withheld = catalog.preview(&first, &scene).unwrap();
    assert!(!withheld.complete());
    assert!(
        matches!(&withheld.items[0].status,Status::Unavailable(why) if why.contains("keeps this definition local"))
    );
    assert!(withheld.items[0].name.is_none());
    assert!(withheld.items[0].intrinsic.is_none());
    assert!(catalog.import(&first, &mut scene, &withheld).is_err());
    assert_eq!(scene, before);
    let second = source.slots[1].pin(&source.document);
    let material = source.definition(&second).unwrap().requires[0].clone();
    let material_file = body(src, &material);
    let mut unsupported: serde_json::Value =
        serde_json::from_slice(&fs::read(&material_file).unwrap()).unwrap();
    unsupported["intrinsic"]["kind"] = "unrecognised-model".into();
    fs::write(&material_file, serde_json::to_vec(&unsupported).unwrap()).unwrap();
    let unknown = catalog.preview(&second, &scene).unwrap();
    assert!(unknown.items.iter().any(|item|
        matches!(&item.status, Status::Unavailable(reason) if reason.contains("unknown variant"))));
    assert!(catalog.import(&second, &mut scene, &unknown).is_err());
    assert_eq!(scene, before);
    fs::remove_file(body(src, &material)).unwrap();
    let missing = catalog.preview(&second, &scene).unwrap();
    assert!(!missing.complete());
    assert!(catalog.import(&second, &mut scene, &missing).is_err());
    assert_eq!(scene, before);
}

#[test]
fn ordinary_legacy_documents_keep_distinct_lineage_and_local_revision_edits_only_rebind_the_selected_slot()
 {
    let legacy = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../institute/fixtures/workspace/stage02/two-screws.workspace");
    let established = storage::open(&legacy).unwrap();
    let library = Catalog::open(&legacy).unwrap();
    let pin = library.available().next().unwrap().pin.clone();
    let mut dest = destination();
    let original = dest.clone();
    let review = library.preview(&pin, &dest).unwrap();
    library.import(&pin, &mut dest, &review).unwrap();
    let mut unrelated = Scene::default();
    unrelated.parts = established.scene.parts.clone();
    let another = tempfile::tempdir().unwrap();
    storage::save(another.path(), &unrelated, None).unwrap();
    let catalog = Catalog::open(another.path()).unwrap();
    let alternate = catalog.available().next().unwrap().pin.clone();
    assert_eq!(
        (&pin.part, &pin.revision),
        (&alternate.part, &alternate.revision)
    );
    assert_ne!(pin.document, alternate.document);
    let r = catalog.preview(&alternate, &dest).unwrap();
    assert_eq!(r.items[0].status, Status::BringIn);
    assert_eq!(r.items[0].conflicts, [pin.clone()]);
    catalog.import(&alternate, &mut dest, &r).unwrap();
    let selected = dest.handle(&dest.slots[1]);
    dest.require_definition(&selected, &alternate).unwrap();
    assert_eq!(dest.slots[0], original.slots[0]);
    assert_eq!(dest.slots[1].record, original.slots[1].record);
    assert_eq!(dest.slots[1].metres, original.slots[1].metres);
    assert_ne!(dest.slots[1].revision, original.slots[1].revision);
    assert_eq!(
        dest.definition(&dest.slots[1].pin(&dest.document))
            .unwrap()
            .requires,
        [alternate]
    );
    dest.validate().unwrap();
    let dir = tempfile::tempdir().unwrap();
    storage::save(dir.path(), &dest, None).unwrap();
    assert_eq!(storage::open(dir.path()).unwrap().scene, dest);
}

#[test]
fn dependency_depth_and_destination_capacity_fail_atomically() {
    let src = tempfile::tempdir().unwrap();
    let mut chain = Scene::default();
    let mut top = None;
    for i in 0..9 {
        let mut p = Part::new(&format!("Layer {i}"), DefinitionKind::Part);
        p.requires = top.take().into_iter().collect();
        top = Some(p.pin(&chain.document).unwrap());
        Arc::make_mut(&mut chain.parts).push(p);
    }
    storage::save(src.path(), &chain, None).unwrap();
    let catalog = Catalog::open(src.path()).unwrap();
    let mut dest = destination();
    assert!(
        catalog
            .preview(&top.unwrap(), &dest)
            .unwrap_err()
            .to_string()
            .contains("depth")
    );
    while dest.parts.len() < nucleus::canvas::scene::MAX_PARTS {
        dest.add_part("Filled").unwrap();
    }
    let first = chain.parts[0].pin(&chain.document).unwrap();
    let before = dest.clone();
    assert!(catalog.preview(&first, &dest).is_err());
    assert_eq!(dest, before);
}

// This integration-test executable meters the peak live Rust allocation above each phase's
// entry value. Run the one ignored benchmark on its own thread after building the test.
mod heap {
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        sync::atomic::{AtomicUsize, Ordering::Relaxed},
    };
    pub struct Meter;
    static LIVE: AtomicUsize = AtomicUsize::new(0);
    static PEAK: AtomicUsize = AtomicUsize::new(0);
    fn add(bytes: usize) {
        let now = LIVE.fetch_add(bytes, Relaxed) + bytes;
        PEAK.fetch_max(now, Relaxed);
    }
    // SAFETY: all allocations, sizes, alignments and frees are forwarded to System unchanged.
    // The bookkeeping uses only atomics and never allocates or re-enters the allocator.
    unsafe impl GlobalAlloc for Meter {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            let p = unsafe { System.alloc(l) };
            if !p.is_null() {
                add(l.size());
            }
            p
        }
        unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
            let p = unsafe { System.alloc_zeroed(l) };
            if !p.is_null() {
                add(l.size());
            }
            p
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            LIVE.fetch_sub(l.size(), Relaxed);
            unsafe { System.dealloc(p, l) }
        }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, size: usize) -> *mut u8 {
            let next = unsafe { System.realloc(p, l, size) };
            if !next.is_null() {
                if size >= l.size() {
                    add(size - l.size());
                } else {
                    LIVE.fetch_sub(l.size() - size, Relaxed);
                }
            }
            next
        }
    }
    pub fn measured<T>(f: impl FnOnce() -> T) -> (T, u128, usize) {
        let base = LIVE.load(Relaxed);
        PEAK.store(base, Relaxed);
        let start = std::time::Instant::now();
        let result = f();
        (
            result,
            start.elapsed().as_micros(),
            PEAK.load(Relaxed).saturating_sub(base),
        )
    }
}
#[global_allocator]
static HEAP: heap::Meter = heap::Meter;

#[test]
#[ignore = "run the prebuilt test alone during the local storage timing window"]
fn measure_verified_b39_import_save_and_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let src = temp.path().join("source");
    let dst = temp.path().join("destination");
    let source = sample();
    storage::save(&src, &source, None).unwrap();
    let mut scene = destination();
    let handle = scene.handle(&scene.slots[1]);
    let before = scene.clone();
    let initial = storage::save(&dst, &scene, None).unwrap();
    let mut costs = Vec::new();
    for slot in source.slots.iter() {
        let pin = slot.pin(&source.document);
        let (catalog, browse_us, browse_heap) = heap::measured(|| Catalog::open(&src).unwrap());
        let (review, review_us, review_heap) =
            heap::measured(|| catalog.preview(&pin, &scene).unwrap());
        let (added, adopt_us, adopt_heap) =
            heap::measured(|| catalog.import(&pin, &mut scene, &review).unwrap());
        costs.push(serde_json::json!({"read":review.cost,"added":added,"browse_us":browse_us,"browse_peak_heap_bytes":browse_heap,"review_us":review_us,"review_peak_heap_bytes":review_heap,"adopt_us":adopt_us,"adopt_peak_heap_bytes":adopt_heap}));
    }
    assert!(Arc::ptr_eq(&before.slots, &scene.slots));
    assert!(!Arc::ptr_eq(&before.parts, &scene.parts));
    let disk = |dir: &Path| {
        fs::read_dir(dir.join("pages"))
            .unwrap()
            .map(|f| f.unwrap().metadata().unwrap().len())
            .sum::<u64>()
    };
    let initial_bytes = disk(&dst);
    let (mut head, first_commit_us, first_commit_heap) =
        heap::measured(|| storage::save(&dst, &scene, Some(&initial)).unwrap());
    let import_delta = disk(&dst) - initial_bytes;
    let mut samples = Vec::new();
    let root = source.slots[1].pin(&source.document);
    for i in 0..=40 {
        let (catalog, browse_us, browse_heap) = heap::measured(|| Catalog::open(&src).unwrap());
        let (review, review_us, review_heap) =
            heap::measured(|| catalog.preview(&root, &scene).unwrap());
        let (count, reuse_us, reuse_heap) =
            heap::measured(|| catalog.import(&root, &mut scene, &review).unwrap());
        assert_eq!(count, 0);
        let previous = scene.clone();
        scene
            .edit(
                &handle,
                [DecimalValue::from_integer(i + 20); 3],
                previous.slot(&handle).unwrap().record.clone(),
            )
            .unwrap();
        assert!(Arc::ptr_eq(&previous.parts, &scene.parts));
        let (revision, save_us, save_heap) =
            heap::measured(|| storage::save(&dst, &scene, Some(&head)).unwrap());
        head = revision;
        let (opened, open_us, open_heap) = heap::measured(|| storage::open(&dst).unwrap());
        assert_eq!(opened.scene, scene);
        if i > 0 {
            samples.push(serde_json::json!({"browse_us":browse_us,"browse_heap":browse_heap,"preview_us":review_us,"preview_heap":review_heap,"reused_us":reuse_us,"reused_heap":reuse_heap,"save_us":save_us,"save_heap":save_heap,"open_us":open_us,"open_heap":open_heap}));
        }
    }
    let summary = serde_json::json!({"first_imports":costs,"first_commit_us":first_commit_us,"first_commit_heap":first_commit_heap,"new_immutable_page_bytes":import_delta,"undo_prior_definition_json_bytes":canonical_json_bytes(&*before.parts).unwrap().len(),"imported_definition_json_bytes":canonical_json_bytes(&*scene.parts).unwrap().len(),"untouched_shared_placement_json_bytes":canonical_json_bytes(&*before.slots).unwrap().len(),"samples":samples});
    eprintln!(
        "b39_measurements {}",
        serde_json::to_string(&summary).unwrap()
    );
    assert!(first_commit_us <= 200_000);
    assert!(
        samples
            .iter()
            .all(|s| s["save_us"].as_u64().unwrap() <= 200_000)
    );
}
