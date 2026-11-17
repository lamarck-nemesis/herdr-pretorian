//! Pretorian: pasta secundária na segunda metade da conversa de cada interface.
//!
//! A divisão é por conexão (`ClientShellLocation::secundaria`). O servidor desenha a aba
//! principal na primeira metade e a aba ativa da secundária na segunda, num quadro só, e ajusta
//! os painéis de cada pasta ao tamanho da metade dela.

use super::*;
use crate::server::clients::ClientShellSecundaria;

/// Área de uma metade a partir da origem, que é como o desenho virtual e o ajuste de painéis
/// medem a aba.
pub(super) fn na_origem(area: Rect) -> Rect {
    Rect::new(0, 0, area.width, area.height)
}

impl HeadlessServer {
    /// Áreas da conversa desta interface: a principal e, nos arranjos divididos, a secundária
    /// com a aba que ela mostra (sem pasta, só a área reservada).
    pub(super) fn areas_shell(
        &self,
        client_id: u64,
    ) -> (Rect, Option<(Option<crate::ui::TabSurfaceTarget>, Rect)>) {
        let Some(client) = self.clients.get(&client_id) else {
            return (Rect::default(), None);
        };
        let (cols, rows) = client.terminal_size;
        let area = Rect::new(0, 0, cols, rows);
        let Some(location) = client.shell_location.as_ref() else {
            return (area, None);
        };
        let Some(secundaria) = location.secundaria.as_ref() else {
            return (area, None);
        };
        let (principal, outra, _) = crate::ui::dividir_superficie(area, secundaria.lado_a_lado);
        if outra.is_empty() {
            return (area, None);
        }
        let alvo = secundaria
            .workspace_id
            .as_ref()
            .filter(|workspace_id| location.focused_workspace_id.as_ref() != Some(*workspace_id))
            .and_then(|_| location.secundaria_tab_id())
            .and_then(|tab_id| self.app.parse_tab_id(tab_id))
            .map(|(workspace_index, tab_index)| crate::ui::TabSurfaceTarget {
                workspace_index,
                tab_index,
            });
        (principal, Some((alvo, outra)))
    }

    pub(super) fn set_client_shell_secondary(
        &mut self,
        client_id: u64,
        params: &api::schema::ClientShellSecondarySetParams,
    ) {
        // A aba escolhida só vale se for da pasta secundária.
        let aba = params
            .tab_id
            .as_ref()
            .zip(params.workspace_id.as_ref())
            .filter(|(tab_id, workspace_id)| {
                self.app.parse_tab_id(tab_id).is_some_and(|(indice, _)| {
                    self.app.public_workspace_id(indice) == **workspace_id
                })
            })
            .map(|(tab_id, workspace_id)| (workspace_id.clone(), tab_id.clone()));
        let Some(client) = self.clients.get_mut(&client_id) else {
            return;
        };
        if !client.is_shell_client() {
            return;
        }
        let location = client.shell_location.get_or_insert_with(Default::default);
        let workspace_id = params
            .workspace_id
            .clone()
            .filter(|workspace_id| location.focused_workspace_id.as_ref() != Some(workspace_id));
        location.secundaria = params
            .direction
            .as_ref()
            .map(|direcao| ClientShellSecundaria {
                lado_a_lado: matches!(direcao, api::schema::SplitDirection::Right),
                foco: params.focus && workspace_id.is_some(),
                workspace_id,
            });
        if let Some((workspace_id, tab_id)) = aba {
            location.active_tab_ids.insert(workspace_id, tab_id);
        }
        client.request_repaint();
        self.apply_shell_tab_geometry(client_id, false);
        for client in self.clients.values_mut() {
            client.request_recompute();
        }
    }

    /// O painel está na aba que a secundária desta interface mostra.
    pub(super) fn shell_secundaria_views_pane(
        &self,
        client_id: u64,
        workspace_index: usize,
        pane_id: crate::layout::PaneId,
    ) -> bool {
        let (_, Some((Some(alvo), _))) = self.areas_shell(client_id) else {
            return false;
        };
        alvo.workspace_index == workspace_index
            && self
                .app
                .state
                .workspaces
                .get(workspace_index)
                .and_then(|workspace| workspace.tabs.get(alvo.tab_index))
                .is_some_and(|tab| tab.layout.pane_ids().contains(&pane_id))
    }
}

fn deslocar(rect: protocol::SurfaceRect, dx: u16, dy: u16) -> protocol::SurfaceRect {
    protocol::SurfaceRect {
        x: rect.x + dx,
        y: rect.y + dy,
        ..rect
    }
}

/// Junta a metade principal (já desenhada a partir da origem) e a secundária num quadro do
/// tamanho da conversa. Links e painéis da secundária são deslocados; o cursor vem da metade
/// que tem o teclado. Imagens e janela flutuante continuam só na principal.
pub(super) fn compor_secundaria(
    principal: crate::server::client_shell::RenderedPaneSurface,
    secundaria: Option<crate::server::client_shell::RenderedPaneSurface>,
    tamanho: (u16, u16),
    origem: (u16, u16),
    foco_secundaria: bool,
) -> crate::server::client_shell::RenderedPaneSurface {
    let (cols, rows) = tamanho;
    let (dx, dy) = origem;
    let mut quadro = FrameData::from_ratatui_buffer_with_hyperlinks(
        &ratatui::buffer::Buffer::empty(Rect::new(0, 0, cols, rows)),
        None,
        &[],
    );
    let mut copiar = |origem_quadro: &FrameData, (ox, oy): (u16, u16), base_link: u32| {
        for y in 0..origem_quadro.height {
            for x in 0..origem_quadro.width {
                let (tx, ty) = (x + ox, y + oy);
                if tx >= cols || ty >= rows {
                    continue;
                }
                let mut celula = origem_quadro.cells
                    [usize::from(y) * usize::from(origem_quadro.width) + usize::from(x)]
                .clone();
                celula.hyperlink = celula.hyperlink.map(|indice| indice + base_link);
                quadro.cells[usize::from(ty) * usize::from(cols) + usize::from(tx)] = celula;
            }
        }
    };
    copiar(&principal.frame, (0, 0), 0);
    let base_link = principal.frame.hyperlinks.len() as u32;
    if let Some(secundaria) = &secundaria {
        copiar(&secundaria.frame, (dx, dy), base_link);
    }
    quadro.hyperlinks = principal.frame.hyperlinks.clone();
    quadro.graphics = principal.frame.graphics.clone();
    quadro.cursor = principal.frame.cursor.clone();
    let mut panes = principal.panes;
    let mut splits = principal.splits;
    if let Some(secundaria) = secundaria {
        quadro.hyperlinks.extend(secundaria.frame.hyperlinks);
        if foco_secundaria {
            quadro.cursor = secundaria.frame.cursor.map(|cursor| protocol::CursorState {
                x: cursor.x + dx,
                y: cursor.y + dy,
                ..cursor
            });
        }
        for pane in &mut panes {
            pane.focused &= !foco_secundaria;
        }
        panes.extend(
            secundaria
                .panes
                .into_iter()
                .map(|pane| protocol::PaneSurfacePane {
                    rect: deslocar(pane.rect, dx, dy),
                    inner_rect: deslocar(pane.inner_rect, dx, dy),
                    scrollbar_rect: pane.scrollbar_rect.map(|rect| deslocar(rect, dx, dy)),
                    focused: pane.focused && foco_secundaria,
                    ..pane
                }),
        );
        splits.extend(secundaria.splits.into_iter().map(|split| {
            let pos = match split.direction {
                protocol::PaneSurfaceSplitDirection::Horizontal => split.pos + dx,
                protocol::PaneSurfaceSplitDirection::Vertical => split.pos + dy,
            };
            protocol::PaneSurfaceSplit {
                pos,
                area: deslocar(split.area, dx, dy),
                hit_rect: deslocar(split.hit_rect, dx, dy),
                ..split
            }
        }));
    }
    crate::server::client_shell::RenderedPaneSurface {
        frame: quadro,
        panes,
        splits,
        ..principal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::client_shell::RenderedPaneSurface;
    use crate::server::clients::{ClientShellLocation, ClientShellTopology};

    #[test]
    fn divide_lado_a_lado_com_coluna_e_em_cima_com_barra_da_secundaria() {
        let area = Rect::new(0, 0, 81, 30);
        let (a, b, barra) = crate::ui::dividir_superficie(area, true);
        assert_eq!(
            (a, b, barra),
            (Rect::new(0, 0, 40, 30), Rect::new(41, 0, 40, 30), None)
        );
        let (a, b, barra) = crate::ui::dividir_superficie(area, false);
        assert_eq!(a, Rect::new(0, 0, 81, 15));
        assert_eq!(barra, Some(Rect::new(0, 15, 81, 1)));
        assert_eq!(b, Rect::new(0, 16, 81, 14));
        // Pequena demais: não divide.
        assert_eq!(
            crate::ui::dividir_superficie(Rect::new(0, 0, 20, 30), true).1,
            Rect::default()
        );
    }

    #[test]
    fn secundaria_que_some_ou_vira_principal_esvazia_a_metade() {
        let topologia = |pastas: &[&str], foco: &str| ClientShellTopology {
            focused_workspace_id: Some(foco.into()),
            fallback_workspace_id: None,
            active_tab_ids: pastas
                .iter()
                .map(|p| (p.to_string(), format!("{p}:t")))
                .collect(),
            tab_workspace_ids: pastas
                .iter()
                .map(|p| (format!("{p}:t"), p.to_string()))
                .collect(),
        };
        let mut local = ClientShellLocation {
            focused_workspace_id: Some("a".into()),
            secundaria: Some(ClientShellSecundaria {
                lado_a_lado: true,
                workspace_id: Some("b".into()),
                foco: true,
            }),
            ..Default::default()
        };
        local.reconcile(&topologia(&["a", "b"], "a"));
        assert_eq!(local.secundaria_tab_id(), Some("b:t"));
        local.reconcile(&topologia(&["a"], "a"));
        let secundaria = local.secundaria.clone().unwrap();
        assert_eq!((secundaria.workspace_id, secundaria.foco), (None, false));
        // A divisão continua pedida: a metade fica reservada para o convite.
        assert!(secundaria.lado_a_lado);
    }

    fn superficie(
        largura: u16,
        altura: u16,
        link: &str,
        cursor: (u16, u16),
    ) -> RenderedPaneSurface {
        let mut quadro = FrameData::from_ratatui_buffer_with_hyperlinks(
            &ratatui::buffer::Buffer::empty(Rect::new(0, 0, largura, altura)),
            Some(protocol::CursorState {
                x: cursor.0,
                y: cursor.1,
                visible: true,
                shape: 0,
            }),
            &[],
        );
        quadro.cells[0].symbol = "X".into();
        quadro.cells[0].hyperlink = Some(0);
        quadro.hyperlinks = vec![link.into()];
        let rect = protocol::SurfaceRect {
            x: 0,
            y: 0,
            width: largura,
            height: altura,
        };
        RenderedPaneSurface {
            frame: quadro,
            panes: vec![protocol::PaneSurfacePane {
                pane_id: link.into(),
                content_revision: 0,
                rect,
                inner_rect: rect,
                scrollbar_rect: None,
                scroll: None,
                focused: true,
                mouse_reporting: false,
                sgr_pixel_mouse: false,
                alternate_screen_active: false,
                pixel_width: 0,
                pixel_height: 0,
            }],
            splits: Vec::new(),
            popup: None,
            graphics: Default::default(),
            graphics_delivery: Default::default(),
            graphics_sources: Default::default(),
        }
    }

    #[test]
    fn compoe_as_metades_deslocando_painel_link_e_cursor_da_secundaria() {
        let quadro = compor_secundaria(
            superficie(10, 4, "a", (1, 1)),
            Some(superficie(9, 4, "b", (2, 3))),
            (20, 4),
            (11, 0),
            true,
        );
        assert_eq!((quadro.frame.width, quadro.frame.height), (20, 4));
        assert_eq!(quadro.frame.cells[0].hyperlink, Some(0));
        assert_eq!(quadro.frame.cells[11].symbol, "X");
        assert_eq!(quadro.frame.cells[11].hyperlink, Some(1));
        assert_eq!(quadro.frame.hyperlinks, ["a", "b"]);
        assert_eq!(quadro.frame.cursor.map(|c| (c.x, c.y)), Some((13, 3)));
        assert_eq!(
            quadro
                .panes
                .iter()
                .map(|p| (p.pane_id.as_str(), p.rect.x, p.focused))
                .collect::<Vec<_>>(),
            [("a", 0, false), ("b", 11, true)]
        );
        // Sem pasta na secundária: só a principal, com o cursor dela.
        let vazio = compor_secundaria(
            superficie(10, 4, "a", (1, 1)),
            None,
            (20, 4),
            (11, 0),
            false,
        );
        assert_eq!(vazio.panes.len(), 1);
        assert_eq!(vazio.frame.cursor.map(|c| (c.x, c.y)), Some((1, 1)));
    }
}
