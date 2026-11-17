use std::io::{BufRead, BufReader, Write};
use std::process::Stdio;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::{Duration, Instant};

pub(super) struct Autorizacao {
    pub mensagem: String,
    pub url: Option<String>,
    pub codigo: Option<String>,
    eventos: mpsc::Receiver<Evento>,
    cancelar: Arc<AtomicBool>,
    concluida: bool,
    entrada: Option<mpsc::SyncSender<String>>,
}

impl std::fmt::Debug for Autorizacao {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Autorizacao").finish_non_exhaustive()
    }
}

enum Evento {
    Link(String),
    Codigo(String),
    Terminou(bool),
    Falhou,
}

impl Drop for Autorizacao {
    fn drop(&mut self) {
        self.cancelar.store(true, Ordering::Relaxed);
    }
}

fn extrair_eventos(texto: &str) -> Vec<Evento> {
    let mut eventos = Vec::new();
    let limpo = texto
        .split('\u{1b}')
        .map(|parte| {
            if parte.starts_with('[') {
                parte.find('m').map(|fim| &parte[fim + 1..]).unwrap_or("")
            } else {
                parte
            }
        })
        .collect::<String>();
    for palavra in limpo.split_whitespace() {
        let url = palavra.trim_matches(['"', '\'', '(', ')', '<', '>']);
        if [
            "https://auth.openai.com/",
            "https://claude.ai/",
            "https://claude.com/",
            "https://console.anthropic.com/",
            "https://platform.claude.com/",
        ]
        .iter()
        .any(|host| url.starts_with(host))
        {
            eventos.push(Evento::Link(url.into()));
        }
        let partes: Vec<_> = palavra.split('-').collect();
        if partes.len() == 2
            && partes.iter().all(|p| {
                (4..=5).contains(&p.len())
                    && p.bytes()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            })
        {
            eventos.push(Evento::Codigo(palavra.into()));
        }
    }
    eventos
}

fn ler_saida(leitor: impl std::io::Read + Send + 'static, envio: mpsc::Sender<Evento>) {
    let _ = crate::thread_spawn::spawn_named("pretorian-login-output", move || {
        let mut leitor = BufReader::new(leitor.take(64 * 1024));
        let mut linha = String::new();
        while leitor.read_line(&mut linha).is_ok_and(|n| n > 0) {
            for evento in extrair_eventos(&linha) {
                if envio.send(evento).is_err() {
                    return;
                }
            }
            linha.clear();
        }
    });
}

impl Autorizacao {
    pub fn iniciar(provedor: &str) -> Result<Self, String> {
        let informar = |mensagem: &str| Self {
            mensagem: mensagem.into(),
            url: None,
            codigo: None,
            eventos: mpsc::channel().1,
            cancelar: Arc::new(AtomicBool::new(false)),
            concluida: true,
            entrada: None,
        };
        if matches!(provedor, "codex" | "claude")
            && crate::config::gateway_modelos(|nome| std::env::var(nome).ok()).is_some()
        {
            return Ok(informar(
                "Esta instalação usa o gateway de modelos do escritório. Não há login do agente a autorizar.",
            ));
        }
        let mut comando = match provedor {
            "codex" => {
                let mut c = crate::noninteractive_process::command("codex");
                c.args(["login", "--device-auth"]);
                c
            }
            "claude" => {
                let mut c = crate::noninteractive_process::command("claude");
                c.args(["auth", "login", "--claudeai"]);
                c
            }
            "deepseek" => {
                let mensagem = if std::env::var("DEEPSEEK_API_KEY")
                    .is_ok_and(|chave| !chave.trim().is_empty())
                {
                    "Chave DeepSeek configurada. O serviço verificará a validade ao iniciar o agente."
                } else {
                    "A chave DeepSeek não está disponível nesta instalação. A configuração deve ser feita pelo responsável."
                };
                return Ok(informar(mensagem));
            }
            _ => return Err("Escolha um provedor antes de autorizar.".into()),
        };
        let (entrada, codigos) = mpsc::sync_channel::<String>(1);
        let aceita_codigo = provedor == "claude";
        comando
            .stdin(if aceita_codigo {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        crate::platform::configure_status_command(&mut comando);
        let (envio, eventos) = mpsc::channel();
        let cancelar = Arc::new(AtomicBool::new(false));
        let cancelamento = cancelar.clone();
        crate::thread_spawn::spawn_named("pretorian-login", move || {
            let mut processo = match comando.spawn() {
                Ok(processo) => processo,
                Err(_) => {
                    let _ = envio.send(Evento::Falhou);
                    return;
                }
            };
            let mut guard = match crate::platform::StatusCommandGuard::from_std(&processo) {
                Ok(guard) => guard,
                Err(_) => {
                    let _ = processo.kill();
                    let _ = processo.wait();
                    let _ = envio.send(Evento::Falhou);
                    return;
                }
            };
            if let Some(stdout) = processo.stdout.take() {
                ler_saida(stdout, envio.clone());
            }
            if let Some(stderr) = processo.stderr.take() {
                ler_saida(stderr, envio.clone());
            }
            let inicio = Instant::now();
            loop {
                if cancelamento.load(Ordering::Relaxed)
                    || inicio.elapsed() > Duration::from_secs(300)
                {
                    guard.terminate();
                    let _ = processo.wait();
                    let _ = envio.send(Evento::Falhou);
                    return;
                }
                if let Ok(codigo) = codigos.try_recv() {
                    let enviado = processo.stdin.take().is_some_and(|mut stdin| {
                        writeln!(stdin, "{codigo}")
                            .and_then(|()| stdin.flush())
                            .is_ok()
                    });
                    if !enviado {
                        guard.terminate();
                        let _ = processo.wait();
                        let _ = envio.send(Evento::Falhou);
                        return;
                    }
                }
                match processo.try_wait() {
                    Ok(Some(status)) => {
                        let _ = envio.send(Evento::Terminou(status.success()));
                        return;
                    }
                    Err(_) => {
                        guard.terminate();
                        let _ = processo.wait();
                        let _ = envio.send(Evento::Falhou);
                        return;
                    }
                    Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                }
            }
        })
        .map_err(|_| "Não foi possível acompanhar a autorização.".to_string())?;
        Ok(Self {
            mensagem: "Preparando o link de autorização…".into(),
            url: None,
            codigo: None,
            eventos,
            cancelar,
            concluida: false,
            entrada: aceita_codigo.then_some(entrada),
        })
    }

    pub fn aceita_codigo(&self) -> bool {
        !self.concluida && self.url.is_some() && self.entrada.is_some()
    }

    pub fn enviar_codigo(&mut self, texto: &str) {
        if !self.aceita_codigo() {
            return;
        }
        let codigo = texto.trim();
        let formato_valido = codigo.split_once('#').is_some_and(|(valor, estado)| {
            !valor.is_empty() && !estado.is_empty() && !estado.contains('#')
        });
        if !formato_valido || codigo.len() > 2048 || !codigo.bytes().all(|b| b.is_ascii_graphic()) {
            self.mensagem =
                "Copie somente o código exibido pelo navegador e tente novamente.".into();
            return;
        }
        let Some(entrada) = self.entrada.take() else {
            return;
        };
        if entrada.try_send(codigo.to_owned()).is_ok() {
            self.mensagem = "Código enviado. Aguardando confirmação do Claude…".into();
        } else {
            self.concluida = true;
            self.cancelar.store(true, Ordering::Relaxed);
            self.url = None;
            self.codigo = None;
            self.mensagem = "Não foi possível enviar o código. Feche e tente novamente.".into();
        }
    }

    pub fn atualizar(&mut self) -> bool {
        let mut mudou = false;
        while let Ok(evento) = self.eventos.try_recv() {
            if self.concluida {
                continue;
            }
            mudou = true;
            match evento {
                Evento::Link(url) => {
                    self.url = Some(url);
                    self.mensagem =
                        "Abra o link e autorize a conta. Esta janela aguarda a confirmação.".into();
                }
                Evento::Codigo(codigo) => {
                    self.codigo = Some(format!("Código de autorização: {codigo}"))
                }
                Evento::Terminou(true) => {
                    self.concluida = true;
                    self.entrada = None;
                    self.mensagem = "Conta autorizada. As próximas abas usarão essa conta.".into();
                    self.url = None;
                    self.codigo = None;
                }
                Evento::Terminou(false) | Evento::Falhou => {
                    self.concluida = true;
                    self.entrada = None;
                    self.mensagem =
                        "Autorização não concluída ou expirada. Feche e tente novamente.".into();
                    self.url = None;
                    self.codigo = None;
                }
            }
        }
        mudou
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codigo_manual_exige_link_valida_entrada_e_envia_uma_unica_vez() {
        let (_eventos, receptor) = mpsc::channel();
        let (entrada, codigos) = mpsc::sync_channel(1);
        let mut auth = Autorizacao {
            mensagem: String::new(),
            url: None,
            codigo: None,
            eventos: receptor,
            cancelar: Arc::new(AtomicBool::new(false)),
            concluida: false,
            entrada: Some(entrada),
        };
        auth.enviar_codigo("codigo#estado");
        assert!(codigos.try_recv().is_err());
        auth.url = Some("https://claude.com/oauth/authorize".into());
        for invalido in [
            "",
            "codigo\noutro",
            "codigo com espaco",
            "codigo-sem-estado",
            "#estado",
            "codigo#",
            "codigo#estado#extra",
            "\u{1b}codigo",
            &"x".repeat(2049),
        ] {
            auth.enviar_codigo(invalido);
            assert!(codigos.try_recv().is_err());
            assert!(auth.aceita_codigo());
        }
        auth.enviar_codigo("  codigo#estado\n");
        assert_eq!(codigos.try_recv().unwrap(), "codigo#estado");
        assert!(!auth.aceita_codigo());
        assert!(!format!("{auth:?}").contains("codigo#estado"));
        assert!(!auth.mensagem.contains("codigo#estado"));
        auth.enviar_codigo("segundo");
        assert!(codigos.try_recv().is_err());
    }

    #[test]
    fn codigo_manual_nao_e_enviado_apos_conclusao() {
        let (eventos, receptor) = mpsc::channel();
        let (entrada, codigos) = mpsc::sync_channel(1);
        let mut auth = Autorizacao {
            mensagem: String::new(),
            url: Some("https://claude.com/oauth/authorize".into()),
            codigo: None,
            eventos: receptor,
            cancelar: Arc::new(AtomicBool::new(false)),
            concluida: false,
            entrada: Some(entrada),
        };
        eventos.send(Evento::Terminou(true)).unwrap();
        auth.atualizar();
        assert!(!auth.aceita_codigo());
        auth.enviar_codigo("codigo#estado");
        assert!(codigos.try_recv().is_err());
    }

    #[test]
    fn falha_remove_link_e_codigo_e_nao_aceita_sucesso_atrasado() {
        for falha in [Evento::Terminou(false), Evento::Falhou] {
            let (envio, eventos) = mpsc::channel();
            let mut auth = Autorizacao {
                mensagem: String::new(),
                url: None,
                codigo: None,
                eventos,
                cancelar: Arc::new(AtomicBool::new(false)),
                concluida: false,
                entrada: None,
            };
            envio
                .send(Evento::Link("https://auth.openai.com/codex/device".into()))
                .unwrap();
            envio.send(Evento::Codigo("ABCD-EFGHI".into())).unwrap();
            assert!(auth.atualizar());
            assert!(auth.url.is_some());
            assert!(auth.codigo.is_some());
            envio.send(falha).unwrap();
            envio.send(Evento::Terminou(true)).unwrap();
            assert!(auth.atualizar());
            assert!(auth.url.is_none());
            assert!(auth.codigo.is_none());
            assert!(auth.mensagem.contains("não concluída"));
            assert!(!auth.atualizar());
        }
    }

    #[test]
    fn conclusao_nao_reabre_link_atrasado_e_fechar_sinaliza_cancelamento() {
        let (envio, eventos) = mpsc::channel();
        let cancelar = Arc::new(AtomicBool::new(false));
        let mut auth = Autorizacao {
            mensagem: String::new(),
            url: None,
            codigo: None,
            eventos,
            cancelar: cancelar.clone(),
            concluida: false,
            entrada: None,
        };
        envio.send(Evento::Terminou(true)).unwrap();
        envio
            .send(Evento::Link("https://claude.ai/oauth/authorize".into()))
            .unwrap();
        assert!(auth.atualizar());
        assert!(auth.url.is_none());
        assert!(auth.mensagem.contains("Conta autorizada"));
        drop(auth);
        assert!(cancelar.load(Ordering::Relaxed));
    }

    #[test]
    fn aceita_somente_links_dos_provedores_e_codigo_do_dispositivo() {
        let eventos = extrair_eventos("https://auth.openai.com/codex/device ABCD-EFGHI https://auth.openai.com.outro.test/ segredo");
        assert_eq!(eventos.len(), 2);
        assert!(
            matches!(&eventos[0], Evento::Link(link) if link == "https://auth.openai.com/codex/device")
        );
        assert!(matches!(&eventos[1], Evento::Codigo(codigo) if codigo == "ABCD-EFGHI"));
        assert!(extrair_eventos("token=secreto http://claude.ai/ https://outro.test/").is_empty());
        let eventos = extrair_eventos(
            "https://claude.com/oauth/authorize?state=exemplo https://claude.com.outro.test/oauth",
        );
        assert_eq!(eventos.len(), 1);
        assert!(
            matches!(&eventos[0], Evento::Link(link) if link.starts_with("https://claude.com/oauth/"))
        );
    }
}
