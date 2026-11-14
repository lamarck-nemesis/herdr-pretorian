use std::io::{self, Write};

const BELL_CHUNK: [u8; 64] = [b'\x07'; 64];

pub(crate) fn write_terminal_bells<W: Write>(writer: &mut W, count: u16) -> io::Result<()> {
    let full_chunks = usize::from(count) / BELL_CHUNK.len();
    let remainder = usize::from(count) % BELL_CHUNK.len();
    for _ in 0..full_chunks {
        writer.write_all(&BELL_CHUNK)?;
    }
    writer.write_all(&BELL_CHUNK[..remainder])?;
    writer.flush()
}

/// No Terminal da Apple, o ícone e a marca da barra de título vêm do arquivo que a sessão
/// representa (OSC 6). A instalação cria esse arquivo com o ícone do Pretorian; sem ele, ou em
/// outro terminal, a marca vai no próprio título.
fn brand_document_sequence() -> Option<&'static str> {
    static SEQUENCE: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    SEQUENCE
        .get_or_init(|| {
            if std::env::var_os("TERM_PROGRAM")? != "Apple_Terminal" {
                return None;
            }
            let path = std::path::Path::new(&std::env::var_os("HOME")?)
                .join("Library/Application Support/pretorian")
                .join(crate::config::MARCA);
            path.is_file().then(|| {
                let host = crate::platform::hostname().unwrap_or_default();
                format!(
                    "\x1b]6;file://{host}{}\x07",
                    percent_encode_path(&path.to_string_lossy())
                )
            })
        })
        .as_deref()
}

fn percent_encode_path(path: &str) -> String {
    use std::fmt::Write as _;
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

pub(crate) fn write_window_brand<W: Write>(writer: &mut W) -> io::Result<()> {
    let Some(sequence) = brand_document_sequence() else {
        return Ok(());
    };
    writer.write_all(sequence.as_bytes())?;
    writer.flush()
}

pub(crate) fn write_window_brand_reset<W: Write>(writer: &mut W) -> io::Result<()> {
    if brand_document_sequence().is_none() {
        return Ok(());
    }
    writer.write_all(b"\x1b]6;\x07")?;
    writer.flush()
}

/// Com a marca no arquivo representado, o título não a repete.
fn title_without_brand(title: &str) -> &str {
    let rest = title.strip_prefix(crate::config::MARCA).unwrap_or(title);
    rest.strip_prefix(crate::config::SEPARADOR).unwrap_or(rest)
}

pub(crate) fn write_window_title<W: Write>(writer: &mut W, title: Option<&str>) -> io::Result<()> {
    let title = title.unwrap_or(crate::config::MARCA);
    let title = if brand_document_sequence().is_some() {
        title_without_brand(title)
    } else {
        title
    };
    let safe_title = title
        .chars()
        .filter(|ch| !matches!(*ch, '\u{1b}' | '\u{7}' | '\u{9c}'))
        .collect::<String>();
    write!(writer, "\x1b]0;{safe_title}\x07")?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_exact_terminal_bell_count() {
        let mut output = Vec::new();

        write_terminal_bells(&mut output, 130).unwrap();

        assert_eq!(output, vec![b'\x07'; 130]);
    }

    #[test]
    fn window_title_strips_terminators_and_defaults_to_pretorian() {
        let mut output = Vec::new();
        write_window_title(&mut output, Some("herdr\x1b api\u{7}\u{9c}")).unwrap();
        assert_eq!(output, b"\x1b]0;herdr api\x07");

        output.clear();
        write_window_title(&mut output, None).unwrap();
        assert_eq!(output, "\x1b]0;LSΛ prεtοrιαn\x07".as_bytes());
    }

    #[test]
    fn marca_no_arquivo_representado_sai_do_titulo_e_o_caminho_e_codificado() {
        assert_eq!(title_without_brand("LSΛ prεtοrιαn │ Ana"), "Ana");
        assert_eq!(title_without_brand("LSΛ prεtοrιαn"), "");
        assert_eq!(title_without_brand("outro título"), "outro título");
        assert_eq!(
            percent_encode_path("/Users/a/Application Support/LSΛ"),
            "/Users/a/Application%20Support/LS%CE%9B"
        );
    }
}
