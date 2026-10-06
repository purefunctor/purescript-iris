use std::sync::Arc;

use super::{
    AnalysisInvalidation, ContentAuthority, DiskObservation, DocumentKey, DocumentKind,
    FileLifecycle, ForeignEvent, LifecycleEvent, LifecycleWarning, ReloadFailure, SourceEvent,
    SourceUnitKey,
};
use crate::QueryEngine;
use files::ForeignSourceKind;

fn unit() -> SourceUnitKey {
    SourceUnitKey::new("file:///src/Main.purs", "file:///src/Main.js")
}

fn text(content: &str) -> Arc<str> {
    Arc::from(content)
}

fn source_opened(version: i32, content: &str) -> LifecycleEvent<i32, bool> {
    LifecycleEvent::Source {
        unit: unit(),
        event: SourceEvent::Opened { text: text(content), version, metadata: true },
    }
}

fn source_disk(content: &str) -> LifecycleEvent<i32, bool> {
    LifecycleEvent::Source {
        unit: unit(),
        event: SourceEvent::DiskObserved {
            disk: DiskObservation::Found(text(content)),
            metadata: true,
        },
    }
}

fn foreign_opened(version: i32, content: &str) -> LifecycleEvent<i32, bool> {
    LifecycleEvent::Foreign {
        unit: unit(),
        kind: ForeignSourceKind::JavaScript,
        event: ForeignEvent::Opened { text: text(content), version },
    }
}

fn foreign_disk(content: &str) -> LifecycleEvent<i32, bool> {
    foreign_disk_for(ForeignSourceKind::JavaScript, content)
}

fn foreign_disk_for(kind: ForeignSourceKind, content: &str) -> LifecycleEvent<i32, bool> {
    LifecycleEvent::Foreign {
        unit: unit(),
        kind,
        event: ForeignEvent::DiskObserved { disk: DiskObservation::Found(text(content)) },
    }
}

#[test]
fn open_source_ignores_disk_events_and_rejects_stale_changes() {
    let engine = QueryEngine::default();
    let mut files = FileLifecycle::default();
    let content = "module Main where\n";
    files.apply(&engine, source_opened(2, content));
    let file_id = files.source_id(unit().source()).unwrap();

    let event = LifecycleEvent::Source {
        unit: unit(),
        event: SourceEvent::DiskObserved { disk: DiskObservation::NotFound, metadata: true },
    };
    files.apply(&engine, event);
    assert_eq!(files.source_id(unit().source()), Some(file_id));
    assert_eq!(files.source_authority(&unit()), Some(ContentAuthority::Open));

    let event = LifecycleEvent::Source {
        unit: unit(),
        event: SourceEvent::Changed { text: text("module Newer where\n"), version: 1 },
    };
    files.apply(&engine, event);
    assert_eq!(engine.content(file_id).unwrap().as_ref(), content);
}

#[test]
fn source_close_reloads_retains_or_removes() {
    let engine = QueryEngine::default();
    let mut files = FileLifecycle::default();
    files.apply(&engine, source_opened(1, "module Main where\n"));
    let file_id = files.source_id(unit().source()).unwrap();

    let event = LifecycleEvent::Source {
        unit: unit(),
        event: SourceEvent::Closed {
            disk: DiskObservation::Failed(ReloadFailure::new(
                std::io::ErrorKind::PermissionDenied,
                "permission denied",
            )),
        },
    };
    files.apply(&engine, event);
    assert_eq!(files.source_authority(&unit()), Some(ContentAuthority::Retained));
    assert_eq!(engine.content(file_id).unwrap().as_ref(), "module Main where\n");

    files.apply(&engine, source_opened(2, "module Main where\n"));
    let event = LifecycleEvent::Source {
        unit: unit(),
        event: SourceEvent::Closed { disk: DiskObservation::NotFound },
    };
    let change = files.apply(&engine, event);
    assert_eq!(files.source_id(unit().source()), None);
    assert_eq!(change.removed_sources()[0].file_id, file_id);
    assert!(matches!(change.analysis(), AnalysisInvalidation::Workspace));
}

#[test]
fn removed_and_recreated_source_gets_new_identity() {
    let engine = QueryEngine::default();
    let mut files = FileLifecycle::default();
    files.apply(&engine, source_disk("module Main where\n"));
    let removed_id = files.source_id(unit().source()).unwrap();

    let event = LifecycleEvent::Source {
        unit: unit(),
        event: SourceEvent::DiskObserved { disk: DiskObservation::NotFound, metadata: true },
    };
    files.apply(&engine, event);
    files.apply(&engine, source_disk("module Main where\n"));
    let recreated_id = files.source_id(unit().source()).unwrap();
    assert!(recreated_id > removed_id);
}

#[test]
fn foreign_only_unit_associates_when_source_appears() {
    let engine = QueryEngine::default();
    let mut files = FileLifecycle::default();
    files.apply(&engine, foreign_opened(1, "export const life = 42;\n"));
    let foreign_id = files.foreign_id(unit().foreign()).unwrap();
    assert_eq!(files.foreign_authority(&unit()), Some(ContentAuthority::Open));

    files.apply(&engine, source_disk("module Main where\n"));
    let source_id = files.source_id(unit().source()).unwrap();
    assert_eq!(engine.foreign_file(source_id), Some(foreign_id));
}

#[test]
fn source_associates_both_javascript_and_jsx_candidates() {
    let engine = QueryEngine::default();
    let mut files = FileLifecycle::default();
    files.apply(
        &engine,
        foreign_disk_for(ForeignSourceKind::JavaScript, "export const life = 41;\n"),
    );
    files.apply(
        &engine,
        foreign_disk_for(ForeignSourceKind::Jsx, "export const life = <Life />;\n"),
    );
    files.apply(&engine, source_disk("module Main where\n"));

    let source_id = files.source_id(unit().source()).unwrap();
    let candidates = engine.foreign_files(source_id);
    assert!(candidates.get(ForeignSourceKind::JavaScript).is_some());
    assert!(candidates.get(ForeignSourceKind::Jsx).is_some());

    let event = LifecycleEvent::Foreign {
        unit: unit(),
        kind: ForeignSourceKind::JavaScript,
        event: ForeignEvent::DiskObserved { disk: DiskObservation::NotFound },
    };
    files.apply(&engine, event);
    assert_eq!(
        engine.foreign_file(source_id).map(|foreign| foreign.kind()),
        Some(ForeignSourceKind::Jsx)
    );
}

#[test]
fn open_foreign_ignores_disk_deletion_until_close() {
    let engine = QueryEngine::default();
    let mut files = FileLifecycle::default();
    files.apply(&engine, source_disk("module Main where\n"));
    files.apply(&engine, foreign_opened(1, "export const life = 42;\n"));
    let foreign_id = files.foreign_id(unit().foreign()).unwrap();

    let event = LifecycleEvent::Foreign {
        unit: unit(),
        kind: ForeignSourceKind::JavaScript,
        event: ForeignEvent::DiskObserved { disk: DiskObservation::NotFound },
    };
    files.apply(&engine, event);
    assert_eq!(files.foreign_id(unit().foreign()), Some(foreign_id));
    assert!(files.is_open(&DocumentKey::Foreign(unit(), ForeignSourceKind::JavaScript)));

    let event = LifecycleEvent::Foreign {
        unit: unit(),
        kind: ForeignSourceKind::JavaScript,
        event: ForeignEvent::Closed { disk: DiskObservation::NotFound },
    };
    files.apply(&engine, event);
    assert_eq!(files.foreign_id(unit().foreign()), None);
}

#[test]
fn invalid_events_are_ignored_with_typed_warnings() {
    let engine = QueryEngine::default();
    let mut files = FileLifecycle::default();

    let event = LifecycleEvent::Source {
        unit: unit(),
        event: SourceEvent::Changed { text: text("module Main where\n"), version: 1 },
    };
    let change = files.apply(&engine, event);
    assert!(matches!(
        change.warnings(),
        [LifecycleWarning::ChangedNonOpen { document: DocumentKind::Source, .. }]
    ));

    let event = LifecycleEvent::Foreign {
        unit: unit(),
        kind: ForeignSourceKind::JavaScript,
        event: ForeignEvent::Closed { disk: DiskObservation::NotFound },
    };
    let change = files.apply(&engine, event);
    assert!(matches!(
        change.warnings(),
        [LifecycleWarning::ClosedNonOpen { document: DocumentKind::Foreign(_), .. }]
    ));

    let failure = ReloadFailure::new(std::io::ErrorKind::PermissionDenied, "permission denied");
    let event = LifecycleEvent::Source {
        unit: unit(),
        event: SourceEvent::DiskObserved {
            disk: DiskObservation::Failed(ReloadFailure::clone(&failure)),
            metadata: true,
        },
    };
    let change = files.apply(&engine, event);
    assert!(matches!(
        change.warnings(),
        [LifecycleWarning::ReloadFailed {
            document: DocumentKind::Source,
            failure: observed,
            ..
        }] if *observed == failure
    ));
    assert_eq!(files.source_id(unit().source()), None);
}

#[test]
fn source_updates_preserve_identity_and_module_ownership() {
    let engine = QueryEngine::default();
    let mut files = FileLifecycle::default();
    files.apply(&engine, source_disk("module Main where\n"));
    let file_id = files.source_id(unit().source()).unwrap();
    assert_eq!(engine.module_file("Main"), Some(file_id));
    engine.parsed(file_id).unwrap();

    let event = LifecycleEvent::Source {
        unit: unit(),
        event: SourceEvent::DiskObserved {
            disk: DiskObservation::Found(text("module Library where\n")),
            metadata: false,
        },
    };
    let change = files.apply(&engine, event);
    assert_eq!(files.source_id(unit().source()), Some(file_id));
    assert_eq!(files.source_metadata(file_id), Some(&false));
    assert_eq!(engine.module_file("Main"), None);
    assert_eq!(engine.module_file("Library"), Some(file_id));
    assert!(matches!(change.analysis(), AnalysisInvalidation::Workspace));

    let event = LifecycleEvent::Source {
        unit: unit(),
        event: SourceEvent::Opened {
            text: text("module Library where\n"),
            version: 3,
            metadata: true,
        },
    };
    let change = files.apply(&engine, event);
    assert_eq!(files.source_version(file_id), Some(3));
    assert!(matches!(change.analysis(), AnalysisInvalidation::Sources(_)));
    engine.parsed(file_id).unwrap();

    let event = LifecycleEvent::Source {
        unit: unit(),
        event: SourceEvent::Changed { text: text("module Newer where\n"), version: 4 },
    };
    files.apply(&engine, event);
    assert_eq!(engine.module_file("Library"), None);
    assert_eq!(engine.module_file("Newer"), Some(file_id));
    engine.parsed(file_id).unwrap();

    let event = LifecycleEvent::Source {
        unit: unit(),
        event: SourceEvent::Closed { disk: DiskObservation::Found(text("module Disk where\n")) },
    };
    files.apply(&engine, event);
    assert_eq!(files.source_id(unit().source()), Some(file_id));
    assert_eq!(files.source_version(file_id), None);
    assert_eq!(files.source_metadata(file_id), Some(&true));
    assert_eq!(files.source_authority(&unit()), Some(ContentAuthority::Disk));
    assert_eq!(engine.module_file("Newer"), None);
    assert_eq!(engine.module_file("Disk"), Some(file_id));
}

#[test]
fn retained_source_recovers_without_changing_identity() {
    let engine = QueryEngine::default();
    let mut files = FileLifecycle::default();
    files.apply(&engine, source_disk("module Main where\n"));
    let file_id = files.source_id(unit().source()).unwrap();
    let failure = ReloadFailure::new(std::io::ErrorKind::TimedOut, "timed out");

    let event = LifecycleEvent::Source {
        unit: unit(),
        event: SourceEvent::DiskObserved {
            disk: DiskObservation::Failed(ReloadFailure::clone(&failure)),
            metadata: false,
        },
    };
    files.apply(&engine, event);
    assert_eq!(files.source_metadata(file_id), Some(&true));
    assert_eq!(files.source_authority(&unit()), Some(ContentAuthority::Retained));
    assert_eq!(files.source_reload_failure(&unit()), Some(&failure));
    assert_eq!(engine.content(file_id).unwrap().as_ref(), "module Main where\n");

    let event = LifecycleEvent::Source {
        unit: unit(),
        event: SourceEvent::DiskObserved {
            disk: DiskObservation::Found(text("module Recovered where\n")),
            metadata: false,
        },
    };
    files.apply(&engine, event);
    assert_eq!(files.source_id(unit().source()), Some(file_id));
    assert_eq!(files.source_metadata(file_id), Some(&false));
    assert_eq!(files.source_authority(&unit()), Some(ContentAuthority::Disk));
    assert_eq!(files.source_reload_failure(&unit()), None);
    assert_eq!(engine.content(file_id).unwrap().as_ref(), "module Recovered where\n");
}

#[test]
fn sibling_identity_and_association_follow_delete_and_recreate() {
    let engine = QueryEngine::default();
    let mut files = FileLifecycle::default();
    files.apply(&engine, source_disk("module Main where\n"));
    let event = LifecycleEvent::Foreign {
        unit: unit(),
        kind: ForeignSourceKind::JavaScript,
        event: ForeignEvent::DiskObserved {
            disk: DiskObservation::Found(text("export const life = 42;\n")),
        },
    };
    files.apply(&engine, event);
    let source_id = files.source_id(unit().source()).unwrap();
    let foreign_id = files.foreign_id(unit().foreign()).unwrap();
    assert_eq!(engine.foreign_file(source_id), Some(foreign_id));

    let event = LifecycleEvent::Source {
        unit: unit(),
        event: SourceEvent::DiskObserved { disk: DiskObservation::NotFound, metadata: true },
    };
    files.apply(&engine, event);
    assert_eq!(files.foreign_id(unit().foreign()), Some(foreign_id));

    files.apply(&engine, source_disk("module Main where\n"));
    let recreated_source_id = files.source_id(unit().source()).unwrap();
    assert!(recreated_source_id > source_id);
    assert_eq!(engine.foreign_file(recreated_source_id), Some(foreign_id));

    let event = LifecycleEvent::Foreign {
        unit: unit(),
        kind: ForeignSourceKind::JavaScript,
        event: ForeignEvent::DiskObserved { disk: DiskObservation::NotFound },
    };
    files.apply(&engine, event);
    assert_eq!(files.foreign_id(unit().foreign()), None);
    assert_eq!(engine.foreign_file(recreated_source_id), None);

    let event = LifecycleEvent::Foreign {
        unit: unit(),
        kind: ForeignSourceKind::JavaScript,
        event: ForeignEvent::DiskObserved {
            disk: DiskObservation::Found(text("export const life = 43;\n")),
        },
    };
    files.apply(&engine, event);
    let recreated_foreign_id = files.foreign_id(unit().foreign()).unwrap();
    assert!(recreated_foreign_id > foreign_id);
    assert_eq!(engine.foreign_file(recreated_source_id), Some(recreated_foreign_id));
}

#[test]
fn foreign_changes_reject_stale_versions_and_recover_retained_content() {
    let engine = QueryEngine::default();
    let mut files = FileLifecycle::default();
    files.apply(&engine, source_disk("module Main where\n"));
    files.apply(&engine, foreign_opened(2, "export const life = 42;\n"));
    let foreign_id = files.foreign_id(unit().foreign()).unwrap();

    let event = LifecycleEvent::Foreign {
        unit: unit(),
        kind: ForeignSourceKind::JavaScript,
        event: ForeignEvent::Changed { text: text("export const life = 1;\n"), version: 1 },
    };
    let change = files.apply(&engine, event);
    assert!(matches!(change.warnings(), [LifecycleWarning::StaleChange { .. }]));
    assert_eq!(engine.foreign_content(foreign_id).unwrap().as_ref(), "export const life = 42;\n");

    let event = LifecycleEvent::Foreign {
        unit: unit(),
        kind: ForeignSourceKind::JavaScript,
        event: ForeignEvent::Changed { text: text("export const life = 43;\n"), version: 3 },
    };
    files.apply(&engine, event);
    assert_eq!(engine.foreign_content(foreign_id).unwrap().as_ref(), "export const life = 43;\n");

    let failure = ReloadFailure::new(std::io::ErrorKind::Interrupted, "interrupted");
    let event = LifecycleEvent::Foreign {
        unit: unit(),
        kind: ForeignSourceKind::JavaScript,
        event: ForeignEvent::Closed {
            disk: DiskObservation::Failed(ReloadFailure::clone(&failure)),
        },
    };
    files.apply(&engine, event);
    assert_eq!(files.foreign_authority(&unit()), Some(ContentAuthority::Retained));
    assert_eq!(files.foreign_reload_failure(&unit()), Some(&failure));

    let event = LifecycleEvent::Foreign {
        unit: unit(),
        kind: ForeignSourceKind::JavaScript,
        event: ForeignEvent::DiskObserved {
            disk: DiskObservation::Found(text("export const life = 44;\n")),
        },
    };
    files.apply(&engine, event);
    assert_eq!(files.foreign_id(unit().foreign()), Some(foreign_id));
    assert_eq!(files.foreign_authority(&unit()), Some(ContentAuthority::Disk));
    assert_eq!(files.foreign_reload_failure(&unit()), None);
    assert_eq!(engine.foreign_content(foreign_id).unwrap().as_ref(), "export const life = 44;\n");
}

#[test]
fn one_source_locator_cannot_belong_to_two_sibling_pairs() {
    let engine = QueryEngine::default();
    let mut files = FileLifecycle::default();
    let original = unit();
    let conflicting = SourceUnitKey::new(original.source(), "file:///src/Alternate.js");

    files.apply(&engine, source_disk("module Main where\n"));
    files.apply(&engine, foreign_disk("export const life = 42;\n"));
    let original_source_id = files.source_id(original.source()).unwrap();

    let event = LifecycleEvent::Source {
        unit: SourceUnitKey::clone(&conflicting),
        event: SourceEvent::DiskObserved {
            disk: DiskObservation::Found(text("module Conflict where\n")),
            metadata: false,
        },
    };
    let change = files.apply(&engine, event);
    assert!(matches!(
        change.warnings(),
        [LifecycleWarning::LocatorAlreadyOwned { locator, owner, requested }]
            if locator.as_ref() == original.source()
                && owner == &original
                && requested == &conflicting
    ));
    assert_eq!(files.source_id(original.source()), Some(original_source_id));
    assert_eq!(files.foreign_id(conflicting.foreign()), None);

    let event = LifecycleEvent::Source {
        unit: SourceUnitKey::clone(&original),
        event: SourceEvent::DiskObserved { disk: DiskObservation::NotFound, metadata: true },
    };
    files.apply(&engine, event);
    let event = LifecycleEvent::Source {
        unit: SourceUnitKey::clone(&conflicting),
        event: SourceEvent::DiskObserved {
            disk: DiskObservation::Found(text("module Conflict where\n")),
            metadata: false,
        },
    };
    let change = files.apply(&engine, event);
    assert!(matches!(change.warnings(), [LifecycleWarning::LocatorAlreadyOwned { .. }]));

    let event = LifecycleEvent::Foreign {
        unit: SourceUnitKey::clone(&original),
        kind: ForeignSourceKind::JavaScript,
        event: ForeignEvent::DiskObserved { disk: DiskObservation::NotFound },
    };
    files.apply(&engine, event);
    let event = LifecycleEvent::Source {
        unit: SourceUnitKey::clone(&conflicting),
        event: SourceEvent::DiskObserved {
            disk: DiskObservation::Found(text("module Conflict where\n")),
            metadata: false,
        },
    };
    files.apply(&engine, event);
    let replacement_source_id = files.source_id(conflicting.source()).unwrap();
    assert!(replacement_source_id > original_source_id);
}

#[test]
fn one_foreign_locator_cannot_belong_to_two_sibling_pairs() {
    let engine = QueryEngine::default();
    let mut files = FileLifecycle::default();
    let original = unit();
    let conflicting = SourceUnitKey::new("file:///src/Alternate.purs", original.foreign());

    files.apply(&engine, source_disk("module Main where\n"));
    files.apply(&engine, foreign_disk("export const life = 42;\n"));
    let original_foreign_id = files.foreign_id(original.foreign()).unwrap();

    let event = LifecycleEvent::Foreign {
        unit: SourceUnitKey::clone(&conflicting),
        kind: ForeignSourceKind::JavaScript,
        event: ForeignEvent::DiskObserved {
            disk: DiskObservation::Found(text("export const conflict = 1;\n")),
        },
    };
    let change = files.apply(&engine, event);
    assert!(matches!(
        change.warnings(),
        [LifecycleWarning::LocatorAlreadyOwned { locator, owner, requested }]
            if locator.as_ref() == original.foreign()
                && owner == &original
                && requested == &conflicting
    ));
    assert_eq!(files.foreign_id(original.foreign()), Some(original_foreign_id));
    assert_eq!(files.source_id(conflicting.source()), None);

    let event = LifecycleEvent::Source {
        unit: SourceUnitKey::clone(&original),
        event: SourceEvent::DiskObserved { disk: DiskObservation::NotFound, metadata: true },
    };
    files.apply(&engine, event);
    let event = LifecycleEvent::Foreign {
        unit: SourceUnitKey::clone(&original),
        kind: ForeignSourceKind::JavaScript,
        event: ForeignEvent::DiskObserved { disk: DiskObservation::NotFound },
    };
    files.apply(&engine, event);

    let event = LifecycleEvent::Foreign {
        unit: SourceUnitKey::clone(&conflicting),
        kind: ForeignSourceKind::JavaScript,
        event: ForeignEvent::DiskObserved {
            disk: DiskObservation::Found(text("export const conflict = 1;\n")),
        },
    };
    files.apply(&engine, event);
    let replacement_foreign_id = files.foreign_id(conflicting.foreign()).unwrap();
    assert!(replacement_foreign_id > original_foreign_id);
}
