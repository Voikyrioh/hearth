//! Lecture filtrée du journal d'activité (HRT-14) : le filtre TYPÉ que l'interface peut demander et
//! sa traduction en requêtes pour l'agent (`GET /audit`, BR-AUDIT-014, 015, 016). Pur : aucune E/S.
//!
//! L'interface ne fournit jamais un chemin, une méthode ni une requête brute : elle remplit un
//! [`AuditFilter`] dont chaque champ est validé ici (identifiants de comptes bornés, types d'action
//! de la liste connue, résultats, bornes de dates, texte de recherche borné), et le chemin est
//! construit par [`path`]. Les types d'action de la spec (« Connexion réussie »…) couplent une action
//! et un résultat : l'agent ne sait pas combiner ces paires par OU, [`AuditFilter::plan`] fait donc
//! une requête par rectangle (actions × résultats) et [`merge_pages`] recolle les pages sans rien
//! perdre ni doubler.

use hearth_proto::api::audit::{AuditEventItem, AuditQuery, AuditResponse, OutcomeName};

/// Comptes retenus au plus dans un filtre.
pub const MAX_ACCOUNTS: usize = 50;
/// Longueur maximale d'un identifiant de compte (le format d'identifiant de l'agent en accepte 32).
pub const MAX_ACCOUNT_CHARS: usize = 64;
/// Longueur maximale du texte de recherche.
pub const MAX_TEXT_CHARS: usize = 200;
/// Entrées d'une page (le maximum de l'agent).
pub const PAGE_SIZE: u8 = 100;
/// Plafond de l'export (celui de l'agent, BR-AUDIT-017).
pub const EXPORT_CAP: usize = 10_000;

/// Les types d'action de la spec (liste fermée, texte de l'interface dans `fr.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionKind {
    /// Connexion réussie.
    LoginOk,
    /// Connexion refusée, y compris le blocage temporaire qui la suit (BR-AUDIT-007).
    LoginDenied,
    /// Gestion des comptes : création, suppression, rôle, mots de passe, sessions.
    Accounts,
    /// Mise à jour de l'agent.
    Update,
    /// Action refusée faute de droits (hors connexion).
    Denied,
}

const ALL_OUTCOMES: [OutcomeName; 3] = [OutcomeName::Ok, OutcomeName::Denied, OutcomeName::Failed];

impl ActionKind {
    /// Codes d'action de l'agent couverts.
    fn codes(self) -> &'static [&'static str] {
        match self {
            Self::LoginOk => &["login"],
            Self::LoginDenied => &["login", "login.locked"],
            Self::Accounts => &[
                "account.create",
                "account.delete",
                "account.role",
                "account.password",
                "account.password.own",
                "sessions.revoke",
            ],
            Self::Update => &["agent.update"],
            Self::Denied => &[
                "logout",
                "account.create",
                "account.delete",
                "account.role",
                "account.password",
                "account.password.own",
                "sessions.revoke",
                "accounts.read",
                "audit.read",
                "agent.update",
            ],
        }
    }

    /// Résultats que ce type implique.
    fn outcomes(self) -> &'static [OutcomeName] {
        match self {
            Self::LoginOk => &[OutcomeName::Ok],
            Self::LoginDenied | Self::Denied => &[OutcomeName::Denied],
            Self::Accounts | Self::Update => &ALL_OUTCOMES,
        }
    }
}

/// Pourquoi un filtre est refusé. Aucun texte : l'interface choisit le sien.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FilterError {
    #[error("trop de comptes dans le filtre")]
    TooManyAccounts,
    #[error("identifiant de compte invalide")]
    BadAccount,
    #[error("texte de recherche trop long")]
    TextTooLong,
    #[error("la fin de la période précède le début")]
    PeriodReversed,
    #[error("date hors des limites")]
    BadDate,
    #[error("curseur invalide")]
    BadCursor,
}

/// Ce que l'interface demande, avant validation. Dates : secondes depuis l'époque (UTC).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawFilter {
    pub accounts: Vec<String>,
    pub kinds: Vec<ActionKind>,
    pub outcomes: Vec<OutcomeName>,
    pub from_s: Option<i64>,
    pub to_s: Option<i64>,
    pub text: Option<String>,
}

/// Un filtre valide.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditFilter {
    accounts: Vec<String>,
    kinds: Vec<ActionKind>,
    outcomes: Vec<OutcomeName>,
    from_s: Option<i64>,
    to_s: Option<i64>,
    text: Option<String>,
}

/// Bornes de date acceptées : 2000-01-01 à 2200-01-01 (au-delà, ce n'est pas une date du journal).
const MIN_DATE_S: i64 = 946_684_800;
const MAX_DATE_S: i64 = 7_258_118_400;

impl AuditFilter {
    pub fn new(raw: RawFilter) -> Result<Self, FilterError> {
        let mut accounts: Vec<String> = Vec::new();
        for account in raw.accounts {
            let account = account.trim().to_owned();
            if account.is_empty()
                || account.chars().count() > MAX_ACCOUNT_CHARS
                || account.contains(',')
                || account.chars().any(char::is_control)
            {
                return Err(FilterError::BadAccount);
            }
            if !accounts.iter().any(|a| a.eq_ignore_ascii_case(&account)) {
                accounts.push(account);
            }
        }
        if accounts.len() > MAX_ACCOUNTS {
            return Err(FilterError::TooManyAccounts);
        }
        let mut kinds = Vec::new();
        for kind in raw.kinds {
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        }
        let mut outcomes = Vec::new();
        for outcome in raw.outcomes {
            if !outcomes.contains(&outcome) {
                outcomes.push(outcome);
            }
        }
        for date in [raw.from_s, raw.to_s].into_iter().flatten() {
            if !(MIN_DATE_S..=MAX_DATE_S).contains(&date) {
                return Err(FilterError::BadDate);
            }
        }
        if let (Some(from), Some(to)) = (raw.from_s, raw.to_s)
            && to < from
        {
            return Err(FilterError::PeriodReversed);
        }
        // Le texte : caractères de contrôle remplacés par des espaces, espaces de bord retirés.
        let text = raw
            .text
            .map(|text| {
                text.chars()
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .collect::<String>()
                    .trim()
                    .to_owned()
            })
            .filter(|text| !text.is_empty());
        if text
            .as_ref()
            .is_some_and(|t| t.chars().count() > MAX_TEXT_CHARS)
        {
            return Err(FilterError::TextTooLong);
        }
        Ok(Self {
            accounts,
            kinds,
            outcomes,
            from_s: raw.from_s,
            to_s: raw.to_s,
            text,
        })
    }

    /// Les requêtes à envoyer à l'agent pour obtenir exactement le résultat filtré : une par
    /// rectangle (actions × résultats). Vide quand le filtre ne peut rien retenir (par exemple
    /// « Connexion réussie » avec le résultat « Refusé ») : inutile d'interroger l'agent.
    pub fn plan(&self) -> Vec<AuditQuery> {
        type Rectangle = (Vec<&'static str>, Vec<OutcomeName>);
        let mut rectangles: Vec<Rectangle> = Vec::new();
        if self.kinds.is_empty() {
            let outcomes = if self.outcomes.is_empty() {
                ALL_OUTCOMES.to_vec()
            } else {
                self.outcomes.clone()
            };
            rectangles.push((Vec::new(), outcomes));
        } else {
            for kind in &self.kinds {
                let outcomes: Vec<OutcomeName> = kind
                    .outcomes()
                    .iter()
                    .copied()
                    .filter(|o| self.outcomes.is_empty() || self.outcomes.contains(o))
                    .collect();
                if !outcomes.is_empty() {
                    rectangles.push((kind.codes().to_vec(), outcomes));
                }
            }
        }
        // Deux rectangles de même résultat ou de mêmes actions n'en font qu'un.
        while let Some((i, j)) = mergeable(&rectangles) {
            let other = rectangles.remove(j);
            let first = &mut rectangles[i];
            for code in other.0 {
                if !first.0.contains(&code) {
                    first.0.push(code);
                }
            }
            for outcome in other.1 {
                if !first.1.contains(&outcome) {
                    first.1.push(outcome);
                }
            }
        }
        rectangles
            .into_iter()
            .map(|(codes, outcomes)| self.query(&codes, &outcomes))
            .collect()
    }

    fn query(&self, codes: &[&str], outcomes: &[OutcomeName]) -> AuditQuery {
        let join = |items: Vec<&str>| (!items.is_empty()).then(|| items.join(","));
        let all = outcomes.len() == ALL_OUTCOMES.len();
        AuditQuery {
            account: join(self.accounts.iter().map(String::as_str).collect()),
            action: join(codes.to_vec()),
            outcome: if all {
                None
            } else {
                join(outcomes.iter().map(|o| outcome_code(*o)).collect())
            },
            from: self.from_s.map(rfc3339),
            to: self.to_s.map(rfc3339),
            q: self.text.clone(),
            before: None,
            limit: None,
        }
    }
}

fn mergeable(rectangles: &[(Vec<&'static str>, Vec<OutcomeName>)]) -> Option<(usize, usize)> {
    for i in 0..rectangles.len() {
        for j in (i + 1)..rectangles.len() {
            if same_set(&rectangles[i].1, &rectangles[j].1)
                || same_set(&rectangles[i].0, &rectangles[j].0)
            {
                return Some((i, j));
            }
        }
    }
    None
}

fn same_set<T: PartialEq>(a: &[T], b: &[T]) -> bool {
    a.len() == b.len() && a.iter().all(|x| b.contains(x))
}

fn outcome_code(outcome: OutcomeName) -> &'static str {
    match outcome {
        OutcomeName::Ok => "ok",
        OutcomeName::Denied => "denied",
        OutcomeName::Failed => "failed",
    }
}

/// Secondes depuis l'époque, en RFC 3339 UTC (`2026-10-04T10:30:15Z`).
pub fn rfc3339(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    // Jours depuis 1970-01-01 vers date civile (algorithme de H. Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

/// Encodage d'une valeur de requête : tout sauf les caractères non réservés (RFC 3986).
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Chemin de `GET /audit` (page : `Some((curseur, taille))`) ou de `GET /audit/export` (`None`),
/// relatif à `/api/v1`, construit ici et nulle part ailleurs.
pub fn path(query: &AuditQuery, page: Option<(Option<i64>, u8)>) -> Result<String, FilterError> {
    let mut parts: Vec<(&str, String)> = Vec::new();
    let named = [
        ("account", &query.account),
        ("action", &query.action),
        ("outcome", &query.outcome),
        ("from", &query.from),
        ("to", &query.to),
        ("q", &query.q),
    ];
    for (name, value) in named {
        if let Some(value) = value {
            parts.push((name, encode(value)));
        }
    }
    let route = if let Some((before, limit)) = page {
        if let Some(before) = before {
            if before <= 0 {
                return Err(FilterError::BadCursor);
            }
            parts.push(("before", before.to_string()));
        }
        parts.push(("limit", limit.clamp(1, PAGE_SIZE).to_string()));
        "/audit"
    } else {
        "/audit/export"
    };
    if parts.is_empty() {
        return Ok(route.to_owned());
    }
    let query: Vec<String> = parts.into_iter().map(|(k, v)| format!("{k}={v}")).collect();
    Ok(format!("{route}?{}", query.join("&")))
}

/// Recolle les pages de plusieurs requêtes : les `limit` entrées les plus récentes de leur union,
/// sans doublon, de la plus récente à la plus ancienne. `next_before` est le curseur de la suite,
/// absent quand rien ne reste. Exact : les `limit` plus récentes de l'union sont parmi les `limit`
/// plus récentes de l'une des requêtes.
pub fn merge_pages(pages: Vec<AuditResponse>, limit: usize) -> AuditResponse {
    let more_upstream = pages.iter().any(|page| page.next_before.is_some());
    let mut events: Vec<AuditEventItem> = pages.into_iter().flat_map(|page| page.events).collect();
    events.sort_by_key(|event| std::cmp::Reverse(event.id));
    events.dedup_by_key(|event| event.id);
    let more = more_upstream || events.len() > limit;
    events.truncate(limit);
    let next_before = if more {
        events.last().map(|event| event.id)
    } else {
        None
    };
    AuditResponse {
        events,
        next_before,
    }
}

#[cfg(test)]
mod tests {
    use hearth_proto::api::audit::{AuditOrigin, OriginKindName};

    use super::*;

    fn raw() -> RawFilter {
        RawFilter::default()
    }

    fn item(id: i64) -> AuditEventItem {
        AuditEventItem {
            id,
            at: "2026-10-04T10:30:15Z".into(),
            account: None,
            origin: AuditOrigin {
                kind: OriginKindName::Cli,
                name: None,
                addr: None,
                text: "ligne de commande du serveur".into(),
            },
            action: "login".into(),
            action_label: "Connexion".into(),
            target: None,
            outcome: OutcomeName::Ok,
            reason: None,
            repeat_count: 0,
        }
    }

    fn page(ids: &[i64], next: Option<i64>) -> AuditResponse {
        AuditResponse {
            events: ids.iter().map(|id| item(*id)).collect(),
            next_before: next,
        }
    }

    #[test]
    fn an_empty_filter_is_one_unfiltered_query() {
        let plan = AuditFilter::new(raw()).unwrap().plan();
        assert_eq!(plan, vec![AuditQuery::default()]);
        assert_eq!(
            path(&plan[0], Some((None, 100))).unwrap(),
            "/audit?limit=100"
        );
        assert_eq!(path(&plan[0], None).unwrap(), "/audit/export");
    }

    #[test]
    fn accounts_are_bounded_trimmed_and_deduplicated() {
        let filter = AuditFilter::new(RawFilter {
            accounts: vec![" marie ".into(), "MARIE".into(), "paul".into()],
            ..raw()
        })
        .unwrap();
        assert_eq!(filter.plan()[0].account.as_deref(), Some("marie,paul"));
        for bad in ["", "a,b", "a\nb", &"x".repeat(65)] {
            let result = AuditFilter::new(RawFilter {
                accounts: vec![bad.into()],
                ..raw()
            });
            assert_eq!(result, Err(FilterError::BadAccount), "{bad:?}");
        }
        let many = (0..51).map(|i| format!("c{i}")).collect();
        assert_eq!(
            AuditFilter::new(RawFilter {
                accounts: many,
                ..raw()
            }),
            Err(FilterError::TooManyAccounts)
        );
    }

    #[test]
    fn the_search_text_is_bounded_and_cleaned_but_never_a_query() {
        let filter = AuditFilter::new(RawFilter {
            text: Some("  10.0.0.7\n\"OR\" *  ".into()),
            ..raw()
        })
        .unwrap();
        assert_eq!(filter.plan()[0].q.as_deref(), Some("10.0.0.7 \"OR\" *"));
        assert_eq!(
            AuditFilter::new(RawFilter {
                text: Some("x".repeat(201)),
                ..raw()
            }),
            Err(FilterError::TextTooLong)
        );
        let blank = AuditFilter::new(RawFilter {
            text: Some(" \t ".into()),
            ..raw()
        })
        .unwrap();
        assert_eq!(blank.plan()[0].q, None);
    }

    #[test]
    fn the_period_must_be_ordered_and_plausible() {
        assert_eq!(
            AuditFilter::new(RawFilter {
                from_s: Some(1_790_000_100),
                to_s: Some(1_790_000_000),
                ..raw()
            }),
            Err(FilterError::PeriodReversed)
        );
        assert_eq!(
            AuditFilter::new(RawFilter {
                from_s: Some(-5),
                ..raw()
            }),
            Err(FilterError::BadDate)
        );
        let ok = AuditFilter::new(RawFilter {
            from_s: Some(1_790_000_000),
            to_s: Some(1_790_000_000),
            ..raw()
        })
        .unwrap();
        assert_eq!(ok.plan()[0].from.as_deref(), Some("2026-09-21T14:13:20Z"));
        assert_eq!(ok.plan()[0].to.as_deref(), Some("2026-09-21T14:13:20Z"));
    }

    #[test]
    fn dates_are_formatted_as_rfc3339_utc() {
        assert_eq!(rfc3339(946_684_800), "2000-01-01T00:00:00Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339(1_790_000_000), "2026-09-21T14:13:20Z");
        assert_eq!(rfc3339(4_102_444_799), "2099-12-31T23:59:59Z");
    }

    #[test]
    fn the_values_of_a_query_are_percent_encoded_so_nothing_can_add_a_parameter() {
        let filter = AuditFilter::new(RawFilter {
            text: Some("a&limit=1#b c".into()),
            ..raw()
        })
        .unwrap();
        let built = path(&filter.plan()[0], Some((Some(42), 100))).unwrap();
        assert_eq!(built, "/audit?q=a%26limit%3D1%23b%20c&before=42&limit=100");
        assert_eq!(
            path(&AuditQuery::default(), Some((Some(0), 100))),
            Err(FilterError::BadCursor)
        );
    }

    #[test]
    fn an_action_kind_alone_is_one_query_with_its_codes_and_result() {
        let plan = AuditFilter::new(RawFilter {
            kinds: vec![ActionKind::LoginOk],
            ..raw()
        })
        .unwrap()
        .plan();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].action.as_deref(), Some("login"));
        assert_eq!(plan[0].outcome.as_deref(), Some("ok"));
    }

    #[test]
    fn a_kind_that_contradicts_the_result_filter_retains_nothing() {
        let plan = AuditFilter::new(RawFilter {
            kinds: vec![ActionKind::LoginOk],
            outcomes: vec![OutcomeName::Denied],
            ..raw()
        })
        .unwrap()
        .plan();
        assert!(plan.is_empty());
    }

    #[test]
    fn kinds_that_cannot_share_a_query_are_asked_separately_and_equal_ones_are_merged() {
        let plan = AuditFilter::new(RawFilter {
            kinds: vec![ActionKind::LoginOk, ActionKind::Accounts],
            ..raw()
        })
        .unwrap()
        .plan();
        assert_eq!(plan.len(), 2, "{plan:?}");
        let merged = AuditFilter::new(RawFilter {
            kinds: vec![ActionKind::LoginDenied, ActionKind::Denied],
            ..raw()
        })
        .unwrap()
        .plan();
        assert_eq!(merged.len(), 1, "{merged:?}");
        assert_eq!(merged[0].outcome.as_deref(), Some("denied"));
        let both = AuditFilter::new(RawFilter {
            kinds: vec![ActionKind::LoginOk, ActionKind::LoginDenied],
            ..raw()
        })
        .unwrap()
        .plan();
        // Les codes diffèrent (le blocage n'est que « refusé ») : fusionner élargirait le résultat.
        assert_eq!(both.len(), 2, "{both:?}");
    }

    #[test]
    fn merged_pages_keep_the_newest_without_duplicates_or_losses() {
        let merged = merge_pages(
            vec![page(&[9, 7, 5], Some(5)), page(&[8, 7, 6], Some(6))],
            4,
        );
        let ids: Vec<i64> = merged.events.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![9, 8, 7, 6]);
        assert_eq!(merged.next_before, Some(6));
        // Rien de plus nulle part : pas de curseur.
        let all = merge_pages(vec![page(&[3, 1], None), page(&[2], None)], 100);
        assert_eq!(all.events.len(), 3);
        assert_eq!(all.next_before, None);
        // Une source en a encore, même si l'union tient dans la page.
        let more = merge_pages(vec![page(&[3], Some(3)), page(&[2], None)], 100);
        assert_eq!(more.next_before, Some(2));
    }
}
