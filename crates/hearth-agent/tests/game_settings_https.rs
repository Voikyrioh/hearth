//! Les réglages d'un serveur de jeu, de bout en bout (HRT-108, ADR-0051) : vrai agent en HTTPS, vraie base, faux
//! moteur, faux dossiers de données. Un serveur Terraria est INSTALLÉ par la route de l'installation, puis ses
//! réglages sont lus, analysés et enregistrés. Règles : BR-FICHE-037, 042, 044, 046 à 049, 052, 053, BR-JEU-035 et
//! BR-JEU-036. La preuve de clé et le mot de passe de la confirmation sont dans la matrice de `admin_reauth.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use hearth_agent::application::catalog::CatalogService;
use hearth_agent::application::ports::EngineInfo;
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::catalog::loading::model::Fiche;
use hearth_agent::domain::catalog::{Origin, load};
use hearth_agent::domain::services::ServiceKey;
use hearth_proto::api::deploy::DeployAccepted;
use hearth_proto::api::game_settings::{
    FileState, ImportState, ImportView, SaveSettingsResponse, SettingEffect, SettingsView,
    ValueState,
};
use hearth_proto::api::service_ops::{OperationStatus, OperationView};
use serde_json::{Value, json};
use support::device::Actor;
use support::engine::{fast_timings, parts_supervised};
use support::fake_engine::{FakeEngine, PullScript};
use support::fake_host::{FakeFreeSpace, FakeHostPorts};
use support::fake_safe_fs::FakeSafeFs;
use support::fake_supervisor::FakeSupervisor;
use support::https::{self, Agent, Wire};
use support::{Env, env};

const GIB: u64 = 1024 * 1024 * 1024;
const TERRARIA_FICHE: &str = include_str!("../catalog/terraria/fiche.toml");
const TERRARIA_SETTINGS: &str = include_str!("../catalog/terraria/settings.toml");
const IMAGE_DIGEST: &str =
    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
/// Un mot de passe qui ne doit apparaître NULLE PART sauf dans le fichier du jeu.
const SENTINEL: &str = "Sentinelle-pw-77QaZ";
const SENTINEL_2: &str = "Sentinelle-neuve-31Xk";
const CONFIG: &str = "config/serverconfig.txt";

fn pinned_terraria() -> Fiche {
    let text = TERRARIA_FICHE.replacen(
        "game_updates = \"never\"",
        &format!("digest = \"{IMAGE_DIGEST}\"\ngame_updates = \"never\""),
        1,
    );
    load(
        Origin::Embedded,
        text.as_bytes(),
        TERRARIA_SETTINGS.as_bytes(),
    )
    .expect("Terraria épinglée valide")
}

struct Bench {
    env: Env,
    engine: Arc<FakeEngine>,
    safe_fs: Arc<FakeSafeFs>,
    agent: Agent,
    fiche: Fiche,
    /// Administrateur avec le droit de gérer les services.
    ada: Actor,
    /// Administrateur SANS le droit.
    bob: Actor,
    /// Lecture seule AVEC le droit.
    dan: Actor,
}

async fn bench() -> Bench {
    let env = env().await;
    let fiche = pinned_terraria();
    let engine = Arc::new(FakeEngine::new());
    engine.set_info(Some(EngineInfo {
        cpus: 8,
        memory_bytes: 16 * GIB,
        storage_dir: Some("/var/lib/docker".into()),
    }));
    let safe_fs = Arc::new(FakeSafeFs::new());
    let mut parts = parts_supervised(
        engine.clone(),
        Arc::new(FakeSupervisor::new()),
        fast_timings(),
    );
    parts.host_ports = Arc::new(FakeHostPorts::new());
    parts.free_space = Arc::new(FakeFreeSpace::new());
    parts.safe_fs = Some(safe_fs.clone());
    parts.catalog.include_preview = true;
    parts.catalog.service = Some(Arc::new(CatalogService::from_parts(
        vec![fiche.clone()],
        Vec::new(),
        true,
    )));
    let agent = https::start_engined(&env, parts).await;
    let ada = agent.actor(&env, "ada", Role::Admin).await;
    let bob = agent.actor(&env, "bob", Role::Admin).await;
    let dan = agent.actor(&env, "dan", Role::ReadOnly).await;
    env.grant_services("ada").await;
    env.grant_services("dan").await;
    Bench {
        env,
        engine,
        safe_fs,
        agent,
        fiche,
        ada,
        bob,
        dan,
    }
}

fn code(wire: &Wire) -> String {
    wire.body["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

impl Bench {
    /// Installe un serveur Terraria par la route de l'installation (mot de passe et message d'accueil donnés).
    async fn install(&self) -> ServiceKey {
        let pinned = format!("ryshe/terraria:vanilla-1.4.5.8@{IMAGE_DIGEST}");
        self.engine.publish_image(&pinned, PullScript::two_layers());
        let agreements: Vec<String> = self
            .fiche
            .file
            .agreement
            .iter()
            .map(|a| a.id.clone())
            .collect();
        let body = json!({
            "name": "mon-terraria",
            "resources": { "cpu_millis": 1000, "memory_bytes": 2 * GIB },
            "source": {
                "type": "fiche", "id": self.fiche.id(), "version": self.fiche.file.version,
                "digest": self.fiche.digest_hex(), "agreements": agreements,
                "answers": [
                    {"id": "password", "value": SENTINEL},
                    {"id": "motd", "value": "Salut"}
                ]
            }
        });
        let reply = self
            .agent
            .confirmed(&self.ada, "POST", "/deployments", body)
            .await
            .send()
            .await;
        assert_eq!(reply.status, 202, "{}", reply.text);
        let accepted: DeployAccepted = serde_json::from_value(reply.body).expect("DeployAccepted");
        let started = Instant::now();
        loop {
            let status = self
                .agent
                .request(
                    "GET",
                    &format!("/operations/services/{}", accepted.operation_id),
                )
                .token(&self.dan.token)
                .send()
                .await;
            let operation: OperationView = serde_json::from_value(status.body).unwrap();
            if operation.status == OperationStatus::Succeeded {
                break;
            }
            assert!(
                !matches!(
                    operation.status,
                    OperationStatus::Failed | OperationStatus::Cancelled
                ),
                "{operation:?}"
            );
            assert!(started.elapsed() < Duration::from_secs(15), "{operation:?}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        ServiceKey::parse(&accepted.service_key).expect("clé")
    }

    fn path(key: &ServiceKey, tail: &str) -> String {
        format!("/services/{}/{tail}", key.as_str())
    }

    async fn view_as(&self, who: &Actor, key: &ServiceKey) -> Wire {
        self.agent
            .request("GET", &Self::path(key, "settings"))
            .token(&who.token)
            .send()
            .await
    }

    async fn view(&self, key: &ServiceKey) -> SettingsView {
        let reply = self.view_as(&self.ada, key).await;
        assert_eq!(reply.status, 200, "{}", reply.text);
        serde_json::from_value(reply.body).expect("SettingsView")
    }

    async fn save_as(&self, who: &Actor, key: &ServiceKey, body: Value) -> Wire {
        self.agent
            .confirmed(who, "PUT", &Self::path(key, "settings"), body)
            .await
            .send()
            .await
    }

    async fn save(&self, key: &ServiceKey, body: Value) -> Wire {
        self.save_as(&self.ada, key, body).await
    }

    async fn analyze(&self, key: &ServiceKey, text: &str) -> Wire {
        self.agent
            .request("POST", &Self::path(key, "settings/analyze"))
            .token(&self.ada.token)
            .json(&json!({ "text": text }))
            .send()
            .await
    }

    fn file(&self, key: &ServiceKey) -> String {
        String::from_utf8(self.safe_fs.content(key, CONFIG).expect("fichier")).expect("utf8")
    }

    /// Le fichier tel que le jeu l'aurait laissé : les octets donnés, droits du fichier produit.
    fn rewrite(&self, key: &ServiceKey, bytes: &[u8]) {
        support::game::put_file(&self.safe_fs, key, bytes);
    }

    /// Les lignes du journal des réglages : (résultat, cible).
    async fn journal(&self) -> Vec<(String, Option<String>)> {
        self.agent.flush_journal().await;
        sqlx::query_as(
            "SELECT outcome, target FROM audit_events WHERE action = 'service.game.settings' ORDER BY id",
        )
        .fetch_all(self.env.db.pool())
        .await
        .unwrap()
    }

    fn change(id: &str, value: &str) -> Value {
        json!({ "id": id, "action": "set", "value": value })
    }

    fn body(rev: u32, stamp: &str, effect: &str, changes: Vec<Value>) -> Value {
        json!({
            "base_rev": rev, "file_stamp": stamp, "origin": "form", "effect": effect,
            "changes": changes
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Lire (BR-FICHE-047, 048)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn reading_shows_the_installed_values_the_file_state_and_never_the_password() {
    let b = bench().await;
    let key = b.install().await;
    let reply = b.view_as(&b.ada, &key).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert!(!reply.text.contains(SENTINEL), "le mot de passe fuit");
    let view: SettingsView = serde_json::from_value(reply.body).unwrap();
    assert_eq!(view.rev, 0);
    assert_eq!(view.file_state, FileState::Ok);
    assert!(view.file_stamp.is_some());
    assert!(view.raw_edit);
    assert!(view.drifts.is_empty(), "{:?}", view.drifts);
    assert!(view.others.is_empty());
    assert!(view.imposed_broken.is_empty(), "{:?}", view.imposed_broken);
    let value = |id: &str| view.settings.iter().find(|s| s.id == id).expect(id).clone();
    assert_eq!(value("motd").value.as_deref(), Some("Salut"));
    assert_eq!(value("motd").effect, SettingEffect::Restart);
    let password = value("password");
    assert_eq!(password.state, ValueState::SecretSet);
    assert!(password.value.is_none());
    assert!(
        value("world_name").frozen,
        "figé après la création du monde"
    );
    assert!(value("upnp").imposed);
    assert_eq!(
        value("announcement_box_off").effect,
        SettingEffect::Recreate
    );
    // Tous les réglages de la fiche sont rendus, rien d'autre.
    assert_eq!(view.settings.len(), b.fiche.settings.len());
    b.agent.shutdown().await;
}

#[tokio::test]
async fn reading_needs_the_right_to_manage_services_and_a_stranger_key_is_unknown() {
    let b = bench().await;
    let key = b.install().await;
    for (who, expected) in [(&b.bob, 403), (&b.dan, 200)] {
        let reply = b.view_as(who, &key).await;
        assert_eq!(reply.status, expected, "{}", reply.text);
    }
    let other = ServiceKey::parse("h_01JZ0000000000000000000777").unwrap();
    assert_eq!(code(&b.view_as(&b.ada, &other).await), "SERVICE_NOT_FOUND");
    let reply = b
        .agent
        .request("GET", "/services/pas-une-cle/settings")
        .token(&b.ada.token)
        .send()
        .await;
    assert_eq!(code(&reply), "SERVICE_NOT_FOUND");
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Enregistrer (BR-FICHE-046 à 048, 053, BR-JEU-035)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn saving_touches_only_the_asked_line_and_every_unknown_line_and_comment_survives() {
    let b = bench().await;
    let key = b.install().await;
    // Le jeu (ou une main) a laissé un commentaire, un réglage inventé et une ligne commentée.
    let installed = b.file(&key);
    let mut with_extras = installed.clone();
    with_extras.push_str("# ajouté à la main\nmod_inventé=42\n#secure=0\n");
    b.rewrite(&key, with_extras.as_bytes());
    let view = b.view(&key).await;
    assert_eq!(
        view.others
            .iter()
            .map(|o| (o.key.as_str(), o.value.as_deref()))
            .collect::<Vec<_>>(),
        vec![("mod_inventé", Some("42"))]
    );
    let stamp = view.file_stamp.clone().unwrap();
    // Un enregistrement du formulaire.
    let reply = b
        .save(
            &key,
            Bench::body(
                0,
                &stamp,
                "restart",
                vec![
                    Bench::change("motd", "Bienvenue"),
                    Bench::change("max_players", "12"),
                ],
            ),
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let done: SaveSettingsResponse = serde_json::from_value(reply.body).unwrap();
    assert_eq!((done.rev, done.changed, done.file_written), (1, 2, true));
    assert_eq!(done.effect, SettingEffect::Restart);
    let after = b.file(&key);
    for kept in ["# ajouté à la main\n", "mod_inventé=42\n", "#secure=0\n"] {
        assert!(after.contains(kept), "{kept:?} a disparu : {after:?}");
    }
    assert!(after.contains("motd=Bienvenue\n") && after.contains("maxplayers=12\n"));
    // Seules ces deux lignes ont changé : le reste est identique, octet pour octet.
    let diff: Vec<_> = installed
        .lines()
        .zip(after.lines())
        .filter(|(a, b)| a != b)
        .collect();
    assert_eq!(diff.len(), 2, "{diff:?}");
    // La remise aux valeurs proposées, puis « non écrit » : l'inconnu survit encore.
    let view = b.view(&key).await;
    assert_eq!(view.rev, 1);
    let stamp = view.file_stamp.clone().unwrap();
    let reset = json!({
        "base_rev": 1, "file_stamp": stamp, "origin": "reset", "effect": "restart",
        "changes": [{"id": "max_players", "action": "reset"}, {"id": "motd", "action": "unset"}]
    });
    let reply = b.save(&key, reset).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let after = b.file(&key);
    assert!(
        after.contains("maxplayers=8\n"),
        "valeur proposée : {after:?}"
    );
    assert!(!after.contains("motd="), "non écrit : ligne retirée");
    for kept in ["# ajouté à la main\n", "mod_inventé=42\n", "#secure=0\n"] {
        assert!(after.contains(kept), "{kept:?} a disparu : {after:?}");
    }
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_save_that_changes_no_byte_writes_nothing_and_leaves_no_journal_entry() {
    let b = bench().await;
    let key = b.install().await;
    let before_file = b.file(&key);
    let view = b.view(&key).await;
    let stamp = view.file_stamp.clone().unwrap();
    // Aucun changement demandé.
    let reply = b
        .save(&key, Bench::body(0, &stamp, "none", Vec::new()))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let done: SaveSettingsResponse = serde_json::from_value(reply.body).unwrap();
    assert_eq!((done.rev, done.changed, done.file_written), (0, 0, false));
    assert_eq!(done.effect, SettingEffect::None);
    assert_eq!(b.file(&key), before_file);
    assert!(
        b.journal().await.is_empty(),
        "aucun réglage changé : aucune entrée"
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn the_journal_names_the_changed_settings_that_are_not_secrets_and_never_a_value() {
    let b = bench().await;
    let key = b.install().await;
    let stamp = b.view(&key).await.file_stamp.unwrap();
    let reply = b
        .save(
            &key,
            Bench::body(
                0,
                &stamp,
                "restart",
                vec![
                    Bench::change("motd", "Un message très reconnaissable"),
                    Bench::change("password", SENTINEL_2),
                ],
            ),
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    // Le mot de passe est dans le fichier du jeu, et seulement là.
    assert!(b.file(&key).contains(&format!("password={SENTINEL_2}")));
    assert!(!reply.text.contains(SENTINEL_2));
    let journal = b.journal().await;
    assert_eq!(journal.len(), 1, "{journal:?}");
    assert_eq!(journal[0].0, "ok");
    let target = journal[0].1.clone().unwrap();
    assert!(
        target.contains("2 réglages changés")
            && target.contains("motd")
            && target.contains("mot de passe modifié"),
        "{target}"
    );
    assert!(!target.contains("password,") && !target.contains("(motd, password)"));
    assert!(!target.contains("reconnaissable") && !target.contains(SENTINEL_2));
    b.agent.shutdown().await;
}

#[tokio::test]
async fn no_secret_value_is_readable_in_the_database_the_views_or_the_error_bodies() {
    let b = bench().await;
    let key = b.install().await;
    let stamp = b.view(&key).await.file_stamp.unwrap();
    let saved = b
        .save(
            &key,
            Bench::body(
                0,
                &stamp,
                "restart",
                vec![Bench::change("password", SENTINEL_2)],
            ),
        )
        .await;
    assert_eq!(saved.status, 200, "{}", saved.text);
    let mut seen = vec![saved.text.clone(), b.view_as(&b.ada, &key).await.text];
    // Une demande refusée ne répète pas la valeur reçue non plus.
    let refused = b
        .save(
            &key,
            Bench::body(
                1,
                "x",
                "restart",
                vec![Bench::change("max_players", SENTINEL_2)],
            ),
        )
        .await;
    seen.push(refused.text.clone());
    let analysed = b
        .analyze(&key, &format!("password={SENTINEL_2}\nmotd=Hello\n"))
        .await;
    assert_eq!(analysed.status, 200, "{}", analysed.text);
    seen.push(analysed.text.clone());
    for text in &seen {
        assert!(
            !text.contains(SENTINEL_2) && !text.contains(SENTINEL),
            "{text}"
        );
    }
    // La base : ni la ligne des réponses ni les scellés ne portent la valeur en clair.
    let pool = b.env.db.pool();
    let answers: String = sqlx::query_scalar("SELECT answers_json FROM game_settings")
        .fetch_one(pool)
        .await
        .unwrap();
    assert!(!answers.contains(SENTINEL_2), "{answers}");
    let sealed: Vec<Vec<u8>> = sqlx::query_scalar("SELECT sealed FROM game_setting_secrets")
        .fetch_all(pool)
        .await
        .unwrap();
    assert_eq!(sealed.len(), 1, "un secret changé, un scellé");
    for blob in sealed {
        assert!(
            !blob
                .windows(SENTINEL_2.len())
                .any(|w| w == SENTINEL_2.as_bytes())
        );
    }
    let audit: Vec<String> =
        sqlx::query_scalar("SELECT COALESCE(target, '') || COALESCE(reason, '') FROM audit_events")
            .fetch_all(pool)
            .await
            .unwrap();
    for line in audit {
        assert!(
            !line.contains(SENTINEL_2) && !line.contains(SENTINEL),
            "{line}"
        );
    }
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_stale_revision_is_a_conflict_and_the_file_is_untouched() {
    let b = bench().await;
    let key = b.install().await;
    let stamp = b.view(&key).await.file_stamp.unwrap();
    let first = b
        .save(
            &key,
            Bench::body(0, &stamp, "restart", vec![Bench::change("motd", "A")]),
        )
        .await;
    assert_eq!(first.status, 200, "{}", first.text);
    let file = b.file(&key);
    // Un second client avait lu la révision 0.
    let late = b
        .save(
            &key,
            Bench::body(0, &stamp, "restart", vec![Bench::change("motd", "B")]),
        )
        .await;
    assert_eq!(
        (late.status, code(&late).as_str()),
        (409, "SETTINGS_CONFLICT")
    );
    assert_eq!(b.file(&key), file);
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Un fichier réécrit par le jeu (BR-FICHE-048)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_file_the_game_rewrote_is_shown_and_nothing_is_written_without_a_choice_per_setting() {
    let b = bench().await;
    let key = b.install().await;
    // Le jeu a changé le message d'accueil et le nombre de joueurs.
    let rewritten = b
        .file(&key)
        .replace("motd=Salut", "motd=Écrit par le jeu")
        .replace("maxplayers=8", "maxplayers=3");
    b.rewrite(&key, rewritten.as_bytes());
    let view = b.view(&key).await;
    assert!(view.file_changed || view.rev == 0);
    let mut ids: Vec<&str> = view.drifts.iter().map(|d| d.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec!["max_players", "motd"]);
    let motd = view.drifts.iter().find(|d| d.id == "motd").unwrap();
    assert_eq!(motd.hearth.as_deref(), Some("Salut"));
    assert_eq!(motd.file.as_deref(), Some("Écrit par le jeu"));
    let stamp = view.file_stamp.clone().unwrap();
    // Sans choix : refusé, avec la liste des réglages, et le fichier est resté tel quel.
    let refused = b
        .save(
            &key,
            Bench::body(0, &stamp, "restart", vec![Bench::change("secure", "0")]),
        )
        .await;
    assert_eq!(
        (refused.status, code(&refused).as_str()),
        (409, "SETTINGS_FILE_CHANGED")
    );
    let mut ids: Vec<String> = refused.body["error"]["details"]["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    ids.sort();
    assert_eq!(ids, vec!["max_players".to_owned(), "motd".to_owned()]);
    assert_eq!(b.file(&key), rewritten);
    assert_eq!(b.view(&key).await.rev, 0, "rien n'est enregistré");
    // Un choix par réglage : garder le fichier pour l'un, remettre la mienne pour l'autre.
    let mut body = Bench::body(0, &stamp, "restart", vec![Bench::change("secure", "0")]);
    body["origin"] = json!("drift");
    body["drift_choices"] = json!([
        {"id": "motd", "choice": "keep_file"},
        {"id": "max_players", "choice": "keep_mine"}
    ]);
    let done = b.save(&key, body).await;
    assert_eq!(done.status, 200, "{}", done.text);
    let after = b.file(&key);
    assert!(after.contains("motd=Écrit par le jeu"), "{after}");
    assert!(after.contains("maxplayers=8"), "{after}");
    assert!(after.contains("secure=0"));
    let view = b.view(&key).await;
    let value = |id: &str| view.settings.iter().find(|s| s.id == id).unwrap().clone();
    assert_eq!(value("motd").value.as_deref(), Some("Écrit par le jeu"));
    assert!(view.drifts.is_empty());
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_file_that_moved_since_it_was_read_is_refused_by_its_stamp() {
    let b = bench().await;
    let key = b.install().await;
    let stamp = b.view(&key).await.file_stamp.unwrap();
    let moved = format!("{}# ajouté entre-temps\n", b.file(&key));
    b.rewrite(&key, moved.as_bytes());
    let reply = b
        .save(
            &key,
            Bench::body(0, &stamp, "restart", vec![Bench::change("motd", "X")]),
        )
        .await;
    assert_eq!(
        (reply.status, code(&reply).as_str()),
        (409, "SETTINGS_FILE_CHANGED")
    );
    assert_eq!(b.file(&key), moved);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn an_imposed_setting_undone_by_hand_is_reported_and_never_rewritten() {
    let b = bench().await;
    let key = b.install().await;
    let undone = b.file(&key).replace("upnp=0", "upnp=1");
    b.rewrite(&key, undone.as_bytes());
    let view = b.view(&key).await;
    assert_eq!(view.imposed_broken, vec!["upnp".to_owned()]);
    assert!(view.drifts.iter().all(|d| d.id != "upnp"));
    let stamp = view.file_stamp.unwrap();
    let reply = b
        .save(
            &key,
            Bench::body(0, &stamp, "restart", vec![Bench::change("motd", "Y")]),
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert!(
        b.file(&key).contains("upnp=1"),
        "aucune écriture ne le corrige"
    );
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Refus (BR-FICHE-042, 046, JEU-036)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn an_unreadable_file_is_refused_whole_for_a_save_and_told_by_the_view() {
    let b = bench().await;
    let key = b.install().await;
    let stamp = b.view(&key).await.file_stamp.unwrap();
    b.rewrite(&key, b"motd=ok\0octet nul\n");
    let view = b.view(&key).await;
    assert_eq!(view.file_state, FileState::Unreadable);
    let reply = b
        .save(
            &key,
            Bench::body(0, &stamp, "restart", vec![Bench::change("motd", "Z")]),
        )
        .await;
    assert_eq!(
        (reply.status, code(&reply).as_str()),
        (422, "SETTINGS_FILE_UNREADABLE")
    );
    assert_eq!(b.file(&key), "motd=ok\0octet nul\n");
    assert!(b.journal().await.iter().all(|(outcome, _)| outcome != "ok"));
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_refused_change_writes_nothing_and_names_the_setting_never_the_value() {
    let b = bench().await;
    let key = b.install().await;
    let before = b.file(&key);
    let stamp = b.view(&key).await.file_stamp.unwrap();
    for (changes, expected_id, reason) in [
        (
            vec![Bench::change("max_players", "9999")],
            "max_players",
            "bounds",
        ),
        (vec![Bench::change("upnp", "1")], "upnp", "imposed"),
        (
            vec![Bench::change("world_name", "Autre")],
            "world_name",
            "frozen",
        ),
        (
            vec![Bench::change("language", "klingon")],
            "language",
            "choice",
        ),
    ] {
        // Un bon changement AVEC le mauvais : tous ou aucun.
        let mut all = vec![Bench::change("motd", "Bon")];
        all.extend(changes);
        let reply = b.save(&key, Bench::body(0, &stamp, "restart", all)).await;
        assert_eq!(
            (reply.status, code(&reply).as_str()),
            (422, "SETTING_REFUSED"),
            "{}",
            reply.text
        );
        let details = &reply.body["error"]["details"];
        assert_eq!(details["id"], expected_id);
        assert_eq!(details["reason"], reason);
        assert!(!reply.text.contains("9999") && !reply.text.contains("klingon"));
        assert_eq!(b.file(&key), before);
    }
    // Un réglage inconnu de la fiche ne s'invente pas.
    let reply = b
        .save(
            &key,
            Bench::body(0, &stamp, "restart", vec![Bench::change("inventé", "1")]),
        )
        .await;
    assert_eq!(code(&reply), "SETTING_REFUSED");
    assert!(reply.body["error"]["details"]["id"].is_null());
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_reset_never_touches_a_secret_nor_a_frozen_setting() {
    let b = bench().await;
    let key = b.install().await;
    let before = b.file(&key);
    let stamp = b.view(&key).await.file_stamp.unwrap();
    let reset = |ids: &[&str]| {
        json!({
            "base_rev": 0, "file_stamp": stamp, "origin": "reset", "effect": "restart",
            "changes": ids.iter().map(|id| json!({"id": id, "action": "reset"})).collect::<Vec<_>>()
        })
    };
    let secret = b.save(&key, reset(&["motd", "password"])).await;
    assert_eq!(
        (secret.status, code(&secret).as_str()),
        (422, "SETTING_REFUSED")
    );
    assert_eq!(secret.body["error"]["details"]["id"], "password");
    assert_eq!(secret.body["error"]["details"]["reason"], "secret");
    let frozen = b.save(&key, reset(&["motd", "world_name"])).await;
    assert_eq!(frozen.body["error"]["details"]["reason"], "frozen");
    assert_eq!(b.file(&key), before, "rien n'est écrit à moitié");
    // Un thème entier remis, sans secret ni figé : appliqué d'un coup.
    let done = b.save(&key, reset(&["motd", "max_players"])).await;
    assert_eq!(done.status, 200, "{}", done.text);
    assert!(
        !b.file(&key).contains("motd="),
        "aucune valeur proposée : ligne retirée"
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_setting_received_by_argument_asks_for_a_recreation_this_agent_cannot_do() {
    let b = bench().await;
    let key = b.install().await;
    let before = b.file(&key);
    let stamp = b.view(&key).await.file_stamp.unwrap();
    let reply = b
        .save(
            &key,
            Bench::body(
                0,
                &stamp,
                "recreate",
                vec![
                    Bench::change("announcement_box_off", "1"),
                    Bench::change("motd", "W"),
                ],
            ),
        )
        .await;
    assert_eq!(
        (reply.status, code(&reply).as_str()),
        (409, "RECREATE_NOT_AVAILABLE")
    );
    assert_eq!(
        reply.body["error"]["details"]["ids"],
        json!(["announcement_box_off"])
    );
    assert_eq!(b.file(&key), before, "rien n'est écrit à moitié");
    b.agent.shutdown().await;
}

#[tokio::test]
async fn the_effect_the_client_signed_must_be_the_one_the_agent_deduces() {
    let b = bench().await;
    let key = b.install().await;
    let before = b.file(&key);
    let stamp = b.view(&key).await.file_stamp.unwrap();
    let reply = b
        .save(
            &key,
            Bench::body(0, &stamp, "none", vec![Bench::change("motd", "V")]),
        )
        .await;
    assert_eq!(
        (reply.status, code(&reply).as_str()),
        (409, "SETTINGS_CONFLICT")
    );
    assert_eq!(reply.body["error"]["details"]["reason"], "effect");
    assert_eq!(b.file(&key), before);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn an_account_without_the_right_changes_nothing() {
    let b = bench().await;
    let key = b.install().await;
    let before = b.file(&key);
    let stamp = b.view(&key).await.file_stamp.unwrap();
    let reply = b
        .save_as(
            &b.bob,
            &key,
            Bench::body(0, &stamp, "restart", vec![Bench::change("motd", "U")]),
        )
        .await;
    assert_eq!(reply.status, 403, "{}", reply.text);
    assert_eq!(b.file(&key), before);
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Importer un texte collé (BR-FICHE-052)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn analysing_a_pasted_text_writes_nothing_and_judges_each_setting() {
    let b = bench().await;
    let key = b.install().await;
    let before = b.file(&key);
    let pasted = format!(
        "motd=Salut\nmaxplayers=12\nlanguage=klingon\nupnp=1\npassword={SENTINEL_2}\nworldname=Autre\nmod_x=1\n"
    );
    let reply = b.analyze(&key, &pasted).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert!(!reply.text.contains(SENTINEL_2));
    let view: ImportView = serde_json::from_value(reply.body).unwrap();
    let state = |id: &str| view.lines.iter().find(|l| l.id == id).unwrap().state;
    assert_eq!(state("motd"), ImportState::Same);
    assert_eq!(state("max_players"), ImportState::Take);
    assert_eq!(state("language"), ImportState::Rejected);
    assert_eq!(state("upnp"), ImportState::Imposed);
    assert_eq!(state("world_name"), ImportState::Frozen);
    assert_eq!(state("password"), ImportState::Take);
    let password = view.lines.iter().find(|l| l.id == "password").unwrap();
    assert!(password.secret && password.pasted.is_none());
    assert_eq!(view.unknown, 1);
    assert!(
        view.imposed_undone.contains(&"upnp".to_owned())
            && view.imposed_undone.contains(&"port".to_owned())
    );
    assert_eq!(b.file(&key), before, "rien n'est écrit");
    assert!(b.journal().await.is_empty(), "une analyse ne s'inscrit pas");
    // Un texte que le format ne sait pas découper est refusé entier.
    let bad = b.analyze(&key, "a=1\0b=2").await;
    assert_eq!(
        (bad.status, code(&bad).as_str()),
        (422, "SETTINGS_FILE_UNREADABLE")
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn an_import_applies_the_chosen_lines_reads_the_secret_on_the_agent_and_keeps_the_unknown() {
    let b = bench().await;
    let key = b.install().await;
    let mut with_unknown = b.file(&key);
    with_unknown.push_str("mod_inventé=42\n");
    b.rewrite(&key, with_unknown.as_bytes());
    let stamp = b.view(&key).await.file_stamp.unwrap();
    let pasted = format!("maxplayers=12\npassword={SENTINEL_2}\nfoo_inconnu=1\n");
    let body = json!({
        "base_rev": 0, "file_stamp": stamp, "origin": "import", "effect": "restart",
        "import_text": pasted,
        "changes": [
            {"id": "max_players", "action": "from_import"},
            {"id": "password", "action": "from_import"}
        ]
    });
    let reply = b.save(&key, body).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert!(!reply.text.contains(SENTINEL_2));
    let after = b.file(&key);
    assert!(after.contains("maxplayers=12") && after.contains(&format!("password={SENTINEL_2}")));
    assert!(
        after.contains("mod_inventé=42\n"),
        "l'inconnu du fichier survit à l'import"
    );
    assert!(
        !after.contains("foo_inconnu"),
        "l'inconnu du texte n'est pas ajouté"
    );
    // Une clé qui n'est pas dans le texte collé ne se reprend pas.
    let stamp = b.view(&key).await.file_stamp.unwrap();
    let missing = json!({
        "base_rev": 1, "file_stamp": stamp, "origin": "import", "effect": "restart",
        "import_text": "maxplayers=12\n",
        "changes": [{"id": "motd", "action": "from_import"}]
    });
    assert_eq!(code(&b.save(&key, missing).await), "VALIDATION_ERROR");
    b.agent.shutdown().await;
}

#[tokio::test]
async fn the_other_settings_of_the_file_can_be_changed_or_removed_and_a_secret_looking_one_is_masked()
 {
    let b = bench().await;
    let key = b.install().await;
    let mut file = b.file(&key);
    file.push_str("mod_a=1\nmod_token=SECRETEMENT-CACHE\n");
    b.rewrite(&key, file.as_bytes());
    let view = b.view(&key).await;
    let other = |name: &str| view.others.iter().find(|o| o.key == name).unwrap().clone();
    assert_eq!(other("mod_a").value.as_deref(), Some("1"));
    assert!(other("mod_token").masked && other("mod_token").value.is_none());
    let raw = serde_json::to_string(&view).unwrap();
    assert!(!raw.contains("SECRETEMENT-CACHE"));
    let stamp = view.file_stamp.unwrap();
    let body = json!({
        "base_rev": 0, "file_stamp": stamp, "origin": "form", "effect": "restart",
        "others": [
            {"key": "mod_a", "value": "2"},
            {"key": "mod_token", "remove": true}
        ]
    });
    let reply = b.save(&key, body).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let after = b.file(&key);
    assert!(after.contains("mod_a=2\n") && !after.contains("mod_token"));
    // Une clé que la fiche connaît passe par son réglage.
    let stamp = b.view(&key).await.file_stamp.unwrap();
    let known = json!({
        "base_rev": 1, "file_stamp": stamp, "origin": "form", "effect": "restart",
        "others": [{"key": "motd", "value": "x"}]
    });
    assert_eq!(code(&b.save(&key, known).await), "VALIDATION_ERROR");
    b.agent.shutdown().await;
}
