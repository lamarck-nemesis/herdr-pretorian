use super::*;

#[test]
fn tab_overflow_controls_scroll_the_client_owned_tab_bar() {
    let mut snapshot = snapshot();
    snapshot.tabs.extend((2..=8).map(|number| ClientShellTab {
        tab_id: format!("tab_{number}"),
        workspace_id: "ws_1".into(),
        number,
        label: number.to_string(),
        custom_label: false,
        zoomed: false,
        focused: false,
        agent_status: AgentStatus::Idle,
    }));
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot));
    state.set_pane_surface(surface());
    state.compose(80, 20).expect("overflow tab bar");

    assert!(state.hits.tab_scroll_right.width > 0);
    let scroll_right = state.hits.tab_scroll_right;
    let outcome =
        state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: scroll_right.x + 1,
            row: scroll_right.y,
            modifiers: KeyModifiers::empty(),
        })]);
    assert!(outcome.repaint);
    assert_eq!(state.tab_scroll, 1);

    let mut update = state.snapshot.as_deref().expect("snapshot").clone();
    update.focused_tab_id = Some("tab_8".into());
    for tab in &mut update.tabs {
        tab.focused = tab.tab_id == "tab_8";
    }
    state.set_snapshot(Box::new(update));
    state.compose(80, 20).expect("focused overflow tab");
    assert!(state.hits.tabs.iter().any(|(_, tab_id)| tab_id == "tab_8"));

    state.compose(300, 20).expect("tabs without overflow");
    assert_eq!(state.tab_scroll, 0);
    assert_eq!(state.hits.tabs.len(), 8);
    state.compose(80, 20).expect("focused tab after narrowing");
    assert!(state.hits.tabs.iter().any(|(_, tab_id)| tab_id == "tab_8"));
}

#[test]
fn focused_last_overflow_tab_shows_its_full_label() {
    let mut projected = snapshot();
    let labels = [
        "1",
        "Laiza Portfolio Site",
        "linkedin posts",
        "update cv",
        "nvim",
        "brother",
        "day organiser",
        "nvim test",
    ];
    projected.tabs = labels
        .iter()
        .enumerate()
        .map(|(index, label)| ClientShellTab {
            tab_id: format!("tab_{}", index + 1),
            workspace_id: "ws_1".into(),
            number: index + 1,
            label: (*label).into(),
            custom_label: index > 0,
            zoomed: false,
            focused: index == 7,
            agent_status: AgentStatus::Idle,
        })
        .collect();
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    for number in [8, 7, 8] {
        let tab_id = format!("tab_{number}");
        projected.focused_tab_id = Some(tab_id.clone());
        projected.workspaces[0].active_tab_id = tab_id.clone();
        projected.panes[0].tab_id = tab_id.clone();
        for tab in &mut projected.tabs {
            tab.focused = tab.tab_id == tab_id;
        }
        state.set_snapshot(Box::new(projected.clone()));
        state.set_pane_surface(surface());
        let frame = state
            .compose(133, 20)
            .expect("reporter's overflowing strip");
        assert_eq!(
            state.hits.new_tab.right() - state.hits.tab_scroll_left.x,
            107
        );
        let rect = state
            .hits
            .tabs
            .iter()
            .find(|(_, id)| id == &tab_id)
            .expect("focused tab")
            .0;
        let text = (rect.x..rect.right())
            .map(|x| {
                frame.cells[(rect.y * frame.width + x) as usize]
                    .symbol
                    .as_str()
            })
            .collect::<String>();
        assert!(
            text.contains(labels[number - 1]),
            "focused tab rendered as {text:?}, rect={rect:?}"
        );
    }

    // Manual scrolling must be able to reveal the rest of a partially drawn last tab.
    let end_scroll = state.tab_scroll;
    let left = state.hits.tab_scroll_left;
    state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: left.x + 1,
        row: left.y,
        modifiers: KeyModifiers::empty(),
    })]);
    let frame = state.compose(133, 20).expect("manual scroll left");
    assert_eq!(state.tab_scroll, end_scroll - 1);
    let right = state.hits.tab_scroll_right;
    assert_eq!(
        frame.cells[(right.y * frame.width + right.x + 1) as usize].fg,
        crate::protocol::color_to_u32(state.config.palette.overlay1),
        "right arrow stays enabled while the final tab is clipped"
    );
    state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: right.x + 1,
        row: right.y,
        modifiers: KeyModifiers::empty(),
    })]);
    let frame = state.compose(133, 20).expect("manual scroll right");
    assert_eq!(state.tab_scroll, end_scroll);
    assert!(frame_rows(&frame)[0].contains("nvim test"));
    assert_eq!(
        frame.cells[(right.y * frame.width + right.x + 1) as usize].fg,
        crate::protocol::color_to_u32(state.config.palette.overlay0),
        "right arrow dims at the useful scroll limit"
    );
}

#[test]
fn focused_workspace_change_reveals_new_workspace_in_full_sidebar() {
    let mut initial = snapshot();
    let template = initial.workspaces[0].clone();
    initial.workspaces = (1..=24)
        .map(|number| ClientShellWorkspace {
            workspace_id: format!("ws_{number}"),
            number,
            label: format!("space-{number}"),
            branch: None,
            focused: number == 1,
            ..template.clone()
        })
        .collect();

    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(initial));
    state.set_pane_surface(surface());
    state.compose(106, 20).expect("full sidebar");
    assert!(state.hits.workspace_max_scroll > 0);
    assert!(state
        .hits
        .workspaces
        .iter()
        .all(|hit| hit.workspace_id != "ws_24"));

    let mut update = state.snapshot.as_deref().expect("snapshot").clone();
    update.revision = 2;
    update.focused_workspace_id = Some("ws_24".into());
    for workspace in &mut update.workspaces {
        workspace.focused = workspace.workspace_id == "ws_24";
    }
    let mut updated_surface = surface();
    updated_surface.projection_revision = 2;
    state.set_snapshot(Box::new(update));
    state.set_pane_surface(updated_surface);
    state.compose(106, 2).expect("zero-height workspace body");
    assert!(state.reveal_focused_workspace);
    state.compose(106, 20).expect("updated full sidebar");

    assert!(state
        .hits
        .workspaces
        .iter()
        .any(|hit| hit.workspace_id == "ws_24"));
}

#[test]
fn client_owned_sidebar_dividers_resize_live() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.config.show_agent_panel = true;
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    state.compose(106, 30).expect("expanded sidebar");
    assert!(state.hits.machines.is_empty());
    let workspace_body = state.hits.workspace_body;
    let needless_scroll =
        state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: workspace_body.x,
            row: workspace_body.y,
            modifiers: KeyModifiers::empty(),
        })]);
    assert_eq!(state.hits.workspace_max_scroll, 0);
    assert_eq!(state.workspace_scroll, 0);
    assert!(!needless_scroll.repaint);
    let width_divider = state.hits.sidebar_divider;
    state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: width_divider.x,
        row: width_divider.y + 2,
        modifiers: KeyModifiers::empty(),
    })]);
    let resize =
        state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: 31,
            row: width_divider.y + 2,
            modifiers: KeyModifiers::empty(),
        })]);
    assert_eq!(state.sidebar_width, 32);
    assert!(state.sidebar_width_manual);
    assert!(resize.repaint);
    assert!(resize.resize);
    let waiting_frame = state.compose(106, 30).expect("waiting for resized surface");
    let waiting_text: String = waiting_frame
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect();
    assert!(
        waiting_text.contains(" Área de Trabalho"),
        "local sidebar must keep spaces while resizing: {waiting_text}"
    );
    assert!(!waiting_text.contains(" máquinas"));
    assert!(!waiting_text.contains("Selecione uma máquina conectada"));
    assert!(!waiting_text.contains("LIVE"));
    assert!(waiting_frame.cursor.is_none());
    assert!(state.pane_surface.is_none());
    assert!(state.hits.panes.is_empty());
    assert!(state.hits.pane_splits.is_empty());
    assert!(state.hits.machines.is_empty());
    assert_eq!(state.hits.sidebar_divider.x, 31);
    assert_eq!(state.hits.workspaces[0].workspace_id, "ws_1");

    let next_resize =
        state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: 32,
            row: width_divider.y + 2,
            modifiers: KeyModifiers::empty(),
        })]);
    assert!(next_resize.resize);
    state.compose(106, 30).expect("continued resize");
    assert_eq!(state.hits.sidebar_divider.x, 32);
    state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 32,
        row: width_divider.y + 2,
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(state.chrome_drag.is_none());

    state.set_pane_surface(surface());
    let recovered_frame = state.compose(106, 30).expect("resized sidebar");
    let recovered_text: String = recovered_frame
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect();
    assert!(recovered_text.contains(" Área de Trabalho"));
    assert!(recovered_text.contains("LIVE"));
    assert!(!state.hits.panes.is_empty());
    let section_divider = state.hits.sidebar_section_divider;
    state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: section_divider.x + 2,
        row: section_divider.y,
        modifiers: KeyModifiers::empty(),
    })]);
    let split = state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: section_divider.x + 2,
        row: 20,
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(state.sidebar_section_split > 0.6);
    assert!(split.repaint);
    assert!(!split.resize);
}

#[test]
fn context_menus_capture_stable_targets_and_route_actions() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    state.compose(106, 20).expect("composed frame");

    let workspace = state.hits.workspaces[0].rect;
    let open_workspace_menu =
        state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column: workspace.x + 2,
            row: workspace.y,
            modifiers: KeyModifiers::empty(),
        })]);
    assert!(open_workspace_menu.actions.is_empty());
    assert!(matches!(
        state.overlay,
        Some(ClientShellOverlay::ContextMenu(ClientContextMenuOverlay {
            target: ClientContextMenuTarget::Workspace { ref workspace_id, .. },
            ..
        })) if workspace_id == "ws_1"
    ));
    let workspace_items = match state.overlay.as_ref() {
        Some(ClientShellOverlay::ContextMenu(menu)) => menu.items(),
        _ => panic!("workspace context menu"),
    };
    assert!(workspace_items
        .iter()
        .any(|item| item.action == ClientContextMenuAction::NewWorktree));
    state.compose(106, 20).expect("workspace context menu");
    let rename = state.hits.context_menu_rows[0].0;
    state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rename.x + 1,
        row: rename.y,
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(matches!(
        state.overlay,
        Some(ClientShellOverlay::Rename(ClientRenameOverlay {
            target: ClientRenameTarget::Workspace { ref workspace_id },
            ..
        })) if workspace_id == "ws_1"
    ));

    state.overlay = None;
    state.compose(106, 20).expect("composed frame");
    let pane = state.hits.panes[0].rect;
    state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: pane.x + 1,
        row: pane.y,
        modifiers: KeyModifiers::empty(),
    })]);
    state.compose(106, 20).expect("pane context menu");
    let split_index = match state.overlay.as_ref() {
        Some(ClientShellOverlay::ContextMenu(menu)) => menu
            .items()
            .iter()
            .position(|item| item.action == ClientContextMenuAction::SplitRight)
            .expect("split right item"),
        _ => panic!("pane context menu"),
    };
    let split = state.hits.context_menu_rows[split_index].0;
    let outcome =
        state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: split.x + 1,
            row: split.y,
            modifiers: KeyModifiers::empty(),
        })]);
    let [ClientShellAction::Endpoint { request, .. }] = &outcome.actions[..] else {
        panic!("pane split context action should use endpoint API");
    };
    assert!(matches!(
        &request.method,
        crate::api::schema::Method::PaneSplit(params)
            if params.target_pane_id.as_deref() == Some("pane_1")
                && params.direction == crate::api::schema::SplitDirection::Right
    ));
}

#[test]
fn global_menu_opens_from_sidebar_and_routes_client_actions() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    state.compose(106, 30).expect("shell frame");
    let launcher = state.hits.global_launcher;
    assert_ne!(launcher, Rect::default());

    let open = state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: launcher.x,
        row: launcher.y,
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(open.repaint);
    let menu = state.compose(106, 30).expect("global menu");
    let text = menu
        .cells
        .chunks(menu.width as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Configurações"));
    assert!(!text.contains("atalhos"));
    assert!(!text.contains("recarregar configuração"));
    assert!(text.contains("Login PJe"));
    assert!(text.contains("Desconectar"));

    let settings_row = state.hits.global_menu_rows[0].0;
    let settings =
        state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: settings_row.x,
            row: settings_row.y,
            modifiers: KeyModifiers::empty(),
        })]);
    assert!(settings.actions.is_empty());
    assert!(matches!(
        state.overlay,
        Some(ClientShellOverlay::Settings(_))
    ));

    // configurações, layout, raciocínio, login pje, desconectar
    state.overlay = Some(ClientShellOverlay::GlobalMenu(ClientGlobalMenuOverlay {
        highlighted: 4,
        ..Default::default()
    }));
    let detach = state.handle_input_bytes(b"\r");
    assert!(detach.detach);
    assert!(state.overlay.is_none());
}

#[test]
fn new_tab_overlay_owns_text_cursor_and_submits_public_api_request() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    let mut open = ClientShellInput::default();
    state.record_binding(
        crate::input::KeybindMatch::Action(crate::input::KeybindAction::NewTab),
        &mut open,
    );
    assert!(open.actions.is_empty());
    let frame = state.compose(106, 20).expect("new tab overlay");
    let text = frame
        .cells
        .chunks(frame.width as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Nova Aba"));
    assert!(text.contains("Salvar"));
    let restored = frame.to_ratatui_buffer().expect("overlay frame");
    assert!(!restored
        .cell((26, 7))
        .expect("overlay title cell")
        .modifier
        .contains(Modifier::DIM));
    assert!(frame.cursor.as_ref().is_some_and(|cursor| cursor.visible));

    assert!(state.handle_input_bytes(b"logs").actions.is_empty());
    let create = state.handle_input_bytes(b"\r");
    let [ClientShellAction::Endpoint { request, .. }] = &create.actions[..] else {
        panic!("new tab save should use endpoint API");
    };
    assert!(matches!(
        &request.method,
        crate::api::schema::Method::TabCreate(params)
            if params.workspace_id.as_deref() == Some("ws_1")
                && params.label.as_deref() == Some("logs")
    ));
    assert!(state.overlay.is_none());
}

#[test]
fn close_confirmation_error_becomes_client_owned_overlay_and_stable_workspace_close() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    let mut close = ClientShellInput::default();
    // Exercise the server error fallback directly: last-pane keybindings now
    // confirm locally before issuing a request.
    state.push_endpoint_method(
        crate::api::schema::Method::PaneClose(crate::api::schema::PaneTarget {
            pane_id: "pane_1".into(),
        }),
        &mut close,
    );
    let [ClientShellAction::Endpoint { request, .. }] = &close.actions[..] else {
        panic!("pane close should use endpoint API");
    };
    let request_id = request.id.clone();
    assert!(
        state
            .handle_endpoint_result(
                "boot-1",
                &request_id,
                Err(ClientShellEndpointError {
                    code: Some("confirmation_required".into()),
                    message: "confirmation required".into(),
                }),
            )
            .0
    );
    let frame = state.compose(106, 20).expect("confirmation overlay");
    let text = frame
        .cells
        .chunks(frame.width as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Fechar espaço?"));
    assert!(text.contains("1 painel"));

    let confirm = state.handle_input_bytes(b"\r");
    let [ClientShellAction::Endpoint { request, .. }] = &confirm.actions[..] else {
        panic!("confirmation should use endpoint API");
    };
    assert!(matches!(
        &request.method,
        crate::api::schema::Method::WorkspaceClose(params)
            if params.workspace_id == "ws_1" && !params.close_group
    ));
}

#[test]
fn menu_tem_login_pje_com_modo_headless_ligado_e_desligado() {
    use crate::client::shell::global_menu::{
        global_menu_items_for, ClientGlobalMenuAction, PaginaRaciocinio,
    };

    let itens = global_menu_items_for(&snapshot(), false);
    let posicao = itens
        .iter()
        .position(|(label, _)| *label == "Login PJe")
        .expect("item login pje");
    assert_eq!(
        itens[posicao].1,
        ClientGlobalMenuAction::Pagina(PaginaRaciocinio::LoginPje)
    );

    let pagina = ClientGlobalMenuOverlay {
        highlighted: 0,
        pagina: PaginaRaciocinio::LoginPje,
        especialista_doutor_iguais: false,
        provedores: vec![],
        autorizacao: None,
        nova_aba: None,
    };
    let acoes: Vec<_> = pagina
        .items(&snapshot())
        .into_iter()
        .map(|(_, acao)| acao)
        .collect();
    assert_eq!(
        acoes,
        vec![
            ClientGlobalMenuAction::PjeSemJanela(true),
            ClientGlobalMenuAction::PjeSemJanela(false),
            ClientGlobalMenuAction::Pagina(PaginaRaciocinio::Principal),
        ]
    );
}

#[test]
fn menu_oferece_atualizar_so_com_lancador_e_pede_a_saida_de_atualizacao() {
    use crate::client::shell::global_menu::{global_menu_items_for, ClientGlobalMenuAction};

    let projected = snapshot();
    let sem_lancador = global_menu_items_for(&projected, false);
    assert!(sem_lancador
        .iter()
        .all(|(_, action)| *action != ClientGlobalMenuAction::Atualizar));

    let com_lancador = global_menu_items_for(&projected, true);
    let posicao = com_lancador
        .iter()
        .position(|(label, _)| *label == "Atualizar o Pretorian")
        .expect("item de atualização");
    assert_eq!(com_lancador[posicao].1, ClientGlobalMenuAction::Atualizar);
    assert_eq!(
        com_lancador.last().map(|(label, _)| *label),
        Some("Desconectar")
    );
}

#[test]
fn raciocinio_abre_submenus_com_perfis_e_provedores_disponiveis() {
    use crate::client::shell::global_menu::{ClientGlobalMenuAction as A, PaginaRaciocinio as P};
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    state.toggle_global_menu();
    let raiz = super::super::global_menu::global_menu_items(state.snapshot.as_deref().unwrap());
    let item = raiz
        .iter()
        .position(|(_, a)| *a == A::Pagina(P::Raciocinio))
        .unwrap();
    let mut outcome = ClientShellInput::default();
    state.activate_global_menu_item(item, &mut outcome);
    let Some(ClientShellOverlay::GlobalMenu(menu)) = &state.overlay else {
        panic!("menu de raciocínio");
    };
    assert_eq!(
        menu.items(state.snapshot.as_deref().unwrap())
            .iter()
            .map(|(texto, _)| *texto)
            .collect::<Vec<_>>(),
        vec!["Trocar", "Autorizar", "Configurar", "Voltar"]
    );
    state.activate_global_menu_item(2, &mut outcome);
    let Some(ClientShellOverlay::GlobalMenu(menu)) = &state.overlay else {
        panic!("perfis de uso");
    };
    assert_eq!(menu.highlighted, 1);
    let itens = menu.items(state.snapshot.as_deref().unwrap());
    assert!(itens[1].0.contains("Especialista (Padrão)"));
    assert!(itens[2].0.contains("Casos complexos"));
    let frame = state.compose(120, 30).unwrap();
    let texto = frame
        .cells
        .iter()
        .map(|celula| celula.symbol.as_str())
        .collect::<String>();
    assert!(texto.contains("Especialista (Padrão)"));
}

#[test]
fn troca_de_raciocinio_destaca_o_provedor_atual() {
    use crate::client::shell::global_menu::PaginaRaciocinio as P;
    let mut config = Config::default();
    config.pretorian.agente = Some("claude".into());
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&config));
    state.set_snapshot(Box::new(snapshot()));
    state.overlay = Some(ClientShellOverlay::GlobalMenu(ClientGlobalMenuOverlay {
        pagina: P::Raciocinio,
        provedores: vec!["codex", "claude"],
        ..Default::default()
    }));
    state.activate_global_menu_item(0, &mut ClientShellInput::default());
    let Some(ClientShellOverlay::GlobalMenu(menu)) = &state.overlay else {
        panic!("seleção de provedor");
    };
    assert_eq!(menu.pagina, P::Trocar);
    assert_eq!(menu.highlighted, 1);
    assert_eq!(
        menu.items(state.snapshot.as_deref().unwrap())[menu.highlighted].0,
        "Claude"
    );
}

#[test]
fn raciocinio_mostra_configuracao_repetida_sem_ocultar_os_niveis() {
    use crate::client::shell::global_menu::{ClientGlobalMenuAction as A, PaginaRaciocinio as P};
    let menu = ClientGlobalMenuOverlay {
        pagina: P::Configurar,
        especialista_doutor_iguais: true,
        ..Default::default()
    };
    let snap = snapshot();
    let itens = menu.items(&snap);
    assert_eq!(itens.len(), 5);
    assert_eq!(itens[2].1, A::Nivel(crate::config::NivelRaciocinio::Doutor));
    assert!(itens[3].0.contains("mesma configuração"));
    assert_eq!(itens[3].1, A::Nada);
    assert_eq!(itens[4].1, A::Pagina(P::Raciocinio));
}

fn secundarias(saida: &ClientShellInput) -> Vec<crate::api::schema::ClientShellSecondarySetParams> {
    saida
        .actions
        .iter()
        .filter_map(|acao| match acao {
            ClientShellAction::Endpoint { request, .. } => match &request.method {
                crate::api::schema::Method::ClientShellSecondarySet(params) => Some(params.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

#[test]
fn menu_layout_escolhe_os_quatro_arranjos_pedindo_a_divisao_ao_servidor() {
    use super::super::arranjo::{Arranjo, LARGURA_APOIO};
    use crate::api::schema::SplitDirection;
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    state.compose(140, 30).expect("frame inicial");
    let inicial = state.layout(140, 30);
    let posicao = |state: &ClientShellState, rotulo: &str| {
        let Some(ClientShellOverlay::GlobalMenu(menu)) = &state.overlay else {
            panic!("menu aberto");
        };
        menu.items(state.snapshot.as_deref().unwrap())
            .iter()
            .position(|(texto, _)| texto.ends_with(rotulo))
            .unwrap_or_else(|| panic!("item {rotulo}"))
    };
    let escolher = |state: &mut ClientShellState, rotulo: &str| {
        let mut saida = ClientShellInput::default();
        state.toggle_global_menu();
        let layout = posicao(state, "Layout");
        state.activate_global_menu_item(layout, &mut saida);
        let item = posicao(state, rotulo);
        state.activate_global_menu_item(item, &mut saida);
        saida
    };

    let com_apoio = escolher(&mut state, "2  Barras à Esquerda e à Direita");
    assert_eq!(state.arranjo.atual, Arranjo::ComApoio);
    assert!(com_apoio.resize, "o servidor precisa saber da nova largura");
    assert_eq!(secundarias(&com_apoio)[0].direction, None);
    let layout = state.layout(140, 30);
    assert_eq!(layout.apoio.width, LARGURA_APOIO);
    assert_eq!(
        layout.pane_surface.width,
        inicial.pane_surface.width - LARGURA_APOIO
    );

    let lado = escolher(&mut state, "3  Barras e Divisão Vertical Central");
    let [pedido] = &secundarias(&lado)[..] else {
        panic!("um pedido de divisão");
    };
    assert_eq!(
        (pedido.direction.clone(), pedido.workspace_id.clone()),
        (Some(SplitDirection::Right), None)
    );
    // Lado a lado, as abas da principal ficam só sobre a metade dela.
    let layout = state.layout(140, 30);
    let (metade, _, _) = crate::ui::dividir_superficie(layout.pane_surface, true);
    assert_eq!(layout.tab_bar.width, metade.width);
    state.set_pane_surface(surface());
    let tela = frame_rows(&state.compose(140, 30).expect("dividido").frame).join("\n");
    assert!(tela.contains("Arraste uma pasta para cá"));
    assert_ne!(state.hits.fechar_secundaria, Rect::default());

    let embaixo = escolher(&mut state, "4  Barras e Divisão Horizontal Central");
    assert_eq!(
        secundarias(&embaixo)[0].direction,
        Some(SplitDirection::Down)
    );
    assert_eq!(
        state.layout(140, 30).tab_bar.width,
        layout.pane_surface.width
    );

    let volta = escolher(&mut state, "1  Barra à Esquerda");
    assert!(volta.resize);
    assert_eq!(secundarias(&volta)[0].direction, None);
    assert_eq!(state.layout(140, 30).apoio.width, 0);
    state.set_pane_surface(surface());
    state.compose(140, 30).expect("apoio oculto");
    assert!(state.hits.apoio_area.is_empty());
    assert!(state.hits.apoio_rows.is_empty());
    assert!(state.hits.apoio_abas.is_empty());
    assert!(state.hits.apoio_adicionar.is_empty());

    // Em tela estreita a barra de apoio não espreme a conversa nem deixa alvos invisíveis.
    state.arranjo.atual = Arranjo::ComApoio;
    state.compose(140, 30).expect("apoio visível novamente");
    assert!(!state.hits.apoio_area.is_empty());
    assert_eq!(state.layout(80, 30).apoio.width, 0);
    state.compose(80, 30).expect("apoio oculto pela largura");
    assert!(state.hits.apoio_area.is_empty());
    assert!(state.hits.apoio_rows.is_empty());
    assert!(state.hits.apoio_abas.is_empty());
    assert!(state.hits.apoio_adicionar.is_empty());
}

#[test]
fn pasta_na_janela_secundaria_tem_abas_mais_fechar_e_teclado_proprios() {
    use super::super::arranjo::Arranjo;
    use crate::api::schema::{Method, SplitDirection};
    let mut duas_pastas = snapshot();
    let mut pasta = duas_pastas.workspaces[0].clone();
    pasta.workspace_id = "ws_2".into();
    pasta.label = "lsa-pje".into();
    pasta.active_tab_id = "tab_2".into();
    pasta.focused = false;
    let mut aba = duas_pastas.tabs[0].clone();
    aba.tab_id = "tab_2".into();
    aba.workspace_id = "ws_2".into();
    aba.focused = false;
    let mut painel = duas_pastas.panes[0].clone();
    painel.pane_id = "pane_9".into();
    painel.workspace_id = "ws_2".into();
    painel.tab_id = "tab_2".into();
    painel.focused = false;
    duas_pastas.workspaces.push(pasta);
    duas_pastas.tabs.push(aba);
    duas_pastas.panes.push(painel);
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(duas_pastas.clone()));
    state.set_pane_surface(surface());
    state.compose(140, 30).expect("frame inicial");
    let clicar = |state: &mut ClientShellState, alvo: Rect| {
        state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: alvo.x + 1,
            row: alvo.y,
            modifiers: KeyModifiers::empty(),
        })])
    };

    state.open_workspace_context_menu("ws_2".into(), 2, 4);
    let Some(ClientShellOverlay::ContextMenu(menu)) = &state.overlay else {
        panic!("menu de contexto da pasta");
    };
    let indice = menu
        .items()
        .iter()
        .position(|item| item.label == "Abrir na Janela Secundária")
        .expect("item no menu da pasta");
    state.compose(140, 30).expect("Menu");
    let linha = state.hits.context_menu_rows[indice].0;
    let puxar = clicar(&mut state, linha);
    assert_eq!(state.arranjo.atual, Arranjo::LadoALado);
    assert!(puxar.resize);
    let [pedido] = &secundarias(&puxar)[..] else {
        panic!("um pedido de secundária");
    };
    assert_eq!(
        (
            pedido.direction.clone(),
            pedido.workspace_id.as_deref(),
            pedido.focus
        ),
        (Some(SplitDirection::Right), Some("ws_2"), false)
    );

    state.set_pane_surface(surface());
    let tela = frame_rows(&state.compose(140, 30).expect("secundária").frame).join("\n");
    assert!(tela.contains("● lsa-pje"), "{tela}");
    let abas = state.hits.abas_secundaria.clone();
    let [(aba, tab_id)] = &abas[..] else {
        panic!("uma aba na barra da secundária");
    };
    let aba = *aba;
    assert_eq!(tab_id, "tab_2");
    let trocar = clicar(&mut state, aba);
    assert_eq!(secundarias(&trocar)[0].tab_id.as_deref(), Some("tab_2"));

    let alvo = state.hits.nova_aba_secundaria;
    assert!(clicar(&mut state, alvo).actions.is_empty());
    state.insert_overlay_text("Revisão");
    let nova = state.handle_input_bytes(b"\r");
    assert!(matches!(
        &nova.actions[..],
        [ClientShellAction::Endpoint { request, .. }] if matches!(&request.method,
            Method::TabCreate(params) if params.workspace_id.as_deref() == Some("ws_2") && !params.focus)
    ));
    // A aba criada vira a ativa da secundária quando chega no snapshot.
    let mut com_nova = duas_pastas.clone();
    let mut nova_aba = com_nova.tabs[1].clone();
    nova_aba.tab_id = "tab_3".into();
    com_nova.tabs.push(nova_aba);
    state.set_snapshot(Box::new(com_nova));
    let mut tique = ClientShellInput::default();
    state.tick_arranjo(&mut tique);
    assert_eq!(secundarias(&tique)[0].tab_id.as_deref(), Some("tab_3"));

    // Teclado: painel da secundária recebe sem trocar a pasta principal.
    let mut saida = ClientShellInput::default();
    assert!(state.foco_por_clique("pane_9", &mut saida));
    assert!(secundarias(&saida)[0].focus);
    assert_eq!(state.focused_pane_id().as_deref(), Some("pane_9"));
    // O painel da secundária fecha: o teclado volta à principal.
    let mut sem_painel = duas_pastas.clone();
    sem_painel.panes.retain(|pane| pane.pane_id != "pane_9");
    state.set_snapshot(Box::new(sem_painel));
    let mut tique = ClientShellInput::default();
    state.tick_arranjo(&mut tique);
    assert!(!secundarias(&tique)[0].focus);
    assert_eq!(state.focused_pane_id().as_deref(), Some("pane_1"));
    // A principal muda de foco por atalho ou aba: o teclado também volta.
    state.set_snapshot(Box::new(duas_pastas.clone()));
    assert!(state.foco_por_clique("pane_9", &mut ClientShellInput::default()));
    let mut outro_foco = duas_pastas.clone();
    outro_foco.focused_pane_id = Some("pane_7".into());
    state.set_snapshot(Box::new(outro_foco));
    let mut tique = ClientShellInput::default();
    state.tick_arranjo(&mut tique);
    assert!(!secundarias(&tique)[0].focus);
    assert_eq!(state.focused_pane_id().as_deref(), Some("pane_7"));
    state.set_snapshot(Box::new(duas_pastas.clone()));
    assert!(state.foco_por_clique("pane_9", &mut ClientShellInput::default()));
    let mut saida = ClientShellInput::default();
    assert!(!state.foco_por_clique("pane_1", &mut saida));
    assert!(!secundarias(&saida)[0].focus);
    assert_eq!(state.focused_pane_id().as_deref(), Some("pane_1"));

    // Em tela estreita o apoio some, mas a barra da secundária continua com o ×.
    assert_eq!(state.layout(80, 30).apoio.width, 0);
    state.compose(80, 30).expect("secundária sem apoio");
    assert!(state.hits.apoio_area.is_empty());
    assert!(!state.hits.fechar_secundaria.is_empty());

    state.compose(140, 30).expect("antes de fechar");
    let alvo = state.hits.fechar_secundaria;
    let fechar = clicar(&mut state, alvo);
    assert_eq!(state.arranjo.atual, Arranjo::ComApoio);
    assert_eq!(secundarias(&fechar)[0].direction, None);
}

#[test]
fn criacao_nome_cancelamento_e_nivel_preservam_destino() {
    std::env::set_var("PRETORIAN_PAINEL_GERENCIADO", "1");
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    for workspace in [false, true] {
        for cancel in [b"\x1b".as_slice(), b"\r".as_slice()] {
            if workspace {
                state.open_new_workspace_overlay();
            } else {
                state.open_new_tab_for_workspace("destino".into());
            }
            assert!(state.handle_input_bytes(cancel).actions.is_empty());
            assert!(state.overlay.is_none());
        }
    }
    state.open_new_tab_for_workspace("destino".into());
    state.insert_overlay_text("Análise 😀");
    assert!(state.handle_input_bytes(b"\r").actions.is_empty());
    assert!(matches!(
        state.overlay,
        Some(ClientShellOverlay::GlobalMenu(_))
    ));
    let mut changed = snapshot();
    changed.focused_workspace_id = Some("outra-pasta".into());
    state.set_snapshot(Box::new(changed));
    let mut outcome = ClientShellInput::default();
    state.activate_global_menu_item(1, &mut outcome);
    std::env::remove_var("PRETORIAN_PAINEL_GERENCIADO");
    let [ClientShellAction::Endpoint { request, .. }] = outcome.actions.as_slice() else {
        panic!("uma criação");
    };
    assert!(
        matches!(&request.method, crate::api::schema::Method::TabCreate(params)
        if params.workspace_id.as_deref() == Some("destino")
            && params.label.as_deref() == Some("Análise 😀")
            && params.env.contains_key("PRETORIAN_NIVEL_ABA"))
    );
}

#[test]
fn contexto_nova_aba_nao_foca_nem_cria_antes_do_nome() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.open_tab_context_menu("tab_1".into(), 0, 0);
    let mut outcome = ClientShellInput::default();
    state.activate_context_menu_item(0, &mut outcome);
    assert!(outcome.actions.is_empty());
    assert!(matches!(state.overlay, Some(ClientShellOverlay::Rename(_))));
    state.insert_overlay_text("Nome escolhido");
    state.save_rename_overlay(&mut outcome);
    assert!(
        matches!(outcome.actions.as_slice(), [ClientShellAction::Endpoint { request, .. }]
        if matches!(&request.method, crate::api::schema::Method::TabCreate(params)
            if params.label.as_deref() == Some("Nome escolhido")))
    );
}

#[test]
fn cabecalho_centauro_mostra_dados_explicitos_e_so_nome_negrito() {
    let mut config = ClientShellConfig::from_config(&Config::default());
    config.nome_usuario = Some("Pessoa Sintética".into());
    config.profissao_centauro = Some("Profissão Explícita".into());
    config.rotulo_centauro = Some("Centaura Admin".into());
    let mut state = ClientShellState::new(config);
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    let frame = state.compose(140, 30).expect("cabeçalho");
    let text = frame_rows(&frame.frame).join("\n");
    assert!(
        text.contains("Pessoa Sintética")
            && text.contains("Profissão Explícita")
            && text.contains("Centaura Admin")
    );
    let buffer = frame.to_ratatui_buffer().expect("buffer");
    let area = state.layout(140, 30).sidebar;
    assert_eq!(state.hits.new_workspace.y, area.y + 4);
    assert_eq!(state.hits.global_launcher.y, area.y + 4);
    assert_eq!(buffer[(area.x + 1, area.y + 3)].symbol(), " ");
    assert!(buffer[(area.x + 1, area.y)]
        .modifier
        .contains(Modifier::BOLD));
    assert!(!buffer[(area.x + 1, area.y + 1)]
        .modifier
        .contains(Modifier::BOLD));
    assert!(!buffer[(area.x + 1, area.y + 2)]
        .modifier
        .contains(Modifier::BOLD));
}

#[test]
fn acoes_no_topo_nao_sobrepoem_lista_menu_ou_assinatura() {
    for collapsed in [false, true] {
        for width in [18, 20, 24, 36] {
            for rows in [8, 12, 30] {
                let mut config = Config::default();
                config.ui.sidebar_width = width;
                let mut state = ClientShellState::new(ClientShellConfig::from_config(&config));
                state.sidebar_collapsed = collapsed;
                state.set_snapshot(Box::new(snapshot()));
                state.set_pane_surface(surface());
                let frame = state.compose(140, rows).expect("barra");
                let sidebar = state.layout(140, rows).sidebar;
                let new = state.hits.new_workspace;
                let menu = state.hits.global_launcher;
                assert_eq!(new.y, sidebar.y + if collapsed { 0 } else { 2 });
                assert_eq!(menu.y, new.y);
                assert!(!new.is_empty() && !menu.is_empty());
                assert!(new.right() <= menu.x && menu.right() < sidebar.right());
                assert!(state.hits.workspaces.iter().all(|hit| hit.rect.y > new.y));
                if !collapsed {
                    let footer = frame_rows(&frame.frame).pop().unwrap();
                    assert!(footer.contains("nεαd.tεch") && !footer.contains('⌘'));
                    assert!(state.hits.workspace_body.bottom() < rows - 1);
                }
                state.toggle_global_menu();
                state.compose(140, rows).expect("menu no topo");
                assert!(!state.hits.global_menu_rows.is_empty());
                assert!(state
                    .hits
                    .global_menu_rows
                    .iter()
                    .all(|(rect, _)| rect.bottom() <= rows));
            }
        }
    }
}

#[test]
fn dialogo_nome_mantem_salvar_cancelar_e_limpeza_pelo_teclado() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    state.open_new_tab_overlay();
    state.insert_overlay_text("Nome provisório");
    let frame = state.compose(100, 20).expect("nome");
    let text = frame_rows(&frame.frame).join("\n");
    assert!(text.contains("Salvar") && text.contains("Cancelar"));
    assert!(!text.contains("Limpar"));
    assert!(state.hits.overlay_clear.is_empty());
    state.handle_input_bytes(b"\x03");
    assert!(state.handle_input_bytes(b"\r").actions.is_empty());
}

#[test]
fn botoes_das_pastas_tem_hierarquia_hover_foco_e_clique() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    for theme in ["pretorian", "pink", "zendataops"] {
        let mut config = Config::default();
        config.theme.name = Some(theme.into());
        let mut state = ClientShellState::new(ClientShellConfig::from_config(&config));
        state.set_snapshot(Box::new(snapshot()));
        state.set_pane_surface(surface());
        let frame = state.compose(140, 30).unwrap();
        let new = state.hits.new_workspace;
        let menu = state.hits.global_launcher;
        let palette = &state.config.palette;
        let buffer = frame.to_ratatui_buffer().unwrap();
        assert_eq!(buffer[(new.x, new.y)].bg, palette.surface0);
        assert_eq!(buffer[(menu.x, menu.y)].bg, palette.sidebar_bg);
        let text = frame_rows(&frame.frame).join("\n");
        assert!(text.contains("+ Nova Pasta") && text.contains("Menu ▾"));
        for (target, expected, is_menu) in [
            (new, state.config.palette.selection_bg, false),
            (menu, state.config.palette.surface1, true),
        ] {
            let event = MouseEvent {
                kind: MouseEventKind::Moved,
                column: target.x,
                row: target.y,
                modifiers: KeyModifiers::NONE,
            };
            assert!(
                state
                    .handle_raw_events(vec![RawInputEvent::Mouse(event)])
                    .repaint
            );
            let frame = state.compose(140, 30).unwrap();
            assert_eq!(
                frame.to_ratatui_buffer().unwrap()[(target.x, target.y)].bg,
                expected
            );
            state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                ..event
            })]);
            assert!(if is_menu {
                matches!(state.overlay, Some(ClientShellOverlay::GlobalMenu(_)))
            } else {
                matches!(state.overlay, Some(ClientShellOverlay::Rename(_)))
            });
            state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
                column: 139,
                row: 29,
                ..event
            })]);
            let frame = state.compose(140, 30).unwrap();
            assert_eq!(
                frame.to_ratatui_buffer().unwrap()[(target.x, target.y)].bg,
                expected
            );
            state.handle_input_bytes(b"\x1b");
            let frame = state.compose(140, 30).unwrap();
            assert_eq!(
                frame.to_ratatui_buffer().unwrap()[(menu.x, menu.y)].bg,
                state.config.palette.sidebar_bg
            );
        }
    }
}

#[test]
fn botoes_compactos_preservam_alvos_separados_com_alerta() {
    for width in 4..42 {
        for attention in [false, true] {
            let area = Rect::new(0, 0, width, 4);
            let mut buffer = ratatui::buffer::Buffer::empty(area);
            let mut hits = ShellHitMap::default();
            super::super::sidebar::render_sidebar_actions(
                &mut buffer,
                area,
                attention,
                &Palette::pretorian(),
                None,
                &mut hits,
            );
            assert!(hits.new_workspace.right() < hits.global_launcher.x);
            assert!(hits.global_launcher.right() < area.right());
            assert_eq!(hits.new_workspace.height, 1);
            assert_eq!(hits.global_launcher.height, 1);
        }
    }
}
