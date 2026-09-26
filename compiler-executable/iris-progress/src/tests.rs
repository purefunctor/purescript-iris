use super::*;
use itertools::Itertools;
use ratatui::backend::Backend;

fn model_with_packages(packages: &[(&str, Duration)], package_count: usize) -> ProgressModel {
    let mut model = ProgressModel::default();
    model.apply(ProgressEvent::PlanReady { package_count });
    for (package_name, duration) in packages {
        model.apply(ProgressEvent::PackageCompleted {
            package_name: SmolStr::new(package_name),
            duration: *duration,
        });
    }
    model
}

fn render_model(
    model: &ProgressModel,
    width: u16,
    height: u16,
    appearance: ProgressAppearance,
    elapsed: Duration,
) -> Buffer {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    ProgressView { model, appearance, elapsed }.render(area, &mut buffer);
    buffer
}

fn buffer_text(buffer: &Buffer) -> String {
    let rows = (0..buffer.area.height).map(|row| {
        let cells =
            (0..buffer.area.width).map(|column| buffer.cell((column, row)).unwrap().symbol());
        cells.collect::<String>().trim_end().to_owned()
    });
    rows.collect_vec().join("\n")
}

fn successful_initial_build(color: bool) -> String {
    let inputs = vec!["Application".to_owned(), "Library".to_owned()];
    render_watch_summary(
        WatchSummary {
            timestamp: "12:34:56",
            initial: true,
            changed_inputs: &inputs,
            duration: Duration::from_micros(12_500),
            outcome: WatchOutcome::Succeeded,
        },
        color,
    )
}

fn rebuild_with_diagnostics(color: bool) -> String {
    let inputs = vec!["Application".to_owned()];
    render_watch_summary(
        WatchSummary {
            timestamp: "12:35:01",
            initial: false,
            changed_inputs: &inputs,
            duration: Duration::from_secs_f64(1.25),
            outcome: WatchOutcome::Diagnostics,
        },
        color,
    )
}

fn failed_rebuild(color: bool) -> String {
    render_watch_summary(
        WatchSummary {
            timestamp: "12:35:02",
            initial: false,
            changed_inputs: &[],
            duration: Duration::from_millis(2),
            outcome: WatchOutcome::Failed,
        },
        color,
    )
}

fn waiting_for_inputs(color: bool) -> String {
    let inputs = vec!["src/Main.purs".to_owned()];
    render_watch_summary(
        WatchSummary {
            timestamp: "12:35:03",
            initial: false,
            changed_inputs: &inputs,
            duration: Duration::ZERO,
            outcome: WatchOutcome::Waiting,
        },
        color,
    )
}

fn rebuild_with_truncated_inputs(color: bool) -> String {
    let long_inputs = vec![
        "First".to_owned(),
        "Second\nModule".to_owned(),
        "Third".to_owned(),
        "Fourth".to_owned(),
        "Fifth".to_owned(),
    ];
    render_watch_summary(
        WatchSummary {
            timestamp: "12:35:04",
            initial: false,
            changed_inputs: &long_inputs,
            duration: Duration::ZERO,
            outcome: WatchOutcome::Succeeded,
        },
        color,
    )
}

#[test]
fn successful_initial_build_has_reviewable_plain_output() {
    let output = successful_initial_build(false);

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_snapshot!("watch_initial_success_plain", output);
    });
}

#[test]
fn rebuild_with_diagnostics_has_reviewable_plain_output() {
    let output = rebuild_with_diagnostics(false);

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_snapshot!("watch_diagnostics_plain", output);
    });
}

#[test]
fn failed_rebuild_has_reviewable_plain_output() {
    let output = failed_rebuild(false);

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_snapshot!("watch_failure_plain", output);
    });
}

#[test]
fn waiting_for_inputs_has_reviewable_plain_output() {
    let output = waiting_for_inputs(false);

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_snapshot!("watch_waiting_plain", output);
    });
}

#[test]
fn rebuild_with_truncated_inputs_has_reviewable_plain_output() {
    let output = rebuild_with_truncated_inputs(false);

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_snapshot!("watch_truncated_inputs_plain", output);
    });
}

#[test]
fn successful_initial_build_has_reviewable_colored_output() {
    let output = successful_initial_build(true);

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_debug_snapshot!("watch_initial_success_colored", output);
    });
}

#[test]
fn rebuild_with_diagnostics_has_reviewable_colored_output() {
    let output = rebuild_with_diagnostics(true);

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_debug_snapshot!("watch_diagnostics_colored", output);
    });
}

#[test]
fn failed_rebuild_has_reviewable_colored_output() {
    let output = failed_rebuild(true);

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_debug_snapshot!("watch_failure_colored", output);
    });
}

#[test]
fn waiting_for_inputs_has_reviewable_colored_output() {
    let output = waiting_for_inputs(true);

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_debug_snapshot!("watch_waiting_colored", output);
    });
}

#[test]
fn rebuild_with_truncated_inputs_has_reviewable_colored_output() {
    let output = rebuild_with_truncated_inputs(true);

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_debug_snapshot!("watch_truncated_inputs_colored", output);
    });
}

fn representative_compilation() -> ProgressModel {
    model_with_packages(
        &[
            ("short", Duration::from_micros(9_500)),
            ("a\nvery-long-package-name", Duration::from_micros(123_450)),
        ],
        12,
    )
}

fn representative_finalization() -> ProgressModel {
    let mut model = representative_compilation();
    model.apply(ProgressEvent::Finalizing { duration: Duration::from_millis(2_890) });
    model
}

fn representative_finished_compilation() -> ProgressModel {
    let mut model = representative_compilation();
    model.apply(ProgressEvent::Finished {
        duration: Duration::from_millis(3_210),
        outcome: ProgressOutcome::Diagnostics,
    });
    model
}

fn render_plain(model: &ProgressModel, width: u16, height: u16) -> String {
    let buffer =
        render_model(model, width, height, ProgressAppearance::Plain, Duration::from_millis(2_340));
    buffer_text(&buffer)
}

fn render_true_color_with_palette(
    model: &ProgressModel,
    width: u16,
    height: u16,
    foreground: (u8, u8, u8),
    background: (u8, u8, u8),
    theme_mode: ThemeMode,
) -> String {
    let buffer = render_model(
        model,
        width,
        height,
        ProgressAppearance::TrueColor { foreground, background, theme_mode },
        Duration::from_millis(2_340),
    );
    let previous = Buffer::empty(buffer.area);
    let changes = previous.diff(&buffer);
    let mut output = Vec::new();
    ratatui::crossterm::style::force_color_output(true);
    CrosstermBackend::new(&mut output).draw(changes.into_iter()).unwrap();
    String::from_utf8(output).unwrap()
}

fn render_true_color(model: &ProgressModel, width: u16, height: u16) -> String {
    render_true_color_with_palette(
        model,
        width,
        height,
        (230, 220, 210),
        (20, 30, 40),
        ThemeMode::Dark,
    )
}

#[test]
fn preparation_frame_has_reviewable_plain_output() {
    let output = render_plain(&ProgressModel::default(), 48, PROGRESS_REGION_HEIGHT);

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_snapshot!("preparation_frame_plain", output);
    });
}

#[test]
fn compilation_frame_has_reviewable_plain_output() {
    let output = render_plain(&representative_compilation(), 48, PROGRESS_REGION_HEIGHT);

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_snapshot!("compilation_frame_plain", output);
    });
}

#[test]
fn package_history_height_tracks_small_build_plan() {
    let packages = [
        ("prelude", Duration::from_micros(6_880)),
        ("effect", Duration::from_micros(3_250)),
        ("console", Duration::from_micros(980)),
        ("assert", Duration::from_micros(410)),
        ("acme", Duration::from_micros(110)),
    ];
    let model = model_with_packages(&packages, packages.len());
    let output = render_plain(&model, 48, progress_region_height(model.package_count));

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_snapshot!("small_build_plan_frame_plain", output);
    });
}

#[test]
fn finalization_frame_has_reviewable_plain_output() {
    let output = render_plain(&representative_finalization(), 48, PROGRESS_REGION_HEIGHT);

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_snapshot!("finalization_frame_plain", output);
    });
}

#[test]
fn finished_frame_has_reviewable_plain_output() {
    let output = render_plain(&representative_finished_compilation(), 48, PROGRESS_REGION_HEIGHT);

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_snapshot!("finished_frame_plain", output);
    });
}

#[test]
fn narrow_frame_has_reviewable_plain_output() {
    let output = render_plain(&representative_compilation(), 18, 4);

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_snapshot!("narrow_frame_plain", output);
    });
}

#[test]
fn preparation_frame_has_reviewable_true_color_output() {
    let output = render_true_color(&ProgressModel::default(), 48, PROGRESS_REGION_HEIGHT);

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_debug_snapshot!("preparation_frame_true_color_ansi", output);
    });
}

#[test]
fn compilation_frame_has_reviewable_true_color_output() {
    let output = render_true_color(&representative_compilation(), 48, PROGRESS_REGION_HEIGHT);

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_debug_snapshot!("compilation_frame_true_color_ansi", output);
    });
}

#[test]
fn finalization_frame_has_reviewable_true_color_output() {
    let output = render_true_color(&representative_finalization(), 48, PROGRESS_REGION_HEIGHT);

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_debug_snapshot!("finalization_frame_true_color_ansi", output);
    });
}

#[test]
fn finished_frame_has_reviewable_true_color_output() {
    let output =
        render_true_color(&representative_finished_compilation(), 48, PROGRESS_REGION_HEIGHT);

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_debug_snapshot!("finished_frame_true_color_ansi", output);
    });
}

#[test]
fn narrow_frame_has_reviewable_true_color_output() {
    let output = render_true_color(&representative_compilation(), 18, 4);

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_debug_snapshot!("narrow_frame_true_color_ansi", output);
    });
}

#[test]
fn compilation_frame_has_reviewable_light_true_color_output() {
    let output = render_true_color_with_palette(
        &representative_compilation(),
        48,
        PROGRESS_REGION_HEIGHT,
        (76, 83, 107),
        (248, 250, 255),
        ThemeMode::Light,
    );

    insta::with_settings!({ omit_expression => true }, {
        insta::assert_debug_snapshot!("compilation_frame_light_true_color_ansi", output);
    });
}

#[test]
fn finished_bar_remains_visible_on_matching_midtone_background() {
    let background = (156, 83, 88);
    let appearance = ProgressAppearance::TrueColor {
        foreground: (0, 0, 0),
        background,
        theme_mode: ThemeMode::Light,
    };
    for finished in [false, true] {
        let first_cell = bar_style(appearance, 0, 48, 12, 0, finished).fg.unwrap();
        assert_ne!(first_cell, Color::Rgb(background.0, background.1, background.2));
        let Color::Rgb(red, green, blue) = first_cell else { panic!("expected RGB") };
        let contrast = (relative_luminance(background) + 0.05)
            / (relative_luminance((red, green, blue)) + 0.05);
        assert!(contrast >= 3.0);
    }

    let output = render_true_color_with_palette(
        &representative_finished_compilation(),
        48,
        PROGRESS_REGION_HEIGHT,
        (0, 0, 0),
        background,
        ThemeMode::Light,
    );
    insta::with_settings!({ omit_expression => true }, {
        insta::assert_debug_snapshot!("finished_frame_midtone_light_true_color_ansi", output);
    });
}

#[test]
fn oldest_history_row_stays_readable_on_light_background() {
    for (foreground, background) in
        [((76, 83, 107), (248, 250, 255)), ((117, 117, 117), (255, 255, 255))]
    {
        let appearance =
            ProgressAppearance::TrueColor { foreground, background, theme_mode: ThemeMode::Light };
        let Color::Rgb(red, green, blue) = history_style(appearance, 0).fg.unwrap() else {
            panic!("expected RGB")
        };
        let contrast = (relative_luminance(background) + 0.05)
            / (relative_luminance((red, green, blue)) + 0.05);
        assert!(contrast >= 4.5, "oldest row on {background:?}: {red}, {green}, {blue}");
    }
    let low_contrast = ProgressAppearance::TrueColor {
        foreground: (0, 0, 0),
        background: (156, 83, 88),
        theme_mode: ThemeMode::Light,
    };
    assert_eq!(history_style(low_contrast, 0).fg, Some(Color::Rgb(0, 0, 0)));

    let packages = [
        ("prelude", Duration::from_millis(1)),
        ("effect", Duration::from_millis(1)),
        ("console", Duration::from_millis(1)),
        ("arrays", Duration::from_millis(1)),
        ("strings", Duration::from_millis(1)),
        ("maybe", Duration::from_millis(1)),
        ("either", Duration::from_millis(1)),
        ("control", Duration::from_millis(1)),
        ("aff", Duration::from_millis(1)),
        ("application", Duration::from_millis(1)),
    ];
    let model = model_with_packages(&packages, packages.len());
    let output = render_true_color_with_palette(
        &model,
        48,
        PROGRESS_REGION_HEIGHT,
        (117, 117, 117),
        (255, 255, 255),
        ThemeMode::Light,
    );
    insta::with_settings!({ omit_expression => true }, {
        insta::assert_debug_snapshot!("history_near_threshold_light_true_color_ansi", output);
    });
}

#[test]
fn filled_bar_contrasts_with_dark_and_gray_backgrounds() {
    for (foreground, background, theme_mode) in [
        ((255, 255, 255), (240, 128, 136), ThemeMode::Dark),
        ((34, 34, 34), (119, 119, 119), ThemeMode::Light),
    ] {
        let appearance = ProgressAppearance::TrueColor { foreground, background, theme_mode };
        for finished in [false, true] {
            let color = bar_style(appearance, 0, 48, 12, 7, finished).fg.unwrap();
            let Color::Rgb(red, green, blue) = color else { panic!("expected RGB") };
            let foreground_luminance = relative_luminance((red, green, blue));
            let background_luminance = relative_luminance(background);
            let contrast = (foreground_luminance.max(background_luminance) + 0.05)
                / (foreground_luminance.min(background_luminance) + 0.05);
            assert!(contrast >= 3.0, "{theme_mode:?} bar on {background:?}: {color:?}");
        }
    }

    let output = render_true_color_with_palette(
        &representative_finished_compilation(),
        48,
        PROGRESS_REGION_HEIGHT,
        (255, 255, 255),
        (240, 128, 136),
        ThemeMode::Dark,
    );
    insta::with_settings!({ omit_expression => true }, {
        insta::assert_debug_snapshot!("finished_frame_matching_dark_true_color_ansi", output);
    });
}

#[test]
fn model_transitions_are_deterministic_and_finished_is_authoritative() {
    let mut model = ProgressModel::default();
    model.apply(ProgressEvent::PlanReady { package_count: 2 });
    model.apply(ProgressEvent::PackageCompleted {
        package_name: SmolStr::new("diagnostic-package"),
        duration: Duration::from_millis(5),
    });
    model.apply(ProgressEvent::Finalizing { duration: Duration::from_millis(750) });
    model.apply(ProgressEvent::Finished {
        duration: Duration::from_secs(1),
        outcome: ProgressOutcome::Diagnostics,
    });

    assert_eq!(model.completed_count, 1);
    assert_eq!(model.completed_packages[0].package_name, "diagnostic-package");
    assert_eq!(
        model.phase,
        ProgressPhase::Finished {
            duration: Duration::from_secs(1),
            outcome: ProgressOutcome::Diagnostics,
        }
    );
}

#[test]
fn model_keeps_only_latest_ten_completions() {
    let names = (1..=12).map(|number| format!("package-{number}")).collect_vec();
    let mut model = ProgressModel::default();
    model.apply(ProgressEvent::PlanReady { package_count: names.len() });
    for package_name in names {
        model.apply(ProgressEvent::PackageCompleted {
            package_name: SmolStr::new(package_name),
            duration: Duration::from_millis(1),
        });
    }

    assert_eq!(model.completed_count, 12);
    assert_eq!(model.completed_packages.len(), 10);
    assert_eq!(model.completed_packages.front().unwrap().package_name, "package-3");
    assert_eq!(model.completed_packages.back().unwrap().package_name, "package-12");
}

#[test]
fn package_history_height_is_capped_at_ten_packages() {
    assert_eq!(package_history_height(0), 0);
    assert_eq!(package_history_height(5), 5);
    assert_eq!(package_history_height(10), 10);
    assert_eq!(package_history_height(12), 10);
}

#[test]
fn hidden_runtime_has_no_thread_and_barrier_is_a_no_op() {
    let runtime = ProgressRuntime::start(false, true);
    let reporter = runtime.reporter();
    assert!(!reporter.is_active());
    assert!(runtime.thread.is_none());
    reporter.report(ProgressEvent::Finished {
        duration: Duration::ZERO,
        outcome: ProgressOutcome::Succeeded,
    });
}
