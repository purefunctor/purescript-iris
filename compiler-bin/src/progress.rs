use std::collections::VecDeque;
use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use console::{Color, Style, measure_text_width, truncate_str};
use indicatif::{MultiProgress, ProgressBar, ProgressState, ProgressStyle};
use itertools::Itertools;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color as RatatuiColor, Modifier, Style as RatatuiStyle};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use ratatui::{Terminal, TerminalOptions, Viewport};
use terminal_colorsaurus::{QueryOptions, ThemeMode};

const CARGO_PROGRESS_REGION_WIDTH: usize = 50;
const CARGO_PROGRESS_FIXED_OVERHEAD: usize = 17;
const SHIMMER_FRAME_INTERVAL: Duration = Duration::from_millis(80);
const PACKAGE_ANIMATION_INTERVAL: Duration = Duration::from_millis(80);
const PACKAGE_HISTORY_LENGTH: usize = 10;
const PACKAGE_REGION_WIDTH: usize = 80;
const PACKAGE_PREPARATION_HEIGHT: u16 = 1;
const PACKAGE_REGION_HEIGHT: u16 = PACKAGE_HISTORY_LENGTH as u16 + 2;
static TERMINAL_THEME_MODE: OnceLock<Option<ThemeMode>> = OnceLock::new();

type PackageTerminal = Terminal<CrosstermBackend<io::Stderr>>;

pub(crate) struct PackageProgress {
    state: Arc<Mutex<PackageProgressState>>,
    terminal: Option<Arc<Mutex<PackageTerminal>>>,
    appearance: PackageAppearance,
    started: Instant,
    animation: Option<PackageAnimation>,
}

struct PackageAnimation {
    active: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

struct PackageProgressState {
    total: usize,
    completed_count: usize,
    completed: VecDeque<CompletedPackage>,
    phase: PackageProgressPhase,
}

struct CompletedPackage {
    name: String,
    elapsed: Duration,
}

enum PackageProgressPhase {
    Preparing,
    Compiling,
    Finished(Duration),
}

#[derive(Clone, Copy)]
enum PackageAppearance {
    TrueColor { foreground: (u8, u8, u8), background: (u8, u8, u8) },
    Ansi,
    Plain,
}

pub(crate) fn bar(total: usize, phase: &'static str, show: bool) -> ProgressBar {
    if !show {
        return ProgressBar::hidden();
    }
    let progress = ProgressBar::new(total as u64);
    configure(&progress, phase);
    progress
}

pub(crate) fn phase(
    progress: &MultiProgress,
    total: usize,
    phase: &'static str,
    show: bool,
) -> ProgressBar {
    if !show {
        return ProgressBar::hidden();
    }
    let phase_progress = progress.add(ProgressBar::new(total as u64));
    configure(&phase_progress, phase);
    phase_progress
}

pub(crate) fn set_message(progress: &ProgressBar, message: &str) {
    progress.set_message(message.to_string());
}

pub(crate) fn finish(progress: &ProgressBar) {
    progress.set_message("");
    progress.finish();
}

pub(crate) fn packages(show: bool, color: bool) -> PackageProgress {
    let state = Arc::new(Mutex::new(PackageProgressState {
        total: 0,
        completed_count: 0,
        completed: VecDeque::with_capacity(PACKAGE_HISTORY_LENGTH),
        phase: PackageProgressPhase::Preparing,
    }));
    let appearance = package_appearance(color);
    let terminal = if show && io::stderr().is_terminal() {
        let backend = CrosstermBackend::new(io::stderr());
        let options = TerminalOptions { viewport: Viewport::Inline(PACKAGE_PREPARATION_HEIGHT) };
        Terminal::with_options(backend, options).ok().map(|terminal| Arc::new(Mutex::new(terminal)))
    } else {
        None
    };
    let started = Instant::now();
    let animation = terminal.as_ref().map(|terminal| {
        let active = Arc::new(AtomicBool::new(true));
        let animation_active = Arc::clone(&active);
        let animation_state = Arc::clone(&state);
        let animation_terminal = Arc::clone(terminal);
        let thread = thread::spawn(move || {
            while animation_active.load(Ordering::Acquire) {
                thread::sleep(PACKAGE_ANIMATION_INTERVAL);
                if !animation_active.load(Ordering::Acquire) {
                    break;
                }
                let animation_frame = package_animation_frame(started.elapsed());
                let _ = draw_packages(
                    &animation_state,
                    &animation_terminal,
                    appearance,
                    animation_frame,
                );
            }
        });
        PackageAnimation { active, thread: Mutex::new(Some(thread)) }
    });
    let progress = PackageProgress { state, terminal, appearance, started, animation };
    progress.draw();
    progress
}

impl PackageProgress {
    pub(crate) fn begin_compilation(&self, total: usize) {
        let mut state =
            self.state.lock().expect("invariant violated: package progress state is not poisoned");
        state.total = total;
        state.phase = PackageProgressPhase::Compiling;
        drop(state);
        self.expand_terminal();
        self.draw();
    }

    pub(crate) fn complete(&self, name: &str, elapsed: Duration) {
        let mut state =
            self.state.lock().expect("invariant violated: package progress state is not poisoned");
        if state.completed.len() == PACKAGE_HISTORY_LENGTH {
            state.completed.pop_front();
        }
        state.completed.push_back(CompletedPackage { name: name.to_owned(), elapsed });
        state.completed_count += 1;
        drop(state);
        self.draw();
    }

    pub(crate) fn finish(&self, elapsed: Duration) {
        self.stop_animation();
        let mut state =
            self.state.lock().expect("invariant violated: package progress state is not poisoned");
        state.phase = PackageProgressPhase::Finished(elapsed);
        drop(state);
        if let Some(terminal) = &self.terminal {
            let _ = finish_packages(&self.state, terminal, self.appearance);
        }
    }

    pub(crate) fn clear(&self) {
        self.stop_animation();
        if let Some(terminal) = &self.terminal {
            let mut terminal = terminal
                .lock()
                .expect("invariant violated: package progress terminal is not poisoned");
            let origin = terminal.get_frame().area().as_position();
            let _ = terminal.clear();
            let _ = terminal.set_cursor_position(origin);
            let _ = terminal.show_cursor();
            let _ = Write::flush(terminal.backend_mut());
        }
    }

    fn draw(&self) {
        let Some(terminal) = &self.terminal else {
            return;
        };
        let animation_frame = package_animation_frame(self.started.elapsed());
        let _ = draw_packages(&self.state, terminal, self.appearance, animation_frame);
    }

    fn expand_terminal(&self) {
        let Some(terminal) = &self.terminal else {
            return;
        };
        let mut terminal =
            terminal.lock().expect("invariant violated: package progress terminal is not poisoned");
        let origin = terminal.get_frame().area().as_position();
        let _ = terminal.clear();
        let _ = terminal.set_cursor_position(origin);
        let _ = Write::flush(terminal.backend_mut());

        let backend = CrosstermBackend::new(io::stderr());
        let options = TerminalOptions { viewport: Viewport::Inline(PACKAGE_REGION_HEIGHT) };
        if let Ok(expanded) = Terminal::with_options(backend, options) {
            *terminal = expanded;
        }
    }

    fn stop_animation(&self) {
        let Some(animation) = &self.animation else {
            return;
        };
        animation.active.store(false, Ordering::Release);
        if let Some(thread) = animation
            .thread
            .lock()
            .expect("invariant violated: package animation thread is not poisoned")
            .take()
        {
            let _ = thread.join();
        }
    }
}

impl Drop for PackageProgress {
    fn drop(&mut self) {
        self.stop_animation();
    }
}

fn package_animation_frame(elapsed: Duration) -> usize {
    (elapsed.as_millis() / PACKAGE_ANIMATION_INTERVAL.as_millis()) as usize
}

fn draw_packages(
    state: &Arc<Mutex<PackageProgressState>>,
    terminal: &Arc<Mutex<PackageTerminal>>,
    appearance: PackageAppearance,
    animation_frame: usize,
) -> io::Result<Rect> {
    let state = state.lock().expect("invariant violated: package progress state is not poisoned");
    let mut terminal =
        terminal.lock().expect("invariant violated: package progress terminal is not poisoned");
    let mut viewport = Rect::ZERO;
    terminal.draw(|frame| {
        viewport = frame.area();
        let content = Rect { width: viewport.width.min(PACKAGE_REGION_WIDTH as u16), ..viewport };
        frame.render_widget(PackageView { state: &state, appearance, animation_frame }, content);
    })?;
    Ok(viewport)
}

fn finish_packages(
    state: &Arc<Mutex<PackageProgressState>>,
    terminal: &Arc<Mutex<PackageTerminal>>,
    appearance: PackageAppearance,
) -> io::Result<()> {
    let area = draw_packages(state, terminal, appearance, 0)?;
    let mut terminal =
        terminal.lock().expect("invariant violated: package progress terminal is not poisoned");
    let screen_height = terminal.size()?.height;
    if area.bottom() < screen_height {
        terminal.set_cursor_position((0, area.bottom()))?;
        terminal.show_cursor()?;
        Write::flush(terminal.backend_mut())
    } else if screen_height > 0 {
        terminal.set_cursor_position((0, screen_height - 1))?;
        terminal.show_cursor()?;
        let writer = terminal.backend_mut();
        writer.write_all(b"\r\n")?;
        Write::flush(writer)
    } else {
        Ok(())
    }
}

fn package_appearance(color: bool) -> PackageAppearance {
    if !color {
        return PackageAppearance::Plain;
    }
    if !console::true_colors_enabled_stderr() {
        return PackageAppearance::Ansi;
    }

    let mut options = QueryOptions::default();
    options.timeout = Duration::from_millis(100);
    let Ok(palette) = terminal_colorsaurus::color_palette(options) else {
        return PackageAppearance::Ansi;
    };
    PackageAppearance::TrueColor {
        foreground: palette.foreground.scale_to_8bit(),
        background: palette.background.scale_to_8bit(),
    }
}

struct PackageView<'a> {
    state: &'a PackageProgressState,
    appearance: PackageAppearance,
    animation_frame: usize,
}

impl Widget for PackageView<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let width = area.width as usize;
        if matches!(self.state.phase, PackageProgressPhase::Preparing) {
            Paragraph::new(render_package_preparation(
                width,
                self.appearance,
                self.animation_frame,
            ))
            .render(Rect::new(area.x, area.y, area.width, 1), buffer);
            return;
        }
        if area.height < PACKAGE_REGION_HEIGHT {
            Paragraph::new(render_package_progress(
                self.state,
                width,
                self.appearance,
                self.animation_frame,
            ))
            .render(Rect::new(area.x, area.bottom() - 1, area.width, 1), buffer);
            return;
        }

        let blank_rows = PACKAGE_HISTORY_LENGTH.saturating_sub(self.state.completed.len());
        for (index, package) in self.state.completed.iter().enumerate() {
            let row = blank_rows + index;
            let line = Line::styled(
                render_completed_package(package, width),
                package_history_style(self.appearance, row),
            );
            Paragraph::new(line)
                .render(Rect::new(area.x, area.y + row as u16, area.width, 1), buffer);
        }

        if area.height > PACKAGE_HISTORY_LENGTH as u16 {
            let separator = "─".repeat(width);
            Paragraph::new(separator).render(
                Rect::new(area.x, area.y + PACKAGE_HISTORY_LENGTH as u16, area.width, 1),
                buffer,
            );
        }
        Paragraph::new(render_package_progress(
            self.state,
            width,
            self.appearance,
            self.animation_frame,
        ))
        .render(
            Rect::new(area.x, area.y + PACKAGE_HISTORY_LENGTH as u16 + 1, area.width, 1),
            buffer,
        );
    }
}

fn render_completed_package(package: &CompletedPackage, width: usize) -> String {
    let elapsed_milliseconds = package.elapsed.as_secs_f64() * 1_000.0;
    let timing = format!("{elapsed_milliseconds:.2} ms");
    let timing_width = measure_text_width(&timing);
    if width <= timing_width + 2 {
        return truncate_str(&timing, width, "").to_string();
    }

    let name_width = width - timing_width - 2;
    let name = sanitize(&package.name);
    let name = truncate_str(&name, name_width, "…");
    let padding = name_width - measure_text_width(&name) + 2;
    format!("{name}{}{timing}", " ".repeat(padding))
}

fn render_package_progress(
    state: &PackageProgressState,
    width: usize,
    appearance: PackageAppearance,
    animation_frame: usize,
) -> Line<'static> {
    let completed = state.completed_count;
    let status = match state.phase {
        PackageProgressPhase::Finished(elapsed) => format!("Finished in {elapsed:.2?}"),
        PackageProgressPhase::Compiling => "Compiling".to_owned(),
        PackageProgressPhase::Preparing => unreachable!("preparation is rendered separately"),
    };
    let count = format!("{completed}/{}", state.total);
    let count_width = measure_text_width(&count);
    if width <= count_width + 4 {
        return Line::from(truncate_str(&count, width, "").to_string());
    }

    const MINIMUM_BAR_WIDTH: usize = 8;
    let status_width = width.saturating_sub(count_width + 4 + MINIMUM_BAR_WIDTH);
    let status = truncate_str(&status, status_width, "…");
    let bar_width = width - measure_text_width(&status) - count_width - 4;
    let accent = package_accent_style(appearance);
    let mut spans = vec![Span::styled(status.into_owned(), accent), Span::raw(" [")];
    spans.extend(render_package_bar_track(
        appearance,
        bar_width,
        completed,
        state.total,
        animation_frame,
        matches!(state.phase, PackageProgressPhase::Finished(_)),
    ));
    spans.push(Span::raw("] "));
    spans.push(Span::styled(count, accent));
    Line::from(spans)
}

fn render_package_preparation(
    width: usize,
    appearance: PackageAppearance,
    animation_frame: usize,
) -> Line<'static> {
    let status = "Preparing";
    if width <= status.len() + 3 {
        return Line::from(truncate_str(status, width, "").to_string());
    }

    let bar_width = width - status.len() - 3;
    let accent = package_accent_style(appearance);
    let mut spans = vec![Span::styled(status, accent), Span::raw(" [")];
    spans.extend(render_package_activity_bar(appearance, bar_width, animation_frame));
    spans.push(Span::raw("]"));
    Line::from(spans)
}

fn render_package_activity_bar(
    appearance: PackageAppearance,
    width: usize,
    animation_frame: usize,
) -> Vec<Span<'static>> {
    const PULSE_WIDTH: usize = 5;
    if matches!(appearance, PackageAppearance::Plain) {
        let mut bar = vec!['-'; width];
        if width > 0 {
            bar[animation_frame % width] = '>';
        }
        return vec![Span::raw(bar.into_iter().collect::<String>())];
    }

    let spans = (0..width).map(|index| {
        let cycle = width + PULSE_WIDTH;
        let highlight = animation_frame % cycle;
        let distance = index + PULSE_WIDTH;
        let distance = distance.abs_diff(highlight).min(cycle - distance.abs_diff(highlight));
        let character = if distance < PULSE_WIDTH { "⣿" } else { "⠀" };
        Span::styled(
            character,
            package_bar_style(appearance, index, width, width, animation_frame, false),
        )
    });
    spans.collect_vec()
}

fn render_package_bar_track(
    appearance: PackageAppearance,
    width: usize,
    completed: usize,
    total: usize,
    animation_frame: usize,
    finished: bool,
) -> Vec<Span<'static>> {
    let filled_eighths = if total == 0 { width * 8 } else { width * 8 * completed / total };
    if matches!(appearance, PackageAppearance::Plain) {
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
        let cell_eighths = filled_eighths.saturating_sub(index * 8).min(8);
        Span::styled(
            BRAILLE_FILL[cell_eighths],
            package_bar_style(appearance, index, width, filled, animation_frame, finished),
        )
    });
    spans.collect_vec()
}

fn package_bar_style(
    appearance: PackageAppearance,
    index: usize,
    width: usize,
    filled: usize,
    animation_frame: usize,
    finished: bool,
) -> RatatuiStyle {
    let style = RatatuiStyle::default();
    match appearance {
        PackageAppearance::TrueColor { .. } if index < filled => {
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
                let highlight = animation_frame % filled;
                let distance = index.abs_diff(highlight);
                let brightness = match distance {
                    0 => 0.35,
                    1 => 0.2,
                    2 => 0.08,
                    _ => 0.0,
                };
                let gold = (248, 216, 120);
                let tint = |channel: u8, gold: u8| {
                    (channel as f32 + (gold as f32 - channel as f32) * brightness).round() as u8
                };
                color = (tint(color.0, gold.0), tint(color.1, gold.1), tint(color.2, gold.2));
            }
            style.fg(RatatuiColor::Rgb(color.0, color.1, color.2))
        }
        PackageAppearance::TrueColor { foreground, .. } if index == filled => style
            .fg(RatatuiColor::Rgb(foreground.0, foreground.1, foreground.2))
            .add_modifier(Modifier::BOLD),
        PackageAppearance::TrueColor { foreground, background } => {
            let blend = |foreground: u8, background: u8| {
                (background as f32 + (foreground as f32 - background as f32) * 0.16).round() as u8
            };
            style.fg(RatatuiColor::Rgb(
                blend(foreground.0, background.0),
                blend(foreground.1, background.1),
                blend(foreground.2, background.2),
            ))
        }
        PackageAppearance::Ansi if index < filled => {
            let color = if index < filled / 3 {
                RatatuiColor::Blue
            } else if index < filled * 2 / 3 {
                RatatuiColor::Cyan
            } else {
                RatatuiColor::Green
            };
            let style = style.fg(color);
            if !finished && index == animation_frame % filled.max(1) {
                style.add_modifier(Modifier::BOLD)
            } else {
                style
            }
        }
        PackageAppearance::Ansi if index == filled => {
            style.fg(RatatuiColor::White).add_modifier(Modifier::BOLD)
        }
        PackageAppearance::Ansi => style.fg(RatatuiColor::DarkGray),
        PackageAppearance::Plain => unreachable!("plain package bars are rendered without styles"),
    }
}

fn package_history_style(appearance: PackageAppearance, row: usize) -> RatatuiStyle {
    let style = RatatuiStyle::default();
    match appearance {
        PackageAppearance::TrueColor { foreground, background } => {
            let opacity = 0.18 + 0.82 * row as f32 / (PACKAGE_HISTORY_LENGTH - 1) as f32;
            let blend = |foreground: u8, background: u8| {
                (background as f32 + (foreground as f32 - background as f32) * opacity).round()
                    as u8
            };
            style.fg(RatatuiColor::Rgb(
                blend(foreground.0, background.0),
                blend(foreground.1, background.1),
                blend(foreground.2, background.2),
            ))
        }
        PackageAppearance::Ansi if row + 1 == PACKAGE_HISTORY_LENGTH => {
            style.fg(RatatuiColor::Green).add_modifier(Modifier::BOLD)
        }
        PackageAppearance::Ansi if row * 2 < PACKAGE_HISTORY_LENGTH => {
            style.add_modifier(Modifier::DIM)
        }
        PackageAppearance::Ansi | PackageAppearance::Plain => style,
    }
}

fn package_accent_style(appearance: PackageAppearance) -> RatatuiStyle {
    match appearance {
        PackageAppearance::Plain => RatatuiStyle::default(),
        PackageAppearance::TrueColor { .. } | PackageAppearance::Ansi => {
            RatatuiStyle::default().fg(RatatuiColor::Cyan)
        }
    }
}

fn sanitize(name: &str) -> String {
    let characters =
        name.chars().map(|character| if character.is_control() { '�' } else { character });
    characters.collect()
}

pub(crate) fn report_completion(elapsed: Duration) {
    if !io::stderr().is_terminal() {
        return;
    }

    let label = format!("{:>12}", "Finished");
    let style = Style::new().green().bold();
    let jobs = rayon::current_num_threads();
    let job_label = if jobs == 1 { "job" } else { "jobs" };
    eprintln!("{} in {elapsed:.2?} via {jobs} {job_label}", style.apply_to(label));
}

fn configure(progress: &ProgressBar, phase: &'static str) {
    let started = Instant::now();
    let characters = phase.chars().collect::<Vec<_>>();
    let theme_mode = *TERMINAL_THEME_MODE
        .get_or_init(|| terminal_colorsaurus::theme_mode(QueryOptions::default()).ok());
    let cycle = characters.len() + 4;
    let total = progress.length().unwrap_or_default();
    let count_width = total.to_string().len().max(4);
    let statistics_width = count_width * 2 + 2;
    let bar_width = CARGO_PROGRESS_REGION_WIDTH
        .saturating_sub(CARGO_PROGRESS_FIXED_OVERHEAD + statistics_width)
        .max(1);
    let phase_text = move |state: &ProgressState, writer: &mut dyn fmt::Write| {
        if state.is_finished() {
            let style = phase_style(theme_mode, 255);
            write!(writer, "{}", style.apply_to(phase))
                .expect("writing to a formatter cannot fail");
            return;
        }

        let frame =
            (started.elapsed().as_millis() / SHIMMER_FRAME_INTERVAL.as_millis()) as usize % cycle;
        let highlight = frame as isize - 2;
        for (index, character) in characters.iter().enumerate() {
            let distance = (index as isize - highlight).unsigned_abs();
            let intensity = match distance {
                0 => 255,
                1 => 210,
                2 => 155,
                3 => 115,
                _ => 90,
            };
            let style = phase_style(theme_mode, intensity);
            write!(writer, "{}", style.apply_to(character))
                .expect("writing to a formatter cannot fail");
        }
    };
    let template =
        format!("{{phase:>12}} [{{bar:{bar_width}.cyan/blue}}] {{pos:>4}}/{{len:4}} {{msg}}");
    let style = ProgressStyle::with_template(&template)
        .expect("progress bar template is valid")
        .with_key("phase", phase_text)
        .progress_chars("=> ");
    progress.set_style(style);
    progress.enable_steady_tick(SHIMMER_FRAME_INTERVAL);
}

fn phase_style(theme_mode: Option<ThemeMode>, intensity: u8) -> Style {
    let style = Style::new().for_stderr();
    let intensity = match theme_mode {
        Some(ThemeMode::Dark) => intensity,
        Some(ThemeMode::Light) => 255 - intensity,
        None => return style,
    };
    style.fg(Color::TrueColor(intensity, intensity, intensity))
}

#[cfg(test)]
mod tests {
    use super::*;
    use itertools::Itertools;

    fn package_state(names: &[&str], total: usize) -> PackageProgressState {
        let completed = names.iter().map(|name| CompletedPackage {
            name: (*name).to_owned(),
            elapsed: Duration::from_millis(125),
        });
        let completed = completed.collect();
        PackageProgressState {
            total,
            completed_count: names.len(),
            completed,
            phase: PackageProgressPhase::Compiling,
        }
    }

    fn render_state(
        state: &PackageProgressState,
        width: u16,
        appearance: PackageAppearance,
    ) -> Buffer {
        render_state_at_height(state, width, PACKAGE_REGION_HEIGHT, appearance)
    }

    fn render_state_at_height(
        state: &PackageProgressState,
        width: u16,
        height: u16,
        appearance: PackageAppearance,
    ) -> Buffer {
        let area = Rect::new(0, 0, width, height);
        let mut buffer = Buffer::empty(area);
        PackageView { state, appearance, animation_frame: 0 }.render(area, &mut buffer);
        buffer
    }

    fn row_text(buffer: &Buffer, row: u16) -> String {
        let characters = (0..buffer.area.width).map(|column| {
            buffer
                .cell((column, row))
                .expect("invariant violated: rendered package row has no cell")
                .symbol()
        });
        characters.collect()
    }

    #[test]
    fn package_frame_has_ten_history_rows_and_two_status_rows() {
        let names = ["one", "two", "three"];
        let state = package_state(&names, 12);

        let buffer = render_state(&state, 60, PackageAppearance::Plain);

        assert!(row_text(&buffer, 7).starts_with("one  "));
        assert!(row_text(&buffer, 9).starts_with("three  "));
        assert!(row_text(&buffer, 10).chars().all(|character| character == '─'));
        assert!(row_text(&buffer, 11).contains("3/12"));
    }

    #[test]
    fn package_frame_transitions_from_preparation_to_compilation() {
        let progress = packages(false, false);
        let state = progress.state.lock().unwrap();
        let preparing = render_state(&state, 60, PackageAppearance::Plain);
        drop(state);

        progress.begin_compilation(12);
        let state = progress.state.lock().unwrap();
        let compiling = render_state(&state, 60, PackageAppearance::Plain);

        assert!(row_text(&preparing, 0).starts_with("Preparing ["));
        assert!(!row_text(&preparing, 0).contains("0/0"));
        assert!((1..PACKAGE_REGION_HEIGHT).all(|row| row_text(&preparing, row).trim().is_empty()));
        assert!(row_text(&compiling, 11).starts_with("Compiling ["));
        assert!(row_text(&compiling, 11).ends_with("0/12"));
    }

    #[test]
    fn package_frame_caps_width_and_aligns_timing_digits() {
        let state = PackageProgressState {
            total: 2,
            completed_count: 2,
            completed: VecDeque::from([
                CompletedPackage {
                    name: "short".to_owned(),
                    elapsed: Duration::from_micros(9_500),
                },
                CompletedPackage {
                    name: "a-much-longer-package-name".to_owned(),
                    elapsed: Duration::from_micros(123_450),
                },
            ]),
            phase: PackageProgressPhase::Compiling,
        };

        let buffer = render_state(&state, PACKAGE_REGION_WIDTH as u16, PackageAppearance::Plain);
        let short = row_text(&buffer, 8);
        let long = row_text(&buffer, 9);

        assert_eq!(measure_text_width(&short), PACKAGE_REGION_WIDTH);
        assert_eq!(measure_text_width(&long), PACKAGE_REGION_WIDTH);
        assert_eq!(short.find('.'), long.find('.'));
        assert!(short.ends_with("9.50 ms"));
        assert!(long.ends_with("123.45 ms"));
    }

    #[test]
    fn package_frame_keeps_only_the_latest_ten_completions() {
        let names = (1..=12).map(|number| format!("package-{number}")).collect_vec();
        let progress = packages(false, false);
        progress.begin_compilation(names.len());
        for name in &names {
            progress.complete(name, Duration::from_millis(125));
        }
        let state = progress.state.lock().unwrap();

        let buffer = render_state(&state, 60, PackageAppearance::Plain);
        let frame = (0..PACKAGE_HISTORY_LENGTH as u16).map(|row| row_text(&buffer, row)).join("\n");

        assert!(!frame.contains("package-1  "));
        assert!(!frame.contains("package-2  "));
        assert!(frame.contains("package-3  "));
        assert!(frame.contains("package-12  "));
    }

    #[test]
    fn package_frame_preserves_progress_status() {
        let state = package_state(&["completed"], 12);

        let buffer = render_state(&state, 60, PackageAppearance::Plain);
        let progress = row_text(&buffer, 11);

        assert!(progress.contains("1/12"));
        assert!(progress.contains('['));
        assert!(progress.contains(']'));
        assert_eq!(measure_text_width(&progress), 60);
    }

    #[test]
    fn package_bar_does_not_exceed_extremely_narrow_terminals() {
        let state = package_state(&[], 12);

        for width in 1..=8 {
            let buffer = render_state(&state, width, PackageAppearance::Plain);
            assert_eq!(measure_text_width(&row_text(&buffer, 11)), width as usize);
        }
    }

    #[test]
    fn short_terminal_keeps_status_visible() {
        let state = package_state(&["completed"], 12);

        let buffer = render_state_at_height(&state, 40, 4, PackageAppearance::Plain);

        assert!(row_text(&buffer, 3).contains("1/12"));
        assert!(!row_text(&buffer, 3).contains("completed"));
    }

    #[test]
    fn package_frame_sanitizes_and_truncates_names_without_wrapping() {
        let state = package_state(&["a\nvery-long-package-name"], 1);

        let buffer = render_state(&state, 18, PackageAppearance::Plain);

        let package = row_text(&buffer, 9);
        assert!(package.contains('�'));
        assert!(package.contains('…'));
        assert!(package.ends_with("125.00 ms"));
    }

    #[test]
    fn package_history_uses_fixed_row_opacity_and_shifts_on_completion() {
        let appearance =
            PackageAppearance::TrueColor { foreground: (240, 220, 200), background: (20, 30, 40) };
        let mut state = package_state(&["first"], 2);

        let first_frame = render_state(&state, 60, appearance);
        let first_style = first_frame.cell((0, 9)).unwrap().style();

        state.completed.push_back(CompletedPackage {
            name: "second".to_owned(),
            elapsed: Duration::from_millis(125),
        });
        state.completed_count += 1;
        let second_frame = render_state(&state, 60, appearance);
        let shifted_style = second_frame.cell((0, 8)).unwrap().style();
        let newest_style = second_frame.cell((0, 9)).unwrap().style();

        assert_eq!(first_style.fg, Some(RatatuiColor::Rgb(240, 220, 200)));
        assert_eq!(shifted_style.fg, Some(RatatuiColor::Rgb(220, 203, 185)));
        assert_eq!(newest_style, first_style);
    }

    #[test]
    fn true_color_history_fades_from_oldest_to_newest_row() {
        let appearance =
            PackageAppearance::TrueColor { foreground: (240, 220, 200), background: (20, 30, 40) };

        assert_eq!(package_history_style(appearance, 0).fg, Some(RatatuiColor::Rgb(60, 64, 69)));
        assert_eq!(package_history_style(appearance, 9).fg, Some(RatatuiColor::Rgb(240, 220, 200)));
    }

    #[test]
    fn true_color_package_bar_uses_iris_costume_gradient() {
        let appearance =
            PackageAppearance::TrueColor { foreground: (240, 240, 240), background: (20, 20, 20) };

        let bar = render_package_bar_track(appearance, 7, 7, 7, 0, true);

        assert_eq!(bar.first().unwrap().style.fg, Some(RatatuiColor::Rgb(240, 128, 136)));
        assert_eq!(bar[2].style.fg, Some(RatatuiColor::Rgb(248, 152, 184)));
        assert_eq!(bar[4].style.fg, Some(RatatuiColor::Rgb(184, 136, 224)));
        assert_eq!(bar.last().unwrap().style.fg, Some(RatatuiColor::Rgb(136, 72, 192)));
    }

    #[test]
    fn colored_package_bar_uses_braille_subcells() {
        let bar = render_package_bar_track(PackageAppearance::Ansi, 2, 1, 3, 0, false);

        assert_eq!(bar[0].content, "⣇");
        assert_eq!(bar[1].content, "⠀");
    }

    #[test]
    fn true_color_package_bar_animates_a_highlight() {
        let appearance =
            PackageAppearance::TrueColor { foreground: (240, 240, 240), background: (20, 20, 20) };
        let first = render_package_bar_track(appearance, 6, 3, 6, 0, false);
        let second = render_package_bar_track(appearance, 6, 3, 6, 1, false);

        assert_ne!(first[0].style.fg, second[0].style.fg);
        assert_ne!(first[1].style.fg, second[1].style.fg);
    }
}
