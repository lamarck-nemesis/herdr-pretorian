use std::io::{BufRead, BufReader};
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
                let (_envio, eventos) = mpsc::channel();
                let mensagem = if std::env::var("DEEPSEEK_API_KEY")
                    .is_ok_and(|chave| !chave.trim().is_empty())
                {
                    "Chave DeepSeek configurada. O serviço verificará a validade ao iniciar o agente."
                } else {
                    "A chave DeepSeek não está disponível nesta instalação. A configuração deve ser feita pelo responsável."
                };
                return Ok(Self {
                    mensagem: mensagem.into(),
                    url: None,
                    codigo: None,
                    eventos,
                    cancelar: Arc::new(AtomicBool::new(false)),
                    concluida: true,
                });
            }
            _ => return Err("Escolha um provedor antes de autorizar.".into()),
        };
        comando
            .stdin(Stdio::null())
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
        })
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
                    self.mensagem = "Conta autorizada. As próximas abas usarão essa conta.".into();
                    self.url = None;
                    self.codigo = None;
                }
                Evento::Terminou(false) | Evento::Falhou => {
                    self.concluida = true;
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
