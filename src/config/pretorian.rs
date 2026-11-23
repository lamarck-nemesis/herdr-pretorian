use std::path::Path;

use serde::Deserialize;

use super::{Config, TabBarRightEntryConfig};

/// Mesmo limite usado pelo lançador do Pretorian ao gravar o cabeçalho.
pub const MAX_NOME_CHARS: usize = 100;
/// Grafia da marca na barra de título da janela.
pub const MARCA: &str = "LSA • prεtοrιαn";
pub const SEPARADOR: &str = " │ ";
/// Assinatura do Lamarck Hub, mostrada à direita da barra de abas.
pub fn assinatura() -> String {
    format!("nεαd.tεch │ v. {}", crate::build_info::pretorian_version())
}

/// Seção `[pretorian]` própria deste fork; fica fora de `[ui]` para não colidir com chaves do
/// Herdr original em atualizações futuras.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PretorianConfig {
    /// Nome mostrado no cabeçalho como `Pretorian · Nome`. Vazio ou ausente mostra só `Pretorian`.
    pub nome: Option<String>,
    pub agente: Option<String>,
    pub nivel: super::NivelRaciocinio,
    /// Login do PJe sem janela (headless). O Pretorian CLI lê esta chave; `PRETORIAN_PJE_SEM_JANELA` a sobrepõe.
    pub pje_sem_janela: bool,
}

impl PretorianConfig {
    pub(crate) fn nome_efetivo(&self) -> Option<String> {
        self.nome_com_identidade(std::env::var("PRETORIAN_NOME_CENTAURO").ok().as_deref())
    }

    fn nome_com_identidade(&self, identidade: Option<&str>) -> Option<String> {
        identidade
            .and_then(sanitizar_nome)
            .or_else(|| self.nome.as_deref().and_then(sanitizar_nome))
    }
}

pub fn comando_agente(agente: Option<&str>, perfil: &Path) -> Result<Option<Vec<String>>, String> {
    match agente {
        None | Some("") => Ok(None),
        Some("codex") => Ok(Some(vec!["codex".into()])),
        Some("claude" | "deepseek") => Ok(Some(vec![
            "claude".into(),
            "--strict-mcp-config".into(),
            "--mcp-config".into(),
            perfil.join(".mcp.json").to_string_lossy().into_owned(),
        ])),
        Some(_) => {
            Err("Agente do Pretorian não suportado; escolha Codex, Claude ou DeepSeek.".into())
        }
    }
}

pub fn sanitizar_nome(valor: &str) -> Option<String> {
    let nome = valor
        .chars()
        .filter(|ch| !ch.is_control())
        .take(MAX_NOME_CHARS)
        .collect::<String>()
        .trim()
        .to_string();
    (!nome.is_empty()).then_some(nome)
}

impl Config {
    /// Sem `ui.window_title` próprio, o título da janela é só a marca.
    pub(crate) fn window_title_efetivo(&self) -> String {
        if self.ui.window_title != super::window_title::default_window_title() {
            return self.ui.window_title.clone();
        }
        MARCA.to_string()
    }

    /// `ui.tab_bar_right` explícito prevalece (inclusive o gravado pelo lançador); sem ele, a
    /// barra de abas fica limpa: a assinatura mora no pé da barra esquerda.
    pub(crate) fn tab_bar_right_efetivo(&self) -> Vec<TabBarRightEntryConfig> {
        self.ui.tab_bar_right.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nome_centauro_sanitizado_prevalece_sem_alterar_nome_local() {
        let config = PretorianConfig {
            nome: Some("Nome local".into()),
            ..Default::default()
        };
        assert_eq!(
            config
                .nome_com_identidade(Some("  Pessoa sintética\n  "))
                .as_deref(),
            Some("Pessoa sintética")
        );
        assert_eq!(
            config.nome_com_identidade(None).as_deref(),
            Some("Nome local")
        );
        assert_eq!(config.nome_com_identidade(Some("\u{7}")), None);
        assert_eq!(config.nome.as_deref(), Some("Nome local"));
    }

    #[test]
    fn nova_aba_preserva_agente_e_conexoes_do_perfil() {
        let perfil = Path::new("perfil com espaços");
        assert_eq!(comando_agente(None, perfil).unwrap(), None);
        assert_eq!(
            comando_agente(Some("codex"), perfil).unwrap(),
            Some(vec!["codex".into()])
        );
        let claude = comando_agente(Some("claude"), perfil).unwrap().unwrap();
        assert_eq!(
            &claude[..3],
            &["claude", "--strict-mcp-config", "--mcp-config"]
        );
        assert_eq!(Path::new(&claude[3]), perfil.join(".mcp.json"));
        assert!(comando_agente(Some("codex; outro-comando"), perfil).is_err());
    }

    #[test]
    fn titulo_da_janela_e_so_a_marca_quando_nao_configurado() {
        let config: Config = toml::from_str("[pretorian]\nnome = \"Ana\"\n").unwrap();
        assert_eq!(config.window_title_efetivo(), "LSA • prεtοrιαn");

        let config: Config = toml::from_str("[ui]\nwindow_title = \"{workspace}\"\n").unwrap();
        assert_eq!(config.window_title_efetivo(), "{workspace}");
    }

    #[test]
    fn identidade_em_branco_ou_so_controle_cai_para_o_nome_do_config() {
        let config: Config = toml::from_str("[pretorian]\nnome = \"Ana\"\n").unwrap();
        let pretorian = &config.pretorian;
        assert_eq!(
            pretorian.nome_com_identidade(Some("Beto")).as_deref(),
            Some("Beto")
        );
        for vazia in ["", "   ", "\u{7}\n"] {
            assert_eq!(
                pretorian.nome_com_identidade(Some(vazia)).as_deref(),
                Some("Ana"),
                "{vazia:?}"
            );
        }
        assert_eq!(pretorian.nome_com_identidade(None).as_deref(), Some("Ana"));
        assert_eq!(
            PretorianConfig::default().nome_com_identidade(Some("  ")),
            None
        );
    }

    #[test]
    fn sanitizar_nome_remove_controles_apara_e_limita() {
        assert_eq!(
            sanitizar_nome("  Ana\u{1b}[0m Souza\n ").as_deref(),
            Some("Ana[0m Souza")
        );
        assert_eq!(sanitizar_nome(""), None);
        assert_eq!(sanitizar_nome("   "), None);
        assert_eq!(sanitizar_nome("\u{7}"), None);
        let longo = "á".repeat(150);
        assert_eq!(
            sanitizar_nome(&longo).map(|nome| nome.chars().count()),
            Some(MAX_NOME_CHARS)
        );
    }

    #[test]
    fn tab_bar_right_explicito_prevalece() {
        let config: Config = toml::from_str(
            r#"
[ui]
tab_bar_right = [{ type = "text", text = "Pretorian · X" }]

[pretorian]
nome = "Y"
"#,
        )
        .unwrap();
        assert_eq!(
            config.tab_bar_right_efetivo(),
            vec![TabBarRightEntryConfig::Text {
                text: "Pretorian · X".into()
            }]
        );
    }

    #[test]
    fn tab_bar_right_vazio_deixa_a_barra_de_abas_limpa() {
        let config: Config = toml::from_str("[pretorian]\nnome = \" Ana \"\n").unwrap();
        assert!(config.tab_bar_right_efetivo().is_empty());
        assert!(Config::default().tab_bar_right_efetivo().is_empty());
    }
}
