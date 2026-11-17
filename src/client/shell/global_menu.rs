use super::*;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum PaginaRaciocinio {
    #[default]
    Principal,
    Raciocinio,
    Trocar,
    Configurar,
    Autorizar,
    /// Preferências do login automático do PJe feito pelo Pretorian CLI.
    LoginPje,
    /// Diálogo de nível técnico mostrado antes de abrir uma aba na janela gerenciada.
    NovaAba,
}

impl ClientGlobalMenuOverlay {
    pub(super) fn items(
        &self,
        snapshot: &ClientShellSnapshot,
    ) -> Vec<(&str, ClientGlobalMenuAction)> {
        use ClientGlobalMenuAction as A;
        use PaginaRaciocinio as P;
        match self.pagina {
            P::Autorizar => {
                let mut items = vec![];
                if let Some(auth) = &self.autorizacao {
                    items.push((auth.mensagem.as_str(), A::Nada));
                    if let Some(codigo) = &auth.codigo {
                        items.push((codigo.as_str(), A::Nada));
                    }
                    if auth.aceita_codigo() {
                        items.push((
                            "Colar código do navegador e enviar ao Claude",
                            A::EnviarCodigoAutorizacao,
                        ));
                    }
                    if let Some(url) = &auth.url {
                        items.push((url.as_str(), A::AbrirAutorizacao));
                    }
                }
                items.push(("Fechar / cancelar autorização", A::Pagina(P::Raciocinio)));
                items
            }
            P::NovaAba => std::iter::once((
                "Nível técnico da nova aba · tecle 1 a 5 · Esc cancela",
                A::Nada,
            ))
            .chain(
                crate::config::NivelRaciocinio::TODOS
                    .into_iter()
                    .map(|n| (n.rotulo_numerado(), A::NovaAba(n))),
            )
            .collect(),
            P::Principal => global_menu_items(snapshot),
            P::Raciocinio => vec![
                ("Trocar", A::Pagina(P::Trocar)),
                ("Autorizar", A::Autorizar),
                ("Configurar", A::Pagina(P::Configurar)),
                ("Voltar", A::Pagina(P::Principal)),
            ],
            P::LoginPje => vec![
                (
                    "Modo headless: ligado (login sem janela)",
                    A::PjeSemJanela(true),
                ),
                (
                    "Modo headless: desligado (acompanhar na janela)",
                    A::PjeSemJanela(false),
                ),
                ("Voltar", A::Pagina(P::Principal)),
            ],
            P::Trocar => self
                .provedores
                .iter()
                .map(|p| {
                    (
                        match *p {
                            "codex" => "Codex",
                            "claude" => "Claude",
                            _ => "DeepSeek",
                        },
                        A::Provedor(p),
                    )
                })
                .chain(std::iter::once(("Voltar", A::Pagina(P::Raciocinio))))
                .collect(),
            P::Configurar => crate::config::NivelRaciocinio::TODOS
                .into_iter()
                .map(|n| (n.descricao(), A::Nivel(n)))
                .chain(self.mestre_doutor_iguais.then_some((
                    "Mestre e Doutor usam a mesma configuração nesta instalação.",
                    A::Nada,
                )))
                .chain(std::iter::once(("Voltar", A::Pagina(P::Raciocinio))))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ClientGlobalMenuAction {
    Binding(crate::input::KeybindAction),
    WhatsNew,
    Lembretes,
    /// Fecha o cliente pedindo ao lançador que atualize o Pretorian e reabra a sessão.
    Atualizar,
    Autorizar,
    AbrirAutorizacao,
    EnviarCodigoAutorizacao,
    Nada,
    Pagina(PaginaRaciocinio),
    Provedor(&'static str),
    Nivel(crate::config::NivelRaciocinio),
    PjeSemJanela(bool),
    /// Abre a aba com o nível escolhido para o agente dela.
    NovaAba(crate::config::NivelRaciocinio),
    PretorianUpdate,
}

pub(super) fn global_menu_attention(snapshot: &ClientShellSnapshot) -> bool {
    snapshot.update_available.is_some()
        || snapshot.integration_updates_available
        || super::pretorian_update::estado().is_some_and(|estado| estado.pede_atencao())
        || super::lembretes::pedem_atencao()
}

pub(super) fn global_menu_item_has_badge(
    snapshot: &ClientShellSnapshot,
    action: ClientGlobalMenuAction,
) -> bool {
    (action == ClientGlobalMenuAction::PretorianUpdate
        && super::pretorian_update::estado().is_some_and(|estado| estado.pede_atencao()))
        || (action == ClientGlobalMenuAction::WhatsNew && snapshot.update_available.is_some())
        || (action == ClientGlobalMenuAction::Lembretes && super::lembretes::pedem_atencao())
        || (action == ClientGlobalMenuAction::Binding(crate::input::KeybindAction::Settings)
            && snapshot.integration_updates_available)
}

pub(super) fn global_menu_items(
    snapshot: &ClientShellSnapshot,
) -> Vec<(&'static str, ClientGlobalMenuAction)> {
    global_menu_items_for(snapshot, crate::terminal_effects::launcher_updates())
}

pub(super) fn global_menu_items_for(
    snapshot: &ClientShellSnapshot,
    launcher_updates: bool,
) -> Vec<(&'static str, ClientGlobalMenuAction)> {
    let mut items = vec![(
        "configurações",
        ClientGlobalMenuAction::Binding(crate::input::KeybindAction::Settings),
    )];
    if super::lembretes::disponivel() {
        items.push(("ver lembretes", ClientGlobalMenuAction::Lembretes));
    }
    if snapshot.update_available.is_some() || snapshot.latest_release_notes_available {
        items.push((
            if snapshot.update_available.is_some() {
                "atualização pronta"
            } else {
                "novidades"
            },
            ClientGlobalMenuAction::WhatsNew,
        ));
    }
    if let Some(rotulo) = super::pretorian_update::rotulo_menu() {
        items.push((rotulo, ClientGlobalMenuAction::PretorianUpdate));
    }
    items.push((
        "raciocínio",
        ClientGlobalMenuAction::Pagina(PaginaRaciocinio::Raciocinio),
    ));
    items.push((
        "login pje",
        ClientGlobalMenuAction::Pagina(PaginaRaciocinio::LoginPje),
    ));
    if launcher_updates && super::pretorian_update::rotulo_menu().is_none() {
        items.push(("atualizar o Pretorian", ClientGlobalMenuAction::Atualizar));
    }
    items.push((
        "desconectar",
        ClientGlobalMenuAction::Binding(crate::input::KeybindAction::Detach),
    ));
    items
}

impl ClientShellState {
    pub(super) fn toggle_global_menu(&mut self) {
        if matches!(self.overlay, Some(ClientShellOverlay::GlobalMenu(_))) {
            self.overlay = None;
        } else {
            self.overlay = Some(ClientShellOverlay::GlobalMenu(ClientGlobalMenuOverlay {
                highlighted: 0,
                pagina: PaginaRaciocinio::Principal,
                mestre_doutor_iguais: false,
                autorizacao: None,
                provedores: crate::config::provedores_disponiveis(
                    std::env::var("PRETORIAN_PROVEDORES").ok().as_deref(),
                ),
            }));
        }
    }

    /// Na janela gerenciada toda aba nova abre um agente; o nível técnico dele é escolhido antes.
    pub(super) fn open_new_tab_level_menu(&mut self) {
        let padrao = crate::config::NivelRaciocinio::TODOS
            .iter()
            .position(|n| *n == self.config.nivel_raciocinio)
            .unwrap_or(2);
        self.overlay = Some(ClientShellOverlay::GlobalMenu(ClientGlobalMenuOverlay {
            // A primeira linha é o título do diálogo.
            highlighted: padrao + 1,
            pagina: PaginaRaciocinio::NovaAba,
            mestre_doutor_iguais: false,
            autorizacao: None,
            provedores: crate::config::provedores_disponiveis(
                std::env::var("PRETORIAN_PROVEDORES").ok().as_deref(),
            ),
        }));
    }

    fn criar_aba_com_nivel(
        &mut self,
        nivel: crate::config::NivelRaciocinio,
        outcome: &mut ClientShellInput,
    ) {
        let Some(workspace_id) = self
            .snapshot
            .as_deref()
            .and_then(|snapshot| snapshot.focused_workspace_id.clone())
        else {
            return;
        };
        // O lançador lê esta variável ao abrir o agente do painel e grava o nível na conversa da aba.
        let env = [("PRETORIAN_NIVEL_ABA".to_owned(), nivel.chave().to_owned())]
            .into_iter()
            .collect();
        self.push_endpoint_method(
            crate::api::schema::Method::TabCreate(crate::api::schema::TabCreateParams {
                workspace_id: Some(workspace_id),
                cwd: None,
                focus: true,
                label: None,
                env,
            }),
            outcome,
        );
    }

    pub(super) fn move_global_menu_selection(&mut self, delta: isize) {
        let item_count = self
            .snapshot
            .as_deref()
            .map(|snapshot| match &self.overlay {
                Some(ClientShellOverlay::GlobalMenu(menu)) => menu.items(snapshot),
                _ => global_menu_items(snapshot),
            })
            .map_or(0, |items| items.len());
        let Some(ClientShellOverlay::GlobalMenu(menu)) = self.overlay.as_mut() else {
            return;
        };
        menu.highlighted = (menu.highlighted as isize + delta)
            .clamp(0, item_count.saturating_sub(1) as isize) as usize;
    }

    pub(super) fn activate_global_menu_item(
        &mut self,
        index: usize,
        outcome: &mut ClientShellInput,
    ) {
        let Some(action) = self.snapshot.as_deref().and_then(|snapshot| {
            match &self.overlay {
                Some(ClientShellOverlay::GlobalMenu(menu)) => menu.items(snapshot),
                _ => global_menu_items(snapshot),
            }
            .get(index)
            .map(|(_, action)| *action)
        }) else {
            return;
        };
        if action == ClientGlobalMenuAction::WhatsNew
            && self
                .snapshot
                .as_deref()
                .and_then(|snapshot| snapshot.release_notes.as_ref())
                .is_none()
        {
            return;
        }
        if action == ClientGlobalMenuAction::Nada {
            return;
        }
        if action == ClientGlobalMenuAction::EnviarCodigoAutorizacao {
            if let Some(ClientShellOverlay::GlobalMenu(menu)) = &mut self.overlay {
                if let Some(auth) = &mut menu.autorizacao {
                    if let Some(codigo) = crate::platform::read_clipboard_text() {
                        auth.enviar_codigo(&codigo);
                    } else {
                        auth.mensagem = "Copie o código do navegador antes de enviá-lo.".into();
                    }
                }
            }
            outcome.repaint = true;
            return;
        }
        if action == ClientGlobalMenuAction::AbrirAutorizacao {
            if let Some(ClientShellOverlay::GlobalMenu(menu)) = &self.overlay {
                if let Some(url) = menu.autorizacao.as_ref().and_then(|auth| auth.url.clone()) {
                    outcome.actions.push(ClientShellAction::OpenSafeWebUrl(url));
                }
            }
            return;
        }
        if matches!(
            action,
            ClientGlobalMenuAction::Provedor(_)
                | ClientGlobalMenuAction::Nivel(_)
                | ClientGlobalMenuAction::Autorizar
        ) && !self.active_endpoint_id.is_local()
        {
            self.set_endpoint_error(
                "Abra a sessão local do Pretorian para configurar ou autorizar o raciocínio.",
            );
            outcome.repaint = true;
            return;
        }
        let provedores = match &self.overlay {
            Some(ClientShellOverlay::GlobalMenu(menu)) => menu.provedores.clone(),
            _ => crate::config::provedores_disponiveis(
                std::env::var("PRETORIAN_PROVEDORES").ok().as_deref(),
            ),
        };
        self.overlay = None;
        match action {
            ClientGlobalMenuAction::Nada
            | ClientGlobalMenuAction::AbrirAutorizacao
            | ClientGlobalMenuAction::EnviarCodigoAutorizacao => {}
            ClientGlobalMenuAction::Autorizar => {
                let provedor = self.config.provedor_raciocinio.as_deref().unwrap_or("");
                match super::authorization::Autorizacao::iniciar(provedor) {
                    Ok(autorizacao) => {
                        self.overlay =
                            Some(ClientShellOverlay::GlobalMenu(ClientGlobalMenuOverlay {
                                highlighted: 0,
                                pagina: PaginaRaciocinio::Autorizar,
                                mestre_doutor_iguais: false,
                                provedores,
                                autorizacao: Some(autorizacao),
                            }))
                    }
                    Err(erro) => self.set_endpoint_error(erro),
                }
            }
            ClientGlobalMenuAction::Pagina(pagina) => {
                let highlighted = if pagina == PaginaRaciocinio::Configurar {
                    crate::config::NivelRaciocinio::TODOS
                        .iter()
                        .position(|n| *n == self.config.nivel_raciocinio)
                        .unwrap_or(0)
                } else if pagina == PaginaRaciocinio::LoginPje {
                    usize::from(!self.config.pje_sem_janela)
                } else if pagina == PaginaRaciocinio::Trocar {
                    provedores
                        .iter()
                        .position(|provedor| {
                            Some(*provedor) == self.config.provedor_raciocinio.as_deref()
                        })
                        .unwrap_or(0)
                } else {
                    0
                };
                self.overlay = Some(ClientShellOverlay::GlobalMenu(ClientGlobalMenuOverlay {
                    highlighted,
                    pagina,
                    mestre_doutor_iguais: pagina == PaginaRaciocinio::Configurar
                        && crate::config::mestre_e_doutor_iguais(
                            self.config.provedor_raciocinio.as_deref().unwrap_or(""),
                            |chave| std::env::var(chave).ok(),
                        ),
                    provedores,
                    autorizacao: None,
                }));
            }
            ClientGlobalMenuAction::Provedor(provedor) => {
                if provedor == "deepseek"
                    && !std::env::var("DEEPSEEK_API_KEY")
                        .is_ok_and(|chave| !chave.trim().is_empty())
                {
                    self.set_endpoint_error("A chave DeepSeek ainda não está configurada. A escolha anterior foi mantida.");
                } else {
                    self.save_settings_edit(crate::config::ConfigEdit::Agente(provedor), outcome);
                }
            }
            ClientGlobalMenuAction::Nivel(nivel) => {
                self.save_settings_edit(crate::config::ConfigEdit::Nivel(nivel), outcome);
            }
            ClientGlobalMenuAction::PjeSemJanela(ligado) => {
                self.save_settings_edit(crate::config::ConfigEdit::PjeSemJanela(ligado), outcome);
            }
            ClientGlobalMenuAction::NovaAba(nivel) => self.criar_aba_com_nivel(nivel, outcome),
            ClientGlobalMenuAction::Binding(binding) => {
                self.record_binding(crate::input::KeybindMatch::Action(binding), outcome)
            }
            ClientGlobalMenuAction::WhatsNew => self.open_release_notes(),
            ClientGlobalMenuAction::Lembretes => {
                super::lembretes::recarregar();
                self.overlay = Some(ClientShellOverlay::Lembretes(Default::default()));
            }
            ClientGlobalMenuAction::Atualizar => {
                crate::terminal_effects::request_update();
                outcome.detach = true;
            }
            ClientGlobalMenuAction::PretorianUpdate => {
                self.overlay = Some(ClientShellOverlay::PretorianUpdate(
                    super::pretorian_update::Cartao::novo(),
                ));
                // O pedido existente continua sendo atendido pelo lançador.
                if super::pretorian_update::aceita_pedido() {
                    super::pretorian_update::pedir();
                }
            }
        }
        outcome.repaint = true;
    }
}

impl ClientShellState {
    pub(crate) fn tick_autorizacao(&mut self) -> bool {
        match (&mut self.overlay, self.snapshot.as_deref()) {
            (Some(ClientShellOverlay::GlobalMenu(menu)), Some(snapshot)) => {
                let mudou = menu
                    .autorizacao
                    .as_mut()
                    .is_some_and(|auth| auth.atualizar());
                if mudou {
                    menu.highlighted = menu
                        .highlighted
                        .min(menu.items(snapshot).len().saturating_sub(1));
                }
                mudou
            }
            _ => false,
        }
    }
}
