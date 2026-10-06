//! Cellules et fichier CSV du journal (BR-AUDIT-017). **Une seule** neutralisation de l'injection
//! de formule pour tout le dépôt : l'agent (export `GET /audit/export`) et la liaison cliente
//! (export d'un résultat qui exige plusieurs requêtes) passent toutes deux par [`field`].
//!
//! UTF-8 avec marque d'ordre des octets, séparateur `;` (le tableur français l'attend), fins de
//! ligne `\r\n`, dates en UTC (RFC 3339).

use super::audit::{AuditEventItem, OutcomeName};

/// Marque d'ordre des octets UTF-8 : sans elle Excel lit les accents de travers.
pub const BOM: &str = "\u{feff}";
pub const SEPARATOR: char = ';';

/// En-tête du fichier : les titres de colonnes du journal.
pub const HEADER: [&str; 7] = [
    "Date et heure",
    "Compte",
    "Origine",
    "Action",
    "Cible",
    "Résultat",
    "Raison",
];

/// Une valeur prête pour une cellule. **Injection de formule** : une valeur qui commence par `=`,
/// `+`, `-`, `@`, une tabulation ou un retour chariot serait lue comme une formule par un tableur ;
/// elle est précédée d'une apostrophe, puis mise entre guillemets si elle contient le séparateur,
/// un guillemet ou un saut de ligne.
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

/// Une ligne de cellules déjà préparées, terminée par `\r\n`.
pub fn line(cells: &[String]) -> String {
    let mut line = cells.join(&SEPARATOR.to_string());
    line.push_str("\r\n");
    line
}

/// Libellé du résultat dans le fichier.
pub fn outcome_label(outcome: OutcomeName) -> &'static str {
    match outcome {
        OutcomeName::Ok => "Réussi",
        OutcomeName::Denied => "Refusé",
        OutcomeName::Failed => "Échoué",
    }
}

/// Le fichier : marque d'ordre des octets, en-tête, une ligne par entrée dans l'ordre donné.
pub fn render_items(items: &[AuditEventItem]) -> String {
    let mut out = String::from(BOM);
    out.push_str(&line(&HEADER.map(field)));
    for item in items {
        out.push_str(&line(&[
            field(&item.at),
            field(item.account.as_deref().unwrap_or("")),
            field(&item.origin.text),
            field(&item.action_label),
            field(item.target.as_deref().unwrap_or("")),
            field(outcome_label(item.outcome)),
            field(item.reason.as_deref().unwrap_or("")),
        ]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::audit::{AuditOrigin, OriginKindName};
    use super::*;

    fn item(account: Option<&str>, target: Option<&str>) -> AuditEventItem {
        AuditEventItem {
            id: 1,
            at: "2026-09-21T14:13:20Z".into(),
            account: account.map(str::to_owned),
            origin: AuditOrigin {
                kind: OriginKindName::Client,
                name: Some("poste".into()),
                addr: Some("10.0.0.7".into()),
                text: "10.0.0.7 (poste)".into(),
            },
            action: "account.create".into(),
            action_label: "Création de compte".into(),
            target: target.map(str::to_owned),
            outcome: OutcomeName::Ok,
            reason: None,
            repeat_count: 0,
        }
    }

    #[test]
    fn values_that_a_spreadsheet_would_read_as_a_formula_are_neutralized() {
        for value in ["=1+1", "+1", "-1", "@SUM(A1)", "\t=1", "\r=1"] {
            let cell = field(value);
            assert!(
                cell.trim_matches('"').starts_with('\''),
                "{value:?} -> {cell:?}"
            );
        }
        assert_eq!(field("=a;b"), "\"'=a;b\"");
        assert_eq!(field("a=b"), "a=b");
        assert_eq!(field("marie"), "marie");
    }

    #[test]
    fn the_file_has_a_byte_order_mark_a_header_and_one_line_per_item() {
        let csv = render_items(&[item(Some("marie"), Some("paul")), item(None, None)]);
        assert!(csv.starts_with('\u{feff}'));
        let lines: Vec<&str> = csv
            .trim_start_matches('\u{feff}')
            .split("\r\n")
            .filter(|l| !l.is_empty())
            .collect();
        assert_eq!(
            lines[0],
            "Date et heure;Compte;Origine;Action;Cible;Résultat;Raison"
        );
        assert_eq!(
            lines[1],
            "2026-09-21T14:13:20Z;marie;10.0.0.7 (poste);Création de compte;paul;Réussi;"
        );
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn a_hostile_value_in_any_column_cannot_become_a_formula() {
        let mut hostile = item(Some("=HYPERLINK(1)"), Some("@x"));
        hostile.origin.text = "-1+1 (=cmd)".into();
        hostile.reason = Some("\t=1".into());
        let csv = render_items(&[hostile]);
        assert!(csv.contains(";'=HYPERLINK(1);"), "{csv}");
        assert!(csv.contains(";'-1+1 (=cmd);"), "{csv}");
        assert!(csv.contains(";'@x;"), "{csv}");
        assert!(csv.contains(";'\t=1\r\n"), "{csv}");
    }
}
