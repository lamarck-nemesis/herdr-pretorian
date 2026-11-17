use super::*;
use ratatui::{
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};
use serde::Deserialize;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Etapa {
    Disponivel,
    Atualizando,
    Concluido,
    Aguardando,
    Erro,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct Estado {
    #[serde(default)]
    sequencia: u64,
    estado: Etapa,
    mensagem: String,
    percentual: u8,
    notas: Vec<String>,
    reiniciar: bool,
    #[serde(default)]
    pedido_reinicio: bool,
}

pub(super) struct Atualizacao {
    canal: Option<PathBuf>,
    consulta: Instant,
    estado: Option<Estado>,
    recebido: Option<Estado>,
    botao: Rect,
}

impl Default for Atualizacao {
    fn default() -> Self {
        Self {
            canal: std::env::var_os("PRETORIAN_ATUALIZACAO").map(PathBuf::from),
            consulta: Instant::now(),
            estado: None,
            recebido: None,
            botao: Rect::default(),
        }
    }
}

impl Atualizacao {
    pub(super) fn visivel(&self) -> bool {
        self.estado.is_some()
    }

    fn consultar(&mut self, now: Instant) -> bool {
        let Some(canal) = self.canal.as_ref() else {
            return false;
        };
        if now.saturating_duration_since(self.consulta) < Duration::from_secs(1) {
            return false;
        }
        self.consulta = now;
        let Ok(texto) = std::fs::read_to_string(canal.join("estado.json")) else {
            return false;
        };
        if texto.len() > 8192 {
            return false;
        }
        let Ok(estado) = serde_json::from_str::<Estado>(&texto) else {
            return false;
        };
        if self.recebido.as_ref() == Some(&estado) {
            return false;
        }
        self.recebido = Some(estado.clone());
        self.estado = Some(estado);
        true
    }

    fn ativar(&mut self, outcome: &mut ClientShellInput) {
        let (Some(estado), Some(canal)) = (self.estado.as_ref(), self.canal.as_ref()) else {
            return;
        };
        match estado.estado {
            Etapa::Disponivel | Etapa::Erro => {
                if std::fs::write(canal.join("atualizar"), b"1").is_ok() {
                    self.estado = Some(Estado {
                        sequencia: estado.sequencia,
                        estado: Etapa::Atualizando,
                        mensagem: "Preparando a atualização…".into(),
                        percentual: 0,
                        notas: vec![],
                        reiniciar: false,
                        pedido_reinicio: estado.pedido_reinicio,
                    });
                }
            }
            Etapa::Concluido | Etapa::Aguardando => {
                if estado.reiniciar && estado.pedido_reinicio {
                    if std::fs::write(canal.join("reiniciar"), b"1").is_ok() {
                        let mut aguardando = estado.clone();
                        aguardando.estado = Etapa::Atualizando;
                        aguardando.mensagem = "Guardando e retomando suas conversas…".into();
                        self.estado = Some(aguardando);
                        outcome.repaint = true;
                    }
                    return;
                }
                if estado.reiniciar {
                    crate::terminal_effects::request_update();
                    outcome.detach = true;
                }
                let _ = std::fs::remove_file(canal.join("estado.json"));
                self.estado = None;
            }
            Etapa::Atualizando => {}
        }
        outcome.repaint = true;
    }

    pub(super) fn input(
        &mut self,
        event: &crate::raw_input::RawInputEvent,
        outcome: &mut ClientShellInput,
    ) -> bool {
        use crate::raw_input::RawInputEvent;
        use crossterm::event::{KeyEventKind, MouseButton, MouseEventKind};
        if self.estado.is_none() {
            return false;
        }
        match event {
            RawInputEvent::Key(key)
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Enter =>
            {
                self.ativar(outcome)
            }
            RawInputEvent::Mouse(mouse)
                if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                    && self.botao.contains((mouse.column, mouse.row).into()) =>
            {
                self.ativar(outcome)
            }
            RawInputEvent::Key(_)
            | RawInputEvent::Text(_)
            | RawInputEvent::Mouse(_)
            | RawInputEvent::Paste(_) => {}
            _ => return false,
        }
        true
    }

    pub(super) fn render(&mut self, buffer: &mut Buffer, palette: &Palette) -> Option<Rect> {
        let estado = self.estado.as_ref()?;
        let area = buffer.area;
        let width = area.width.min(72);
        let height = area.height.min(17);
        let rect = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        let title = match estado.estado {
            Etapa::Disponivel => " Atualização obrigatória ",
            Etapa::Atualizando => " Atualizando o Pretorian ",
            Etapa::Concluido if estado.reiniciar => " Atualização instalada ",
            Etapa::Concluido => " Pretorian atualizado ",
            Etapa::Erro => " Atualização não concluída ",
            Etapa::Aguardando => " Atualização aguardando ",
        };
        Clear.render(rect, buffer);
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .style(Style::default().bg(palette.panel_bg).fg(palette.text));
        let inner = block.inner(rect);
        block.render(rect, buffer);
        let mut linhas = vec![Line::from("")];
        if estado.estado == Etapa::Disponivel {
            for nota in estado.notas.iter().take(3) {
                linhas.push(Line::from(format!(" • {nota}")));
                linhas.push(Line::from(""));
            }
        } else {
            linhas.push(Line::from(estado.mensagem.clone()));
            linhas.push(Line::from(""));
            if matches!(estado.estado, Etapa::Atualizando | Etapa::Concluido) {
                let percentual = estado.percentual.min(100);
                let n = usize::from(percentual / 5);
                linhas.push(Line::from(format!(
                    "[{}{}] {percentual}% das etapas",
                    "━".repeat(n),
                    "─".repeat(20 - n)
                )));
            }
        }
        let corpo = Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(2),
        );
        Paragraph::new(linhas)
            .wrap(Wrap { trim: false })
            .render(corpo, buffer);
        self.botao = Rect::new(
            inner.x,
            inner.bottom().saturating_sub(1),
            inner.width,
            u16::from(inner.height > 0),
        );
        let label = match estado.estado {
            Etapa::Disponivel => "[ Atualizar ]",
            Etapa::Atualizando => "Aguarde… Suas sessões continuam abertas.",
            Etapa::Concluido => "[ Continuar ]",
            Etapa::Erro => "[ Tentar novamente ]",
            Etapa::Aguardando => "[ Voltar à conversa ]",
        };
        Paragraph::new(label)
            .centered()
            .style(Style::default().fg(palette.accent))
            .render(self.botao, buffer);
        Some(rect)
    }
}

impl ClientShellState {
    pub(crate) fn tick_atualizacao(&mut self, now: Instant) -> bool {
        self.atualizacao.consultar(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cartao(etapa: Etapa) -> Atualizacao {
        Atualizacao {
            canal: None,
            consulta: Instant::now(),
            recebido: None,
            estado: Some(Estado {
                sequencia: 1,
                estado: etapa,
                mensagem: "Suas sessões continuam abertas.".into(),
                percentual: 35,
                notas: vec!["Permite atualizar sem encerrar sessões.".into()],
                reiniciar: false,
                pedido_reinicio: false,
            }),
            botao: Rect::default(),
        }
    }

    #[test]
    fn atualizacao_obrigatoria_aparece_no_centro_com_novidades() {
        let mut card = cartao(Etapa::Disponivel);
        let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 30));
        let rect = card
            .render(&mut buffer, &Palette::from_name("pretorian").unwrap())
            .unwrap();
        assert_eq!(rect.x, 14);
        let texto: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(texto.contains("Atualização obrigatória"));
        assert!(texto.contains("Permite atualizar sem encerrar sessões."));
        assert!(texto.contains("[ Atualizar ]"));
    }

    #[test]
    fn cartao_bloqueia_escape_e_texto_sem_enviar_para_sessao() {
        let mut card = cartao(Etapa::Disponivel);
        let mut outcome = ClientShellInput::default();
        assert!(card.input(
            &crate::raw_input::RawInputEvent::Key(crate::input::TerminalKey::new(
                KeyCode::Esc,
                crossterm::event::KeyModifiers::empty()
            )),
            &mut outcome
        ));
        assert!(card.input(
            &crate::raw_input::RawInputEvent::Paste("texto".into()),
            &mut outcome
        ));
        assert!(outcome.requests.is_empty());
        assert!(!outcome.detach);
        assert!(card.visivel());
    }

    #[test]
    fn aceite_nao_reabre_aviso_antigo_e_recebe_nova_falha() {
        let canal = std::env::temp_dir().join(format!(
            "pretorian-card-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&canal).unwrap();
        let mut card = cartao(Etapa::Disponivel);
        card.canal = Some(canal.clone());
        let inicio = Instant::now();
        std::fs::write(canal.join("estado.json"), r#"{"sequencia":1,"estado":"disponivel","mensagem":"","percentual":0,"notas":[],"reiniciar":false}"#).unwrap();
        assert!(card.consultar(inicio + Duration::from_secs(2)));
        card.ativar(&mut ClientShellInput::default());
        assert!(!card.consultar(inicio + Duration::from_secs(4)));
        assert_eq!(card.estado.as_ref().unwrap().estado, Etapa::Atualizando);
        std::fs::write(canal.join("estado.json"), r#"{"sequencia":2,"estado":"erro","mensagem":"Falha","percentual":0,"notas":[],"reiniciar":false}"#).unwrap();
        assert!(card.consultar(inicio + Duration::from_secs(6)));
        assert_eq!(card.estado.as_ref().unwrap().estado, Etapa::Erro);
        card.ativar(&mut ClientShellInput::default());
        std::fs::write(canal.join("estado.json"), r#"{"sequencia":3,"estado":"erro","mensagem":"Falha","percentual":0,"notas":[],"reiniciar":false}"#).unwrap();
        assert!(card.consultar(inicio + Duration::from_secs(8)));
        assert_eq!(card.estado.as_ref().unwrap().estado, Etapa::Erro);
        std::fs::remove_dir_all(canal).unwrap();
    }

    #[test]
    fn progresso_nao_oferece_botao_de_reinicio() {
        let mut card = cartao(Etapa::Atualizando);
        let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 30));
        card.render(&mut buffer, &Palette::from_name("pretorian").unwrap());
        let texto: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(texto.contains("35% das etapas"));
        assert!(!texto.contains("[ Continuar ]"));
    }
    #[test]
    fn conversa_ocupada_pode_ser_acessada_sem_anunciar_atualizacao_concluida() {
        let mut card = cartao(Etapa::Aguardando);
        let canal = std::env::temp_dir().join(format!("pretorian-ocupada-{}", std::process::id()));
        std::fs::create_dir(&canal).unwrap();
        card.canal = Some(canal.clone());
        let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 30));
        card.render(&mut buffer, &Palette::from_name("pretorian").unwrap());
        let texto: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(texto.contains("Atualização aguardando"));
        assert!(texto.contains("[ Voltar à conversa ]"));
        assert!(!texto.contains("Pretorian atualizado"));
        let mut outcome = ClientShellInput::default();
        card.ativar(&mut outcome);
        assert!(!card.visivel());
        assert!(!outcome.detach);
        std::fs::remove_dir_all(canal).unwrap();
    }

    #[test]
    fn retomada_por_pedido_mantem_cliente_aberto_ate_confirmacao_do_lancador() {
        let canal = std::env::temp_dir().join(format!("pretorian-reinicio-{}", std::process::id()));
        std::fs::create_dir_all(&canal).unwrap();
        let mut card = cartao(Etapa::Concluido);
        card.canal = Some(canal.clone());
        let estado = card.estado.as_mut().unwrap();
        estado.reiniciar = true;
        estado.pedido_reinicio = true;
        let mut outcome = ClientShellInput::default();
        card.ativar(&mut outcome);
        assert_eq!(
            std::fs::read_to_string(canal.join("reiniciar")).unwrap(),
            "1"
        );
        assert!(!outcome.detach);
        assert_eq!(card.estado.as_ref().unwrap().estado, Etapa::Atualizando);
        std::fs::remove_dir_all(canal).unwrap();
    }
}
