use super::*;

fn choice_style(selected: bool, palette: &Palette) -> Style {
    if selected {
        Style::default()
            .fg(contrast(palette))
            .bg(palette.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(palette.text).bg(palette.panel_bg)
    }
}

fn draw_choice(
    buffer: &mut Buffer,
    rect: Rect,
    label: &str,
    selected: bool,
    current: bool,
    palette: &Palette,
) {
    let style = choice_style(selected, palette);
    buffer.set_style(rect, style);
    let marker = if selected { "▸" } else { " " };
    let current = if current { " ✓" } else { "" };
    put_text(
        buffer,
        rect.x,
        rect.y,
        rect.width,
        &format!(" {marker} {label}{current}"),
        style,
    );
}

pub(super) fn render_settings_overlay(
    buffer: &mut Buffer,
    settings: &ClientSettingsOverlay,
    integration_updates_available: bool,
    palette: &Palette,
) -> Option<OverlayRender> {
    let integration_height = 14u16
        .saturating_add(settings.integrations.len().max(1) as u16)
        .saturating_add(settings.integration_messages.len().min(6) as u16);
    let height = if settings.section == ClientSettingsSection::Usuario {
        34
    } else if settings.section == ClientSettingsSection::Integrations {
        integration_height.max(22)
    } else {
        22
    };
    let popup = popup(buffer.area, 76, height)?;
    let inner = panel(buffer, popup, palette.accent, palette.panel_bg)?;
    if inner.width < 20 || inner.height < 8 {
        return None;
    }

    put_text(
        buffer,
        inner.x,
        inner.y,
        inner.width,
        " Configurações",
        Style::default()
            .fg(palette.text)
            .bg(palette.panel_bg)
            .add_modifier(Modifier::BOLD),
    );

    let integration_badge = integration_updates_available
        || settings
            .integrations
            .iter()
            .any(|integration| integration.state == crate::api::schema::IntegrationState::Outdated);
    let mut tab_x = inner.x;
    let mut tab_hits = Vec::new();
    for section in ClientSettingsSection::ALL {
        let badge = (*section == ClientSettingsSection::Integrations && integration_badge)
            || (*section == ClientSettingsSection::Usuario
                && super::super::pretorian_dependencias::pede_atencao());
        let label = if badge {
            format!(" ● {} ", section.label())
        } else {
            format!(" {} ", section.label())
        };
        let width = display_width(&label).min(inner.right().saturating_sub(tab_x));
        let rect = Rect::new(tab_x, inner.y + 1, width, 1);
        let active = *section == settings.section;
        let style = if active {
            Style::default()
                .fg(contrast(palette))
                .bg(palette.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette.overlay1).bg(palette.panel_bg)
        };
        buffer.set_style(rect, style);
        put_text(buffer, rect.x, rect.y, rect.width, &label, style);
        if badge && !active {
            put_text(
                buffer,
                rect.x.saturating_add(1),
                rect.y,
                rect.width.saturating_sub(1).min(2),
                "● ",
                Style::default()
                    .fg(palette.accent)
                    .bg(palette.panel_bg)
                    .add_modifier(Modifier::BOLD),
            );
        }
        tab_hits.push((rect, *section));
        tab_x = tab_x.saturating_add(width.saturating_add(1));
        if tab_x >= inner.right() {
            break;
        }
    }
    put_text(
        buffer,
        inner.x,
        inner.y + 2,
        inner.width,
        &"─".repeat(inner.width as usize),
        Style::default().fg(palette.surface0).bg(palette.panel_bg),
    );

    let content = Rect::new(
        inner.x,
        inner.y + 4,
        inner.width,
        inner.height.saturating_sub(7),
    );
    let mut choice_hits = Vec::new();
    let mut cursor = None;
    match settings.section {
        ClientSettingsSection::Usuario => {
            cursor = render_usuario_section(buffer, content, settings, palette, &mut choice_hits);
        }
        ClientSettingsSection::Sound => {
            put_text(
                buffer,
                content.x,
                content.y,
                content.width,
                "Som",
                Style::default().fg(palette.text).bg(palette.panel_bg),
            );
            for (index, label) in [
                "Ligado",
                "Desligado",
                "Avisos Desligados",
                "Avisos no Pretorian",
                "Avisos no Terminal",
                "Avisos no Sistema",
            ]
            .iter()
            .enumerate()
            {
                let y = content.y + 1 + index as u16 + u16::from(index >= 2);
                if index == 2 {
                    put_text(
                        buffer,
                        content.x,
                        y - 1,
                        content.width,
                        "Avisos",
                        Style::default().fg(palette.text).bg(palette.panel_bg),
                    );
                }
                let rect = Rect::new(content.x, y, content.width, 1);
                let current = match index {
                    0 => settings.sound_enabled,
                    1 => !settings.sound_enabled,
                    2 => settings.toast_delivery == crate::config::ToastDelivery::Off,
                    3 => settings.toast_delivery == crate::config::ToastDelivery::Herdr,
                    4 => settings.toast_delivery == crate::config::ToastDelivery::Terminal,
                    _ => settings.toast_delivery == crate::config::ToastDelivery::System,
                };
                draw_choice(
                    buffer,
                    rect,
                    label,
                    settings.selected == index,
                    current,
                    palette,
                );
                choice_hits.push((rect, index));
            }
        }
        ClientSettingsSection::Integrations => {
            render_integrations(buffer, content, settings, palette);
        }
    }

    if settings.section == ClientSettingsSection::Usuario {
        let entries: &[(usize, &str)] = if settings.dependencias {
            &[(0, "Voltar"), (7, "Fechar")]
        } else {
            &[
                (3, "Salvar"),
                (4, "Apagar"),
                (5, "Recarregar"),
                (6, "Dependências"),
                (7, "Fechar"),
            ]
        };
        let widths: Vec<u16> = entries
            .iter()
            .map(|(_, label)| display_width(label) + 2)
            .collect();
        let buttons = row(inner, &widths, 1, inner.height.saturating_sub(1));
        let mut close = Rect::default();
        for ((index, label), rect) in entries.iter().zip(buttons) {
            button(
                buffer,
                rect,
                label,
                choice_style(settings.selected == *index, palette),
            );
            if *index == 7 {
                close = rect;
            } else {
                choice_hits.push((rect, *index));
            }
        }
        put_text(
            buffer,
            inner.x,
            inner.bottom().saturating_sub(2),
            inner.width,
            "Ctrl+S Salvar · Ctrl+R Recarregar",
            Style::default().fg(palette.overlay1).bg(palette.panel_bg),
        );
        return Some(OverlayRender {
            area: popup,
            cancel: close,
            settings_popup: popup,
            settings_tabs: tab_hits,
            settings_choices: choice_hits,
            cursor,
            ..OverlayRender::default()
        });
    }

    let installable = settings
        .integrations
        .iter()
        .any(super::super::settings::integration_needs_install);
    let show_primary = (settings.section != ClientSettingsSection::Integrations || installable)
        && !(settings.section == ClientSettingsSection::Usuario
            && settings.selected == 0
            && settings.nome_somente_leitura);
    let labels = if show_primary { vec![12, 12] } else { vec![12] };
    let buttons = row(inner, &labels, 2, inner.height.saturating_sub(1));
    let (primary, close) = if show_primary {
        let primary = buttons[0];
        button(
            buffer,
            primary,
            if settings.section == ClientSettingsSection::Integrations {
                " ↵ instalar "
            } else if settings.section == ClientSettingsSection::Usuario && settings.selected > 0 {
                " ↵ executar "
            } else {
                " ↵ Aplicar "
            },
            Style::default()
                .fg(contrast(palette))
                .bg(palette.accent)
                .add_modifier(Modifier::BOLD),
        );
        (primary, buttons[1])
    } else {
        (Rect::default(), buttons[0])
    };
    button(
        buffer,
        close,
        " esc Fechar ",
        Style::default()
            .fg(palette.text)
            .bg(palette.surface0)
            .add_modifier(Modifier::BOLD),
    );
    put_text(
        buffer,
        inner.x,
        inner.bottom().saturating_sub(2),
        inner.width,
        if settings.section == ClientSettingsSection::Usuario && settings.selected > 0 {
            " ↑↓ selecionar  ↵ executar  tab seção"
        } else if settings.section == ClientSettingsSection::Usuario
            && settings.nome_somente_leitura
        {
            " ↓ dependências  tab seção"
        } else if settings.section == ClientSettingsSection::Usuario {
            " digite o nome  ↓ dependências  ^c limpar  tab seção"
        } else {
            " ↑↓ selecionar  tab seção"
        },
        Style::default().fg(palette.overlay1).bg(palette.panel_bg),
    );

    Some(OverlayRender {
        area: popup,
        primary,
        cancel: close,
        settings_popup: popup,
        settings_tabs: tab_hits,
        settings_choices: choice_hits,
        cursor,
        ..OverlayRender::default()
    })
}

fn render_usuario_section(
    buffer: &mut Buffer,
    area: Rect,
    settings: &ClientSettingsOverlay,
    palette: &Palette,
    hits: &mut Vec<(Rect, usize)>,
) -> Option<crate::protocol::CursorState> {
    if settings.dependencias {
        render_dependencias(buffer, area, settings.selected, palette, hits);
        return None;
    }
    let style = Style::default().fg(palette.text).bg(palette.panel_bg);
    put_text(
        buffer,
        area.x,
        area.y,
        5,
        "Nome",
        style.add_modifier(Modifier::BOLD),
    );
    let input = Rect::new(area.x + 6, area.y, area.width.saturating_sub(17), 1);
    let mut cursor = if settings.nome_somente_leitura {
        put_text(
            buffer,
            input.x,
            input.y,
            input.width,
            settings.nome.as_str(),
            style,
        );
        None
    } else {
        hits.push((input, 0));
        let apply = Rect::new(area.right().saturating_sub(10), area.y, 10, 1);
        button(
            buffer,
            apply,
            "Aplicar",
            choice_style(settings.selected == 1, palette),
        );
        hits.push((apply, 1));
        text_editor::render(buffer, input, &settings.nome, style.bg(palette.surface0))
            .filter(|_| settings.selected == 0)
    };
    put_text(
        buffer,
        area.x,
        area.y + 2,
        area.width,
        "Sobre Você",
        style.add_modifier(Modifier::BOLD),
    );
    let mut about_hits = Vec::new();
    let about_cursor = render_sobre_section(
        buffer,
        Rect::new(
            area.x,
            area.y + 3,
            area.width,
            area.height.saturating_sub(3),
        ),
        settings,
        palette,
        &mut about_hits,
    );
    hits.extend(about_hits.into_iter().map(|(rect, _)| (rect, 2)));
    if settings.selected == 2 {
        cursor = about_cursor;
    }
    cursor
}

/// Lista das dependências da estação; posição 1 em diante da seleção da seção usuário.
fn render_dependencias(
    buffer: &mut Buffer,
    area: Rect,
    selected: usize,
    palette: &Palette,
    hits: &mut Vec<(Rect, usize)>,
) {
    let itens = super::super::pretorian_dependencias::itens();
    if itens.is_empty() || area.height < 2 {
        return;
    }
    put_text(
        buffer,
        area.x,
        area.y,
        area.width,
        "Dependências",
        Style::default()
            .fg(palette.text)
            .bg(palette.panel_bg)
            .add_modifier(Modifier::BOLD),
    );
    let mut y = area.y + 1;
    for (index, item) in itens.iter().enumerate() {
        if y >= area.bottom() {
            break;
        }
        let escolhido = selected == index + 1;
        let cor = match item.estado.as_str() {
            "ok" => palette.green,
            "em_andamento" => palette.yellow,
            "bloqueado" => palette.overlay0,
            _ => palette.red,
        };
        let fundo = if escolhido {
            palette.surface0
        } else {
            palette.panel_bg
        };
        let linha = Rect::new(area.x, y, area.width, 1);
        buffer.set_style(linha, Style::default().bg(fundo));
        put_text(
            buffer,
            area.x,
            y,
            4,
            &format!("{} {}", if escolhido { "›" } else { " " }, item.simbolo()),
            Style::default().fg(cor).bg(fundo),
        );
        let acao = if item.aceita_pedido() {
            item.acao.as_str()
        } else {
            ""
        };
        let largura_acao = display_width(acao);
        put_text(
            buffer,
            area.x + 4,
            y,
            area.width.saturating_sub(5 + largura_acao),
            &item.titulo,
            Style::default().fg(palette.text).bg(fundo),
        );
        put_text(
            buffer,
            area.right().saturating_sub(largura_acao + 1),
            y,
            largura_acao,
            acao,
            Style::default().fg(palette.accent).bg(fundo),
        );
        hits.push((linha, index + 1));
        y += 1;
        if escolhido {
            for texto in quebrar(&item.detalhe, area.width.saturating_sub(6)) {
                if y >= area.bottom() {
                    break;
                }
                put_text(
                    buffer,
                    area.x + 4,
                    y,
                    area.width.saturating_sub(4),
                    &texto,
                    Style::default().fg(palette.overlay1).bg(palette.panel_bg),
                );
                y += 1;
            }
        }
    }
}

/// Quebra por palavras para caber em `largura` colunas.
fn quebrar(texto: &str, largura: u16) -> Vec<String> {
    let mut linhas = Vec::new();
    let mut atual = String::new();
    for palavra in texto.split_whitespace() {
        if !atual.is_empty() && display_width(&atual) + 1 + display_width(palavra) > largura {
            linhas.push(std::mem::take(&mut atual));
        }
        if !atual.is_empty() {
            atual.push(' ');
        }
        atual.push_str(palavra);
    }
    if !atual.is_empty() {
        linhas.push(atual);
    }
    linhas
}

fn render_integrations(
    buffer: &mut Buffer,
    area: Rect,
    settings: &ClientSettingsOverlay,
    palette: &Palette,
) {
    put_text(
        buffer,
        area.x,
        area.y,
        area.width,
        "integrações de agentes",
        Style::default()
            .fg(palette.text)
            .bg(palette.panel_bg)
            .add_modifier(Modifier::BOLD),
    );
    put_text(
        buffer,
        area.x,
        area.y + 1,
        area.width,
        "restaura sessões e, quando possível, atualiza o estado",
        Style::default().fg(palette.overlay1).bg(palette.panel_bg),
    );
    if settings.loading_integrations {
        put_text(
            buffer,
            area.x,
            area.y + 3,
            area.width,
            " carregando integrações…",
            Style::default().fg(palette.overlay1).bg(palette.panel_bg),
        );
        return;
    }
    if settings.integrations.is_empty() {
        put_text(
            buffer,
            area.x,
            area.y + 3,
            area.width,
            " nenhuma integração disponível",
            Style::default().fg(palette.overlay1).bg(palette.panel_bg),
        );
        return;
    }
    for (index, integration) in settings.integrations.iter().enumerate() {
        let y = area.y + 3 + index as u16;
        if y >= area.bottom() {
            break;
        }
        let (marker, color, status) = match integration.state {
            crate::api::schema::IntegrationState::Current => ("✓", palette.green, "instalado"),
            crate::api::schema::IntegrationState::Outdated => {
                ("↻", palette.yellow, "atualização disponível")
            }
            crate::api::schema::IntegrationState::NotInstalled if integration.available => {
                ("+", palette.accent, "disponível")
            }
            crate::api::schema::IntegrationState::NotInstalled => {
                ("–", palette.overlay0, "não encontrado")
            }
        };
        put_text(
            buffer,
            area.x,
            y,
            3,
            &format!(" {marker}"),
            Style::default().fg(color).bg(palette.panel_bg),
        );
        put_text(
            buffer,
            area.x + 3,
            y,
            11.min(area.width.saturating_sub(3)),
            &format!("{:<9}", integration.label),
            Style::default().fg(palette.subtext0).bg(palette.panel_bg),
        );
        put_text(
            buffer,
            area.x + 14,
            y,
            area.width.saturating_sub(14),
            status,
            Style::default().fg(palette.overlay1).bg(palette.panel_bg),
        );
    }
    let message_y = area
        .y
        .saturating_add(4)
        .saturating_add(settings.integrations.len() as u16);
    for (offset, message) in settings.integration_messages.iter().take(6).enumerate() {
        let y = message_y.saturating_add(offset as u16);
        if y >= area.bottom() {
            break;
        }
        put_text(
            buffer,
            area.x,
            y,
            area.width,
            &format!(" {message}"),
            Style::default().fg(palette.overlay1).bg(palette.panel_bg),
        );
    }
    if settings.installing_integrations && message_y < area.bottom() {
        put_text(
            buffer,
            area.x,
            message_y,
            area.width,
            " instalando…",
            Style::default().fg(palette.overlay1).bg(palette.panel_bg),
        );
    }
}

fn render_sobre_section(
    buffer: &mut Buffer,
    area: Rect,
    settings: &ClientSettingsOverlay,
    palette: &Palette,
    hits: &mut Vec<(Rect, usize)>,
) -> Option<crate::protocol::CursorState> {
    let detail = Style::default().fg(palette.subtext0).bg(palette.panel_bg);
    if area.height < 3 {
        return None;
    }
    let input = Rect::new(area.x, area.y, area.width, area.height - 2);
    hits.push((input, 0));
    let cursor = text_editor::render_multiline(
        buffer,
        input,
        &settings.sobre.editor,
        Style::default().fg(palette.text).bg(palette.surface0),
    );
    if settings.sobre.editor.is_empty() {
        put_text(
            buffer,
            input.x,
            input.y,
            input.width,
            "Profissão, gostos e como prefere conversar.",
            Style::default().fg(palette.overlay1).bg(palette.surface0),
        );
    }
    put_text(
        buffer,
        area.x,
        area.bottom() - 2,
        area.width,
        settings.sobre.feedback(),
        detail,
    );
    put_text(
        buffer,
        area.x,
        area.bottom() - 1,
        area.width,
        &format!(
            "{} / 4.000 caracteres",
            settings.sobre.editor.chars().count()
        ),
        detail,
    );
    cursor
}
