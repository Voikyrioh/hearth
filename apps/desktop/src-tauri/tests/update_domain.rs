//! Règles pures de la mise à jour du client (BR-UPDATE-001 à 010) : fréquence, report,
//! validation d'une annonce. Horloge injectée : on passe l'instant, on n'attend jamais.
#![allow(clippy::unwrap_used, clippy::expect_used)] // tests : les helpers peuvent paniquer

use hearth_desktop_lib::update::domain::{
    CHECK_INTERVAL_MS, Candidate, DownloadPolicy, FEED_URL, MAX_AUTOMATIC_ATTEMPTS_PER_DAY,
    MAX_REDIRECTS, NOTES_MAX_CHARS, POSTPONE_MS, Rejection, Release, UpdateRecord,
    attempts_in_window, automatic_check_allowed, banner_visible, check_is_due, clean_notes,
    forget_installed, is_postponed, manual_check_allowed, postponed_until, validate_candidate,
    with_attempt,
};

const HOUR: i64 = 60 * 60 * 1000;
const NOW: i64 = 1_800_000_000_000;

#[test]
fn the_first_check_is_always_allowed() {
    assert!(check_is_due(NOW, None));
}

#[test]
fn a_second_automatic_check_waits_a_full_day() {
    assert!(!check_is_due(NOW, Some(NOW)));
    assert!(!check_is_due(NOW, Some(NOW - 23 * HOUR)));
    assert!(!check_is_due(NOW, Some(NOW - CHECK_INTERVAL_MS + 1)));
    assert!(check_is_due(NOW, Some(NOW - CHECK_INTERVAL_MS)));
    assert!(check_is_due(NOW, Some(NOW - 5 * CHECK_INTERVAL_MS)));
}

#[test]
fn a_last_check_in_the_future_means_the_clock_was_corrected() {
    assert!(check_is_due(NOW, Some(NOW + 3 * HOUR)));
}

#[test]
fn a_manual_check_waits_thirty_seconds_after_the_previous_attempt() {
    assert!(manual_check_allowed(NOW, None));
    assert!(!manual_check_allowed(NOW, Some(NOW - 29_000)));
    assert!(manual_check_allowed(NOW, Some(NOW - 30_000)));
    assert!(manual_check_allowed(NOW, Some(NOW + HOUR)));
}

#[test]
fn redirects_are_https_at_every_rank_bounded_exactly_and_not_limited_to_hosts() {
    let policy = DownloadPolicy::github_releases();
    let ok = |url: &str, rank: usize| policy.allows_redirect(&url::Url::parse(url).unwrap(), rank);
    // Le rang 1 est la première redirection : l'adresse de départ n'est pas comptée.
    for rank in 1..=MAX_REDIRECTS {
        assert!(ok("https://github.com/x", rank), "rang {rank}");
    }
    assert!(!ok("https://github.com/x", MAX_REDIRECTS + 1));
    assert_eq!(MAX_REDIRECTS, 5, "marge réelle : GitHub en fait déjà 2");
    // HTTPS à n'importe quel rang.
    for rank in 1..=MAX_REDIRECTS {
        assert!(!ok("http://github.com/x", rank), "http au rang {rank}");
        assert!(!ok("http://objects.githubusercontent.com/x", rank));
    }
    // Aucune liste d'hôtes sur les sauts : un autre hôte en HTTPS est suivi (la signature et HTTPS
    // protègent, une liste ferait taire le parc au premier changement de stockage chez GitHub).
    assert!(ok("https://release-assets.githubusercontent.com/x", 2));
    assert!(ok("https://un-autre-stockage.example/x", 2));
    // Pas d'identifiants dans l'adresse.
    assert!(!ok("https://user:pw@github.com/x", 1));
    assert!(policy.https_only());
}

#[test]
fn at_most_three_automatic_attempts_per_sliding_day_whatever_their_outcome() {
    let day = 24 * HOUR;
    assert_eq!(MAX_AUTOMATIC_ATTEMPTS_PER_DAY, 3);
    assert!(automatic_check_allowed(NOW, None, &[]));
    // Trois tentatives dans la fenêtre : plus aucune, même sans requête partie.
    let three = [NOW - 5 * HOUR, NOW - 3 * HOUR, NOW - HOUR];
    assert_eq!(attempts_in_window(NOW, &three), 3);
    assert!(!automatic_check_allowed(NOW, None, &three));
    // La fenêtre glisse : la plus ancienne sort 24 h après elle.
    assert!(!automatic_check_allowed(NOW + 18 * HOUR, None, &three));
    assert!(automatic_check_allowed(NOW + 19 * HOUR, None, &three));
    // Une tentative future (horloge corrigée) ne compte pas.
    assert_eq!(attempts_in_window(NOW, &[NOW + HOUR]), 0);
    // La règle des 24 h depuis la dernière requête reste valable en plus.
    assert!(!automatic_check_allowed(NOW, Some(NOW - HOUR), &[]));
    assert!(automatic_check_allowed(NOW, Some(NOW - day), &[]));
    // On garde la fenêtre et on y ajoute l'instant.
    let kept = with_attempt(NOW, &[NOW - 2 * day, NOW - HOUR]);
    assert_eq!(kept, [NOW - HOUR, NOW]);
}

#[test]
fn build_metadata_is_refused_like_a_prerelease() {
    // `0.1.0+1` se classe au-dessus de `0.1.0` pour `semver` (vérifié ici) : la version en cours se
    // reproposerait.
    assert!(semver::Version::parse("0.1.0+1").unwrap() > semver::Version::parse("0.1.0").unwrap());
    assert!(matches!(
        validate_candidate("0.1.0", &candidate("0.1.0+1", GOOD_URL), &policy()),
        Err(Rejection::Malformed(_))
    ));
}

#[test]
fn later_hides_the_banner_for_exactly_a_day() {
    let until = postponed_until(NOW);
    assert_eq!(until - NOW, POSTPONE_MS);
    assert!(is_postponed(NOW, Some(until)));
    assert!(is_postponed(NOW + 23 * HOUR, Some(until)));
    assert!(!is_postponed(until, Some(until)));
    assert!(!is_postponed(until + 1, Some(until)));
    assert!(!is_postponed(NOW, None));
}

#[test]
fn a_postponement_further_than_a_day_away_is_stale() {
    // Horloge reculée après un « Plus tard » : on ne garde pas un bandeau masqué des mois.
    assert!(!is_postponed(NOW, Some(NOW + 10 * POSTPONE_MS)));
}

fn release(version: &str) -> Release {
    Release {
        version: version.to_owned(),
        notes: String::new(),
    }
}

#[test]
fn the_banner_needs_a_known_release_and_no_active_postponement() {
    let mut record = UpdateRecord::default();
    assert!(!banner_visible(&record, NOW));
    record.available = Some(release("1.1.0"));
    assert!(banner_visible(&record, NOW));
    record.postponed_until = Some(postponed_until(NOW));
    assert!(!banner_visible(&record, NOW));
    assert!(banner_visible(&record, NOW + POSTPONE_MS));
}

#[test]
fn a_release_already_installed_is_forgotten() {
    let mut record = UpdateRecord {
        available: Some(release("1.1.0")),
        ..UpdateRecord::default()
    };
    forget_installed(&mut record, "1.0.0");
    assert!(record.available.is_some());
    forget_installed(&mut record, "1.1.0");
    assert!(record.available.is_none());
    record.available = Some(release("1.1.0"));
    forget_installed(&mut record, "2.0.0");
    assert!(record.available.is_none());
}

fn policy() -> DownloadPolicy {
    DownloadPolicy::github_releases()
}

fn candidate(version: &str, url: &str) -> Candidate {
    Candidate {
        version: version.to_owned(),
        notes: Some("Corrections.".to_owned()),
        download_url: url.to_owned(),
        signature: "c2lnbmF0dXJl".to_owned(),
    }
}

const GOOD_URL: &str =
    "https://github.com/Voikyrioh/hearth/releases/download/v1.1.0/Hearth_1.1.0_x64-setup.exe";

#[test]
fn a_newer_release_from_the_repository_is_accepted() {
    let accepted = validate_candidate("1.0.0", &candidate("1.1.0", GOOD_URL), &policy()).unwrap();
    assert_eq!(accepted.version, "1.1.0");
    assert_eq!(accepted.notes, "Corrections.");
    // Un « v » devant le numéro est toléré (étiquette de release).
    assert_eq!(
        validate_candidate("1.0.0", &candidate("v1.1.0", GOOD_URL), &policy())
            .unwrap()
            .version,
        "1.1.0"
    );
}

#[test]
fn the_same_or_an_older_version_is_ignored_never_a_downgrade() {
    for version in ["1.0.0", "0.9.9", "0.1.0"] {
        assert_eq!(
            validate_candidate("1.0.0", &candidate(version, GOOD_URL), &policy()),
            Err(Rejection::NotNewer),
            "{version}"
        );
    }
}

#[test]
fn a_prerelease_is_never_offered() {
    assert!(matches!(
        validate_candidate("1.0.0", &candidate("2.0.0-beta.1", GOOD_URL), &policy()),
        Err(Rejection::Malformed(_))
    ));
}

#[test]
fn an_installer_must_come_from_the_repository_over_https() {
    for url in [
        "http://github.com/Voikyrioh/hearth/releases/download/v1.1.0/x.exe",
        "https://example.com/Voikyrioh/hearth/releases/download/v1.1.0/x.exe",
        "https://github.com/Autre/hearth/releases/download/v1.1.0/x.exe",
        "https://github.com/Voikyrioh/hearth/archive/main.zip",
        "https://user:pass@github.com/Voikyrioh/hearth/releases/download/v1.1.0/x.exe",
        "https://github.com:8443/Voikyrioh/hearth/releases/download/v1.1.0/x.exe",
        "https://github.com.evil.test/Voikyrioh/hearth/releases/download/v1.1.0/x.exe",
        "file:///C:/Windows/System32/cmd.exe",
    ] {
        assert_eq!(
            validate_candidate("1.0.0", &candidate("1.1.0", url), &policy()),
            Err(Rejection::UntrustedSource),
            "{url}"
        );
    }
}

#[test]
fn a_malformed_announcement_is_refused() {
    for bad in [
        candidate("pas-une-version", GOOD_URL),
        candidate("1.1.0", "pas une adresse"),
        Candidate {
            signature: "   ".to_owned(),
            ..candidate("1.1.0", GOOD_URL)
        },
        Candidate {
            signature: "x".repeat(5_000),
            ..candidate("1.1.0", GOOD_URL)
        },
    ] {
        assert!(matches!(
            validate_candidate("1.0.0", &bad, &policy()),
            Err(Rejection::Malformed(_))
        ));
    }
}

#[test]
fn notes_are_plain_bounded_text() {
    assert_eq!(
        clean_notes("  Titre\n- un\n- deux\t \n"),
        "Titre\n- un\n- deux"
    );
    assert_eq!(clean_notes("a\u{0}b\u{7}c\u{1b}[31m"), "abc[31m");
    let long = "é".repeat(NOTES_MAX_CHARS + 50);
    assert_eq!(clean_notes(&long).chars().count(), NOTES_MAX_CHARS);
    let announced = validate_candidate(
        "1.0.0",
        &Candidate {
            notes: None,
            ..candidate("1.1.0", GOOD_URL)
        },
        &policy(),
    )
    .unwrap();
    assert_eq!(announced.notes, "");
}

#[test]
fn the_feed_address_is_a_fixed_https_address_of_the_public_repository() {
    let url = url::Url::parse(FEED_URL).unwrap();
    assert_eq!(url.scheme(), "https");
    assert_eq!(url.host_str(), Some("github.com"));
    assert!(url.path().starts_with("/Voikyrioh/hearth/releases/"));
}

#[test]
fn the_record_survives_a_json_round_trip_and_tolerates_missing_fields() {
    let record = UpdateRecord {
        last_attempt_at: Some(NOW),
        last_request_at: Some(NOW),
        automatic_attempts: vec![NOW - 1, NOW],
        last_success_at: Some(NOW - 1),
        postponed_until: Some(NOW + HOUR),
        available: Some(Release {
            version: "1.1.0".to_owned(),
            notes: "n".to_owned(),
        }),
    };
    let text = serde_json::to_string(&record).unwrap();
    assert_eq!(serde_json::from_str::<UpdateRecord>(&text).unwrap(), record);
    assert_eq!(
        serde_json::from_str::<UpdateRecord>("{}").unwrap(),
        UpdateRecord::default()
    );
}
