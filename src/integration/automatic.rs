use std::fs::{self, OpenOptions};
use std::io;

use super::{install_target, integration_recommendations, IntegrationRecommendation};

pub(crate) fn prepare_managed() -> io::Result<()> {
    if std::env::var("PRETORIAN_PAINEL_GERENCIADO").as_deref() != Ok("1") {
        return Ok(());
    }
    let directory = crate::config::config_dir();
    fs::create_dir_all(&directory)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("integracoes.lock"))?;
    lock.lock()?;
    let failures = prepare(integration_recommendations(), install_target);
    if failures.is_empty() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "Não foi possível preparar as integrações de {}. A próxima abertura tentará novamente.",
            failures.join(", ")
        )))
    }
}

fn prepare(
    recommendations: Vec<IntegrationRecommendation>,
    mut install: impl FnMut(crate::api::schema::IntegrationTarget) -> io::Result<Vec<String>>,
) -> Vec<&'static str> {
    let mut failures = Vec::new();
    for item in recommendations.iter().filter(|item| item.needs_install()) {
        if install(item.target).is_err() {
            tracing::warn!(
                integration = item.label,
                "Falha ao preparar integração automática"
            );
            failures.push(item.label);
        }
    }
    failures
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::schema::IntegrationTarget;
    use crate::integration::IntegrationStatusKind;

    fn recommendation(
        target: IntegrationTarget,
        available: bool,
        state: IntegrationStatusKind,
    ) -> IntegrationRecommendation {
        IntegrationRecommendation {
            target,
            label: crate::integration::integration_target_label(target),
            command: "synthetic",
            available,
            state,
            path: std::path::PathBuf::new(),
        }
    }

    #[test]
    fn automatic_integrations_skip_current_and_absent_and_continue_after_failure() {
        use IntegrationStatusKind::{Current, NotInstalled, Outdated};
        use IntegrationTarget::{Claude, Codex, Copilot, Kimi};
        let mut called = Vec::new();
        let failures = prepare(
            vec![
                recommendation(Claude, true, NotInstalled),
                recommendation(Codex, false, Outdated),
                recommendation(Kimi, true, Current),
                recommendation(Copilot, false, NotInstalled),
            ],
            |target| {
                called.push(target);
                if target == Claude {
                    Err(io::Error::other("falha sintética"))
                } else {
                    Ok(Vec::new())
                }
            },
        );
        assert_eq!(called, vec![Claude, Codex]);
        assert_eq!(failures, vec!["claude"]);
    }

    #[test]
    fn automatic_integrations_install_real_claude_preserving_preferences_and_skip_next_open() {
        let _guard = crate::integration::integration_env_lock();
        let root = std::env::temp_dir().join(format!(
            "pretorian-integracao-auto-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let previous = std::env::var_os("CLAUDE_CONFIG_DIR");
        std::env::set_var("CLAUDE_CONFIG_DIR", &root);
        let settings = root.join("settings.json");
        fs::write(
            &settings,
            r#"{"model":"synthetic-model","permissions":{"allow":["Read"]}}"#,
        )
        .unwrap();
        let only_claude = || {
            integration_recommendations()
                .into_iter()
                .filter(|item| item.target == IntegrationTarget::Claude)
                .map(|mut item| {
                    item.available = true;
                    item
                })
                .collect()
        };
        assert!(prepare(only_claude(), install_target).is_empty());
        let installed = fs::read(&settings).unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&installed).unwrap();
        assert_eq!(parsed["model"], "synthetic-model");
        assert_eq!(parsed["permissions"]["allow"][0], "Read");
        assert!(parsed["hooks"]
            .as_object()
            .is_some_and(|hooks| !hooks.is_empty()));
        assert!(prepare(only_claude(), |_| panic!(
            "não deve reinstalar integração atual"
        ))
        .is_empty());
        assert_eq!(fs::read(settings).unwrap(), installed);
        if let Some(value) = previous {
            std::env::set_var("CLAUDE_CONFIG_DIR", value);
        } else {
            std::env::remove_var("CLAUDE_CONFIG_DIR");
        }
        fs::remove_dir_all(root).unwrap();
    }
}
