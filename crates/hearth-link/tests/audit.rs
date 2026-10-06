//! Lecture du journal d'activité contre un VRAI agent (TLS 1.3, SQLite, WebSocket) : pages et
//! curseur, filtres (comptes, types d'action, résultats, période), recherche plein texte, export
//! CSV, refus d'un compte lecture seule, direct (sujet `audit` du flux). HRT-14.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::collections::HashSet;

use hearth_agent::domain::accounts::Role;
use hearth_link::domain::audit_query::{ActionKind, AuditFilter, RawFilter};
use hearth_link::domain::event::Event;
use hearth_link::domain::secret::Secret;
use hearth_link::ports::Transport as _;
use hearth_link::ports::transport::{Pin, Target};
use hearth_link::{LinkConfig, LinkError};
use hearth_proto::api::audit::{AuditEventItem, OutcomeName};
use hearth_proto::api::sessions::LoginRequest;
use hearth_proto::error::ErrorCode;
use support::{Options, PASSWORD, WAIT, World, fast_config};

fn config() -> LinkConfig {
    LinkConfig {
        subscribe_audit: true,
        ..fast_config()
    }
}

fn filter(raw: RawFilter) -> AuditFilter {
    AuditFilter::new(raw).unwrap()
}

fn none() -> AuditFilter {
    filter(RawFilter::default())
}

async fn admin() -> World {
    World::connected(Options {
        config: config(),
        ..Options::default()
    })
    .await
}

/// Tout le journal (toutes les pages), de la plus récente à la plus ancienne.
async fn everything(world: &World, filter: &AuditFilter) -> Vec<AuditEventItem> {
    let mut all = Vec::new();
    let mut before = None;
    loop {
        let page = world
            .manager
            .audit_page(&world.id, filter, before, 100)
            .await
            .unwrap();
        all.extend(page.events);
        match page.next_before {
            Some(next) => before = Some(next),
            None => return all,
        }
    }
}

fn sorted_without_duplicates(items: &[AuditEventItem]) -> bool {
    items.windows(2).all(|pair| pair[0].id > pair[1].id)
}

#[tokio::test]
async fn pages_come_newest_first_with_a_cursor_and_nothing_is_lost_or_doubled() {
    let world = admin().await;
    for n in 0..120 {
        world
            .agent
            .create_account(&format!("compte-{n:03}"), Role::ReadOnly)
            .await;
    }
    let first = world
        .manager
        .audit_page(&world.id, &none(), None, 100)
        .await
        .unwrap();
    assert_eq!(first.events.len(), 100);
    assert!(first.next_before.is_some());
    assert!(sorted_without_duplicates(&first.events));
    // La plus récente est la dernière création.
    assert_eq!(first.events[0].target.as_deref(), Some("compte-119"));

    let all = everything(&world, &none()).await;
    assert!(
        sorted_without_duplicates(&all),
        "ordre stable, aucun doublon"
    );
    // 120 créations + la connexion réussie de marie + la création de marie elle-même.
    let creations = all.iter().filter(|e| e.action == "account.create").count();
    assert_eq!(creations, 121);
    assert!(all.iter().any(|e| e.action == "login"));
    // Une page de taille 1 suit le curseur sans rien perdre.
    let mut walked = Vec::new();
    let mut before = None;
    for _ in 0..5 {
        let page = world
            .manager
            .audit_page(&world.id, &none(), before, 1)
            .await
            .unwrap();
        walked.extend(page.events);
        before = page.next_before;
    }
    assert_eq!(
        walked.iter().map(|e| e.id).collect::<Vec<_>>(),
        all.iter().take(5).map(|e| e.id).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn filters_combine_by_and_and_each_filter_is_exact() {
    let world = admin().await;
    world.agent.create_account("paul", Role::ReadOnly).await;
    world.agent.create_account("lea", Role::Admin).await;
    // Une connexion refusée de marie (mot de passe faux).
    let refused = world
        .manager
        .login(
            &world.id,
            "marie",
            Secret::from("Mauvais-mot-de-passe-1"),
            false,
        )
        .await;
    assert!(refused.is_err());

    let everything_ = everything(&world, &none()).await;
    let denied_logins = everything(
        &world,
        &filter(RawFilter {
            kinds: vec![ActionKind::LoginDenied],
            ..RawFilter::default()
        }),
    )
    .await;
    assert!(!denied_logins.is_empty());
    assert!(
        denied_logins
            .iter()
            .all(|e| e.action.starts_with("login") && e.outcome == OutcomeName::Denied)
    );

    let accounts = everything(
        &world,
        &filter(RawFilter {
            kinds: vec![ActionKind::Accounts],
            ..RawFilter::default()
        }),
    )
    .await;
    assert_eq!(
        accounts.len(),
        everything_
            .iter()
            .filter(|e| e.action == "account.create")
            .count()
    );

    // Compte : sans distinction de casse ; résultat : réussi seulement.
    let marie_ok = everything(
        &world,
        &filter(RawFilter {
            accounts: vec!["MARIE".into()],
            outcomes: vec![OutcomeName::Ok],
            ..RawFilter::default()
        }),
    )
    .await;
    assert!(
        marie_ok
            .iter()
            .all(|e| e.account.as_deref() == Some("marie") && e.outcome == OutcomeName::Ok)
    );

    // Un type d'action qui contredit le résultat ne retient rien, sans interroger l'agent.
    let contradiction = everything(
        &world,
        &filter(RawFilter {
            kinds: vec![ActionKind::LoginOk],
            outcomes: vec![OutcomeName::Denied],
            ..RawFilter::default()
        }),
    )
    .await;
    assert!(contradiction.is_empty());

    // Plusieurs types qui ne se combinent pas en une requête : l'union exacte, sans doublon.
    let union = everything(
        &world,
        &filter(RawFilter {
            kinds: vec![ActionKind::LoginOk, ActionKind::Accounts],
            ..RawFilter::default()
        }),
    )
    .await;
    assert!(sorted_without_duplicates(&union));
    let expected: HashSet<i64> = everything_
        .iter()
        .filter(|e| {
            (e.action == "login" && e.outcome == OutcomeName::Ok) || e.action == "account.create"
        })
        .map(|e| e.id)
        .collect();
    assert_eq!(union.iter().map(|e| e.id).collect::<HashSet<_>>(), expected);
    // Et sa pagination par curseur recolle sans trou.
    let mut walked = Vec::new();
    let mut before = None;
    loop {
        let page = world
            .manager
            .audit_page(
                &world.id,
                &filter(RawFilter {
                    kinds: vec![ActionKind::LoginOk, ActionKind::Accounts],
                    ..RawFilter::default()
                }),
                before,
                2,
            )
            .await
            .unwrap();
        walked.extend(page.events);
        match page.next_before {
            Some(next) => before = Some(next),
            None => break,
        }
    }
    assert_eq!(
        walked.iter().map(|e| e.id).collect::<Vec<_>>(),
        union.iter().map(|e| e.id).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn the_period_bounds_are_inclusive_and_in_utc() {
    let world = admin().await;
    world.agent.create_account("paul", Role::ReadOnly).await;
    // L'horloge de l'agent de test est figée à 1 790 000 000 s.
    let at = 1_790_000_000;
    let inside = everything(
        &world,
        &filter(RawFilter {
            from_s: Some(at),
            to_s: Some(at),
            ..RawFilter::default()
        }),
    )
    .await;
    assert!(!inside.is_empty());
    let after = everything(
        &world,
        &filter(RawFilter {
            from_s: Some(at + 1),
            ..RawFilter::default()
        }),
    )
    .await;
    assert!(after.is_empty());
    let before = everything(
        &world,
        &filter(RawFilter {
            to_s: Some(at - 1),
            ..RawFilter::default()
        }),
    )
    .await;
    assert!(before.is_empty());
}

#[tokio::test]
async fn the_search_is_a_text_never_a_query() {
    let world = admin().await;
    world
        .agent
        .create_account("paul-durand", Role::ReadOnly)
        .await;
    world.agent.create_account("lea", Role::ReadOnly).await;
    let found = everything(
        &world,
        &filter(RawFilter {
            text: Some("PAUL".into()),
            ..RawFilter::default()
        }),
    )
    .await;
    assert!(
        found
            .iter()
            .any(|e| e.target.as_deref() == Some("paul-durand"))
    );
    assert!(found.iter().all(|e| e.target.as_deref() != Some("lea")));
    // Des opérateurs de requête sont des mots ordinaires : aucune erreur, aucun résultat.
    for hostile in [
        "\"",
        "*",
        "OR",
        "a OR b",
        "NEAR(a b)",
        "-x",
        "col:valeur",
        "(",
        "'; DROP TABLE x;--",
    ] {
        let result = world
            .manager
            .audit_page(
                &world.id,
                &filter(RawFilter {
                    text: Some(hostile.into()),
                    ..RawFilter::default()
                }),
                None,
                100,
            )
            .await;
        assert!(result.is_ok(), "{hostile:?} : {result:?}");
    }
}

#[tokio::test]
async fn the_export_is_the_filtered_result_as_a_spreadsheet_file_with_formulas_neutralized() {
    let world = admin().await;
    // Des comptes dont l'identifiant ressemble à une formule de tableur (« - » et « _ » sont permis).
    world
        .agent
        .create_account("-cmd-calc", Role::ReadOnly)
        .await;
    world.agent.create_account("paul", Role::ReadOnly).await;

    let file = world
        .manager
        .audit_export(
            &world.id,
            &filter(RawFilter {
                kinds: vec![ActionKind::Accounts],
                ..RawFilter::default()
            }),
        )
        .await
        .unwrap();
    assert!(!file.truncated);
    let text = String::from_utf8(file.bytes).unwrap();
    assert!(
        text.starts_with('\u{feff}'),
        "UTF-8 avec marque d'ordre des octets"
    );
    let lines: Vec<&str> = text.trim_start_matches('\u{feff}').split("\r\n").collect();
    assert_eq!(
        lines[0],
        "Date et heure;Compte;Origine;Action;Cible;Résultat;Raison"
    );
    // Seulement le résultat filtré : des créations de compte, pas la connexion de marie.
    assert!(
        lines[1..]
            .iter()
            .filter(|l| !l.is_empty())
            .all(|l| l.contains("Création de compte"))
    );
    // La neutralisation est celle de l'agent, une seule fois (pas d'apostrophe doublée).
    assert!(text.contains(";'-cmd-calc;"), "{text}");
    assert!(!text.contains("''-cmd-calc"), "{text}");
    for line in lines[1..].iter().filter(|l| !l.is_empty()) {
        for cell in line.split(';') {
            assert!(
                !cell.starts_with(['=', '+', '-', '@', '\t']),
                "cellule non neutralisée : {cell:?}"
            );
        }
    }

    // Plusieurs requêtes (types qui ne se combinent pas) : même rendu, même neutralisation.
    let merged = world
        .manager
        .audit_export(
            &world.id,
            &filter(RawFilter {
                kinds: vec![ActionKind::LoginOk, ActionKind::Accounts],
                ..RawFilter::default()
            }),
        )
        .await
        .unwrap();
    let text = String::from_utf8(merged.bytes).unwrap();
    assert!(text.starts_with('\u{feff}'));
    assert!(text.contains(";'-cmd-calc;"), "{text}");
    assert!(text.contains("Connexion;"), "{text}");
    assert!(!text.contains("''"), "{text}");
}

#[tokio::test]
async fn a_read_only_account_is_refused_and_the_refusal_is_itself_journaled() {
    let world = World::connected(Options {
        role: Role::ReadOnly,
        config: config(),
        ..Options::default()
    })
    .await;
    let refused = world
        .manager
        .audit_page(&world.id, &none(), None, 100)
        .await
        .unwrap_err();
    assert_eq!(refused, LinkError::Rejected(Some(ErrorCode::ForbiddenRole)));
    let export = world
        .manager
        .audit_export(&world.id, &none())
        .await
        .unwrap_err();
    assert_eq!(export, LinkError::Rejected(Some(ErrorCode::ForbiddenRole)));
    // L'agent a consigné chaque tentative (BR-AUDIT-021) : une entrée par refus, ni plus ni moins
    // (l'abonnement au sujet `audit`, refusé aussi, ne laisse pas de trace).
    let journal = world
        .agent
        .services
        .audit
        .search(
            Role::Admin,
            &hearth_agent::domain::audit::AuditFilter::new(Default::default()).unwrap(),
        )
        .await
        .unwrap();
    let attempts = journal
        .records
        .iter()
        .filter(|record| record.action == "audit.read")
        .count();
    assert_eq!(attempts, 2, "une page et un export refusés");
    // S'abonner au sujet `audit` n'a, lui, rien consigné : le lien est « Connecté » et muet.
    assert!(
        world
            .recorder
            .since(0)
            .iter()
            .all(|(_, event)| !matches!(event, Event::Audit { .. }))
    );
}

#[tokio::test]
async fn new_entries_arrive_live_exactly_once_for_an_administrator() {
    let world = admin().await;
    let mark = world.recorder.mark();
    // Une connexion refusée, par l'interface de l'agent (c'est elle qui publie sur le flux).
    let target = Target {
        host: "127.0.0.1".into(),
        port: world.proxy.port(),
        pin: Pin::Pinned(world.fingerprint),
    };
    let refused = support::transport()
        .login(
            &target,
            &LoginRequest {
                username: "marie".into(),
                password: "Mauvais-mot-de-passe-1".into(),
            },
        )
        .await;
    assert!(refused.is_err());
    let (_, event) = world
        .recorder
        .wait_for(mark, "une entrée en direct", WAIT, |event| {
            matches!(event, Event::Audit { event, .. } if event.action == "login" && event.outcome == OutcomeName::Denied)
        })
        .await;
    let Event::Audit { event, server } = event else {
        unreachable!()
    };
    assert_eq!(server, world.id);
    assert_eq!(event.outcome, OutcomeName::Denied);
    // Et la lecture retrouve la même entrée : même identifiant, une seule fois.
    let page = world
        .manager
        .audit_page(&world.id, &none(), None, 100)
        .await
        .unwrap();
    assert_eq!(page.events.iter().filter(|e| e.id == event.id).count(), 1);
    let live: Vec<_> = world
        .recorder
        .since(mark)
        .into_iter()
        .filter(|(_, e)| matches!(e, Event::Audit { event: ev, .. } if ev.id == event.id))
        .collect();
    assert_eq!(live.len(), 1, "jamais deux fois");
}

#[tokio::test]
async fn nothing_is_sent_before_the_link_is_connected_or_with_an_invalid_cursor() {
    let world = admin().await;
    let invalid = world
        .manager
        .audit_page(&world.id, &none(), Some(0), 100)
        .await
        .unwrap_err();
    assert!(matches!(invalid, LinkError::InvalidInput(_)), "{invalid:?}");
    // Un serveur ajouté mais jamais connecté.
    let other = support::TestAgent::install().await;
    let bare = {
        let probe = world
            .manager
            .probe("127.0.0.1", other.addr.port())
            .await
            .unwrap();
        world
            .manager
            .add_server(hearth_link::NewServer {
                name: "Autre".into(),
                color: "#7aa2f7".into(),
                host: "127.0.0.1".into(),
                port: other.addr.port(),
                fingerprint: probe.fingerprint,
                mac_addresses: vec![],
            })
            .await
            .unwrap()
    };
    let error = world
        .manager
        .audit_page(&bare, &none(), None, 100)
        .await
        .unwrap_err();
    assert_eq!(error, LinkError::NotConnected);
    let _ = PASSWORD;
}
