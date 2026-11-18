use ratatui::layout::Rect;

pub(crate) const ONBOARDING_TITLE: &str = "Bem-vindo ao Pretorian";
pub(crate) const ONBOARDING_DESCRIPTION: &str = "Use Nova Pasta para começar.";
pub(crate) const ONBOARDING_PREFIX_SUFFIX: &str = " abre atalhos · ";
pub(crate) const ONBOARDING_HELP_LABEL: &str = "?";
pub(crate) const ONBOARDING_HELP_SUFFIX: &str = " mostra atalhos e configurações";

pub(crate) fn onboarding_welcome_continue_rect(area: Rect) -> Rect {
    super::widgets::continue_button_rect(area)
}
