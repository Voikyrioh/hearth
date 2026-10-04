//! Export du journal en CSV (BR-AUDIT-017) : UTF-8 avec marque d'ordre des octets, séparateur `;`
//! (le tableur français l'attend), fins de ligne `\r\n`.
//!
//! **Injection de formule** : une valeur qui commence par `=`, `+`, `-`, `@`, une tabulation ou un
//! retour chariot serait lue comme une formule par un tableur ; elle est précédée d'une
//! apostrophe (neutralisée avant d'être mise entre guillemets).

use time::UtcOffset;
use time::format_description::well_known::Rfc3339;

use super::event::AuditRecord;

/// Marque d'ordre des octets UTF-8 : sans elle Excel lit les accents de travers.
pub const BOM: &str = "\u{feff}";
pub const SEPARATOR: char = ';';

const HEADER: [&str; 7] = [
    "Date et heure",
    "Compte",
    "Origine",
    "Action",
    "Cible",
    "Résultat",
    "Raison",
];

/// Une valeur prête pour une cellule : neutralisée contre l'injection de formule, puis entre
/// guillemets si elle contient le séparateur, un guillemet ou un saut de ligne.
pub fn field(value: &str) -> String {
    let neutralized = if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    if neutralized.contains([SEPARATOR, '"', '\n', '\r']) {
        format!("\"{}\"", neutralized.replace('"', "\"\""))
    } else {
        neutralized
    }
}

fn line(cells: &[String]) -> String {
    let mut line = cells.join(&SEPARATOR.to_string());
    line.push_str("\r\n");
    line
}

/// Le fichier : marque d'ordre des octets, en-tête, une ligne par entrée dans l'ordre donné.
/// Les dates sont en UTC (RFC 3339), sans ambiguïté de fuseau.
pub fn render(records: &[AuditRecord]) -> String {
    let mut out = String::from(BOM);
    out.push_str(&line(&HEADER.map(field)));
    for record in records {
        let date = record
            .at
            .to_offset(UtcOffset::UTC)
            .format(&Rfc3339)
            .unwrap_or_default();
        let outcome = record.outcome.label();
        out.push_str(&line(&[
            field(&date),
            field(record.account.as_deref().unwrap_or("")),
            field(&record.origin_text()),
            field(&record.action_label),
            field(record.target.as_deref().unwrap_or("")),
            field(outcome),
            field(record.reason.as_deref().unwrap_or("")),
        ]));
    }
    out
}

#[cfg(test)]
mod tests {
    use time::{Duration, OffsetDateTime};

    use super::super::event::{OriginKind, OutcomeKind};
    use super::*;

    fn record(account: Option<&str>, target: Option<&str>) -> AuditRecord {
        AuditRecord {
            id: 1,
            at: OffsetDateTime::UNIX_EPOCH + Duration::seconds(1_790_000_000),
            account: account.map(str::to_owned),
            origin_kind: OriginKind::Client,
            origin_name: Some("poste".into()),
            origin_addr: Some("10.0.0.7".into()),
            action: "account.create".into(),
            action_label: "Création de compte".into(),
            target: target.map(str::to_owned),
            outcome: OutcomeKind::Ok,
            reason: None,
        }
    }

    #[test]
    fn plain_values_are_left_alone() {
        assert_eq!(field("marie"), "marie");
        assert_eq!(field(""), "");
        assert_eq!(field("10.0.0.7 (poste)"), "10.0.0.7 (poste)");
    }

    #[test]
    fn values_that_a_spreadsheet_would_read_as_a_formula_are_neutralized() {
        for value in ["=1+1", "+1", "-1", "@SUM(A1)", "\t=1", "\r=1"] {
            let cell = field(value);
            let inner = cell.trim_matches('"');
            assert!(inner.starts_with('\''), "{value:?} -> {cell:?}");
        }
        assert_eq!(field("=cmd|' /C calc'!A0"), "'=cmd|' /C calc'!A0");
        // Un signe au milieu n'est pas une formule.
        assert_eq!(field("a=b"), "a=b");
    }

    #[test]
    fn separators_quotes_and_line_breaks_are_quoted() {
        assert_eq!(field("a;b"), "\"a;b\"");
        assert_eq!(field("dit \"oui\""), "\"dit \"\"oui\"\"\"");
        assert_eq!(field("a\nb"), "\"a\nb\"");
        // Neutralisé puis mis entre guillemets.
        assert_eq!(field("=a;b"), "\"'=a;b\"");
    }

    #[test]
    fn the_file_starts_with_the_byte_order_mark_and_the_header() {
        let csv = render(&[]);
        assert!(csv.starts_with('\u{feff}'));
        assert_eq!(
            csv.trim_start_matches('\u{feff}'),
            "Date et heure;Compte;Origine;Action;Cible;Résultat;Raison\r\n"
        );
    }

    #[test]
    fn one_line_per_record_in_the_given_order() {
        let csv = render(&[record(Some("marie"), Some("paul")), record(None, None)]);
        let lines: Vec<&str> = csv
            .trim_start_matches('\u{feff}')
            .split("\r\n")
            .filter(|line| !line.is_empty())
            .collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(
            lines[1],
            "2026-09-21T14:13:20Z;marie;10.0.0.7 (poste);Création de compte;paul;Réussi;"
        );
        assert_eq!(
            lines[2],
            "2026-09-21T14:13:20Z;;10.0.0.7 (poste);Création de compte;;Réussi;"
        );
    }

    #[test]
    fn a_hostile_account_or_target_cannot_become_a_formula() {
        let csv = render(&[record(Some("=HYPERLINK(1)"), Some("@x"))]);
        assert!(csv.contains(";'=HYPERLINK(1);"), "{csv}");
        assert!(csv.contains(";'@x;"), "{csv}");
    }
}
