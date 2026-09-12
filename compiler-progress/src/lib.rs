use std::collections::VecDeque;
use std::io::{self, IsTerminal, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use console::{measure_text_width, truncate_str};
use itertools::Itertools;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use ratatui::{Terminal, TerminalOptions, Viewport};
use terminal_colorsaurus::QueryOptions;

const ANIMATION_INTERVAL: Duration = Duration::from_millis(80);
pub const PACKAGE_HISTORY_LENGTH: usize = 10;
pub const PROGRESS_REGION_WIDTH: usize = 80;
pub const PROGRESS_REGION_HEIGHT: u16 = PACKAGE_HISTORY_LENGTH as u16 + 3;

type ProgressTerminal = Terminal<CrosstermBackend<io::Stderr>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgressOutcome {
    Succeeded,
    Diagnostics,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProgressEvent {
    Preparing,
    PlanReady { package_count: usize },
    PackageCompleted { package_name: String, duration: Duration },
    Finalizing { duration: Duration },
    Finished { duration: Duration, outcome: ProgressOutcome },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletedPackage {
    pub package_name: String,
    pub duration: Duration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProgressPhase {
    Preparing,
    Compiling,
    Finalizing { duration: Duration },
    Finished { duration: Duration, outcome: ProgressOutcome },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgressModel {
    pub package_count: usize,
    pub completed_count: usize,
    pub completed_packages: VecDeque<CompletedPackage>,
    pub phase: ProgressPhase,
}

impl Default for ProgressModel {
    fn default() -> ProgressModel {
        ProgressModel {
            package_count: 0,
            completed_count: 0,
            completed_packages: VecDeque::with_capacity(PACKAGE_HISTORY_LENGTH),
            phase: ProgressPhase::Preparing,
        }
    }
}

impl ProgressModel {
    pub fn apply(&mut self, event: ProgressEvent) {
        match event {
            ProgressEvent::Preparing => *self = ProgressModel::default(),
            ProgressEvent::PlanReady { package_count } => {
                self.package_count = package_count;
                self.phase = ProgressPhase::Compiling;
            }
            ProgressEvent::PackageCompleted { package_name, duration } => {
                if self.completed_packages.len() == PACKAGE_HISTORY_LENGTH {
                    self.completed_packages.pop_front();
                }
                self.completed_packages.push_back(CompletedPackage { package_name, duration });
                self.completed_count += 1;
            }
            ProgressEvent::Finalizing { duration } => {
                self.phase = ProgressPhase::Finalizing { duration };
            }
            ProgressEvent::Finished { duration, outcome } => {
                self.phase = ProgressPhase::Finished { duration, outcome };
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgressAppearance {
    TrueColor { foreground: (u8, u8, u8), background: (u8, u8, u8) },
    Ansi,
    Plain,
}

pub struct ProgressView<'a> {
    pub model: &'a ProgressModel,
    pub appearance: ProgressAppearance,
    pub elapsed: Duration,
}

impl Widget for ProgressView<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let width = area.width.min(PROGRESS_REGION_WIDTH as u16) as usize;
        let area = Rect { width: width as u16, ..area };
        let animation_frame = animation_frame(self.elapsed);
        if matches!(self.model.phase, ProgressPhase::Preparing) {
            Paragraph::new(render_preparation(width, self.appearance, animation_frame))
                .render(Rect::new(area.x, area.y, area.width, 1), buffer);
            return;
        }
        if area.height < PROGRESS_REGION_HEIGHT {
            let bar = render_progress_bar(self.model, width, self.appearance, animation_frame);
            let bar_row = area.bottom() - area.height.min(2);
            Paragraph::new(bar).render(Rect::new(area.x, bar_row, area.width, 1), buffer);
            if area.height > 1 {
                Paragraph::new(render_progress_status(
                    self.model,
                    width,
                    self.appearance,
                    self.elapsed,
                ))
                .render(Rect::new(area.x, area.bottom() - 1, area.width, 1), buffer);
            }
            return;
        }

        let blank_rows = PACKAGE_HISTORY_LENGTH.saturating_sub(self.model.completed_packages.len());
        for (index, package) in self.model.completed_packages.iter().enumerate() {
            let row = blank_rows + index;
            let line = Line::styled(
                render_completed_package(package, width),
                history_style(self.appearance, row),
            );
            Paragraph::new(line)
                .render(Rect::new(area.x, area.y + row as u16, area.width, 1), buffer);
        }
        Paragraph::new("─".repeat(width)).render(
            Rect::new(area.x, area.y + PACKAGE_HISTORY_LENGTH as u16, area.width, 1),
            buffer,
        );
        Paragraph::new(render_progress_bar(self.model, width, self.appearance, animation_frame))
            .render(
                Rect::new(area.x, area.y + PACKAGE_HISTORY_LENGTH as u16 + 1, area.width, 1),
                buffer,
            );
        Paragraph::new(render_progress_status(self.model, width, self.appearance, self.elapsed))
            .render(
                Rect::new(area.x, area.y + PACKAGE_HISTORY_LENGTH as u16 + 2, area.width, 1),
                buffer,
            );
    }
}

enum RuntimeMessage {
    Event(ProgressEvent),
    Finish { duration: Duration, outcome: ProgressOutcome, acknowledged: Sender<()> },
    Stop,
}

#[derive(Clone)]
pub struct ProgressReporter {
    sender: Option<Sender<RuntimeMessage>>,
}

impl ProgressReporter {
    pub fn report(&self, event: ProgressEvent) {
        match event {
            ProgressEvent::Finished { duration, outcome } => self.finish(duration, outcome),
            event => {
                if let Some(sender) = &self.sender {
                    let _ = sender.send(RuntimeMessage::Event(event));
                }
            }
        }
    }

    /// Commits the final inline frame and waits until stderr is safe for diagnostics.
    fn finish(&self, duration: Duration, outcome: ProgressOutcome) {
        let Some(sender) = &self.sender else { return };
        let (acknowledged_sender, acknowledged_receiver) = mpsc::channel();
        let message =
            RuntimeMessage::Finish { duration, outcome, acknowledged: acknowledged_sender };
        if sender.send(message).is_ok() {
            let _ = acknowledged_receiver.recv();
        }
    }

    pub fn is_active(&self) -> bool {
        self.sender.is_some()
    }
}

pub struct ProgressRuntime {
    reporter: ProgressReporter,
    thread: Option<JoinHandle<()>>,
}

impl ProgressRuntime {
    pub fn start(show: bool, color: bool) -> ProgressRuntime {
        if !show || !io::stderr().is_terminal() {
            return ProgressRuntime { reporter: ProgressReporter { sender: None }, thread: None };
        }
        let (sender, receiver) = mpsc::channel();
        let thread = thread::spawn(move || run(receiver, color));
        ProgressRuntime {
            reporter: ProgressReporter { sender: Some(sender) },
            thread: Some(thread),
        }
    }

    pub fn reporter(&self) -> ProgressReporter {
        ProgressReporter::clone(&self.reporter)
    }
}

impl Drop for ProgressRuntime {
    fn drop(&mut self) {
        if let Some(sender) = &self.reporter.sender {
            let _ = sender.send(RuntimeMessage::Stop);
        }
        self.reporter.sender = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(receiver: Receiver<RuntimeMessage>, color: bool) {
    let appearance = detect_appearance(color);
    let backend = CrosstermBackend::new(io::stderr());
    let options = TerminalOptions { viewport: Viewport::Inline(1) };
    let Ok(mut terminal) = Terminal::with_options(backend, options) else { return };
    let started = Instant::now();
    let mut model = ProgressModel::default();
    let _ = draw(&mut terminal, &model, appearance, started.elapsed());

    loop {
        match receiver.recv_timeout(ANIMATION_INTERVAL) {
            Ok(RuntimeMessage::Event(event)) => {
                let expand = matches!(event, ProgressEvent::PlanReady { .. });
                model.apply(event);
                if expand {
                    let _ = clear_terminal(&mut terminal);
                    let backend = CrosstermBackend::new(io::stderr());
                    let options =
                        TerminalOptions { viewport: Viewport::Inline(PROGRESS_REGION_HEIGHT) };
                    if let Ok(expanded) = Terminal::with_options(backend, options) {
                        terminal = expanded;
                    }
                }
                let _ = draw(&mut terminal, &model, appearance, started.elapsed());
            }
            Ok(RuntimeMessage::Finish { duration, outcome, acknowledged }) => {
                model.apply(ProgressEvent::Finished { duration, outcome });
                let _ = draw(&mut terminal, &model, appearance, started.elapsed());
                let _ = finish_terminal(&mut terminal);
                drop(terminal);
                let _ = acknowledged.send(());
                return;
            }
            Ok(RuntimeMessage::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = clear_terminal(&mut terminal);
                return;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = draw(&mut terminal, &model, appearance, started.elapsed());
            }
        }
    }
}

fn animation_frame(elapsed: Duration) -> usize {
    (elapsed.as_millis() / ANIMATION_INTERVAL.as_millis()) as usize
}

fn draw(
    terminal: &mut ProgressTerminal,
    model: &ProgressModel,
    appearance: ProgressAppearance,
    elapsed: Duration,
) -> io::Result<Rect> {
    let mut viewport = Rect::ZERO;
    terminal.draw(|frame| {
        viewport = frame.area();
        frame.render_widget(ProgressView { model, appearance, elapsed }, viewport);
    })?;
    Ok(viewport)
}

fn clear_terminal(terminal: &mut ProgressTerminal) -> io::Result<()> {
    let origin = terminal.get_frame().area().as_position();
    terminal.clear()?;
    terminal.set_cursor_position(origin)?;
    terminal.show_cursor()?;
    Write::flush(terminal.backend_mut())
}

fn finish_terminal(terminal: &mut ProgressTerminal) -> io::Result<()> {
    let area = terminal.get_frame().area();
    let screen_height = terminal.size()?.height;
    if area.bottom() < screen_height {
        terminal.set_cursor_position((0, area.bottom()))?;
        terminal.show_cursor()?;
        Write::flush(terminal.backend_mut())
    } else if screen_height > 0 {
        terminal.set_cursor_position((0, screen_height - 1))?;
        terminal.show_cursor()?;
        terminal.backend_mut().write_all(b"\r\n")?;
        Write::flush(terminal.backend_mut())
    } else {
        Ok(())
    }
}

fn detect_appearance(color: bool) -> ProgressAppearance {
    if !color {
        return ProgressAppearance::Plain;
    }
    if !console::true_colors_enabled_stderr() {
        return ProgressAppearance::Ansi;
    }
    let mut options = QueryOptions::default();
    options.timeout = Duration::from_millis(100);
    let Ok(palette) = terminal_colorsaurus::color_palette(options) else {
        return ProgressAppearance::Ansi;
    };
    ProgressAppearance::TrueColor {
        foreground: palette.foreground.scale_to_8bit(),
        background: palette.background.scale_to_8bit(),
    }
}

fn render_completed_package(package: &CompletedPackage, width: usize) -> String {
    let milliseconds = package.duration.as_secs_f64() * 1_000.0;
    let timing = format!("{milliseconds:.2} ms");
    let timing_width = measure_text_width(&timing);
    if width <= timing_width + 2 {
        return truncate_str(&timing, width, "").to_string();
    }
    let name_width = width - timing_width - 2;
    let package_name = sanitize(&package.package_name);
    let package_name = truncate_str(&package_name, name_width, "…");
    let padding = name_width - measure_text_width(&package_name) + 2;
    format!("{package_name}{}{timing}", " ".repeat(padding))
}

fn render_progress_bar(
    model: &ProgressModel,
    width: usize,
    appearance: ProgressAppearance,
    animation_frame: usize,
) -> Line<'static> {
    if width <= 2 {
        return Line::from(truncate_str("[]", width, "").to_string());
    }
    let bar_width = width - 2;
    let mut spans = vec![Span::raw("[")];
    spans.extend(render_bar(
        appearance,
        bar_width,
        model.completed_count,
        model.package_count,
        animation_frame,
        matches!(model.phase, ProgressPhase::Finished { .. }),
    ));
    spans.push(Span::raw("]"));
    Line::from(spans)
}

fn render_progress_status(
    model: &ProgressModel,
    width: usize,
    appearance: ProgressAppearance,
    elapsed: Duration,
) -> Line<'static> {
    let status = match model.phase {
        ProgressPhase::Finished { duration, .. } | ProgressPhase::Finalizing { duration } => {
            format!(
                "Finished {} of {} packages in {duration:.2?}",
                model.completed_count, model.package_count
            )
        }
        ProgressPhase::Compiling => format!(
            "Compiling {} of {} packages in {elapsed:.2?}",
            model.completed_count, model.package_count
        ),
        ProgressPhase::Preparing => String::new(),
    };
    let status = truncate_str(&status, width, "…").into_owned();
    Line::from(Span::styled(status, accent_style(appearance)))
}

fn render_preparation(width: usize, appearance: ProgressAppearance, frame: usize) -> Line<'static> {
    let status = "Preparing";
    if width <= status.len() + 3 {
        return Line::from(truncate_str(status, width, "").to_string());
    }
    let bar_width = width - status.len() - 3;
    let mut spans = vec![Span::styled(status, accent_style(appearance)), Span::raw(" [")];
    spans.extend(render_activity_bar(appearance, bar_width, frame));
    spans.push(Span::raw("]"));
    Line::from(spans)
}

fn render_activity_bar(
    appearance: ProgressAppearance,
    width: usize,
    frame: usize,
) -> Vec<Span<'static>> {
    if appearance == ProgressAppearance::Plain {
        let mut bar = vec!['='; width];
        if width > 0 {
            bar[frame % width] = '#';
        }
        return vec![Span::raw(bar.into_iter().collect::<String>())];
    }
    let spans = (0..width)
        .map(|index| Span::styled("⣿", bar_style(appearance, index, width, width, frame, false)));
    spans.collect_vec()
}

fn render_bar(
    appearance: ProgressAppearance,
    width: usize,
    completed: usize,
    total: usize,
    frame: usize,
    finished: bool,
) -> Vec<Span<'static>> {
    let filled_eighths = if total == 0 { width * 8 } else { width * 8 * completed / total };
    if appearance == ProgressAppearance::Plain {
        let filled = filled_eighths / 8;
        let mut bar = String::with_capacity(width);
        bar.extend(std::iter::repeat_n('=', filled));
        if filled < width {
            bar.push('>');
            bar.extend(std::iter::repeat_n('-', width - filled - 1));
        }
        return vec![Span::raw(bar)];
    }
    const BRAILLE_FILL: [&str; 9] = ["⠀", "⡀", "⡄", "⡆", "⡇", "⣇", "⣧", "⣷", "⣿"];
    let filled = filled_eighths.div_ceil(8);
    let spans = (0..width).map(|index| {
        let eighths = filled_eighths.saturating_sub(index * 8).min(8);
        Span::styled(
            BRAILLE_FILL[eighths],
            bar_style(appearance, index, width, filled, frame, finished),
        )
    });
    spans.collect_vec()
}

fn bar_style(
    appearance: ProgressAppearance,
    index: usize,
    width: usize,
    filled: usize,
    frame: usize,
    finished: bool,
) -> Style {
    let style = Style::default();
    match appearance {
        ProgressAppearance::TrueColor { .. } if index < filled => {
            let progress = index as f32 / width.saturating_sub(1).max(1) as f32;
            let colors = [(240, 128, 136), (248, 152, 184), (184, 136, 224), (136, 72, 192)];
            let scaled = progress * (colors.len() - 1) as f32;
            let segment = (scaled.floor() as usize).min(colors.len() - 2);
            let progress = scaled - segment as f32;
            let from = colors[segment];
            let to = colors[segment + 1];
            let interpolate = |from: u8, to: u8| {
                (from as f32 + (to as f32 - from as f32) * progress).round() as u8
            };
            let mut color =
                (interpolate(from.0, to.0), interpolate(from.1, to.1), interpolate(from.2, to.2));
            if !finished && filled > 0 {
                let distance = index.abs_diff(frame % filled);
                let brightness = match distance {
                    0 => 0.88,
                    1 => 0.55,
                    2 => 0.25,
                    _ => 0.0,
                };
                let lighten = |channel: u8| {
                    (channel as f32 + (255.0 - channel as f32) * brightness).round() as u8
                };
                color = (lighten(color.0), lighten(color.1), lighten(color.2));
            }
            style.fg(Color::Rgb(color.0, color.1, color.2))
        }
        ProgressAppearance::TrueColor { foreground, .. } if index == filled => style
            .fg(Color::Rgb(foreground.0, foreground.1, foreground.2))
            .add_modifier(Modifier::BOLD),
        ProgressAppearance::TrueColor { foreground, background } => {
            let blend = |foreground: u8, background: u8| {
                (background as f32 + (foreground as f32 - background as f32) * 0.16).round() as u8
            };
            style.fg(Color::Rgb(
                blend(foreground.0, background.0),
                blend(foreground.1, background.1),
                blend(foreground.2, background.2),
            ))
        }
        ProgressAppearance::Ansi if index < filled => {
            let color = if index < filled / 3 {
                Color::Blue
            } else if index < filled * 2 / 3 {
                Color::Cyan
            } else {
                Color::Green
            };
            let style = style.fg(color);
            if !finished && index == frame % filled.max(1) {
                style.add_modifier(Modifier::BOLD)
            } else {
                style
            }
        }
        ProgressAppearance::Ansi if index == filled => {
            style.fg(Color::White).add_modifier(Modifier::BOLD)
        }
        ProgressAppearance::Ansi => style.fg(Color::DarkGray),
        ProgressAppearance::Plain => unreachable!("plain bars are rendered without styles"),
    }
}

fn history_style(appearance: ProgressAppearance, row: usize) -> Style {
    match appearance {
        ProgressAppearance::TrueColor { foreground, background } => {
            let opacity = 0.18 + 0.82 * row as f32 / (PACKAGE_HISTORY_LENGTH - 1) as f32;
            let blend = |foreground: u8, background: u8| {
                (background as f32 + (foreground as f32 - background as f32) * opacity).round()
                    as u8
            };
            Style::default().fg(Color::Rgb(
                blend(foreground.0, background.0),
                blend(foreground.1, background.1),
                blend(foreground.2, background.2),
            ))
        }
        ProgressAppearance::Ansi if row + 1 == PACKAGE_HISTORY_LENGTH => {
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
        }
        ProgressAppearance::Ansi if row * 2 < PACKAGE_HISTORY_LENGTH => {
            Style::default().add_modifier(Modifier::DIM)
        }
        ProgressAppearance::Ansi | ProgressAppearance::Plain => Style::default(),
    }
}

fn accent_style(appearance: ProgressAppearance) -> Style {
    match appearance {
        ProgressAppearance::Plain => Style::default(),
        ProgressAppearance::TrueColor { .. } | ProgressAppearance::Ansi => {
            Style::default().fg(Color::White)
        }
    }
}

fn sanitize(name: &str) -> String {
    name.chars().map(|character| if character.is_control() { '�' } else { character }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use itertools::Itertools;

    fn model_with_packages(names: &[&str], package_count: usize) -> ProgressModel {
        let mut model = ProgressModel::default();
        model.apply(ProgressEvent::PlanReady { package_count });
        for name in names {
            model.apply(ProgressEvent::PackageCompleted {
                package_name: (*name).to_owned(),
                duration: Duration::from_millis(125),
            });
        }
        model
    }

    fn render_model(
        model: &ProgressModel,
        width: u16,
        height: u16,
        appearance: ProgressAppearance,
    ) -> Buffer {
        let area = Rect::new(0, 0, width, height);
        let mut buffer = Buffer::empty(area);
        ProgressView { model, appearance, elapsed: Duration::ZERO }.render(area, &mut buffer);
        buffer
    }

    fn row_text(buffer: &Buffer, row: u16) -> String {
        (0..buffer.area.width).map(|column| buffer.cell((column, row)).unwrap().symbol()).collect()
    }

    #[test]
    fn model_transitions_are_deterministic_and_finished_is_authoritative() {
        let mut model = ProgressModel::default();
        model.apply(ProgressEvent::PlanReady { package_count: 2 });
        model.apply(ProgressEvent::PackageCompleted {
            package_name: "diagnostic-package".to_owned(),
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
                package_name,
                duration: Duration::from_millis(1),
            });
        }

        assert_eq!(model.completed_count, 12);
        assert_eq!(model.completed_packages.len(), 10);
        assert_eq!(model.completed_packages.front().unwrap().package_name, "package-3");
        assert_eq!(model.completed_packages.back().unwrap().package_name, "package-12");
    }

    #[test]
    fn frame_has_ten_history_rows_separator_and_status() {
        let model = model_with_packages(&["one", "two", "three"], 12);
        let buffer = render_model(&model, 60, PROGRESS_REGION_HEIGHT, ProgressAppearance::Plain);

        assert!(row_text(&buffer, 7).starts_with("one  "));
        assert!(row_text(&buffer, 9).starts_with("three  "));
        assert!(row_text(&buffer, 10).chars().all(|character| character == '─'));
        assert!(row_text(&buffer, 11).starts_with('['));
        assert!(row_text(&buffer, 12).starts_with("Compiling 3 of 12 packages in"));
    }

    #[test]
    fn frame_transitions_from_one_line_preparation_to_compilation() {
        let preparing = ProgressModel::default();
        let compiling = model_with_packages(&[], 12);
        let preparing =
            render_model(&preparing, 60, PROGRESS_REGION_HEIGHT, ProgressAppearance::Plain);
        let compiling =
            render_model(&compiling, 60, PROGRESS_REGION_HEIGHT, ProgressAppearance::Plain);

        assert!(row_text(&preparing, 0).starts_with("Preparing ["));
        assert!((1..PROGRESS_REGION_HEIGHT).all(|row| row_text(&preparing, row).trim().is_empty()));
        assert!(row_text(&compiling, 11).starts_with('['));
        assert!(row_text(&compiling, 12).starts_with("Compiling 0 of 12 packages in"));
    }

    #[test]
    fn preparation_has_a_full_animated_bar_and_white_status() {
        let appearance =
            ProgressAppearance::TrueColor { foreground: (220, 220, 220), background: (20, 20, 20) };
        let first = render_preparation(20, appearance, 0);
        let second = render_preparation(20, appearance, 1);
        let first_bar = &first.spans[2..first.spans.len() - 1];
        let second_bar = &second.spans[2..second.spans.len() - 1];

        assert_eq!(first.spans[0].style.fg, Some(Color::White));
        assert!(first_bar.iter().all(|span| span.content == "⣿"));
        assert_eq!(first_bar[0].style.fg, Some(Color::Rgb(253, 240, 241)));
        assert_ne!(first_bar[0].style.fg, second_bar[0].style.fg);

        let plain = render_preparation(20, ProgressAppearance::Plain, 0);
        assert_eq!(plain.spans[2].content, "#=======");
    }

    #[test]
    fn compilation_and_finished_statuses_are_white() {
        let mut model = model_with_packages(&["complete"], 2);
        let compiling_bar = render_progress_bar(&model, 60, ProgressAppearance::Ansi, 0);
        let compiling_status =
            render_progress_status(&model, 60, ProgressAppearance::Ansi, Duration::from_secs(2));
        assert!(compiling_bar.to_string().starts_with('['));
        assert!(!compiling_bar.to_string().contains("1/2"));
        assert_eq!(compiling_status.to_string(), "Compiling 1 of 2 packages in 2.00s");
        assert_eq!(compiling_status.spans.first().unwrap().style.fg, Some(Color::White));

        model.apply(ProgressEvent::Finished {
            duration: Duration::from_secs(1),
            outcome: ProgressOutcome::Succeeded,
        });
        let finished_status =
            render_progress_status(&model, 60, ProgressAppearance::Ansi, Duration::from_secs(2));
        assert_eq!(finished_status.to_string(), "Finished 1 of 2 packages in 1.00s");
        assert_eq!(finished_status.spans.first().unwrap().style.fg, Some(Color::White));
    }

    #[test]
    fn progress_bar_geometry_is_independent_of_package_count() {
        let mut model = model_with_packages(&[], 100);
        model.completed_count = 9;
        let single_digit =
            render_progress_bar(&model, 40, ProgressAppearance::Plain, 0).to_string();
        model.completed_count = 10;
        let double_digit =
            render_progress_bar(&model, 40, ProgressAppearance::Plain, 0).to_string();

        assert_eq!(measure_text_width(&single_digit), 40);
        assert_eq!(measure_text_width(&double_digit), 40);
        assert!(single_digit.starts_with('['));
        assert!(double_digit.starts_with('['));
        assert!(!single_digit.contains("9/100"));
        assert!(!double_digit.contains("10/100"));
    }

    #[test]
    fn frame_caps_width_and_aligns_timing_digits() {
        let mut model = ProgressModel::default();
        model.apply(ProgressEvent::PlanReady { package_count: 2 });
        for (package_name, duration) in [
            ("short", Duration::from_micros(9_500)),
            ("a-much-longer-package-name", Duration::from_micros(123_450)),
        ] {
            model.apply(ProgressEvent::PackageCompleted {
                package_name: package_name.to_owned(),
                duration,
            });
        }
        let buffer = render_model(&model, 100, PROGRESS_REGION_HEIGHT, ProgressAppearance::Plain);
        let short = row_text(&buffer, 8);
        let long = row_text(&buffer, 9);

        assert!(short[PROGRESS_REGION_WIDTH..].trim().is_empty());
        assert_eq!(
            short[..PROGRESS_REGION_WIDTH].find('.'),
            long[..PROGRESS_REGION_WIDTH].find('.')
        );
        assert!(short[..PROGRESS_REGION_WIDTH].ends_with("9.50 ms"));
        assert!(long[..PROGRESS_REGION_WIDTH].ends_with("123.45 ms"));
    }

    #[test]
    fn narrow_and_short_terminals_keep_status_visible() {
        let model = model_with_packages(&["completed"], 12);
        for width in 1..=8 {
            let buffer =
                render_model(&model, width, PROGRESS_REGION_HEIGHT, ProgressAppearance::Plain);
            assert_eq!(measure_text_width(&row_text(&buffer, 11)), width as usize);
        }
        let buffer = render_model(&model, 40, 4, ProgressAppearance::Plain);
        assert!(row_text(&buffer, 2).starts_with('['));
        assert!(row_text(&buffer, 3).contains("Compiling 1 of 12 packages in"));
        assert!(!row_text(&buffer, 3).contains("completed"));
    }

    #[test]
    fn frame_sanitizes_and_truncates_names_without_wrapping() {
        let model = model_with_packages(&["a\nvery-long-package-name"], 1);
        let buffer = render_model(&model, 18, PROGRESS_REGION_HEIGHT, ProgressAppearance::Plain);
        let package = row_text(&buffer, 9);
        assert!(package.contains('�'));
        assert!(package.contains('…'));
        assert!(package.ends_with("125.00 ms"));
    }

    #[test]
    fn history_opacity_is_fixed_by_row() {
        let appearance =
            ProgressAppearance::TrueColor { foreground: (240, 220, 200), background: (20, 30, 40) };
        assert_eq!(history_style(appearance, 0).fg, Some(Color::Rgb(60, 64, 69)));
        assert_eq!(history_style(appearance, 9).fg, Some(Color::Rgb(240, 220, 200)));

        let first = model_with_packages(&["first"], 2);
        let second = model_with_packages(&["first", "second"], 2);
        let first = render_model(&first, 60, PROGRESS_REGION_HEIGHT, appearance);
        let second = render_model(&second, 60, PROGRESS_REGION_HEIGHT, appearance);
        assert_eq!(first.cell((0, 9)).unwrap().style(), second.cell((0, 9)).unwrap().style());
        assert_ne!(second.cell((0, 8)).unwrap().style(), second.cell((0, 9)).unwrap().style());
    }

    #[test]
    fn colored_bar_uses_braille_and_iris_costume_gradient() {
        let appearance =
            ProgressAppearance::TrueColor { foreground: (240, 240, 240), background: (20, 20, 20) };
        let bar = render_bar(appearance, 7, 7, 7, 0, true);
        assert_eq!(bar.first().unwrap().style.fg, Some(Color::Rgb(240, 128, 136)));
        assert_eq!(bar[2].style.fg, Some(Color::Rgb(248, 152, 184)));
        assert_eq!(bar[4].style.fg, Some(Color::Rgb(184, 136, 224)));
        assert_eq!(bar.last().unwrap().style.fg, Some(Color::Rgb(136, 72, 192)));

        let partial = render_bar(ProgressAppearance::Ansi, 2, 1, 3, 0, false);
        assert_eq!(partial[0].content, "⣇");
        assert_eq!(partial[1].content, "⠀");
    }

    #[test]
    fn true_color_bar_animates_highlight_and_bottom_bar_has_no_package_name() {
        let appearance =
            ProgressAppearance::TrueColor { foreground: (240, 240, 240), background: (20, 20, 20) };
        let first = render_bar(appearance, 6, 3, 6, 0, false);
        let second = render_bar(appearance, 6, 3, 6, 1, false);
        assert_ne!(first[0].style.fg, second[0].style.fg);

        let model = model_with_packages(&["must-not-appear"], 2);
        let buffer = render_model(&model, 60, PROGRESS_REGION_HEIGHT, ProgressAppearance::Plain);
        assert!(!row_text(&buffer, 11).contains("must-not-appear"));
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
}
