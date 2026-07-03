use std::io::{self, Stdout};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use g7_core::commands::{DoctorCheckStatus, doctor, install, plan};
use miette::{Result, miette};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

use crate::plan_options;

type TuiTerminal = Terminal<CrosstermBackend<Stdout>>;

const WEB_SERVERS: [&str; 2] = ["nginx", "apache"];
const PHP_VERSIONS: [&str; 2] = ["8.5", "8.3"];
const DATABASES: [&str; 2] = ["mariadb", "mysql"];
const WWW_MODES: [&str; 4] = ["redirect-to-root", "redirect-to-www", "include", "none"];
const MAIL_MODES: [&str; 3] = ["none", "smtp-relay", "local-postfix"];
const ENCRYPTION_MODES: [&str; 3] = ["starttls", "tls", "none"];

pub fn run(domain_arg: Option<String>, local_test_arg: bool) -> Result<()> {
    let mut terminal = enter_terminal()?;
    let mut app = SetupApp::new(domain_arg, local_test_arg);
    let result = run_loop(&mut terminal, &mut app);
    restore_terminal(&mut terminal)?;

    if let Some(report) = app.install_report {
        println!("G7 Installer setup prepared");
        println!("domain: {}", report.domain);
        println!("phase: {}", report.phase);
        println!("state: {}", report.state_path.display());
        println!("owned_files: {}", report.owned_files_path.display());
    }

    result
}

fn run_loop(terminal: &mut TuiTerminal, app: &mut SetupApp) -> Result<()> {
    loop {
        terminal
            .draw(|frame| render(frame, app))
            .map_err(|err| miette!("failed to draw setup UI: {err}"))?;

        if app.should_quit {
            return Ok(());
        }

        if event::poll(Duration::from_millis(200))
            .map_err(|err| miette!("failed to poll terminal event: {err}"))?
        {
            match event::read().map_err(|err| miette!("failed to read terminal event: {err}"))? {
                Event::Key(key) => app.handle_key(key),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }
}

fn enter_terminal() -> Result<TuiTerminal> {
    enable_raw_mode().map_err(|err| miette!("failed to enable terminal raw mode: {err}"))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|err| miette!("failed to enter alternate screen: {err}"))?;
    Terminal::new(CrosstermBackend::new(stdout))
        .map_err(|err| miette!("failed to initialize terminal: {err}"))
}

fn restore_terminal(terminal: &mut TuiTerminal) -> Result<()> {
    disable_raw_mode().map_err(|err| miette!("failed to disable terminal raw mode: {err}"))?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .map_err(|err| miette!("failed to leave alternate screen: {err}"))?;
    terminal
        .show_cursor()
        .map_err(|err| miette!("failed to restore terminal cursor: {err}"))
}

#[derive(Debug)]
struct SetupApp {
    focus: usize,
    local_test: bool,
    domain: String,
    web_server: usize,
    php_version: usize,
    database: usize,
    www_mode: usize,
    redis_enabled: bool,
    mail_mode: usize,
    smtp_host: String,
    smtp_port: String,
    smtp_from: String,
    smtp_encryption: usize,
    doctor_report: doctor::DoctorReport,
    logs: Vec<String>,
    install_report: Option<install::InstallReport>,
    should_quit: bool,
}

impl SetupApp {
    fn new(domain_arg: Option<String>, local_test_arg: bool) -> Self {
        let domain = domain_arg.unwrap_or_else(|| {
            if local_test_arg {
                "g7-test.local".to_string()
            } else {
                "example.com".to_string()
            }
        });
        let doctor_report = doctor::run();
        let mut logs = vec![
            "doctor completed".to_string(),
            "Use Up/Down to move, Left/Right to change choices.".to_string(),
            "Type in text fields. Press Enter on Prepare install.".to_string(),
        ];
        if !doctor_report.install_allowed {
            logs.push("preflight failed; fix failed checks before preparing install".to_string());
        }

        Self {
            focus: 0,
            local_test: local_test_arg,
            domain,
            web_server: 0,
            php_version: 0,
            database: 0,
            www_mode: if local_test_arg { 3 } else { 0 },
            redis_enabled: true,
            mail_mode: 0,
            smtp_host: String::new(),
            smtp_port: plan::DEFAULT_SMTP_PORT.to_string(),
            smtp_from: String::new(),
            smtp_encryption: 0,
            doctor_report,
            logs,
            install_report: None,
            should_quit: false,
        }
    }

    fn visible_fields(&self) -> Vec<Field> {
        let mut fields = vec![
            Field::Profile,
            Field::Domain,
            Field::WebServer,
            Field::PhpVersion,
            Field::Database,
            Field::WwwMode,
            Field::Redis,
            Field::MailMode,
        ];

        if self.mail_value() == "smtp-relay" {
            fields.extend([
                Field::SmtpHost,
                Field::SmtpPort,
                Field::SmtpFrom,
                Field::SmtpEncryption,
            ]);
        }

        fields.extend([Field::Prepare, Field::Quit]);
        fields
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        match key.code {
            KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('q') if !self.focused_field().is_text() => self.should_quit = true,
            KeyCode::Up => self.move_focus(-1),
            KeyCode::Down | KeyCode::Tab => self.move_focus(1),
            KeyCode::BackTab => self.move_focus(-1),
            KeyCode::Left => self.adjust_choice(-1),
            KeyCode::Right => self.adjust_choice(1),
            KeyCode::Enter => self.activate(),
            KeyCode::Backspace => self.backspace_text(),
            KeyCode::Char(ch) => self.push_text(ch),
            _ => {}
        }
    }

    fn move_focus(&mut self, delta: isize) {
        let count = self.visible_fields().len();
        let next = self.focus as isize + delta;
        self.focus = next.rem_euclid(count as isize) as usize;
    }

    fn adjust_choice(&mut self, delta: isize) {
        match self.focused_field() {
            Field::Profile => {
                self.local_test = !self.local_test;
                if self.local_test && self.domain == "example.com" {
                    self.domain = "g7-test.local".to_string();
                }
                if !self.local_test && self.domain == "g7-test.local" {
                    self.domain = "example.com".to_string();
                }
            }
            Field::WebServer => adjust_index(&mut self.web_server, WEB_SERVERS.len(), delta),
            Field::PhpVersion => adjust_index(&mut self.php_version, PHP_VERSIONS.len(), delta),
            Field::Database => adjust_index(&mut self.database, DATABASES.len(), delta),
            Field::WwwMode => adjust_index(&mut self.www_mode, WWW_MODES.len(), delta),
            Field::Redis => self.redis_enabled = !self.redis_enabled,
            Field::MailMode => {
                adjust_index(&mut self.mail_mode, MAIL_MODES.len(), delta);
                if self.mail_value() == "local-postfix" {
                    self.smtp_port = "25".to_string();
                }
            }
            Field::SmtpEncryption => {
                adjust_index(&mut self.smtp_encryption, ENCRYPTION_MODES.len(), delta);
            }
            _ => {}
        }
    }

    fn activate(&mut self) {
        match self.focused_field() {
            Field::Prepare => self.prepare_install(),
            Field::Quit => self.should_quit = true,
            field if !field.is_text() => self.adjust_choice(1),
            _ => self.move_focus(1),
        }
    }

    fn push_text(&mut self, ch: char) {
        match self.focused_field() {
            Field::Domain => self.domain.push(ch),
            Field::SmtpHost => self.smtp_host.push(ch),
            Field::SmtpPort if ch.is_ascii_digit() => self.smtp_port.push(ch),
            Field::SmtpFrom => self.smtp_from.push(ch),
            _ => {}
        }
    }

    fn backspace_text(&mut self) {
        match self.focused_field() {
            Field::Domain => {
                self.domain.pop();
            }
            Field::SmtpHost => {
                self.smtp_host.pop();
            }
            Field::SmtpPort => {
                self.smtp_port.pop();
            }
            Field::SmtpFrom => {
                self.smtp_from.pop();
            }
            _ => {}
        }
    }

    fn prepare_install(&mut self) {
        self.logs.push("preparing install...".to_string());
        if !self.doctor_report.install_allowed {
            self.logs
                .push("blocked: doctor preflight failed".to_string());
            return;
        }

        let smtp_port = match self.smtp_port.parse::<u16>() {
            Ok(port) => port,
            Err(_) => {
                self.logs
                    .push("blocked: SMTP port must be a number".to_string());
                return;
            }
        };

        let options = plan_options(
            self.local_test,
            self.web_server_value().to_string(),
            self.php_version_value().to_string(),
            self.database_value().to_string(),
            self.www_mode_value().to_string(),
            self.redis_value().to_string(),
            self.mail_value().to_string(),
            if self.smtp_host.is_empty() {
                None
            } else {
                Some(self.smtp_host.clone())
            },
            smtp_port,
            if self.smtp_from.is_empty() {
                None
            } else {
                Some(self.smtp_from.clone())
            },
            self.smtp_encryption_value().to_string(),
            true,
            true,
            !self.local_test,
        );

        match plan::build_with_options(self.domain.clone(), options.clone()) {
            Ok(_) => {}
            Err(err) => {
                self.logs.push(format!("blocked: {err}"));
                return;
            }
        }

        match install::run(self.domain.clone(), options) {
            Ok(report) => {
                self.logs.push("prepared install state written".to_string());
                self.logs
                    .push(format!("state: {}", report.state_path.display()));
                self.logs.push(format!(
                    "owned_files: {}",
                    report.owned_files_path.display()
                ));
                self.install_report = Some(report);
            }
            Err(err) => self.logs.push(format!("failed: {err}")),
        }
    }

    fn focused_field(&self) -> Field {
        self.visible_fields()[self.focus]
    }

    fn web_server_value(&self) -> &'static str {
        WEB_SERVERS[self.web_server]
    }

    fn php_version_value(&self) -> &'static str {
        PHP_VERSIONS[self.php_version]
    }

    fn database_value(&self) -> &'static str {
        DATABASES[self.database]
    }

    fn www_mode_value(&self) -> &'static str {
        WWW_MODES[self.www_mode]
    }

    fn redis_value(&self) -> &'static str {
        if self.redis_enabled {
            "enable"
        } else {
            "disable"
        }
    }

    fn mail_value(&self) -> &'static str {
        MAIL_MODES[self.mail_mode]
    }

    fn smtp_encryption_value(&self) -> &'static str {
        ENCRYPTION_MODES[self.smtp_encryption]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Profile,
    Domain,
    WebServer,
    PhpVersion,
    Database,
    WwwMode,
    Redis,
    MailMode,
    SmtpHost,
    SmtpPort,
    SmtpFrom,
    SmtpEncryption,
    Prepare,
    Quit,
}

impl Field {
    fn label(self) -> &'static str {
        match self {
            Self::Profile => "Profile",
            Self::Domain => "Domain",
            Self::WebServer => "Web server",
            Self::PhpVersion => "PHP-FPM",
            Self::Database => "Database",
            Self::WwwMode => "www policy",
            Self::Redis => "Redis",
            Self::MailMode => "Mail",
            Self::SmtpHost => "SMTP host",
            Self::SmtpPort => "SMTP port",
            Self::SmtpFrom => "SMTP from",
            Self::SmtpEncryption => "SMTP encryption",
            Self::Prepare => "Prepare install",
            Self::Quit => "Quit",
        }
    }

    fn is_text(self) -> bool {
        matches!(
            self,
            Self::Domain | Self::SmtpHost | Self::SmtpPort | Self::SmtpFrom
        )
    }
}

fn adjust_index(index: &mut usize, len: usize, delta: isize) {
    *index = (*index as isize + delta).rem_euclid(len as isize) as usize;
}

fn render(frame: &mut ratatui::Frame<'_>, app: &SetupApp) {
    let area = frame.area();
    frame.render_widget(Clear, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(7),
        ])
        .split(area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(24),
            Constraint::Percentage(46),
            Constraint::Percentage(34),
        ])
        .split(rows[1]);

    render_header(frame, rows[0]);
    render_steps(frame, columns[0], app);
    render_form(frame, columns[1], app);
    render_doctor(frame, columns[2], app);
    render_logs(frame, rows[2], app);
}

fn render_header(frame: &mut ratatui::Frame<'_>, area: Rect) {
    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                "G7 Installer Setup",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  full-screen TUI"),
        ]),
        Line::from(
            "Up/Down move | Left/Right change | type text | Enter action | q/Esc/Ctrl+C quit",
        ),
    ])
    .block(Block::default().borders(Borders::ALL));

    frame.render_widget(header, area);
}

fn render_steps(frame: &mut ratatui::Frame<'_>, area: Rect, app: &SetupApp) {
    let steps = [
        ("1 Doctor", app.doctor_report.install_allowed),
        ("2 Options", true),
        ("3 Summary", true),
        ("4 Prepare", app.install_report.is_some()),
        ("5 Verify", false),
    ];
    let items = steps
        .iter()
        .map(|(step, done)| {
            let marker = if *done { "[ok]" } else { "[..]" };
            ListItem::new(format!("{marker} {step}"))
        })
        .collect::<Vec<_>>();

    frame.render_widget(
        List::new(items).block(Block::default().title("Steps").borders(Borders::ALL)),
        area,
    );
}

fn render_form(frame: &mut ratatui::Frame<'_>, area: Rect, app: &SetupApp) {
    let fields = app.visible_fields();
    let lines = fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let selected = index == app.focus;
            let prefix = if selected { "> " } else { "  " };
            let style = if selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(format!("{:<16}", field.label()), style),
                Span::raw(field_value(*field, app)),
            ])
        })
        .collect::<Vec<_>>();

    let paragraph = Paragraph::new(lines)
        .block(Block::default().title("Config").borders(Borders::ALL))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

fn field_value(field: Field, app: &SetupApp) -> String {
    match field {
        Field::Profile => {
            if app.local_test {
                "local-test".to_string()
            } else {
                "public".to_string()
            }
        }
        Field::Domain => app.domain.clone(),
        Field::WebServer => app.web_server_value().to_string(),
        Field::PhpVersion => app.php_version_value().to_string(),
        Field::Database => app.database_value().to_string(),
        Field::WwwMode => app.www_mode_value().to_string(),
        Field::Redis => app.redis_value().to_string(),
        Field::MailMode => app.mail_value().to_string(),
        Field::SmtpHost => app.smtp_host.clone(),
        Field::SmtpPort => app.smtp_port.clone(),
        Field::SmtpFrom => app.smtp_from.clone(),
        Field::SmtpEncryption => app.smtp_encryption_value().to_string(),
        Field::Prepare => "Enter to run prepared install".to_string(),
        Field::Quit => "Enter to exit".to_string(),
    }
}

fn render_doctor(frame: &mut ratatui::Frame<'_>, area: Rect, app: &SetupApp) {
    let mut lines = vec![Line::from(format!(
        "install_allowed: {}",
        app.doctor_report.install_allowed
    ))];
    lines.extend(app.doctor_report.checks.iter().map(|check| {
        let status = match check.status {
            DoctorCheckStatus::Pass => "pass",
            DoctorCheckStatus::Warn => "warn",
            DoctorCheckStatus::Fail => "fail",
            DoctorCheckStatus::Pending => "wait",
        };
        Line::from(format!("[{status}] {} - {}", check.name, check.message))
    }));

    let paragraph = Paragraph::new(lines)
        .block(Block::default().title("Doctor").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_logs(frame: &mut ratatui::Frame<'_>, area: Rect, app: &SetupApp) {
    let height = area.height.saturating_sub(2) as usize;
    let start = app.logs.len().saturating_sub(height);
    let lines = app.logs[start..]
        .iter()
        .map(|line| Line::from(line.as_str()))
        .collect::<Vec<_>>();

    let paragraph = Paragraph::new(lines)
        .block(Block::default().title("Live log").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}
