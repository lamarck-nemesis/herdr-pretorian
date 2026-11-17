use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::App;

pub(super) struct IniciadorAba {
    comando: Command,
    binario: PathBuf,
    perfil: PathBuf,
    sessao: String,
}

fn executar_limitado(mut comando: Command, limite: Duration) -> std::io::Result<bool> {
    comando
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    crate::platform::configure_status_command(&mut comando);
    let mut processo = comando.spawn()?;
    let mut grupo = match crate::platform::StatusCommandGuard::from_std(&processo) {
        Ok(grupo) => grupo,
        Err(erro) => {
            let _ = processo.kill();
            let _ = processo.wait();
            return Err(erro);
        }
    };
    let inicio = Instant::now();
    loop {
        match processo.try_wait() {
            Ok(Some(status)) => return Ok(status.success()),
            Ok(None) if inicio.elapsed() < limite => std::thread::sleep(Duration::from_millis(100)),
            resultado => {
                grupo.terminate();
                let _ = processo.wait();
                return resultado.map(|_| false);
            }
        }
    }
}

fn ambiente_deepseek(modelo: &str, extra_env: &mut Vec<(String, String)>) -> Result<(), String> {
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
        ("ANTHROPIC_DEFAULT_HAIKU_MODEL".into(), modelo.to_owned()),
        ("ANTHROPIC_DEFAULT_SONNET_MODEL".into(), modelo.to_owned()),
        ("ANTHROPIC_DEFAULT_OPUS_MODEL".into(), modelo.to_owned()),
        ("CLAUDE_CODE_SUBAGENT_MODEL".into(), modelo.to_owned()),
    ]);
    Ok(())
}

/// Aponta o Claude Code da aba para o gateway de modelos, quando a instalação tem um.
fn ambiente_gateway(extra_env: &mut Vec<(String, String)>) {
    let Some((url, chave)) = crate::config::gateway_modelos(|nome| std::env::var(nome).ok()) else {
        return;
    };
    extra_env.extend([
        ("ANTHROPIC_BASE_URL".into(), url),
        ("ANTHROPIC_AUTH_TOKEN".into(), chave),
        ("ANTHROPIC_API_KEY".into(), String::new()),
        // Fora de um host da Anthropic o Claude Code desliga a busca de ferramentas.
        ("ENABLE_TOOL_SEARCH".into(), "true".into()),
    ]);
}

impl App {
    pub(super) fn preparar_iniciador_aba(
        &mut self,
        perfil: &Path,
    ) -> Result<Option<IniciadorAba>, String> {
        if self.state.pretorian_painel_gerenciado || !self.state.pretorian_inicio_gerenciado {
            return Ok(None);
        }
        let agente_inicial =
            std::env::var("PRETORIAN_AGENTE").map_err(|_| "Agente inicial não identificado.")?;
        let iniciou = self
            .state
            .workspaces
            .iter()
            .enumerate()
            .any(|(ws_idx, ws)| {
                ws.tabs.iter().enumerate().any(|(tab_idx, _)| {
                    self.root_pane_info(ws_idx, tab_idx).is_some_and(|pane| {
                        pane.agent.as_deref() == Some(agente_inicial.as_str())
                            && pane.cwd.as_deref().map(Path::new) == Some(perfil)
                    })
                })
            });
        if !self.state.pretorian_primeiro_agente_identificado && !iniciou {
            return Err(
                "Aguarde a identificação do primeiro agente antes de abrir outra aba.".into(),
            );
        }
        self.state.pretorian_primeiro_agente_identificado = true;
        let caminho = |nome| {
            std::env::var_os(nome)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute() && p.is_file())
                .ok_or_else(|| {
                    "Reabra o Pretorian pelo lançador atualizado para iniciar novas abas."
                        .to_string()
                })
        };
        let python = caminho("PRETORIAN_PYTHON_GERENCIADO")?;
        let helper = caminho("PRETORIAN_INICIADOR_ABA")?;
        let sessao = std::env::var("PRETORIAN_SESSAO")
            .ok()
            .filter(|s| !s.is_empty())
            .ok_or("Sessão gerenciada não identificada.")?;
        let provedor = self
            .state
            .pretorian_agente
            .as_deref()
            .ok_or("Escolha um provedor de raciocínio.")?;
        let (modelo, esforco) =
            crate::config::configuracao_modelo(provedor, self.state.pretorian_nivel, |chave| {
                std::env::var(chave).ok()
            })?;
        let binario =
            std::env::current_exe().map_err(|_| "O executável do terminal não foi localizado.")?;
        let mut comando = crate::noninteractive_process::command(python);
        comando
            .arg(helper)
            .arg("--binario")
            .arg(&binario)
            .arg("--cwd")
            .arg(perfil)
            .arg("--provedor")
            .arg(provedor)
            .arg("--modelo")
            .arg(modelo)
            .arg("--esforco")
            .arg(esforco)
            .arg("--servidor-pid")
            .arg(std::process::id().to_string())
            .current_dir(perfil);
        Ok(Some(IniciadorAba {
            comando,
            binario,
            perfil: perfil.to_owned(),
            sessao,
        }))
    }

    pub(super) fn iniciar_aba_gerenciada(
        &mut self,
        mut inicio: IniciadorAba,
        pane: &crate::api::schema::PaneInfo,
    ) {
        inicio
            .comando
            .arg("--pane")
            .arg(&pane.pane_id)
            .arg("--terminal")
            .arg(&pane.terminal_id);
        let mensagem = "Não foi possível iniciar o agente nesta aba. Confira a identidade e reabra a aba pelo Pretorian.";
        let tarefa = std::thread::Builder::new()
            .name("pretorian-iniciar-aba".into())
            .spawn(move || {
                if executar_limitado(inicio.comando, Duration::from_secs(180)).unwrap_or(false) {
                    return;
                }
                let mut aviso = crate::noninteractive_process::command(inicio.binario);
                aviso
                    .args([
                        "--session",
                        &inicio.sessao,
                        "notification",
                        "show",
                        "Pretorian",
                        "--body",
                        mensagem,
                    ])
                    .current_dir(inicio.perfil);
                let _ = executar_limitado(aviso, Duration::from_secs(3));
            });
        if tarefa.is_err() {
            self.handle_notification_show(
                "pretorian-iniciar-aba".into(),
                crate::api::schema::NotificationShowParams {
                    title: "Pretorian".into(),
                    body: Some(mensagem.into()),
                    position: None,
                    sound: Default::default(),
                },
            );
        }
    }

    pub(super) fn comando_pretorian(
        &self,
        perfil: &Path,
        extra_env: &mut Vec<(String, String)>,
        inicial: bool,
    ) -> Result<Option<Vec<String>>, String> {
        if self.state.pretorian_painel_gerenciado {
            return Ok(None);
        }
        if self.state.pretorian_inicio_gerenciado && inicial {
            let parametro = |chave: &str| {
                extra_env
                    .iter()
                    .find(|(k, _)| k == chave)
                    .map(|(_, v)| v.clone())
                    .or_else(|| std::env::var(chave).ok())
            };
            let provedor = parametro("PRETORIAN_INICIO_PROVEDOR");
            let modelo = parametro("PRETORIAN_INICIO_MODELO");
            extra_env.retain(|(k, _)| {
                k != "PRETORIAN_INICIO_PROVEDOR" && k != "PRETORIAN_INICIO_MODELO"
            });
            match provedor.as_deref() {
                Some("deepseek") => {
                    let modelo = modelo
                        .filter(|m| !m.trim().is_empty() && !m.chars().any(char::is_control))
                        .ok_or("Modelo da conversa DeepSeek não identificado.")?;
                    ambiente_deepseek(&modelo, extra_env)?;
                }
                Some("codex") => {}
                // Sem provedor explícito o agente inicial pode ser o Claude; no Codex o ambiente é inerte.
                Some("claude") | None => ambiente_gateway(extra_env),
                Some(_) => return Err("Provedor inicial inválido.".into()),
            }
            return Ok(None);
        }
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
            if let Some((url, _)) = crate::config::gateway_modelos(|nome| std::env::var(nome).ok())
            {
                argv.extend(crate::config::argumentos_gateway_codex(&url));
            }
        } else {
            extra_env.push(("CLAUDE_CODE_EFFORT_LEVEL".into(), esforco));
            if provedor == "deepseek" {
                ambiente_deepseek(&modelo, extra_env)?;
            } else {
                ambiente_gateway(extra_env);
            }
        }
        Ok(if self.state.pretorian_inicio_gerenciado {
            None
        } else {
            comando
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn painel_gerenciado_abre_shell_do_perfil_sem_comando_cru_ou_segundo_iniciador() {
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &crate::config::Config::default(),
            crate::app::AppPolicy::TEST,
            None,
            rx,
            crate::api::EventHub::default(),
        );
        app.state.pretorian_painel_gerenciado = true;
        app.state.pretorian_agente = Some("claude".into());
        let mut ambiente = vec![("PRESERVADO".into(), "valor".into())];
        for inicial in [true, false] {
            assert!(app
                .comando_pretorian(Path::new("perfil"), &mut ambiente, inicial)
                .unwrap()
                .is_none());
        }
        assert!(app
            .preparar_iniciador_aba(Path::new("perfil"))
            .unwrap()
            .is_none());
        assert_eq!(ambiente, vec![("PRESERVADO".into(), "valor".into())]);
    }

    #[tokio::test]
    async fn inicio_gerenciado_deixa_o_lancador_validar_e_iniciar_o_agente() {
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &crate::config::Config::default(),
            crate::app::AppPolicy::TEST,
            None,
            rx,
            crate::api::EventHub::default(),
        );
        app.state.pretorian_inicio_gerenciado = true;
        app.state.pretorian_agente = Some("claude".into());
        let mut ambiente = vec![("PRESERVADO".into(), "valor".into())];
        assert_eq!(
            app.comando_pretorian(Path::new("perfil"), &mut ambiente, true)
                .unwrap(),
            None
        );
        assert_eq!(ambiente, vec![("PRESERVADO".into(), "valor".into())]);
    }
}
