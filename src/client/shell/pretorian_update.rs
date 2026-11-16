//! Estado da atualização do Pretorian, mantido em arquivo pelo lançador do CLI.
//!
//! O fork não confere versão nem baixa nada. O lançador grava uma palavra no arquivo
//! indicado por `PRETORIAN_ATUALIZACAO_ESTADO` e instala a versão nova quando encontra
//! o pedido gravado ao lado dele.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const ESTADO_ENV: &str = "PRETORIAN_ATUALIZACAO_ESTADO";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PretorianUpdate {
    Disponivel,
    Instalando,
    Instalada,
    Falhou,
}

impl PretorianUpdate {
    pub(super) fn rotulo(self) -> &'static str {
        match self {
            Self::Disponivel => "atualizar o Pretorian",
            Self::Instalando => "atualizando o Pretorian…",
            Self::Instalada => "atualizado: vale ao reabrir",
            Self::Falhou => "atualização falhou: tentar de novo",
        }
    }

    /// Estados em que a pessoa tem algo a fazer: o menu ganha o sinal de atenção.
    pub(super) fn pede_atencao(self) -> bool {
        matches!(self, Self::Disponivel | Self::Falhou)
    }
}

fn interpretar(texto: &str) -> Option<PretorianUpdate> {
    match texto.trim() {
        "disponivel" => Some(PretorianUpdate::Disponivel),
        "instalando" => Some(PretorianUpdate::Instalando),
        "instalada" => Some(PretorianUpdate::Instalada),
        "falhou" => Some(PretorianUpdate::Falhou),
        _ => None,
    }
}

fn ler(caminho: &Path) -> Option<PretorianUpdate> {
    std::fs::read_to_string(caminho)
        .ok()
        .and_then(|texto| interpretar(&texto))
}

fn arquivo() -> Option<PathBuf> {
    std::env::var_os(ESTADO_ENV)
        .filter(|valor| !valor.is_empty())
        .map(PathBuf::from)
}

// O menu é consultado a cada redesenho; o arquivo é relido no máximo uma vez por segundo.
static LEITURA: Mutex<Option<(Instant, Option<PretorianUpdate>)>> = Mutex::new(None);

pub(super) fn estado() -> Option<PretorianUpdate> {
    let caminho = arquivo()?;
    let mut leitura = LEITURA.lock().unwrap_or_else(|erro| erro.into_inner());
    match *leitura {
        Some((quando, valor)) if quando.elapsed() < Duration::from_secs(1) => return valor,
        _ => {}
    }
    let valor = ler(&caminho);
    *leitura = Some((Instant::now(), valor));
    valor
}

fn gravar_pedido(caminho: &Path) -> std::io::Result<()> {
    let progresso = caminho.with_file_name(format!(
        "{}.progresso",
        caminho.file_name().unwrap_or_default().to_string_lossy()
    ));
    match std::fs::remove_file(progresso) {
        Ok(()) => {}
        Err(erro) if erro.kind() == std::io::ErrorKind::NotFound => {}
        Err(erro) => return Err(erro),
    }
    // O estado muda aqui para o menu responder de imediato; o lançador conclui com `instalada` ou `falhou`.
    std::fs::write(caminho, "instalando\n")?;
    let mut pedido = caminho.as_os_str().to_owned();
    pedido.push(".pedido");
    std::fs::write(pedido, "instalar\n")
}

/// Pede a instalação ao lançador. Sem arquivo configurado ou sem permissão, nada muda.
pub(super) fn pedir() {
    let Some(caminho) = arquivo() else {
        return;
    };
    if gravar_pedido(&caminho).is_ok() {
        *LEITURA.lock().unwrap_or_else(|erro| erro.into_inner()) = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpreta_somente_os_estados_conhecidos() {
        assert_eq!(
            interpretar("disponivel\n"),
            Some(PretorianUpdate::Disponivel)
        );
        assert_eq!(
            interpretar(" instalando "),
            Some(PretorianUpdate::Instalando)
        );
        assert_eq!(interpretar("instalada"), Some(PretorianUpdate::Instalada));
        assert_eq!(interpretar("falhou"), Some(PretorianUpdate::Falhou));
        assert_eq!(interpretar(""), None);
        assert_eq!(interpretar("em_dia"), None);
        assert_eq!(interpretar("disponivel agora"), None);
    }

    #[test]
    fn so_disponivel_e_falha_pedem_atencao() {
        assert!(PretorianUpdate::Disponivel.pede_atencao());
        assert!(PretorianUpdate::Falhou.pede_atencao());
        assert!(!PretorianUpdate::Instalando.pede_atencao());
        assert!(!PretorianUpdate::Instalada.pede_atencao());
    }

    #[test]
    fn pedido_muda_o_estado_para_instalando_e_grava_o_arquivo_ao_lado() {
        let pasta = std::env::temp_dir().join(format!(
            "pretorian-atualizacao-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&pasta).unwrap();
        let estado = pasta.join("atualizacao");
        std::fs::write(&estado, "disponivel\n").unwrap();
        assert_eq!(ler(&estado), Some(PretorianUpdate::Disponivel));

        gravar_pedido(&estado).unwrap();

        assert_eq!(ler(&estado), Some(PretorianUpdate::Instalando));
        assert_eq!(
            std::fs::read_to_string(pasta.join("atualizacao.pedido")).unwrap(),
            "instalar\n"
        );
        assert_eq!(ler(&pasta.join("ausente")), None);
        std::fs::remove_dir_all(pasta).unwrap();
    }
}

#[derive(Debug)]
pub(super) struct Cartao {
    pub(super) percentual: u8,
    pub(super) mensagem: String,
    proxima: Instant,
}

impl Cartao {
    pub(super) fn novo() -> Self {
        Self {
            percentual: 0,
            mensagem: "Preparando atualização…".into(),
            proxima: Instant::now(),
        }
    }

    pub(super) fn atualizar(&mut self, agora: Instant) -> bool {
        if agora < self.proxima {
            return false;
        }
        self.proxima = agora + Duration::from_secs(1);
        if let Some(caminho) = arquivo() {
            let progresso = caminho.with_file_name(format!(
                "{}.progresso",
                caminho.file_name().unwrap_or_default().to_string_lossy()
            ));
            if let Ok(texto) = std::fs::read_to_string(progresso) {
                if let Ok(dados) = serde_json::from_str::<serde_json::Value>(&texto) {
                    self.percentual = dados["percentual"].as_u64().unwrap_or(0).min(100) as u8;
                    self.mensagem = dados["mensagem"]
                        .as_str()
                        .unwrap_or("Aguardando informações da atualização…")
                        .to_owned();
                }
            }
            match ler(&caminho) {
                Some(PretorianUpdate::Instalada) => {
                    self.percentual = 100;
                    self.mensagem = "Atualização concluída. Reabra o Pretorian.".into();
                }
                Some(PretorianUpdate::Falhou) => {
                    self.percentual = self.percentual.min(99);
                    self.mensagem = "Atualização não concluída. Tente novamente pelo menu.".into();
                }
                _ => {}
            }
        }
        true
    }
}
