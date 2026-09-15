fn compiler(path: &std::path::Path) -> datatest_stable::Result<()> {
    tests_integration::fixtures::compiler(path)
}

fn initial_build_cancellation(path: &std::path::Path) -> datatest_stable::Result<()> {
    use building::{QueryCancellation, QueryError};
    use iris_build::compile::{CompileError, InitialBuildConfig, PackageExecution, build_initial};
    use iris_build::events::{BuildEvent, BuildEventSink};

    #[derive(Clone, Copy)]
    enum CancelAt {
        BeforeBuild,
        Loading,
        PackageCompleted,
        Finalizing,
        Never,
    }

    struct Events {
        cancellation: QueryCancellation,
        point: CancelAt,
    }

    impl BuildEventSink for Events {
        fn send(&self, event: BuildEvent) {
            if matches!(
                (self.point, event),
                (CancelAt::PackageCompleted, BuildEvent::PackageCompleted { .. })
                    | (CancelAt::Finalizing, BuildEvent::Finalizing { .. })
            ) {
                self.cancellation.cancel();
            }
        }
    }

    let path = std::fs::canonicalize(path)?;
    for point in [
        CancelAt::BeforeBuild,
        CancelAt::Loading,
        CancelAt::PackageCompleted,
        CancelAt::Finalizing,
        CancelAt::Never,
    ] {
        let cancellation = QueryCancellation::default();
        let events = Events { cancellation: QueryCancellation::clone(&cancellation), point };
        if matches!(point, CancelAt::BeforeBuild) {
            cancellation.cancel();
        }
        let result = build_initial::<(), (), _>(InitialBuildConfig {
            root: path.parent().unwrap(),
            source_globs: std::slice::from_ref(&path),
            excluded: &[],
            packages: vec![iris_build::plan::PackageInput {
                name: "fixture".into(),
                source_identities: vec![std::path::PathBuf::clone(&path)],
                dependencies: vec![],
            }],
            prim_metadata: (),
            source_metadata: |_: &std::path::Path| {
                if matches!(point, CancelAt::Loading) {
                    cancellation.cancel();
                }
            },
            execution: PackageExecution::Parallel,
            cancellation: Some(QueryCancellation::clone(&cancellation)),
            events: &events,
        });
        if matches!(point, CancelAt::Never) {
            let build = result?;
            assert!(!build.has_errors());
            assert_eq!(build.sources().len(), 1);
            let snapshot = build.compilation().snapshot();
            assert!(snapshot.javascript(build.sources()[0])?.is_ok());
        } else {
            assert!(matches!(result, Err(CompileError::Query(QueryError::Cancelled))));
        }
    }
    Ok(())
}

datatest_stable::harness! {
    { test = compiler, root = "fixtures/compiler", pattern = r".*/Main\.purs$" },
    { test = initial_build_cancellation, root = "fixtures/compiler/1784777100_numeric_and_boolean_expression_literals", pattern = r"Main\.purs$" },
}
