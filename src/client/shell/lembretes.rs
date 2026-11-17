//! Lembretes das análises de caso registradas pelo CLI do Pretorian.
//!
//! O banco pertence ao CLI. O terminal só chama o auxiliar indicado por
//! `PRETORIAN_LEMBRETES_PYTHON` e `PRETORIAN_LEMBRETES_HELPER` (`listar`, `lido <id>`,
//! `excluir <id>`, `adicionar --quando … --texto … [--cliente …]`), sempre fora da linha
//! de desenho, e guarda a última lista recebida.

use std::ffi::{OsStr, OsString};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::TextEditor;

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
    /// Hora local da estação no momento da leitura. O terminal não calcula fuso: no Unix,
    /// o fuso local não pode ser lido com segurança num processo com várias threads.
    pub(super) agora: String,
    #[serde(skip)]
    pub(super) recebida_em: Option<Instant>,
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

    /// Hora da leitura somada ao tempo decorrido desde ela, para o relógio não parar entre leituras.
    pub(super) fn agora_atual(&self, agora: Instant) -> String {
        let decorrido = self.recebida_em.map_or(0, |quando| {
            agora.saturating_duration_since(quando).as_secs()
        });
        somar_segundos(&self.agora, decorrido).unwrap_or_else(|| self.agora.clone())
    }
}

pub(super) fn interpretar(texto: &str) -> Result<Lista, String> {
    let mut lista: Lista = serde_json::from_str(texto)
        .map_err(|_| "Lembretes indisponíveis: resposta inválida do Pretorian.".to_owned())?;
    lista.recebida_em = Some(Instant::now());
    Ok(lista)
}

fn montar(ano: i32, mes: u8, dia: u8, hora: u8, minuto: u8) -> Option<time::PrimitiveDateTime> {
    let data = time::Date::from_calendar_date(ano, time::Month::try_from(mes).ok()?, dia).ok()?;
    let hora = time::Time::from_hms(hora, minuto, 0).ok()?;
    Some(time::PrimitiveDateTime::new(data, hora))
}

fn iso(valor: time::PrimitiveDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}",
        valor.year(),
        u8::from(valor.month()),
        valor.day(),
        valor.hour(),
        valor.minute()
    )
}

/// Lê `AAAA-MM-DDTHH:MM`.
fn ler_iso(valor: &str) -> Option<time::PrimitiveDateTime> {
    let (data, hora) = valor.split_once('T')?;
    let mut data = data.splitn(3, '-');
    let (h, m) = hora.split_once(':')?;
    montar(
        data.next()?.parse().ok()?,
        data.next()?.parse().ok()?,
        data.next()?.parse().ok()?,
        h.parse().ok()?,
        m.parse().ok()?,
    )
}

fn somar_segundos(valor: &str, segundos: u64) -> Option<String> {
    let segundos = i64::try_from(segundos).ok()?;
    Some(iso(ler_iso(valor)? + time::Duration::seconds(segundos)))
}

/// `DD/MM/AAAA HH:MM`, como a pessoa digita, para `AAAA-MM-DDTHH:MM`; `None` se a data não existe.
pub(super) fn ler_data_digitada(texto: &str) -> Option<String> {
    let (data, hora) = texto.trim().split_once(' ')?;
    let mut data = data.split('/');
    let (dia, mes, ano) = (data.next()?, data.next()?, data.next()?);
    let (h, m) = hora.trim().split_once(':')?;
    if data.next().is_some() || ano.len() != 4 {
        return None;
    }
    let valor = montar(
        ano.parse().ok()?,
        mes.parse().ok()?,
        dia.parse().ok()?,
        h.parse().ok()?,
        m.parse().ok()?,
    )?;
    Some(iso(valor))
}

/// `AAAA-MM-DDTHH:MM` para `DD/MM/AAAA HH:MM`, que é como o formulário mostra a data.
pub(super) fn data_para_digitar(valor: &str) -> String {
    match (
        valor.get(0..4),
        valor.get(5..7),
        valor.get(8..10),
        valor.get(11..16),
    ) {
        (Some(ano), Some(mes), Some(dia), Some(hora)) => format!("{dia}/{mes}/{ano} {hora}"),
        _ => String::new(),
    }
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

fn executar<S: AsRef<OsStr>>(argumentos: &[S]) -> Result<String, String> {
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
fn disparar(acao: Vec<String>) {
    if !disponivel() {
        return;
    }
    com_cache(|cache| {
        let envio = cache.envio.clone();
        cache.pendentes += 1;
        cache.lida_em = Some(Instant::now());
        std::thread::spawn(move || {
            let resultado = if acao.is_empty() {
                Ok(String::new())
            } else {
                executar(&acao)
            }
            .and_then(|_| executar(&["listar"]))
            .and_then(|texto| interpretar(&texto));
            let _ = envio.send(resultado);
        });
    });
}

/// Chamado a cada tique do terminal. Devolve verdadeiro quando a lista mudou.
pub(super) fn atualizar(agora: Instant) -> bool {
    if !disponivel() {
        return false;
    }
    let (mudou, vencida) = com_cache(|cache| {
        let mut mudou = false;
        while let Ok(resultado) = cache.recebimento.try_recv() {
            cache.pendentes = cache.pendentes.saturating_sub(1);
            mudou = true;
            cache.lista = Some(resultado);
        }
        let vencida = cache
            .lida_em
            .is_none_or(|quando| agora.duration_since(quando) >= INTERVALO);
        (mudou, cache.pendentes == 0 && vencida)
    });
    if vencida {
        disparar(Vec::new());
    }
    mudou
}

pub(super) fn recarregar() {
    disparar(Vec::new());
}

fn marcar_lido(id: &str) {
    disparar(vec!["lido".into(), id.into()]);
}

fn excluir(id: &str) {
    disparar(vec!["excluir".into(), id.into()]);
}

fn adicionar(quando: &str, texto: &str, cliente: &str) {
    let mut acao: Vec<String> = vec![
        "adicionar".into(),
        "--quando".into(),
        quando.into(),
        "--texto".into(),
        texto.into(),
    ];
    if !cliente.is_empty() {
        acao.extend(["--cliente".into(), cliente.into()]);
    }
    disparar(acao);
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

pub(super) const CAMPOS: [&str; 3] = [
    "Cliente (opcional)",
    "Lembrete",
    "Data e hora (DD/MM/AAAA HH:MM)",
];
const LIMITES: [usize; 3] = [120, 300, 16];

/// Formulário de lembrete novo. Quem grava é o CLI, como nos demais lembretes.
#[derive(Debug, Default)]
pub(super) struct Formulario {
    pub(super) campos: [TextEditor; 3],
    pub(super) foco: usize,
    pub(super) erro: Option<&'static str>,
}

impl Formulario {
    pub(super) fn novo(agora: &str) -> Self {
        let mut formulario = Self::default();
        formulario.campos[2] = TextEditor::new(&data_para_digitar(agora), false);
        formulario
    }

    pub(super) fn focar(&mut self, delta: isize) {
        self.foco = (self.foco as isize + delta).rem_euclid(CAMPOS.len() as isize) as usize;
    }

    pub(super) fn inserir(&mut self, texto: &str) {
        let campo = &mut self.campos[self.foco];
        if campo.chars().count() + texto.chars().count() <= LIMITES[self.foco] {
            campo.insert(texto);
        }
    }

    /// Confere e devolve `(quando, texto, cliente)`; em erro, guarda a mensagem e foca o campo.
    pub(super) fn conferir(&mut self) -> Option<(String, String, String)> {
        let texto = self.campos[1].trim().to_owned();
        let cliente = self.campos[0].trim().to_owned();
        let (foco, erro) = match ler_data_digitada(&self.campos[2]) {
            _ if texto.is_empty() => (1, "Escreva o lembrete."),
            None => (2, "Data inválida: use DD/MM/AAAA HH:MM."),
            Some(quando) => return Some((quando, texto, cliente)),
        };
        self.foco = foco;
        self.erro = Some(erro);
        None
    }
}

/// Estado do painel aberto. A lista vem do cache; aqui ficam a navegação e as janelas.
#[derive(Debug, Default)]
pub(super) struct Painel {
    pub(super) selecionado: usize,
    /// Lembrete aberto na janela de detalhe.
    pub(super) aberto: Option<Lembrete>,
    pub(super) novo: Option<Formulario>,
    /// Minuto mostrado no relógio do cabeçalho; quando muda, o painel é redesenhado.
    pub(super) relogio: String,
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

    pub(super) fn abrir_formulario(&mut self) {
        let agora = match lista() {
            Some(Ok(lista)) => lista.agora_atual(Instant::now()),
            _ => String::new(),
        };
        self.aberto = None;
        self.novo = Some(Formulario::novo(&agora));
    }

    pub(super) fn salvar_formulario(&mut self) {
        if let Some((quando, texto, cliente)) = self.novo.as_mut().and_then(Formulario::conferir) {
            adicionar(&quando, &texto, &cliente);
            self.novo = None;
        }
    }

    /// Verdadeiro quando o minuto do relógio mudou desde o último desenho.
    pub(super) fn relogio_mudou(&mut self, agora: Instant) -> bool {
        let Some(Ok(lista)) = lista() else {
            return false;
        };
        let minuto = lista.agora_atual(agora);
        if minuto == self.relogio {
            return false;
        }
        self.relogio = minuto;
        true
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
        assert!(lista.recebida_em.is_some());
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
            ..Lista::default()
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
    fn relogio_avanca_com_o_tempo_decorrido_e_vira_o_dia() {
        let recebida = Instant::now();
        let lista = Lista {
            agora: "2026-11-30T23:59".into(),
            recebida_em: Some(recebida),
            ..Lista::default()
        };
        assert_eq!(lista.agora_atual(recebida), "2026-11-30T23:59");
        assert_eq!(
            lista.agora_atual(recebida + Duration::from_secs(90)),
            "2026-12-01T00:00"
        );
        assert_eq!(somar_segundos("amanhã", 60), None);
    }

    #[test]
    fn data_digitada_vira_iso_e_data_inexistente_e_recusada() {
        assert_eq!(
            ler_data_digitada("20/11/2026 09:00").as_deref(),
            Some("2026-11-20T09:00")
        );
        assert_eq!(
            ler_data_digitada(" 5/1/2027 8:05 ").as_deref(),
            Some("2027-01-05T08:05")
        );
        for ruim in [
            "31/02/2026 09:00",
            "20/11/26 09:00",
            "20/11/2026",
            "20/11/2026 25:00",
            "amanhã",
        ] {
            assert_eq!(ler_data_digitada(ruim), None, "{ruim}");
        }
        assert_eq!(data_para_digitar("2026-11-17T05:10"), "17/11/2026 05:10");
    }

    #[test]
    fn formulario_exige_texto_e_data_valida() {
        let mut formulario = Formulario::novo("2026-11-17T05:10");
        assert_eq!(formulario.conferir(), None);
        assert_eq!(
            (formulario.foco, formulario.erro),
            (1, Some("Escreva o lembrete."))
        );
        formulario.inserir("Ligar para o cliente");
        assert_eq!(
            formulario.conferir(),
            Some((
                "2026-11-17T05:10".into(),
                "Ligar para o cliente".into(),
                String::new()
            ))
        );
        formulario.campos[2].clear();
        formulario.foco = 2;
        formulario.inserir("31/02/2026 09:00");
        assert_eq!(formulario.conferir(), None);
        assert_eq!(formulario.foco, 2);
        formulario.foco = 1;
        formulario.inserir(&"x".repeat(400));
        assert_eq!(formulario.campos[1].chars().count(), 20);
        formulario.focar(-2);
        assert_eq!(formulario.foco, 2);
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
