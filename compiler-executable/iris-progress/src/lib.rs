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
use smol_str::SmolStr;
use terminal_colorsaurus::QueryOptions;

const ANIMATION_INTERVAL: Duration = Duration::from_millis(80);
pub const PACKAGE_HISTORY_LENGTH: usize = 10;
pub const PROGRESS_REGION_WIDTH: usize = 80;
pub const PROGRESS_REGION_HEIGHT: u16 = progress_region_height(PACKAGE_HISTORY_LENGTH);
const WATCH_INPUT_DISPLAY_LIMIT: usize = 4;

const fn package_history_height(package_count: usize) -> usize {
    if package_count < PACKAGE_HISTORY_LENGTH { package_count } else { PACKAGE_HISTORY_LENGTH }
}

const fn progress_region_height(package_count: usize) -> u16 {
    package_history_height(package_count) as u16 + 3
}

type ProgressTerminal = Terminal<CrosstermBackend<io::Stderr>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgressOutcome {
    Succeeded,
    Diagnostics,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WatchOutcome {
    Succeeded,
    Diagnostics,
    Failed,
    Waiting,
}

pub struct WatchSummary<'a> {
    pub timestamp: &'a str,
    pub initial: bool,
    pub changed_inputs: &'a [String],
    pub duration: Duration,
    pub outcome: WatchOutcome,
}

pub fn render_watch_summary(summary: WatchSummary<'_>, color: bool) -> String {
    let timestamp =
        console::Style::new().cyan().dim().force_styling(color).apply_to(summary.timestamp);
    let (status, style) = match (summary.initial, summary.outcome) {
        (true, WatchOutcome::Succeeded) => ("Build succeeded", console::Style::new().green()),
        (false, WatchOutcome::Succeeded) => ("Rebuild succeeded", console::Style::new().green()),
        (true, WatchOutcome::Diagnostics) => {
            ("Build completed with diagnostics", console::Style::new().yellow())
        }
        (false, WatchOutcome::Diagnostics) => {
            ("Rebuild completed with diagnostics", console::Style::new().yellow())
        }
        (true, WatchOutcome::Failed) => ("Build failed", console::Style::new().red()),
        (false, WatchOutcome::Failed) => ("Rebuild failed", console::Style::new().red()),
        (_, WatchOutcome::Waiting) => {
            ("No input files; waiting for changes", console::Style::new().cyan())
        }
    };
    let status = style.bold().force_styling(color).apply_to(status);
    let headline = if summary.outcome == WatchOutcome::Waiting {
        format!("{timestamp}  {status}")
    } else {
        let duration = format_watch_duration(summary.duration);
        format!("{timestamp}  {status} in {duration}")
    };
    if summary.changed_inputs.is_empty() {
        return headline;
    }

    let count = summary.changed_inputs.len();
    let noun = if count == 1 { "input" } else { "inputs" };
    let action = if summary.initial { "Loaded" } else { "Changed" };
    let prefix = format!("{action} {count} {noun}: ");
    let available = PROGRESS_REGION_WIDTH.saturating_sub(10 + measure_text_width(&prefix));
    let displayed = render_watch_inputs(summary.changed_inputs, available);
    let details =
        console::Style::new().dim().force_styling(color).apply_to(format!("{prefix}{displayed}"));
    format!("{headline}\n          {details}")
}

fn render_watch_inputs(inputs: &[String], width: usize) -> String {
    let sanitized = inputs.iter().map(|input| sanitize(input));
    let sanitized = sanitized.collect_vec();
    let mut displayed_count = inputs.len().min(WATCH_INPUT_DISPLAY_LIMIT);
    loop {
        let mut displayed = sanitized[..displayed_count].join(", ");
        let omitted = inputs.len() - displayed_count;
        if omitted > 0 {
            if !displayed.is_empty() {
                displayed.push_str(", ");
            }
            displayed.push_str(&format!("… +{omitted} more"));
        }
        if measure_text_width(&displayed) <= width || displayed_count == 0 {
            return truncate_str(&displayed, width, "…").into_owned();
        }
        displayed_count -= 1;
    }
}

fn format_watch_duration(duration: Duration) -> String {
    if duration >= Duration::from_secs(1) {
        format!("{:.2} s", duration.as_secs_f64())
    } else {
        format!("{:.2} ms", duration.as_secs_f64() * 1_000.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProgressEvent {
    Preparing,
    PlanReady { package_count: usize },
    PackageCompleted { package_name: SmolStr, duration: Duration },
    Finalizing { duration: Duration },
    Finished { duration: Duration, outcome: ProgressOutcome },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletedPackage {
    pub package_name: SmolStr,
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
        let package_history_height = package_history_height(self.model.package_count);
        if area.height < progress_region_height(self.model.package_count) {
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

        let blank_rows = package_history_height.saturating_sub(self.model.completed_packages.len());
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
            Rect::new(area.x, area.y + package_history_height as u16, area.width, 1),
            buffer,
        );
        Paragraph::new(render_progress_bar(self.model, width, self.appearance, animation_frame))
            .render(
                Rect::new(area.x, area.y + package_history_height as u16 + 1, area.width, 1),
                buffer,
            );
        Paragraph::new(render_progress_status(self.model, width, self.appearance, self.elapsed))
            .render(
                Rect::new(area.x, area.y + package_history_height as u16 + 2, area.width, 1),
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
                    let height = progress_region_height(model.package_count);
                    let options = TerminalOptions { viewport: Viewport::Inline(height) };
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
    let filled_eighths = (width * 8 * completed).checked_div(total).unwrap_or(width * 8);
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
mod tests;
