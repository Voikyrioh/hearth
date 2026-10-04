//! Conversions entre les types du domaine et leur forme en base, et erreurs de stockage.

use time::OffsetDateTime;
use time::UtcOffset;
use time::format_description::well_known::Rfc3339;

use crate::application::ports::StoreError;

/// Date en texte RFC 3339, en UTC.
pub(super) fn format_date(date: OffsetDateTime) -> String {
    // Le formatage RFC 3339 d'une date valide ne peut échouer que hors de l'intervalle d'années
    // représentable ; on retombe alors sur l'époque plutôt que de paniquer.
    date.to_offset(UtcOffset::UTC)
        .format(&Rfc3339)
        .unwrap_or_else(|_| String::from("1970-01-01T00:00:00Z"))
}

pub(super) fn parse_date(
    resource: &'static str,
    value: &str,
) -> Result<OffsetDateTime, StoreError> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(|_| StoreError::Unavailable {
        resource,
        message: format!("date illisible « {value} »"),
    })
}

/// Erreur SQLx rendue en erreur de stockage qui nomme la ressource. Les valeurs liées aux
/// requêtes ne figurent jamais dans le message de SQLx.
pub(super) fn storage(resource: &'static str) -> impl Fn(sqlx::Error) -> StoreError {
    move |error| StoreError::Unavailable {
        resource,
        message: error.to_string(),
    }
}

pub(super) fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|database| database.is_unique_violation())
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;

    #[test]
    fn dates_round_trip_in_utc() {
        let date = (OffsetDateTime::UNIX_EPOCH + Duration::seconds(1_790_000_000))
            .to_offset(UtcOffset::from_hms(2, 0, 0).unwrap());
        let text = format_date(date);
        assert!(text.ends_with('Z'), "{text}");
        assert_eq!(parse_date("accounts", &text).unwrap(), date);
    }

    #[test]
    fn an_unreadable_date_names_the_resource() {
        let error = parse_date("accounts", "hier").unwrap_err();
        assert!(error.to_string().contains("accounts"));
    }
}
