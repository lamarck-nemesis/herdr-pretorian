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
    let height = if settings.section == ClientSettingsSection::Integrations {
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
        " configurações",
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
        ClientSettingsSection::Indicators => {
            render_choice_section(
                buffer,
                content,
                "estado dos agentes",
                "escolha cores ou símbolos para cada estado",
                &[
                    "pontos coloridos  ● ● ● ○ ·",
                    "símbolos distintos  ▲ ◐ ✓ ○ ·",
                ],
                settings.selected,
                palette,
                &mut choice_hits,
            );
        }
        ClientSettingsSection::Sound => {
            render_choice_section(
                buffer,
                content,
                "alertas sonoros",
                "tocar sons quando agentes mudarem de estado",
                &["ligado", "desligado"],
                settings.selected,
                palette,
                &mut choice_hits,
            );
        }
        ClientSettingsSection::Toast => {
            render_choice_section(
                buffer,
                content,
                "avisos de notificação",
                "escolha onde mostrar avisos em segundo plano",
                &["desligado", "no Pretorian", "no terminal", "no sistema"],
                settings.selected,
                palette,
                &mut choice_hits,
            );
        }
        ClientSettingsSection::Integrations => {
            render_integrations(buffer, content, settings, palette);
        }
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
                " ↵ aplicar "
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
        " esc fechar ",
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
    put_text(
        buffer,
        area.x,
        area.y,
        area.width,
        if settings.nome_somente_leitura {
            "nome da conta"
        } else {
            "seu nome"
        },
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
        if settings.nome_somente_leitura {
            "identificado pela sua conta"
        } else {
            "aparece na barra lateral, abaixo de Área de Trabalho"
        },
        Style::default().fg(palette.overlay1).bg(palette.panel_bg),
    );
    if area.height < 6 {
        return None;
    }
    let cursor = if settings.nome_somente_leitura {
        put_text(
            buffer,
            area.x,
            area.y + 3,
            area.width,
            settings.nome.as_str(),
            Style::default().fg(palette.text).bg(palette.panel_bg),
        );
        None
    } else {
        let input = Rect::new(area.x, area.y + 3, area.width, 1);
        let input_style = Style::default().fg(palette.text).bg(palette.surface0);
        buffer.set_style(input, input_style);
        let cursor = text_editor::render(
            buffer,
            Rect::new(input.x + 1, input.y, input.width.saturating_sub(1), 1),
            &settings.nome,
            input_style,
        );
        let previa = crate::config::sanitizar_nome(settings.nome.as_str()).unwrap_or_default();
        put_text(
            buffer,
            area.x,
            area.y + 5,
            area.width,
            &format!("prévia: {previa}"),
            Style::default().fg(palette.subtext0).bg(palette.panel_bg),
        );
        cursor
    };
    render_dependencias(
        buffer,
        Rect::new(
            area.x,
            area.y + 7,
            area.width,
            area.height.saturating_sub(7),
        ),
        settings.selected,
        palette,
        hits,
    );
    // Com uma dependência selecionada o campo do nome não recebe teclas: sem cursor nele.
    cursor.filter(|_| settings.selected == 0)
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
        "dependências",
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

fn render_choice_section(
    buffer: &mut Buffer,
    area: Rect,
    title: &str,
    description: &str,
    choices: &[&str],
    selected: usize,
    palette: &Palette,
    hits: &mut Vec<(Rect, usize)>,
) {
    put_text(
        buffer,
        area.x,
        area.y,
        area.width,
        title,
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
        description,
        Style::default().fg(palette.overlay1).bg(palette.panel_bg),
    );
    let row_gap = u16::from(choices.len() > 2);
    for (index, choice) in choices.iter().enumerate() {
        let y = area.y + 3 + index as u16 * (1 + row_gap);
        if y >= area.bottom() {
            break;
        }
        let rect = Rect::new(area.x, y, area.width, 1);
        draw_choice(buffer, rect, choice, index == selected, false, palette);
        hits.push((rect, index));
    }
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
