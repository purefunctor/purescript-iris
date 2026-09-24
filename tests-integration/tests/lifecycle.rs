use std::fs;
use std::path::Path;
use std::sync::Arc;

use building::{
    DiskObservation, FileLifecycle, LifecycleEvent, QueryEngine, SourceEvent, SourceUnitKey,
};

fn source_disk(unit: &SourceUnitKey, content: &Arc<str>) -> LifecycleEvent<i32, ()> {
    LifecycleEvent::Source {
        unit: SourceUnitKey::clone(unit),
        event: SourceEvent::DiskObserved {
            disk: DiskObservation::Found(Arc::clone(content)),
            metadata: (),
        },
    }
}

fn missing_header(path: &Path) -> datatest_stable::Result<()> {
    let content: Arc<str> = fs::read_to_string(path)?.into();
    let missing: Arc<str> = fs::read_to_string(path.with_file_name("Missing.purs"))?.into();
    let unit = SourceUnitKey::new("file:///src/Main.purs", "file:///src/Main.js");
    let engine = QueryEngine::default();
    let mut files = FileLifecycle::default();
    let event = LifecycleEvent::Source {
        unit: SourceUnitKey::clone(&unit),
        event: SourceEvent::Opened { text: Arc::clone(&missing), version: 1, metadata: () },
    };
    files.apply(&engine, event);
    let file_id = files.source_id(unit.source()).unwrap();
    assert_eq!(engine.module_file("Main"), None);

    for (version, text, registered) in
        [(2, &content, true), (3, &missing, false), (4, &content, true)]
    {
        engine.parsed(file_id).unwrap();
        let event = LifecycleEvent::Source {
            unit: SourceUnitKey::clone(&unit),
            event: SourceEvent::Changed { text: Arc::clone(text), version },
        };
        files.apply(&engine, event);
        assert_eq!(files.source_id(unit.source()), Some(file_id));
        assert_eq!(engine.module_file("Main"), registered.then_some(file_id));
    }
    Ok(())
}

fn duplicate_registration(path: &Path) -> datatest_stable::Result<()> {
    let content: Arc<str> = fs::read_to_string(path)?.into();
    let renamed: Arc<str> = fs::read_to_string(path.with_file_name("Renamed.purs"))?.into();
    let edited: Arc<str> = fs::read_to_string(path.with_file_name("Edited.purs"))?.into();
    let unit = SourceUnitKey::new("file:///src/Main.purs", "file:///src/Main.js");
    let duplicate = SourceUnitKey::new("file:///other/Main.purs", "file:///other/Main.js");
    let engine = QueryEngine::default();
    let mut files = FileLifecycle::default();
    files.apply(&engine, source_disk(&unit, &content));
    let original_id = files.source_id(unit.source()).unwrap();
    engine.parsed(original_id).unwrap();

    files.apply(&engine, source_disk(&duplicate, &content));
    let duplicate_id = files.source_id(duplicate.source()).unwrap();
    assert_eq!(engine.module_file("Main"), Some(duplicate_id));

    files.apply(&engine, source_disk(&unit, &renamed));
    assert_eq!(engine.module_file("Main"), Some(duplicate_id));
    assert_eq!(engine.module_file("Renamed"), Some(original_id));

    files.apply(&engine, source_disk(&unit, &content));
    assert_eq!(engine.module_file("Renamed"), None);
    assert_eq!(engine.module_file("Main"), Some(original_id));

    engine.parsed(duplicate_id).unwrap();
    files.apply(&engine, source_disk(&duplicate, &edited));
    assert_eq!(engine.module_file("Main"), Some(duplicate_id));
    Ok(())
}

fn batched_registration(path: &Path) -> datatest_stable::Result<()> {
    let content: Arc<str> = fs::read_to_string(path)?.into();
    let renamed: Arc<str> = fs::read_to_string(path.with_file_name("Renamed.purs"))?.into();
    let edited: Arc<str> = fs::read_to_string(path.with_file_name("Edited.purs"))?.into();
    let unit = SourceUnitKey::new("file:///src/Main.purs", "file:///src/Main.js");
    let duplicate = SourceUnitKey::new("file:///other/Main.purs", "file:///other/Main.js");
    let events = || {
        vec![
            source_disk(&unit, &content),
            source_disk(&duplicate, &content),
            LifecycleEvent::Source {
                unit: SourceUnitKey::clone(&duplicate),
                event: SourceEvent::DiskObserved { disk: DiskObservation::NotFound, metadata: () },
            },
            source_disk(&duplicate, &renamed),
            source_disk(&unit, &renamed),
            source_disk(&duplicate, &edited),
            source_disk(&unit, &content),
        ]
    };

    for length in 0..=events().len() {
        let individual_engine = QueryEngine::default();
        let mut individual_files = FileLifecycle::default();
        let mut individual_change = building::LifecycleChange::default();
        for event in events().into_iter().take(length) {
            individual_change.combine(individual_files.apply(&individual_engine, event));
        }
        let batched_engine = QueryEngine::default();
        let mut batched_files = FileLifecycle::default();
        let batched_change =
            batched_files.apply_all(&batched_engine, events().into_iter().take(length));
        assert_eq!(batched_change, individual_change, "prefix {length}");
        for name in ["Main", "Renamed"] {
            assert_eq!(
                batched_engine.module_file(name),
                individual_engine.module_file(name),
                "{name}, prefix {length}"
            );
        }
        for unit in [&unit, &duplicate] {
            assert_eq!(
                batched_files.source_id(unit.source()),
                individual_files.source_id(unit.source())
            );
        }
        if length == 3 {
            assert_eq!(batched_engine.module_file("Main"), None);
        }
    }
    Ok(())
}

datatest_stable::harness! {
    { test = missing_header, root = "fixtures/lifecycle/missing_header", pattern = r"Main\.purs$" },
    { test = duplicate_registration, root = "fixtures/lifecycle/duplicate_registration", pattern = r"Main\.purs$" },
    { test = batched_registration, root = "fixtures/lifecycle/duplicate_registration", pattern = r"Main\.purs$" },
}
