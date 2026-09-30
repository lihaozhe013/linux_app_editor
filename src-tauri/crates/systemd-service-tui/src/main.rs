use std::io;
use std::path::PathBuf;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use ratatui_textarea::TextArea;
use systemd_service_core::unit_text::{self, FormProjection, ServiceForm};
use systemd_service_core::{
    self as core, Diagnostic, SaveOutcome, Scope, ServiceDocument, ServiceItem, Severity,
};

const FORM_FIELDS: [(&str, &str); 9] = [
    ("description", "Description"),
    ("service_type", "Service type"),
    ("exec_start", "ExecStart"),
    ("working_directory", "Working directory"),
    ("user", "User"),
    ("group", "Group"),
    ("restart", "Restart"),
    ("restart_sec", "Restart delay"),
    ("wanted_by", "WantedBy"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Form,
    Raw,
}

struct App {
    scope: Scope,
    services: Vec<ServiceItem>,
    filtered: Vec<usize>,
    selected: usize,
    filter: String,
    prompt: Option<Prompt>,
    document: Option<ServiceDocument>,
    raw: TextArea<'static>,
    mode: Mode,
    form: ServiceForm,
    locked: Vec<String>,
    dirty: Vec<String>,
    selected_field: usize,
    field_editor: Option<String>,
    diagnostics: Vec<Diagnostic>,
    message: String,
    diff: String,
    reviewed_contents: Option<String>,
    backup: bool,
    help: bool,
    show_source: bool,
    should_quit: bool,
    raw_trailing_newline: bool,
    raw_line_ending: &'static str,
    discard_armed: bool,
}

struct Prompt {
    label: &'static str,
    value: String,
    action: PromptAction,
}

#[derive(Clone, Copy)]
enum PromptAction {
    Search,
    Create,
}

impl App {
    fn new(scope: Scope, unit: Option<&str>) -> Result<Self, String> {
        let mut app = Self {
            scope,
            services: Vec::new(),
            filtered: Vec::new(),
            selected: 0,
            filter: String::new(),
            prompt: None,
            document: None,
            raw: TextArea::default(),
            mode: Mode::Form,
            form: ServiceForm::default(),
            locked: Vec::new(),
            dirty: Vec::new(),
            selected_field: 0,
            field_editor: None,
            diagnostics: Vec::new(),
            message: String::new(),
            diff: String::new(),
            reviewed_contents: None,
            backup: false,
            help: false,
            show_source: false,
            should_quit: false,
            raw_trailing_newline: true,
            raw_line_ending: "\n",
            discard_armed: false,
        };
        app.refresh()?;
        if let Some(unit_name) = unit {
            if let Some(item) = app.services.iter().find(|item| item.unit_name == unit_name) {
                app.open(item.path.clone())?;
            } else {
                let document = core::create_service(scope, unit_name).map_err(|e| e.to_string())?;
                app.set_document(document);
            }
        }
        Ok(app)
    }

    fn refresh(&mut self) -> Result<(), String> {
        self.services = core::list_services(self.scope).map_err(|e| e.to_string())?;
        self.apply_filter();
        Ok(())
    }

    fn apply_filter(&mut self) {
        let needle = self.filter.to_lowercase();
        self.filtered = self
            .services
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                (needle.is_empty()
                    || item.unit_name.to_lowercase().contains(&needle)
                    || item.path.to_string_lossy().to_lowercase().contains(&needle))
                .then_some(index)
            })
            .collect();
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
    }

    fn open(&mut self, path: PathBuf) -> Result<(), String> {
        let document = core::open_service(&path, self.scope).map_err(|e| e.to_string())?;
        self.set_document(document);
        Ok(())
    }

    fn set_document(&mut self, document: ServiceDocument) {
        self.raw_trailing_newline = document.contents.ends_with('\n');
        self.raw_line_ending = if document.contents.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        self.raw = textarea_from(&document.contents);
        self.diagnostics = document.diagnostics.clone();
        self.form = unit_text::project_form(&document.contents, document.is_drop_in).form;
        self.locked =
            unit_text::project_form(&document.contents, document.is_drop_in).locked_fields;
        self.document = Some(document);
        self.mode = Mode::Form;
        self.dirty.clear();
        self.selected_field = 0;
        self.field_editor = None;
        self.diff.clear();
        self.reviewed_contents = None;
        self.message.clear();
    }

    fn raw_contents(&self) -> String {
        let mut contents = self.raw.lines().join(self.raw_line_ending);
        if self.raw_trailing_newline && !contents.ends_with('\n') {
            contents.push_str(self.raw_line_ending);
        }
        contents
    }

    fn contents(&self) -> Result<String, String> {
        let Some(document) = &self.document else {
            return Err("No service is open".to_string());
        };
        if self.mode == Mode::Raw || self.dirty.is_empty() {
            return Ok(self.raw_contents());
        }
        unit_text::apply_form(
            &self.raw_contents(),
            &self.form,
            &self.dirty,
            document.is_drop_in,
        )
        .map_err(|error| error.to_string())
    }

    fn toggle_mode(&mut self) {
        let Some(document) = &self.document else {
            return;
        };
        if self.mode == Mode::Form {
            match self.contents() {
                Ok(contents) => {
                    self.raw = textarea_from(&contents);
                    self.raw_trailing_newline = contents.ends_with('\n');
                    self.raw_line_ending = if contents.contains("\r\n") {
                        "\r\n"
                    } else {
                        "\n"
                    };
                    self.dirty.clear();
                    self.mode = Mode::Raw;
                    self.diff.clear();
                    self.reviewed_contents = None;
                    self.diagnostics = core::validate(&contents, document.is_drop_in);
                    self.message.clear();
                }
                Err(error) => self.message = error,
            }
        } else {
            let contents = self.raw_contents();
            self.diagnostics = core::validate(&contents, document.is_drop_in);
            if self
                .diagnostics
                .iter()
                .any(|item| item.severity == Severity::Error)
            {
                self.message = "Fix raw syntax errors before opening the form.".into();
                return;
            }
            let projection: FormProjection =
                unit_text::project_form(&contents, document.is_drop_in);
            self.form = projection.form;
            self.locked = projection.locked_fields;
            self.dirty.clear();
            self.mode = Mode::Form;
            self.diff.clear();
            self.reviewed_contents = None;
            self.message.clear();
        }
    }

    fn save(&mut self) {
        let Some(document) = &self.document else {
            return;
        };
        match self.contents() {
            Ok(contents) => {
                self.diagnostics = core::validate(&contents, document.is_drop_in);
                if self
                    .diagnostics
                    .iter()
                    .any(|item| item.severity == Severity::Error)
                {
                    self.message = "Fix validation errors before saving.".into();
                    return;
                }
                if !document.is_new && contents == document.contents {
                    self.message = "No changes to save.".into();
                    return;
                }
                if self.reviewed_contents.as_deref() != Some(contents.as_str()) {
                    let before = document.expected_contents.as_deref().unwrap_or("");
                    self.diff = core::diff(before, &contents);
                    self.reviewed_contents = Some(contents);
                    self.message = "Review the diff, then press s again to save.".into();
                    return;
                }
                match core::save_service(document, &contents, self.backup, false) {
                    Ok(outcome) => self.saved(outcome, contents),
                    Err(error) => self.message = error.to_string(),
                }
            }
            Err(error) => self.message = error,
        }
    }

    fn saved(&mut self, outcome: SaveOutcome, contents: String) {
        if let Some(document) = self.document.as_mut() {
            document.contents = contents.clone();
            document.target_mode = outcome.target_mode;
            if !document.is_drop_in {
                document.source_contents = contents.clone();
            }
            document.expected_contents = Some(contents.clone());
            document.is_new = false;
        }
        self.raw = textarea_from(&contents);
        self.raw_trailing_newline = contents.ends_with('\n');
        self.raw_line_ending = if contents.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        self.dirty.clear();
        self.diff.clear();
        self.reviewed_contents = None;
        self.message = "Saved. Reload unit files when ready.".into();
        let _ = self.refresh();
    }

    fn make_diff(&mut self) {
        match self.contents() {
            Ok(contents) => {
                let before = self
                    .document
                    .as_ref()
                    .and_then(|document| document.expected_contents.as_deref())
                    .unwrap_or("");
                self.diff = core::diff(before, &contents);
                self.reviewed_contents = Some(contents);
                self.message = "Diff shown below the editor.".into();
            }
            Err(error) => self.message = error,
        }
    }

    fn verify(&mut self) {
        let Some(document) = &self.document else {
            return;
        };
        if document.is_new
            || self
                .contents()
                .is_ok_and(|contents| contents != document.contents)
        {
            self.message = "Save the current draft before verifying the installed unit.".into();
            return;
        }
        match core::verify_service(&document.source_path, self.scope) {
            Ok(result) if !result.available => {
                self.message = "systemd-analyze is unavailable.".into()
            }
            Ok(result) => {
                self.message = format!(
                    "systemd-analyze verify exit code: {} {}",
                    result
                        .exit_code
                        .map_or_else(|| "unknown".into(), |code| code.to_string()),
                    result.output.trim()
                )
            }
            Err(error) => self.message = error.to_string(),
        }
    }

    fn reload(&mut self) {
        self.message = match core::reload(self.scope) {
            Ok(output) if output.trim().is_empty() => "Unit files reloaded.".into(),
            Ok(output) => output,
            Err(error) => error.to_string(),
        };
    }

    fn close_document(&mut self) {
        if let Some(document) = &self.document {
            let changed = self
                .contents()
                .is_ok_and(|contents| contents != document.contents);
            if changed && !self.discard_armed {
                self.message = "Unsaved changes. Press Esc again to discard, or s to save.".into();
                self.discard_armed = true;
                return;
            }
        }
        self.document = None;
        self.discard_armed = false;
        self.diff.clear();
        self.reviewed_contents = None;
        if !self.message.starts_with("Unsaved changes.") {
            self.message.clear();
        } else {
            self.message = "Unsaved changes were discarded.".into();
        }
    }

    fn start_prompt(&mut self, action: PromptAction) {
        let (label, value) = match action {
            PromptAction::Search => ("Search", self.filter.clone()),
            PromptAction::Create => ("New service name", String::new()),
        };
        self.prompt = Some(Prompt {
            label,
            value,
            action,
        });
    }

    fn finish_prompt(&mut self) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        match prompt.action {
            PromptAction::Search => {
                self.filter = prompt.value;
                self.apply_filter();
            }
            PromptAction::Create => match core::create_service(self.scope, &prompt.value) {
                Ok(document) => self.set_document(document),
                Err(error) => self.message = error.to_string(),
            },
        }
    }
}

fn textarea_from(text: &str) -> TextArea<'static> {
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    let lines = if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    };
    let mut textarea = TextArea::new(lines);
    textarea.set_block(
        Block::default()
            .borders(Borders::ALL)
            .title("Unit text · Ctrl+S save · Ctrl+D diff"),
    );
    textarea
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (scope, unit) = parse_args()?;
    let mut app = App::new(scope, unit.as_deref()).map_err(io::Error::other)?;
    let mut terminal = ratatui::init();
    let result = run_app(&mut terminal, &mut app);
    ratatui::restore();
    result?;
    Ok(())
}

fn parse_args() -> Result<(Scope, Option<String>), Box<dyn std::error::Error>> {
    let mut scope = Scope::User;
    let mut unit = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--scope" => {
                scope = match args.next().as_deref() {
                    Some("user") => Scope::User,
                    Some("system") => Scope::System,
                    _ => return Err("--scope requires user or system".into()),
                };
            }
            "--unit" => unit = args.next(),
            "--help" | "-h" => {
                println!("systemd-service-editor [--scope user|system] [--unit NAME]");
                std::process::exit(0);
            }
            unknown => return Err(format!("unknown argument: {unknown}").into()),
        }
    }
    Ok((scope, unit))
}

fn run_app(
    terminal: &mut Terminal<impl ratatui::backend::Backend>,
    app: &mut App,
) -> io::Result<()> {
    while !app.should_quit {
        terminal
            .draw(|frame| draw(frame, app))
            .map_err(|error| io::Error::other(error.to_string()))?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            handle_key(app, key);
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, key: KeyEvent) {
    if app.discard_armed && key.code != KeyCode::Esc {
        app.discard_armed = false;
    }
    if app.help {
        if key.code == KeyCode::Esc || key.code == KeyCode::Char('?') {
            app.help = false;
        }
        return;
    }
    if app.show_source {
        if key.code == KeyCode::Esc || key.code == KeyCode::Char('o') {
            app.show_source = false;
        }
        return;
    }
    if !app.diff.is_empty() {
        if key.code == KeyCode::Char('s') {
            app.diff.clear();
            app.save();
        } else if key.code == KeyCode::Esc || key.code == KeyCode::Char('d') {
            app.diff.clear();
        }
        return;
    }
    if let Some(prompt) = app.prompt.as_mut() {
        match key.code {
            KeyCode::Enter => app.finish_prompt(),
            KeyCode::Esc => app.prompt = None,
            KeyCode::Backspace => {
                prompt.value.pop();
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                prompt.value.push(character)
            }
            _ => {}
        }
        return;
    }
    if let Some(editor) = app.field_editor.as_mut() {
        match key.code {
            KeyCode::Enter => {
                let value = std::mem::take(editor);
                let field = FORM_FIELDS[app.selected_field].0;
                set_form_field(&mut app.form, field, value);
                if !app.dirty.iter().any(|dirty| dirty == field) {
                    app.dirty.push(field.to_string());
                }
                app.diff.clear();
                app.reviewed_contents = None;
                app.field_editor = None;
                app.message.clear();
            }
            KeyCode::Esc => app.field_editor = None,
            KeyCode::Backspace => {
                editor.pop();
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                editor.push(character)
            }
            _ => {}
        }
        return;
    }
    if app.document.is_none() {
        match key.code {
            KeyCode::Char('q') => app.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => {
                app.selected = (app.selected + 1).min(app.filtered.len().saturating_sub(1))
            }
            KeyCode::Char('k') | KeyCode::Up => app.selected = app.selected.saturating_sub(1),
            KeyCode::Enter => {
                if let Some(index) = app.filtered.get(app.selected).copied() {
                    let path = app.services[index].path.clone();
                    if let Err(error) = app.open(path) {
                        app.message = error;
                    }
                }
            }
            KeyCode::Char('/') => app.start_prompt(PromptAction::Search),
            KeyCode::Char('c') => app.start_prompt(PromptAction::Create),
            KeyCode::Char('u') => {
                app.scope = if app.scope == Scope::User {
                    Scope::System
                } else {
                    Scope::User
                };
                if let Err(error) = app.refresh() {
                    app.message = error;
                }
            }
            KeyCode::Char('r') => {
                if let Err(error) = app.refresh() {
                    app.message = error;
                }
            }
            KeyCode::Char('?') => app.help = true,
            _ => {}
        }
        return;
    }

    if app.mode == Mode::Raw {
        if key.code == KeyCode::Tab {
            app.toggle_mode();
        } else if key.code == KeyCode::Esc {
            app.close_document();
        } else if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            app.save();
        } else if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('d') {
            app.make_diff();
        } else if key.code == KeyCode::Char('?') && key.modifiers.is_empty() {
            app.help = true;
        } else {
            app.raw.input(key);
            app.diff.clear();
            app.reviewed_contents = None;
        }
        return;
    }

    match key.code {
        KeyCode::Esc => app.close_document(),
        KeyCode::Tab => app.toggle_mode(),
        KeyCode::Char('j') | KeyCode::Down => {
            app.selected_field = (app.selected_field + 1).min(FORM_FIELDS.len() - 1)
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.selected_field = app.selected_field.saturating_sub(1)
        }
        KeyCode::Enter => {
            let key = FORM_FIELDS[app.selected_field].0;
            if app.locked.iter().any(|locked| locked == key) {
                app.message =
                    format!("{key} has repeated or continued values; edit it in raw mode.");
            } else if let Some(document) = &app.document {
                if document.is_drop_in && key == "wanted_by" {
                    app.message = "[Install] WantedBy belongs in a base unit.".into();
                } else {
                    app.field_editor = Some(form_field(&app.form, key).to_string());
                }
            }
        }
        KeyCode::Char('s') => app.save(),
        KeyCode::Char('d') => app.make_diff(),
        KeyCode::Char('v') => app.verify(),
        KeyCode::Char('r') => app.reload(),
        KeyCode::Char('o') => app.show_source = true,
        KeyCode::Char('b') => app.backup = !app.backup,
        KeyCode::Char('?') => app.help = true,
        _ => {}
    }
}

fn form_field<'a>(form: &'a ServiceForm, key: &str) -> &'a str {
    match key {
        "description" => &form.description,
        "service_type" => &form.service_type,
        "exec_start" => &form.exec_start,
        "working_directory" => &form.working_directory,
        "user" => &form.user,
        "group" => &form.group,
        "restart" => &form.restart,
        "restart_sec" => &form.restart_sec,
        "wanted_by" => &form.wanted_by,
        _ => "",
    }
}

fn set_form_field(form: &mut ServiceForm, key: &str, value: String) {
    match key {
        "description" => form.description = value,
        "service_type" => form.service_type = value,
        "exec_start" => form.exec_start = value,
        "working_directory" => form.working_directory = value,
        "user" => form.user = value,
        "group" => form.group = value,
        "restart" => form.restart = value,
        "restart_sec" => form.restart_sec = value,
        "wanted_by" => form.wanted_by = value,
        _ => {}
    }
}

fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(2),
        ])
        .split(area);
    let title = match &app.document {
        Some(document) => format!(
            "Systemd Service Editor · {} · {}",
            app.scope.as_str(),
            document.unit_name
        ),
        None => format!("Systemd Service Editor · {} scope", app.scope.as_str()),
    };
    frame.render_widget(
        Paragraph::new(title)
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .block(Block::default().borders(Borders::ALL)),
        rows[0],
    );
    if app.document.is_none() {
        draw_list(frame, app, rows[1]);
    } else {
        draw_editor(frame, app, rows[1]);
    }
    let help = if app.document.is_some() {
        "Tab mode · s save · d diff · v verify · r reload · b backup · Esc list · ? help"
    } else {
        "j/k move · Enter open · / search · c create · u scope · r refresh · q quit · ? help"
    };
    frame.render_widget(
        Paragraph::new(help).style(Style::default().fg(Color::Gray)),
        rows[2],
    );
    if let Some(prompt) = &app.prompt {
        draw_prompt(frame, area, prompt);
    }
    if let Some(value) = &app.field_editor {
        draw_text_prompt(frame, area, FORM_FIELDS[app.selected_field].1, value);
    }
    if app.help {
        draw_help(frame, area);
    }
    if app.show_source {
        draw_source(frame, area, app);
    }
}

fn draw_list(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(area);
    let header = format!(
        "{} service units · filter: {}",
        app.filtered.len(),
        if app.filter.is_empty() {
            "(none)"
        } else {
            &app.filter
        }
    );
    frame.render_widget(Paragraph::new(header), chunks[0]);
    let entries: Vec<ListItem> = app
        .filtered
        .iter()
        .map(|index| {
            let item = &app.services[*index];
            let marker = if item.masked {
                " [MASKED]"
            } else if item.is_vendor {
                " [VENDOR]"
            } else {
                " [CONFIG]"
            };
            ListItem::new(Text::from(vec![
                Line::from(Span::styled(
                    format!("{}{}", item.unit_name, marker),
                    if item.masked {
                        Style::default().fg(Color::Red)
                    } else {
                        Style::default().add_modifier(Modifier::BOLD)
                    },
                )),
                Line::from(Span::styled(
                    item.path.display().to_string(),
                    Style::default().fg(Color::Gray),
                )),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(app.selected));
    frame.render_stateful_widget(
        List::new(entries)
            .block(Block::default().borders(Borders::ALL).title("Services"))
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
        chunks[1],
        &mut state,
    );
}

fn draw_editor(frame: &mut Frame, app: &App, area: Rect) {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(area);
    let document = app.document.as_ref().expect("editor has a document");
    let mode = if app.mode == Mode::Form {
        "COMMON FIELDS"
    } else {
        "RAW UNIT"
    };
    frame.render_widget(
        Paragraph::new(format!(
            "{mode} · target: {}",
            document.target_path.display()
        ))
        .style(Style::default().fg(Color::Yellow)),
        vertical[0],
    );

    if app.mode == Mode::Raw {
        frame.render_widget(&app.raw, vertical[1]);
    } else {
        draw_form(frame, app, vertical[1]);
    }
    let status = if !app.message.is_empty() {
        app.message.clone()
    } else if document.is_drop_in {
        format!(
            "Vendor unit: edits save to a local drop-in · {}",
            document.source_path.display()
        )
    } else {
        String::new()
    };
    frame.render_widget(
        Paragraph::new(status)
            .style(Style::default().fg(Color::Yellow))
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title("Status")),
        vertical[2],
    );
    if !app.diff.is_empty() {
        let rect = centered_rect(90, 70, area);
        frame.render_widget(Clear, rect);
        frame.render_widget(
            Paragraph::new(app.diff.as_str())
                .wrap(Wrap { trim: false })
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Diff · press Esc to close"),
                ),
            rect,
        );
    }
    if !app.diagnostics.is_empty() && app.diff.is_empty() {
        let first = &app.diagnostics[0];
        frame.render_widget(
            Paragraph::new(format!(
                "{} diagnostic(s): {}",
                app.diagnostics.len(),
                first.message
            ))
            .style(Style::default().fg(if first.severity == Severity::Error {
                Color::Red
            } else {
                Color::Yellow
            })),
            Rect {
                x: area.x + 1,
                y: area.y + area.height.saturating_sub(1),
                width: area.width.saturating_sub(2),
                height: 1,
            },
        );
    }
}

fn draw_form(frame: &mut Frame, app: &App, area: Rect) {
    let rows: Vec<ListItem> = FORM_FIELDS
        .iter()
        .map(|(field, label)| {
            let value = form_field(&app.form, field);
            let locked = app.locked.iter().any(|item| item == field);
            let disabled = app
                .document
                .as_ref()
                .is_some_and(|document| document.is_drop_in && *field == "wanted_by");
            let suffix = if locked {
                "  [raw only]"
            } else if disabled {
                "  [base unit only]"
            } else {
                ""
            };
            ListItem::new(format!(
                "{label:<24} {}{}",
                if value.is_empty() { "(unset)" } else { value },
                suffix
            ))
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(app.selected_field));
    frame.render_stateful_widget(
        List::new(rows)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Select a field and press Enter to edit"),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
        area,
        &mut state,
    );
}

fn draw_prompt(frame: &mut Frame, area: Rect, prompt: &Prompt) {
    draw_text_prompt(frame, area, prompt.label, &prompt.value);
}

fn draw_text_prompt(frame: &mut Frame, area: Rect, title: &str, value: &str) {
    let rect = centered_rect(65, 20, area);
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(value).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("{title} · Enter accept · Esc cancel")),
        ),
        rect,
    );
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let rect = centered_rect(72, 55, area);
    frame.render_widget(Clear, rect);
    let lines = [
        "Systemd Service Editor",
        "",
        "List: j/k move, Enter open, / search, c create, u switch scope, r refresh.",
        "Editor: Tab switches form/raw, s saves, d shows diff, v verifies, r reloads, o views source.",
        "Form: j/k select, Enter edits a value, Esc returns to the service list.",
        "Raw: Ctrl+S saves, Ctrl+D shows diff, Tab switches back to the form.",
        "Vendor files are edited through a drop-in. System writes require sudo.",
        "Reloading units does not start, stop, or restart a service.",
        "",
        "Press Esc or ? to close help.",
    ];
    frame.render_widget(
        Paragraph::new(lines.join("\n"))
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title("Help")),
        rect,
    );
}

fn draw_source(frame: &mut Frame, area: Rect, app: &App) {
    let Some(document) = app.document.as_ref() else {
        return;
    };
    let rect = centered_rect(90, 78, area);
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(document.source_contents.as_str())
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Base unit · read only · press Esc to close"),
            ),
        rect,
    );
}

fn centered_rect(width_percent: u16, height_percent: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage((100 - height_percent) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(vertical[1])[1]
}
