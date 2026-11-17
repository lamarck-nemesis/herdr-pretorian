//! Lembretes das análises de caso registradas pelo CLI do Pretorian.
//!
//! O banco pertence ao CLI. O terminal só chama o auxiliar indicado por
//! `PRETORIAN_LEMBRETES_PYTHON` e `PRETORIAN_LEMBRETES_HELPER` (`listar`, `lido <id>`,
//! `excluir <id>`), sempre fora da linha de desenho, e guarda a última lista recebida.

use std::ffi::OsString;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const PYTHON_ENV: &str = "PRETORIAN_LEMBRETES_PYTHON";
const HELPER_ENV: &str = "PRETORIAN_LEMBRETES_HELPER";
const INTERVALO: Duration = Duration::from_secs(60);
const MESES: [&str; 12] = [
    "jan.", "fev.", "mar.", "abr.", "mai.", "jun.", "jul.", "ago.", "set.", "out.", "nov.", "dez.",
];

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub(super) struct Lembrete {
    pub(super) id: String,
    pub(super) cliente_nome: Option<String>,
    pub(super) relatorio: Option<String>,
    pub(super) assunto: Option<String>,
    /// `AAAA-MM-DDTHH:MM`, hora local da estação.
    pub(super) verificar_em: String,
    pub(super) lido: bool,
}

impl Lembrete {
    pub(super) fn cliente(&self) -> &str {
        self.cliente_nome
            .as_deref()
            .unwrap_or("Sem cliente identificado")
    }

    fn vence_ate(&self, dia: &str) -> bool {
        self.verificar_em.get(..10).is_some_and(|data| data <= dia)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub(super) struct Lista {
    /// Hora local da estação no momento da leitura; o terminal não calcula fuso.
    pub(super) agora: String,
    pub(super) lembretes: Vec<Lembrete>,
    pub(super) total: usize,
    pub(super) nao_lidos: usize,
}

impl Lista {
    /// Não lidos com data de hoje ou anterior: são os que pedem atenção agora.
    pub(super) fn pendentes_hoje(&self) -> usize {
        let hoje = self.agora.get(..10).unwrap_or("");
        self.lembretes
            .iter()
            .filter(|l| !l.lido && l.vence_ate(hoje))
            .count()
    }
}

pub(super) fn interpretar(texto: &str) -> Result<Lista, String> {
    serde_json::from_str(texto)
        .map_err(|_| "Lembretes indisponíveis: resposta inválida do Pretorian.".into())
}

/// `16 de nov. 16:05`; texto fora do formato volta como veio.
pub(super) fn data_curta(valor: &str) -> String {
    let partes = (
        valor.get(5..7).and_then(|m| m.parse::<usize>().ok()),
        valor.get(8..10).and_then(|d| d.parse::<u8>().ok()),
        valor.get(11..16),
    );
    match partes {
        (Some(mes @ 1..=12), Some(dia), Some(hora)) => {
            format!("{dia} de {} {hora}", MESES[mes - 1])
        }
        _ => valor.to_owned(),
    }
}

fn comando() -> Option<(OsString, OsString)> {
    let ler = |chave| std::env::var_os(chave).filter(|v| !v.is_empty());
    Some((ler(PYTHON_ENV)?, ler(HELPER_ENV)?))
}

/// Fora do lançador do Pretorian não há de onde ler lembretes, e o item não aparece.
pub(super) fn disponivel() -> bool {
    comando().is_some()
}

fn executar(argumentos: &[&str]) -> Result<String, String> {
    let (python, helper) = comando().ok_or("Lembretes indisponíveis nesta sessão.")?;
    let saida = Command::new(python)
        .arg(helper)
        .args(argumentos)
        .stdin(Stdio::null())
        .output()
        .map_err(|_| "Lembretes indisponíveis: o Pretorian não respondeu.".to_owned())?;
    if saida.status.success() {
        return Ok(String::from_utf8_lossy(&saida.stdout).into_owned());
    }
    let erro = String::from_utf8_lossy(&saida.stderr);
    let erro = erro
        .trim()
        .lines()
        .last()
        .unwrap_or("")
        .chars()
        .take(160)
        .collect::<String>();
    Err(if erro.is_empty() {
        "Lembretes indisponíveis.".into()
    } else {
        erro
    })
}

struct Cache {
    lista: Option<Result<Lista, String>>,
    lida_em: Option<Instant>,
    pendentes: usize,
    envio: Sender<Result<Lista, String>>,
    recebimento: Receiver<Result<Lista, String>>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);
static ABERTURA_AUTOMATICA_FEITA: AtomicBool = AtomicBool::new(false);

fn com_cache<T>(f: impl FnOnce(&mut Cache) -> T) -> T {
    let mut guarda = CACHE.lock().unwrap_or_else(|erro| erro.into_inner());
    let cache = guarda.get_or_insert_with(|| {
        let (envio, recebimento) = channel();
        Cache {
            lista: None,
            lida_em: None,
            pendentes: 0,
            envio,
            recebimento,
        }
    });
    f(cache)
}

/// Executa a ação (se houver) e relê a lista numa thread; o resultado chega por `atualizar`.
fn disparar(cache: &mut Cache, acao: Option<(&'static str, String)>) {
    let envio = cache.envio.clone();
    cache.pendentes += 1;
    cache.lida_em = Some(Instant::now());
    std::thread::spawn(move || {
        let resultado = acao
            .map_or(Ok(String::new()), |(verbo, id)| executar(&[verbo, &id]))
            .and_then(|_| executar(&["listar"]))
            .and_then(|texto| interpretar(&texto));
        let _ = envio.send(resultado);
    });
}

/// Chamado a cada tique do terminal. Devolve verdadeiro quando a lista mudou.
pub(super) fn atualizar(agora: Instant) -> bool {
    if !disponivel() {
        return false;
    }
    com_cache(|cache| {
        let mut mudou = false;
        while let Ok(resultado) = cache.recebimento.try_recv() {
            cache.pendentes = cache.pendentes.saturating_sub(1);
            mudou |= cache.lista.as_ref() != Some(&resultado);
            cache.lista = Some(resultado);
        }
        let vencida = cache
            .lida_em
            .is_none_or(|quando| agora.duration_since(quando) >= INTERVALO);
        if cache.pendentes == 0 && vencida {
            disparar(cache, None);
        }
        mudou
    })
}

pub(super) fn recarregar() {
    if disponivel() {
        com_cache(|cache| disparar(cache, None));
    }
}

pub(super) fn marcar_lido(id: &str) {
    if disponivel() {
        com_cache(|cache| disparar(cache, Some(("lido", id.to_owned()))));
    }
}

pub(super) fn excluir(id: &str) {
    if disponivel() {
        com_cache(|cache| disparar(cache, Some(("excluir", id.to_owned()))));
    }
}

/// `None` enquanto a primeira leitura não terminou.
pub(super) fn lista() -> Option<Result<Lista, String>> {
    if !disponivel() {
        return None;
    }
    com_cache(|cache| cache.lista.clone())
}

pub(super) fn pedem_atencao() -> bool {
    matches!(lista(), Some(Ok(lista)) if lista.pendentes_hoje() > 0)
}

/// O painel abre sozinho uma vez por sessão, na primeira leitura, se houver lembrete de hoje.
pub(super) fn abrir_automaticamente() -> bool {
    match lista() {
        Some(Ok(lista)) if !ABERTURA_AUTOMATICA_FEITA.swap(true, Ordering::Relaxed) => {
            lista.pendentes_hoje() > 0
        }
        Some(_) => {
            ABERTURA_AUTOMATICA_FEITA.store(true, Ordering::Relaxed);
            false
        }
        None => false,
    }
}

/// Estado do painel aberto. A lista vem do cache; aqui só fica a navegação.
#[derive(Debug, Default)]
pub(super) struct Painel {
    pub(super) selecionado: usize,
    /// Lembrete aberto na janela de detalhe.
    pub(super) aberto: Option<Lembrete>,
}

impl Painel {
    pub(super) fn mover(&mut self, delta: isize, total: usize) {
        self.selecionado =
            (self.selecionado as isize + delta).clamp(0, total.saturating_sub(1) as isize) as usize;
    }

    pub(super) fn abrir(&mut self, lembrete: Lembrete) {
        if !lembrete.lido {
            marcar_lido(&lembrete.id);
        }
        self.aberto = Some(lembrete);
    }

    pub(super) fn excluir_aberto(&mut self) {
        if let Some(lembrete) = self.aberto.take() {
            excluir(&lembrete.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lembrete(id: &str, quando: &str, lido: bool) -> Lembrete {
        Lembrete {
            id: id.into(),
            cliente_nome: Some("José Francisco da Silva".into()),
            relatorio: Some("Analisar a qualidade de segurado.".into()),
            assunto: Some("qualidade-segurado".into()),
            verificar_em: quando.into(),
            lido,
        }
    }

    #[test]
    fn interpreta_a_resposta_do_auxiliar() {
        let texto = r#"{"agora":"2026-11-16T16:05","total":1,"nao_lidos":1,"lembretes":[
            {"id":"a1","cliente_nome":null,"relatorio":"Pedir CNIS.","assunto":"bpc","verificar_em":"2026-11-16T09:00","lido":false}]}"#;
        let lista = interpretar(texto).unwrap();
        assert_eq!(lista.lembretes[0].cliente(), "Sem cliente identificado");
        assert_eq!(lista.pendentes_hoje(), 1);
        assert!(interpretar("{}").is_err());
        assert!(interpretar("não é json").is_err());
    }

    #[test]
    fn pendentes_de_hoje_ignoram_lidos_e_datas_futuras() {
        let lista = Lista {
            agora: "2026-11-16T16:05".into(),
            lembretes: vec![
                lembrete("vencido", "2026-11-10T09:00", false),
                lembrete("hoje-mais-tarde", "2026-11-16T22:00", false),
                lembrete("lido", "2026-11-16T08:00", true),
                lembrete("futuro", "2026-11-20T09:00", false),
            ],
            total: 4,
            nao_lidos: 3,
        };
        assert_eq!(lista.pendentes_hoje(), 2);
    }

    #[test]
    fn data_curta_em_portugues() {
        assert_eq!(data_curta("2026-11-16T16:05"), "16 de nov. 16:05");
        assert_eq!(data_curta("2026-05-01T08:30"), "1 de mai. 08:30");
        assert_eq!(data_curta("amanhã"), "amanhã");
    }

    #[test]
    fn selecao_fica_dentro_da_lista() {
        let mut painel = Painel::default();
        painel.mover(5, 3);
        assert_eq!(painel.selecionado, 2);
        painel.mover(-10, 3);
        assert_eq!(painel.selecionado, 0);
        painel.mover(1, 0);
        assert_eq!(painel.selecionado, 0);
    }
}
