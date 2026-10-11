use deckhand_core::{Context, Result};
use windows::Data::Xml::Dom::XmlDocument;
use windows::UI::Notifications::{NotificationSetting, ToastNotification, ToastNotificationManager};
use windows_core::HSTRING;

use crate::{app_identity::APP_USER_MODEL_ID, com::ComApartment, toast_xml::toast_xml};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastBlock {
    ForApplication,
    ForUser,
    ByGroupPolicy,
    ByManifest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToastResult {
    Sent,
    Blocked(ToastBlock),
}

impl ToastBlock {
    pub const fn describe(self) -> &'static str {
        match self {
            Self::ForApplication => "notifications from Deckhand are turned off in Settings > System > Notifications",
            Self::ForUser => "notifications are turned off in Settings > System > Notifications",
            Self::ByGroupPolicy => "notifications are turned off by group policy",
            Self::ByManifest => "Windows does not allow notifications for Deckhand's app identity",
        }
    }
}

pub(crate) fn show_toast(title: &str, message: &str) -> Result<ToastResult> {
    let _apartment = ComApartment::enter_mta()?;
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_USER_MODEL_ID))
        .context("creating the Deckhand toast notifier")?;
    let setting = notifier.Setting().context("reading whether Deckhand may show notifications")?;
    if let Some(block) = blocked_by(setting) {
        return Ok(ToastResult::Blocked(block));
    }
    let document = XmlDocument::new().context("creating the toast content")?;
    document.LoadXml(&HSTRING::from(toast_xml(title, message))).context("writing the toast content")?;
    let toast = ToastNotification::CreateToastNotification(&document).context("creating the toast")?;
    notifier.Show(&toast).context("showing the toast")?;
    Ok(ToastResult::Sent)
}

fn blocked_by(setting: NotificationSetting) -> Option<ToastBlock> {
    match setting {
        NotificationSetting::DisabledForApplication => Some(ToastBlock::ForApplication),
        NotificationSetting::DisabledForUser => Some(ToastBlock::ForUser),
        NotificationSetting::DisabledByGroupPolicy => Some(ToastBlock::ByGroupPolicy),
        NotificationSetting::DisabledByManifest => Some(ToastBlock::ByManifest),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_enabled_setting_lets_the_toast_through() {
        assert_eq!(blocked_by(NotificationSetting::Enabled), None);
        assert_eq!(blocked_by(NotificationSetting::DisabledForApplication), Some(ToastBlock::ForApplication));
        assert_eq!(blocked_by(NotificationSetting::DisabledForUser), Some(ToastBlock::ForUser));
        assert_eq!(blocked_by(NotificationSetting::DisabledByGroupPolicy), Some(ToastBlock::ByGroupPolicy));
        assert_eq!(blocked_by(NotificationSetting::DisabledByManifest), Some(ToastBlock::ByManifest));
    }

    #[test]
    fn blocks_name_where_to_turn_notifications_back_on() {
        assert!(ToastBlock::ForApplication.describe().contains("Deckhand"));
        assert!(ToastBlock::ForUser.describe().contains("Settings > System > Notifications"));
        assert_eq!(ToastBlock::ByGroupPolicy.describe(), "notifications are turned off by group policy");
    }
}
