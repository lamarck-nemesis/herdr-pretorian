use serde::Deserialize;

use super::{Config, TabBarRightEntryConfig};

/// Mesmo limite usado pelo lançador do Pretorian ao gravar o cabeçalho.
pub const MAX_NOME_CHARS: usize = 100;
/// Grafia da marca na barra de título da janela.
pub const MARCA: &str = "LSΛ prεtοrιαn";
pub const SEPARADOR: &str = " │ ";
/// Assinatura do Lamarck Hub, mostrada à direita da barra de abas.
const ASSINATURA: &str = "⌘ nεαd.tεch │ 2026";

/// Seção `[pretorian]` própria deste fork; fica fora de `[ui]` para não colidir com chaves do
/// Herdr original em atualizações futuras.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PretorianConfig {
    /// Nome mostrado no cabeçalho como `Pretorian · Nome`. Vazio ou ausente mostra só `Pretorian`.
    pub nome: Option<String>,
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

pub fn cabecalho(nome: Option<&str>) -> String {
    match nome {
        Some(nome) => format!("{MARCA}{SEPARADOR}{nome}"),
        None => MARCA.to_string(),
    }
}

impl Config {
    /// Sem `ui.window_title` próprio, o título da janela é a marca seguida do nome do usuário.
    pub(crate) fn window_title_efetivo(&self) -> String {
        if self.ui.window_title != super::window_title::default_window_title() {
            return self.ui.window_title.clone();
        }
        let nome = self.pretorian.nome.as_deref().and_then(sanitizar_nome);
        // Chaves no nome seriam lidas como marcador do formato de título.
        cabecalho(nome.as_deref())
            .replace('{', "{{")
            .replace('}', "}}")
    }

    /// `ui.tab_bar_right` explícito prevalece (inclusive o gravado pelo lançador); sem ele, a
    /// barra mostra a assinatura. Marca e usuário ficam no título da janela.
    pub(crate) fn tab_bar_right_efetivo(&self) -> Vec<TabBarRightEntryConfig> {
        if !self.ui.tab_bar_right.is_empty() {
            return self.ui.tab_bar_right.clone();
        }
        vec![TabBarRightEntryConfig::Text {
            text: ASSINATURA.to_string(),
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cabecalho_sem_nome_e_so_a_marca() {
        assert_eq!(cabecalho(None), "LSΛ prεtοrιαn");
    }

    #[test]
    fn titulo_da_janela_usa_marca_e_nome_salvo_quando_configurado() {
        let config: Config = toml::from_str("[pretorian]\nnome = \"Ana {1}\"\n").unwrap();
        assert_eq!(config.window_title_efetivo(), "LSΛ prεtοrιαn │ Ana {{1}}");

        let config: Config = toml::from_str("[ui]\nwindow_title = \"{workspace}\"\n").unwrap();
        assert_eq!(config.window_title_efetivo(), "{workspace}");
    }

    #[test]
    fn cabecalho_com_nome_usa_divisor() {
        assert_eq!(cabecalho(Some("Ana")), "LSΛ prεtοrιαn │ Ana");
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
    fn tab_bar_right_vazio_usa_assinatura_sem_nome() {
        let assinatura = vec![TabBarRightEntryConfig::Text {
            text: "⌘ nεαd.tεch │ 2026".into(),
        }];
        let config: Config = toml::from_str("[pretorian]\nnome = \" Ana \"\n").unwrap();
        assert_eq!(config.tab_bar_right_efetivo(), assinatura);
        assert_eq!(Config::default().tab_bar_right_efetivo(), assinatura);
    }
}
