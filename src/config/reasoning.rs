use serde::Deserialize;

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NivelRaciocinio {
    #[serde(alias = "estagiario")]
    Graduado,
    #[default]
    #[serde(alias = "mestre")]
    Especialista,
    Doutor,
}

impl NivelRaciocinio {
    pub const TODOS: [Self; 3] = [Self::Graduado, Self::Especialista, Self::Doutor];

    pub fn chave(self) -> &'static str {
        match self {
            Self::Graduado => "graduado",
            Self::Especialista => "especialista",
            Self::Doutor => "doutor",
        }
    }

    /// Rótulo do diálogo de nível ao abrir uma aba; o número é a tecla que escolhe o nível.
    pub fn rotulo_numerado(self) -> &'static str {
        match self {
            Self::Graduado => {
                "1 · Estagiário · agilidade para tarefas simples e rotinas bem definidas."
            }
            Self::Especialista => {
                "2 · Especialista (padrão) · equilíbrio entre rapidez e precisão. Use no dia a dia."
            }
            Self::Doutor => {
                "3 · Doutor · análise aprofundada para casos desafiadores. Use excepcionalmente."
            }
        }
    }

    pub fn descricao(self) -> &'static str {
        match self {
            Self::Graduado => "Estagiário · agilidade para tarefas simples e rotinas bem definidas.",
            Self::Especialista => {
                "Especialista (padrão) · equilíbrio entre rapidez e precisão. Use no dia a dia."
            }
            Self::Doutor => {
                "Doutor · análise aprofundada para casos desafiadores. Use excepcionalmente."
            }
        }
    }

    pub fn modelo_padrao(self, provedor: &str) -> Result<(&'static str, &'static str), String> {
        let modelo = match (provedor, self) {
            ("codex", Self::Graduado) => ("gpt-6-luna", "medium"),
            ("codex", Self::Especialista) => ("gpt-6.1-terra", "medium"),
            ("codex", Self::Doutor) => ("gpt-6.2-sol", "medium"),
            ("claude", Self::Graduado) => ("haiku", "medium"),
            ("claude", Self::Especialista) => ("sonnet", "medium"),
            ("claude", Self::Doutor) => ("claude-opus-5-5", "medium"),
            ("deepseek", Self::Graduado) => ("deepseek-flash", "high"),
            ("deepseek", Self::Especialista) => ("deepseek-v4-pro", "high"),
            ("deepseek", Self::Doutor) => ("deepseek-v4-pro", "max"),
            _ => return Err("Provedor de raciocínio não suportado.".into()),
        };
        Ok(modelo)
    }
}

pub fn provedores_disponiveis(valor: Option<&str>) -> Vec<&'static str> {
    let lista = valor.unwrap_or("codex,claude,deepseek");
    ["codex", "claude", "deepseek"]
        .into_iter()
        .filter(|nome| lista.split(',').any(|item| item.trim() == *nome))
        .collect()
}

/// Gateway de modelos do escritório: `(url, chave)` quando as duas variáveis estão no ambiente.
/// Com ele, Claude e Codex usam chave de API em vez do login de assinatura. O lançador valida a URL.
pub fn gateway_modelos(ambiente: impl Fn(&str) -> Option<String>) -> Option<(String, String)> {
    let valor = |nome: &str| {
        ambiente(nome)
            .map(|v| v.trim().trim_end_matches('/').to_owned())
            .filter(|v| !v.is_empty() && !v.chars().any(|c| c.is_control() || c.is_whitespace()))
    };
    Some((
        valor("PRETORIAN_GATEWAY_URL")?,
        valor("PRETORIAN_GATEWAY_CHAVE")?,
    ))
}

/// Provedor do Codex que lê a chave do ambiente; a chave não entra na linha de comando.
pub fn argumentos_gateway_codex(url: &str) -> Vec<String> {
    let texto = |valor: &str| toml::Value::String(valor.into()).to_string();
    [
        format!("model_provider={}", texto("pretorian")),
        format!("model_providers.pretorian.name={}", texto("Pretorian")),
        format!(
            "model_providers.pretorian.base_url={}",
            texto(&format!("{url}/v1"))
        ),
        format!(
            "model_providers.pretorian.env_key={}",
            texto("PRETORIAN_GATEWAY_CHAVE")
        ),
        format!("model_providers.pretorian.wire_api={}", texto("responses")),
    ]
    .into_iter()
    .flat_map(|valor| ["-c".to_owned(), valor])
    .collect()
}

pub fn configuracao_modelo(
    provedor: &str,
    nivel: NivelRaciocinio,
    ambiente: impl Fn(&str) -> Option<String>,
) -> Result<(String, String), String> {
    let (modelo, esforco) = nivel.modelo_padrao(provedor)?;
    let prefixo = format!(
        "PRETORIAN_{}_{}",
        provedor.to_uppercase(),
        nivel.chave().to_uppercase()
    );
    // No gateway o nome é o do nível, igual para Claude e Codex: quem escolhe o modelo real (e a
    // reserva) é o gateway. Só a URL é consultada; a chave não é lida para resolver o catálogo.
    let pelo_gateway = provedor != "deepseek"
        && ambiente("PRETORIAN_GATEWAY_URL").is_some_and(|url| !url.trim().is_empty());
    let modelo = ambiente(&format!("{prefixo}_MODELO")).unwrap_or_else(|| {
        if pelo_gateway {
            format!("pretorian-{}", nivel.chave())
        } else {
            modelo.into()
        }
    });
    let esforco = ambiente(&format!("{prefixo}_ESFORCO")).unwrap_or_else(|| esforco.into());
    if modelo.trim().is_empty() || modelo.chars().any(char::is_control) {
        return Err("O modelo configurado para este perfil é inválido.".into());
    }
    if !["low", "medium", "high", "xhigh", "max"].contains(&esforco.as_str()) {
        return Err("O esforço configurado para este perfil é inválido.".into());
    }
    let esforco = if provedor == "deepseek" && matches!(esforco.as_str(), "medium" | "xhigh") {
        "high".into()
    } else {
        esforco
    };
    Ok((modelo, esforco))
}

pub fn especialista_e_doutor_iguais(
    provedor: &str,
    ambiente: impl Fn(&str) -> Option<String>,
) -> bool {
    match (
        configuracao_modelo(provedor, NivelRaciocinio::Especialista, &ambiente),
        configuracao_modelo(provedor, NivelRaciocinio::Doutor, &ambiente),
    ) {
        (Ok(especialista), Ok(doutor)) => especialista == doutor,
        _ => false,
    }
}

pub fn resolver_raciocinio(
    provedor: &str,
    nivel: &str,
    ambiente: impl Fn(&str) -> Option<String>,
) -> Result<serde_json::Value, String> {
    let nivel: NivelRaciocinio = serde_json::from_value(serde_json::json!(nivel))
        .map_err(|_| "Nível de raciocínio inválido.")?;
    if !provedores_disponiveis(ambiente("PRETORIAN_PROVEDORES").as_deref()).contains(&provedor) {
        return Err("Provedor indisponível nesta instalação.".into());
    }
    let (modelo, esforco) = configuracao_modelo(provedor, nivel, ambiente)?;
    Ok(serde_json::json!({
        "versao": 1,
        "provedor": provedor,
        "agente": if provedor == "codex" { "codex" } else { "claude" },
        "nivel": nivel.chave(),
        "modelo": modelo,
        "esforco": esforco,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_usa_o_nome_do_nivel_para_claude_e_codex_e_preserva_deepseek() {
        let gateway = |chave: &str| {
            assert!(!chave.contains("CHAVE") && !chave.contains("KEY") && !chave.contains("TOKEN"));
            (chave == "PRETORIAN_GATEWAY_URL").then(|| "http://127.0.0.1:4000".to_owned())
        };
        for provedor in ["claude", "codex"] {
            for nivel in NivelRaciocinio::TODOS {
                let (modelo, _) = configuracao_modelo(provedor, nivel, gateway).unwrap();
                assert_eq!(modelo, format!("pretorian-{}", nivel.chave()));
            }
        }
        let (modelo, _) =
            configuracao_modelo("deepseek", NivelRaciocinio::Doutor, gateway).unwrap();
        assert_eq!(modelo, "deepseek-v4-pro");
        let (modelo, _) = configuracao_modelo("claude", NivelRaciocinio::Doutor, |chave| {
            Some(match chave {
                "PRETORIAN_CLAUDE_DOUTOR_MODELO" => "outro".to_owned(),
                _ => "http://127.0.0.1:4000".to_owned(),
            })
            .filter(|_| chave.ends_with("_MODELO") || chave == "PRETORIAN_GATEWAY_URL")
        })
        .unwrap();
        assert_eq!(modelo, "outro");
        let (modelo, _) = configuracao_modelo("claude", NivelRaciocinio::Doutor, |_| None).unwrap();
        assert_eq!(modelo, "claude-opus-5-5");
    }

    #[test]
    fn gateway_exige_url_e_chave_e_codex_nao_recebe_a_chave_em_argumento() {
        let com = |url: &'static str, chave: &'static str| {
            gateway_modelos(move |nome| {
                Some(match nome {
                    "PRETORIAN_GATEWAY_URL" => url.into(),
                    _ => chave.into(),
                })
                .filter(|v: &String| !v.is_empty())
            })
        };
        assert_eq!(com("", "sk-teste"), None);
        assert_eq!(com("http://127.0.0.1:4000", ""), None);
        assert_eq!(com("http://127.0.0.1:4000", "sk com espaço"), None);
        let (url, chave) = com("http://127.0.0.1:4000/", "sk-teste").unwrap();
        assert_eq!(
            (url.as_str(), chave.as_str()),
            ("http://127.0.0.1:4000", "sk-teste")
        );
        let argumentos = argumentos_gateway_codex(&url);
        assert!(argumentos.contains(
            &r#"model_providers.pretorian.base_url="http://127.0.0.1:4000/v1""#.to_owned()
        ));
        assert!(argumentos.contains(
            &r#"model_providers.pretorian.env_key="PRETORIAN_GATEWAY_CHAVE""#.to_owned()
        ));
        assert!(!argumentos.iter().any(|a| a.contains("sk-teste")));
    }

    #[test]
    fn resolucao_publica_reutiliza_catalogo_sem_ler_segredos() {
        let escolha = resolver_raciocinio("deepseek", "doutor", |chave| {
            assert!(!chave.contains("KEY") && !chave.contains("TOKEN"));
            (chave == "PRETORIAN_DEEPSEEK_DOUTOR_ESFORCO").then(|| "xhigh".into())
        })
        .unwrap();
        assert_eq!(
            escolha,
            serde_json::json!({"versao": 1, "provedor": "deepseek",
            "agente": "claude", "nivel": "doutor", "modelo": "deepseek-v4-pro", "esforco": "high"})
        );
        assert!(resolver_raciocinio("codex", "desconhecido", |_| None).is_err());
        assert!(resolver_raciocinio("desconhecido", "mestre", |_| None).is_err());
        assert!(
            resolver_raciocinio("codex", "mestre", |chave| (chave == "PRETORIAN_PROVEDORES")
                .then(String::new))
            .is_err()
        );
    }

    #[test]
    fn igualdade_de_configuracao_considera_esforco_efetivo_e_substituicoes() {
        assert!(!especialista_e_doutor_iguais("deepseek", |_| None));
        assert!(!especialista_e_doutor_iguais("codex", |_| None));
        assert!(!especialista_e_doutor_iguais("invalido", |_| None));
        assert!(!especialista_e_doutor_iguais("deepseek", |chave| {
            (chave == "PRETORIAN_DEEPSEEK_DOUTOR_MODELO").then(|| "outro-modelo".into())
        }));
        assert!(especialista_e_doutor_iguais("deepseek", |chave| {
            if chave.ends_with("ESPECIALISTA_ESFORCO") {
                Some("medium".into())
            } else if chave.ends_with("DOUTOR_ESFORCO") {
                Some("xhigh".into())
            } else {
                None
            }
        }));
        for esforco in ["medium", "xhigh"] {
            let (_, efetivo) = configuracao_modelo("deepseek", NivelRaciocinio::Doutor, |chave| {
                chave.ends_with("_ESFORCO").then(|| esforco.into())
            })
            .unwrap();
            assert_eq!(efetivo, "high");
        }
    }

    #[test]
    fn rotulos_numerados_seguem_a_ordem_das_teclas_do_dialogo() {
        for (indice, nivel) in NivelRaciocinio::TODOS.iter().enumerate() {
            assert!(
                nivel
                    .rotulo_numerado()
                    .starts_with(&format!("{} · ", indice + 1)),
                "{nivel:?}"
            );
        }
    }

    #[test]
    fn especialista_e_padrao_e_modelos_podem_mudar_sem_trocar_o_perfil() {
        assert_eq!(NivelRaciocinio::default(), NivelRaciocinio::Especialista);
        let (modelo, esforco) =
            configuracao_modelo("codex", NivelRaciocinio::Especialista, |chave| {
                (chave == "PRETORIAN_CODEX_ESPECIALISTA_MODELO").then(|| "modelo-novo".into())
            })
            .unwrap();
        assert_eq!(modelo, "modelo-novo");
        assert_eq!(esforco, "medium");
        assert!(
            configuracao_modelo("codex", NivelRaciocinio::Doutor, |_| Some("\n".into())).is_err()
        );
    }

    #[test]
    fn preferencias_antigas_convergem_para_niveis_atuais() {
        for (antigo, esperado) in [
            ("estagiario", NivelRaciocinio::Graduado),
            ("mestre", NivelRaciocinio::Especialista),
        ] {
            let nivel: NivelRaciocinio = serde_json::from_value(serde_json::json!(antigo)).unwrap();
            assert_eq!(nivel, esperado);
            assert_eq!(
                resolver_raciocinio("codex", antigo, |_| None).unwrap()["nivel"],
                esperado.chave()
            );
        }
        assert_eq!(NivelRaciocinio::TODOS.len(), 3);
    }

    #[test]
    fn disponibilidade_nao_aceita_provedor_desconhecido_nem_repete() {
        assert_eq!(
            provedores_disponiveis(Some("claude,codex,claude,invalido")),
            vec!["codex", "claude"]
        );
        assert!(provedores_disponiveis(Some("")).is_empty());
        assert_eq!(provedores_disponiveis(None).len(), 3);
    }
}
