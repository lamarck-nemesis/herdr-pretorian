use super::{ClientShellOverlay, ClientShellState, TextEditor};
use serde::Deserialize;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(super) const LIMIT: usize = 4000;
static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct State {
    version: u8,
    revision: u64,
    text: String,
    status: String,
    message: String,
    request_id: Option<String>,
}

pub(super) struct SobreVoce {
    pub editor: TextEditor,
    pub revision: u64,
    pub status: String,
    pub message: String,
    path: Option<PathBuf>,
    baseline: String,
    directory_identity: Option<String>,
    initialized: bool,
    pending: Option<(String, Option<String>)>,
    last_poll: Option<Instant>,
}

impl std::fmt::Debug for SobreVoce {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SobreVoce")
            .field("status", &self.status)
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}

impl Default for SobreVoce {
    fn default() -> Self {
        Self::at(None)
    }
}

impl SobreVoce {
    pub fn new() -> Self {
        Self::at(
            std::env::var_os("PRETORIAN_SOBRE_VOCE")
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
        )
    }

    fn at(path: Option<PathBuf>) -> Self {
        let directory_identity = path
            .as_deref()
            .and_then(|p| crate::platform::private_bridge_identity(p).ok());
        let mut value = Self {
            editor: TextEditor::multiline(""),
            revision: 0,
            status: if path.is_some() {
                "loading"
            } else {
                "unavailable"
            }
            .into(),
            message: String::new(),
            path,
            baseline: String::new(),
            directory_identity,
            initialized: false,
            pending: None,
            last_poll: None,
        };
        value.poll(Instant::now());
        value
    }

    pub fn open(&mut self) {
        if !self.initialized && self.pending.is_none() {
            self.request(false);
        }
    }

    pub fn dirty(&self) -> bool {
        self.editor.as_str() != self.baseline
    }
    pub fn can_save(&self) -> bool {
        self.initialized
            && self.pending.is_none()
            && !matches!(
                self.status.as_str(),
                "unavailable" | "loading" | "saving" | "conflict"
            )
    }

    pub fn feedback(&self) -> &str {
        if !self.message.is_empty() {
            return &self.message;
        }
        match self.status.as_str() {
            "loading" => "Carregando…",
            "saving" => "Salvando… Você pode continuar escrevendo.",
            "saved" if !self.dirty() => "Salvo.",
            "conflict" => "Alterado em outra conversa. Recarregue antes de salvar novamente.",
            "unavailable" => "Indisponível nesta sessão. Reabra pelo Pretorian autenticado.",
            "error" => "Não foi possível concluir. Tente novamente quando quiser.",
            _ if self.dirty() => "Alterações ainda não salvas.",
            _ => "Opcional. Edite ou apague quando quiser.",
        }
    }

    fn directory(&self) -> std::io::Result<&std::path::Path> {
        let path = self
            .path
            .as_deref()
            .ok_or_else(|| std::io::Error::other("Ponte ausente."))?;
        if !path.is_absolute()
            || path
                .components()
                .any(|part| part == std::path::Component::ParentDir)
        {
            return Err(std::io::Error::other("Ponte inválida."));
        }
        if Some(crate::platform::private_bridge_identity(path)?) != self.directory_identity {
            return Err(std::io::Error::other("Ponte substituída."));
        }
        Ok(path)
    }

    fn read_state(&self) -> std::io::Result<State> {
        let path = self.directory()?.join("estado.json");
        crate::platform::validate_private_bridge(&path, false)?;
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(65537)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 65536 {
            return Err(std::io::Error::other("Estado excede limite."));
        }
        let state: State = serde_json::from_slice(&bytes)?;
        if state
            .text
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
            || state.version != 1
            || state.text.chars().count() > LIMIT
            || state.message.chars().count() > 500
            || ![
                "loading",
                "ready",
                "saving",
                "saved",
                "error",
                "conflict",
                "unavailable",
            ]
            .contains(&state.status.as_str())
        {
            return Err(std::io::Error::other("Estado inválido."));
        }
        Ok(state)
    }

    pub fn poll(&mut self, now: Instant) -> bool {
        if self.path.is_none()
            || self
                .last_poll
                .is_some_and(|last| now.duration_since(last) < Duration::from_millis(300))
        {
            return false;
        }
        self.last_poll = Some(now);
        match self.read_state() {
            Ok(state) => self.receive(state),
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && !self.initialized
                    && self.directory().is_ok() =>
            {
                false
            }
            Err(_) => {
                let changed = self.status != "unavailable";
                self.unavailable();
                changed
            }
        }
    }

    fn unavailable(&mut self) {
        self.editor.clear();
        self.baseline.clear();
        self.revision = 0;
        self.initialized = false;
        self.pending = None;
        self.status = "unavailable".into();
        self.message.clear();
    }

    fn receive(&mut self, state: State) -> bool {
        if state.status == "unavailable" {
            self.unavailable();
            return true;
        }
        if let Some((id, _)) = &self.pending {
            if state.request_id.as_deref() != Some(id) {
                return false;
            }
        } else if self.initialized || state.request_id.is_some() {
            return false;
        }
        if matches!(state.status.as_str(), "ready" | "saved") {
            let dirty = self.dirty();
            if state.status == "saved" {
                self.baseline = self
                    .pending
                    .as_ref()
                    .and_then(|(_, text)| text.clone())
                    .unwrap_or_else(|| state.text.clone());
            } else {
                self.baseline = state.text.clone();
                if !dirty {
                    self.editor = TextEditor::multiline(&state.text);
                }
            }
            self.revision = state.revision;
            self.initialized = true;
        }
        self.status = state.status;
        self.message = if matches!(self.status.as_str(), "ready" | "saved") {
            String::new()
        } else {
            state
                .message
                .chars()
                .filter(|c| !c.is_control())
                .take(180)
                .collect()
        };
        if !matches!(self.status.as_str(), "loading" | "saving") {
            self.pending = None;
        }
        true
    }

    pub fn request(&mut self, save: bool) {
        if self.path.is_none() || self.pending.is_some() || (save && !self.can_save()) {
            return;
        }
        let id = format!(
            "{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            NEXT_REQUEST.fetch_add(1, Ordering::Relaxed)
        );
        let text = if save { self.editor.as_str() } else { "" };
        if text.chars().count() > LIMIT {
            self.message = "Limite de 4.000 caracteres.".into();
            return;
        }
        let payload = serde_json::json!({"id":id,"action":if save {"save"}else{"load"},"expectedRevision":self.revision,"text":text});
        let result = (|| -> std::io::Result<()> {
            let directory = self.directory()?;
            let temporary = directory.join(format!(
                ".request-{}-{}",
                std::process::id(),
                NEXT_REQUEST.fetch_add(1, Ordering::Relaxed)
            ));
            let mut file = crate::platform::create_private_state_file(&temporary)?;
            let result = (|| -> std::io::Result<()> {
                file.write_all(&serde_json::to_vec(&payload)?)?;
                file.sync_all()?;
                // hard_link publica o arquivo completo sem substituir um pedido ainda pendente.
                std::fs::hard_link(&temporary, directory.join("request.json"))
            })();
            drop(file);
            let _ = std::fs::remove_file(temporary);
            result
        })();
        match result {
            Ok(()) => {
                self.pending = Some((id, save.then(|| text.to_owned())));
                self.status = if save { "saving" } else { "loading" }.into();
                self.message.clear();
            }
            Err(error) => {
                self.status = "error".into();
                self.message = if error.kind() == std::io::ErrorKind::AlreadyExists {
                    "Há um pedido em andamento. Aguarde e tente novamente."
                } else {
                    "Não foi possível enviar. Reabra pelo Pretorian autenticado."
                }
                .into();
            }
        }
    }

    pub fn insert(&mut self, text: &str) {
        if text
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
        {
            self.message = "O texto contém caracteres de controle não permitidos.".into();
            return;
        }
        let mut candidate = self.editor.clone();
        candidate.insert(text);
        self.accept(candidate);
    }

    pub fn key(&mut self, key: &crate::input::TerminalKey) {
        let mut candidate = self.editor.clone();
        candidate.handle_key(key);
        self.accept(candidate);
    }

    fn accept(&mut self, candidate: TextEditor) {
        if candidate.chars().count() > LIMIT {
            self.message = "Limite de 4.000 caracteres; o texto não foi inserido.".into();
        } else {
            self.editor = candidate;
            self.message.clear();
        }
    }
}

impl ClientShellState {
    pub(crate) fn tick_sobre_voce(&mut self, now: Instant) -> bool {
        if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
            return settings.sobre.poll(now);
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Bridge(PathBuf);
    impl Bridge {
        fn new() -> Self {
            let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
                "sobre-{}-{}",
                std::process::id(),
                NEXT_REQUEST.fetch_add(1, Ordering::Relaxed)
            ));
            crate::platform::create_remote_private_dir(&path).unwrap();
            Self(path)
        }
        fn state(&self, revision: u64, text: &str, status: &str, id: Option<&str>) {
            let path = self.0.join("estado.json");
            let _ = std::fs::remove_file(&path);
            let mut file = crate::platform::create_private_state_file(&path).unwrap();
            file.write_all(&serde_json::to_vec(&serde_json::json!({"version":1,"revision":revision,"text":text,"status":status,"message":"","requestId":id})).unwrap()).unwrap();
        }
        fn request(&self) -> serde_json::Value {
            let path = self.0.join("request.json");
            let value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            std::fs::remove_file(path).unwrap();
            value
        }
        fn poll(&self, model: &mut SobreVoce) {
            model.last_poll = None;
            model.poll(Instant::now());
        }
    }
    impl Drop for Bridge {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn sobre_voce_correlaciona_salvar_preserva_edicao_e_apaga() {
        let bridge = Bridge::new();
        bridge.state(1, "Texto inicial", "ready", None);
        let mut model = SobreVoce::at(Some(bridge.0.clone()));
        model.insert("\nNova linha 😀");
        model.request(true);
        let request = bridge.request();
        let id = request["id"].as_str().unwrap();
        assert_eq!(request["text"], "Texto inicial\nNova linha 😀");
        assert_eq!(request["expectedRevision"], 1);
        model.insert(" depois");
        bridge.state(2, "errado", "saved", Some("antigo"));
        bridge.poll(&mut model);
        assert_eq!(model.status, "saving");
        bridge.state(2, request["text"].as_str().unwrap(), "saved", Some(id));
        bridge.poll(&mut model);
        assert!(model.dirty());
        assert!(model.editor.ends_with(" depois"));
        assert_eq!(model.revision, 2);
        model.editor.clear();
        model.request(true);
        let clear = bridge.request();
        assert_eq!(clear["text"], "");
        bridge.state(3, "", "saved", clear["id"].as_str());
        bridge.poll(&mut model);
        assert!(!model.dirty());
        assert_eq!(model.feedback(), "Salvo.");
    }

    #[test]
    fn sobre_voce_conflito_recarrega_sem_perder_rascunho_e_reenvia_explicitamente() {
        let bridge = Bridge::new();
        bridge.state(1, "Primeiro", "ready", None);
        let mut model = SobreVoce::at(Some(bridge.0.clone()));
        model.insert(" editado");
        model.request(true);
        let req = bridge.request();
        bridge.state(1, "Primeiro", "conflict", req["id"].as_str());
        bridge.poll(&mut model);
        assert!(!model.can_save());
        model.request(true);
        assert!(!bridge.0.join("request.json").exists());
        model.request(false);
        let req = bridge.request();
        bridge.state(2, "Outro texto", "ready", req["id"].as_str());
        bridge.poll(&mut model);
        assert_eq!(model.editor.as_str(), "Primeiro editado");
        assert!(model.can_save());
        assert_eq!(model.revision, 2);
        model.request(true);
        let req = bridge.request();
        assert_eq!(req["expectedRevision"], 2);
        bridge.state(2, "Outro texto", "error", req["id"].as_str());
        bridge.poll(&mut model);
        assert!(model.can_save());
        assert_eq!(model.editor.as_str(), "Primeiro editado");
    }

    #[test]
    fn sobre_voce_nao_sobrescreve_pedido_e_reabre_estado_com_id_antigo() {
        let bridge = Bridge::new();
        bridge.state(2, "Anterior", "saved", Some("anterior"));
        let mut model = SobreVoce::at(Some(bridge.0.clone()));
        model.open();
        let bytes = std::fs::read(bridge.0.join("request.json")).unwrap();
        model.request(false);
        assert_eq!(bytes, std::fs::read(bridge.0.join("request.json")).unwrap());
        let mut other = SobreVoce::at(Some(bridge.0.clone()));
        other.open();
        assert_eq!(other.status, "error");
        assert_eq!(bytes, std::fs::read(bridge.0.join("request.json")).unwrap());
        let req = bridge.request();
        bridge.state(2, "Anterior", "ready", req["id"].as_str());
        bridge.poll(&mut model);
        assert_eq!(model.editor.as_str(), "Anterior");
    }

    #[test]
    fn sobre_voce_isola_contas_e_recusa_diretorio_substituido() {
        let first = Bridge::new();
        first.state(1, "Conta A", "ready", None);
        let second = Bridge::new();
        second.state(9, "Conta B", "ready", None);
        let mut a = SobreVoce::at(Some(first.0.clone()));
        let b = SobreVoce::at(Some(second.0.clone()));
        assert_eq!(a.editor.as_str(), "Conta A");
        assert_eq!(b.editor.as_str(), "Conta B");
        let moved = first.0.with_extension("antiga");
        std::fs::rename(&first.0, &moved).unwrap();
        crate::platform::create_remote_private_dir(&first.0).unwrap();
        first.state(9, "Conta B", "ready", None);
        first.poll(&mut a);
        assert_eq!(a.status, "unavailable");
        assert_eq!(a.editor.as_str(), "");
        std::fs::remove_dir_all(moved).unwrap();
        let mut none = SobreVoce::default();
        none.request(false);
        none.request(true);
        assert_eq!(none.status, "unavailable");
    }

    #[test]
    fn sobre_voce_limita_codepoints_sem_truncar_paste_multiline() {
        let mut model = SobreVoce::default();
        model.insert(&"😀".repeat(3998));
        model.insert("\nX");
        assert_eq!(model.editor.chars().count(), 4000);
        model.insert("Y");
        assert_eq!(model.editor.chars().count(), 4000);
        assert!(model.editor.ends_with("\nX"));
        assert!(model.message.contains("Limite"));
        assert!(model.feedback().contains("Limite"));
    }
}

#[cfg(test)]
mod binding_tests {
    use super::*;
    #[test]
    fn sobre_voce_unavailable_espontaneo_limpa_dados_e_rascunho_sem_pedido() {
        let mut model = SobreVoce::default();
        model.receive(State {
            version: 1,
            revision: 4,
            text: "Dado pessoal".into(),
            status: "ready".into(),
            message: String::new(),
            request_id: None,
        });
        model.insert(" editado");
        assert!(!format!("{model:?}").contains("Dado pessoal"));
        assert!(model.initialized && model.dirty());
        model.receive(State {
            version: 1,
            revision: 0,
            text: String::new(),
            status: "unavailable".into(),
            message: String::new(),
            request_id: None,
        });
        assert_eq!(model.editor.as_str(), "");
        assert_eq!(model.baseline, "");
        assert_eq!(model.revision, 0);
        assert!(!model.initialized);
        assert!(!model.can_save());
        assert_eq!(model.status, "unavailable");
    }
}
