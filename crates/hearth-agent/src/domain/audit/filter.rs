//! Filtres de lecture du journal (BR-AUDIT-014 à BR-AUDIT-016) : comptes, actions et résultats en
//! multi-valeurs (combinés par ET entre filtres, par OU à l'intérieur d'un filtre), période,
//! recherche plein texte, curseur et taille de page.
//!
//! La saisie de recherche est du **texte**, jamais une requête : chaque mot devient une chaîne
//! entre guillemets pour le moteur de recherche (guillemets doublés), si bien qu'aucun caractère
//! (`"`, `*`, `-`, `:`, `(`, `AND`, `NEAR`…) ne peut en changer le sens ni le faire échouer.

use thiserror::Error;
use time::OffsetDateTime;

use super::action::AuditAction;
use super::event::OutcomeKind;

/// Entrées par page, au plus (et par défaut).
pub const MAX_PAGE_SIZE: usize = 100;
/// Mots de recherche pris en compte, au plus.
const MAX_TERMS: usize = 8;
/// Longueur maximale (en caractères) d'un mot de recherche.
const MAX_TERM_LEN: usize = 64;
/// Valeurs d'un même filtre, au plus.
const MAX_VALUES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FilterError {
    #[error("Action inconnue : {0}")]
    UnknownAction(String),
    #[error("Résultat inconnu : {0}")]
    UnknownOutcome(String),
    #[error("La fin de la période doit suivre le début")]
    PeriodReversed,
    #[error("Curseur invalide")]
    InvalidCursor,
    #[error("Trop de valeurs pour un même filtre")]
    TooManyValues,
}

/// Recherche plein texte prête pour le moteur : un mot (au moins une lettre ou un chiffre) donne
/// une chaîne entre guillemets cherchée en préfixe ; plusieurs mots s'ajoutent par ET (tous
/// doivent figurer, insensible à la casse et aux accents).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery(String);

impl SearchQuery {
    /// `None` quand il ne reste aucun mot cherchable (saisie vide ou faite de ponctuation) : le
    /// filtre est alors sans effet, jamais une erreur.
    pub fn parse(text: &str) -> Option<Self> {
        let terms: Vec<String> = text
            .split_whitespace()
            .filter(|word| word.chars().any(char::is_alphanumeric))
            .take(MAX_TERMS)
            .map(|word| {
                let word: String = word.chars().take(MAX_TERM_LEN).collect();
                format!("\"{}\"*", word.replace('"', "\"\""))
            })
            .collect();
        (!terms.is_empty()).then(|| Self(terms.join(" ")))
    }

    /// L'expression à confier au moteur de recherche.
    pub fn expression(&self) -> &str {
        &self.0
    }
}

/// Filtres tels que reçus : textes et dates déjà lus, rien de contrôlé.
#[derive(Debug, Clone, Default)]
pub struct RawFilter {
    pub accounts: Vec<String>,
    pub actions: Vec<String>,
    pub outcomes: Vec<String>,
    pub from: Option<OffsetDateTime>,
    pub to: Option<OffsetDateTime>,
    pub text: Option<String>,
    pub before: Option<i64>,
    pub limit: Option<usize>,
}

/// Filtres contrôlés.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditFilter {
    /// Identifiants de comptes (minuscules).
    pub accounts: Vec<String>,
    pub actions: Vec<AuditAction>,
    pub outcomes: Vec<OutcomeKind>,
    pub from: Option<OffsetDateTime>,
    pub to: Option<OffsetDateTime>,
    pub search: Option<SearchQuery>,
    /// Curseur : seulement les entrées d'identifiant plus petit (plus anciennes).
    pub before: Option<i64>,
    /// Taille de page, de 1 à `MAX_PAGE_SIZE`.
    pub limit: usize,
}

impl AuditFilter {
    pub fn new(raw: RawFilter) -> Result<Self, FilterError> {
        if raw.accounts.len() > MAX_VALUES
            || raw.actions.len() > MAX_VALUES
            || raw.outcomes.len() > MAX_VALUES
        {
            return Err(FilterError::TooManyValues);
        }
        let accounts = raw
            .accounts
            .iter()
            .map(|account| account.trim().to_ascii_lowercase())
            .filter(|account| !account.is_empty())
            .collect();
        let actions = raw
            .actions
            .iter()
            .map(|code| code.trim())
            .filter(|code| !code.is_empty())
            .map(|code| {
                AuditAction::from_code(code)
                    .ok_or_else(|| FilterError::UnknownAction(code.to_owned()))
            })
            .collect::<Result<_, _>>()?;
        let outcomes = raw
            .outcomes
            .iter()
            .map(|code| code.trim())
            .filter(|code| !code.is_empty())
            .map(|code| {
                OutcomeKind::from_code(code)
                    .ok_or_else(|| FilterError::UnknownOutcome(code.to_owned()))
            })
            .collect::<Result<_, _>>()?;
        if let (Some(from), Some(to)) = (raw.from, raw.to)
            && to < from
        {
            return Err(FilterError::PeriodReversed);
        }
        if raw.before.is_some_and(|before| before <= 0) {
            return Err(FilterError::InvalidCursor);
        }
        Ok(Self {
            accounts,
            actions,
            outcomes,
            from: raw.from,
            to: raw.to,
            search: raw.text.as_deref().and_then(SearchQuery::parse),
            before: raw.before,
            limit: raw.limit.unwrap_or(MAX_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE),
        })
    }
}

#[cfg(test)]
mod tests {
    use time::Duration;

    use super::*;

    fn t(seconds: i64) -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH + Duration::seconds(seconds)
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn an_empty_filter_is_one_page_of_everything() {
        let filter = AuditFilter::new(RawFilter::default()).unwrap();
        assert!(filter.accounts.is_empty() && filter.actions.is_empty());
        assert!(filter.outcomes.is_empty() && filter.search.is_none());
        assert_eq!((filter.from, filter.to, filter.before), (None, None, None));
        assert_eq!(filter.limit, MAX_PAGE_SIZE);
    }

    #[test]
    fn filters_accept_several_values_and_ignore_blanks() {
        let filter = AuditFilter::new(RawFilter {
            accounts: strings(&[" Marie ", "", "paul"]),
            actions: strings(&["login", " account.create "]),
            outcomes: strings(&["denied", "failed"]),
            ..RawFilter::default()
        })
        .unwrap();
        assert_eq!(filter.accounts, ["marie", "paul"]);
        assert_eq!(
            filter.actions,
            [AuditAction::Login, AuditAction::AccountCreate]
        );
        assert_eq!(filter.outcomes, [OutcomeKind::Denied, OutcomeKind::Failed]);
    }

    #[test]
    fn an_unknown_action_or_outcome_is_refused() {
        let action = AuditFilter::new(RawFilter {
            actions: strings(&["nope"]),
            ..RawFilter::default()
        });
        assert_eq!(action, Err(FilterError::UnknownAction("nope".into())));
        let outcome = AuditFilter::new(RawFilter {
            outcomes: strings(&["maybe"]),
            ..RawFilter::default()
        });
        assert_eq!(outcome, Err(FilterError::UnknownOutcome("maybe".into())));
    }

    #[test]
    fn a_period_must_not_end_before_it_starts() {
        let reversed = AuditFilter::new(RawFilter {
            from: Some(t(100)),
            to: Some(t(99)),
            ..RawFilter::default()
        });
        assert_eq!(reversed, Err(FilterError::PeriodReversed));
        assert!(
            AuditFilter::new(RawFilter {
                from: Some(t(100)),
                to: Some(t(100)),
                ..RawFilter::default()
            })
            .is_ok()
        );
        assert!(
            AuditFilter::new(RawFilter {
                from: Some(t(100)),
                ..RawFilter::default()
            })
            .is_ok()
        );
    }

    #[test]
    fn the_page_size_is_clamped_and_the_cursor_checked() {
        let limit = |value| {
            AuditFilter::new(RawFilter {
                limit: Some(value),
                ..RawFilter::default()
            })
            .unwrap()
            .limit
        };
        assert_eq!(
            (limit(0), limit(1), limit(50), limit(100), limit(5000)),
            (1, 1, 50, 100, 100)
        );
        for bad in [0, -3] {
            let result = AuditFilter::new(RawFilter {
                before: Some(bad),
                ..RawFilter::default()
            });
            assert_eq!(result, Err(FilterError::InvalidCursor), "{bad}");
        }
        let ok = AuditFilter::new(RawFilter {
            before: Some(7),
            ..RawFilter::default()
        })
        .unwrap();
        assert_eq!(ok.before, Some(7));
    }

    #[test]
    fn too_many_values_are_refused() {
        let many: Vec<String> = (0..65).map(|n| format!("user{n}")).collect();
        let result = AuditFilter::new(RawFilter {
            accounts: many,
            ..RawFilter::default()
        });
        assert_eq!(result, Err(FilterError::TooManyValues));
    }

    #[test]
    fn each_word_becomes_a_quoted_prefix_term_joined_by_and() {
        assert_eq!(
            SearchQuery::parse("Connexion 10.0.0.7")
                .unwrap()
                .expression(),
            "\"Connexion\"* \"10.0.0.7\"*"
        );
        assert_eq!(
            SearchQuery::parse("  marie  ").unwrap().expression(),
            "\"marie\"*"
        );
    }

    #[test]
    fn engine_syntax_in_the_input_is_only_text() {
        let query = SearchQuery::parse("a\"b OR NEAR(x y) -z col:val ^w * AND").unwrap();
        let expression = query.expression();
        // Chaque mot est une chaîne entre guillemets (guillemets internes doublés) cherchée en
        // préfixe : OR, AND, NEAR, -, :, ^ y sont des mots ordinaires.
        assert!(expression.starts_with("\"a\"\"b\"* \"OR\"* \"NEAR(x\"*"));
        assert!(expression.contains("\"-z\"*") && expression.contains("\"col:val\"*"));
        assert!(!expression.contains(" * "), "un mot sans lettre est écarté");
    }

    #[test]
    fn punctuation_only_or_blank_input_means_no_search() {
        for text in ["", "   ", "***", "\" \"", "( ) - :", "\t\n"] {
            assert_eq!(SearchQuery::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn the_search_is_bounded() {
        let many = (0..50)
            .map(|n| format!("mot{n}"))
            .collect::<Vec<_>>()
            .join(" ");
        let query = SearchQuery::parse(&many).unwrap();
        assert_eq!(query.expression().matches(" \"").count(), MAX_TERMS - 1);
        let long = SearchQuery::parse(&"x".repeat(500)).unwrap();
        assert_eq!(long.expression().chars().count(), MAX_TERM_LEN + 3);
    }

    #[test]
    fn the_input_search_is_kept_by_the_filter() {
        let filter = AuditFilter::new(RawFilter {
            text: Some("marie".into()),
            ..RawFilter::default()
        })
        .unwrap();
        assert_eq!(filter.search.unwrap().expression(), "\"marie\"*");
        let none = AuditFilter::new(RawFilter {
            text: Some("***".into()),
            ..RawFilter::default()
        })
        .unwrap();
        assert!(none.search.is_none());
    }
}
