use std::path::Path;

use super::App;

impl App {
    pub(super) fn comando_pretorian(
        &self,
        perfil: &Path,
        extra_env: &mut Vec<(String, String)>,
    ) -> Result<Option<Vec<String>>, String> {
        let mut comando =
            crate::config::comando_agente(self.state.pretorian_agente.as_deref(), perfil)?;
        let (Some(provedor), Some(argv)) =
            (self.state.pretorian_agente.as_deref(), comando.as_mut())
        else {
            return Ok(comando);
        };
        let disponiveis = crate::config::provedores_disponiveis(
            std::env::var("PRETORIAN_PROVEDORES").ok().as_deref(),
        );
        if !disponiveis.contains(&provedor) {
            return Err("Este provedor não está disponível nesta instalação.".into());
        }
        let (modelo, esforco) =
            crate::config::configuracao_modelo(provedor, self.state.pretorian_nivel, |chave| {
                std::env::var(chave).ok()
            })?;
        argv.extend(["--model".into(), modelo.clone()]);
        if provedor == "codex" {
            argv.extend([
                "-c".into(),
                format!("model_reasoning_effort={}", toml::Value::String(esforco)),
            ]);
        } else {
            extra_env.push(("CLAUDE_CODE_EFFORT_LEVEL".into(), esforco));
            if provedor == "deepseek" {
                let chave = std::env::var("DEEPSEEK_API_KEY")
                    .ok()
                    .filter(|valor| !valor.trim().is_empty())
                    .ok_or("A chave do DeepSeek ainda não está disponível nesta instalação.")?;
                extra_env.extend([
                    (
                        "ANTHROPIC_BASE_URL".into(),
                        "https://api.deepseek.com/anthropic".into(),
                    ),
                    ("ANTHROPIC_AUTH_TOKEN".into(), chave),
                    ("ANTHROPIC_API_KEY".into(), String::new()),
                    ("ANTHROPIC_DEFAULT_HAIKU_MODEL".into(), modelo.clone()),
                    ("ANTHROPIC_DEFAULT_SONNET_MODEL".into(), modelo.clone()),
                    ("ANTHROPIC_DEFAULT_OPUS_MODEL".into(), modelo.clone()),
                    ("CLAUDE_CODE_SUBAGENT_MODEL".into(), modelo),
                ]);
            }
        }
        Ok(comando)
    }
}
