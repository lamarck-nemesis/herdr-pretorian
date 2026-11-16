//! Build identity helpers.

pub const BASE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Versão semântica do Pretorian (arquivo `VERSION`), independente da versão do Herdr upstream.
pub fn pretorian_version() -> &'static str {
    include_str!("../VERSION").trim()
}

/// Linha exibida por `--version`: versão do Pretorian e do Herdr de base.
pub fn version_line() -> String {
    format!("pretorian {} (herdr {})", pretorian_version(), version())
}

pub fn channel() -> &'static str {
    non_empty(option_env!("HERDR_BUILD_CHANNEL")).unwrap_or("stable")
}

pub fn build_id() -> Option<&'static str> {
    non_empty(option_env!("HERDR_BUILD_ID"))
}

pub fn version() -> String {
    match channel() {
        "stable" => BASE_VERSION.to_string(),
        channel => match build_id() {
            Some(build_id) => format!("{BASE_VERSION}-{channel}.{build_id}"),
            None => format!("{BASE_VERSION}-{channel}"),
        },
    }
}

pub fn is_preview() -> bool {
    channel() == "preview"
}

fn non_empty(value: Option<&'static str>) -> Option<&'static str> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn stable_version_defaults_to_cargo_version() {
        assert!(!super::version().is_empty());
    }

    #[test]
    fn pretorian_version_is_semver_and_shown_with_herdr_version() {
        let version = super::pretorian_version();
        let parts: Vec<&str> = version.split('-').next().unwrap().split('.').collect();
        assert_eq!(parts.len(), 3, "VERSION deve ser X.Y.Z: {version}");
        assert!(parts.iter().all(|part| part.parse::<u64>().is_ok()));
        assert_eq!(
            super::version_line(),
            format!("pretorian {version} (herdr {})", super::version())
        );
    }
}
