//! Conversions entre les structures applicatives et les types du fil de `hearth-proto`.
//! C'est ici, et seulement dans `entrypoint/http`, que le contrat JSON est connu.

use hearth_proto::api::accounts::{AccountInfo, AccountItem, RoleName};
use hearth_proto::api::audit::{AuditEventItem, AuditOrigin, OriginKindName, OutcomeName};
use hearth_proto::api::sessions::SessionsClosedNotice;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::error::ApiError;
use crate::application::accounts::{AccountSummary, AccountView};
use crate::domain::accounts::Role;
use crate::domain::audit::{AuditRecord, OriginKind, OutcomeKind};
use crate::domain::sessions::ClosureNotice;

pub fn role_to_wire(role: Role) -> RoleName {
    match role {
        Role::Admin => RoleName::Admin,
        Role::ReadOnly => RoleName::Readonly,
    }
}

/// Le rôle demandé par un client. Un rôle que cette version ne connaît pas (client plus récent) est un
/// refus de validation typé, jamais un rôle par défaut (FIX:01M4VR6Y4Q4AVQVT9FCME7Z2SB).
pub fn role_from_wire(role: RoleName) -> Result<Role, ApiError> {
    match role {
        RoleName::Admin => Ok(Role::Admin),
        RoleName::Readonly => Ok(Role::ReadOnly),
        // FIX:01M4VR6Y4Q4AVQVT9FCME7Z2SB
        RoleName::Unknown => Err(ApiError::invalid("role", "Rôle inconnu de cet agent")),
    }
}

/// Date RFC 3339 en UTC (`2026-10-04T10:30:15.25Z`).
pub fn date(date: OffsetDateTime) -> Result<String, ApiError> {
    date.to_offset(time::UtcOffset::UTC)
        .format(&Rfc3339)
        .map_err(|error| ApiError::internal(&error))
}

/// L'avis de fermeture des sessions (BR-ACCT-019) : l'identifiant de l'administrateur et la date.
pub fn sessions_closed_notice(notice: &ClosureNotice) -> Result<SessionsClosedNotice, ApiError> {
    Ok(SessionsClosedNotice {
        by: notice.by.as_str().to_owned(),
        at: date(notice.at)?,
    })
}

pub fn account_info(account: &AccountView) -> AccountInfo {
    AccountInfo {
        id: account.id.to_string(),
        username: account.username.as_str().to_owned(),
        role: role_to_wire(account.role),
        rights: account.services_right.wire_rights(),
    }
}

pub fn account_item(account: &AccountView, sessions_open: usize) -> Result<AccountItem, ApiError> {
    Ok(AccountItem {
        id: account.id.to_string(),
        username: account.username.as_str().to_owned(),
        role: role_to_wire(account.role),
        created_at: date(account.created_at)?,
        last_login_at: account.last_login_at.map(date).transpose()?,
        sessions_open: u64::try_from(sessions_open).unwrap_or(u64::MAX),
        rights: account.services_right.wire_rights(),
    })
}

pub fn audit_item(record: &AuditRecord) -> Result<AuditEventItem, ApiError> {
    Ok(AuditEventItem {
        id: record.id,
        at: date(record.at)?,
        account: record.account.clone(),
        origin: AuditOrigin {
            kind: match record.origin_kind {
                OriginKind::Client => OriginKindName::Client,
                OriginKind::CommandLine => OriginKindName::Cli,
                OriginKind::Assistant => OriginKindName::Assistant,
                OriginKind::System => OriginKindName::System,
            },
            name: record.origin_name.clone(),
            addr: record.origin_addr.clone(),
            text: record.origin_text(),
        },
        action: record.action.clone(),
        action_label: record.action_label.clone(),
        target: record.target.clone(),
        outcome: match record.outcome {
            OutcomeKind::Ok => OutcomeName::Ok,
            OutcomeKind::Denied => OutcomeName::Denied,
            OutcomeKind::Failed => OutcomeName::Failed,
        },
        reason: record.reason.clone(),
        repeat_count: record.repeat_count,
    })
}

pub fn summary_item(summary: &AccountSummary) -> Result<AccountItem, ApiError> {
    account_item(&summary.account, summary.sessions_open)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;

    #[test]
    fn a_role_this_version_does_not_know_is_a_typed_validation_refusal_never_a_default() {
        let Err(ApiError(body)) = role_from_wire(RoleName::Unknown) else {
            panic!("un rôle inconnu doit être refusé");
        };
        assert_eq!(
            body.error.code,
            hearth_proto::error::ErrorCode::ValidationError
        );
        assert_eq!(body.error.details["field"], "role");
    }

    #[test]
    fn roles_convert_both_ways() {
        for role in [Role::Admin, Role::ReadOnly] {
            assert_eq!(role_from_wire(role_to_wire(role)).ok(), Some(role));
        }
    }

    #[test]
    fn dates_are_rfc3339_in_utc() {
        let at = OffsetDateTime::UNIX_EPOCH + Duration::seconds(1_790_000_000);
        assert_eq!(date(at).unwrap(), "2026-09-21T14:13:20Z");
    }
}
