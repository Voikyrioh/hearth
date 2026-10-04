//! Journal d'activité de bout en bout sur HTTPS : vrai agent, vrai TLS 1.3, vraie base. Une action
//! produit son entrée, un refus faute de droits aussi, un administrateur lit, filtre, pagine et
//! exporte ; aucun secret ne figure nulle part.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use hearth_agent::domain::accounts::Role;
use serde_json::{Value, json};
use support::https::{self, Agent, Wire};
use support::{PASSWORD, env};

const OTHER_PASSWORD: &str = "Another-Pass-77";
const WRONG: &str = "Wrong-Horse-9999";

async fn login(agent: &Agent, username: &str, password: &str) -> Wire {
    agent
        .request("POST", "/sessions")
        .header("x-hearth-client", "poste-de-test/1.0")
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
}

async fn token(agent: &Agent, username: &str) -> String {
    let reply = login(agent, username, PASSWORD).await;
    assert_eq!(reply.status, 201, "{:?}", reply.body);
    reply.body["token"].as_str().unwrap().to_owned()
}

/// Les entrées lues par un administrateur, de la plus récente à la plus ancienne.
async fn events(agent: &Agent, token: &str, query: &str) -> Vec<Value> {
    let reply = agent
        .request("GET", &format!("/audit{query}"))
        .token(token)
        .send()
        .await;
    assert_eq!(reply.status, 200, "{:?}", reply.body);
    reply.body["events"].as_array().unwrap().clone()
}

fn actions(events: &[Value]) -> Vec<(&str, &str)> {
    events
        .iter()
        .map(|event| {
            (
                event["action"].as_str().unwrap(),
                event["outcome"].as_str().unwrap(),
            )
        })
        .collect()
}

#[tokio::test]
async fn an_account_creation_produces_its_entry_with_who_where_and_what() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let agent = https::start(&env).await;
    let admin = token(&agent, "marie").await;

    let created = agent
        .request("POST", "/accounts")
        .token(&admin)
        .header("x-hearth-client", "poste-de-marie/2.0")
        .header("idempotency-key", "KEY-CREATE-1")
        .json(&json!({ "username": "paul", "password": OTHER_PASSWORD, "role": "readonly" }))
        .send()
        .await;
    assert_eq!(created.status, 201, "{:?}", created.body);

    let listed = events(&agent, &admin, "").await;
    let creation = &listed[0];
    assert_eq!(creation["action"], "account.create");
    assert_eq!(creation["action_label"], "Création de compte");
    assert_eq!(creation["outcome"], "ok");
    assert_eq!(creation["account"], "marie");
    assert_eq!(creation["target"], "paul");
    assert_eq!(creation["reason"], json!(null));
    assert_eq!(creation["origin"]["kind"], "client");
    assert_eq!(creation["origin"]["addr"], "127.0.0.1");
    assert_eq!(creation["origin"]["name"], "poste-de-marie/2.0");
    assert_eq!(creation["origin"]["text"], "127.0.0.1 (poste-de-marie/2.0)");
    assert!(creation["at"].as_str().unwrap().ends_with('Z'));

    // La connexion de marie est là aussi ; lire le journal n'y ajoute rien (BR-AUDIT-004).
    assert_eq!(
        actions(&listed),
        [
            ("account.create", "ok"),
            ("login", "ok"),
            ("account.create", "ok")
        ],
        "la création de marie par la ligne de commande de test, sa connexion, celle de paul"
    );
    let again = events(&agent, &admin, "").await;
    assert_eq!(
        again.len(),
        listed.len(),
        "une consultation n'est pas journalisée"
    );

    agent.shutdown().await;
}

#[tokio::test]
async fn a_refused_login_names_the_account_only_when_it_exists_and_never_the_typed_identifier() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let agent = https::start(&env).await;
    let admin = token(&agent, "marie").await;

    for name in ["marie", "fantome", "Mon Mot De Passe Secret 1!"] {
        let refused = login(&agent, name, WRONG).await;
        assert_eq!(refused.status, 401, "{name}");
    }
    let listed = events(&agent, &admin, "?outcome=denied").await;
    assert_eq!(listed.len(), 3);
    for event in &listed {
        assert_eq!(event["action"], "login");
        assert_eq!(event["origin"]["addr"], "127.0.0.1");
        assert_eq!(event["origin"]["name"], "poste-de-test/1.0");
    }
    // Le compte n'est renseigné que s'il existe (ce n'est alors pas un mot de passe tapé par
    // erreur) ; l'identifiant saisi inconnu ou impossible n'est jamais retenu.
    assert_eq!(listed[2]["account"], "marie");
    assert_eq!(listed[1]["account"], json!(null));
    assert_eq!(listed[0]["account"], json!(null));
    // Même raison que l'identifiant existe ou non ; le format impossible a la sienne.
    assert_eq!(listed[2]["reason"], listed[1]["reason"]);
    assert_eq!(listed[2]["reason"], "identifiants incorrects");
    assert_eq!(listed[0]["reason"], "identifiant invalide");
    let dump = serde_json::to_string(&listed).unwrap().to_lowercase();
    for typed in ["fantome", "mon mot de passe", WRONG.to_lowercase().as_str()] {
        assert!(!dump.contains(typed), "{typed}");
    }

    agent.shutdown().await;
}

#[tokio::test]
async fn a_read_only_account_is_refused_and_its_attempts_are_journaled() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    env.create("lucas", Role::ReadOnly).await;
    let agent = https::start(&env).await;
    let admin = token(&agent, "marie").await;
    let readonly = token(&agent, "lucas").await;

    for path in ["/audit", "/audit/export", "/accounts"] {
        // Deux refus identiques à moins d'une minute se regroupent : on espace les demandes.
        env.clock.advance(time::Duration::seconds(61));
        let reply = agent
            .request("GET", path)
            .token(&readonly)
            .header("x-hearth-client", "poste-de-lucas/1.0")
            .send()
            .await;
        assert_eq!(
            (reply.status, reply.code()),
            (403, "FORBIDDEN_ROLE"),
            "{path}"
        );
    }
    let forbidden = agent
        .request("POST", "/accounts")
        .token(&readonly)
        .json(&json!({ "username": "intrus", "password": OTHER_PASSWORD, "role": "admin" }))
        .send()
        .await;
    assert_eq!(forbidden.status, 403);
    // Ce que le compte lecture seule a le droit de faire ne s'écrit pas.
    let me = agent.request("GET", "/me").token(&readonly).send().await;
    assert_eq!(me.status, 200);

    let denied = events(&agent, &admin, "?outcome=denied&account=lucas").await;
    assert_eq!(
        actions(&denied),
        [
            ("account.create", "denied"),
            ("accounts.read", "denied"),
            ("audit.read", "denied"),
            ("audit.read", "denied"),
        ]
    );
    let first = denied
        .iter()
        .find(|event| event["target"] == "/audit")
        .unwrap();
    assert_eq!(first["action_label"], "Tentative de lecture du journal");
    assert_eq!(first["reason"], "lecture seule");
    assert_eq!(first["origin"]["name"], "poste-de-lucas/1.0");
    assert_eq!(first["account"], "lucas");
    assert!(
        denied
            .iter()
            .any(|event| event["target"] == "/audit/export"),
        "l'export refusé aussi"
    );
    // Et rien n'a été créé.
    let accounts = agent.request("GET", "/accounts").token(&admin).send().await;
    assert_eq!(accounts.body["accounts"].as_array().unwrap().len(), 2);

    agent.shutdown().await;
}

#[tokio::test]
async fn a_failed_modifying_request_is_journaled_once_even_when_replayed() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    env.create("paul", Role::ReadOnly).await;
    let agent = https::start(&env).await;
    let admin = token(&agent, "marie").await;

    let request = |key: &str, username: &str, password: &str| {
        agent
            .request("POST", "/accounts")
            .token(&admin)
            .header("idempotency-key", key)
            .json(&json!({ "username": username, "password": password, "role": "readonly" }))
    };
    assert_eq!(
        request("K-WEAK", "nouveau", "faible").send().await.status,
        422
    );
    // Deux échecs identiques à moins d'une minute se regroupent : on espace les requêtes.
    env.clock.advance(time::Duration::seconds(61));
    assert_eq!(
        request("K-TAKEN", "PAUL", OTHER_PASSWORD)
            .send()
            .await
            .status,
        409
    );
    // La même clé rejouée rend le premier résultat sans rien exécuter : pas de seconde entrée.
    let replay = request("K-TAKEN", "PAUL", OTHER_PASSWORD).send().await;
    assert_eq!(replay.status, 409);
    assert_eq!(replay.header("idempotent-replayed"), Some("true"));

    let failed = events(&agent, &admin, "?outcome=failed").await;
    assert_eq!(
        actions(&failed),
        [("account.create", "failed"), ("account.create", "failed")]
    );
    assert_eq!(failed[0]["reason"], "identifiant déjà utilisé");
    assert_eq!(failed[1]["reason"], "données invalides");
    assert_eq!(failed[0]["account"], "marie");
    assert_eq!(failed[0]["target"], "/accounts");

    agent.shutdown().await;
}

#[tokio::test]
async fn the_journal_is_filtered_searched_and_paged_over_the_wire() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let agent = https::start(&env).await;
    let admin = token(&agent, "marie").await;
    for name in ["alice", "bruno", "carla", "denis"] {
        let created = agent
            .request("POST", "/accounts")
            .token(&admin)
            .json(&json!({ "username": name, "password": OTHER_PASSWORD, "role": "readonly" }))
            .send()
            .await;
        assert_eq!(created.status, 201);
    }
    let one_wrong = login(&agent, "marie", WRONG).await;
    assert_eq!(one_wrong.status, 401);

    // Filtres multi-valeurs et combinés.
    let both = events(&agent, &admin, "?action=account.create&q=alice+bruno").await;
    assert_eq!(
        both.len(),
        0,
        "ET entre les mots : aucune entrée ne porte les deux"
    );
    let alice = events(&agent, &admin, "?action=account.create&q=alice").await;
    assert_eq!(alice.len(), 1);
    assert_eq!(alice[0]["target"], "alice");
    let some = events(&agent, &admin, "?q=carla").await;
    assert_eq!(some.len(), 1);
    let logins = events(&agent, &admin, "?action=login&outcome=ok,denied").await;
    assert_eq!(logins.len(), 2);
    // Un mot de recherche qui ressemble à une requête du moteur n'est que du texte.
    for hostile in ["%22", "*", "NEAR%28a%29", "a+OR+b", "-x", "%28"] {
        let reply = agent
            .request("GET", &format!("/audit?q={hostile}"))
            .token(&admin)
            .send()
            .await;
        assert_eq!(reply.status, 200, "{hostile}");
    }

    // Pagination : deux entrées par page, curseur `next_before`, aucune perte ni répétition.
    let mut seen = Vec::new();
    let mut before: Option<i64> = None;
    for _ in 0..20 {
        let query = match before {
            Some(id) => format!("?limit=2&before={id}"),
            None => "?limit=2".to_owned(),
        };
        let reply = agent
            .request("GET", &format!("/audit{query}"))
            .token(&admin)
            .send()
            .await;
        assert_eq!(reply.status, 200);
        let page = reply.body["events"].as_array().unwrap();
        assert!(page.len() <= 2);
        seen.extend(page.iter().map(|event| event["id"].as_i64().unwrap()));
        before = reply.body["next_before"].as_i64();
        if before.is_none() {
            break;
        }
    }
    let all = events(&agent, &admin, "").await;
    let all_ids: Vec<i64> = all
        .iter()
        .map(|event| event["id"].as_i64().unwrap())
        .collect();
    assert_eq!(seen, all_ids);
    assert!(seen.windows(2).all(|pair| pair[0] > pair[1]));

    // Paramètres invalides : 422 qui nomme le champ.
    for (query, field) in [
        ("?action=nope", "action"),
        ("?outcome=maybe", "outcome"),
        ("?from=hier", "from"),
        ("?before=abc", "before"),
        ("?from=2026-10-04T00:00:00Z&to=2026-10-01T00:00:00Z", "to"),
    ] {
        let reply = agent
            .request("GET", &format!("/audit{query}"))
            .token(&admin)
            .send()
            .await;
        assert_eq!(
            (reply.status, reply.code()),
            (422, "VALIDATION_ERROR"),
            "{query}"
        );
        assert_eq!(reply.body["error"]["details"]["field"], field, "{query}");
    }

    agent.shutdown().await;
}

#[tokio::test]
async fn the_export_is_a_spreadsheet_file_of_the_filtered_result() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let agent = https::start(&env).await;
    let admin = token(&agent, "marie").await;
    // Un identifiant qui commencerait une formule dans un tableur.
    let created = agent
        .request("POST", "/accounts")
        .token(&admin)
        .json(&json!({ "username": "-cmd", "password": OTHER_PASSWORD, "role": "readonly" }))
        .send()
        .await;
    assert_eq!(created.status, 201);
    let refused = login(&agent, "fantome", WRONG).await;
    assert_eq!(refused.status, 401);

    let export = agent
        .request("GET", "/audit/export?outcome=ok&action=account.create")
        .token(&admin)
        .send()
        .await;
    assert_eq!(export.status, 200);
    assert_eq!(
        export.header("content-type"),
        Some("text/csv; charset=utf-8")
    );
    assert!(
        export
            .header("content-disposition")
            .unwrap()
            .starts_with("attachment")
    );
    assert_eq!(export.header("x-hearth-export-truncated"), None);
    assert!(
        export.text.starts_with('\u{feff}'),
        "marque d'ordre des octets"
    );
    let lines: Vec<&str> = export.text.trim_start_matches('\u{feff}').lines().collect();
    assert_eq!(
        lines[0],
        "Date et heure;Compte;Origine;Action;Cible;Résultat;Raison"
    );
    assert_eq!(
        lines.len(),
        3,
        "l'en-tête et les deux créations (marie, -cmd) : {lines:?}"
    );
    // Séparateur `;`, formule neutralisée.
    let cmd = lines.iter().find(|line| line.contains("-cmd")).unwrap();
    assert!(cmd.contains(";'-cmd;"), "{cmd}");
    assert!(cmd.contains(";Création de compte;"), "{cmd}");
    assert!(cmd.ends_with(";Réussi;"), "{cmd}");
    // Rien de la connexion refusée : l'export porte sur le résultat filtré.
    assert!(!export.text.contains("Refusé"));

    // Sans filtre, tout (UTF-8, accents compris).
    let everything = agent
        .request("GET", "/audit/export")
        .token(&admin)
        .send()
        .await;
    assert!(everything.text.contains(";Refusé;identifiants incorrects"));

    agent.shutdown().await;
}

#[tokio::test]
async fn no_password_nor_token_is_anywhere_in_the_journal_after_a_full_scenario() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    env.create("lucas", Role::ReadOnly).await;
    let agent = https::start(&env).await;
    let admin = token(&agent, "marie").await;
    let readonly = token(&agent, "lucas").await;

    login(&agent, "marie", WRONG).await;
    login(&agent, PASSWORD, PASSWORD).await;
    agent
        .request("POST", "/accounts")
        .token(&admin)
        .json(&json!({ "username": "paul", "password": OTHER_PASSWORD, "role": "readonly" }))
        .send()
        .await;
    agent
        .request("POST", "/accounts")
        .token(&readonly)
        .json(&json!({ "username": "intrus", "password": OTHER_PASSWORD, "role": "admin" }))
        .send()
        .await;
    agent
        .request("PUT", "/me/password")
        .token(&admin)
        .json(&json!({ "current": WRONG, "password": "Brand-New-Pass-7" }))
        .send()
        .await;
    agent
        .request("PUT", "/accounts/UNKNOWN/password")
        .token(&admin)
        .json(&json!({ "password": "Brand-New-Pass-7" }))
        .send()
        .await;
    agent
        .request("DELETE", "/sessions/current")
        .token(&readonly)
        .send()
        .await;

    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT COALESCE(account,'') || '|' || origin_kind || '|' || COALESCE(origin_name,'') || '|' ||
                COALESCE(origin_addr,'') || '|' || action || '|' || action_label || '|' ||
                COALESCE(target,'') || '|' || outcome || '|' || COALESCE(reason,'')
         FROM audit_events",
    )
    .fetch_all(env.db.pool())
    .await
    .unwrap();
    assert!(rows.len() >= 8, "{}", rows.len());
    let wire = serde_json::to_string(&events(&agent, &admin, "").await).unwrap();
    let export = agent
        .request("GET", "/audit/export")
        .token(&admin)
        .send()
        .await
        .text;
    let mut haystacks: Vec<String> = rows.into_iter().map(|row| row.0).collect();
    haystacks.push(wire);
    haystacks.push(export);
    for haystack in &haystacks {
        for secret in [
            PASSWORD,
            WRONG,
            OTHER_PASSWORD,
            "Brand-New-Pass-7",
            "$argon2",
        ] {
            assert!(!haystack.contains(secret), "{secret} dans {haystack}");
        }
        for bearer in [&admin, &readonly] {
            assert!(!haystack.contains(bearer.as_str()), "jeton dans {haystack}");
        }
    }
    // Le mot de passe tapé à la place de l'identifiant n'apparaît pas non plus, même en minuscules.
    assert!(
        haystacks
            .iter()
            .all(|haystack| !haystack.to_lowercase().contains("correct-horse"))
    );

    agent.shutdown().await;
}

#[tokio::test]
async fn a_failure_or_a_refusal_on_an_account_route_names_the_account() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    let lucas = env.create("lucas", Role::ReadOnly).await;
    let agent = https::start(&env).await;
    let admin = token(&agent, "marie").await;
    let readonly = token(&agent, "lucas").await;

    // Échec : supprimer le dernier administrateur.
    let refused = agent
        .request("DELETE", &format!("/accounts/{}", marie.id))
        .token(&admin)
        .json(&json!({ "confirmation": "marie" }))
        .send()
        .await;
    assert_eq!(refused.status, 409);
    // Refus : un compte lecture seule vise le compte de marie.
    env.clock.advance(time::Duration::seconds(61));
    let denied = agent
        .request("PUT", &format!("/accounts/{}/password", lucas.id))
        .token(&readonly)
        .json(&json!({ "password": OTHER_PASSWORD }))
        .send()
        .await;
    assert_eq!(denied.status, 403);
    // Compte inconnu : le motif de la route.
    env.clock.advance(time::Duration::seconds(61));
    let unknown = agent
        .request("PATCH", "/accounts/INCONNU")
        .token(&admin)
        .json(&json!({ "role": "readonly" }))
        .send()
        .await;
    assert_eq!(unknown.status, 404);

    let listed = events(&agent, &admin, "?outcome=denied,failed").await;
    let targets: Vec<(&str, &str)> = listed
        .iter()
        .map(|e| (e["action"].as_str().unwrap(), e["target"].as_str().unwrap()))
        .collect();
    assert_eq!(
        targets,
        [
            ("account.role", "/accounts/{id}"),
            ("account.password", "lucas"),
            ("account.delete", "marie"),
        ]
    );
    // Le message du refus parle de ce que la route protège.
    let journal = agent.request("GET", "/audit").token(&readonly).send().await;
    assert!(
        journal.body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("journal")
    );
    let accounts = agent
        .request("GET", "/accounts")
        .token(&readonly)
        .send()
        .await;
    assert!(
        accounts.body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("comptes")
    );

    agent.shutdown().await;
}
