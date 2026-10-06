//! HRT-13 : la gestion des comptes de la coquille contre un vrai agent (TLS 1.3, SQLite, journal) :
//! liste, création, rôle, mots de passe, fermeture des sessions, suppression, dernier
//! administrateur, refus pour un compte Lecture seule qui force l'appel, et une action coupée avant
//! sa réponse (résultat inconnu, jamais rejouée, issue relue au retour du lien).
//!
//! Déterminisme : aucune assertion de durée ; chaque attente porte sur un fait observable (état du
//! lien, action arrivée chez l'agent, opération terminée côté agent).
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "../../../../crates/hearth-link/tests/support/agent.rs"]
mod agent;
#[path = "../../../../crates/hearth-link/tests/support/proxy.rs"]
mod proxy;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent::{PASSWORD, TestAgent};
use hearth_agent::domain::accounts::Role;
use hearth_desktop_lib::accounts::dto::{
    AccountDto, AccountListDto, AccountOutcome, AccountRefusal, PasswordRuleDto,
};
use hearth_desktop_lib::accounts::service;
use hearth_desktop_lib::link::{LinkRuntime, UiSink};
use hearth_desktop_lib::link_dto::{InvalidField, LinkFailure, RoleDto};
use hearth_desktop_lib::vault::{CredentialBackend, CredentialVault};
use hearth_link::LinkConfig;
use hearth_link::domain::server::ServerId;
use hearth_link::domain::state::{LinkState, Thresholds};
use proxy::FaultProxy;

const GUARD: Duration = Duration::from_secs(60);
const NEW_PASSWORD: &str = "Sunny-Walk-Home-42";

#[derive(Default)]
struct Memory(Mutex<HashMap<String, Vec<u8>>>);

#[derive(Clone)]
struct Shared(Arc<Memory>);

impl CredentialBackend for Shared {
    fn read(&self, target: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.0.lock().unwrap().get(target).cloned())
    }
    fn write(&self, target: &str, secret: &[u8]) -> Result<(), String> {
        self.0
            .0
            .lock()
            .unwrap()
            .insert(target.into(), secret.into());
        Ok(())
    }
    fn remove(&self, target: &str) -> Result<(), String> {
        self.0.0.lock().unwrap().remove(target);
        Ok(())
    }
}

struct Nowhere;

impl UiSink for Nowhere {
    fn emit(&self, _: &str, _: serde_json::Value) {}
}

async fn eventually(what: &str, mut condition: impl FnMut() -> bool) {
    let started = Instant::now();
    while !condition() {
        assert!(started.elapsed() < GUARD, "attendu : {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Seuils du lien : hors d'atteinte d'une machine lente, sauf si le scénario les éprouve.
fn config(reconnecting: bool, offline: bool) -> LinkConfig {
    let hour = Duration::from_secs(3_600);
    let scaled = Thresholds::scaled(6);
    LinkConfig {
        thresholds: Thresholds {
            silence: hour,
            reconnecting_after: if reconnecting {
                scaled.reconnecting_after
            } else {
                hour
            },
            offline_after: if offline {
                scaled.offline_after
            } else {
                hour * 2
            },
            ..scaled
        },
        attempt_timeout: Duration::from_secs(30),
        request_timeout: Duration::from_secs(30),
        ..LinkConfig::default()
    }
}

/// Un client connecté à l'agent (par `port`) sous ce compte : sa propre session.
struct Client {
    runtime: Arc<LinkRuntime>,
    id: ServerId,
    _dir: tempfile::TempDir,
}

async fn client(port: u16, username: &str, password: &str, config: LinkConfig) -> Client {
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(Memory::default());
    let vault = Arc::new(CredentialVault::new(Shared(secrets)));
    let runtime = Arc::new(
        LinkRuntime::open_with(dir.path(), vault, "poste-test/0.1", config)
            .await
            .unwrap(),
    );
    let probe = runtime.probe("127.0.0.1", Some(port)).await.unwrap();
    let server = runtime
        .add_and_login(
            "forge".into(),
            2,
            "127.0.0.1".into(),
            Some(port),
            &probe.fingerprint,
            probe.mac_addresses,
            username,
            password.into(),
            false,
            &Nowhere,
        )
        .await
        .unwrap();
    let id = ServerId::parse(&server.id).unwrap();
    let client = Client {
        runtime,
        id,
        _dir: dir,
    };
    client.wait_for(LinkState::Connected).await;
    client
}

impl Client {
    async fn wait_for(&self, state: LinkState) {
        eventually(&format!("état {state:?}"), || {
            self.runtime.manager().state(&self.id).unwrap().state == state
        })
        .await;
    }

    async fn list(&self) -> AccountListDto {
        service::list(self.runtime.manager(), &self.id)
            .await
            .unwrap()
    }

    async fn accounts(&self) -> Vec<AccountDto> {
        match self.list().await {
            AccountListDto::Listed { accounts } => accounts,
            other => panic!("liste attendue, reçu {other:?}"),
        }
    }

    async fn account(&self, username: &str) -> AccountDto {
        self.accounts()
            .await
            .into_iter()
            .find(|account| account.username == username)
            .unwrap_or_else(|| panic!("compte {username} absent de la liste"))
    }

    async fn create(&self, username: &str, password: &str, role: RoleDto) -> AccountOutcome {
        service::create(self.runtime.manager(), &self.id, username, password, role)
            .await
            .unwrap()
    }

    async fn set_role(&self, account: &str, role: RoleDto) -> AccountOutcome {
        service::change_role(self.runtime.manager(), &self.id, account, role)
            .await
            .unwrap()
    }

    async fn set_password(&self, account: &AccountDto, password: &str) -> AccountOutcome {
        service::set_password(
            self.runtime.manager(),
            &self.id,
            &account.id,
            &account.username,
            password,
        )
        .await
        .unwrap()
    }

    async fn close_sessions(&self, account: &str) -> AccountOutcome {
        service::close_sessions(self.runtime.manager(), &self.id, account)
            .await
            .unwrap()
    }

    async fn delete(&self, account: &str, confirmation: Option<&str>) -> AccountOutcome {
        service::delete(
            self.runtime.manager(),
            &self.id,
            account,
            confirmation.map(str::to_owned),
        )
        .await
        .unwrap()
    }

    async fn change_own_password(&self, current: &str, password: &str) -> AccountOutcome {
        service::change_own_password(self.runtime.manager(), &self.id, current, password)
            .await
            .unwrap()
    }
}

fn refused(outcome: &AccountOutcome) -> &AccountRefusal {
    match outcome {
        AccountOutcome::Refused { refusal } => refusal,
        other => panic!("refus attendu, reçu {other:?}"),
    }
}

fn done(outcome: AccountOutcome) -> (Option<AccountDto>, u32) {
    match outcome {
        AccountOutcome::Done {
            account,
            sessions_closed,
        } => (account, sessions_closed),
        other => panic!("succès attendu, reçu {other:?}"),
    }
}

/// Un agent avec `marie` (Administrateur) et son client connecté.
async fn rig() -> (TestAgent, Client) {
    let agent = TestAgent::install().await;
    agent.create_account("marie", Role::Admin).await;
    let admin = client(agent.addr.port(), "marie", PASSWORD, config(false, false)).await;
    (agent, admin)
}

#[tokio::test]
async fn the_list_shows_the_accounts_and_a_creation_adds_one_without_ever_returning_a_password() {
    let (_agent, admin) = rig().await;
    let accounts = admin.accounts().await;
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].username, "marie");
    assert_eq!(accounts[0].role, RoleDto::Admin);
    assert_eq!(accounts[0].sessions_open, 1, "la session de ce client");
    assert!(accounts[0].last_login_at.is_some());

    let (created, closed) = done(admin.create("paul", NEW_PASSWORD, RoleDto::Readonly).await);
    let created = created.expect("le compte créé");
    assert_eq!(
        (created.username.as_str(), created.role),
        ("paul", RoleDto::Readonly)
    );
    assert_eq!(closed, 0);
    assert!(
        !format!("{created:?}").contains(NEW_PASSWORD),
        "aucun mot de passe en retour"
    );
    let accounts = admin.accounts().await;
    assert_eq!(accounts.len(), 2);
    let paul = accounts.iter().find(|a| a.username == "paul").unwrap();
    assert_eq!(paul.last_login_at, None, "jamais connecté");
    assert_eq!(paul.sessions_open, 0);
}

#[tokio::test]
async fn a_taken_username_a_bad_format_and_a_weak_password_are_refused() {
    let (_agent, admin) = rig().await;
    // Même identifiant à la casse près : l'agent refuse (BR-ACCT-003).
    let taken = admin.create("MARIE", NEW_PASSWORD, RoleDto::Readonly).await;
    assert_eq!(refused(&taken), &AccountRefusal::UsernameTaken);
    // Refus local, avant tout envoi : rien n'est créé.
    let bad = admin.create("a b", NEW_PASSWORD, RoleDto::Readonly).await;
    assert!(matches!(
        refused(&bad),
        AccountRefusal::InvalidUsername { problem: Some(_) }
    ));
    let weak = admin.create("paul", "abc", RoleDto::Readonly).await;
    assert_eq!(
        refused(&weak),
        &AccountRefusal::WeakPassword {
            rules: vec![
                PasswordRuleDto::MinLength,
                PasswordRuleDto::Digit,
                PasswordRuleDto::Uppercase
            ]
        }
    );
    assert_eq!(admin.accounts().await.len(), 1);
}

#[tokio::test]
async fn roles_change_and_the_last_administrator_can_be_neither_demoted_nor_removed() {
    let (_agent, admin) = rig().await;
    done(admin.create("paul", NEW_PASSWORD, RoleDto::Readonly).await);
    let marie = admin.account("marie").await;
    let paul = admin.account("paul").await;

    let demote_last = admin.set_role(&marie.id, RoleDto::Readonly).await;
    assert_eq!(refused(&demote_last), &AccountRefusal::LastAdmin);
    let remove_last = admin.delete(&marie.id, Some("marie")).await;
    assert_eq!(refused(&remove_last), &AccountRefusal::LastAdmin);
    assert_eq!(admin.account("marie").await.role, RoleDto::Admin);

    // Un compte inconnu, ou un identifiant qui ne peut pas être un chemin.
    let unknown = admin
        .set_role("ZZZZZZZZZZZZZZZZZZZZZZZZZZ", RoleDto::Admin)
        .await;
    assert_eq!(refused(&unknown), &AccountRefusal::NotFound);
    let bad = service::change_role(admin.runtime.manager(), &admin.id, "../me", RoleDto::Admin)
        .await
        .unwrap_err();
    assert_eq!(
        bad,
        LinkFailure::InvalidInput {
            field: InvalidField::Other
        }
    );

    done(admin.set_role(&paul.id, RoleDto::Admin).await);
    assert_eq!(admin.account("paul").await.role, RoleDto::Admin);
    // Deux administrateurs : marie peut maintenant être rétrogradée ; l'agent applique le nouveau
    // rôle tout de suite, même à la session ouverte avec l'ancien (le client n'est pas l'arbitre).
    done(admin.set_role(&marie.id, RoleDto::Readonly).await);
    assert_eq!(
        admin.list().await,
        AccountListDto::Refused {
            refusal: AccountRefusal::Forbidden
        }
    );
}

#[tokio::test]
async fn an_admin_setting_a_password_closes_every_session_of_that_account_and_only_that_one() {
    let (agent, admin) = rig().await;
    done(admin.create("paul", NEW_PASSWORD, RoleDto::Readonly).await);
    let paul_client = client(
        agent.addr.port(),
        "paul",
        NEW_PASSWORD,
        config(false, false),
    )
    .await;
    assert_eq!(agent.sessions_open("paul").await, 1);
    let paul = admin.account("paul").await;

    let (_, closed) = done(admin.set_password(&paul, "Another-Long-Pass-91").await);
    assert_eq!(closed, 1);
    assert_eq!(agent.sessions_open("paul").await, 0);
    assert_eq!(
        agent.sessions_open("marie").await,
        1,
        "les autres comptes n'en sont pas touchés"
    );
    paul_client.wait_for(LinkState::AccessRevoked).await;
    // Le nouveau mot de passe ouvre une session ; l'ancien n'ouvre plus rien.
    client(
        agent.addr.port(),
        "paul",
        "Another-Long-Pass-91",
        config(false, false),
    )
    .await;
}

#[tokio::test]
async fn closing_the_sessions_of_an_account_keeps_its_password() {
    let (agent, admin) = rig().await;
    done(admin.create("paul", NEW_PASSWORD, RoleDto::Readonly).await);
    let paul_client = client(
        agent.addr.port(),
        "paul",
        NEW_PASSWORD,
        config(false, false),
    )
    .await;
    let paul = admin.account("paul").await;
    assert_eq!(paul.sessions_open, 1);

    let (_, closed) = done(admin.close_sessions(&paul.id).await);
    assert_eq!(closed, 1);
    assert_eq!(admin.account("paul").await.sessions_open, 0);
    paul_client.wait_for(LinkState::AccessRevoked).await;
    // Le même mot de passe rouvre tout de suite une session.
    client(
        agent.addr.port(),
        "paul",
        NEW_PASSWORD,
        config(false, false),
    )
    .await;
}

#[tokio::test]
async fn deleting_an_account_closes_its_sessions_and_an_unknown_one_is_refused() {
    let (agent, admin) = rig().await;
    done(admin.create("paul", NEW_PASSWORD, RoleDto::Readonly).await);
    let paul_client = client(
        agent.addr.port(),
        "paul",
        NEW_PASSWORD,
        config(false, false),
    )
    .await;
    let paul = admin.account("paul").await;

    let (_, closed) = done(admin.delete(&paul.id, None).await);
    assert_eq!(closed, 1);
    paul_client.wait_for(LinkState::AccessRevoked).await;
    assert_eq!(admin.accounts().await.len(), 1);
    let again = admin.delete(&paul.id, None).await;
    assert_eq!(refused(&again), &AccountRefusal::NotFound);
}

#[tokio::test]
async fn deleting_your_own_account_asks_for_your_username_and_ends_your_session() {
    let (_agent, admin) = rig().await;
    done(admin.create("paul", NEW_PASSWORD, RoleDto::Admin).await);
    let marie = admin.account("marie").await;

    let none = admin.delete(&marie.id, None).await;
    assert_eq!(refused(&none), &AccountRefusal::ConfirmationMismatch);
    let wrong = admin.delete(&marie.id, Some("paul")).await;
    assert_eq!(refused(&wrong), &AccountRefusal::ConfirmationMismatch);
    assert_eq!(admin.accounts().await.len(), 2);

    let (_, closed) = done(admin.delete(&marie.id, Some("marie")).await);
    assert!(closed >= 1);
    // La session du compte supprimé n'est plus : l'agent le dit, l'interface s'en tient là.
    match admin.list().await {
        AccountListDto::Refused { refusal } => assert_eq!(refusal, AccountRefusal::SessionEnded),
        other => panic!("session terminée attendue, reçu {other:?}"),
    }
}

#[tokio::test]
async fn changing_your_own_password_asks_for_the_old_one_and_keeps_the_current_session() {
    let (agent, admin) = rig().await;
    // Une seconde session du même compte, sur un autre client.
    let other = client(agent.addr.port(), "marie", PASSWORD, config(false, false)).await;
    assert_eq!(agent.sessions_open("marie").await, 2);

    let wrong = admin
        .change_own_password("Not-The-Old-One-1", NEW_PASSWORD)
        .await;
    assert_eq!(refused(&wrong), &AccountRefusal::WrongPassword);
    let weak = admin.change_own_password(PASSWORD, "marie").await;
    assert!(matches!(
        refused(&weak),
        AccountRefusal::WeakPassword { .. }
    ));
    assert_eq!(agent.sessions_open("marie").await, 2, "rien n'a bougé");

    let (_, closed) = done(admin.change_own_password(PASSWORD, NEW_PASSWORD).await);
    assert_eq!(closed, 1, "les AUTRES sessions");
    assert_eq!(agent.sessions_open("marie").await, 1);
    other.wait_for(LinkState::AccessRevoked).await;
    // La session courante tient : la liste se lit encore.
    assert_eq!(admin.accounts().await.len(), 1);
    // Le nouveau mot de passe ouvre une session.
    client(
        agent.addr.port(),
        "marie",
        NEW_PASSWORD,
        config(false, false),
    )
    .await;
}

#[tokio::test]
async fn a_read_only_account_that_forces_every_call_is_refused_by_the_agent() {
    let (agent, admin) = rig().await;
    done(admin.create("paul", NEW_PASSWORD, RoleDto::Readonly).await);
    let marie = admin.account("marie").await;
    let paul = admin.account("paul").await;
    let readonly = client(
        agent.addr.port(),
        "paul",
        NEW_PASSWORD,
        config(false, false),
    )
    .await;

    let forbidden = AccountRefusal::Forbidden;
    assert_eq!(
        readonly.list().await,
        AccountListDto::Refused {
            refusal: AccountRefusal::Forbidden
        }
    );
    assert_eq!(
        refused(&readonly.create("lea", NEW_PASSWORD, RoleDto::Admin).await),
        &forbidden
    );
    assert_eq!(
        refused(&readonly.set_role(&paul.id, RoleDto::Admin).await),
        &forbidden
    );
    assert_eq!(
        refused(&readonly.set_password(&marie, "Another-Long-Pass-91").await),
        &forbidden
    );
    assert_eq!(
        refused(&readonly.close_sessions(&marie.id).await),
        &forbidden
    );
    assert_eq!(refused(&readonly.delete(&marie.id, None).await), &forbidden);
    // Chaque refus est consigné au journal de l'agent, au nom de ce compte (BR-ACCT-016,
    // BR-AUDIT-003) : six appels refusés, six entrées « refusé ».
    agent.services.audit_recorder.flush_all().await;
    let page = agent
        .services
        .audit
        .search(
            Role::Admin,
            &hearth_agent::domain::audit::AuditFilter::new(Default::default()).unwrap(),
        )
        .await
        .unwrap();
    let denied = page
        .records
        .iter()
        .filter(|record| {
            record.account.as_deref() == Some("paul")
                && record.outcome == hearth_agent::domain::audit::OutcomeKind::Denied
        })
        .count();
    assert_eq!(denied, 6, "{:?}", page.records);
    // Rien n'a bougé côté serveur.
    assert_eq!(admin.accounts().await.len(), 2);
    assert_eq!(admin.account("paul").await.role, RoleDto::Readonly);
    assert_eq!(agent.sessions_open("marie").await, 1);

    // Mais chacun change son propre mot de passe (BR-ACCT-013).
    let (_, closed) = done(
        readonly
            .change_own_password(NEW_PASSWORD, "Another-Long-Pass-91")
            .await,
    );
    assert_eq!(closed, 0, "aucune autre session");
}

#[tokio::test]
async fn an_action_cut_before_its_answer_is_unknown_never_replayed_and_its_outcome_comes_back() {
    let agent = TestAgent::install().await;
    agent.create_account("marie", Role::Admin).await;
    let proxy = FaultProxy::start(agent.addr).await;
    let admin = client(proxy.port(), "marie", PASSWORD, config(true, true)).await;

    // L'agent retient la vérification du mot de passe : l'action reste « en cours » chez lui.
    agent.hold_actions();
    let started = agent.verifications_started();
    let runtime = admin.runtime.clone();
    let server = admin.id.clone();
    let call = tokio::spawn(async move {
        service::change_own_password(runtime.manager(), &server, PASSWORD, NEW_PASSWORD).await
    });
    agent.wait_action_started(started).await;
    proxy.cut();
    let outcome = tokio::time::timeout(GUARD, call)
        .await
        .expect("l'appel ne reste pas suspendu")
        .unwrap()
        .unwrap();
    let AccountOutcome::Unknown { op_id } = outcome else {
        panic!("résultat inconnu attendu, reçu {outcome:?}");
    };
    // Jamais rejouée : une seule exécution côté agent.
    assert_eq!(agent.verifications_started(), started + 1);

    // L'agent finit pendant la coupure ; le lien revient ; l'issue est relue, une fois.
    agent.release_actions();
    agent.wait_operation_settled("marie", &op_id).await;
    proxy.heal();
    eventually("issue de l'opération relue", || {
        admin
            .runtime
            .operations()
            .iter()
            .any(|event| event.op_id == op_id)
    })
    .await;
    let events: Vec<_> = admin
        .runtime
        .operations()
        .into_iter()
        .filter(|event| event.op_id == op_id)
        .collect();
    assert_eq!(events.len(), 1);
    assert_eq!(
        serde_json::to_value(&events[0]).unwrap()["outcome"],
        "done",
        "« fait pendant la coupure »"
    );
    assert_eq!(agent.verifications_started(), started + 1, "pas rejouée");
    // Le mot de passe a bien été changé, une fois.
    admin.wait_for(LinkState::Connected).await;
    client(
        agent.addr.port(),
        "marie",
        NEW_PASSWORD,
        config(false, false),
    )
    .await;
}

#[tokio::test]
async fn an_action_is_refused_without_anything_sent_when_the_link_is_not_connected() {
    let agent = TestAgent::install().await;
    agent.create_account("marie", Role::Admin).await;
    let proxy = FaultProxy::start(agent.addr).await;
    let admin = client(proxy.port(), "marie", PASSWORD, config(true, true)).await;
    proxy.cut();
    admin.wait_for(LinkState::Offline).await;
    let started = agent.verifications_started();
    let created = service::create(
        admin.runtime.manager(),
        &admin.id,
        "paul",
        NEW_PASSWORD,
        RoleDto::Readonly,
    )
    .await;
    assert_eq!(created.unwrap_err(), LinkFailure::NotConnected);
    let listed = service::list(admin.runtime.manager(), &admin.id).await;
    assert_eq!(listed.unwrap_err(), LinkFailure::NotConnected);
    assert_eq!(agent.verifications_started(), started, "rien n'est parti");
}
