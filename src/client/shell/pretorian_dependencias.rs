//! Dependências da estação (conta Google no navegador, conferência do BB…), mantidas em arquivo
//! JSON pelo lançador do CLI.
//!
//! O fork não confere nada: lê o arquivo indicado por `PRETORIAN_DEPENDENCIAS_ESTADO` e, quando a
//! pessoa ativa um item em configurações › usuário, grava o id dele em `<estado>.pedido`. O
//! lançador abre a janela da ação e regrava o estado.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const ESTADO_ENV: &str = "PRETORIAN_DEPENDENCIAS_ESTADO";
const VERSAO: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub(super) struct Dependencia {
    pub(super) id: String,
    pub(super) titulo: String,
    pub(super) estado: String,
    #[serde(default)]
    pub(super) detalhe: String,
    #[serde(default)]
    pub(super) acao: String,
}

impl Dependencia {
    /// A pessoa tem algo a fazer: o menu e a seção usuário ganham o sinal de atenção.
    pub(super) fn pede_atencao(&self) -> bool {
        matches!(self.estado.as_str(), "pendente" | "falhou")
    }

    /// Enter executa a ação; bloqueado espera outro item e em andamento já tem janela aberta.
    pub(super) fn aceita_pedido(&self) -> bool {
        matches!(self.estado.as_str(), "pendente" | "falhou" | "ok")
    }

    pub(super) fn simbolo(&self) -> &'static str {
        match self.estado.as_str() {
            "ok" => "✓",
            "em_andamento" => "◐",
            "bloqueado" => "○",
            _ => "●",
        }
    }
}

#[derive(serde::Deserialize)]
struct Arquivo {
    versao: u32,
    itens: Vec<Dependencia>,
}

fn interpretar(texto: &str) -> Vec<Dependencia> {
    match serde_json::from_str::<Arquivo>(texto) {
        Ok(arquivo) if arquivo.versao == VERSAO => arquivo.itens,
        _ => Vec::new(),
    }
}

fn arquivo() -> Option<PathBuf> {
    std::env::var_os(ESTADO_ENV)
        .filter(|valor| !valor.is_empty())
        .map(PathBuf::from)
}

fn ler(caminho: &Path) -> Vec<Dependencia> {
    std::fs::read_to_string(caminho)
        .map(|texto| interpretar(&texto))
        .unwrap_or_default()
}

// O menu é consultado a cada redesenho; o arquivo é relido no máximo uma vez por segundo.
static LEITURA: Mutex<Option<(Instant, Vec<Dependencia>)>> = Mutex::new(None);

/// Itens na ordem do lançador; vazio fora do lançador do Pretorian ou com arquivo ilegível.
pub(super) fn itens() -> Vec<Dependencia> {
    let Some(caminho) = arquivo() else {
        return Vec::new();
    };
    let mut leitura = LEITURA.lock().unwrap_or_else(|erro| erro.into_inner());
    if let Some((quando, itens)) = leitura.as_ref() {
        if quando.elapsed() < Duration::from_secs(1) {
            return itens.clone();
        }
    }
    let itens = ler(&caminho);
    *leitura = Some((Instant::now(), itens.clone()));
    itens
}

pub(super) fn pede_atencao() -> bool {
    itens().iter().any(Dependencia::pede_atencao)
}

fn gravar_pedido(caminho: &Path, id: &str) -> std::io::Result<()> {
    let mut pedido = caminho.as_os_str().to_owned();
    pedido.push(".pedido");
    std::fs::write(pedido, format!("{id}\n"))
}

/// Pede ao lançador a ação do item. Sem arquivo configurado ou sem permissão, nada muda.
pub(super) fn pedir(id: &str) {
    let Some(caminho) = arquivo() else {
        return;
    };
    if gravar_pedido(&caminho, id).is_ok() {
        *LEITURA.lock().unwrap_or_else(|erro| erro.into_inner()) = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXEMPLO: &str = r#"{"versao": 1, "itens": [
        {"id": "google", "titulo": "Conta Google no navegador", "estado": "pendente",
         "detalhe": "Entre uma vez.", "acao": "entrar no Google"},
        {"id": "bb-resgate", "titulo": "Consulta de resgate judicial do BB", "estado": "bloqueado",
         "detalhe": "Conclua antes: Conta Google no navegador.", "acao": "conferir"}]}"#;

    #[test]
    fn interpreta_o_arquivo_do_lancador() {
        let itens = interpretar(EXEMPLO);
        assert_eq!(itens.len(), 2);
        assert_eq!(itens[0].id, "google");
        assert!(itens[0].pede_atencao());
        assert!(itens[0].aceita_pedido());
        assert!(!itens[1].pede_atencao());
        assert!(!itens[1].aceita_pedido());
        assert_eq!(itens[1].simbolo(), "○");
    }

    #[test]
    fn versao_desconhecida_ou_texto_invalido_nao_mostra_nada() {
        assert!(interpretar(&EXEMPLO.replace("\"versao\": 1", "\"versao\": 2")).is_empty());
        assert!(interpretar("não é json").is_empty());
    }

    #[test]
    fn estados_e_simbolos() {
        let item = |estado: &str| Dependencia {
            id: "x".into(),
            titulo: "X".into(),
            estado: estado.into(),
            detalhe: String::new(),
            acao: String::new(),
        };
        assert_eq!(item("ok").simbolo(), "✓");
        assert!(item("ok").aceita_pedido());
        assert!(!item("ok").pede_atencao());
        assert_eq!(item("em_andamento").simbolo(), "◐");
        assert!(!item("em_andamento").aceita_pedido());
        assert!(item("falhou").pede_atencao());
        assert_eq!(item("falhou").simbolo(), "●");
    }

    #[test]
    fn pedido_grava_o_id_ao_lado_do_estado() {
        let pasta =
            std::env::temp_dir().join(format!("pretorian-dependencias-{}", std::process::id()));
        std::fs::create_dir_all(&pasta).unwrap();
        gravar_pedido(&pasta.join("dependencias"), "google").unwrap();
        let pedido = std::fs::read_to_string(pasta.join("dependencias.pedido")).unwrap();
        std::fs::remove_dir_all(&pasta).unwrap();
        assert_eq!(pedido, "google\n");
    }
}
