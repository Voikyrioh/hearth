//! Règles de la mise à jour du client, pures : ni E/S, ni réseau, ni Tauri, horloge injectée
//! (BR-UPDATE-001 à 010). Le greffon de mise à jour de Tauri fait le réseau et la vérification de
//! la signature ; ici on décide QUAND vérifier, QUAND montrer le bandeau, et ce qu'on croit d'une
//! annonce de version avant de la montrer (ADR-0017).

use serde::{Deserialize, Serialize};
use url::Url;

use crate::agent_update::domain::AgentTargetRecord;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// Intervalle minimal entre deux vérifications automatiques (BR-UPDATE-001).
pub const CHECK_INTERVAL_MS: i64 = DAY_MS;
/// Durée du report « Plus tard » (BR-UPDATE-006).
pub const POSTPONE_MS: i64 = DAY_MS;

/// Adresse du flux de versions : fixée à la compilation, jamais saisie (ADR-0017). GitHub
/// redirige `latest` vers la dernière release publiée du dépôt public (hors préversions).
pub const FEED_URL: &str =
    "https://github.com/Voikyrioh/hearth/releases/latest/download/latest.json";
/// Les installateurs ne se téléchargent que de ce dépôt (défense en profondeur : la signature reste
/// la barrière).
pub const DOWNLOAD_HOST: &str = "github.com";
pub const DOWNLOAD_PATH_PREFIX: &str = "/Voikyrioh/hearth/releases/download/";

/// Nombre de redirections suivies au plus, pour le manifeste comme pour l'installateur. GitHub en
/// fait déjà DEUX aujourd'hui (`releases/latest/download/…` vers `releases/download/vN/…`, puis vers
/// le stockage des releases, relevé sur les en-têtes d'une release publique d'un autre dépôt). La
/// borne est large exprès : un saut de plus chez GitHub ne doit pas rendre muets tous les clients
/// installés, sans correctif possible puisque le correctif passerait par ce même canal. C'est le
/// rang de la redirection (la 1re, la 2e…) qui est borné, l'adresse de départ n'est pas comptée.
pub const MAX_REDIRECTS: usize = 5;
/// Tentatives réseau AUTOMATIQUES au plus par 24 h glissantes, quoi qu'il arrive (réussies, échouées,
/// sans réseau) : plafond dur, compté et persisté, indépendant du classement des erreurs.
pub const MAX_AUTOMATIC_ATTEMPTS_PER_DAY: usize = 3;
/// Une vérification manuelle ne repart pas moins de 30 s après la précédente tentative (une page
/// compromise ne peut pas boucler sur la requête vers GitHub).
pub const MANUAL_CHECK_MIN_INTERVAL_MS: i64 = 30_000;

/// Les notes de version sont du texte affiché tel quel, jamais du HTML ; bornées.
pub const NOTES_MAX_CHARS: usize = 8_000;
/// Une signature minisign encodée fait quelques centaines d'octets.
pub const SIGNATURE_MAX_LEN: usize = 4_096;

/// Ce que le client retient sur disque (reste valable si la WebView est vidée : la règle « une fois
/// par jour au plus » ne dépend d'aucun stockage de la page). Dates en millisecondes depuis l'époque.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdateRecord {
    /// Dernière vérification commencée (automatique ou manuelle), réussie ou non.
    pub last_attempt_at: Option<i64>,
    /// Dernière vérification qui a ÉMIS une requête (réponse ou non) : c'est elle, et elle seule, qui
    /// consomme le quota d'une vérification automatique par 24 h. Une tentative sans réseau (aucune
    /// requête partie) ne la change pas.
    pub last_request_at: Option<i64>,
    /// Instants des tentatives réseau AUTOMATIQUES des dernières 24 h (plafond dur,
    /// `MAX_AUTOMATIC_ATTEMPTS_PER_DAY`).
    pub automatic_attempts: Vec<i64>,
    /// Dernière vérification qui a obtenu une réponse valable (« Dernière vérification »).
    pub last_success_at: Option<i64>,
    /// Le bandeau est masqué jusque-là (« Plus tard »).
    pub postponed_until: Option<i64>,
    /// Version plus récente connue à la dernière vérification réussie.
    pub available: Option<Release>,
    /// La cible de l'AGENT lue dans `agent.json`, à la même vérification (ADR-0021). Relue et
    /// validée de nouveau à chaque usage : ce fichier se modifie à la main.
    pub agent: Option<AgentTargetRecord>,
    /// Pour chaque serveur, la date (`at`) du dernier résultat de mise à jour de l'agent déjà ANNONCÉ à
    /// l'utilisateur : un résultat est annoncé une seule fois, même après un redémarrage du client
    /// (HRT-17). Borné à `MAX_SEEN_RESULTS` serveurs.
    pub agent_results_seen: std::collections::BTreeMap<String, String>,
}

/// Serveurs dont on garde le dernier résultat annoncé.
pub const MAX_SEEN_RESULTS: usize = 64;

/// Une version annoncée et acceptée par les règles ci-dessous.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub version: String,
    pub notes: String,
}

/// L'annonce telle que le greffon la rend, avant toute confiance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub version: String,
    pub notes: Option<String>,
    pub download_url: String,
    pub signature: String,
}

/// D'où l'installateur peut venir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadPolicy {
    scheme: &'static str,
    host: String,
    port: Option<u16>,
    path_prefix: String,
    /// Chaque redirection doit être en HTTPS (toujours vrai en production).
    redirects_https_only: bool,
}

impl DownloadPolicy {
    /// Production : HTTPS, `github.com`, releases du dépôt public.
    pub fn github_releases() -> Self {
        Self {
            scheme: "https",
            host: DOWNLOAD_HOST.to_owned(),
            port: None,
            path_prefix: DOWNLOAD_PATH_PREFIX.to_owned(),
            redirects_https_only: true,
        }
    }

    /// Serveur de versions local des tests : n'existe pas dans un binaire de publication.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn local_for_tests(port: u16) -> Self {
        Self {
            scheme: "http",
            host: "127.0.0.1".to_owned(),
            port: Some(port),
            path_prefix: "/".to_owned(),
            redirects_https_only: false,
        }
    }

    /// Un hôte public de test en HTTPS (la cible de l'agent d'un test contre un vrai agent dont le
    /// téléchargeur est simulé) : n'existe pas dans un binaire de publication.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn host_for_tests(host: &str) -> Self {
        Self {
            scheme: "https",
            host: host.to_owned(),
            port: None,
            path_prefix: "/".to_owned(),
            redirects_https_only: true,
        }
    }

    /// Comme `local_for_tests`, mais avec la règle de production sur les redirections : HTTPS
    /// obligatoire à chaque saut (un saut vers `http://` est refusé).
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn local_strict_redirects_for_tests(port: u16) -> Self {
        Self {
            redirects_https_only: true,
            ..Self::local_for_tests(port)
        }
    }

    /// Une redirection est-elle suivie (manifeste et installateur) ? `rank` = rang de cette
    /// redirection (1 pour la première) : au plus `MAX_REDIRECTS`. HTTPS à chaque saut. AUCUNE liste
    /// d'hôtes sur les sauts suivants : la signature (clé embarquée + version signée) protège le
    /// contenu, HTTPS protège le transport, et une liste ferait taire tous les clients au premier
    /// changement de domaine de stockage chez GitHub (ADR-0017).
    pub fn allows_redirect(&self, target: &Url, rank: usize) -> bool {
        rank <= MAX_REDIRECTS
            && (!self.redirects_https_only || target.scheme() == "https")
            && target.username().is_empty()
            && target.password().is_none()
            && target.host_str().is_some()
    }

    /// L'installateur n'est téléchargé qu'en HTTPS quand la source l'est.
    pub fn https_only(&self) -> bool {
        self.scheme == "https"
    }

    /// L'adresse est-elle une source permise d'installateur ?
    pub fn allows(&self, url: &Url) -> bool {
        url.scheme() == self.scheme
            && url.host_str() == Some(self.host.as_str())
            && url.port() == self.port
            && url.username().is_empty()
            && url.password().is_none()
            && url.path().starts_with(&self.path_prefix)
    }
}

/// Pourquoi une annonce n'est pas retenue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Rejection {
    /// Pas plus récente que la version en cours : ignorée, jamais de rétrogradation (ADR-0017).
    #[error("version pas plus récente")]
    NotNewer,
    /// Numéro de version, adresse ou signature inutilisables.
    #[error("annonce mal formée : {0}")]
    Malformed(&'static str),
    /// L'installateur ne vient pas du dépôt prévu, ou pas en HTTPS.
    #[error("source de téléchargement refusée")]
    UntrustedSource,
}

/// Vérifie une annonce avant de l'afficher et de la retenir.
pub fn validate_candidate(
    current: &str,
    candidate: &Candidate,
    policy: &DownloadPolicy,
) -> Result<Release, Rejection> {
    let current = semver::Version::parse(current)
        .map_err(|_| Rejection::Malformed("version du client illisible"))?;
    let version = semver::Version::parse(candidate.version.trim_start_matches('v'))
        .map_err(|_| Rejection::Malformed("numéro de version"))?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        // Ni préversion ni métadonnées de construction (`0.1.0+1` se classe au-dessus de `0.1.0`).
        return Err(Rejection::Malformed(
            "préversion ou métadonnées de construction",
        ));
    }
    if version <= current {
        return Err(Rejection::NotNewer);
    }
    let url = Url::parse(&candidate.download_url)
        .map_err(|_| Rejection::Malformed("adresse de téléchargement"))?;
    if !policy.allows(&url) {
        return Err(Rejection::UntrustedSource);
    }
    let signature = candidate.signature.trim();
    if signature.is_empty() || signature.len() > SIGNATURE_MAX_LEN {
        return Err(Rejection::Malformed("signature"));
    }
    Ok(Release {
        version: version.to_string(),
        notes: clean_notes(candidate.notes.as_deref().unwrap_or("")),
    })
}

/// Texte seul : caractères de contrôle retirés (hors saut de ligne et tabulation), espaces de
/// bord retirés, longueur bornée.
pub fn clean_notes(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect();
    cleaned.trim().chars().take(NOTES_MAX_CHARS).collect()
}

/// Une vérification automatique est-elle permise (BR-UPDATE-001) ? Jamais deux en moins de 24 h.
/// Une dernière tentative « dans le futur » (horloge remise à l'heure) ne peut pas être vraie :
/// on vérifie.
pub fn check_is_due(now: i64, last_attempt_at: Option<i64>) -> bool {
    match last_attempt_at {
        None => true,
        Some(last) if last > now => true,
        Some(last) => now - last >= CHECK_INTERVAL_MS,
    }
}

/// Tentatives automatiques encore dans la fenêtre glissante de 24 h.
pub fn attempts_in_window(now: i64, attempts: &[i64]) -> usize {
    attempts
        .iter()
        .filter(|at| **at <= now && now - **at < CHECK_INTERVAL_MS)
        .count()
}

/// Une vérification automatique est-elle permise ? La règle des 24 h depuis la dernière requête
/// (`check_is_due`) ET le plafond dur de tentatives par 24 h glissantes.
pub fn automatic_check_allowed(now: i64, last_request_at: Option<i64>, attempts: &[i64]) -> bool {
    check_is_due(now, last_request_at)
        && attempts_in_window(now, attempts) < MAX_AUTOMATIC_ATTEMPTS_PER_DAY
}

/// Les tentatives de la fenêtre, plus celle de `now`.
pub fn with_attempt(now: i64, attempts: &[i64]) -> Vec<i64> {
    let mut kept: Vec<i64> = attempts
        .iter()
        .copied()
        .filter(|at| *at <= now && now - *at < CHECK_INTERVAL_MS)
        .collect();
    kept.push(now);
    kept
}

/// Le bandeau est-il masqué par un « Plus tard » (BR-UPDATE-006) ? Un report ne dépasse jamais
/// 24 h à partir de maintenant : une date plus lointaine (horloge reculée) est périmée.
pub fn is_postponed(now: i64, postponed_until: Option<i64>) -> bool {
    postponed_until.is_some_and(|until| now < until && until - now <= POSTPONE_MS)
}

pub fn postponed_until(now: i64) -> i64 {
    now.saturating_add(POSTPONE_MS)
}

/// Le bandeau « Nouvelle version disponible » est-il visible (BR-UPDATE-003) ?
pub fn banner_visible(record: &UpdateRecord, now: i64) -> bool {
    record.available.is_some() && !is_postponed(now, record.postponed_until)
}

/// Après une mise à jour, la version retenue peut être celle qui tourne : on l'oublie.
pub fn forget_installed(record: &mut UpdateRecord, current: &str) {
    let Ok(current) = semver::Version::parse(current) else {
        return;
    };
    let stale = record
        .available
        .as_ref()
        .and_then(|release| semver::Version::parse(&release.version).ok())
        .is_none_or(|version| version <= current);
    if stale {
        record.available = None;
    }
}

/// « Vérifier maintenant » est-il permis (pas de tentative dans les 30 dernières secondes) ?
pub fn manual_check_allowed(now: i64, last_attempt_at: Option<i64>) -> bool {
    match last_attempt_at {
        Some(last) if last <= now => now - last >= MANUAL_CHECK_MIN_INTERVAL_MS,
        _ => true,
    }
}
