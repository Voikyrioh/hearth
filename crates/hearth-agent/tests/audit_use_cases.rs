//! Le journal d'activité à travers les cas d'usage, sur une vraie base temporaire : les entrées
//! de la gestion des comptes et de la connexion sont écrites dans la transaction de l'action,
//! diffusées une fois validée, lues par les administrateurs seulement, purgées par âge puis par
//! nombre.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use hearth_agent::application::audit::{AuditError, EXPORT_LIMIT};
use hearth_agent::application::ports::AuditFeed;
use hearth_agent::application::sessions::LoginError;
use hearth_agent::domain::accounts::{Role, Username};
use hearth_agent::domain::audit::{
    Actor, AuditAction, AuditFilter, AuditRecord, MAX_ENTRIES, Origin, OriginKind, Outcome,
    OutcomeKind, RawFilter, Reason, Target,
};
use support::{PASSWORD, by, client, client_at, env, secret, start_time};

const WRONG: &str = "Wrong-Horse-9999";

async fn all(env: &support::Env) -> Vec<AuditRecord> {
    let filter = AuditFilter::new(RawFilter::default()).unwrap();
    let mut records = env
        .audit
        .search(Role::Admin, &filter)
        .await
        .unwrap()
        .records;
    records.reverse();
    records
}

fn compact(record: &AuditRecord) -> (String, String, Option<String>, Option<String>) {
    (
        record.action.clone(),
        record.outcome.code().to_owned(),
        record.account.clone(),
        record.target.clone(),
    )
}

#[tokio::test]
async fn account_management_writes_one_entry_per_action_with_who_where_and_what() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    let paul = env.create("paul", Role::ReadOnly).await;
    env.service
        .change_role(&paul.id, Role::Admin, by())
        .await
        .unwrap();
    env.service
        .set_password(&paul.id, secret("Another-Pass-77"), by())
        .await
        .unwrap();
    env.service
        .change_own_password(
            &marie.id,
            secret(PASSWORD),
            secret("Brand-New-Pass-7"),
            None,
            by(),
        )
        .await
        .unwrap();
    env.service.revoke_sessions(&paul.id, by()).await.unwrap();
    env.service
        .delete(&paul.id, Some(&marie.id), None, by())
        .await
        .unwrap();

    let records = all(&env).await;
    let rows: Vec<_> = records.iter().map(compact).collect();
    let root = Some("root".to_owned());
    assert_eq!(
        rows,
        [
            (
                "account.create".into(),
                "ok".into(),
                root.clone(),
                Some("marie".into())
            ),
            (
                "account.create".into(),
                "ok".into(),
                root.clone(),
                Some("paul".into())
            ),
            (
                "account.role".into(),
                "ok".into(),
                root.clone(),
                Some("paul (Administrateur)".into())
            ),
            (
                "account.password".into(),
                "ok".into(),
                root.clone(),
                Some("paul".into())
            ),
            (
                "account.password.own".into(),
                "ok".into(),
                root.clone(),
                Some("marie".into())
            ),
            (
                "sessions.revoke".into(),
                "ok".into(),
                root.clone(),
                Some("paul".into())
            ),
            (
                "account.delete".into(),
                "ok".into(),
                root,
                Some("paul".into())
            ),
        ]
    );
    // L'origine est celle de la demande : adresse et nom du poste.
    for record in &records {
        assert_eq!(record.origin_kind, OriginKind::Client);
        assert_eq!(record.origin_addr.as_deref(), Some(support::CLIENT_ADDR));
        assert_eq!(record.origin_name.as_deref(), Some("poste/1.0"));
        assert_eq!(record.reason, None);
    }
    // Chaque entrée est datée de l'horloge du service.
    assert!(records.iter().all(|record| record.at == start_time()));
}

#[tokio::test]
async fn an_action_that_fails_leaves_no_success_entry() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    let before = all(&env).await.len();
    // Identifiant déjà pris, dernier administrateur, mot de passe faible, compte inconnu.
    env.service
        .create("MARIE", secret(PASSWORD), Role::Admin, by())
        .await
        .unwrap_err();
    env.service
        .change_role(&marie.id, Role::ReadOnly, by())
        .await
        .unwrap_err();
    env.service
        .delete(&marie.id, None, None, by())
        .await
        .unwrap_err();
    env.service
        .create("paul", secret("faible"), Role::Admin, by())
        .await
        .unwrap_err();
    env.service
        .revoke_sessions(
            &hearth_agent::domain::accounts::AccountId::new("inconnu"),
            by(),
        )
        .await
        .unwrap_err();
    assert_eq!(all(&env).await.len(), before);
}

#[tokio::test]
async fn an_entry_is_published_once_the_action_is_committed_and_never_before_or_for_a_failure() {
    let env = env().await;
    let mut feed = env.feed.subscribe();
    env.service
        .create("marie", secret("faible"), Role::Admin, by())
        .await
        .unwrap_err();
    assert!(feed.try_recv().is_err(), "rien pour une action échouée");

    env.create("marie", Role::Admin).await;
    let published = feed.try_recv().unwrap();
    let stored = all(&env).await;
    assert_eq!(
        published, stored[0],
        "l'entrée diffusée est celle qui est écrite"
    );
    assert!(feed.try_recv().is_err());
}

#[tokio::test]
async fn a_successful_login_and_a_logout_are_journaled_for_the_account() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let outcome = env
        .sessions
        .login("MARIE", secret(PASSWORD), &client())
        .await
        .unwrap();
    let marie = Actor::new(
        Some(Username::parse("marie").unwrap()),
        Origin::client(Some("poste/1.0"), support::CLIENT_ADDR),
    );
    env.sessions
        .logout(&outcome.session_id, &marie)
        .await
        .unwrap();

    let records = all(&env).await;
    let rows: Vec<_> = records.iter().skip(1).map(compact).collect();
    assert_eq!(
        rows,
        [
            ("login".into(), "ok".into(), Some("marie".into()), None),
            ("logout".into(), "ok".into(), Some("marie".into()), None),
        ]
    );
    assert_eq!(records[1].origin_text(), "10.0.0.7 (poste/1.0)");
}

#[tokio::test]
async fn a_refused_login_is_journaled_without_account_and_says_the_same_thing_for_every_cause() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let base = all(&env).await.len();

    // Mot de passe faux sur un compte qui existe, identifiant qui n'existe pas.
    env.sessions
        .login("marie", secret(WRONG), &client())
        .await
        .unwrap_err();
    env.sessions
        .login("fantome", secret(WRONG), &client())
        .await
        .unwrap_err();
    // Un mot de passe tapé à la place de l'identifiant : format impossible, valeur jamais retenue.
    env.sessions
        .login(
            "Mon Mot De Passe Secret 1!",
            secret(WRONG),
            &client_at("10.0.0.9"),
        )
        .await
        .unwrap_err();

    let records = all(&env).await;
    let refused = &records[base..];
    assert_eq!(refused.len(), 3);
    for record in refused {
        assert_eq!(record.action, "login");
        assert_eq!(record.outcome, OutcomeKind::Denied);
        assert_eq!(
            record.account, None,
            "l'identifiant saisi n'est jamais retenu"
        );
        assert_eq!(record.target, None);
    }
    // BR-AUDIT-006 : même raison, que l'identifiant existe ou non.
    assert_eq!(refused[0].reason, refused[1].reason);
    assert_eq!(
        refused[0].reason.as_deref(),
        Some("identifiants incorrects")
    );
    assert_eq!(refused[2].reason.as_deref(), Some("identifiant invalide"));
    assert_eq!(refused[2].origin_addr.as_deref(), Some("10.0.0.9"));
    // Nulle part dans les entrées : ni le mot de passe, ni l'identifiant saisi.
    let dump = format!("{refused:?}");
    for secret_value in [WRONG, "Mon Mot De Passe Secret", "fantome"] {
        assert!(!dump.contains(secret_value), "{secret_value}");
    }
}

#[tokio::test]
async fn the_attempt_that_locks_adds_a_lock_entry_and_attempts_during_the_wait_add_nothing() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let base = all(&env).await.len();
    for _ in 0..4 {
        env.sessions
            .login("marie", secret(WRONG), &client())
            .await
            .unwrap_err();
    }
    assert_eq!(all(&env).await.len(), base + 4, "pas encore de blocage");

    let locking = env
        .sessions
        .login("marie", secret(WRONG), &client())
        .await
        .unwrap_err();
    assert!(
        matches!(locking, LoginError::TooManyAttempts { .. }),
        "{locking:?}"
    );
    let records = all(&env).await;
    let tail: Vec<_> = records[base + 4..].iter().map(compact).collect();
    assert_eq!(
        tail,
        [
            ("login".into(), "denied".into(), None, None),
            ("login.locked".into(), "denied".into(), None, None),
        ]
    );
    assert_eq!(
        records.last().unwrap().reason.as_deref(),
        Some("trop de tentatives, attente de 60 s")
    );

    // Pendant l'attente : refusées avant toute vérification, sans entrée (le journal ne se
    // remplit pas d'une rafale contre un compte bloqué).
    for _ in 0..3 {
        let blocked = env
            .sessions
            .login("marie", secret(WRONG), &client())
            .await
            .unwrap_err();
        assert!(matches!(blocked, LoginError::TooManyAttempts { .. }));
    }
    assert_eq!(all(&env).await.len(), base + 6);
}

#[tokio::test]
async fn only_an_administrator_reads_the_journal() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let filter = AuditFilter::new(RawFilter::default()).unwrap();
    let role = Role::ReadOnly;
    assert!(matches!(
        env.audit.search(role, &filter).await,
        Err(AuditError::Forbidden)
    ));
    assert!(matches!(
        env.audit.export(role, &filter).await,
        Err(AuditError::Forbidden)
    ));
    assert!(matches!(
        env.audit.subscribe(role),
        Err(AuditError::Forbidden)
    ));
    assert!(env.audit.search(Role::Admin, &filter).await.is_ok());
    assert!(env.audit.subscribe(Role::Admin).is_ok());
}

#[tokio::test]
async fn a_page_tells_where_the_next_one_starts() {
    let env = env().await;
    for n in 0..5 {
        env.create(&format!("compte{n}"), Role::ReadOnly).await;
    }
    let first_filter = AuditFilter::new(RawFilter {
        limit: Some(2),
        ..RawFilter::default()
    })
    .unwrap();
    let first = env.audit.search(Role::Admin, &first_filter).await.unwrap();
    assert_eq!(first.records.len(), 2);
    let cursor = first.next_before.expect("il en reste");
    assert_eq!(cursor, first.records[1].id);

    let second_filter = AuditFilter::new(RawFilter {
        limit: Some(2),
        before: Some(cursor),
        ..RawFilter::default()
    })
    .unwrap();
    let second = env.audit.search(Role::Admin, &second_filter).await.unwrap();
    assert_eq!(second.records.len(), 2);
    assert!(second.next_before.is_some());

    let last_filter = AuditFilter::new(RawFilter {
        limit: Some(2),
        before: second.next_before,
        ..RawFilter::default()
    })
    .unwrap();
    let last = env.audit.search(Role::Admin, &last_filter).await.unwrap();
    assert_eq!(last.records.len(), 1);
    assert_eq!(last.next_before, None, "dernière page");

    // Une page pile : pas de curseur.
    let exact = AuditFilter::new(RawFilter {
        limit: Some(5),
        ..RawFilter::default()
    })
    .unwrap();
    assert_eq!(
        env.audit
            .search(Role::Admin, &exact)
            .await
            .unwrap()
            .next_before,
        None
    );
}

#[tokio::test]
async fn the_export_is_the_filtered_result_as_a_spreadsheet_file() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    env.create("paul", Role::ReadOnly).await;
    let filter = AuditFilter::new(RawFilter {
        text: Some("paul".into()),
        // Le curseur et la page ne comptent pas pour un export.
        before: Some(1),
        limit: Some(1),
        ..RawFilter::default()
    })
    .unwrap();
    let export = env.audit.export(Role::Admin, &filter).await.unwrap();
    assert!(!export.truncated);
    assert!(export.csv.starts_with('\u{feff}'));
    let lines: Vec<&str> = export.csv.trim_start_matches('\u{feff}').lines().collect();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].starts_with("Date et heure;Compte;"));
    assert!(
        lines[1].contains(";Création de compte;paul;Réussi;"),
        "{}",
        lines[1]
    );
}

/// Insère `count` entrées d'un coup, datées de `at`.
async fn flood(env: &support::Env, count: i64, at: &str) {
    sqlx::query(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < ?1)
         INSERT INTO audit_events (at, account, origin_kind, action, action_label, outcome)
         SELECT ?2, 'marie', 'cli', 'login', 'Connexion', 'ok' FROM n",
    )
    .bind(count)
    .bind(at)
    .execute(env.db.pool())
    .await
    .unwrap();
}

#[tokio::test]
async fn an_export_stops_at_its_cap_and_says_so() {
    let env = env().await;
    flood(
        &env,
        i64::try_from(EXPORT_LIMIT).unwrap() + 5,
        "2026-09-21T14:13:20.000Z",
    )
    .await;
    let filter = AuditFilter::new(RawFilter::default()).unwrap();
    let export = env.audit.export(Role::Admin, &filter).await.unwrap();
    assert!(export.truncated);
    let lines = export.csv.trim_start_matches('\u{feff}').lines().count();
    assert_eq!(lines, EXPORT_LIMIT + 1, "l'en-tête et le plafond");
}

#[tokio::test]
async fn the_hourly_purge_removes_by_age_then_by_count() {
    let env = env().await;
    // 40 entrées de plus de 90 jours, puis le plafond + 25 entrées récentes.
    flood(&env, 40, "2026-01-01T00:00:00.000Z").await;
    let recent = i64::try_from(MAX_ENTRIES).unwrap() + 25;
    flood(&env, recent, "2026-09-21T14:13:20.000Z").await;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(total, 40 + recent);

    let report = env.maintenance.purge().await.unwrap();
    assert_eq!(report.audit_events, 40 + 25, "l'âge, puis le nombre");
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(left, i64::try_from(MAX_ENTRIES).unwrap());
    // Les plus anciennes sont parties, les plus récentes restent.
    let oldest: i64 = sqlx::query_scalar("SELECT MIN(id) FROM audit_events")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(oldest, 40 + 25 + 1);
    let old_left: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE at < '2026-02-01'")
            .fetch_one(env.db.pool())
            .await
            .unwrap();
    assert_eq!(old_left, 0);

    // Idempotente.
    assert_eq!(env.maintenance.purge().await.unwrap().audit_events, 0);
}

#[tokio::test]
async fn the_purge_keeps_entries_of_exactly_ninety_days_and_removes_older_ones() {
    let env = env().await;
    // start_time = 2026-09-21T14:13:20Z ; 90 jours avant = 2026-06-23T14:13:20Z.
    flood(&env, 2, "2026-06-23T14:13:19.999Z").await;
    flood(&env, 3, "2026-06-23T14:13:20.000Z").await;
    let report = env.maintenance.purge().await.unwrap();
    assert_eq!(report.audit_events, 2);
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(left, 3);
}

#[tokio::test]
async fn the_sink_writes_and_publishes_and_a_failing_write_never_fails_the_caller() {
    let env = env().await;
    let mut feed = env.feed.subscribe();
    let actor = Actor::new(
        Some(Username::parse("marie").unwrap()),
        Origin::client(Some("poste"), "10.0.0.7"),
    );
    let record = |actor: Actor| {
        env.audit_sink.record(
            actor,
            AuditAction::AuditRead,
            Target::Route("/audit"),
            Outcome::Denied(Reason::ReadOnly),
        )
    };
    record(actor.clone()).await;
    let stored = all(&env).await;
    assert_eq!(stored.len(), 1);
    assert_eq!(feed.try_recv().unwrap(), stored[0]);

    // Base indisponible : l'écriture échoue, l'appelant continue (tracé en `error`).
    env.db.pool().close().await;
    record(actor).await;
    assert!(
        feed.try_recv().is_err(),
        "rien de diffusé pour une entrée non écrite"
    );
}

#[tokio::test]
async fn no_event_ever_holds_a_password_or_a_token_after_a_full_scenario() {
    let env = env().await;
    let secrets = [
        PASSWORD,
        WRONG,
        "Another-Pass-77",
        "Brand-New-Pass-7",
        "Typed-As-Name-42x",
    ];
    let marie = env.create("marie", Role::Admin).await;
    let paul = env.create("paul", Role::ReadOnly).await;
    let outcome = env
        .sessions
        .login("marie", secret(PASSWORD), &client())
        .await
        .unwrap();
    let token = outcome.token.encode();
    env.sessions
        .login("marie", secret(WRONG), &client())
        .await
        .unwrap_err();
    env.sessions
        .login("Typed-As-Name-42x", secret("Typed-As-Name-42x"), &client())
        .await
        .unwrap_err();
    env.service
        .set_password(&paul.id, secret("Another-Pass-77"), by())
        .await
        .unwrap();
    env.service
        .change_own_password(
            &marie.id,
            secret(PASSWORD),
            secret("Brand-New-Pass-7"),
            None,
            by(),
        )
        .await
        .unwrap();
    for _ in 0..5 {
        let _ = env
            .sessions
            .login("typed-as-name-42x", secret(WRONG), &client())
            .await;
    }
    let actor = Actor::new(
        Some(Username::parse("marie").unwrap()),
        Origin::client(Some("poste/1.0"), support::CLIENT_ADDR),
    );
    env.sessions
        .logout(&outcome.session_id, &actor)
        .await
        .unwrap();

    // Toute la table, colonne par colonne, et la table de recherche.
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT COALESCE(at,'') || '|' || COALESCE(account,'') || '|' || origin_kind || '|' ||
                COALESCE(origin_name,'') || '|' || COALESCE(origin_addr,'') || '|' || action || '|' ||
                action_label || '|' || COALESCE(target,'') || '|' || outcome || '|' ||
                COALESCE(reason,'')
         FROM audit_events",
    )
    .fetch_all(env.db.pool())
    .await
    .unwrap();
    assert!(rows.len() >= 10, "{}", rows.len());
    let fts: Vec<(String,)> = sqlx::query_as(
        "SELECT COALESCE(account,'') || ' ' || COALESCE(origin_name,'') || ' ' ||
                COALESCE(origin_addr,'') || ' ' || COALESCE(action_label,'') || ' ' ||
                COALESCE(target,'') || ' ' || COALESCE(reason,'')
         FROM audit_fts",
    )
    .fetch_all(env.db.pool())
    .await
    .unwrap();
    // Et ce que lit un administrateur, export compris.
    let filter = AuditFilter::new(RawFilter::default()).unwrap();
    let page = format!(
        "{:?}",
        env.audit.search(Role::Admin, &filter).await.unwrap()
    );
    let export = env.audit.export(Role::Admin, &filter).await.unwrap().csv;
    let mut haystacks: Vec<String> = rows.iter().map(|row| row.0.clone()).collect();
    haystacks.extend(fts.into_iter().map(|row| row.0));
    haystacks.push(page);
    haystacks.push(export);
    for haystack in &haystacks {
        for value in secrets.iter().copied().chain(["$argon2"]) {
            assert!(!haystack.contains(value), "{value} dans {haystack}");
        }
        assert!(!haystack.contains(&token), "jeton dans {haystack}");
        assert!(
            !haystack.to_lowercase().contains("typed-as-name"),
            "identifiant saisi dans {haystack}"
        );
    }
    // Le jeton n'a jamais non plus la moindre place dans la base du journal, hachée ou non.
    let token_hash = outcome.token.hash().to_hex();
    for haystack in &rows {
        assert!(!haystack.0.contains(&token_hash));
    }
}
