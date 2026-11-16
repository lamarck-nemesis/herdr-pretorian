use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{write::ConfigEdit, Config, NivelRaciocinio};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Preferences {
    pretorian: Selection,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    agente: String,
    nivel: NivelRaciocinio,
}

pub(super) fn path() -> Option<PathBuf> {
    if let Some(path) =
        std::env::var_os("PRETORIAN_RACIOCINIO_ARQUIVO").filter(|value| !value.is_empty())
    {
        return Some(PathBuf::from(path));
    }
    std::env::var_os("PRETORIAN_PASTA_PERFIL")
        .filter(|value| !value.is_empty())
        .map(|profile| PathBuf::from(profile).join(".pretorian-herdr/raciocinio.toml"))
}

fn parse(content: &str) -> Result<Selection, String> {
    let preferences: Preferences = toml::from_str(content)
        .map_err(|_| "Preferência de raciocínio inválida; revise provedor e nível.".to_string())?;
    if !matches!(
        preferences.pretorian.agente.as_str(),
        "codex" | "claude" | "deepseek"
    ) {
        return Err("Provedor da preferência de raciocínio não suportado.".into());
    }
    Ok(preferences.pretorian)
}

fn check_file(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("O caminho da preferência de raciocínio deve ser absoluto.".into());
    }
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => {
            Err("A preferência de raciocínio deve ser um arquivo regular.".into())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Não foi possível verificar a preferência de raciocínio.".into()),
    }
}

fn overlay_at(config: &mut Config, path: &Path) -> Result<(), String> {
    check_file(path)?;
    let Some(content) = super::io::read_optional_config(path)
        .map_err(|_| "Não foi possível ler a preferência de raciocínio.".to_string())?
    else {
        return Ok(());
    };
    let selection = parse(&content)?;
    config.pretorian.agente = Some(selection.agente);
    config.pretorian.nivel = selection.nivel;
    Ok(())
}

pub(super) fn overlay(config: &mut Config) -> Result<(), String> {
    if let Some(path) = path() {
        overlay_at(config, &path)?;
    }
    Ok(())
}

pub(super) fn save(path: &Path, config: &Config, edit: ConfigEdit<'_>) -> Result<(), String> {
    use std::io::Write;

    let parent = path
        .parent()
        .ok_or("Pasta da preferência não identificada.")?;
    std::fs::create_dir_all(parent)
        .map_err(|_| "Não foi possível criar a pasta da preferência.")?;
    let lock_path = path.with_extension("lock");
    check_file(&lock_path)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|_| "Não foi possível proteger a gravação da preferência.")?;
    lock.try_lock()
        .map_err(|_| "Preferência em uso por outra janela; tente salvar novamente.")?;
    check_file(path)?;
    let content = super::io::read_optional_config(path)
        .map_err(|_| "Não foi possível ler a preferência de raciocínio.")?;
    let initial;
    let content = if let Some(content) = &content {
        parse(content)?;
        content
    } else {
        let provider = config
            .pretorian
            .agente
            .clone()
            .or_else(|| std::env::var("PRETORIAN_AGENTE").ok())
            .unwrap_or_else(|| "codex".into());
        initial = ConfigEdit::Nivel(config.pretorian.nivel)
            .apply(&ConfigEdit::Agente(&provider).apply(""));
        &initial
    };
    let updated = edit.apply(content);
    parse(&updated)?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "Não foi possível preparar a gravação da preferência.")?
        .as_nanos();
    let temporary = path.with_extension(format!("{}-{nonce}.tmp", std::process::id()));
    let mut file = crate::platform::create_private_state_file(&temporary)
        .map_err(|_| "Não foi possível preparar a gravação da preferência.")?;
    let result = file
        .write_all(updated.as_bytes())
        .and_then(|()| file.sync_all());
    drop(file);
    let result = result.and_then(|()| crate::platform::replace_file(&temporary, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(|_| "Não foi possível salvar a preferência de raciocínio.".to_string())?;
    crate::platform::sync_parent_directory(parent).map_err(|_| {
        "Preferência gravada; não foi possível confirmar a sincronização em disco.".to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferencias_rejeitam_comandos_campos_desconhecidos_e_niveis_invalidos() {
        for content in [
            "",
            "[pretorian]\nagente='outro'\nnivel='mestre'",
            "[pretorian]\nagente='codex'\nnivel='max'",
            "[pretorian]\nagente='claude'\nnivel='mestre'\nnome='Ana'",
            "[pretorian]\nagente='claude'\nnivel='mestre'\n[terminal]\ncommand='sh'",
        ] {
            assert!(parse(content).is_err(), "{content}");
        }
    }

    #[test]
    fn preferencias_compartilhadas_preservam_configuracao_da_conversa() {
        let directory =
            std::env::temp_dir().join(format!("pretorian-preferences-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("raciocinio.toml");
        let original = "[pretorian]\nagente='claude'\nnome='Ana'\nnivel='especialista'\n[terminal]\nnew_cwd='current'\n";
        let execution = directory.join("config-claude.toml");
        std::fs::write(&execution, original).unwrap();
        let mut config: Config = toml::from_str(original).unwrap();
        overlay_at(&mut config, &path).unwrap();
        assert_eq!(config.pretorian.agente.as_deref(), Some("claude"));
        save(&path, &config, ConfigEdit::Nivel(NivelRaciocinio::Doutor)).unwrap();
        save(&path, &config, ConfigEdit::Agente("deepseek")).unwrap();
        overlay_at(&mut config, &path).unwrap();
        assert_eq!(config.pretorian.agente.as_deref(), Some("deepseek"));
        assert_eq!(config.pretorian.nivel, NivelRaciocinio::Doutor);
        assert_eq!(config.pretorian.nome.as_deref(), Some("Ana"));
        assert_eq!(std::fs::read_to_string(&execution).unwrap(), original);
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path.with_extension("lock"))
            .unwrap();
        lock.lock().unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(save(&path, &config, ConfigEdit::Agente("codex")).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        drop(lock);
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::write(&temporary, "arquivo existente").unwrap();
        save(&path, &config, ConfigEdit::Nivel(NivelRaciocinio::Doutor)).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(
            std::fs::read_to_string(&temporary).unwrap(),
            "arquivo existente"
        );
        std::fs::remove_file(temporary).unwrap();
        save(&path, &config, ConfigEdit::Nivel(NivelRaciocinio::Mestre)).unwrap();
        let saved = parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved.agente, "deepseek");
        assert_eq!(saved.nivel, NivelRaciocinio::Mestre);
        std::fs::write(&path, "invalid").unwrap();
        assert!(overlay_at(&mut config, &path).is_err());
        assert!(save(&path, &config, ConfigEdit::Agente("codex")).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "invalid");
        std::fs::remove_dir_all(directory).unwrap();
    }
}
