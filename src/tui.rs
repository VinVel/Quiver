use std::{
    io::{self, IsTerminal},
    sync::{Arc, OnceLock},
    time::Duration,
};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
};
use directories::BaseDirs;
use quiver_lib::{
    download::{DownloadEvent, DownloadRequest, DownloadService},
    presets::{self, DownloadPreset, DownloadPresetInput, PresetCommandPreview},
    settings::{DownloadInputSettings, SettingsStore, ThemePreferences},
    yt_dlp,
};
use ratatui_kit::{
    components::tui_widget_list::{ListBuilder, ListState, ListView},
    ratatui::{
        self, Frame,
        layout::{Constraint, Direction, Layout, Rect},
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Block, Borders, Paragraph, Wrap},
    },
};
use tokio::{sync::mpsc, task::JoinHandle};
use tui_input::{Input, backend::crossterm::EventHandler};

const THEMES: [&str; 4] = ["crystal", "ocean", "forest", "sun"];
const OUTPUT_LIMIT: usize = 100_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Focus {
    Url,
    Presets,
    Directory,
    Cookies,
    Advanced,
    Appearance,
    About,
    Preview,
    Output,
}

impl Focus {
    const ALL: [Self; 9] = [
        Self::Url,
        Self::Presets,
        Self::Directory,
        Self::Cookies,
        Self::Advanced,
        Self::Appearance,
        Self::About,
        Self::Preview,
        Self::Output,
    ];
    fn next(self, backward: bool) -> Self {
        let index = Self::ALL.iter().position(|item| *item == self).unwrap_or(0);
        Self::ALL[(index + if backward { Self::ALL.len() - 1 } else { 1 }) % Self::ALL.len()]
    }
    fn is_input(self) -> bool {
        matches!(self, Self::Url | Self::Directory | Self::Cookies)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Overlay {
    None,
    Help,
    Licenses,
}

struct App {
    settings: SettingsStore,
    service: Arc<DownloadService>,
    presets: [DownloadPreset; 8],
    selected: usize,
    focus: Focus,
    url: Input,
    directory: Input,
    cookies: Input,
    theme: ThemePreferences,
    advanced: bool,
    preview: Option<PresetCommandPreview>,
    output: String,
    output_scroll: u16,
    preview_scroll: u16,
    output_list: ListState,
    overlay_list: ListState,
    overlay: Overlay,
    overlay_scroll: u16,
    overlay_max_scroll: u16,
    status: String,
    running: bool,
    events: Option<mpsc::UnboundedReceiver<DownloadEvent>>,
    task: Option<JoinHandle<()>>,
    quit: bool,
}

impl App {
    fn new(settings: SettingsStore) -> Self {
        let inputs = settings.download_inputs();
        let theme = settings.theme();
        Self {
            settings,
            service: Arc::new(DownloadService::default()),
            presets: presets::all_presets(),
            selected: 0,
            focus: Focus::Url,
            url: Input::default(),
            directory: Input::new(inputs.directory),
            cookies: Input::new(inputs.cookies_path),
            theme,
            advanced: false,
            preview: None,
            output: String::new(),
            output_scroll: 0,
            preview_scroll: 0,
            output_list: ListState::default(),
            overlay_list: ListState::default(),
            overlay: Overlay::None,
            overlay_scroll: 0,
            overlay_max_scroll: 0,
            status: "Paste a URL to preview the command, then press Ctrl+R to download.".into(),
            running: false,
            events: None,
            task: None,
            quit: false,
        }
    }

    fn save_inputs(&mut self) {
        let result = self.settings.set_download_inputs(DownloadInputSettings {
            directory: self.directory.value().to_owned(),
            cookies_path: self.cookies.value().to_owned(),
        });
        if let Err(error) = result {
            self.status = error.to_string();
        }
    }

    fn prepare(&mut self) {
        self.save_inputs();
        self.refresh_preview();
        if self.preview.is_some() {
            self.status = "Ready. Press Ctrl+R to download.".into();
        }
    }

    fn refresh_preview(&mut self) {
        let input = DownloadPresetInput {
            link: self.url.value().to_owned(),
            directory: Some(self.directory.value().to_owned()),
            cookies_path: Some(self.cookies.value().to_owned()),
        };
        match presets::command_preview(self.presets[self.selected].id, &input) {
            Ok(preview) => {
                self.preview = Some(preview);
                self.preview_scroll = 0;
            }
            Err(error) => {
                if !self.url.value().is_empty() {
                    self.status = error;
                }
                self.preview = None;
            }
        }
    }

    fn start(&mut self) {
        if self.running {
            self.status = "A download is already running.".into();
            return;
        }
        let Some(preview) = &self.preview else {
            self.status = "Enter a valid URL before downloading.".into();
            return;
        };
        let request = DownloadRequest {
            preset_id: preview.preset.id,
            link: self.url.value().to_owned(),
            args: preview.args.clone(),
            advanced_subtitle_pipeline: self.advanced,
            plugin_dirs: yt_dlp::candidate_plugin_dirs(None),
        };
        let (sender, receiver) = mpsc::unbounded_channel();
        let service = Arc::clone(&self.service);
        self.task = Some(tokio::spawn(async move {
            let _ = service.run(request, sender).await;
        }));
        self.events = Some(receiver);
        self.running = true;
        self.output.clear();
        self.output_scroll = 0;
        self.output_list = ListState::default();
        self.status = "Downloading... Press Ctrl+X to cancel.".into();
        self.focus = Focus::Output;
    }

    fn drain_events(&mut self) -> bool {
        let mut received = Vec::new();
        if let Some(receiver) = &mut self.events {
            while let Ok(event) = receiver.try_recv() {
                received.push(event);
            }
        }
        let mut changed = !received.is_empty();
        for event in received {
            match event {
                DownloadEvent::Output { chunk, .. } => {
                    self.output.push_str(&chunk.replace('\r', "\n"));
                    if self.output.len() > OUTPUT_LIMIT {
                        let cut = self.output.len() - OUTPUT_LIMIT;
                        let boundary = self.output.ceil_char_boundary(cut);
                        self.output.drain(..boundary);
                    }
                    self.output_scroll = u16::MAX;
                }
                DownloadEvent::Finished { success, exit_code } => {
                    self.running = false;
                    self.status = if success {
                        "Download complete.".into()
                    } else {
                        format!("Download failed (exit code {exit_code:?}).")
                    };
                }
                DownloadEvent::Failed(error) => {
                    self.running = false;
                    self.status = error;
                }
            }
        }
        if self.running
            && self
                .events
                .as_ref()
                .is_some_and(mpsc::UnboundedReceiver::is_closed)
        {
            self.running = false;
            self.status = "Download stopped unexpectedly.".into();
            changed = true;
        }
        changed
    }

    fn on_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        if self.overlay != Overlay::None {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => self.overlay = Overlay::None,
                KeyCode::Up => self.overlay_scroll = self.overlay_scroll.saturating_sub(1),
                KeyCode::Down => {
                    self.overlay_scroll = self
                        .overlay_scroll
                        .saturating_add(1)
                        .min(self.overlay_max_scroll);
                }
                KeyCode::PageUp => self.overlay_scroll = self.overlay_scroll.saturating_sub(10),
                KeyCode::PageDown => {
                    self.overlay_scroll = self
                        .overlay_scroll
                        .saturating_add(10)
                        .min(self.overlay_max_scroll);
                }
                _ => {}
            }
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            self.on_control_key(key.code);
            return;
        }
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                if matches!(self.focus, Focus::Directory | Focus::Cookies) {
                    self.save_inputs();
                }
                self.focus = self.focus.next(key.code == KeyCode::BackTab);
                return;
            }
            KeyCode::F(1) => {
                self.overlay = Overlay::Help;
                self.overlay_scroll = 0;
                return;
            }
            KeyCode::F(2) => {
                if matches!(self.focus, Focus::Directory | Focus::Cookies) {
                    self.save_inputs();
                }
                self.focus = Focus::Directory;
                return;
            }
            KeyCode::F(3) => {
                if matches!(self.focus, Focus::Directory | Focus::Cookies) {
                    self.save_inputs();
                }
                self.focus = Focus::Cookies;
                return;
            }
            _ => {}
        }
        if self.focus.is_input() {
            if key.code == KeyCode::Enter {
                if self.focus == Focus::Url {
                    self.prepare();
                } else {
                    self.save_inputs();
                    self.status = "Inputs saved.".into();
                }
            } else if self.input_mut().handle_event(&Event::Key(key)).is_some() {
                self.refresh_preview();
            }
            return;
        }
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => {
                self.overlay = Overlay::Help;
                self.overlay_scroll = 0;
            }
            KeyCode::Char('l') => {
                self.open_licenses();
            }
            KeyCode::Char('p') => self.prepare(),
            KeyCode::Char('r') => self.start(),
            KeyCode::Char('c') | KeyCode::Esc if self.running => {
                self.service.cancel();
                self.status = "Cancelling download...".into();
            }
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::PageUp => self.scroll(-10),
            KeyCode::PageDown => self.scroll(10),
            KeyCode::Home => self.set_scroll(0),
            KeyCode::End => self.set_scroll(u16::MAX),
            KeyCode::Enter if self.focus == Focus::Presets => self.prepare(),
            KeyCode::Enter if self.focus == Focus::About => self.open_licenses(),
            KeyCode::Left if self.focus == Focus::Appearance => self.cycle_theme(-1),
            KeyCode::Right if self.focus == Focus::Appearance => self.cycle_theme(1),
            KeyCode::Char(' ') if self.focus == Focus::Appearance => self.cycle_mode(),
            KeyCode::Char(' ') if self.focus == Focus::Advanced => self.toggle_advanced(),
            _ => {}
        }
    }

    fn on_control_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('p') => self.prepare(),
            KeyCode::Char('r') => self.start(),
            KeyCode::Char('x') => {
                self.service.cancel();
                self.status = "Cancelling download...".into();
            }
            KeyCode::Char('c') => self.quit = true,
            KeyCode::Up => self.select_preset(-1),
            KeyCode::Down => self.select_preset(1),
            KeyCode::Left => self.cycle_theme(-1),
            KeyCode::Right => self.cycle_theme(1),
            KeyCode::Char(' ' | '\0') => self.toggle_advanced(),
            KeyCode::Char('l') => self.open_licenses(),
            _ => {}
        }
    }

    fn input_mut(&mut self) -> &mut Input {
        match self.focus {
            Focus::Url => &mut self.url,
            Focus::Directory => &mut self.directory,
            Focus::Cookies => &mut self.cookies,
            _ => unreachable!(),
        }
    }
    fn move_selection(&mut self, direction: i32) {
        match self.focus {
            Focus::Presets => self.select_preset(direction),
            Focus::Preview | Focus::Output => self.scroll(direction),
            _ => {}
        }
    }
    fn select_preset(&mut self, direction: i32) {
        let index = if direction < 0 {
            self.selected.saturating_sub(1)
        } else {
            (self.selected + 1).min(self.presets.len() - 1)
        };
        self.select_preset_index(index);
    }
    fn select_preset_index(&mut self, index: usize) {
        self.selected = index.min(self.presets.len() - 1);
        self.refresh_preview();
        if !presets::is_youtube_video_preset(self.presets[self.selected].id) {
            self.advanced = false;
        }
    }
    fn toggle_advanced(&mut self) {
        if presets::is_youtube_video_preset(self.presets[self.selected].id) {
            self.advanced = !self.advanced;
        } else {
            self.status = "Advanced subtitles are available for YouTube video presets.".into();
        }
    }
    fn open_licenses(&mut self) {
        self.overlay = Overlay::Licenses;
        self.overlay_scroll = 0;
    }
    fn scroll(&mut self, amount: i32) {
        let value = if self.focus == Focus::Preview {
            &mut self.preview_scroll
        } else {
            &mut self.output_scroll
        };
        Self::scroll_value(value, amount);
    }
    fn scroll_value(value: &mut u16, amount: i32) {
        *value = if amount < 0 {
            value.saturating_sub(u16::try_from(amount.unsigned_abs()).unwrap_or(u16::MAX))
        } else {
            value.saturating_add(u16::try_from(amount).unwrap_or(u16::MAX))
        };
    }
    fn set_scroll(&mut self, value: u16) {
        match self.focus {
            Focus::Preview => self.preview_scroll = value,
            Focus::Output => self.output_scroll = value,
            _ => {}
        }
    }
    fn cycle_theme(&mut self, direction: i32) {
        let index = THEMES
            .iter()
            .position(|item| *item == self.theme.preset)
            .unwrap_or(0);
        let next = (index + if direction < 0 { THEMES.len() - 1 } else { 1 }) % THEMES.len();
        self.set_theme(next);
    }
    fn set_theme(&mut self, index: usize) {
        let supported = THEMES.map(str::to_owned);
        match self.settings.set_theme_preset(
            THEMES[index.min(THEMES.len() - 1)].into(),
            &supported,
            "crystal",
        ) {
            Ok(preset) => self.theme.preset = preset,
            Err(error) => self.status = error.to_string(),
        }
    }
    fn cycle_mode(&mut self) {
        let next = match self.theme.mode.as_str() {
            "system" => "dark",
            "dark" => "light",
            _ => "system",
        };
        match self.settings.set_theme_mode(next.into()) {
            Ok(mode) => self.theme.mode = mode,
            Err(error) => self.status = error.to_string(),
        }
    }

    fn on_mouse(&mut self, event: MouseEvent, area: Rect) {
        if self.overlay != Overlay::None {
            match event.kind {
                MouseEventKind::ScrollUp => {
                    self.overlay_scroll = self.overlay_scroll.saturating_sub(3);
                }
                MouseEventKind::ScrollDown => {
                    self.overlay_scroll = self
                        .overlay_scroll
                        .saturating_add(3)
                        .min(self.overlay_max_scroll);
                }
                MouseEventKind::Down(MouseButton::Left) => self.overlay = Overlay::None,
                _ => {}
            }
            return;
        }
        let regions = screen_regions(area, self.focus);
        if matches!(
            event.kind,
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
        ) {
            let amount = if event.kind == MouseEventKind::ScrollUp {
                -3
            } else {
                3
            };
            if contains(regions.preview, event.column, event.row) {
                Self::scroll_value(&mut self.preview_scroll, amount);
            } else if contains(regions.output, event.column, event.row) {
                Self::scroll_value(&mut self.output_scroll, amount);
            }
            return;
        }
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        if matches!(self.focus, Focus::Directory | Focus::Cookies) {
            self.save_inputs();
        }
        if contains(regions.url, event.column, event.row) {
            self.focus = Focus::Url;
            return;
        }
        for (index, rect) in preset_cards(regions.presets, self.selected) {
            if contains(rect, event.column, event.row) {
                self.select_preset_index(index);
                self.focus = Focus::Presets;
                return;
            }
        }
        if contains(regions.advanced, event.column, event.row) {
            self.focus = Focus::Advanced;
            self.toggle_advanced();
            return;
        }
        let rows = input_rows(regions.inputs);
        if contains(rows[1], event.column, event.row) {
            self.focus = Focus::Directory;
            return;
        }
        if contains(rows[4], event.column, event.row) {
            self.focus = Focus::Cookies;
            return;
        }
        for (index, rect) in theme_cards(regions.appearance).into_iter().enumerate() {
            if contains(rect, event.column, event.row) {
                self.focus = Focus::Appearance;
                self.set_theme(index);
                return;
            }
        }
        if contains(
            appearance_mode_row(regions.appearance),
            event.column,
            event.row,
        ) {
            self.focus = Focus::Appearance;
            self.cycle_mode();
            return;
        }
        if contains(regions.appearance, event.column, event.row) {
            self.focus = Focus::Appearance;
            return;
        }
        if contains(regions.about, event.column, event.row) {
            self.focus = Focus::About;
            self.open_licenses();
        }
    }

    fn refresh_overlay_bounds(&mut self, _screen: Rect) {
        if self.overlay == Overlay::None {
            self.overlay_scroll = 0;
            self.overlay_list = ListState::default();
            return;
        }
        self.overlay_max_scroll =
            u16::try_from(overlay_lines(self.overlay).len().saturating_sub(1)).unwrap_or(u16::MAX);
        self.overlay_scroll = self.overlay_scroll.min(self.overlay_max_scroll);
    }
}

struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), DisableMouseCapture);
        ratatui::restore();
    }
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("an interactive terminal is required; use --help for usage".into());
    }
    let base = BaseDirs::new().ok_or("could not determine the configuration directory")?;
    let settings = SettingsStore::new(base.config_dir().join("net.velcore.quiver"));
    let mut app = App::new(settings);
    let mut terminal = ratatui::init();
    let _guard = TerminalGuard;
    execute!(io::stdout(), EnableMouseCapture)?;
    let mut needs_draw = true;
    let (width, height) = crossterm::terminal::size()?;
    let mut screen = Rect::new(0, 0, width, height);
    while !app.quit {
        needs_draw |= app.drain_events();
        if needs_draw {
            terminal.draw(|frame| draw(frame, &mut app))?;
            needs_draw = false;
        }
        if event::poll(Duration::from_millis(50))? {
            // Drain a burst before drawing: a fast mouse wheel can queue many events.
            for _ in 0..256 {
                match event::read()? {
                    Event::Key(key) => {
                        app.on_key(key);
                        needs_draw = true;
                    }
                    Event::Resize(width, height) => {
                        screen = Rect::new(0, 0, width, height);
                        needs_draw = true;
                    }
                    Event::Mouse(mouse)
                        if matches!(
                            mouse.kind,
                            MouseEventKind::ScrollUp
                                | MouseEventKind::ScrollDown
                                | MouseEventKind::Down(MouseButton::Left)
                        ) =>
                    {
                        app.on_mouse(mouse, screen);
                        needs_draw = true;
                    }
                    _ => {}
                }
                app.refresh_overlay_bounds(screen);
                if app.quit || !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
        tokio::task::yield_now().await;
    }
    app.save_inputs();
    app.service.stop();
    if let Some(task) = app.task.take() {
        let _ = tokio::time::timeout(Duration::from_secs(2), task).await;
    }
    Ok(())
}

fn accent(app: &App) -> Color {
    if app.theme.mode == "light" {
        return match app.theme.preset.as_str() {
            "ocean" => Color::Rgb(27, 83, 145),
            "forest" => Color::Rgb(49, 94, 36),
            "sun" => Color::Rgb(121, 78, 0),
            _ => Color::Rgb(94, 55, 132),
        };
    }
    match app.theme.preset.as_str() {
        "ocean" => Color::Cyan,
        "forest" => Color::LightGreen,
        "sun" => Color::Yellow,
        _ => Color::Magenta,
    }
}

fn selection_background(app: &App) -> Color {
    if app.theme.mode == "light" {
        match app.theme.preset.as_str() {
            "ocean" => Color::Rgb(146, 197, 231),
            "forest" => Color::Rgb(172, 210, 147),
            "sun" => Color::Rgb(237, 207, 114),
            _ => Color::Rgb(194, 172, 220),
        }
    } else {
        accent(app)
    }
}

fn muted(app: &App) -> Color {
    if app.theme.mode == "light" {
        Color::Rgb(70, 70, 70)
    } else {
        Color::Gray
    }
}

fn subtle(app: &App) -> Color {
    if app.theme.mode == "light" {
        Color::Rgb(90, 90, 90)
    } else {
        Color::DarkGray
    }
}

fn panel(title: &str, focused: bool, color: Color, muted_border: Color) -> Block<'_> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(if focused {
            Style::default().fg(color)
        } else {
            Style::default().fg(muted_border)
        })
}

#[derive(Clone, Copy)]
struct ScreenRegions {
    url: Rect,
    presets: Rect,
    advanced: Rect,
    inputs: Rect,
    appearance: Rect,
    about: Rect,
    preview: Rect,
    output: Rect,
    footer: Rect,
}

fn screen_regions(area: Rect, focus: Focus) -> ScreenRegions {
    let page = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .split(area);
    let empty = Rect::default();
    let mut regions = ScreenRegions {
        url: page[0],
        presets: empty,
        advanced: empty,
        inputs: empty,
        appearance: empty,
        about: empty,
        preview: empty,
        output: empty,
        footer: page[2],
    };
    if page[1].width >= 112 {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30),
                Constraint::Percentage(34),
                Constraint::Percentage(36),
            ])
            .split(page[1]);
        let left = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(3)])
            .split(columns[0]);
        regions.presets = left[0];
        regions.advanced = left[1];
        let center = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(47),
                Constraint::Percentage(36),
                Constraint::Percentage(17),
            ])
            .split(columns[1]);
        regions.inputs = center[0];
        regions.appearance = center[1];
        regions.about = center[2];
        regions.preview = Rect {
            height: regions.inputs.height,
            ..columns[2]
        };
        regions.output = Rect {
            y: regions.preview.bottom(),
            height: columns[2].height.saturating_sub(regions.preview.height),
            ..columns[2]
        };
    } else {
        match focus {
            Focus::Url | Focus::Presets | Focus::Advanced => {
                let left = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(6), Constraint::Length(3)])
                    .split(page[1]);
                regions.presets = left[0];
                regions.advanced = left[1];
            }
            Focus::Directory | Focus::Cookies => regions.inputs = page[1],
            Focus::Appearance => regions.appearance = page[1],
            Focus::About => regions.about = page[1],
            Focus::Preview => regions.preview = page[1],
            Focus::Output => regions.output = page[1],
        }
    }
    regions
}

fn contains(rect: Rect, column: u16, row: u16) -> bool {
    column >= rect.x && column < rect.right() && row >= rect.y && row < rect.bottom()
}

fn input_rows(area: Rect) -> [Rect; 6] {
    let inner = area.inner(ratatui::layout::Margin {
        horizontal: 2,
        vertical: 1,
    });
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(inner);
    [rows[0], rows[1], rows[2], rows[3], rows[4], rows[5]]
}

fn preset_cards(area: Rect, selected: usize) -> Vec<(usize, Rect)> {
    let inner = area.inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 1,
    });
    let visible = usize::from((inner.height / 3).max(1)).min(8);
    let start = selected.saturating_sub(visible.saturating_sub(1));
    let visible_height = u16::try_from(visible).unwrap_or(1);
    let card_height = inner.height / visible_height;
    let extra = inner.height % visible_height;
    (start..(start + visible).min(8))
        .enumerate()
        .map(|(position, index)| {
            let position = u16::try_from(position).unwrap_or(0);
            let y = inner.y + position * card_height + position.min(extra);
            let height = card_height + u16::from(position < extra);
            (
                index,
                Rect {
                    x: inner.x,
                    y,
                    width: inner.width,
                    height,
                },
            )
        })
        .collect()
}

fn theme_cards(area: Rect) -> [Rect; 4] {
    let inner = area.inner(ratatui::layout::Margin {
        horizontal: 2,
        vertical: 2,
    });
    let band = Rect {
        x: inner.x,
        y: inner.y.saturating_add(2),
        width: inner.width,
        height: inner.height.saturating_sub(3).min(5),
    };
    let parts = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25); 4])
        .split(band);
    [parts[0], parts[1], parts[2], parts[3]]
}

fn appearance_mode_row(area: Rect) -> Rect {
    let inner = area.inner(ratatui::layout::Margin {
        horizontal: 2,
        vertical: 2,
    });
    Rect {
        x: inner.x,
        y: inner
            .y
            .saturating_add(inner.height.saturating_sub(1).min(8)),
        width: inner.width,
        height: 1,
    }
}

fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    let background = match app.theme.mode.as_str() {
        "light" => Style::default().fg(Color::Black).bg(Color::White),
        "dark" => Style::default().fg(Color::White).bg(Color::Rgb(20, 23, 20)),
        _ => Style::default().fg(Color::Reset).bg(Color::Reset),
    };
    frame.render_widget(Block::default().style(background), area);
    if area.width < 50 || area.height < 12 {
        frame.render_widget(
            Paragraph::new("Quiver needs a terminal at least 50 columns wide and 12 rows high."),
            area,
        );
        return;
    }
    let color = accent(app);
    let regions = screen_regions(area, app.focus);
    draw_url(frame, app, regions.url, color);
    if regions.presets.width > 0 {
        draw_presets(frame, app, regions.presets, color);
        draw_advanced(frame, app, regions.advanced, color);
    }
    if regions.inputs.width > 0 {
        draw_inputs(frame, app, regions.inputs, color);
    }
    if regions.appearance.width > 0 {
        draw_appearance(frame, app, regions.appearance, color);
    }
    if regions.about.width > 0 {
        draw_about(frame, app, regions.about, color);
    }
    if regions.preview.width > 0 {
        draw_preview(frame, app, regions.preview, color);
    }
    if regions.output.width > 0 {
        draw_output(frame, app, regions.output, color);
    }
    let footer = format!(
        "{} | F2/F3: edit paths · Ctrl+↑↓: preset · Ctrl+←→: theme · Ctrl+Space: subs · F1: help",
        app.status
    );
    frame.render_widget(
        Paragraph::new(footer)
            .style(Style::default().fg(color))
            .wrap(Wrap { trim: true }),
        regions.footer,
    );
    if app.overlay != Overlay::None {
        draw_overlay(frame, app, color);
    }
}

fn draw_url(frame: &mut Frame<'_>, app: &App, area: Rect, color: Color) {
    let title = if app.focus == Focus::Url {
        "Quiver · URL (live preview)"
    } else {
        "Quiver · URL"
    };
    let scroll = if app.focus == Focus::Url {
        app.url
            .visual_scroll(usize::from(area.width.saturating_sub(2)))
    } else {
        0
    };
    frame.render_widget(
        Paragraph::new(if app.url.value().is_empty() {
            "Paste a URL to preview a download"
        } else {
            app.url.value()
        })
        .style(if app.url.value().is_empty() {
            Style::default().fg(muted(app))
        } else {
            Style::default()
        })
        .scroll((0, u16::try_from(scroll).unwrap_or(u16::MAX)))
        .block(panel(title, app.focus == Focus::Url, color, subtle(app))),
        area,
    );
    if app.focus == Focus::Url {
        frame.set_cursor_position((
            area.x
                + 1
                + u16::try_from(app.url.visual_cursor().saturating_sub(scroll))
                    .unwrap_or(u16::MAX)
                    .min(area.width.saturating_sub(2)),
            area.y + 1,
        ));
    }
}

fn draw_presets(frame: &mut Frame<'_>, app: &App, area: Rect, color: Color) {
    frame.render_widget(
        panel(
            "Presets · Ctrl+↑↓",
            app.focus == Focus::Presets,
            color,
            subtle(app),
        ),
        area,
    );
    for (index, rect) in preset_cards(area, app.selected) {
        let preset = &app.presets[index];
        let selected = index == app.selected;
        let card_style = if selected {
            Style::default()
                .fg(Color::Black)
                .bg(selection_background(app))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let media = if matches!(preset.media, presets::PresetMedia::Audio) {
            "Audio"
        } else {
            "Video"
        };
        let cookies = if preset.uses_cookies {
            "with Cookies"
        } else {
            "no Cookies"
        };
        let text = if rect.height >= 4 {
            vec![
                Line::from(format!(
                    "{} {}",
                    if selected { "▶" } else { " " },
                    preset.label
                )),
                Line::from(format!("  {media} · {cookies}")),
            ]
        } else {
            vec![Line::from(format!(
                "{} {}",
                if selected { "▶" } else { " " },
                preset.label
            ))]
        };
        frame.render_widget(
            Paragraph::new(text).style(card_style).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(if selected {
                        Style::default().fg(color)
                    } else {
                        Style::default().fg(subtle(app))
                    }),
            ),
            rect,
        );
    }
}

fn draw_advanced(frame: &mut Frame<'_>, app: &App, area: Rect, color: Color) {
    let available = presets::is_youtube_video_preset(app.presets[app.selected].id);
    let text = if available {
        format!(
            "[{}] Advanced subtitles (Ctrl+Space)",
            if app.advanced { "x" } else { " " }
        )
    } else {
        "Advanced subtitles: YouTube video only".into()
    };
    frame.render_widget(
        Paragraph::new(text)
            .style(if available {
                Style::default()
            } else {
                Style::default().fg(muted(app))
            })
            .block(panel(
                "Subtitle conversion",
                app.focus == Focus::Advanced,
                color,
                subtle(app),
            )),
        area,
    );
}

fn draw_inputs(frame: &mut Frame<'_>, app: &App, area: Rect, color: Color) {
    frame.render_widget(
        panel(
            "Inputs · click a field or use Tab",
            matches!(app.focus, Focus::Directory | Focus::Cookies),
            color,
            subtle(app),
        ),
        area,
    );
    let rows = input_rows(area);
    let directory_scroll = if app.focus == Focus::Directory {
        app.directory
            .visual_scroll(usize::from(rows[1].width.saturating_sub(2)))
    } else {
        0
    };
    let cookies_scroll = if app.focus == Focus::Cookies {
        app.cookies
            .visual_scroll(usize::from(rows[4].width.saturating_sub(2)))
    } else {
        0
    };
    frame.render_widget(
        Paragraph::new("SAVE DIRECTORY")
            .style(Style::default().fg(color).add_modifier(Modifier::BOLD)),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(app.directory.value())
            .scroll((0, u16::try_from(directory_scroll).unwrap_or(u16::MAX)))
            .block(panel(
                "edit · Enter saves",
                app.focus == Focus::Directory,
                color,
                subtle(app),
            )),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new("Defaults to ~/Downloads").style(Style::default().fg(muted(app))),
        rows[2],
    );
    frame.render_widget(
        Paragraph::new("COOKIES FILE")
            .style(Style::default().fg(color).add_modifier(Modifier::BOLD)),
        rows[3],
    );
    frame.render_widget(
        Paragraph::new(app.cookies.value())
            .scroll((0, u16::try_from(cookies_scroll).unwrap_or(u16::MAX)))
            .block(panel(
                "edit · Enter saves",
                app.focus == Focus::Cookies,
                color,
                subtle(app),
            )),
        rows[4],
    );
    frame.render_widget(
        Paragraph::new("Used for cookie-enabled presets").style(Style::default().fg(muted(app))),
        rows[5],
    );
    for (focus, rect, input, scroll) in [
        (Focus::Directory, rows[1], &app.directory, directory_scroll),
        (Focus::Cookies, rows[4], &app.cookies, cookies_scroll),
    ] {
        if app.focus == focus {
            frame.set_cursor_position((
                rect.x
                    + 1
                    + u16::try_from(input.visual_cursor().saturating_sub(scroll))
                        .unwrap_or(u16::MAX)
                        .min(rect.width.saturating_sub(2)),
                rect.y + 1,
            ));
        }
    }
}

fn draw_appearance(frame: &mut Frame<'_>, app: &App, area: Rect, color: Color) {
    frame.render_widget(
        panel(
            "Appearance · Ctrl+←→ theme",
            app.focus == Focus::Appearance,
            color,
            subtle(app),
        ),
        area,
    );
    let inner = area.inner(ratatui::layout::Margin {
        horizontal: 2,
        vertical: 2,
    });
    frame.render_widget(
        Paragraph::new("Choose a color scheme and display mode."),
        Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        },
    );
    for (index, rect) in theme_cards(area).into_iter().enumerate() {
        if rect.height == 0 || rect.width == 0 {
            continue;
        }
        let name = THEMES[index];
        let selected = app.theme.preset == name;
        let (background, foreground) = match name {
            "crystal" => (Color::Rgb(98, 81, 121), Color::White),
            "ocean" => (Color::Rgb(49, 85, 123), Color::White),
            "forest" => (Color::Rgb(70, 105, 43), Color::White),
            _ => (Color::Rgb(180, 143, 38), Color::Black),
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(capitalize(name)),
                Line::from(if selected { "● active" } else { "○" }),
            ])
            .style(Style::default().fg(foreground).bg(background))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(if selected {
                        Style::default().fg(Color::White)
                    } else {
                        Style::default().fg(subtle(app))
                    }),
            ),
            rect,
        );
    }
    let mode_area = appearance_mode_row(area);
    if mode_area.y < inner.bottom() {
        let mode = Line::from(vec![
            Span::raw("Display  "),
            mode_span("System", "system", &app.theme, color),
            Span::raw("  "),
            mode_span("Dark", "dark", &app.theme, color),
            Span::raw("  "),
            mode_span("Light", "light", &app.theme, color),
        ]);
        frame.render_widget(Paragraph::new(mode), mode_area);
    }
}

fn capitalize(name: &str) -> String {
    let mut characters = name.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + characters.as_str()
    })
}

fn mode_span<'a>(label: &'a str, mode: &str, current: &ThemePreferences, color: Color) -> Span<'a> {
    let style = if current.mode == mode {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    Span::styled(format!("[{label}]"), style)
}

fn draw_about(frame: &mut Frame<'_>, app: &App, area: Rect, color: Color) {
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(format!("Quiver v{}", env!("CARGO_PKG_VERSION"))),
            Line::from("A terminal interface for yt-dlp"),
            Line::from(Span::styled(
                "[ View licenses ↵ ]",
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )),
        ])
        .block(panel(
            "About",
            app.focus == Focus::About,
            color,
            subtle(app),
        )),
        area,
    );
}

fn draw_preview(frame: &mut Frame<'_>, app: &App, area: Rect, color: Color) {
    let text = command_preview_text(app);
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .scroll((app.preview_scroll, 0))
            .block(panel(
                "Command Preview · Ctrl+R run",
                false,
                color,
                subtle(app),
            )),
        area,
    );
}

fn command_preview_text(app: &App) -> String {
    app.preview.as_ref().map_or_else(
        || "Paste a URL to preview the command.".into(),
        |preview| {
            let mut command = String::from("yt-dlp");
            for arg in &preview.args {
                command.push(' ');
                command.push_str(&quote(arg));
            }
            command
        },
    )
}

fn quote(arg: &str) -> String {
    if arg
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "-._/:=?&".contains(character))
    {
        arg.into()
    } else {
        format!("\"{}\"", arg.replace('"', "\\\""))
    }
}

fn draw_output(frame: &mut Frame<'_>, app: &mut App, area: Rect, color: Color) {
    let text = if app.output.is_empty() {
        "yt-dlp output will appear here."
    } else {
        &app.output
    };
    let lines: Vec<&str> = text.lines().collect();
    let selected = usize::from(app.output_scroll).min(lines.len().saturating_sub(1));
    app.output_list.select(Some(selected));
    let border = subtle(app);
    render_virtual_text(
        frame,
        area,
        "Command Output · ↑↓ scroll",
        color,
        border,
        &lines,
        &mut app.output_list,
    );
}

// Ratatui Kit's VirtualList uses this same windowed list renderer. Building only
// rows in the viewport keeps large licenses and long process logs responsive.
fn render_virtual_text(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    color: Color,
    border: Color,
    lines: &[&str],
    state: &mut ListState,
) {
    let builder = ListBuilder::new(|context| {
        let line = lines[context.index];
        let width = usize::from(context.cross_axis_size.max(1));
        let height = u16::try_from(textwrap::wrap(line, width).len().max(1)).unwrap_or(u16::MAX);
        (Paragraph::new(line).wrap(Wrap { trim: false }), height)
    });
    let list = ListView::new(builder, lines.len())
        .infinite_scrolling(false)
        .block(panel(title, false, color, border));
    frame.render_stateful_widget(list, area, state);
}

fn overlay_area(outer: Rect) -> Rect {
    Rect {
        x: outer.x + 3,
        y: outer.y + 2,
        width: outer.width.saturating_sub(6),
        height: outer.height.saturating_sub(4),
    }
}

fn overlay_source(overlay: Overlay) -> Option<(&'static str, &'static str)> {
    match overlay {
        Overlay::Help => Some((
            "Help",
            "Tab / Shift+Tab  Move between fields and panels\nClick              Focus a field, preset, color, or button\nF2 / F3           Edit save directory / cookies file\nEnter              Prepare URL or preset; save paths\nCtrl+↑ / Ctrl+↓   Change preset from any field\nCtrl+← / Ctrl+→   Change color theme from any field\nCtrl+Space        Toggle advanced subtitles\nCtrl+P             Prepare command\nCtrl+R             Run previewed command\nCtrl+X             Cancel current download\nCtrl+L             Open licenses\nF1                 Open help\nCtrl+C             Quit\nEsc                Close this window\n\nYouTube video presets offer advanced subtitle conversion.\nThe preview updates as inputs change.",
        )),
        Overlay::Licenses => Some(("Licenses · Esc to close", license_text().as_str())),
        Overlay::None => None,
    }
}

fn overlay_lines(overlay: Overlay) -> &'static Vec<&'static str> {
    static HELP_LINES: OnceLock<Vec<&'static str>> = OnceLock::new();
    static LICENSE_LINES: OnceLock<Vec<&'static str>> = OnceLock::new();
    match overlay {
        Overlay::Help => HELP_LINES.get_or_init(|| {
            overlay_source(Overlay::Help)
                .expect("help text")
                .1
                .lines()
                .collect()
        }),
        Overlay::Licenses => LICENSE_LINES.get_or_init(|| license_text().lines().collect()),
        Overlay::None => HELP_LINES.get_or_init(Vec::new),
    }
}

fn draw_overlay(frame: &mut Frame<'_>, app: &mut App, color: Color) {
    let area = overlay_area(frame.area());
    frame.render_widget(ratatui::widgets::Clear, area);
    let Some((title, _)) = overlay_source(app.overlay) else {
        return;
    };
    let lines = overlay_lines(app.overlay);
    app.overlay_list.select(Some(
        usize::from(app.overlay_scroll).min(lines.len().saturating_sub(1)),
    ));
    let border = subtle(app);
    render_virtual_text(
        frame,
        area,
        title,
        color,
        border,
        lines,
        &mut app.overlay_list,
    );
}

fn license_text() -> &'static String {
    static TEXT: OnceLock<String> = OnceLock::new();
    TEXT.get_or_init(|| {
        let html = include_str!("../src-tauri/license.html");
        let body = html.split_once("<body>").map_or(html, |(_, body)| body);
        let mut text = String::new();
        let mut tag = String::new();
        let mut inside_tag = false;
        for character in body.chars() {
            match character {
                '<' => {
                    inside_tag = true;
                    tag.clear();
                }
                '>' if inside_tag => {
                    if matches!(
                        tag.as_str(),
                        "br" | "br/" | "/p" | "/h1" | "/h2" | "/h3" | "/li" | "/pre" | "/div"
                    ) {
                        text.push('\n');
                    }
                    inside_tag = false;
                }
                _ if inside_tag => tag.push(character),
                _ => text.push(character),
            }
        }
        text.replace("&quot;", "\"")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&#39;", "'")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn tiny_terminal_draw_does_not_panic() {
        let temp = tempfile::tempdir().expect("temporary settings");
        let mut app = App::new(SettingsStore::new(temp.path().to_path_buf()));
        let mut terminal = Terminal::new(TestBackend::new(40, 10)).expect("test terminal");
        terminal.draw(|frame| draw(frame, &mut app)).expect("draw");
    }

    #[test]
    fn compact_terminal_can_draw_each_panel_and_overlay() {
        let temp = tempfile::tempdir().expect("temporary settings");
        let mut app = App::new(SettingsStore::new(temp.path().to_path_buf()));
        app.selected = 7;
        let mut terminal = Terminal::new(TestBackend::new(80, 16)).expect("test terminal");
        for focus in Focus::ALL {
            app.focus = focus;
            terminal
                .draw(|frame| draw(frame, &mut app))
                .expect("draw panel");
        }
        app.overlay = Overlay::Help;
        terminal
            .draw(|frame| draw(frame, &mut app))
            .expect("draw help");
    }

    #[test]
    fn editing_updates_command_preview() {
        let temp = tempfile::tempdir().expect("temporary settings");
        let mut app = App::new(SettingsStore::new(temp.path().to_path_buf()));
        app.url = Input::new("https://example.com/video".into());
        app.prepare();
        assert!(app.preview.is_some());
        app.on_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        let preview = app.preview.as_ref().expect("live command preview");
        assert!(preview.args.iter().any(|arg| arg.ends_with("videox")));
    }

    #[test]
    fn preview_uses_yt_dlp_name_and_omits_subtitle_status() {
        let temp = tempfile::tempdir().expect("temporary settings");
        let mut app = App::new(SettingsStore::new(temp.path().to_path_buf()));
        app.url = Input::new("https://example.com/video".into());
        app.refresh_preview();
        app.advanced = true;
        let text = command_preview_text(&app);
        assert!(text.starts_with("yt-dlp "));
        assert!(!text.contains("quiver-yt-dlp"));
        assert!(!text.contains("Advanced subtitle conversion enabled"));
        let mode = app.theme.mode.clone();
        app.on_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::CONTROL));
        assert_eq!(app.theme.mode, mode);
    }

    #[test]
    fn control_shortcuts_work_while_url_is_focused() {
        let temp = tempfile::tempdir().expect("temporary settings");
        let mut app = App::new(SettingsStore::new(temp.path().to_path_buf()));
        app.url = Input::new("https://example.com/video".into());
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::CONTROL));
        assert_eq!(app.selected, 1);
        app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::CONTROL));
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::CONTROL));
        assert!(app.advanced);
        let old_theme = app.theme.preset.clone();
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL));
        assert_ne!(app.theme.preset, old_theme);
        assert_eq!(app.url.value(), "https://example.com/video");
    }

    #[test]
    fn click_focuses_editable_inputs_and_opens_licenses() {
        let temp = tempfile::tempdir().expect("temporary settings");
        let mut app = App::new(SettingsStore::new(temp.path().to_path_buf()));
        let screen = Rect::new(0, 0, 160, 45);
        let regions = screen_regions(screen, app.focus);
        let rows = input_rows(regions.inputs);
        let click = |rect: Rect| MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x + 1,
            row: rect.y + 1,
            modifiers: KeyModifiers::NONE,
        };
        app.on_mouse(click(rows[1]), screen);
        assert_eq!(app.focus, Focus::Directory);
        app.on_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(app.directory.value().ends_with('x'));
        app.on_mouse(click(rows[4]), screen);
        assert_eq!(app.focus, Focus::Cookies);
        app.on_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        assert!(app.cookies.value().ends_with('y'));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let saved = app.settings.download_inputs();
        assert!(saved.directory.ends_with('x'));
        assert!(saved.cookies_path.ends_with('y'));
        app.on_mouse(click(regions.about), screen);
        assert_eq!(app.overlay, Overlay::Licenses);
    }

    #[test]
    fn preview_and_output_clicks_do_not_take_focus() {
        let temp = tempfile::tempdir().expect("temporary settings");
        let mut app = App::new(SettingsStore::new(temp.path().to_path_buf()));
        let screen = Rect::new(0, 0, 160, 45);
        let regions = screen_regions(screen, app.focus);
        for rect in [regions.preview, regions.output] {
            app.on_mouse(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: rect.x + 2,
                    row: rect.y + 2,
                    modifiers: KeyModifiers::NONE,
                },
                screen,
            );
            assert_eq!(app.focus, Focus::Url);
        }
    }

    #[test]
    fn license_scroll_stops_at_document_end_and_reverses_immediately() {
        let temp = tempfile::tempdir().expect("temporary settings");
        let mut app = App::new(SettingsStore::new(temp.path().to_path_buf()));
        let screen = Rect::new(0, 0, 120, 35);
        app.open_licenses();
        app.refresh_overlay_bounds(screen);
        assert!(app.overlay_max_scroll > 0);
        app.overlay_scroll = u16::MAX;
        app.refresh_overlay_bounds(screen);
        assert_eq!(app.overlay_scroll, app.overlay_max_scroll);
        let wheel = |kind| MouseEvent {
            kind,
            column: 10,
            row: 10,
            modifiers: KeyModifiers::NONE,
        };
        app.on_mouse(wheel(MouseEventKind::ScrollDown), screen);
        assert_eq!(app.overlay_scroll, app.overlay_max_scroll);
        app.on_mouse(wheel(MouseEventKind::ScrollUp), screen);
        assert_eq!(app.overlay_scroll, app.overlay_max_scroll - 3);
    }

    #[test]
    fn light_mode_uses_dark_secondary_text() {
        let temp = tempfile::tempdir().expect("temporary settings");
        let mut app = App::new(SettingsStore::new(temp.path().to_path_buf()));
        app.theme.mode = "light".into();
        assert_eq!(muted(&app), Color::Rgb(70, 70, 70));
        assert_eq!(subtle(&app), Color::Rgb(90, 90, 90));
        assert_eq!(accent(&app), Color::Rgb(94, 55, 132));
    }
}
