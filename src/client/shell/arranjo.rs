//! Arranjos da shell do Pretorian, escolhidos em "menu > mudar layout".
//!
//! Regiões (calculadas em `ClientShellConfig::layout`):
//!
//! ```text
//! ┃ esquerda       ┃ abas            ┃ apoio ┃
//! ┃ pastas         ┃ conversa        ┃       ┃
//! ┃ nova pasta menu┃                 ┃       ┃
//! ┃ ⌘ nεαd.tεch    ┃                 ┃       ┃
//! ```
//!
//! A barra de apoio é genérica: hoje mostra os lembretes. Os painéis da divisão nunca são
//! fechados pelo botão, porque podem ter uma sessão em andamento: para esconder a divisão o
//! painel em foco é ampliado, e para trocar a direção o segundo painel é movido.
//!
//! Janela secundária (arranjos 3 e 4), no padrão `join-pane`/`break-pane` do tmux: puxar uma
//! pasta leva a sessão viva dela para a segunda metade; puxar outra devolve a anterior à
//! pasta de origem, ou a uma pasta nova com o mesmo nome se a origem já não existir.

use super::*;
use crate::api::schema::SplitDirection;
use crossterm::event::{MouseButton, MouseEventKind};

pub(super) const LARGURA_APOIO: u16 = 46;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum Arranjo {
    #[default]
    Conversa,
    ComApoio,
    LadoALado,
    EmCimaEmbaixo,
}

impl Arranjo {
    pub(super) const TODOS: [Self; 4] = [
        Self::Conversa,
        Self::ComApoio,
        Self::LadoALado,
        Self::EmCimaEmbaixo,
    ];

    /// Rótulo no menu; o arranjo em uso ganha o marcador.
    pub(super) fn rotulo(self, atual: bool) -> &'static str {
        match (self, atual) {
            (Self::Conversa, false) => "  1  barra à esquerda",
            (Self::Conversa, true) => "● 1  barra à esquerda",
            (Self::ComApoio, false) => "  2  barras à esquerda e à direita",
            (Self::ComApoio, true) => "● 2  barras à esquerda e à direita",
            (Self::LadoALado, false) => "  3  barras e divisão vertical central",
            (Self::LadoALado, true) => "● 3  barras e divisão vertical central",
            (Self::EmCimaEmbaixo, false) => "  4  barras e divisão horizontal central",
            (Self::EmCimaEmbaixo, true) => "● 4  barras e divisão horizontal central",
        }
    }

    pub(super) fn tem_apoio(self) -> bool {
        self != Self::Conversa
    }

    fn divisao(self) -> Option<SplitDirection> {
        match self {
            Self::LadoALado => Some(SplitDirection::Right),
            Self::EmCimaEmbaixo => Some(SplitDirection::Down),
            Self::Conversa | Self::ComApoio => None,
        }
    }
}

/// Conteúdos da barra de apoio, um por aba da navegação dela. Novo conteúdo entra aqui.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConteudoApoio {
    #[default]
    Lembretes,
}

impl ConteudoApoio {
    pub(super) const TODOS: [Self; 1] = [Self::Lembretes];

    pub(super) fn rotulo(self) -> &'static str {
        match self {
            Self::Lembretes => "Lembretes",
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct EstadoArranjo {
    pub(super) atual: Arranjo,
    pub(super) conteudo: ConteudoApoio,
    /// Primeira aba visível da navegação quando as abas não cabem.
    pub(super) navegacao_inicio: usize,
    /// Navegação da barra de apoio.
    pub(super) apoio: super::lembretes::Painel,
    /// Direção da divisão criada ou movida pelo botão; `None` antes da primeira.
    orientacao: Option<SplitDirection>,
    /// A largura da conversa mudou fora de um evento de entrada; o servidor precisa saber.
    redimensionar: bool,
    /// Painel puxado para a metade secundária e a pasta de onde veio.
    secundaria: Option<(String, Origem)>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Origem {
    workspace_id: String,
    rotulo: String,
}

impl EstadoArranjo {
    /// `None` fora dos arranjos divididos; `Some(None)` com a janela secundária vazia.
    pub(super) fn janela_secundaria(&self) -> Option<Option<&str>> {
        self.atual.divisao().map(|_| {
            self.secundaria
                .as_ref()
                .map(|(_, origem)| origem.rotulo.as_str())
        })
    }

    pub(super) fn largura_apoio(&self) -> u16 {
        if self.atual.tem_apoio() {
            LARGURA_APOIO
        } else {
            0
        }
    }
}

/// Movimentos para puxar o painel `fonte` para a metade secundária da aba.
fn plano_puxar(
    aba: &str,
    paineis: &[&str],
    direcao: SplitDirection,
    fonte: &str,
    secundaria: Option<&(String, Origem)>,
    pastas: &[&str],
) -> Vec<crate::api::schema::PaneMoveParams> {
    use crate::api::schema::{PaneMoveDestination as Destino, PaneMoveParams};
    let Some(principal) = paineis.first() else {
        return Vec::new();
    };
    // Já está aqui, ou a divisão tem mais de dois painéis e é da pessoa.
    if paineis.contains(&fonte) || paineis.len() > 2 {
        return Vec::new();
    }
    let mut passos = Vec::new();
    if let Some(atual) = paineis.get(1) {
        let destino = match secundaria.filter(|(painel, _)| painel == atual) {
            Some((_, origem)) if pastas.contains(&origem.workspace_id.as_str()) => {
                Destino::NewTab {
                    workspace_id: Some(origem.workspace_id.clone()),
                    label: None,
                }
            }
            Some((_, origem)) => Destino::NewWorkspace {
                label: Some(origem.rotulo.clone()),
                tab_label: None,
            },
            None => Destino::NewWorkspace {
                label: None,
                tab_label: None,
            },
        };
        passos.push(PaneMoveParams {
            pane_id: (*atual).to_owned(),
            destination: destino,
            focus: false,
        });
    }
    passos.push(PaneMoveParams {
        pane_id: fonte.to_owned(),
        destination: Destino::Tab {
            tab_id: aba.to_owned(),
            target_pane_id: Some((*principal).to_owned()),
            split: direcao,
            ratio: None,
        },
        focus: false,
    });
    passos
}

#[derive(Debug, PartialEq)]
enum PassoDivisao {
    Dividir(SplitDirection),
    /// Leva o segundo painel para o lado ou para baixo do primeiro.
    Mover {
        painel: String,
        alvo: String,
        direcao: SplitDirection,
    },
    AlternarAmpliacao,
}

/// O que fazer na aba para chegar à divisão pedida (`None` esconde a divisão).
fn plano_divisao(
    pedida: Option<SplitDirection>,
    paineis: &[&str],
    ampliada: bool,
    orientacao: Option<SplitDirection>,
) -> Vec<PassoDivisao> {
    let visivel = paineis.len() > 1 && !ampliada;
    let Some(direcao) = pedida else {
        return if visivel {
            vec![PassoDivisao::AlternarAmpliacao]
        } else {
            Vec::new()
        };
    };
    if paineis.len() <= 1 {
        return vec![PassoDivisao::Dividir(direcao)];
    }
    let mut passos = Vec::new();
    if ampliada {
        passos.push(PassoDivisao::AlternarAmpliacao);
    }
    // Com mais de dois painéis a divisão é da pessoa: o botão não reorganiza.
    if let [primeiro, segundo] = paineis {
        if orientacao.is_some_and(|atual| atual != direcao) {
            passos.push(PassoDivisao::Mover {
                painel: (*segundo).to_owned(),
                alvo: (*primeiro).to_owned(),
                direcao,
            });
        }
    }
    passos
}

impl ClientShellState {
    fn ajustar_divisao(&mut self, outcome: &mut ClientShellInput) {
        let Some(snapshot) = self.snapshot.as_deref() else {
            return;
        };
        let Some(aba) = snapshot.tabs.iter().find(|tab| {
            tab.focused
                && Some(tab.workspace_id.as_str()) == snapshot.focused_workspace_id.as_deref()
        }) else {
            return;
        };
        let paineis: Vec<&str> = snapshot
            .panes
            .iter()
            .filter(|pane| pane.tab_id == aba.tab_id)
            .map(|pane| pane.pane_id.as_str())
            .collect();
        let pedida = self.arranjo.atual.divisao();
        // Sessão restaurada já dividida: a direção sai da posição dos dois painéis na tela.
        let orientacao = self.arranjo.orientacao.clone().or_else(|| {
            let [a, b] = paineis[..] else {
                return None;
            };
            let topo = |id: &str| {
                self.hits
                    .panes
                    .iter()
                    .find(|hit| hit.pane_id == id)
                    .map(|hit| hit.rect.y)
            };
            Some(match (topo(a)?, topo(b)?) {
                (ya, yb) if ya == yb => SplitDirection::Right,
                _ => SplitDirection::Down,
            })
        });
        let passos = plano_divisao(pedida.clone(), &paineis, aba.zoomed, orientacao);
        let aba_id = aba.tab_id.clone();
        let aba_workspace = aba.workspace_id.clone();
        for passo in passos {
            match passo {
                PassoDivisao::Dividir(direcao) => {
                    let acao = if direcao == SplitDirection::Right {
                        crate::input::KeybindAction::SplitVertical
                    } else {
                        crate::input::KeybindAction::SplitHorizontal
                    };
                    self.record_binding(crate::input::KeybindMatch::Action(acao), outcome);
                }
                PassoDivisao::AlternarAmpliacao => self.record_binding(
                    crate::input::KeybindMatch::Action(crate::input::KeybindAction::Zoom),
                    outcome,
                ),
                // O servidor não move painel dentro da mesma aba: o segundo painel passa por
                // uma aba temporária, que some quando ele volta.
                PassoDivisao::Mover {
                    painel,
                    alvo,
                    direcao,
                } => {
                    use crate::api::schema::{
                        Method, PaneMoveDestination as Destino, PaneMoveParams,
                    };
                    let mover = |destination| {
                        Method::PaneMove(PaneMoveParams {
                            pane_id: painel.clone(),
                            destination,
                            focus: false,
                        })
                    };
                    self.push_endpoint_method(
                        mover(Destino::NewTab {
                            workspace_id: Some(aba_workspace.clone()),
                            label: None,
                        }),
                        outcome,
                    );
                    self.push_endpoint_method(
                        mover(Destino::Tab {
                            tab_id: aba_id.clone(),
                            target_pane_id: Some(alvo),
                            split: direcao,
                            ratio: None,
                        }),
                        outcome,
                    );
                }
            }
        }
        if pedida.is_some() {
            self.arranjo.orientacao = pedida;
        }
    }

    /// Leva a sessão ativa da pasta para a metade secundária da aba em foco.
    pub(super) fn puxar_pasta(&mut self, workspace_id: &str, outcome: &mut ClientShellInput) {
        if self.arranjo.atual.divisao().is_none() {
            let antes = self.arranjo.largura_apoio();
            self.arranjo.atual = Arranjo::LadoALado;
            super::lembretes::recarregar();
            outcome.resize |= self.aplicar_largura(antes);
        }
        let direcao = self
            .arranjo
            .atual
            .divisao()
            .unwrap_or(SplitDirection::Right);
        let Some(snapshot) = self.snapshot.as_deref() else {
            return;
        };
        let Some(aba) = snapshot.tabs.iter().find(|tab| {
            tab.focused
                && Some(tab.workspace_id.as_str()) == snapshot.focused_workspace_id.as_deref()
        }) else {
            return;
        };
        if aba.workspace_id == workspace_id {
            self.set_endpoint_error("Esta pasta já está na janela principal.");
            outcome.repaint = true;
            return;
        }
        let Some(pasta) = snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_id == workspace_id)
        else {
            return;
        };
        let da_pasta: Vec<_> = snapshot
            .panes
            .iter()
            .filter(|pane| pane.tab_id == pasta.active_tab_id)
            .collect();
        let Some(fonte) = da_pasta
            .iter()
            .find(|pane| pane.focused)
            .or(da_pasta.first())
            .map(|pane| pane.pane_id.clone())
        else {
            return;
        };
        let paineis: Vec<&str> = snapshot
            .panes
            .iter()
            .filter(|pane| pane.tab_id == aba.tab_id)
            .map(|pane| pane.pane_id.as_str())
            .collect();
        let pastas: Vec<&str> = snapshot
            .workspaces
            .iter()
            .map(|workspace| workspace.workspace_id.as_str())
            .collect();
        let passos = plano_puxar(
            &aba.tab_id,
            &paineis,
            direcao.clone(),
            &fonte,
            self.arranjo.secundaria.as_ref(),
            &pastas,
        );
        let origem = Origem {
            workspace_id: pasta.workspace_id.clone(),
            rotulo: pasta.label.clone(),
        };
        // Painel não entra nem sai de aba ampliada: o servidor recusa.
        if aba.zoomed && !passos.is_empty() {
            self.record_binding(
                crate::input::KeybindMatch::Action(crate::input::KeybindAction::Zoom),
                outcome,
            );
        }
        if !passos.is_empty() {
            self.arranjo.secundaria = Some((fonte, origem));
            self.arranjo.orientacao = Some(direcao);
        }
        for passo in passos {
            self.push_endpoint_method(crate::api::schema::Method::PaneMove(passo), outcome);
        }
        outcome.repaint = true;
    }

    fn aplicar_largura(&mut self, antes: u16) -> bool {
        let mudou = self.arranjo.largura_apoio() != antes;
        if mudou {
            self.invalidate_pane_surface();
        }
        mudou
    }

    pub(super) fn escolher_arranjo(&mut self, alvo: Arranjo, outcome: &mut ClientShellInput) {
        let antes = self.arranjo.largura_apoio();
        self.arranjo.atual = alvo;
        self.ajustar_divisao(outcome);
        if self.arranjo.atual.tem_apoio() {
            super::lembretes::recarregar();
        }
        outcome.resize |= self.aplicar_largura(antes);
        outcome.repaint = true;
    }

    /// "ver lembretes" e a abertura automática levam ao arranjo com a barra de apoio.
    pub(super) fn mostrar_lembretes(&mut self) {
        let antes = self.arranjo.largura_apoio();
        if !self.arranjo.atual.tem_apoio() {
            self.arranjo.atual = Arranjo::ComApoio;
        }
        super::lembretes::recarregar();
        self.arranjo.redimensionar |= self.aplicar_largura(antes);
    }

    /// Entrega ao laço do cliente o pedido de novo tamanho feito fora de um evento de entrada.
    pub(crate) fn tomar_redimensionamento(&mut self) -> bool {
        std::mem::take(&mut self.arranjo.redimensionar)
    }

    /// Barra de apoio. Devolve verdadeiro quando o evento foi dela.
    pub(super) fn tratar_mouse_arranjo(
        &mut self,
        mouse: crossterm::event::MouseEvent,
        outcome: &mut ClientShellInput,
    ) -> bool {
        let point = (mouse.column, mouse.row);
        let no_apoio = super::contains(self.hits.apoio_area, point);
        let lembretes = match super::lembretes::lista() {
            Some(Ok(lista)) => lista.lembretes,
            _ => Vec::new(),
        };
        let pagina = self.hits.apoio_por_pagina.max(1) as isize;
        let apoio = &mut self.arranjo.apoio;
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) if no_apoio => {
                let linha = self
                    .hits
                    .apoio_rows
                    .iter()
                    .find(|(area, _)| super::contains(*area, point))
                    .map(|(_, n)| *n);
                let aba = self
                    .hits
                    .apoio_abas
                    .iter()
                    .find(|(area, _)| super::contains(*area, point))
                    .map(|(_, n)| *n);
                let mut janela = super::lembretes::Painel::default();
                if let Some(n) = aba {
                    self.arranjo.conteudo = ConteudoApoio::TODOS[n];
                } else if super::contains(self.hits.apoio_rolar.0, point) {
                    self.arranjo.navegacao_inicio = self.arranjo.navegacao_inicio.saturating_sub(1);
                } else if super::contains(self.hits.apoio_rolar.1, point) {
                    self.arranjo.navegacao_inicio =
                        (self.arranjo.navegacao_inicio + 1).min(ConteudoApoio::TODOS.len() - 1);
                } else if super::contains(self.hits.apoio_adicionar, point) {
                    janela.abrir_formulario();
                } else if let Some(lembrete) = linha.and_then(|n| {
                    apoio.selecionado = n;
                    lembretes.get(n).cloned()
                }) {
                    janela.abrir(lembrete);
                } else if super::contains(self.hits.apoio_anterior, point) {
                    apoio.mover(-pagina, lembretes.len());
                } else if super::contains(self.hits.apoio_proxima, point) {
                    apoio.mover(pagina, lembretes.len());
                }
                if janela.aberto.is_some() || janela.novo.is_some() {
                    self.overlay = Some(ClientShellOverlay::Lembretes(janela));
                }
                outcome.repaint = true;
            }
            MouseEventKind::ScrollUp if no_apoio => {
                apoio.mover(-1, lembretes.len());
                outcome.repaint = true;
            }
            MouseEventKind::ScrollDown if no_apoio => {
                apoio.mover(1, lembretes.len());
                outcome.repaint = true;
            }
            _ => return no_apoio && mouse.kind != MouseEventKind::Moved,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use PassoDivisao as P;
    use SplitDirection::{Down, Right};

    #[test]
    fn menu_lista_os_quatro_arranjos_e_marca_o_atual() {
        let rotulos: Vec<_> = Arranjo::TODOS
            .iter()
            .map(|a| a.rotulo(*a == Arranjo::LadoALado))
            .collect();
        assert_eq!(
            rotulos,
            [
                "  1  barra à esquerda",
                "  2  barras à esquerda e à direita",
                "● 3  barras e divisão vertical central",
                "  4  barras e divisão horizontal central"
            ]
        );
        let mut estado = EstadoArranjo::default();
        assert_eq!(estado.largura_apoio(), 0);
        estado.atual = Arranjo::EmCimaEmbaixo;
        assert_eq!(estado.largura_apoio(), LARGURA_APOIO);
    }

    #[test]
    fn puxar_pasta_devolve_a_anterior_a_origem_e_traz_a_nova() {
        use crate::api::schema::PaneMoveDestination as D;
        let origem = Origem {
            workspace_id: "ws_b".into(),
            rotulo: "lsa-pje".into(),
        };
        let destinos = |passos: Vec<crate::api::schema::PaneMoveParams>| {
            passos
                .into_iter()
                .map(|p| (p.pane_id, p.destination))
                .collect::<Vec<_>>()
        };
        // Aba sem divisão: só entra, ao lado do painel principal.
        let passos = destinos(plano_puxar("t1", &["p1"], Right, "pc", None, &["ws_a"]));
        assert_eq!(
            passos,
            [(
                "pc".into(),
                D::Tab {
                    tab_id: "t1".into(),
                    target_pane_id: Some("p1".into()),
                    split: Right,
                    ratio: None
                }
            )]
        );
        // Secundária veio de uma pasta que ainda existe: volta para uma aba nela.
        let anterior = ("p2".to_owned(), origem.clone());
        let passos = destinos(plano_puxar(
            "t1",
            &["p1", "p2"],
            Right,
            "pc",
            Some(&anterior),
            &["ws_a", "ws_b"],
        ));
        assert_eq!(
            passos[0],
            (
                "p2".into(),
                D::NewTab {
                    workspace_id: Some("ws_b".into()),
                    label: None
                }
            )
        );
        assert_eq!(passos[1].0, "pc");
        // A pasta de origem sumiu (era o único painel): é recriada com o mesmo nome.
        let passos = destinos(plano_puxar(
            "t1",
            &["p1", "p2"],
            Down,
            "pc",
            Some(&anterior),
            &["ws_a"],
        ));
        assert_eq!(
            passos[0].1,
            D::NewWorkspace {
                label: Some("lsa-pje".into()),
                tab_label: None
            }
        );
        // Já está aqui, ou a divisão é da pessoa: nada muda.
        assert!(plano_puxar("t1", &["p1", "pc"], Right, "pc", None, &[]).is_empty());
        assert!(plano_puxar("t1", &["p1", "p2", "p3"], Right, "pc", None, &[]).is_empty());
    }

    #[test]
    fn divisao_cria_uma_vez_move_para_trocar_direcao_e_nunca_fecha() {
        let mover = |direcao| P::Mover {
            painel: "p2".into(),
            alvo: "p1".into(),
            direcao,
        };
        assert_eq!(
            plano_divisao(Some(Right), &["p1"], false, None),
            [P::Dividir(Right)]
        );
        assert_eq!(
            plano_divisao(Some(Down), &["p1"], false, None),
            [P::Dividir(Down)]
        );
        assert_eq!(
            plano_divisao(Some(Down), &["p1", "p2"], false, Some(Right)),
            [mover(Down)]
        );
        assert_eq!(
            plano_divisao(Some(Right), &["p1", "p2"], true, Some(Down)),
            [P::AlternarAmpliacao, mover(Right)]
        );
        assert_eq!(
            plano_divisao(Some(Right), &["p1", "p2"], false, Some(Right)),
            []
        );
        // Esconder a divisão amplia o painel em foco; nada é fechado.
        assert_eq!(
            plano_divisao(None, &["p1", "p2"], false, Some(Down)),
            [P::AlternarAmpliacao]
        );
        assert_eq!(plano_divisao(None, &["p1", "p2"], true, Some(Down)), []);
        // Divisão feita pela pessoa, com três painéis, não é reorganizada.
        assert_eq!(
            plano_divisao(Some(Down), &["p1", "p2", "p3"], false, Some(Right)),
            []
        );
    }
}
