use super::*;
use crate::terminal::{TerminalId, TerminalRuntime};
use crate::terminal_theme::{RgbColor, TerminalTheme};
use bytes::Bytes;
use tokio::sync::mpsc;

const DARK: RgbColor = RgbColor {
    r: 17,
    g: 17,
    b: 17,
};
const LIGHT: RgbColor = RgbColor {
    r: 255,
    g: 255,
    b: 255,
};

/// Tema que acompanha o hospedeiro; o padrão (Pretorian) fixa as cores dos painéis.
fn app() -> App {
    app_com_tema("catppuccin")
}

fn app_com_tema(nome: &str) -> App {
    let (_, api_rx) = mpsc::unbounded_channel();
    let config: crate::config::Config =
        toml::from_str(&format!("[theme]\nname = \"{nome}\"\n")).unwrap();
    App::new(
        &config,
        crate::app::AppPolicy::TEST,
        None,
        api_rx,
        crate::api::EventHub::default(),
    )
}

fn app_pretorian() -> App {
    let (_, api_rx) = mpsc::unbounded_channel();
    App::new(
        &crate::config::Config::default(),
        crate::app::AppPolicy::TEST,
        None,
        api_rx,
        crate::api::EventHub::default(),
    )
}

fn child(app: &mut App) -> (TerminalId, mpsc::Receiver<Bytes>) {
    let (runtime, rx) = TerminalRuntime::test_with_channel(80, 24);
    runtime.apply_host_terminal_theme(app.state.host_terminal_theme);
    runtime.apply_host_terminal_appearance(app.pane_host_terminal_appearance());
    runtime.test_process_pty_bytes(b"\x1b[?2031h");
    let id = TerminalId::alloc();
    app.terminal_runtimes.insert(id.clone(), runtime);
    (id, rx)
}

fn background(app: &mut App, color: RgbColor) {
    app.set_host_terminal_theme(TerminalTheme {
        background: Some(color),
        ..app.state.host_terminal_theme
    });
}

fn query(app: &App, id: &TerminalId, request: &[u8]) -> Vec<Bytes> {
    app.terminal_runtimes
        .get(id)
        .unwrap()
        .test_process_pty_bytes(request)
}

#[tokio::test]
async fn appearance_waits_for_background_not_foreground_or_palette() {
    let mut app = app();
    background(&mut app, DARK);
    app.set_host_terminal_appearance_state(Some(HostAppearance::Dark), true);
    let (id, mut rx) = child(&mut app);

    app.set_host_terminal_appearance_state(Some(HostAppearance::Light), true);
    assert!(rx.try_recv().is_err(), "notification must wait for OSC 11");
    let mut theme = app.state.host_terminal_theme;
    theme.foreground = Some(DARK);
    theme.palette[1] = Some(LIGHT);
    app.set_host_terminal_theme(theme);
    assert!(
        rx.try_recv().is_err(),
        "OSC 10/palette must not release the hold"
    );
    assert_eq!(
        query(&app, &id, b"\x1b[?996n"),
        [Bytes::from_static(b"\x1b[?997;1n")]
    );
    background(&mut app, LIGHT);
    assert_eq!(rx.try_recv().unwrap(), b"\x1b[?997;2n"[..]);
    assert_eq!(
        query(&app, &id, b"\x1b]11;?\x07"),
        [Bytes::from_static(b"\x1b]11;rgb:ffff/ffff/ffff\x1b\\")]
    );
}

#[tokio::test]
async fn appearance_with_matching_background_is_immediate() {
    let mut app = app();
    background(&mut app, DARK);
    app.set_host_terminal_appearance_state(Some(HostAppearance::Dark), true);
    let (_, mut rx) = child(&mut app);
    background(&mut app, LIGHT);
    app.set_host_terminal_appearance_state(Some(HostAppearance::Light), true);
    assert_eq!(rx.try_recv().unwrap(), b"\x1b[?997;2n"[..]);
    assert!(app.pending_host_appearance_deadline.is_none());
}

#[tokio::test]
async fn appearance_deadline_bounds_missing_or_inconsistent_background() {
    for missing in [false, true] {
        let mut app = app();
        background(&mut app, DARK);
        app.set_host_terminal_appearance_state(Some(HostAppearance::Dark), true);
        let (_, mut rx) = child(&mut app);
        app.set_host_terminal_appearance_state(Some(HostAppearance::Light), true);
        if missing {
            app.set_host_terminal_theme(TerminalTheme::default());
        }
        let deadline = app.pending_host_appearance_deadline.unwrap();
        assert!(deadline <= Instant::now() + Duration::from_millis(750));
        assert!(!app.release_host_appearance_if_ready(deadline - Duration::from_nanos(1)));
        assert!(rx.try_recv().is_err());
        assert!(app.release_host_appearance_if_ready(deadline));
        assert_eq!(rx.try_recv().unwrap(), b"\x1b[?997;2n"[..]);
        assert!(!app.release_host_appearance_if_ready(deadline));
    }
}

#[tokio::test]
async fn pane_opened_without_background_during_appearance_hold_answers_and_receives_release() {
    for (outgoing, incoming) in [
        (HostAppearance::Dark, HostAppearance::Light),
        (HostAppearance::Light, HostAppearance::Dark),
    ] {
        let mut app = app();
        app.set_host_terminal_appearance_state(Some(outgoing), true);
        app.set_host_terminal_appearance_state(Some(incoming), true);
        let (id, mut rx) = child(&mut app);
        assert_eq!(
            query(&app, &id, b"\x1b[?996n"),
            [Bytes::from_static(outgoing.color_scheme_report())],
            "a pane opened during the hold must answer with the known outgoing appearance"
        );
        assert!(rx.try_recv().is_err());
        let deadline = app.pending_host_appearance_deadline.unwrap();
        assert!(app.release_host_appearance_if_ready(deadline));
        assert_eq!(rx.try_recv().unwrap(), incoming.color_scheme_report());
        assert_eq!(
            query(&app, &id, b"\x1b[?996n"),
            [Bytes::from_static(incoming.color_scheme_report())]
        );
    }
}

#[tokio::test]
async fn appearance_coalesces_without_extending_deadline() {
    let mut app = app();
    background(&mut app, DARK);
    app.set_host_terminal_appearance_state(Some(HostAppearance::Dark), true);
    let (_, mut rx) = child(&mut app);
    app.set_host_terminal_theme(TerminalTheme::default());
    app.set_host_terminal_appearance_state(Some(HostAppearance::Light), true);
    let deadline = app.pending_host_appearance_deadline;
    // A provenance-only update must not cancel a pending flip either.
    app.set_host_terminal_appearance_state(Some(HostAppearance::Light), false);
    assert_eq!(app.pending_host_appearance_deadline, deadline);
    app.set_host_terminal_appearance_state(Some(HostAppearance::Dark), true);
    app.set_host_terminal_appearance_state(Some(HostAppearance::Light), true);
    assert_eq!(app.pending_host_appearance_deadline, deadline);
    assert!(rx.try_recv().is_err());
    background(&mut app, LIGHT);
    assert_eq!(rx.try_recv().unwrap(), b"\x1b[?997;2n"[..]);
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn appearance_baseline_establish_and_clear_are_immediate() {
    let mut app = app();
    let (id, mut rx) = child(&mut app);
    app.set_host_terminal_appearance_state(Some(HostAppearance::Dark), true);
    assert_eq!(
        query(&app, &id, b"\x1b[?996n"),
        [Bytes::from_static(b"\x1b[?997;1n")]
    );
    assert!(app.pending_host_appearance_deadline.is_none());
    app.set_host_terminal_appearance_state(Some(HostAppearance::Light), true);
    app.set_host_terminal_appearance_state(None, false);
    assert!(app.pending_host_appearance_deadline.is_none());
    assert!(query(&app, &id, b"\x1b[?996n").is_empty());
    assert!(rx.try_recv().is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn pane_created_during_appearance_hold_gets_consistent_pair() {
    let mut app = app();
    app.state.default_shell = "/bin/sh".into();
    background(&mut app, DARK);
    app.set_host_terminal_appearance_state(Some(HostAppearance::Dark), true);
    app.set_host_terminal_appearance_state(Some(HostAppearance::Light), true);
    let index = app
        .create_workspace_with_options(std::env::temp_dir(), false)
        .unwrap();
    let ws = &app.state.workspaces[index];
    let id = ws.terminal_id(ws.tabs[0].root_pane).unwrap().clone();
    assert_eq!(
        query(&app, &id, b"\x1b[?996n"),
        [Bytes::from_static(b"\x1b[?997;1n")]
    );
    assert_eq!(
        query(&app, &id, b"\x1b]11;?\x07"),
        [Bytes::from_static(b"\x1b]11;rgb:1111/1111/1111\x1b\\")]
    );
    background(&mut app, LIGHT);
    assert_eq!(
        query(&app, &id, b"\x1b[?996n"),
        [Bytes::from_static(b"\x1b[?997;2n")]
    );
    assert_eq!(
        query(&app, &id, b"\x1b]11;?\x07"),
        [Bytes::from_static(b"\x1b]11;rgb:ffff/ffff/ffff\x1b\\")]
    );
    for (_, runtime) in app.terminal_runtimes.drain() {
        runtime.shutdown();
    }
}

const FUNDO_PRETORIAN: &[u8] = b"\x1b]11;rgb:ffff/ffff/ffff\x1b\\";

#[tokio::test]
async fn tema_pretorian_fixa_fundo_claro_dos_paineis_em_hospedeiro_escuro() {
    let mut app = app_pretorian();
    background(&mut app, DARK);
    app.set_host_terminal_appearance_state(Some(HostAppearance::Dark), true);
    let (runtime, mut rx) = TerminalRuntime::test_with_channel(80, 24);
    runtime.apply_host_terminal_theme(app.pane_terminal_theme());
    runtime.apply_host_terminal_appearance(app.pane_host_terminal_appearance());
    runtime.test_process_pty_bytes(b"\x1b[?2031h");
    let id = TerminalId::alloc();
    app.terminal_runtimes.insert(id.clone(), runtime);

    assert_eq!(
        query(&app, &id, b"\x1b]11;?\x07"),
        [Bytes::copy_from_slice(FUNDO_PRETORIAN)]
    );
    assert_eq!(
        query(&app, &id, b"\x1b[?996n"),
        [Bytes::from_static(b"\x1b[?997;2n")]
    );

    // Uma mudança de aparência do hospedeiro não troca o fundo fixo dos painéis.
    background(&mut app, LIGHT);
    app.set_host_terminal_appearance_state(Some(HostAppearance::Light), true);
    background(&mut app, DARK);
    app.set_host_terminal_appearance_state(Some(HostAppearance::Dark), true);
    assert_eq!(
        query(&app, &id, b"\x1b]11;?\x07"),
        [Bytes::copy_from_slice(FUNDO_PRETORIAN)]
    );
    assert_eq!(
        query(&app, &id, b"\x1b[?996n"),
        [Bytes::from_static(b"\x1b[?997;2n")]
    );
    while let Ok(report) = rx.try_recv() {
        assert_ne!(
            report,
            b"\x1b[?997;1n"[..],
            "painel não pode ser avisado de tema escuro"
        );
    }
}

#[test]
fn outros_temas_seguem_o_hospedeiro() {
    let mut app = app_com_tema("tokyo-night");
    background(&mut app, DARK);
    assert_eq!(app.pane_terminal_theme(), app.state.host_terminal_theme);
    app.set_host_terminal_appearance_state(Some(HostAppearance::Dark), true);
    assert_eq!(
        app.pane_host_terminal_appearance(),
        Some(HostAppearance::Dark)
    );
}
