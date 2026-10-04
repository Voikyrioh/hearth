//! Conversions entre les types du domaine et leur forme en base, et erreurs de stockage.

use time::OffsetDateTime;
use time::UtcOffset;
use time::format_description::well_known::Rfc3339;

use crate::application::ports::StoreError;

/// Date en texte à largeur fixe, en UTC, au millième de seconde : `2026-10-04T10:30:15.250Z`.
/// Les fractions sont toujours présentes : l'ordre du texte est l'ordre chronologique, donc
/// `ORDER BY` et les comparaisons SQL sont justes.
pub(super) fn format_date(
    resource: &'static str,
    date: OffsetDateTime,
) -> Result<String, StoreError> {
    let date = date.to_offset(UtcOffset::UTC);
    if !(0..=9999).contains(&date.year()) {
        return Err(StoreError::Unavailable {
            resource,
            message: format!(
                "date hors de l'intervalle stockable (année {})",
                date.year()
            ),
        });
    }
    Ok(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        date.year(),
        u8::from(date.month()),
        date.day(),
        date.hour(),
        date.minute(),
        date.second(),
        date.millisecond()
    ))
}

/// Relit une date écrite par `format_date` (toute date RFC 3339 valide est acceptée).
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

    fn at(seconds: i64, millis: i64) -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH + Duration::seconds(seconds) + Duration::milliseconds(millis)
    }

    #[test]
    fn dates_have_a_fixed_width_with_milliseconds_in_utc() {
        let date = at(1_790_000_000, 0).to_offset(UtcOffset::from_hms(2, 0, 0).unwrap());
        assert_eq!(
            format_date("accounts", date).unwrap(),
            "2026-09-21T14:13:20.000Z"
        );
        assert_eq!(
            format_date("accounts", at(1_790_000_000, 5)).unwrap(),
            "2026-09-21T14:13:20.005Z"
        );
        assert_eq!(
            format_date("accounts", OffsetDateTime::UNIX_EPOCH)
                .unwrap()
                .len(),
            24
        );
    }

    #[test]
    fn dates_round_trip() {
        for date in [at(1_790_000_000, 0), at(1_790_000_000, 250), at(0, 999)] {
            let text = format_date("accounts", date).unwrap();
            assert_eq!(parse_date("accounts", &text).unwrap(), date);
        }
    }

    #[test]
    fn text_order_is_chronological_with_and_without_fraction() {
        let dates = [
            at(100, 0),
            at(100, 5),
            at(100, 500),
            at(100, 999),
            at(101, 0),
            at(1_000_000_000, 1),
        ];
        let texts: Vec<String> = dates
            .iter()
            .map(|&date| format_date("accounts", date).unwrap())
            .collect();
        let mut sorted = texts.clone();
        sorted.sort();
        assert_eq!(texts, sorted);
    }

    #[test]
    fn a_date_that_cannot_be_written_is_an_error_not_a_default() {
        let far = OffsetDateTime::UNIX_EPOCH - Duration::days(365 * 2100);
        let error = format_date("sessions", far).unwrap_err();
        assert!(error.to_string().contains("sessions"), "{error}");
    }

    #[test]
    fn an_unreadable_date_names_the_resource() {
        let error = parse_date("accounts", "hier").unwrap_err();
        assert!(error.to_string().contains("accounts"));
    }
}
