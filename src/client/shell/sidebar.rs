use super::*;
use ratatui::{
    text::Line,
    widgets::{Paragraph, Widget},
};

fn workspace_selection_background(palette: &Palette) -> ratatui::style::Color {
    if palette.selection_bg == ratatui::style::Color::Reset {
        palette.active_row_bg
    } else {
        palette.selection_bg
    }
}

pub(in crate::client::shell) fn workspace_active_background(
    palette: &Palette,
    navigating: bool,
) -> ratatui::style::Color {
    // The fallback cursor shares the active-row color; only fill the cursor while navigating.
    if navigating && palette.selection_bg == ratatui::style::Color::Reset {
        palette.sidebar_bg
    } else {
        palette.active_row_bg
    }
}

pub(in crate::client::shell) fn collapsed_sidebar_sections(
    area: Rect,
    show_agent_panel: bool,
) -> (Rect, Option<u16>, Rect) {
    let content = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
    if content.is_empty() {
        return (Rect::default(), None, Rect::default());
    }
    if !show_agent_panel || content.height < 7 {
        return (content, None, Rect::default());
    }
    let workspace_height = content.height.div_ceil(2);
    let divider_y = content.y + workspace_height;
    let detail_height = content.height.saturating_sub(workspace_height + 1);
    (
        Rect::new(content.x, content.y, content.width, workspace_height),
        Some(divider_y),
        Rect::new(content.x, divider_y + 1, content.width, detail_height),
    )
}

/// Divide a barra expandida em área de trabalho, seção de agentes e divisória arrastável.
/// Sem a seção de agentes, a área de trabalho ocupa a altura toda.
pub(in crate::client::shell) fn expanded_sidebar_sections(
    area: Rect,
    split_ratio: f32,
    show_agent_panel: bool,
) -> (Rect, Rect, Rect) {
    if !show_agent_panel {
        return (
            Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height),
            Rect::default(),
            Rect::default(),
        );
    }
    let (workspace_area, detail_area) = crate::ui::expanded_sidebar_sections(area, split_ratio);
    (
        workspace_area,
        detail_area,
        crate::ui::sidebar_section_divider_rect(area, split_ratio),
    )
}

/// Seção de baixo da barra nos arranjos divididos: a pasta na metade secundária da conversa,
/// ou o convite para arrastar uma pasta até aqui.
fn render_janela_secundaria(
    buffer: &mut Buffer,
    area: Rect,
    rotulo: Option<&str>,
    palette: &Palette,
) {
    if area.height < 3 {
        return;
    }
    put_text(
        buffer,
        area.x,
        area.y,
        area.width,
        " Janela Secundária",
        Style::default()
            .fg(palette.text)
            .add_modifier(Modifier::BOLD),
    );
    let x = area.x.saturating_add(1);
    let largura = area.width.saturating_sub(1);
    match rotulo {
        Some(rotulo) => {
            put_text(
                buffer,
                x,
                area.y + 2,
                2,
                "● ",
                Style::default().fg(palette.accent),
            );
            put_text(
                buffer,
                x + 2,
                area.y + 2,
                largura.saturating_sub(2),
                rotulo,
                Style::default().fg(palette.text),
            );
        }
        None => {
            let convite = Style::default()
                .fg(palette.subtext0)
                .add_modifier(Modifier::ITALIC);
            put_text(
                buffer,
                x,
                area.y + 2,
                largura,
                "Arraste uma pasta aqui",
                convite,
            );
            put_text(
                buffer,
                x,
                area.y + 3,
                largura,
                "ou use o menu dela.",
                convite,
            );
        }
    }
}

pub(in crate::client::shell) fn render_sidebar_actions(
    buffer: &mut Buffer,
    area: Rect,
    attention: bool,
    palette: &Palette,
    hits: &mut ShellHitMap,
) -> Rect {
    let width = area.width.saturating_sub(1);
    if width < 3 || area.height == 0 {
        return area;
    }
    let menu_label = if width < 8 {
        "≡"
    } else if attention {
        "● Menu<<"
    } else {
        "Menu<<"
    };
    let menu_width = if width < 8 {
        1
    } else if attention {
        8.min(width.saturating_sub(11).max(6))
    } else {
        6
    };
    let compact = width < 11 + menu_width;
    let new_label = if compact { "+" } else { "+Nova Pasta" };
    hits.new_workspace = Rect::new(area.x, area.y, if compact { 1 } else { 11 }, 1);
    hits.global_launcher = Rect::new(area.x + width - menu_width, area.y, menu_width, 1);
    let button_style = Style::default()
        .fg(palette.accent)
        .bg(palette.surface0)
        .remove_modifier(Modifier::BOLD);
    buffer.set_style(hits.new_workspace, button_style);
    buffer.set_style(hits.global_launcher, button_style);
    put_text(
        buffer,
        hits.new_workspace.x,
        area.y,
        hits.new_workspace.width,
        new_label,
        button_style,
    );
    put_right_text(
        buffer,
        hits.global_launcher,
        area.y,
        menu_label,
        button_style,
    );
    Rect::new(area.x, area.y + 1, area.width, area.height - 1)
}

pub(crate) fn render_collapsed_sidebar(
    buffer: &mut Buffer,
    area: Rect,
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    selected_workspace_id: Option<&str>,
    hits: &mut ShellHitMap,
) {
    let palette = &config.palette;
    let selection_background = workspace_selection_background(palette);
    let active_background = workspace_active_background(palette, selected_workspace_id.is_some());
    render_sidebar_background(buffer, area, palette);
    let content = if config.mouse_capture {
        render_sidebar_actions(
            buffer,
            area,
            super::super::global_menu::global_menu_attention(snapshot),
            palette,
            hits,
        )
    } else {
        area
    };
    let content = Rect {
        height: content
            .height
            .saturating_sub(u16::from(!config.show_agent_panel)),
        ..content
    };
    let (workspace_area, divider_y, detail_area) =
        collapsed_sidebar_sections(content, config.show_agent_panel);
    for (index, workspace) in snapshot
        .workspaces
        .iter()
        .take(workspace_area.height as usize)
        .enumerate()
    {
        let rect = Rect::new(
            workspace_area.x,
            workspace_area.y + index as u16,
            workspace_area.width,
            1,
        );
        let selected = selected_workspace_id == Some(workspace.workspace_id.as_str());
        if selected {
            buffer.set_style(rect, Style::default().bg(selection_background));
        } else if workspace.focused {
            buffer.set_style(rect, Style::default().bg(active_background));
        }
        let number_style = if selected {
            Style::default()
                .fg(palette.overlay1)
                .bg(selection_background)
        } else if workspace.focused {
            Style::default().fg(palette.text).bg(active_background)
        } else {
            Style::default().fg(palette.overlay0)
        };
        put_text(
            buffer,
            rect.x,
            rect.y,
            rect.width.min(2),
            &format!("{:<2}", index + 1),
            number_style,
        );
        let status = workspace.agent_status;
        put_text(
            buffer,
            rect.x.saturating_add(2),
            rect.y,
            rect.width.saturating_sub(2),
            status_icon(status, config.status_indicators),
            Style::default().fg(status_color(status, palette)),
        );
        hits.workspaces.push(WorkspaceHit {
            rect,
            endpoint_id: ClientEndpointId::Local,
            workspace_id: workspace.workspace_id.clone(),
            indented: false,
            group_toggle: None,
        });
    }

    if let Some(divider_y) = divider_y {
        put_text(
            buffer,
            workspace_area.x,
            divider_y,
            workspace_area.width,
            &"─".repeat(workspace_area.width as usize),
            Style::default().fg(palette.surface_dim),
        );
    }

    let detail_content = Rect::new(
        detail_area.x,
        detail_area.y,
        detail_area.width,
        detail_area.height.saturating_sub(1),
    );
    for (index, pane_id) in super::ordered_agent_pane_ids(snapshot, config.agent_panel_sort)
        .into_iter()
        .take(detail_content.height as usize)
        .enumerate()
    {
        let Some(agent) = snapshot
            .agents
            .iter()
            .find(|agent| agent.pane_id == pane_id)
        else {
            continue;
        };
        let rect = Rect::new(
            detail_content.x,
            detail_content.y + index as u16,
            detail_content.width,
            1,
        );
        if agent.focused {
            buffer.set_style(rect, Style::default().bg(palette.active_row_bg));
        }
        put_text(
            buffer,
            rect.x,
            rect.y,
            rect.width.min(2),
            &format!("{:<2}", index + 1),
            Style::default().fg(if agent.focused {
                palette.text
            } else {
                palette.overlay0
            }),
        );
        put_text(
            buffer,
            rect.x.saturating_add(2),
            rect.y,
            rect.width.saturating_sub(2),
            status_icon(agent.agent_status, config.status_indicators),
            Style::default().fg(status_color(agent.agent_status, palette)),
        );
        hits.agents.push((rect, pane_id));
    }
    hits.sidebar_toggle = if area.is_empty() || workspace_area.width == 0 {
        Rect::default()
    } else {
        Rect::new(
            workspace_area.x + workspace_area.width / 2,
            area.bottom().saturating_sub(1),
            1,
            1,
        )
    };
    put_text(
        buffer,
        hits.sidebar_toggle.x,
        hits.sidebar_toggle.y,
        hits.sidebar_toggle.width,
        "»",
        if super::super::global_menu::global_menu_attention(snapshot) {
            Style::default()
                .fg(palette.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette.overlay0)
        },
    );
}

pub(in crate::client::shell) fn render_centauro_header(
    buffer: &mut Buffer,
    area: Rect,
    config: &ClientShellConfig,
) -> u16 {
    let lines = [
        config.nome_usuario.as_deref().unwrap_or("Área de Trabalho"),
        config.profissao_centauro.as_deref().unwrap_or(""),
        config.rotulo_centauro.as_deref().unwrap_or(""),
    ];
    let count = if config.nome_usuario.is_some() { 3 } else { 1 };
    for (index, line) in lines.iter().take(count).enumerate() {
        if index as u16 >= area.height {
            break;
        }
        let style = Style::default().fg(config.palette.text);
        put_text(
            buffer,
            area.x.saturating_add(1),
            area.y + index as u16,
            area.width.saturating_sub(1),
            line,
            if index == 0 {
                style.add_modifier(Modifier::BOLD)
            } else {
                style
            },
        );
    }
    count as u16 + 1
}

pub(crate) fn render_sidebar(
    buffer: &mut Buffer,
    area: Rect,
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    state: &mut ShellRenderState<'_>,
    hits: &mut ShellHitMap,
) {
    let palette = &config.palette;
    render_sidebar_background(buffer, area, palette);
    hits.sidebar_divider = if area.is_empty() {
        Rect::default()
    } else {
        Rect::new(area.right().saturating_sub(1), area.y, 1, area.height)
    };
    let dividir = config.show_agent_panel || state.janela_secundaria.is_some();
    let (workspace_area, detail_area, section_divider) =
        expanded_sidebar_sections(area, state.sidebar_section_split, dividir);
    hits.sidebar_section_divider = section_divider;
    let header_rows = render_centauro_header(buffer, workspace_area, config);
    let workspace_area = Rect::new(
        workspace_area.x,
        workspace_area
            .y
            .saturating_add(header_rows.min(workspace_area.height)),
        workspace_area.width,
        workspace_area.height.saturating_sub(header_rows),
    );
    let workspace_area = if config.mouse_capture {
        render_sidebar_actions(
            buffer,
            workspace_area,
            super::super::global_menu::global_menu_attention(snapshot),
            palette,
            hits,
        )
    } else {
        workspace_area
    };

    let entries = workspace_entries(snapshot, state.collapsed_groups);
    let body = Rect::new(
        workspace_area.x,
        workspace_area.y,
        workspace_area.width,
        workspace_area.height.saturating_sub(1),
    );
    hits.workspace_body = body;
    let row_heights = entries
        .iter()
        .map(|entry| {
            snapshot
                .workspaces
                .get(entry.index)
                .map(|workspace| {
                    workspace_rows(
                        workspace,
                        displayed_workspace_status(snapshot, workspace, state.collapsed_groups),
                        entry.indented,
                        &config.spaces,
                    )
                    .len()
                    .max(1)
                    .min(u16::MAX as usize) as u16
                })
                .unwrap_or(1)
        })
        .collect::<Vec<_>>();
    let gaps = entries
        .iter()
        .enumerate()
        .map(|(index, _)| {
            entries
                .get(index + 1)
                .map_or(0, |next| u16::from(!next.indented) * config.spaces.row_gap)
        })
        .collect::<Vec<_>>();
    let mut metrics = super::scroll::list_scroll_metrics(
        &row_heights,
        &gaps,
        body.height,
        *state.workspace_scroll,
    );
    if !body.is_empty() && std::mem::take(state.reveal_focused_workspace) {
        if let Some(target) = entries
            .iter()
            .position(|entry| snapshot.workspaces[entry.index].focused)
        {
            *state.workspace_scroll = super::scroll::list_scroll_start_to_reveal(
                &row_heights,
                &gaps,
                body.height,
                *state.workspace_scroll,
                target,
            );
            metrics = super::scroll::list_scroll_metrics(
                &row_heights,
                &gaps,
                body.height,
                *state.workspace_scroll,
            );
        }
    }
    hits.workspace_max_scroll = metrics.max_offset_from_bottom;
    hits.workspace_scroll_metrics = Some(metrics);
    *state.workspace_scroll = metrics
        .max_offset_from_bottom
        .saturating_sub(metrics.offset_from_bottom);
    let show_scrollbar = metrics.max_offset_from_bottom > 0 && body.width > 1;
    let content_width = body.width.saturating_sub(u16::from(show_scrollbar));
    let mut y = body.y;
    for (entry_position, entry) in entries.iter().enumerate().skip(*state.workspace_scroll) {
        let Some(workspace) = snapshot.workspaces.get(entry.index) else {
            continue;
        };
        let status = displayed_workspace_status(snapshot, workspace, state.collapsed_groups);
        let rows = workspace_rows(workspace, status, entry.indented, &config.spaces);
        let row_height = (rows.len().max(1).min(u16::MAX as usize) as u16).min(body.height);
        if y.saturating_add(row_height) > body.bottom() {
            break;
        }
        let rect = Rect::new(body.x, y, content_width, row_height);
        let selected = state.selected_workspace_id.is_some_and(|target| {
            target.matches(state.active_endpoint_id, &workspace.workspace_id)
        });
        let dragged = state.dragged_workspace_id == Some(workspace.workspace_id.as_str());
        if selected {
            buffer.set_style(rect, Style::default().bg(palette.selection_bg));
        } else if dragged {
            buffer.set_style(rect, Style::default().bg(palette.surface1));
        } else if workspace.focused {
            buffer.set_style(rect, Style::default().bg(palette.active_row_bg));
        }
        render_workspace_rows(
            buffer,
            rect,
            status,
            config.status_indicators,
            entry,
            rows,
            workspace.focused,
            selected,
            state.selected_workspace_id.is_some(),
            dragged,
            palette,
        );
        let group_toggle = render_parent_group_toggle(
            buffer,
            rect,
            snapshot,
            entry.index,
            state.collapsed_groups,
            palette,
        );
        hits.workspaces.push(WorkspaceHit {
            rect,
            endpoint_id: ClientEndpointId::Local,
            workspace_id: workspace.workspace_id.clone(),
            indented: entry.indented,
            group_toggle,
        });
        let gap = entries
            .get(entry_position + 1)
            .map_or(0, |next| u16::from(!next.indented) * config.spaces.row_gap);
        y = y.saturating_add(row_height + gap);
    }

    if show_scrollbar {
        let track = Rect::new(body.right().saturating_sub(1), body.y, 1, body.height);
        hits.workspace_scrollbar = track;
        super::scroll::render_list_scrollbar(buffer, track, metrics, palette);
    }

    if let Some(row) = state.workspace_drop_indicator_row.filter(|row| {
        *row >= workspace_area.y.saturating_add(1)
            && *row < workspace_area.bottom().saturating_sub(1)
    }) {
        put_text(
            buffer,
            body.x,
            row,
            body.width,
            &"─".repeat(body.width as usize),
            Style::default().fg(palette.accent),
        );
    }

    hits.janela_secundaria = Rect::default();
    if let Some(rotulo) = state.janela_secundaria {
        // A primeira linha da seção é o divisor arrastável, como no painel de agentes.
        let conteudo = Rect {
            y: detail_area.y.saturating_add(1),
            height: detail_area.height.saturating_sub(1),
            ..detail_area
        };
        render_janela_secundaria(buffer, conteudo, rotulo, palette);
        hits.janela_secundaria = detail_area;
        for x in section_divider.x..section_divider.right() {
            buffer[(x, section_divider.y)]
                .set_symbol("─")
                .set_style(Style::default().fg(palette.surface_dim));
        }
    } else if config.show_agent_panel {
        super::render_agent_panel(
            buffer,
            detail_area,
            snapshot,
            config,
            state.agent_scroll,
            hits,
        );
    }

    hits.sidebar_toggle = Rect::new(
        area.right().saturating_sub(2),
        area.bottom().saturating_sub(1),
        u16::from(area.width > 1),
        u16::from(area.height > 0),
    );
    put_text(
        buffer,
        hits.sidebar_toggle.x,
        hits.sidebar_toggle.y,
        hits.sidebar_toggle.width,
        "«",
        Style::default().fg(palette.overlay0),
    );
}

pub(crate) fn workspace_entries(
    snapshot: &ClientShellSnapshot,
    collapsed_groups: &HashSet<String>,
) -> Vec<WorkspaceEntry> {
    let mut members = HashMap::<&str, Vec<usize>>::new();
    for (index, workspace) in snapshot.workspaces.iter().enumerate() {
        if let Some(worktree) = &workspace.worktree {
            members.entry(&worktree.key).or_default().push(index);
        }
    }
    let grouped = members
        .iter()
        .filter(|(_, indices)| {
            indices.iter().any(|index| {
                snapshot.workspaces[*index]
                    .worktree
                    .as_ref()
                    .is_some_and(|worktree| worktree.is_linked_worktree)
            }) && indices.iter().any(|index| {
                snapshot.workspaces[*index]
                    .worktree
                    .as_ref()
                    .is_some_and(|worktree| !worktree.is_linked_worktree)
            })
        })
        .map(|(key, _)| *key)
        .collect::<HashSet<_>>();
    let mut emitted = HashSet::<&str>::new();
    let mut entries = Vec::new();
    for (index, workspace) in snapshot.workspaces.iter().enumerate() {
        let Some(worktree) = workspace
            .worktree
            .as_ref()
            .filter(|worktree| grouped.contains(worktree.key.as_str()))
        else {
            entries.push(WorkspaceEntry {
                index,
                indented: false,
                last_child: false,
            });
            continue;
        };
        if !emitted.insert(&worktree.key) {
            continue;
        }
        let Some(group_members) = members.get(worktree.key.as_str()) else {
            continue;
        };
        for parent in group_members.iter().copied().filter(|member| {
            snapshot.workspaces[*member]
                .worktree
                .as_ref()
                .is_some_and(|worktree| !worktree.is_linked_worktree)
        }) {
            entries.push(WorkspaceEntry {
                index: parent,
                indented: false,
                last_child: false,
            });
        }
        if collapsed_groups.contains(&worktree.key) {
            if let Some(active) = group_members.iter().copied().find(|member| {
                let workspace = &snapshot.workspaces[*member];
                workspace.focused
                    && workspace
                        .worktree
                        .as_ref()
                        .is_some_and(|worktree| worktree.is_linked_worktree)
            }) {
                entries.push(WorkspaceEntry {
                    index: active,
                    indented: true,
                    last_child: true,
                });
            }
            continue;
        }
        let children = group_members
            .iter()
            .copied()
            .filter(|member| {
                snapshot.workspaces[*member]
                    .worktree
                    .as_ref()
                    .is_some_and(|worktree| worktree.is_linked_worktree)
            })
            .collect::<Vec<_>>();
        for (child_index, child) in children.iter().enumerate() {
            entries.push(WorkspaceEntry {
                index: *child,
                indented: true,
                last_child: child_index + 1 == children.len(),
            });
        }
    }
    entries
}

fn parent_group_key(snapshot: &ClientShellSnapshot, index: usize) -> Option<String> {
    let workspace = snapshot.workspaces.get(index)?;
    let worktree = workspace.worktree.as_ref()?;
    if worktree.is_linked_worktree {
        return None;
    }
    snapshot
        .workspaces
        .iter()
        .any(|candidate| {
            candidate.worktree.as_ref().is_some_and(|candidate| {
                candidate.key == worktree.key && candidate.is_linked_worktree
            })
        })
        .then(|| worktree.key.clone())
}

pub(in crate::client::shell) fn workspace_close_is_group(
    snapshot: &ClientShellSnapshot,
    workspace: &ClientShellWorkspace,
) -> bool {
    let Some(worktree) = workspace
        .worktree
        .as_ref()
        .filter(|worktree| !worktree.is_linked_worktree)
    else {
        return false;
    };
    let mut has_child = false;
    for member in &snapshot.workspaces {
        if member.workspace_id == workspace.workspace_id {
            continue;
        }
        if let Some(candidate) = member
            .worktree
            .as_ref()
            .filter(|candidate| candidate.key == worktree.key)
        {
            if !candidate.is_linked_worktree {
                return false;
            }
            has_child = true;
        }
    }
    has_child
}

pub(in crate::client::shell) fn render_parent_group_toggle(
    buffer: &mut Buffer,
    workspace_rect: Rect,
    snapshot: &ClientShellSnapshot,
    workspace_index: usize,
    collapsed_groups: &HashSet<String>,
    palette: &Palette,
) -> Option<(Rect, String)> {
    let key = parent_group_key(snapshot, workspace_index)?;
    let toggle = Rect::new(
        workspace_rect.right().saturating_sub(1),
        workspace_rect.y,
        1,
        1,
    );
    put_text(
        buffer,
        toggle.x,
        toggle.y,
        toggle.width,
        if collapsed_groups.contains(&key) {
            "▸"
        } else {
            "▾"
        },
        Style::default().fg(palette.accent),
    );
    Some((toggle, key))
}

pub(in crate::client::shell) fn displayed_workspace_status(
    snapshot: &ClientShellSnapshot,
    workspace: &ClientShellWorkspace,
    collapsed_groups: &HashSet<String>,
) -> crate::api::schema::AgentStatus {
    let Some(worktree) = workspace
        .worktree
        .as_ref()
        .filter(|worktree| !worktree.is_linked_worktree)
    else {
        return workspace.agent_status;
    };
    if !collapsed_groups.contains(&worktree.key) {
        return workspace.agent_status;
    }
    snapshot
        .workspaces
        .iter()
        .filter(|candidate| {
            candidate.worktree.as_ref().is_some_and(|member| {
                member.key == worktree.key
                    && (member.is_linked_worktree
                        || candidate.workspace_id == workspace.workspace_id)
            })
        })
        .map(|candidate| candidate.agent_status)
        .max_by_key(|status| status_priority(*status))
        .unwrap_or(workspace.agent_status)
}

pub(in crate::client::shell) fn workspace_rows(
    workspace: &ClientShellWorkspace,
    status: crate::api::schema::AgentStatus,
    indented: bool,
    config: &SpacesSidebarConfig,
) -> Vec<Vec<crate::ui::ResolvedToken>> {
    let label = if indented && !workspace.custom_label {
        workspace
            .branch
            .as_deref()
            .and_then(|branch| branch.strip_prefix("worktree/").or(Some(branch)))
            .unwrap_or(&workspace.label)
    } else {
        &workspace.label
    };
    let token_values = workspace.tokens.iter().cloned().collect::<HashMap<_, _>>();
    crate::ui::sidebar_space_rows(
        config,
        crate::ui::SpaceTokenContext {
            workspace: label,
            branch: workspace.branch.as_deref(),
            state_text: status_text(status),
            ahead_behind: workspace.git_ahead_behind,
            tokens: &token_values,
            suppress_git_details: indented,
        },
    )
}

pub(in crate::client::shell) fn render_workspace_rows(
    buffer: &mut Buffer,
    area: Rect,
    status: crate::api::schema::AgentStatus,
    indicators: crate::config::StatusIndicatorStyle,
    entry: &WorkspaceEntry,
    rows: Vec<Vec<crate::ui::ResolvedToken>>,
    focused: bool,
    selected: bool,
    navigating: bool,
    dragged: bool,
    palette: &Palette,
) {
    for (row_index, row) in rows.iter().enumerate() {
        let y = area.y + row_index as u16;
        if y >= area.bottom() {
            break;
        }
        let mut x = area.x;
        if entry.indented {
            let prefix = if row_index == 0 {
                if entry.last_child {
                    "   └─ "
                } else {
                    "   ├─ "
                }
            } else if entry.last_child {
                "        "
            } else {
                "   │    "
            };
            x = put_segment(
                buffer,
                x,
                y,
                area.right(),
                prefix,
                Style::default().fg(palette.overlay0),
            );
        } else if row_index == 0 {
            x = x.saturating_add(1);
        } else {
            x = x.saturating_add(3);
        }
        let highlighted = focused || dragged;
        let workspace_style = Style::default()
            .fg(if highlighted {
                palette.text
            } else {
                palette.subtext0
            })
            .add_modifier(if highlighted {
                Modifier::BOLD
            } else {
                Modifier::empty()
            });
        let secondary_style = Style::default().fg(if focused {
            palette.mauve
        } else {
            palette.overlay0
        });
        let spans = crate::ui::resolved_token_spans(
            row,
            (
                status_icon(status, indicators),
                Style::default().fg(status_color(status, palette)),
            ),
            Style::default().fg(status_color(status, palette)),
            workspace_style,
            secondary_style,
            Style::default().fg(palette.overlay1),
            palette,
            area.right().saturating_sub(2).saturating_sub(x) as usize,
        );
        Paragraph::new(Line::from(spans)).render(
            Rect::new(x, y, area.right().saturating_sub(2).saturating_sub(x), 1),
            buffer,
        );
    }

    let background = if selected {
        Some(workspace_selection_background(palette))
    } else if dragged {
        Some(palette.surface1)
    } else if focused {
        Some(workspace_active_background(palette, navigating))
    } else {
        None
    };
    if let Some(background) = background {
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                buffer[(x, y)].set_bg(background);
            }
        }
    }
}
