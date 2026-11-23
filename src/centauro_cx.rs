use std::io::{self, Read, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use serde_json::{json, Value};

#[derive(Clone)]
struct Request {
    command: &'static str,
    payload: Value,
}
struct Reply {
    command: &'static str,
    result: Result<Value, HelperError>,
}

/// Definitivo: o auxiliar terminou normalmente e respondeu com erro (ou nem
/// foi iniciado), então a ação não foi aplicada. Incerto: timeout, cancelamento
/// ou falha do processo/da resposta, quando o efeito é desconhecido.
#[derive(Debug, Clone, PartialEq)]
enum HelperError {
    Definitive(String),
    Uncertain(String),
}
impl HelperError {
    fn message(&self) -> &str {
        match self {
            Self::Definitive(m) | Self::Uncertain(m) => m,
        }
    }
}

fn helper(command: &str, payload: &Value, cancel: &AtomicBool) -> Result<Value, HelperError> {
    let python = std::env::var_os("PRETORIAN_CX_PYTHON")
        .ok_or_else(|| HelperError::Definitive("Auxiliar não configurado".into()))?;
    let script = std::env::var_os("PRETORIAN_CX_HELPER")
        .ok_or_else(|| HelperError::Definitive("Auxiliar não configurado".into()))?;
    let mut child = Command::new(python)
        .arg(script)
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| HelperError::Definitive("Não foi possível iniciar o auxiliar".into()))?;
    let input = serde_json::to_vec(payload)
        .map_err(|_| HelperError::Definitive("Não foi possível preparar a solicitação".into()))?;
    let unc = |m: &str| HelperError::Uncertain(m.to_owned());
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| unc("Entrada indisponível"))?;
    let writer = thread::spawn(move || stdin.write_all(&input));
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| unc("Resposta indisponível"))?;
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let started = Instant::now();
    let status = loop {
        if cancel.load(Ordering::Relaxed) || started.elapsed() >= Duration::from_secs(15) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(unc("Tempo esgotado ou operação cancelada. Confira o atendimento antes de tentar novamente."));
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(unc("Falha ao aguardar o auxiliar"));
            }
        }
    };
    writer
        .join()
        .map_err(|_| unc("Falha ao enviar solicitação"))?
        .map_err(|_| unc("Falha ao enviar solicitação"))?;
    let bytes = reader
        .join()
        .map_err(|_| unc("Falha ao ler resposta"))?
        .map_err(|_| unc("Falha ao ler resposta"))?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| unc("Resposta inválida do serviço"))?;
    parse_output(status.success(), value)
}

/// Interpreta a saída de um auxiliar que terminou e entregou JSON válido.
/// Erro explícito com saída normal é definitivo; saída anormal é incerta.
fn parse_output(exited_normally: bool, value: Value) -> Result<Value, HelperError> {
    let failed = value.get("error").is_some() || value.get("ok") == Some(&Value::Bool(false));
    if !exited_normally || failed {
        let message = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Serviço indisponível")
            .to_owned();
        return Err(if exited_normally {
            HelperError::Definitive(message)
        } else {
            HelperError::Uncertain(message)
        });
    }
    Ok(value.get("data").cloned().unwrap_or(value))
}

fn worker(rx: Receiver<Request>, tx: Sender<Reply>, cancel: Arc<AtomicBool>) {
    let mut next_snapshot = Instant::now();
    let mut next_presence = Instant::now() + Duration::from_secs(15);
    let mut available = false;
    loop {
        let timeout = next_snapshot.saturating_duration_since(Instant::now());
        match rx.recv_timeout(timeout) {
            Ok(request) => {
                let result = helper(request.command, &request.payload, &cancel);
                if request.command == "presence" && result.is_ok() {
                    available = request.payload["state"] == "available";
                    next_presence = Instant::now() + Duration::from_secs(15);
                }
                if tx
                    .send(Reply {
                        command: request.command,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
                next_snapshot = Instant::now();
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if available && Instant::now() >= next_presence {
                    let result = helper("presence", &json!({"state":"available"}), &cancel);
                    if tx
                        .send(Reply {
                            command: "heartbeat",
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                    next_presence = Instant::now() + Duration::from_secs(15);
                }
                let result = helper("snapshot", &json!({}), &cancel);
                if tx
                    .send(Reply {
                        command: "snapshot",
                        result,
                    })
                    .is_err()
                {
                    break;
                }
                next_snapshot = Instant::now() + Duration::from_secs(2);
            }
        }
    }
}

fn string<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}
fn list<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn message_status(status: &str) -> &str {
    match status {
        "received" => "recebida",
        "pending" => "pendente",
        "sent" => "enviada",
        "delivered" => "entregue",
        "failed" => "falhou",
        "uncertain" => "incerta",
        "abandoned" => "acompanhamento encerrado",
        _ => status,
    }
}

fn failed_messages(attendance: &Value) -> Vec<&Value> {
    list(attendance, "messages")
        .iter()
        .filter(|m| matches!(string(m, "status"), "failed" | "uncertain"))
        .collect()
}

#[derive(Default)]
struct State {
    snapshot: Value,
    focus: usize,
    focus_id: String,
    focus_payload: Value,
    scroll: [u16; 2],
    right_scroll: u16,
    abandon_mode: u8,
    abandon_index: usize,
    abandon_message: String,
    abandon_revision: Value,
    input: String,
    editing: bool,
    completing: bool,
    pending: bool,
    offline: bool,
    notice: String,
    schedule_open: bool,
    schedule: Value,
    schedule_rows: [String; 7],
    schedule_row: usize,
    schedule_editing: bool,
    last_request: Option<Request>,
    uncertain: bool,
    selected: Option<(String, Value, usize)>,
    audio_script: Option<(String, String)>,
    too_small: bool,
}
impl State {
    fn attendances(&self) -> &[Value] {
        list(&self.snapshot, "attendances")
    }
    fn current(&self) -> Option<&Value> {
        self.attendances().get(self.focus)
    }
    fn update(&mut self, snapshot: Value) {
        let id = self.focus_id.clone();
        self.snapshot = snapshot;
        self.focus = self
            .attendances()
            .iter()
            .position(|a| string(a, "id") == id)
            .unwrap_or(0);
        self.sync_focus();
    }
    fn sync_focus(&mut self) {
        let id = self
            .current()
            .map(|a| string(a, "id"))
            .unwrap_or("")
            .to_owned();
        if id != self.focus_id {
            self.focus_id = id;
            self.input.clear();
            self.editing = false;
            self.completing = false;
            self.abandon_mode = 0;
            self.right_scroll = 0;
        }
        let focus = self.current().map(|a| json!({"id":a["id"],"revision":a["revision"],"name":a["customerName"],"audioOnly":a["audioOnly"]})).unwrap_or(json!({"id":null}));
        if focus != self.focus_payload {
            self.focus_payload = focus.clone();
            Self::notify_focus(&focus);
        }
    }
    fn notify_focus(focus: &Value) {
        let _ = write!(io::stdout(), "\x1b]777;pretorian-cx;{}\x1b\\", focus);
        let _ = io::stdout().flush();
    }
    fn load_schedule(&mut self, value: Value) {
        self.schedule_rows = Default::default();
        for period in list(&value, "periods") {
            if let Some(day) = period["day"].as_u64().filter(|d| *d < 7) {
                let row = &mut self.schedule_rows[day as usize];
                if !row.is_empty() {
                    row.push_str(", ");
                }
                row.push_str(&format!(
                    "{}-{}",
                    string(period, "start"),
                    string(period, "end")
                ));
            }
        }
        self.schedule = value;
    }
    fn save_schedule(&mut self, tx: &Sender<Request>) {
        let mut periods = Vec::new();
        for (day, row) in self.schedule_rows.iter().enumerate() {
            if row.trim().is_empty() {
                continue;
            }
            for part in row.split(',') {
                let Some((start, end)) = part.trim().split_once('-') else {
                    self.notice = "Use HH:MM-HH:MM; deixe vazio para fechado.".into();
                    return;
                };
                fn valid(t: &str) -> bool {
                    let b = t.as_bytes();
                    b.len() == 5
                        && b[2] == b':'
                        && b.iter()
                            .enumerate()
                            .all(|(i, c)| i == 2 || c.is_ascii_digit())
                        && &t[..2] < "24"
                        && &t[3..] < "60"
                }
                if !valid(start) || !valid(end) || start >= end {
                    self.notice =
                        "Horário inválido. Use HH:MM-HH:MM, com início anterior ao fim.".into();
                    return;
                }
                periods.push(json!({"day":day,"start":start,"end":end}));
            }
        }
        let payload = json!({"revision":self.schedule["revision"],"timezone":self.schedule["timezone"],"periods":periods,"closedDates":self.schedule["closedDates"]});
        self.request(tx, "schedule-save", payload);
    }
    fn request(&mut self, tx: &Sender<Request>, command: &'static str, mut payload: Value) {
        if self.pending
            || self.offline
            || self.uncertain
            || self.snapshot.get("enabled") == Some(&Value::Bool(false))
        {
            return;
        }
        if matches!(
            command,
            "claim" | "choose" | "agent" | "complete" | "abandon"
        ) {
            payload["requestId"] = json!(uuid::Uuid::new_v4().to_string());
        }
        let request = Request { command, payload };
        if tx.send(request.clone()).is_ok() {
            self.pending = true;
            self.last_request = Some(request);
            self.notice = "Enviando...".into();
        }
    }
    /// Aplica a falha de uma resposta. Só marca "incerto" quando o efeito de
    /// uma ação de escrita é desconhecido (timeout, cancelamento, falha de processo).
    fn apply_error(&mut self, command: &str, error: HelperError) {
        self.right_scroll = 0;
        let mutating = self.last_request.as_ref().is_some_and(|request| {
            request.command == command
                && matches!(
                    request.command,
                    "claim" | "choose" | "agent" | "complete" | "abandon"
                )
        });
        if mutating {
            if matches!(error, HelperError::Uncertain(_)) {
                self.uncertain = true;
                self.notice = format!("{} Resultado incerto. r repete a mesma solicitação; x libera após conferência.", error.message());
                return;
            }
            // Erro definitivo: nada foi aplicado; libera a escolha local.
            self.selected = None;
            self.last_request = None;
            self.uncertain = false;
        }
        if command == "snapshot" || command == "heartbeat" {
            self.offline = true;
        }
        self.notice = error.message().to_owned();
    }
    fn request_quit(&mut self) -> bool {
        true
    }
    fn retry(&mut self, tx: &Sender<Request>) {
        if !self.uncertain || self.pending || self.offline {
            return;
        }
        if let Some(request) = self.last_request.clone() {
            if tx.send(request).is_ok() {
                self.pending = true;
                self.notice = "Consultando o resultado da mesma solicitação...".into();
            }
        }
    }
    fn choose(&mut self, tx: &Sender<Request>, option: usize) {
        if let Some(a) = self.current() {
            let s = &a["suggestions"];
            if s.is_null()
                || s["revision"] != a["revision"]
                || list(s, "options").get(option - 1).is_none()
            {
                self.notice =
                    "Sugestões indisponíveis ou desatualizadas. Aguarde a atualização.".into();
                return;
            }
            let payload = json!({"attendanceId":a["id"],"suggestionId":s["id"],"revision":a["revision"],"option":option});
            let attendance_id = string(a, "id").to_owned();
            let revision = a["revision"].clone();
            self.request(tx, "choose", payload);
            if self.pending {
                self.selected = Some((attendance_id, revision, option));
                if let Some(a) = self
                    .snapshot
                    .get_mut("attendances")
                    .and_then(Value::as_array_mut)
                    .and_then(|items| items.get_mut(self.focus))
                {
                    a["suggestions"] = Value::Null;
                }
            }
        }
    }
    /// Ponto de entrada do teclado: ignora combinações com Ctrl/Alt nas ações.
    /// Ctrl+C só pede para sair (com a mesma confirmação do `q`), nunca age.
    fn handle_key(&mut self, tx: &Sender<Request>, event: KeyEvent) -> bool {
        if self.too_small {
            // Janela pequena: só a saída continua disponível.
            return (matches!(event.code, KeyCode::Char('q'))
                && !event
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                || event.modifiers == KeyModifiers::CONTROL
                    && matches!(event.code, KeyCode::Char('c' | 'C')))
                && self.request_quit();
        }
        if event
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return event.modifiers.contains(KeyModifiers::CONTROL)
                && !event.modifiers.contains(KeyModifiers::ALT)
                && matches!(event.code, KeyCode::Char('c' | 'C'))
                && self.request_quit();
        }
        self.action(tx, event.code)
    }
    fn action(&mut self, tx: &Sender<Request>, key: KeyCode) -> bool {
        if self.abandon_mode > 0 {
            match (self.abandon_mode, key) {
                (_, KeyCode::Esc) => {
                    self.abandon_mode = 0;
                    self.input.clear();
                }
                (1, KeyCode::Down) => {
                    let count = self
                        .current()
                        .map(failed_messages)
                        .map(|m| m.len())
                        .unwrap_or(0);
                    self.abandon_index = (self.abandon_index + 1).min(count.saturating_sub(1));
                }
                (1, KeyCode::Up) => self.abandon_index = self.abandon_index.saturating_sub(1),
                (1, KeyCode::Enter) => {
                    let selection = self.current().and_then(|a| {
                        failed_messages(a)
                            .get(self.abandon_index)
                            .map(|m| (string(m, "id").to_owned(), a["revision"].clone()))
                    });
                    if let Some((id, revision)) = selection {
                        self.abandon_message = id;
                        self.abandon_revision = revision;
                        self.input.clear();
                        self.abandon_mode = 2;
                    }
                }
                (2, KeyCode::Backspace) => {
                    self.input.pop();
                }
                (2, KeyCode::Char(c)) if self.input.chars().count() < 2000 => self.input.push(c),
                (2, KeyCode::Enter) if self.input.trim().chars().count() >= 10 => {
                    self.abandon_mode = 3
                }
                (2, KeyCode::Enter) => {
                    self.notice = "A justificativa precisa ter pelo menos 10 caracteres.".into()
                }
                (3, KeyCode::Enter) => {
                    if let Some(a) = self.current() {
                        if a["revision"] != self.abandon_revision
                            || !failed_messages(a)
                                .iter()
                                .any(|m| string(m, "id") == self.abandon_message)
                        {
                            self.notice =
                                "O atendimento mudou. Selecione a mensagem novamente.".into();
                            self.abandon_mode = 0;
                        } else {
                            let payload = json!({"attendanceId":a["id"],"messageId":self.abandon_message,"revision":self.abandon_revision,"reason":self.input.trim(),"confirm":true});
                            self.request(tx, "abandon", payload);
                            if self.pending {
                                self.abandon_mode = 0;
                                self.input.clear();
                            }
                        }
                    }
                }
                _ => {}
            }
            return false;
        }
        if self.schedule_open {
            if self.schedule_editing {
                match key {
                    KeyCode::Esc | KeyCode::Enter => self.schedule_editing = false,
                    KeyCode::Backspace => {
                        self.schedule_rows[self.schedule_row].pop();
                    }
                    KeyCode::Char(c) if c.is_ascii_digit() || [':', '-', ',', ' '].contains(&c) => {
                        self.schedule_rows[self.schedule_row].push(c)
                    }
                    _ => {}
                }
            } else {
                match key {
                    KeyCode::Esc | KeyCode::Char('h') => self.schedule_open = false,
                    KeyCode::Down => self.schedule_row = (self.schedule_row + 1).min(6),
                    KeyCode::Up => self.schedule_row = self.schedule_row.saturating_sub(1),
                    KeyCode::Enter if self.schedule["canManage"] == true => {
                        self.schedule_editing = true
                    }
                    KeyCode::Char('s') if self.schedule["canManage"] == true => {
                        self.save_schedule(tx)
                    }
                    _ => {}
                }
            }
            return false;
        }
        if self.editing || self.completing {
            match key {
                KeyCode::Esc => {
                    self.editing = false;
                    self.completing = false;
                    self.input.clear();
                }
                KeyCode::Backspace => {
                    self.input.pop();
                }
                KeyCode::Enter if !self.input.trim().is_empty() => {
                    if let Some(a) = self.current() {
                        let command = if self.completing { "complete" } else { "agent" };
                        let mut payload = json!({"attendanceId":a["id"],"revision":a["revision"]});
                        payload[if self.completing { "nextStep" } else { "text" }] =
                            json!(self.input.trim());
                        self.request(tx, command, payload);
                        if self.pending {
                            self.input.clear();
                            self.editing = false;
                            self.completing = false;
                        }
                    }
                }
                KeyCode::Char(c) => self.input.push(c),
                _ => {}
            }
            return false;
        }
        match key {
            KeyCode::Char('q') => return self.request_quit(),
            KeyCode::Char('r') => self.retry(tx),
            KeyCode::Char('x') if self.uncertain => {
                self.uncertain = false;
                self.last_request = None;
                self.notice = "Confira o atendimento antes de uma nova ação.".into();
            }
            KeyCode::Char('b') => {
                if self
                    .current()
                    .is_some_and(|a| !failed_messages(a).is_empty())
                {
                    self.abandon_mode = 1;
                    self.abandon_index = 0;
                } else {
                    self.notice = "Nenhum envio falho ou incerto neste atendimento.".into();
                    self.right_scroll = 0;
                }
            }
            KeyCode::Right => self.right_scroll = self.right_scroll.saturating_add(3),
            KeyCode::Left => self.right_scroll = self.right_scroll.saturating_sub(3),
            KeyCode::Char('h') => {
                self.schedule_open = true;
                self.request(tx, "schedule", json!({}));
            }
            KeyCode::PageDown | KeyCode::Down => {
                self.scroll[self.focus.min(1)] = self.scroll[self.focus.min(1)].saturating_add(3);
            }
            KeyCode::PageUp | KeyCode::Up => {
                self.scroll[self.focus.min(1)] = self.scroll[self.focus.min(1)].saturating_sub(3);
            }
            KeyCode::Tab => {
                if self.attendances().len() > 1 {
                    self.focus = 1 - self.focus.min(1);
                    self.sync_focus();
                }
            }
            KeyCode::Char('1'..='3') => {
                if let KeyCode::Char(c) = key {
                    self.choose(tx, c as usize - '0' as usize);
                }
            }
            KeyCode::Char('4') => {
                if self.current().is_some() {
                    self.right_scroll = 0;
                    self.editing = true;
                }
            }
            KeyCode::Char('f') => {
                if self.current().is_some() {
                    self.right_scroll = 0;
                    self.completing = true;
                }
            }
            KeyCode::Char('a') => self.request(tx, "presence", json!({"state":"available"})),
            KeyCode::Char('p') => self.request(tx, "presence", json!({"state":"paused"})),
            KeyCode::Char('c') => self.request(tx, "claim", json!({})),
            _ => {}
        }
        false
    }
}

const MIN_WIDTH: u16 = 85;
const MIN_HEIGHT: u16 = 24;
fn is_too_small(width: u16, height: u16) -> bool {
    width < MIN_WIDTH || height < MIN_HEIGHT
}

fn layout(area: Rect) -> Vec<Rect> {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(22),
            Constraint::Percentage(52),
            Constraint::Percentage(26),
        ])
        .split(area)
        .to_vec()
}
fn box_text<'a>(title: &'a str, lines: Vec<Line<'a>>, active: bool) -> Paragraph<'a> {
    Paragraph::new(lines)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if active {
                    Color::Cyan
                } else {
                    Color::DarkGray
                })),
        )
        .wrap(Wrap { trim: false })
}
fn draw(frame: &mut Frame, s: &State) {
    let area = frame.area();
    if is_too_small(area.width, area.height) {
        frame.render_widget(Paragraph::new("Amplie a janela para pelo menos 85 colunas e 24 linhas. Os dados permanecem disponíveis." ).wrap(Wrap { trim: true }), area);
        return;
    }
    let cols = layout(area);
    let profile = &s.snapshot["profile"];
    let mut left = vec![
        Line::from("Centauro CX"),
        Line::from(format!(
            "{} · {}",
            string(profile, "name"),
            string(profile, "title")
        )),
        Line::from(string(profile, "userName").to_owned()),
        Line::from(""),
        Line::from(format!(
            "Disponibilidade: {}",
            match string(&s.snapshot["presence"], "state") {
                "available" => "disponível",
                "paused" => "pausada",
                _ => "desconhecida",
            }
        )),
        Line::from("a disponível · p pausar"),
        Line::from(""),
        Line::from("Fila · c assumir próximo"),
    ];
    if !string(&s.snapshot, "notice").is_empty() {
        left.push(Line::styled(
            string(&s.snapshot, "notice").to_owned(),
            Style::default().fg(Color::Yellow),
        ));
    }
    if list(&s.snapshot, "queue").is_empty() && string(&s.snapshot, "notice").is_empty() {
        left.push(Line::from("Nenhum cliente na fila"));
    }
    for item in list(&s.snapshot, "queue") {
        left.push(Line::from(format!(
            "{} {}",
            string(item, "customerName"),
            string(item, "priority")
        )));
    }
    left.push(Line::from(""));
    left.push(Line::from("h jornada · Tab alterna"));
    left.push(Line::from("↑↓ rolam · r retoma · q sair"));
    frame.render_widget(box_text("Equipe", left, false), cols[0]);
    let halves = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(cols[1]);
    for (i, rect) in halves.iter().enumerate() {
        let mut lines = Vec::new();
        if let Some(a) = s.attendances().get(i) {
            lines.push(Line::from(format!(
                "{} · {}{}",
                string(a, "customerName"),
                string(a, "state"),
                if a["audioOnly"] == true {
                    " · somente áudio"
                } else {
                    ""
                }
            )));
            for m in list(a, "messages") {
                lines.push(Line::from(format!(
                    "{}: {} [{}]",
                    string(m, "author"),
                    string(m, "text"),
                    message_status(string(m, "status"))
                )));
            }
        } else {
            lines.push(Line::from("Nenhum atendimento nesta posição"));
        }
        frame.render_widget(
            box_text(
                if i == 0 {
                    "Atendimento 1"
                } else {
                    "Atendimento 2"
                },
                lines,
                i == s.focus && s.current().is_some(),
            )
            .scroll((s.scroll[i], 0)),
            *rect,
        );
    }
    let right_parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(1)])
        .split(cols[2]);
    let audio = s.current().is_some_and(|a| a["audioOnly"] == true);
    let controls = vec![
        Line::from(if audio {
            "1 aprovar roteiro 1"
        } else {
            "1 enviar resposta 1"
        }),
        Line::from(if audio {
            "2 aprovar roteiro 2"
        } else {
            "2 enviar resposta 2"
        }),
        Line::from(if audio {
            "3 aprovar roteiro 3"
        } else {
            "3 enviar resposta 3"
        }),
        Line::from("4 escrever ao agente"),
        Line::from("f finalizar"),
        Line::from("b resolver envio"),
        Line::from("←→ rolar sugestões"),
    ];
    frame.render_widget(box_text("Ações", controls, false), right_parts[0]);
    let mut right = Vec::new();
    if s.offline {
        right.push(Line::styled(
            "Sem conexão. Ações indisponíveis.",
            Style::default().fg(Color::Red),
        ));
    }
    if !s.notice.is_empty() {
        right.push(Line::styled(
            s.notice.clone(),
            Style::default().fg(Color::Yellow),
        ));
    }
    if s.editing || s.completing {
        right.push(Line::from(if s.completing {
            "Próximo passo:"
        } else {
            "Mensagem privada:"
        }));
        right.push(Line::from(s.input.clone()));
        right.push(Line::from("Enter envia · Esc cancela"));
    }
    if let Some(a) = s.current() {
        right.push(Line::from(format!(
            "Cliente: {}",
            string(a, "customerName")
        )));
        if !string(a, "notice").is_empty() {
            right.push(Line::from(string(a, "notice").to_owned()));
        }
        let suggestions = &a["suggestions"];
        let analysis = string(suggestions, "explanation");
        if !analysis.is_empty() && suggestions["revision"] == a["revision"] {
            right.push(Line::from(format!("Análise: {analysis}")));
        }
        if let Some((id, revision, option)) = &s.selected {
            if id == string(a, "id") && *revision == a["revision"] {
                right.push(Line::from(format!("◉ Resposta {option} escolhida")));
            }
        }
        if suggestions.is_null() || suggestions["revision"] != a["revision"] {
            right.push(Line::from("Aguardando sugestões atualizadas"));
        } else {
            for (n, o) in list(suggestions, "options").iter().take(3).enumerate() {
                right.push(Line::from(vec![
                    Span::styled(
                        format!(
                            "({}){} ",
                            n + 1,
                            if o["recommended"] == true {
                                " recomendada"
                            } else {
                                ""
                            }
                        ),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(string(o, "text").to_owned()),
                ]));
            }
        }
        if audio {
            right.push(Line::from("As opções são roteiros para áudio"));
            if let Some((id, script)) = &s.audio_script {
                if id == string(a, "id") {
                    right.push(Line::from(format!("Roteiro aprovado: {script}")));
                }
            }
        }
        right.push(Line::from("Conversa privada com o agente:"));
        for m in list(a, "agentMessages") {
            right.push(Line::from(format!(
                "{}: {}",
                string(m, "author"),
                string(m, "text")
            )));
        }
    } else {
        right.push(Line::from("Selecione um atendimento"));
    }
    frame.render_widget(
        box_text("Análise e sugestões", right, false).scroll((s.right_scroll, 0)),
        right_parts[1],
    );
    if s.abandon_mode > 0 {
        let area = Rect::new(
            area.x + area.width / 6,
            area.y + 2,
            area.width * 2 / 3,
            area.height.saturating_sub(4),
        );
        let mut lines = vec![
            Line::from("Entrega não confirmada; sem reenvio automático."),
            Line::from("O próximo passo será decidido com novas sugestões."),
        ];
        if s.abandon_mode == 1 {
            lines.push(Line::from("Selecione o envio · ↑↓ e Enter · Esc cancela"));
            if let Some(a) = s.current() {
                let messages = failed_messages(a);
                let capacity = area.height.saturating_sub(7) as usize;
                let start = s.abandon_index.saturating_sub(capacity.saturating_sub(1));
                for (i, m) in messages.iter().enumerate().skip(start).take(capacity) {
                    lines.push(Line::from(format!(
                        "{} {}: {} [{}]",
                        if i == s.abandon_index { ">" } else { " " },
                        i + 1,
                        string(m, "text").chars().take(20).collect::<String>(),
                        message_status(string(m, "status"))
                    )));
                }
            }
        } else if s.abandon_mode == 2 {
            lines.push(Line::from(
                "Justificativa (mínimo 10 caracteres) · Enter continua",
            ));
            lines.push(Line::from(s.input.clone()));
            lines.push(Line::from(s.notice.clone()));
        } else {
            lines.push(Line::from(format!("Justificativa: {}", s.input)));
            lines.push(Line::from(
                "Enter confirma encerrar acompanhamento · Esc cancela",
            ));
            lines.push(Line::from(
                "Entrega não confirmada. Não reenviar automaticamente.",
            ));
        }
        frame.render_widget(ratatui::widgets::Clear, area);
        frame.render_widget(box_text("Envio sem confirmação", lines, true), area);
        if area.height > 3 {
            let button = Rect::new(
                area.x + 2,
                area.bottom() - 2,
                area.width.saturating_sub(4),
                1,
            );
            frame.render_widget(
                Paragraph::new(match s.abandon_mode {
                    1 => "[ Selecionar ]",
                    2 => "[ Continuar ]",
                    _ => "[ Confirmar encerramento ]",
                })
                .style(Style::default().fg(Color::Cyan)),
                button,
            );
        }
    }
    if s.schedule_open {
        let area = Rect::new(
            area.x + area.width / 6,
            area.y + 2,
            area.width * 2 / 3,
            area.height.saturating_sub(4),
        );
        let mut lines = vec![
            Line::from(format!("Fuso: {}", string(&s.schedule, "timezone"))),
            Line::from("Dia           Horários (vazio = fechado)"),
        ];
        for (i, day) in [
            "Domingo", "Segunda", "Terça", "Quarta", "Quinta", "Sexta", "Sábado",
        ]
        .iter()
        .enumerate()
        {
            lines.push(Line::styled(
                format!(
                    "{} {:<12} {}",
                    if i == s.schedule_row { ">" } else { " " },
                    day,
                    if s.schedule_rows[i].is_empty() {
                        "fechado"
                    } else {
                        &s.schedule_rows[i]
                    }
                ),
                if i == s.schedule_row {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            ));
        }
        lines.push(Line::from(if s.schedule["canManage"] == true {
            "↑↓ selecionar · Enter editar · s salvar · Esc fechar"
        } else {
            "Consulta somente · Esc fechar"
        }));
        if s.schedule_editing {
            lines.push(Line::from(
                "Digite HH:MM-HH:MM; separe intervalos por vírgula.",
            ));
        }
        if !s.notice.is_empty() {
            lines.push(Line::from(s.notice.clone()));
        }
        frame.render_widget(ratatui::widgets::Clear, area);
        frame.render_widget(box_text("Jornada de atendimento", lines, true), area);
    }
}

pub fn run() -> io::Result<()> {
    let (tx, rx) = mpsc::channel();
    let (replies, results) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel);
    let worker = thread::spawn(move || worker(rx, replies, worker_cancel));
    enable_raw_mode()?;
    let mut out = io::stdout();
    if let Err(e) = execute!(out, EnterAlternateScreen, EnableMouseCapture) {
        let _ = disable_raw_mode();
        return Err(e);
    }
    let result = (|| {
        let mut terminal = Terminal::new(CrosstermBackend::new(out))?;
        let mut state = State {
            offline: true,
            ..State::default()
        };
        state.notice = "Conectando...".into();
        loop {
            while let Ok(reply) = results.try_recv() {
                if reply.command != "snapshot" && reply.command != "heartbeat" {
                    state.pending = false;
                }
                match reply.result {
                    Ok(value) => {
                        if reply.command != "snapshot" && reply.command != "heartbeat" {
                            state.uncertain = false;
                        }
                        if reply.command == "snapshot" {
                            state.offline = false;
                            state.update(value);
                        } else if reply.command == "schedule" {
                            state.load_schedule(value);
                            state.notice.clear();
                        } else if reply.command == "schedule-save" {
                            state.schedule_open = false;
                            state.notice = "Jornada salva.".into();
                        } else if reply.command == "abandon" {
                            state.right_scroll = 0;
                            state.notice =
                                "Acompanhamento encerrado. Entrega não confirmada; sem reenvio."
                                    .into();
                        } else if reply.command != "heartbeat" {
                            if reply.command == "choose" && value["script"] == true {
                                if let Some(request) = &state.last_request {
                                    state.audio_script = Some((
                                        string(&request.payload, "attendanceId").to_owned(),
                                        string(&value, "text").to_owned(),
                                    ));
                                }
                                state.notice = "Roteiro aprovado; grave o áudio na estação.".into();
                            } else {
                                state.notice = "Concluído.".into();
                            }
                        }
                    }
                    Err(error) => state.apply_error(reply.command, error),
                }
            }
            let size = terminal.size()?;
            state.too_small = is_too_small(size.width, size.height);
            terminal.draw(|f| draw(f, &state))?;
            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(k) if k.kind == KeyEventKind::Press => {
                        if state.handle_key(&tx, k) {
                            break;
                        }
                    }
                    Event::Mouse(m)
                        if matches!(
                            m.kind,
                            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
                        ) =>
                    {
                        if state.too_small {
                            continue;
                        }
                        let size = terminal.size()?;
                        let cols = layout(Rect::new(0, 0, size.width, size.height));
                        let down = m.kind == MouseEventKind::ScrollDown;
                        let key = if m.column >= cols[2].x {
                            if down {
                                KeyCode::Right
                            } else {
                                KeyCode::Left
                            }
                        } else if down {
                            KeyCode::Down
                        } else {
                            KeyCode::Up
                        };
                        state.action(&tx, key);
                    }
                    Event::Mouse(m) if m.kind == MouseEventKind::Down(event::MouseButton::Left) => {
                        let size = terminal.size()?;
                        let area = Rect::new(0, 0, size.width, size.height);
                        if !is_too_small(area.width, area.height) {
                            if state.abandon_mode > 0 {
                                let dialog = Rect::new(
                                    area.x + area.width / 6,
                                    area.y + 2,
                                    area.width * 2 / 3,
                                    area.height.saturating_sub(4),
                                );
                                if state.abandon_mode == 1
                                    && m.row >= dialog.y + 4
                                    && m.row < dialog.bottom().saturating_sub(2)
                                    && m.column > dialog.x
                                    && m.column < dialog.right()
                                {
                                    let capacity = dialog.height.saturating_sub(7) as usize;
                                    let start = state
                                        .abandon_index
                                        .saturating_sub(capacity.saturating_sub(1));
                                    let index = start + (m.row - dialog.y - 4) as usize;
                                    if state
                                        .current()
                                        .is_some_and(|a| index < failed_messages(a).len())
                                    {
                                        state.abandon_index = index;
                                    }
                                }
                                if m.row == dialog.bottom().saturating_sub(2)
                                    && m.column > dialog.x
                                    && m.column < dialog.right()
                                {
                                    state.action(&tx, KeyCode::Enter);
                                }
                                continue;
                            }
                            if state.schedule_open {
                                let top = area.y + 2;
                                if m.row >= top + 3 && m.row <= top + 9 {
                                    state.schedule_row = (m.row - top - 3) as usize;
                                    if state.schedule["canManage"] == true {
                                        state.schedule_editing = true;
                                    }
                                }
                                continue;
                            }
                            let cols = layout(area);
                            if m.column > cols[2].x
                                && m.column < cols[2].right()
                                && !state.editing
                                && !state.completing
                                && state.abandon_mode == 0
                            {
                                let key = match m.row {
                                    1 => Some('1'),
                                    2 => Some('2'),
                                    3 => Some('3'),
                                    4 => Some('4'),
                                    5 => Some('f'),
                                    6 => Some('b'),
                                    _ => None,
                                };
                                if let Some(key) = key {
                                    state.action(&tx, KeyCode::Char(key));
                                }
                            }
                            if state.abandon_mode == 0
                                && m.column >= cols[1].x
                                && m.column < cols[1].right()
                            {
                                let i = usize::from(m.row >= area.height / 2);
                                if state.attendances().get(i).is_some() {
                                    state.focus = i;
                                    state.sync_focus();
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        let _ = write!(io::stdout(), "\x1b]777;pretorian-cx;{{\"id\":null}}\x1b\\");
        Ok(())
    })();
    cancel.store(true, Ordering::Relaxed);
    drop(tx);
    let _ = worker.join();
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> State {
        let mut s = State::default();
        s.update(json!({"attendances":[{"id":"a","customerName":"Ana","revision":1,"suggestions":{"id":"s","revision":1,"options":[{"text":"um"},{"text":"dois"},{"text":"três"}]}},{"id":"b","customerName":"Bia","revision":2,"suggestions":{"id":"t","revision":1,"options":[{"text":"velha"}]}}]}));
        s
    }
    #[test]
    fn choice_targets_focused_revision_and_rejects_stale() {
        let (tx, rx) = mpsc::channel();
        let mut s = state();
        s.action(&tx, KeyCode::Char('2'));
        let r = rx.try_recv().expect("choice");
        assert_eq!(r.command, "choose");
        assert_eq!(r.payload["attendanceId"], "a");
        assert_eq!(r.payload["option"], 2);
        s.pending = false;
        s.action(&tx, KeyCode::Tab);
        s.action(&tx, KeyCode::Char('1'));
        assert!(rx.try_recv().is_err());
        assert!(s.notice.contains("desatualizadas"));
    }
    #[test]
    fn private_message_never_chooses_customer_response() {
        let (tx, rx) = mpsc::channel();
        let mut s = state();
        s.action(&tx, KeyCode::Char('4'));
        s.action(&tx, KeyCode::Char('1'));
        s.action(&tx, KeyCode::Enter);
        let r = rx.try_recv().expect("private message");
        assert_eq!(r.command, "agent");
        assert_eq!(r.payload["text"], "1");
    }
    #[test]
    fn shrinking_terminal_keeps_data_and_renders_warning() {
        let s = state();
        let backend = ratatui::backend::TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| draw(f, &s)).expect("draw");
        assert_eq!(s.attendances().len(), 2);
        assert!(format!("{:?}", terminal.backend().buffer()).contains("Amplie"));
    }
    #[test]
    fn schedule_save_preserves_revision_and_closed_dates() {
        let (tx, rx) = mpsc::channel();
        let mut s = State::default();
        s.load_schedule(json!({"revision":4,"timezone":"America/Fortaleza","closedDates":["2026-12-25"],"canManage":true,"periods":[{"day":1,"start":"07:00","end":"17:00"}]}));
        s.schedule_rows[6] = "08:00-11:00".into();
        s.save_schedule(&tx);
        let r = rx.try_recv().expect("schedule");
        assert_eq!(r.command, "schedule-save");
        assert_eq!(r.payload["revision"], 4);
        assert_eq!(r.payload["closedDates"][0], "2026-12-25");
        assert_eq!(r.payload["periods"][1]["day"], 6);
    }
    #[test]
    fn long_options_can_be_read_and_render_is_inspectable() {
        let mut s = state();
        s.snapshot["profile"] =
            json!({"name":"Centauro CX","title":"Support Hero","userName":"Pessoa de teste"});
        s.snapshot["presence"] = json!({"state":"available"});
        s.snapshot["attendances"][0]["messages"] = json!([{"id":"message-a","author":"Ana","text":"Preciso de ajuda","status":"received"}]);
        s.snapshot["attendances"][1]["messages"] =
            json!([{"id":"message-b","author":"Bia","text":"Qual o prazo?","status":"received"}]);
        s.snapshot["attendances"][0]["suggestions"]["options"] = json!([
            {"text": "PRIMEIRA resposta longa que explica todos os detalhes ao cliente e continua em outra linha, com informações suficientes para uma escolha consciente.", "recommended":true},
            {"text": "SEGUNDA resposta longa que oferece uma alternativa e continua em outra linha, mantendo o texto completo para leitura antes da escolha."},
            {"text": "TERCEIRA resposta longa que esclarece o próximo passo e também precisa ser lida completamente sem perda de conteúdo."}
        ]);
        let mut terminal =
            Terminal::new(ratatui::backend::TestBackend::new(120, 36)).expect("terminal");
        let mut capture = String::new();
        for scroll in [0, 8, 16, 24] {
            s.right_scroll = scroll;
            terminal.draw(|f| draw(f, &s)).expect("draw");
            capture.push_str(&format!("\n--- rolagem {scroll} ---\n"));
            let buffer = terminal.backend().buffer();
            for y in 0..36 {
                for x in 0..120 {
                    capture.push_str(buffer[(x, y)].symbol());
                }
                capture.push('\n');
            }
        }
        assert!(capture.contains("PRIMEIRA"));
        assert!(capture.contains("SEGUNDA"));
        assert!(capture.contains("TERCEIRA"));
        assert!(capture.contains("recomendada"));
        assert!(capture.contains("Preciso de ajuda"));
        assert!(capture.contains("Qual o prazo?"));
        for line in capture.lines().filter(|line| {
            line.contains("PRIMEIRA") || line.contains("SEGUNDA") || line.contains("TERCEIRA")
        }) {
            let pos = ["PRIMEIRA", "SEGUNDA", "TERCEIRA"]
                .iter()
                .filter_map(|word| line.find(word))
                .min()
                .expect("option");
            assert!(line[..pos].chars().count() > 80);
        }
        s.schedule_open = true;
        s.load_schedule(json!({"revision":1,"timezone":"America/Fortaleza","closedDates":[],"canManage":true,"periods":[{"day":1,"start":"07:00","end":"17:00"},{"day":6,"start":"08:00","end":"11:00"}]}));
        terminal.draw(|f| draw(f, &s)).expect("schedule draw");
        capture.push_str("\n--- jornada ---\n");
        let buffer = terminal.backend().buffer();
        for y in 0..36 {
            for x in 0..120 {
                capture.push_str(buffer[(x, y)].symbol());
            }
            capture.push('\n');
        }
        assert!(capture.contains("America/Fortaleza"));
        assert!(capture.contains("07:00-17:00"));
        assert!(capture.contains("08:00-11:00"));
        s.schedule_open = false;
        s.abandon_mode = 3;
        s.input = "Cliente não confirmou".into();
        terminal.draw(|f| draw(f, &s)).expect("confirmation draw");
        capture.push_str("\n--- encerramento de acompanhamento ---\n");
        let buffer = terminal.backend().buffer();
        for y in 0..36 {
            for x in 0..120 {
                capture.push_str(buffer[(x, y)].symbol());
            }
            capture.push('\n');
        }
        assert!(capture.contains("Entrega não confirmada"));
        assert!(capture.contains("Não reenviar automaticamente"));
        print!("{capture}");
    }
    #[test]
    fn retry_preserves_request_id() {
        let (tx, rx) = mpsc::channel();
        let mut s = state();
        s.action(&tx, KeyCode::Char('2'));
        let first = rx.try_recv().expect("first");
        s.pending = false;
        s.uncertain = true;
        s.action(&tx, KeyCode::Char('r'));
        let retry = rx.try_recv().expect("retry");
        assert_eq!(first.payload["requestId"], retry.payload["requestId"]);
        assert_eq!(first.payload, retry.payload);
    }
    #[test]
    fn cancellation_stops_auxiliary_process() {
        let path = std::env::current_dir()
            .expect("cwd")
            .join("target/cx-test-sleep.py");
        std::fs::write(&path, "import time\ntime.sleep(60)\n").expect("fixture");
        let previous_python = std::env::var_os("PRETORIAN_CX_PYTHON");
        let previous_helper = std::env::var_os("PRETORIAN_CX_HELPER");
        std::env::set_var(
            "PRETORIAN_CX_PYTHON",
            if cfg!(windows) { "python" } else { "python3" },
        );
        std::env::set_var("PRETORIAN_CX_HELPER", &path);
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&cancel);
        let started = Instant::now();
        let thread = thread::spawn(move || {
            helper(
                "snapshot",
                &json!({"payload":"x".repeat(1_000_000)}),
                &signal,
            )
        });
        thread::sleep(Duration::from_millis(150));
        cancel.store(true, Ordering::Relaxed);
        assert!(matches!(
            thread.join().expect("join"),
            Err(HelperError::Uncertain(_))
        ));
        assert!(started.elapsed() < Duration::from_secs(3));
        if let Some(v) = previous_python {
            std::env::set_var("PRETORIAN_CX_PYTHON", v);
        } else {
            std::env::remove_var("PRETORIAN_CX_PYTHON");
        }
        if let Some(v) = previous_helper {
            std::env::set_var("PRETORIAN_CX_HELPER", v);
        } else {
            std::env::remove_var("PRETORIAN_CX_HELPER");
        }
        std::fs::remove_file(path).expect("remove fixture");
    }
    #[test]
    fn abandoning_requires_reason_confirmation_and_current_revision() {
        let (tx, rx) = mpsc::channel();
        let mut s = state();
        s.snapshot["attendances"][0]["messages"] =
            json!([{"id":"message-a","status":"failed","text":"Envio A"}]);
        s.snapshot["attendances"][1]["messages"] =
            json!([{"id":"message-b","status":"uncertain","text":"Envio B"}]);
        s.action(&tx, KeyCode::Char('b'));
        s.action(&tx, KeyCode::Enter);
        for c in "Cliente não confirmou".chars() {
            s.action(&tx, KeyCode::Char(c));
        }
        s.action(&tx, KeyCode::Enter);
        assert!(rx.try_recv().is_err());
        let mut updated = s.snapshot.clone();
        updated["attendances"]
            .as_array_mut()
            .expect("list")
            .swap(0, 1);
        s.update(updated);
        assert_eq!(s.focus_id, "a");
        s.action(&tx, KeyCode::Enter);
        let r = rx.try_recv().expect("abandon");
        assert_eq!(r.command, "abandon");
        assert_eq!(r.payload["attendanceId"], "a");
        assert_eq!(r.payload["messageId"], "message-a");
        assert_eq!(r.payload["revision"], 1);
        assert_eq!(r.payload["confirm"], true);
        s.pending = false;
        s.action(&tx, KeyCode::Tab);
        s.action(&tx, KeyCode::Char('b'));
        s.action(&tx, KeyCode::Enter);
        for c in "Verificação pendente".chars() {
            s.action(&tx, KeyCode::Char(c));
        }
        s.action(&tx, KeyCode::Enter);
        s.snapshot["attendances"][0]["revision"] = json!(3);
        s.action(&tx, KeyCode::Enter);
        assert!(rx.try_recv().is_err());
        assert!(s.notice.contains("mudou"));
    }
    fn key(c: char, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), modifiers)
    }
    #[test]
    fn control_and_alt_keys_never_trigger_actions() {
        let (tx, rx) = mpsc::channel();
        let mut s = state();
        for c in ['c', 'a', 'p', '1', '2', '3', '4', 'f', 'h', 'b', 'q'] {
            for m in [
                KeyModifiers::CONTROL,
                KeyModifiers::ALT,
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ] {
                let quit = s.handle_key(&tx, key(c, m));
                assert!(!quit || (c == 'c' && m == KeyModifiers::CONTROL));
            }
        }
        assert!(rx.try_recv().is_err());
        assert!(!s.pending && !s.editing && !s.completing && !s.schedule_open);
        assert_eq!(s.abandon_mode, 0);
    }
    #[test]
    fn ctrl_c_quits_instead_of_claiming() {
        let (tx, rx) = mpsc::channel();
        let mut s = state();
        assert!(s.handle_key(&tx, key('c', KeyModifiers::CONTROL)));
        assert!(rx.try_recv().is_err());
        assert!(!s.handle_key(&tx, key('c', KeyModifiers::NONE)));
        assert_eq!(rx.try_recv().expect("claim").command, "claim");
    }
    #[test]
    fn small_window_blocks_keyboard_actions_but_allows_quit() {
        let (tx, rx) = mpsc::channel();
        let mut s = state();
        s.too_small = true;
        for c in ['c', 'a', 'p', '1', '2', '3', '4', 'f', 'h', 'b', 'r', 'x'] {
            assert!(!s.handle_key(&tx, key(c, KeyModifiers::NONE)));
        }
        assert!(!s.handle_key(&tx, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));
        assert!(!s.handle_key(&tx, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(rx.try_recv().is_err());
        assert!(!s.editing && !s.completing && !s.schedule_open && !s.pending);
        assert_eq!(s.focus, 0);
        assert!(s.handle_key(&tx, key('q', KeyModifiers::NONE)));
        assert!(s.handle_key(&tx, key('c', KeyModifiers::CONTROL)));
        s.too_small = false;
        s.handle_key(&tx, key('c', KeyModifiers::NONE));
        assert_eq!(rx.try_recv().expect("claim").command, "claim");
    }
    #[test]
    fn minimum_size_is_85_by_24() {
        assert!(is_too_small(84, 24));
        assert!(is_too_small(85, 23));
        assert!(!is_too_small(85, 24));
    }
    #[test]
    fn json_error_with_normal_exit_is_definitive() {
        assert_eq!(
            parse_output(true, json!({"error":"Sem permissão"})),
            Err(HelperError::Definitive("Sem permissão".into()))
        );
        assert_eq!(
            parse_output(true, json!({"ok":false})),
            Err(HelperError::Definitive("Serviço indisponível".into()))
        );
        assert!(matches!(
            parse_output(false, json!({"error":"caiu"})),
            Err(HelperError::Uncertain(_))
        ));
        assert_eq!(
            parse_output(true, json!({"data":{"x":1}})),
            Ok(json!({"x":1}))
        );
    }
    #[test]
    fn definitive_error_does_not_mark_uncertain_but_failure_does() {
        let (tx, rx) = mpsc::channel();
        let mut s = state();
        s.action(&tx, KeyCode::Char('2'));
        rx.try_recv().expect("choice");
        s.pending = false;
        s.apply_error("choose", HelperError::Definitive("Revisão antiga".into()));
        assert!(!s.uncertain);
        assert!(s.selected.is_none());
        assert_eq!(s.notice, "Revisão antiga");
        assert!(!s.notice.contains("incerto"));
        s.action(&tx, KeyCode::Char('c'));
        assert_eq!(rx.try_recv().expect("nova ação liberada").command, "claim");
        s.pending = false;
        s.apply_error("claim", HelperError::Uncertain("Tempo esgotado.".into()));
        assert!(s.uncertain);
        assert!(s.notice.contains("Resultado incerto"));
        s.action(&tx, KeyCode::Char('c'));
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn read_only_failures_never_mark_uncertain() {
        let mut s = state();
        s.apply_error("snapshot", HelperError::Uncertain("Tempo esgotado.".into()));
        assert!(!s.uncertain);
        assert!(s.offline);
    }
    #[test]
    fn queue_notice_replaces_empty_queue_message() {
        let mut s = state();
        s.snapshot["enabled"] = json!(false);
        s.snapshot["notice"] = json!("Fila indisponível");
        let mut terminal =
            Terminal::new(ratatui::backend::TestBackend::new(120, 36)).expect("terminal");
        terminal.draw(|f| draw(f, &s)).expect("draw");
        let buffer = terminal.backend().buffer();
        let mut output = String::new();
        for y in 0..36 {
            for x in 0..120 {
                output.push_str(buffer[(x, y)].symbol());
            }
            output.push('\n');
        }
        assert!(output.contains("Fila indisponível"));
        assert!(!output.contains("Nenhum cliente na fila"));
        assert!(!output.contains("indisponível para este perfil"));
    }
}
