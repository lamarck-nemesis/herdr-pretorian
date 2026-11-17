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
//! A barra de apoio é genérica: hoje mostra os lembretes.
//!
//! Arranjos 3 e 4: a conversa se divide entre a pasta principal e uma pasta secundária, cada
//! uma com sua barra de abas. O servidor desenha as duas metades (`client_shell.secondary.set`);
//! puxar uma pasta só a mostra na segunda metade, sem mover painel nem fechar sessão. Fechar a
//! secundária volta à conversa única.

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
    /// A largura da conversa mudou fora de um evento de entrada; o servidor precisa saber.
    redimensionar: bool,
    /// Pasta mostrada na segunda metade; lembrada ao sair e voltar aos arranjos divididos.
    pub(super) pasta_secundaria: Option<String>,
    /// O teclado está na secundária, no painel clicado por último nela.
    pub(super) painel_secundario: Option<String>,
    /// "+" pedido na secundária: abas que ela tinha, para escolher a nova quando chegar.
    abas_antes_da_nova: Option<Vec<String>>,
}

impl EstadoArranjo {
    /// `Some(true)` lado a lado, `Some(false)` em cima e embaixo, `None` sem divisão.
    pub(super) fn lado_a_lado(&self) -> Option<bool> {
        self.atual
            .divisao()
            .map(|direcao| direcao == SplitDirection::Right)
    }

    /// Pasta da secundária, só nos arranjos divididos.
    pub(super) fn secundaria(&self) -> Option<&str> {
        self.atual.divisao().and(self.pasta_secundaria.as_deref())
    }

    pub(super) fn largura_apoio(&self) -> u16 {
        if self.atual.tem_apoio() {
            LARGURA_APOIO
        } else {
            0
        }
    }
}

impl ClientShellState {
    /// Avisa o servidor da divisão, da pasta secundária e de qual metade tem o teclado.
    fn enviar_secundaria(&mut self, aba: Option<String>, outcome: &mut ClientShellInput) {
        let params = crate::api::schema::ClientShellSecondarySetParams {
            direction: self.arranjo.atual.divisao(),
            workspace_id: self.arranjo.secundaria().map(str::to_owned),
            tab_id: aba,
            focus: self.arranjo.painel_secundario.is_some(),
        };
        self.push_endpoint_method(
            crate::api::schema::Method::ClientShellSecondarySet(params),
            outcome,
        );
        outcome.repaint = true;
    }

    /// Mostra a pasta na segunda metade, com todas as abas dela. A sessão continua na pasta.
    pub(super) fn puxar_pasta(&mut self, workspace_id: &str, outcome: &mut ClientShellInput) {
        let principal = self
            .snapshot
            .as_deref()
            .and_then(|snapshot| snapshot.focused_workspace_id.as_deref());
        if principal == Some(workspace_id) {
            self.set_endpoint_error("Esta pasta já está na janela principal.");
            outcome.repaint = true;
            return;
        }
        let antes = self.arranjo.largura_apoio();
        if self.arranjo.atual.divisao().is_none() {
            self.arranjo.atual = Arranjo::LadoALado;
            super::lembretes::recarregar();
        }
        self.arranjo.pasta_secundaria = Some(workspace_id.to_owned());
        self.arranjo.painel_secundario = None;
        outcome.resize |= self.aplicar_largura(antes);
        self.enviar_secundaria(None, outcome);
    }

    /// Fechar a secundária volta à conversa única; nada é fechado na pasta.
    pub(super) fn fechar_secundaria(&mut self, outcome: &mut ClientShellInput) {
        self.arranjo.pasta_secundaria = None;
        self.arranjo.painel_secundario = None;
        self.arranjo.atual = Arranjo::ComApoio;
        self.enviar_secundaria(None, outcome);
    }

    /// Trocar de aba na secundária devolve o teclado à principal: o painel clicado antes ficou
    /// na aba escondida.
    pub(super) fn escolher_aba_secundaria(
        &mut self,
        tab_id: String,
        outcome: &mut ClientShellInput,
    ) {
        self.arranjo.painel_secundario = None;
        self.enviar_secundaria(Some(tab_id), outcome);
    }

    /// "+" da secundária: a aba nasce na pasta dela sem tirar o foco da principal.
    pub(super) fn nova_aba_secundaria(&mut self, outcome: &mut ClientShellInput) {
        let (Some(pasta), Some(snapshot)) = (self.arranjo.secundaria(), self.snapshot.as_deref())
        else {
            return;
        };
        let pasta = pasta.to_owned();
        self.arranjo.abas_antes_da_nova = Some(
            snapshot
                .tabs
                .iter()
                .filter(|tab| tab.workspace_id == pasta)
                .map(|tab| tab.tab_id.clone())
                .collect(),
        );
        self.push_endpoint_method(
            crate::api::schema::Method::TabCreate(crate::api::schema::TabCreateParams {
                workspace_id: Some(pasta),
                cwd: None,
                focus: false,
                label: None,
                env: Default::default(),
            }),
            outcome,
        );
    }

    /// Clique num painel: o da secundária leva o teclado para ela sem trocar a pasta principal.
    /// Devolve verdadeiro quando o foco do servidor não deve ser pedido.
    pub(super) fn foco_por_clique(
        &mut self,
        pane_id: &str,
        outcome: &mut ClientShellInput,
    ) -> bool {
        let pasta = self.snapshot.as_deref().and_then(|snapshot| {
            snapshot
                .panes
                .iter()
                .find(|pane| pane.pane_id == pane_id)
                .map(|pane| pane.workspace_id.clone())
        });
        if pasta.is_some() && pasta.as_deref() == self.arranjo.secundaria() {
            if self.arranjo.painel_secundario.as_deref() != Some(pane_id) {
                self.arranjo.painel_secundario = Some(pane_id.to_owned());
                self.enviar_secundaria(None, outcome);
            }
            return true;
        }
        if self.arranjo.painel_secundario.take().is_some() {
            self.enviar_secundaria(None, outcome);
        }
        false
    }

    /// Painel que recebe o teclado quando ele está na secundária.
    pub(super) fn painel_do_teclado(&self) -> Option<String> {
        self.arranjo
            .secundaria()
            .and(self.arranjo.painel_secundario.clone())
    }

    /// A cada tique: a aba criada pelo "+" da secundária vira a ativa dela, e a secundária que
    /// passou a ser a principal (clique na barra esquerda) deixa a metade vazia.
    pub(crate) fn tick_arranjo(&mut self, outcome: &mut ClientShellInput) {
        outcome.resize |= std::mem::take(&mut self.arranjo.redimensionar);
        let Some(snapshot) = self.snapshot.as_deref() else {
            return;
        };
        let pasta = self.arranjo.secundaria().map(str::to_owned);
        if pasta.is_some() && pasta == snapshot.focused_workspace_id {
            self.arranjo.pasta_secundaria = None;
            self.arranjo.painel_secundario = None;
            self.enviar_secundaria(None, outcome);
            return;
        }
        let (Some(pasta), Some(antes)) = (pasta, self.arranjo.abas_antes_da_nova.as_ref()) else {
            return;
        };
        let nova = snapshot
            .tabs
            .iter()
            .find(|tab| tab.workspace_id == pasta && !antes.contains(&tab.tab_id))
            .map(|tab| tab.tab_id.clone());
        if let Some(nova) = nova {
            self.arranjo.abas_antes_da_nova = None;
            self.escolher_aba_secundaria(nova, outcome);
        }
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
        self.arranjo.painel_secundario = None;
        self.enviar_secundaria(None, outcome);
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

    /// Barra de apoio. Devolve verdadeiro quando o evento foi dela.
    pub(super) fn tratar_mouse_arranjo(
        &mut self,
        mouse: crossterm::event::MouseEvent,
        outcome: &mut ClientShellInput,
    ) -> bool {
        let point = (mouse.column, mouse.row);
        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
            let aba = self
                .hits
                .abas_secundaria
                .iter()
                .find(|(area, _)| super::contains(*area, point))
                .map(|(_, tab_id)| tab_id.clone());
            if let Some(tab_id) = aba {
                self.escolher_aba_secundaria(tab_id, outcome);
                return true;
            }
            if super::contains(self.hits.nova_aba_secundaria, point) {
                self.nova_aba_secundaria(outcome);
                return true;
            }
            if super::contains(self.hits.fechar_secundaria, point) {
                self.fechar_secundaria(outcome);
                return true;
            }
        }
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
}
