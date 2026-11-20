use super::*;
use crossterm::event::{KeyCode, KeyModifiers};

pub(super) fn integration_needs_install(info: &crate::api::schema::IntegrationInfo) -> bool {
    info.state == crate::api::schema::IntegrationState::Outdated
        || info.available && info.state == crate::api::schema::IntegrationState::NotInstalled
}

impl ClientShellState {
    pub(super) fn open_settings_overlay(&mut self) {
        self.overlay = Some(ClientShellOverlay::Settings(ClientSettingsOverlay {
            section: ClientSettingsSection::Usuario,
            selected: 0,
            nome: TextEditor::new(self.config.nome_usuario.as_deref().unwrap_or(""), false),
            nome_somente_leitura: std::env::var("PRETORIAN_NOME_CENTAURO")
                .ok()
                .and_then(|nome| crate::config::sanitizar_nome(&nome))
                .is_some(),
            dependencias: false,
            sound_enabled: self.config.sound_enabled,
            toast_delivery: self.config.toast_delivery,
            sobre: super::sobre_voce::SobreVoce::new(),
            integrations: Vec::new(),
            integration_messages: Vec::new(),
            loading_integrations: false,
            installing_integrations: false,
        }));
        if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
            if settings.nome_somente_leitura {
                settings.selected = 2;
            }
            settings.sobre.open();
        }
    }

    fn selected_index_for_settings_section(&self, section: ClientSettingsSection) -> usize {
        match section {
            ClientSettingsSection::Usuario => 0,
            ClientSettingsSection::Sound => usize::from(!self.config.sound_enabled),
            ClientSettingsSection::Integrations => 0,
        }
    }

    pub(super) fn select_settings_section(
        &mut self,
        section: ClientSettingsSection,
        outcome: &mut ClientShellInput,
    ) {
        let section = match section {
            ClientSettingsSection::Sound => ClientSettingsSection::Sound,
            _ => ClientSettingsSection::Usuario,
        };
        let selected = self.selected_index_for_settings_section(section);
        let request_integrations = matches!(section, ClientSettingsSection::Integrations)
            && matches!(
                self.overlay,
                Some(ClientShellOverlay::Settings(ClientSettingsOverlay {
                    loading_integrations: false,
                    installing_integrations: false,
                    ..
                }))
            );
        if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
            settings.section = section;
            settings.selected =
                if section == ClientSettingsSection::Usuario && settings.nome_somente_leitura {
                    2
                } else {
                    selected
                };
            settings.dependencias = false;
            if section == ClientSettingsSection::Usuario {
                settings.sobre.open();
            }
        }
        if request_integrations {
            self.queue_integration_list(outcome, true);
        }
        outcome.repaint = true;
    }

    fn move_settings_section(&mut self, delta: isize, outcome: &mut ClientShellInput) {
        let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_ref() else {
            return;
        };
        let current = ClientSettingsSection::ALL
            .iter()
            .position(|section| *section == settings.section)
            .unwrap_or(0);
        let next = (current as isize + delta).rem_euclid(ClientSettingsSection::ALL.len() as isize)
            as usize;
        self.select_settings_section(ClientSettingsSection::ALL[next], outcome);
    }

    fn settings_choice_count(&self) -> usize {
        match self.overlay.as_ref() {
            Some(ClientShellOverlay::Settings(settings)) => match settings.section {
                ClientSettingsSection::Usuario if settings.dependencias => {
                    1 + super::pretorian_dependencias::itens().len()
                }
                ClientSettingsSection::Usuario => 8,
                ClientSettingsSection::Sound => 6,
                ClientSettingsSection::Integrations => settings.integrations.len(),
            },
            _ => 0,
        }
    }

    pub(super) fn move_settings_selection(&mut self, delta: isize) {
        let count = self.settings_choice_count();
        let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() else {
            return;
        };
        if count == 0 {
            settings.selected = 0;
            return;
        }
        settings.selected = (settings.selected as isize + delta)
            .clamp(0, count.saturating_sub(1) as isize) as usize;
    }

    pub(super) fn select_settings_choice(&mut self, index: usize) {
        let count = self.settings_choice_count();
        if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
            if count > 0 {
                settings.selected = index.min(count - 1);
            }
        }
    }

    pub(super) fn cancel_settings_overlay(&mut self) {
        if matches!(self.overlay, Some(ClientShellOverlay::Settings(_))) {
            self.overlay = None;
        }
    }

    pub(super) fn save_settings_edit(
        &mut self,
        edit: crate::config::ConfigEdit<'_>,
        outcome: &mut ClientShellInput,
    ) -> bool {
        if let Err(error) = crate::config::write_edit(edit) {
            self.set_endpoint_error(error);
            outcome.repaint = true;
            return false;
        }
        self.reload_client_config();
        if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
            settings.sound_enabled = self.config.sound_enabled;
            settings.toast_delivery = self.config.toast_delivery;
        }
        self.push_endpoint_method_with_kind(
            crate::api::schema::Method::ServerReloadConfig(
                crate::api::schema::EmptyParams::default(),
            ),
            PendingEndpointKind::ReloadConfig,
            outcome,
        );
        outcome.repaint = true;
        true
    }

    pub(super) fn apply_settings_choice(&mut self, outcome: &mut ClientShellInput) {
        let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_ref() else {
            return;
        };
        let section = settings.section;
        let selected = settings.selected;
        match section {
            ClientSettingsSection::Usuario => {
                if settings.dependencias {
                    if selected == 0 {
                        if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut()
                        {
                            settings.dependencias = false;
                            settings.selected = 6;
                        }
                    } else if let Some(item) = super::pretorian_dependencias::itens()
                        .get(selected - 1)
                        .filter(|item| item.aceita_pedido())
                    {
                        super::pretorian_dependencias::pedir(&item.id);
                    }
                } else if selected == 0 || selected == 1 {
                    self.salvar_nome_local(outcome);
                } else if selected == 3 {
                    self.salvar_perfil(outcome);
                } else if selected == 7 {
                    self.cancel_settings_overlay();
                } else if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
                    match selected {
                        4 if settings.sobre.can_save() => {
                            settings.sobre.editor.clear();
                            settings.sobre.request(true);
                        }
                        5 => settings.sobre.request(false),
                        6 => {
                            settings.dependencias = true;
                            settings.selected = 0;
                        }
                        _ => {}
                    }
                }
                outcome.repaint = true;
            }
            ClientSettingsSection::Sound => {
                if selected < 2 {
                    self.save_settings_edit(
                        crate::config::ConfigEdit::Sound(selected == 0),
                        outcome,
                    );
                } else {
                    let delivery = match selected {
                        2 => crate::config::ToastDelivery::Off,
                        3 => crate::config::ToastDelivery::Herdr,
                        4 => crate::config::ToastDelivery::Terminal,
                        _ => crate::config::ToastDelivery::System,
                    };
                    self.save_settings_edit(
                        crate::config::ConfigEdit::ToastDelivery(delivery),
                        outcome,
                    );
                }
            }
            ClientSettingsSection::Integrations => self.install_recommended_integrations(outcome),
        }
    }

    fn salvar_nome_local(&mut self, outcome: &mut ClientShellInput) {
        let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_ref() else {
            return;
        };
        if settings.nome_somente_leitura {
            return;
        }
        let Some(nome) = crate::config::sanitizar_nome(settings.nome.as_str()) else {
            return;
        };
        if self.config.nome_usuario.as_deref() == Some(&nome) {
            return;
        }
        if self.save_settings_edit(crate::config::ConfigEdit::Nome(&nome), outcome) {
            if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
                settings.nome = TextEditor::new(&nome, false);
            }
        }
    }

    fn salvar_perfil(&mut self, outcome: &mut ClientShellInput) {
        self.salvar_nome_local(outcome);
        if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
            if settings.sobre.dirty() {
                settings.sobre.request(true);
            }
        }
    }

    fn queue_integration_list(&mut self, outcome: &mut ClientShellInput, clear_messages: bool) {
        if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
            settings.loading_integrations = true;
            if clear_messages {
                settings.integration_messages.clear();
            }
        }
        if !self.push_endpoint_method_with_kind(
            crate::api::schema::Method::IntegrationList(crate::api::schema::EmptyParams::default()),
            PendingEndpointKind::IntegrationList,
            outcome,
        ) {
            if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
                settings.loading_integrations = false;
            }
        }
    }

    fn install_recommended_integrations(&mut self, outcome: &mut ClientShellInput) {
        if self.pending_integration_installs > 0 {
            return;
        }
        let targets = match self.overlay.as_ref() {
            Some(ClientShellOverlay::Settings(settings)) => settings
                .integrations
                .iter()
                .filter(|integration| integration_needs_install(integration))
                .map(|integration| integration.target)
                .collect::<Vec<_>>(),
            _ => return,
        };
        if targets.is_empty() {
            return;
        }
        if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
            settings.installing_integrations = true;
            settings.integration_messages.clear();
        }
        self.pending_integration_installs = 0;
        for target in targets {
            if self.push_endpoint_method_with_kind(
                crate::api::schema::Method::IntegrationInstall(
                    crate::api::schema::IntegrationInstallParams { target },
                ),
                PendingEndpointKind::IntegrationInstall,
                outcome,
            ) {
                self.pending_integration_installs += 1;
            }
        }
        if self.pending_integration_installs == 0 {
            if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
                settings.installing_integrations = false;
            }
        }
        outcome.repaint = true;
    }

    pub(super) fn handle_settings_endpoint_result(
        &mut self,
        kind: PendingEndpointKind,
        result: Result<crate::api::schema::ResponseResult, ClientShellEndpointError>,
    ) -> (bool, Vec<ClientShellAction>) {
        match kind {
            PendingEndpointKind::IntegrationList => {
                if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
                    settings.loading_integrations = false;
                    match result {
                        Ok(crate::api::schema::ResponseResult::IntegrationList {
                            integrations,
                        }) => {
                            settings.integrations = integrations;
                            settings.selected = settings
                                .selected
                                .min(settings.integrations.len().saturating_sub(1));
                        }
                        Ok(_) => {
                            self.set_endpoint_error(
                                "a conexão retornou um resultado inesperado para a lista de integrações",
                            );
                        }
                        Err(_) => {}
                    }
                }
                (true, Vec::new())
            }
            PendingEndpointKind::IntegrationInstall => {
                let cancelled = result
                    .as_ref()
                    .is_err_and(|error| error.code.as_deref() == Some("endpoint_cancelled"));
                self.pending_integration_installs =
                    self.pending_integration_installs.saturating_sub(1);
                if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
                    match result {
                        Ok(crate::api::schema::ResponseResult::IntegrationInstall {
                            details,
                            ..
                        }) => settings.integration_messages.extend(details.messages),
                        Ok(_) => settings.integration_messages.push(
                            "a conexão retornou um resultado inesperado para a integração".into(),
                        ),
                        Err(error) => settings.integration_messages.push(error.message),
                    }
                    settings.installing_integrations = self.pending_integration_installs > 0;
                }
                let actions = if !cancelled
                    && self.pending_integration_installs == 0
                    && matches!(self.overlay, Some(ClientShellOverlay::Settings(_)))
                {
                    let mut deferred = ClientShellInput::default();
                    self.queue_integration_list(&mut deferred, false);
                    deferred.actions
                } else {
                    Vec::new()
                };
                (true, actions)
            }
            _ => (false, Vec::new()),
        }
    }

    pub(super) fn route_settings_key(
        &mut self,
        key: &crate::input::TerminalKey,
        outcome: &mut ClientShellInput,
    ) -> bool {
        if !matches!(self.overlay, Some(ClientShellOverlay::Settings(_))) {
            return false;
        }
        let (code, modifiers) = crate::config::normalize_key_combo((key.code, key.modifiers));
        if code == KeyCode::Esc {
            if !matches!(
                self.overlay,
                Some(ClientShellOverlay::Settings(ClientSettingsOverlay {
                    installing_integrations: true,
                    ..
                }))
            ) {
                self.cancel_settings_overlay();
                outcome.repaint = true;
            }
            return true;
        }
        if matches!(code, KeyCode::Tab | KeyCode::BackTab)
            && modifiers.contains(KeyModifiers::CONTROL)
        {
            self.move_settings_section(
                if modifiers.contains(KeyModifiers::SHIFT) || code == KeyCode::BackTab {
                    -1
                } else {
                    1
                },
                outcome,
            );
            return true;
        }
        if self.route_settings_nome_key(key, code, modifiers, outcome) {
            return true;
        }
        if matches!(code, KeyCode::Tab | KeyCode::Right | KeyCode::Char('l'))
            && modifiers.is_empty()
        {
            self.move_settings_section(1, outcome);
            return true;
        }
        if matches!(code, KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h'))
            && modifiers.difference(KeyModifiers::SHIFT).is_empty()
        {
            self.move_settings_section(-1, outcome);
            return true;
        }
        if matches!(code, KeyCode::Up | KeyCode::Char('k')) && modifiers.is_empty() {
            self.move_settings_selection(-1);
            outcome.repaint = true;
            return true;
        }
        if matches!(code, KeyCode::Down | KeyCode::Char('j')) && modifiers.is_empty() {
            self.move_settings_selection(1);
            outcome.repaint = true;
            return true;
        }
        if matches!(code, KeyCode::Enter | KeyCode::Char(' ')) && modifiers.is_empty() {
            self.apply_settings_choice(outcome);
            return true;
        }
        true
    }

    fn route_settings_nome_key(
        &mut self,
        key: &crate::input::TerminalKey,
        code: KeyCode,
        modifiers: KeyModifiers,
        outcome: &mut ClientShellInput,
    ) -> bool {
        let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_ref() else {
            return false;
        };
        if settings.section != ClientSettingsSection::Usuario {
            return false;
        }
        if code == KeyCode::Tab || code == KeyCode::BackTab {
            let count = self.settings_choice_count();
            if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
                let first =
                    usize::from(settings.nome_somente_leitura && !settings.dependencias) * 2;
                let backwards = code == KeyCode::BackTab || modifiers.contains(KeyModifiers::SHIFT);
                settings.selected = if backwards {
                    if settings.selected <= first {
                        count - 1
                    } else {
                        settings.selected - 1
                    }
                } else if settings.selected + 1 >= count {
                    first
                } else {
                    settings.selected + 1
                };
            }
        } else if !settings.dependencias
            && modifiers == KeyModifiers::CONTROL
            && matches!(code, KeyCode::Char('s') | KeyCode::Char('r'))
        {
            if code == KeyCode::Char('s') {
                self.salvar_perfil(outcome);
            } else if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
                settings.sobre.request(false);
            }
        } else if !settings.dependencias && settings.selected == 2 {
            if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
                settings.sobre.key(key);
            }
        } else if code == KeyCode::Enter {
            self.apply_settings_choice(outcome);
        } else if !settings.dependencias && settings.selected == 0 && !settings.nome_somente_leitura
        {
            if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
                if code == KeyCode::Char('c') && modifiers == KeyModifiers::CONTROL {
                    settings.nome.clear();
                } else {
                    settings.nome.handle_key(key);
                }
            }
        } else if code == KeyCode::Up || code == KeyCode::Left {
            self.move_settings_selection(-1);
        } else if code == KeyCode::Down || code == KeyCode::Right {
            self.move_settings_selection(1);
        }
        outcome.repaint = true;
        true
    }
}
