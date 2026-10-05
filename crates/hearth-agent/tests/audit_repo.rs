//! Dépôt SQLite du journal d'activité sur une base temporaire : écriture, filtres combinés,
//! curseur, recherche plein texte (caractères spéciaux compris), conservation, immutabilité.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use hearth_agent::application::ports::{AuditRepo, Store};
use hearth_agent::domain::accounts::{Role, Username};
use hearth_agent::domain::audit::{
    Actor, AuditAction, AuditEvent, AuditFilter, AuditRecord, Origin, OriginKind, Outcome,
    OutcomeKind, RawFilter, Reason, Target,
};
use hearth_agent::infrastructure::sqlite::{SqliteAuditRepo, SqliteStore};
use support::{env, start_time};
use time::Duration;

struct Written {
    env: support::Env,
    repo: SqliteAuditRepo,
}

async fn written() -> Written {
    let env = env().await;
    let repo = SqliteAuditRepo::new(env.db.pool().clone());
    Written { env, repo }
}

fn event(
    seconds: i64,
    account: Option<&str>,
    action: AuditAction,
    outcome: Outcome,
    host: &str,
    addr: &str,
) -> AuditEvent {
    AuditEvent::new(
        start_time() + Duration::seconds(seconds),
        Actor::new(
            account.map(|name| Username::parse(name).unwrap()),
            Origin::client(Some(host), addr),
        ),
        action,
        Target::None,
        outcome,
    )
}

fn simple(seconds: i64) -> AuditEvent {
    event(
        seconds,
        Some("marie"),
        AuditAction::Login,
        Outcome::Succeeded,
        "poste-de-marie",
        "10.0.0.7",
    )
}

impl Written {
    async fn write(&self, events: &[AuditEvent]) -> Vec<AuditRecord> {
        let store = SqliteStore::new(self.env.db.pool().clone());
        let mut tx = store.begin().await.unwrap();
        let mut records = Vec::new();
        for event in events {
            records.push(tx.audit().record(event).await.unwrap());
        }
        tx.commit().await.unwrap();
        records
    }

    async fn search(&self, raw: RawFilter) -> Vec<AuditRecord> {
        let filter = AuditFilter::new(raw).unwrap();
        self.repo.search(&filter, filter.limit).await.unwrap()
    }

    async fn count(&self) -> u64 {
        let store = SqliteStore::new(self.env.db.pool().clone());
        let mut tx = store.begin().await.unwrap();
        tx.audit().count().await.unwrap()
    }

    async fn ids(&self, raw: RawFilter) -> Vec<i64> {
        self.search(raw)
            .await
            .iter()
            .map(|record| record.id)
            .collect()
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn text(value: &str) -> RawFilter {
    RawFilter {
        text: Some(value.to_owned()),
        ..RawFilter::default()
    }
}

#[tokio::test]
async fn an_entry_is_written_and_read_back_as_written() {
    let w = written().await;
    let event = AuditEvent::new(
        start_time() + Duration::milliseconds(250),
        Actor::new(
            Some(Username::parse("marie").unwrap()),
            Origin::client(Some("poste-de-marie"), "10.0.0.7"),
        ),
        AuditAction::AccountRole,
        Target::AccountRole(Username::parse("paul").unwrap(), Role::ReadOnly),
        Outcome::Failed(Reason::LastAdmin),
    );
    let records = w.write(std::slice::from_ref(&event)).await;
    assert_eq!(records[0], event.into_record(records[0].id));

    let read = w.search(RawFilter::default()).await;
    assert_eq!(read, records);
    assert_eq!(read[0].target.as_deref(), Some("paul (Lecture seule)"));
    assert_eq!(read[0].reason.as_deref(), Some("dernier administrateur"));
    assert_eq!(read[0].origin_kind, OriginKind::Client);
}

#[tokio::test]
async fn the_command_line_and_an_unknown_host_are_stored_without_inventing_values() {
    let w = written().await;
    let cli = AuditEvent::new(
        start_time(),
        Actor::command_line(),
        AuditAction::AccountCreate,
        Target::Account(Username::parse("paul").unwrap()),
        Outcome::Succeeded,
    );
    let unknown_host = AuditEvent::new(
        start_time() + Duration::seconds(1),
        Actor::new(
            None,
            Origin::Client {
                name: None,
                addr: "10.0.0.9".into(),
            },
        ),
        AuditAction::Login,
        Target::None,
        Outcome::Denied(Reason::InvalidCredentials),
    );
    w.write(&[cli, unknown_host]).await;
    let read = w.search(RawFilter::default()).await;
    assert_eq!(read[0].origin_text(), "10.0.0.9 (inconnu)");
    assert_eq!(read[0].account, None);
    assert_eq!(read[1].origin_text(), "ligne de commande du serveur");
    assert_eq!(read[1].origin_name, None);
    assert_eq!(read[1].origin_addr, None);
}

#[tokio::test]
async fn entries_come_newest_first_and_ids_only_grow() {
    let w = written().await;
    let records = w.write(&[simple(1), simple(2), simple(3)]).await;
    assert!(records[0].id < records[1].id && records[1].id < records[2].id);
    let ids = w.ids(RawFilter::default()).await;
    assert_eq!(ids, [records[2].id, records[1].id, records[0].id]);
}

#[tokio::test]
async fn filters_combine_with_and_between_filters_and_or_inside_one() {
    let w = written().await;
    let marie_ok = event(
        1,
        Some("marie"),
        AuditAction::Login,
        Outcome::Succeeded,
        "a",
        "10.0.0.1",
    );
    let marie_denied = event(
        2,
        Some("marie"),
        AuditAction::AuditRead,
        Outcome::Denied(Reason::ReadOnly),
        "a",
        "10.0.0.1",
    );
    let paul_ok = event(
        3,
        Some("paul"),
        AuditAction::AccountCreate,
        Outcome::Succeeded,
        "b",
        "10.0.0.2",
    );
    let paul_failed = event(
        4,
        Some("paul"),
        AuditAction::AccountCreate,
        Outcome::Failed(Reason::UsernameTaken),
        "b",
        "10.0.0.2",
    );
    let anonymous = event(
        5,
        None,
        AuditAction::Login,
        Outcome::Denied(Reason::InvalidCredentials),
        "c",
        "10.0.0.3",
    );
    let r = w
        .write(&[marie_ok, marie_denied, paul_ok, paul_failed, anonymous])
        .await;
    let (marie_ok, marie_denied, paul_ok, paul_failed, anonymous) =
        (r[0].id, r[1].id, r[2].id, r[3].id, r[4].id);

    let by = |accounts: &[&str], actions: &[&str], outcomes: &[&str]| RawFilter {
        accounts: strings(accounts),
        actions: strings(actions),
        outcomes: strings(outcomes),
        ..RawFilter::default()
    };
    // Un filtre, plusieurs valeurs : OU.
    assert_eq!(
        w.ids(by(&["marie", "paul"], &[], &[])).await,
        [paul_failed, paul_ok, marie_denied, marie_ok]
    );
    assert_eq!(
        w.ids(by(&[], &["login", "audit.read"], &[])).await,
        [anonymous, marie_denied, marie_ok]
    );
    assert_eq!(
        w.ids(by(&[], &[], &["denied", "failed"])).await,
        [anonymous, paul_failed, marie_denied]
    );
    // Plusieurs filtres : ET.
    assert_eq!(
        w.ids(by(&["paul"], &["account.create"], &["failed"])).await,
        [paul_failed]
    );
    assert_eq!(
        w.ids(by(&["marie"], &[], &["failed"])).await,
        Vec::<i64>::new()
    );
    assert_eq!(
        w.ids(by(&["paul"], &["account.create"], &["ok", "failed"]))
            .await,
        [paul_failed, paul_ok]
    );
    // Insensible à la casse de la saisie du compte.
    assert_eq!(w.ids(by(&["MARIE"], &["login"], &[])).await, [marie_ok]);
    // Aucun filtre : tout.
    assert_eq!(w.ids(RawFilter::default()).await.len(), 5);
}

#[tokio::test]
async fn the_period_is_inclusive_at_both_ends() {
    let w = written().await;
    let r = w
        .write(&[simple(10), simple(20), simple(30), simple(40)])
        .await;
    let period = |from: i64, to: i64| RawFilter {
        from: Some(start_time() + Duration::seconds(from)),
        to: Some(start_time() + Duration::seconds(to)),
        ..RawFilter::default()
    };
    assert_eq!(w.ids(period(20, 30)).await, [r[2].id, r[1].id]);
    assert_eq!(w.ids(period(11, 29)).await, [r[1].id]);
    assert_eq!(w.ids(period(0, 5)).await, Vec::<i64>::new());
    let only_from = RawFilter {
        from: Some(start_time() + Duration::seconds(30)),
        ..RawFilter::default()
    };
    assert_eq!(w.ids(only_from).await, [r[3].id, r[2].id]);
    let only_to = RawFilter {
        to: Some(start_time() + Duration::seconds(10)),
        ..RawFilter::default()
    };
    assert_eq!(w.ids(only_to).await, [r[0].id]);
}

#[tokio::test]
async fn the_cursor_walks_all_the_pages_without_gap_or_repeat() {
    let w = written().await;
    let events: Vec<AuditEvent> = (0..250).map(simple).collect();
    let all = w.write(&events).await;

    let mut seen = Vec::new();
    let mut before = None;
    let mut pages = 0;
    loop {
        let page = w
            .search(RawFilter {
                before,
                ..RawFilter::default()
            })
            .await;
        if page.is_empty() {
            break;
        }
        assert!(page.len() <= 100);
        pages += 1;
        before = page.last().map(|record| record.id);
        seen.extend(page.into_iter().map(|record| record.id));
    }
    assert_eq!(pages, 3);
    let mut expected: Vec<i64> = all.iter().map(|record| record.id).collect();
    expected.reverse();
    assert_eq!(seen, expected);
}

#[tokio::test]
async fn the_cursor_and_the_filters_work_together() {
    let w = written().await;
    let mut events = Vec::new();
    for n in 0..30 {
        let (account, outcome) = if n % 3 == 0 {
            ("paul", Outcome::Denied(Reason::InvalidCredentials))
        } else {
            ("marie", Outcome::Succeeded)
        };
        events.push(event(
            n,
            Some(account),
            AuditAction::Login,
            outcome,
            "p",
            "10.0.0.1",
        ));
    }
    w.write(&events).await;
    let denied = RawFilter {
        outcomes: strings(&["denied"]),
        limit: Some(4),
        ..RawFilter::default()
    };
    let first = w.search(denied.clone()).await;
    assert_eq!(first.len(), 4);
    let second = w
        .search(RawFilter {
            before: first.last().map(|record| record.id),
            ..denied
        })
        .await;
    assert_eq!(second.len(), 4);
    assert!(second[0].id < first[3].id);
    assert!(
        first
            .iter()
            .chain(&second)
            .all(|r| r.outcome == OutcomeKind::Denied)
    );
}

#[tokio::test]
async fn the_search_finds_every_visible_field_regardless_of_case_and_accents() {
    let w = written().await;
    let login = event(
        1,
        Some("marie"),
        AuditAction::Login,
        Outcome::Succeeded,
        "poste-de-marie",
        "10.0.0.7",
    );
    let logout = event(
        2,
        Some("paul"),
        AuditAction::Logout,
        Outcome::Succeeded,
        "bureau",
        "192.168.1.20",
    );
    let read_denied = AuditEvent::new(
        start_time() + Duration::seconds(3),
        Actor::new(
            Some(Username::parse("lea").unwrap()),
            Origin::client(Some("tablette"), "10.0.0.8"),
        ),
        AuditAction::AuditRead,
        Target::Route("/audit"),
        Outcome::Denied(Reason::ReadOnly),
    );
    let created = AuditEvent::new(
        start_time() + Duration::seconds(4),
        Actor::command_line(),
        AuditAction::AccountCreate,
        Target::Account(Username::parse("nouveau-compte").unwrap()),
        Outcome::Succeeded,
    );
    let r = w.write(&[login, logout, read_denied, created]).await;
    let (login, logout, read_denied, created) = (r[0].id, r[1].id, r[2].id, r[3].id);

    // Compte, adresse, nom du poste, action, cible, raison.
    assert_eq!(w.ids(text("marie")).await, [login]);
    assert_eq!(w.ids(text("MARIE")).await, [login]);
    assert_eq!(w.ids(text("10.0.0.7")).await, [login]);
    assert_eq!(w.ids(text("192.168.1.20")).await, [logout]);
    assert_eq!(w.ids(text("bureau")).await, [logout]);
    assert_eq!(w.ids(text("connexion")).await, [login]);
    assert_eq!(w.ids(text("deconnexion")).await, [logout], "sans accent");
    assert_eq!(w.ids(text("Déconnexion")).await, [logout]);
    assert_eq!(w.ids(text("lecture du journal")).await, [read_denied]);
    assert_eq!(w.ids(text("/audit")).await, [read_denied]);
    assert_eq!(w.ids(text("lecture seule")).await, [read_denied]);
    assert_eq!(w.ids(text("nouveau-compte")).await, [created]);
    // Début de mot et début d'adresse.
    assert_eq!(w.ids(text("poste")).await, [login]);
    assert_eq!(w.ids(text("10.0.0")).await, [read_denied, login]);
    // Plusieurs mots : ET.
    assert_eq!(w.ids(text("connexion marie")).await, [login]);
    assert_eq!(w.ids(text("connexion paul")).await, Vec::<i64>::new());
    // Aucun résultat.
    assert_eq!(w.ids(text("introuvable")).await, Vec::<i64>::new());
}

#[tokio::test]
async fn special_characters_in_the_search_are_text_never_a_query() {
    let w = written().await;
    let r = w.write(&[simple(1), simple(2)]).await;
    let both = [r[1].id, r[0].id];
    // (saisie, lignes attendues) : les deux entrées portent « marie », « poste-de-marie »,
    // « 10.0.0.7 » et « Connexion ». Une saisie sans lettre ni chiffre n'a pas d'effet (tout) ;
    // sinon chaque mot est un mot ordinaire : la syntaxe du moteur n'agit jamais.
    let cases: [(&str, bool); 36] = [
        ("\"", true),
        ("\"\"", true),
        ("*", true),
        ("**", true),
        ("-", true),
        ("(", true),
        (")", true),
        ("%", true),
        ("_", true),
        ("\\", true),
        ("@", true),
        ("+", true),
        ("~", true),
        ("-marie", true),
        ("^marie", true),
        ("marie^", true),
        ("((marie)", true),
        ("{marie}", true),
        ("[marie]", true),
        ("\u{202E}marie", true),
        ("marie, poste", true),
        ("10.0.0.7:*", true),
        ("\"marie\"", true),
        ("marie*", true),
        ("\"unclosed", false),
        ("a*", false),
        ("marie OR paul", false),
        ("marie AND", false),
        ("AND", false),
        ("NOT marie", false),
        ("NEAR(marie poste)", false),
        ("NEAR/2", false),
        ("col:marie", false),
        ("account:marie", false),
        ("'; DROP TABLE audit_events; --", false),
        ("marie\u{0}x", false),
    ];
    for (input, matches) in cases {
        let found = w.ids(text(input)).await;
        let expected: Vec<i64> = if matches { both.to_vec() } else { Vec::new() };
        assert_eq!(found, expected, "{input:?}");
    }
    // La table est intacte après « DROP TABLE ».
    assert_eq!(w.count().await, 2);
}

#[tokio::test]
async fn the_search_is_combined_with_the_other_filters() {
    let w = written().await;
    let a = event(
        1,
        Some("marie"),
        AuditAction::Login,
        Outcome::Succeeded,
        "poste",
        "10.0.0.7",
    );
    let b = event(
        2,
        Some("marie"),
        AuditAction::Login,
        Outcome::Denied(Reason::InvalidCredentials),
        "poste",
        "10.0.0.7",
    );
    let c = event(
        3,
        Some("paul"),
        AuditAction::Login,
        Outcome::Succeeded,
        "poste",
        "10.0.0.7",
    );
    let r = w.write(&[a, b, c]).await;
    let filter = RawFilter {
        accounts: strings(&["marie"]),
        outcomes: strings(&["denied"]),
        text: Some("10.0.0.7".into()),
        ..RawFilter::default()
    };
    assert_eq!(w.ids(filter).await, [r[1].id]);
}

#[tokio::test]
async fn the_purge_removes_by_age_and_then_by_count_and_keeps_the_search_in_step() {
    let w = written().await;
    let events: Vec<AuditEvent> = (0..10).map(|n| simple(n * 10)).collect();
    let r = w.write(&events).await;
    let store = SqliteStore::new(w.env.db.pool().clone());

    // Par âge : les entrées écrites avant la date partent.
    let mut tx = store.begin().await.unwrap();
    let removed = tx
        .audit()
        .purge_before(start_time() + Duration::seconds(30), 1000)
        .await
        .unwrap();
    assert_eq!(removed, 3);
    assert_eq!(tx.audit().count().await.unwrap(), 7);
    tx.commit().await.unwrap();
    assert_eq!(w.ids(RawFilter::default()).await.last(), Some(&r[3].id));

    // Par nombre : les plus anciennes partent d'abord.
    let mut tx = store.begin().await.unwrap();
    assert_eq!(tx.audit().purge_oldest(4).await.unwrap(), 4);
    tx.commit().await.unwrap();
    assert_eq!(
        w.ids(RawFilter::default()).await,
        [r[9].id, r[8].id, r[7].id]
    );
    // Rien à supprimer : zéro, sans erreur.
    let mut tx = store.begin().await.unwrap();
    assert_eq!(
        tx.audit().purge_before(start_time(), 1000).await.unwrap(),
        0
    );
    assert_eq!(tx.audit().purge_oldest(0).await.unwrap(), 0);
    tx.commit().await.unwrap();

    // La table de recherche suit les suppressions : une entrée purgée ne se trouve plus.
    assert_eq!(w.search(text("marie")).await.len(), 3);
    let fts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_fts WHERE audit_fts MATCH 'marie'")
            .fetch_one(w.env.db.pool())
            .await
            .unwrap();
    assert_eq!(fts, 3);
}

#[tokio::test]
async fn a_purge_that_is_not_committed_removes_nothing() {
    let w = written().await;
    w.write(&[simple(1), simple(2)]).await;
    let store = SqliteStore::new(w.env.db.pool().clone());
    let mut tx = store.begin().await.unwrap();
    tx.audit().purge_oldest(2).await.unwrap();
    drop(tx);
    assert_eq!(w.count().await, 2);
}

#[tokio::test]
async fn an_entry_cannot_be_modified_in_place() {
    let w = written().await;
    w.write(&[simple(1)]).await;
    for statement in [
        "UPDATE audit_events SET account = 'autre'",
        "UPDATE audit_events SET outcome = 'ok'",
        "UPDATE audit_events SET reason = NULL",
    ] {
        let error = sqlx::query(statement)
            .execute(w.env.db.pool())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("ne se modifie pas"), "{error}");
    }
    assert_eq!(
        w.search(RawFilter::default()).await[0].account.as_deref(),
        Some("marie")
    );
}

#[tokio::test]
async fn an_uncommitted_entry_is_not_written() {
    let w = written().await;
    let store = SqliteStore::new(w.env.db.pool().clone());
    let mut tx = store.begin().await.unwrap();
    tx.audit().record(&simple(1)).await.unwrap();
    drop(tx);
    assert_eq!(w.count().await, 0);
}

#[tokio::test]
async fn a_batch_purge_removes_at_most_its_limit() {
    let w = written().await;
    let events: Vec<AuditEvent> = (0..10).map(simple).collect();
    w.write(&events).await;
    let store = SqliteStore::new(w.env.db.pool().clone());
    let mut tx = store.begin().await.unwrap();
    let cutoff = start_time() + Duration::seconds(100);
    assert_eq!(tx.audit().purge_before(cutoff, 4).await.unwrap(), 4);
    assert_eq!(tx.audit().purge_before(cutoff, 4).await.unwrap(), 4);
    assert_eq!(tx.audit().purge_before(cutoff, 4).await.unwrap(), 2);
    assert_eq!(tx.audit().purge_before(cutoff, 4).await.unwrap(), 0);
    assert_eq!(tx.audit().count().await.unwrap(), 0);
}
