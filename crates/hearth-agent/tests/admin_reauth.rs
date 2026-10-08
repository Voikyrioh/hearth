//! La confirmation des actes d'administration sur l'API HTTP (HRT-28 et HRT-30, BR-TRUST-036 à 053) : le
//! membre `reauth`, la preuve d'usage `0x05` liée à l'acte, le mot de passe par le chemin de la
//! connexion, l'élévation de 5 minutes. Vraie base SQLite, vraies signatures Ed25519, routeur en processus.
//!
//! L'agent **exige** la confirmation dès la construction du service, et tous les bancs d'essai de l'agent
//! le font aussi : un acte sans `reauth` reçoit `426`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use axum::http::{Method, StatusCode};
use hearth_agent::domain::accounts::Role;
use hearth_proto::admin_act::{ActKind, AdminAct};
use hearth_proto::api::accounts::RoleName;
use hearth_proto::api::reauth::ReauthMode;
use serde_json::{Value, json};
use support::api::{Api, Reply, state_with};
use support::device::{DeviceKey, device_login_body, login_token, reauth_member};
use support::update::Rig;
use support::{Env, PASSWORD, by, env};
use time::Duration;

const WRONG: &str = "Wrong-Horse-9999";
const OTHER_PASSWORD: &str = "Another-Pass-77";
const BINARY: &[u8] = b"nouvel agent";

// ---------------------------------------------------------------------------------------------
// Banc d'essai
// ---------------------------------------------------------------------------------------------

/// Un acte d'administration complet, possédé : de quoi bâtir la requête et l'acte que le client signe.
#[derive(Clone, Debug)]
enum Spec {
    Create { username: String, role: RoleName },
    Role { target: String, role: RoleName },
    Password { target: String },
    Delete { target: String },
    Revoke { target: String },
    Update { version: String, body: Value },
    Attack { enable: bool },
    Own,
    Setting { mode: ReauthMode },
}

fn role_wire(role: RoleName) -> &'static str {
    match role {
        RoleName::Admin => "admin",
        RoleName::Readonly => "readonly",
    }
}

impl Spec {
    fn act(&self) -> AdminAct<'_> {
        match self {
            Self::Create { username, role } => AdminAct::AccountCreate {
                username,
                role: *role,
            },
            Self::Role { target, role } => AdminAct::AccountRole {
                target,
                role: *role,
            },
            Self::Password { target } => AdminAct::AccountPassword { target },
            Self::Delete { target } => AdminAct::AccountDelete { target },
            Self::Revoke { target } => AdminAct::SessionsRevoke { target },
            Self::Update { version, body } => AdminAct::AgentUpdate {
                version,
                sha256: body["sha256"].as_str().unwrap(),
            },
            Self::Attack { enable } => AdminAct::AttackMode { enable: *enable },
            Self::Own => AdminAct::AccountPasswordOwn,
            Self::Setting { mode } => AdminAct::ReauthSetting { mode: *mode },
        }
    }

    /// Méthode, chemin et corps de la requête, sans `reauth`. `current` : l'ancien mot de passe de
    /// `PUT /me/password`.
    fn request(&self, current: &str) -> (Method, String, Value) {
        match self {
            Self::Create { username, role } => (
                Method::POST,
                "/accounts".into(),
                json!({ "username": username, "password": OTHER_PASSWORD, "role": role_wire(*role) }),
            ),
            Self::Role { target, role } => (
                Method::PATCH,
                format!("/accounts/{target}"),
                json!({ "role": role_wire(*role) }),
            ),
            Self::Password { target } => (
                Method::PUT,
                format!("/accounts/{target}/password"),
                json!({ "password": OTHER_PASSWORD }),
            ),
            Self::Delete { target } => (Method::DELETE, format!("/accounts/{target}"), json!({})),
            Self::Revoke { target } => (
                Method::DELETE,
                format!("/accounts/{target}/sessions"),
                json!({}),
            ),
            Self::Update { body, .. } => (Method::POST, "/agent/update".into(), body.clone()),
            Self::Attack { enable } => (
                Method::PUT,
                "/security/attack-mode".into(),
                json!({ "active": enable }),
            ),
            Self::Own => (
                Method::PUT,
                "/me/password".into(),
                json!({ "current": current, "password": OTHER_PASSWORD }),
            ),
            Self::Setting { mode } => (
                Method::PUT,
                "/me/reauth".into(),
                json!({ "password": mode.as_str() }),
            ),
        }
    }

    /// Le même acte avec, tour à tour, chaque cible ou paramètre signé qui change son sens (vide s'il n'en
    /// a pas) : la cible, le nom, le rôle, la version, la somme, le geste, la valeur du réglage.
    fn altered_all(&self) -> Vec<Spec> {
        let flip = |role: RoleName| match role {
            RoleName::Admin => RoleName::Readonly,
            RoleName::Readonly => RoleName::Admin,
        };
        match self {
            Self::Create { role, .. } => vec![
                Self::Create {
                    username: "autre".into(),
                    role: *role,
                },
                Self::Create {
                    username: "nouveau".into(),
                    role: flip(*role),
                },
            ],
            Self::Role { target, role } => vec![
                Self::Role {
                    target: "AUTRE-CIBLE".into(),
                    role: *role,
                },
                Self::Role {
                    target: target.clone(),
                    role: flip(*role),
                },
            ],
            Self::Password { .. } => vec![Self::Password {
                target: "AUTRE-CIBLE".into(),
            }],
            Self::Delete { .. } => vec![Self::Delete {
                target: "AUTRE-CIBLE".into(),
            }],
            Self::Revoke { .. } => vec![Self::Revoke {
                target: "AUTRE-CIBLE".into(),
            }],
            Self::Update { version, body } => {
                let mut other = body.clone();
                other["sha256"] = json!("ab".repeat(32));
                vec![
                    Self::Update {
                        version: "9.9.9".into(),
                        body: body.clone(),
                    },
                    Self::Update {
                        version: version.clone(),
                        body: other,
                    },
                ]
            }
            Self::Attack { enable } => vec![Self::Attack { enable: !enable }],
            Self::Setting { mode } => vec![Self::Setting {
                mode: match mode {
                    ReauthMode::Each => ReauthMode::Window,
                    ReauthMode::Window => ReauthMode::Each,
                },
            }],
            Self::Own => Vec::new(),
        }
    }
}

struct Actor {
    name: &'static str,
    key: std::sync::Arc<DeviceKey>,
    token: String,
}

impl Actor {
    /// Le même poste, une autre session.
    async fn reopened(&self, api: &Api) -> Actor {
        Actor {
            name: self.name,
            key: self.key.clone(),
            token: login_token(api, &self.key, self.name, PASSWORD).await,
        }
    }
}

struct Bench {
    env: Env,
    api: Api,
    rig: Rig,
    marie: Actor,
    carl: Actor,
    paul: Actor,
}

async fn actor(env: &Env, api: &Api, name: &'static str, role: Role) -> Actor {
    env.create(name, role).await;
    let key = std::sync::Arc::new(DeviceKey::new());
    let token = login_token(api, &key, name, PASSWORD).await;
    Actor { name, key, token }
}

async fn bench() -> Bench {
    let env = env().await;
    let rig = Rig::new(&env, true, true);
    let api = Api::from_state(state_with(&env, rig.service.clone()));
    let marie = actor(&env, &api, "marie", Role::Admin).await;
    let carl = actor(&env, &api, "carl", Role::ReadOnly).await;
    let paul = actor(&env, &api, "paul", Role::ReadOnly).await;
    Bench {
        env,
        api,
        rig,
        marie,
        carl,
        paul,
    }
}

impl Bench {
    /// Qui fait l'acte : le changement de son propre mot de passe est celui de `carl`, un compte qui n'est
    /// pas le titulaire des autres vérifications.
    fn actor_for(&self, kind: ActKind) -> &Actor {
        match kind {
            ActKind::AccountPasswordOwn => &self.carl,
            _ => &self.marie,
        }
    }

    /// Un exemple d'acte de ce genre, sur des comptes neufs.
    async fn spec(&self, kind: ActKind) -> Spec {
        match kind {
            ActKind::AccountCreate => Spec::Create {
                username: "nouveau".into(),
                role: RoleName::Readonly,
            },
            ActKind::AccountRole => Spec::Role {
                target: self.victim("v-role").await,
                role: RoleName::Admin,
            },
            ActKind::AccountPassword => Spec::Password {
                target: self.victim("v-pass").await,
            },
            ActKind::AccountDelete => Spec::Delete {
                target: self.victim("v-del").await,
            },
            ActKind::SessionsRevoke => Spec::Revoke {
                target: self.victim("v-sess").await,
            },
            ActKind::AgentUpdate => Spec::Update {
                version: "0.2.0".into(),
                body: self.rig.request("0.2.0", BINARY),
            },
            ActKind::AttackModeEnable => Spec::Attack { enable: true },
            ActKind::AttackModeDisable => {
                // Rien à désactiver si le mode est éteint : on l'allume d'abord, par le service.
                self.env
                    .attack
                    .change(
                        true,
                        by(),
                        hearth_agent::domain::trust::attack_mode::EndHow::Manual,
                    )
                    .await
                    .unwrap();
                Spec::Attack { enable: false }
            }
            ActKind::AccountPasswordOwn => Spec::Own,
            ActKind::ReauthSetting => Spec::Setting {
                mode: ReauthMode::Each,
            },
        }
    }

    async fn victim(&self, name: &str) -> String {
        self.env.create(name, Role::ReadOnly).await.id.to_string()
    }

    /// Le membre `reauth` de cet acte, signé par cet acteur (mot de passe vide : absent).
    async fn member(&self, who: &Actor, spec: &Spec, password: &str) -> Value {
        reauth_member(
            &self.api,
            &who.key,
            who.name,
            &who.token,
            &spec.act(),
            password,
        )
        .await
    }

    /// Envoie l'acte avec ce membre `reauth`.
    async fn send(&self, who: &Actor, spec: &Spec, member: Option<Value>, current: &str) -> Reply {
        let (method, path, mut body) = spec.request(current);
        if let Some(member) = member {
            body["reauth"] = member;
        }
        self.api
            .call(method, &path)
            .token(&who.token)
            .json(&body)
            .send()
            .await
    }

    /// L'acte, confirmé par ce mot de passe (vide : preuve seule).
    async fn act(&self, who: &Actor, spec: &Spec, password: &str) -> Reply {
        let member = self.member(who, spec, password).await;
        let current = if password.is_empty() {
            PASSWORD
        } else {
            password
        };
        self.send(who, spec, Some(member), current).await
    }

    async fn ok_entries(&self, kind: ActKind) -> i64 {
        self.env.audit_recorder.flush_all().await;
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE action = ? AND outcome = 'ok'")
            .bind(kind.audit_code())
            .fetch_one(self.env.db.pool())
            .await
            .unwrap()
    }

    async fn entries(&self, kind: ActKind, outcome: &str, reason: &str) -> i64 {
        self.env.audit_recorder.flush_all().await;
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_events WHERE action = ? AND outcome = ? AND COALESCE(reason, '') LIKE ?",
        )
        .bind(kind.audit_code())
        .bind(outcome)
        .bind(format!("{reason}%"))
        .fetch_one(self.env.db.pool())
        .await
        .unwrap()
    }

    /// Un instantané de ce qu'un acte pourrait changer.
    async fn snapshot(&self) -> String {
        let accounts: Vec<(String, String, String, i64)> = sqlx::query_as(
            "SELECT username, role, password_hash, reauth_window_s FROM accounts ORDER BY username",
        )
        .fetch_all(self.env.db.pool())
        .await
        .unwrap();
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM sessions), (SELECT COUNT(*) FROM trusted_devices), (SELECT active FROM attack_mode WHERE id = 1)",
        )
        .fetch_one(self.env.db.pool())
        .await
        .unwrap();
        format!("{accounts:?} {counts:?}")
    }

    async fn failures(&self) -> i64 {
        sqlx::query_scalar("SELECT COALESCE(MAX(failures), 0) FROM login_attempts")
            .fetch_one(self.env.db.pool())
            .await
            .unwrap()
    }
}

fn reason(reply: &Reply) -> &str {
    reply.body["error"]["details"]["reason"]
        .as_str()
        .unwrap_or("")
}

fn assert_refused_with(reply: &Reply, status: StatusCode, code: &str, why: &str, context: &str) {
    assert_eq!(reply.status, status, "{context} : {:?}", reply.body);
    assert_eq!(reply.code(), code, "{context} : {:?}", reply.body);
    assert_eq!(reason(reply), why, "{context} : {:?}", reply.body);
}

// ---------------------------------------------------------------------------------------------
// Tranche A : le contrat accepté
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn every_admin_act_with_a_valid_confirmation_succeeds_and_leaves_one_success_entry() {
    for kind in ActKind::ALL {
        let b = bench().await;
        let spec = b.spec(kind).await;
        let who = b.actor_for(kind);
        let before = b.ok_entries(kind).await;
        let reply = b.act(who, &spec, PASSWORD).await;
        assert!(
            reply.status.is_success(),
            "{kind:?} : {} {:?}",
            reply.status,
            reply.body
        );
        // La mise à jour de l'agent répond `202` : son entrée réussie est écrite par l'agent qui revient
        // (BR-UPDATE-024), pas par la requête.
        if kind != ActKind::AgentUpdate {
            assert_eq!(b.ok_entries(kind).await - before, 1, "{kind:?}");
        }
    }
}

#[tokio::test]
async fn a_proof_for_another_act_target_account_session_or_replayed_is_refused_and_changes_nothing()
{
    for kind in ActKind::ALL {
        let b = bench().await;
        let spec = b.spec(kind).await;
        let who = b.actor_for(kind);
        let other_session = login_token(&b.api, &who.key, who.name, PASSWORD).await;

        let mut refusals: Vec<(&str, Value)> = Vec::new();
        // Une autre action.
        let other_act = if kind == ActKind::AccountPasswordOwn {
            Spec::Delete { target: "x".into() }
        } else {
            Spec::Own
        };
        refusals.push(("autre acte", b.member(who, &other_act, PASSWORD).await));
        // Une autre cible ou un autre paramètre.
        for altered in spec.altered_all() {
            refusals.push(("autre cible", b.member(who, &altered, PASSWORD).await));
        }
        // Un autre compte : le défi et la signature portent l'identifiant d'un autre.
        refusals.push((
            "autre compte",
            reauth_member(&b.api, &who.key, "paul", &who.token, &spec.act(), PASSWORD).await,
        ));
        // La clé inscrite d'un autre compte.
        refusals.push((
            "clé d'un autre compte",
            reauth_member(
                &b.api,
                &b.paul.key,
                who.name,
                &who.token,
                &spec.act(),
                PASSWORD,
            )
            .await,
        ));
        // Un autre jeton de session.
        refusals.push((
            "autre session",
            reauth_member(
                &b.api,
                &who.key,
                who.name,
                &other_session,
                &spec.act(),
                PASSWORD,
            )
            .await,
        ));

        let current = PASSWORD;
        for (label, member) in refusals {
            let context = format!("{kind:?} / {label}");
            let before = b.snapshot().await;
            let verifications = b.env.hasher.verifications();
            let reply = b.send(who, &spec, Some(member), current).await;
            assert_refused_with(
                &reply,
                StatusCode::CONFLICT,
                "POST_NOT_RECOGNIZED",
                "proof_invalid",
                &context,
            );
            assert_eq!(b.snapshot().await, before, "{context} : base inchangée");
            assert_eq!(
                b.env.hasher.verifications(),
                verifications,
                "{context} : aucun mot de passe n'est essayé"
            );
        }
        assert!(
            b.entries(kind, "denied", "preuve de clé invalide").await >= 1,
            "{kind:?} : le refus est consigné"
        );

        // Une preuve rejouée : le premier envoi réussit, le second est refusé.
        let member = b.member(who, &spec, PASSWORD).await;
        let first = b.send(who, &spec, Some(member.clone()), current).await;
        assert!(first.status.is_success(), "{kind:?} : {:?}", first.body);
        let before = b.snapshot().await;
        let verifications = b.env.hasher.verifications();
        let replay = b.send(who, &spec, Some(member), current).await;
        assert_refused_with(
            &replay,
            StatusCode::CONFLICT,
            "POST_NOT_RECOGNIZED",
            "proof_invalid",
            &format!("{kind:?} / rejeu"),
        );
        assert_eq!(
            b.snapshot().await,
            before,
            "{kind:?} : rejeu, base inchangée"
        );
        assert_eq!(b.env.hasher.verifications(), verifications);
    }
}

#[tokio::test]
async fn a_confirmation_without_a_proof_or_without_a_password_is_refused_with_its_own_reason_and_tries_no_password()
 {
    for kind in ActKind::ALL {
        let b = bench().await;
        let spec = b.spec(kind).await;
        let who = b.actor_for(kind);
        let before = b.snapshot().await;
        let verifications = b.env.hasher.verifications();

        // Le mot de passe sans la preuve de clé : la session volée qui connaît le mot de passe.
        let reply = b
            .send(who, &spec, Some(json!({ "password": PASSWORD })), PASSWORD)
            .await;
        assert_refused_with(
            &reply,
            StatusCode::CONFLICT,
            "POST_NOT_RECOGNIZED",
            "proof_missing",
            &format!("{kind:?} / sans preuve"),
        );
        // Un `reauth` illisible vaut un `reauth` sans rien d'utilisable.
        let reply = b.send(who, &spec, Some(json!(42)), PASSWORD).await;
        assert_refused_with(
            &reply,
            StatusCode::CONFLICT,
            "POST_NOT_RECOGNIZED",
            "proof_missing",
            &format!("{kind:?} / illisible"),
        );
        // La preuve sans le mot de passe, hors élévation.
        let reply = b.act(who, &spec, "").await;
        assert_refused_with(
            &reply,
            StatusCode::CONFLICT,
            "POST_NOT_RECOGNIZED",
            "password_required",
            &format!("{kind:?} / sans mot de passe"),
        );
        assert_eq!(b.snapshot().await, before, "{kind:?} : base inchangée");
        assert_eq!(
            b.env.hasher.verifications(),
            verifications,
            "{kind:?} : aucun mot de passe n'est essayé"
        );
        assert!(b.entries(kind, "denied", "preuve de clé absente").await >= 1);
        assert!(b.entries(kind, "denied", "mot de passe requis").await >= 1);
    }
}

#[tokio::test]
async fn a_wrong_password_at_the_confirmation_counts_as_a_login_failure_and_the_wait_comes_at_the_fifth()
 {
    for kind in ActKind::ALL {
        let b = bench().await;
        let spec = b.spec(kind).await;
        let who = b.actor_for(kind);
        let before = b.snapshot().await;
        let failures = b.failures().await;
        for attempt in 1..=4 {
            let reply = b.act(who, &spec, WRONG).await;
            assert_eq!(
                reply.code(),
                "WRONG_PASSWORD",
                "{kind:?} tentative {attempt} : {:?}",
                reply.body
            );
            assert_eq!(
                b.failures().await,
                failures + attempt,
                "{kind:?} : un échec de connexion de plus"
            );
        }
        // Le cinquième échec ouvre l'attente du couple, exactement comme à la connexion.
        let reply = b.act(who, &spec, WRONG).await;
        assert!(
            matches!(reply.status.as_u16(), 422 | 429),
            "{kind:?} : {:?}",
            reply.body
        );
        let reply = b.act(who, &spec, PASSWORD).await;
        assert_eq!(
            reply.status,
            StatusCode::TOO_MANY_REQUESTS,
            "{kind:?} : {:?}",
            reply.body
        );
        assert_eq!(reply.code(), "TOO_MANY_ATTEMPTS");
        assert!(
            reply.body["error"]["details"]["retry_after_s"]
                .as_u64()
                .unwrap()
                >= 1
        );
        assert_eq!(b.snapshot().await, before, "{kind:?} : rien n'a changé");
        // L'attente est consignée sous l'acte, et le mot de passe faux aussi (constat C5).
        assert!(
            b.entries(kind, "denied", "trop de tentatives").await >= 1,
            "{kind:?} : l'attente est consignée"
        );
        assert!(
            b.entries(kind, "failed", "mot de passe actuel incorrect")
                .await
                >= 1
        );
    }
}

#[tokio::test]
async fn with_reauth_the_old_password_of_the_body_must_be_the_confirmed_one() {
    let b = bench().await;
    let spec = Spec::Own;
    let member = b.member(&b.carl, &spec, PASSWORD).await;
    let reply = b
        .send(&b.carl, &spec, Some(member), "Autre-Ancien-12")
        .await;
    assert_eq!(reply.code(), "VALIDATION_ERROR", "{:?}", reply.body);
}

#[tokio::test]
async fn the_key_of_another_enrolled_device_of_the_account_is_accepted_and_the_key_of_another_account_is_not()
 {
    let b = bench().await;
    // Un second poste de marie : une autre clé, inscrite par une autre connexion.
    let second = DeviceKey::new();
    let _second_token = login_token(&b.api, &second, "marie", PASSWORD).await;
    let spec = Spec::Create {
        username: "par-le-second".into(),
        role: RoleName::Readonly,
    };
    // La session est celle du premier poste ; la preuve vient du second : une clé inscrite du compte suffit.
    let member = reauth_member(
        &b.api,
        &second,
        "marie",
        &b.marie.token,
        &spec.act(),
        PASSWORD,
    )
    .await;
    let reply = b.send(&b.marie, &spec, Some(member), PASSWORD).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
    // La clé de paul, inscrite pour paul : refusée.
    let spec = Spec::Create {
        username: "par-paul".into(),
        role: RoleName::Readonly,
    };
    let member = reauth_member(
        &b.api,
        &b.paul.key,
        "marie",
        &b.marie.token,
        &spec.act(),
        PASSWORD,
    )
    .await;
    let reply = b.send(&b.marie, &spec, Some(member), PASSWORD).await;
    assert_refused_with(
        &reply,
        StatusCode::CONFLICT,
        "POST_NOT_RECOGNIZED",
        "proof_invalid",
        "clé d'un autre compte",
    );
}

#[tokio::test]
async fn two_requests_with_the_same_proof_at_the_same_time_succeed_only_once() {
    let b = bench().await;
    let spec = b.spec(ActKind::SessionsRevoke).await;
    let member = b.member(&b.marie, &spec, PASSWORD).await;
    let (first, second) = tokio::join!(
        b.send(&b.marie, &spec, Some(member.clone()), PASSWORD),
        b.send(&b.marie, &spec, Some(member.clone()), PASSWORD),
    );
    let successes = [&first, &second]
        .iter()
        .filter(|reply| reply.status.is_success())
        .count();
    assert_eq!(successes, 1, "{:?} / {:?}", first.body, second.body);
    let refused = if first.status.is_success() {
        &second
    } else {
        &first
    };
    assert_eq!(reason(refused), "proof_invalid", "{:?}", refused.body);
}

#[tokio::test]
async fn the_journal_keeps_no_password_no_challenge_no_signature_and_no_key() {
    let b = bench().await;
    let mut secrets: Vec<String> = vec![PASSWORD.into(), WRONG.into(), OTHER_PASSWORD.into()];
    for kind in [
        ActKind::AccountCreate,
        ActKind::AccountPassword,
        ActKind::AccountPasswordOwn,
    ] {
        let spec = b.spec(kind).await;
        let who = b.actor_for(kind);
        for password in [WRONG, PASSWORD, ""] {
            let member = b.member(who, &spec, password).await;
            for field in ["challenge", "signature", "public_key"] {
                secrets.push(member["device"][field].as_str().unwrap().to_owned());
            }
            let current = if password.is_empty() {
                PASSWORD
            } else {
                password
            };
            b.send(who, &spec, Some(member), current).await;
        }
        // Un membre sans preuve.
        b.send(who, &spec, Some(json!({ "password": WRONG })), WRONG)
            .await;
    }
    b.env.audit_recorder.flush_all().await;
    let dump: String = sqlx::query_scalar(
        "SELECT json_group_array(json_object('account', account, 'name', origin_name, 'addr', origin_addr, 'action', action, 'label', action_label, 'target', target, 'reason', reason)) FROM audit_events",
    )
    .fetch_one(b.env.db.pool())
    .await
    .unwrap();
    assert!(dump.len() > 100, "le journal n'est pas vide");
    for secret in secrets {
        assert!(!secret.is_empty());
        assert!(
            !dump.contains(&secret),
            "un secret est dans le journal : {secret}"
        );
    }
}

#[tokio::test]
async fn when_the_agent_requires_the_confirmation_an_act_without_reauth_is_told_to_update_the_client()
 {
    let b = bench().await;
    let reply = b.api.get("/security").token(&b.marie.token).send().await;
    assert_eq!(reply.body["admin_reauth"]["required"], true);
    for kind in ActKind::ALL {
        let spec = b.spec(kind).await;
        let who = b.actor_for(kind);
        let before = b.snapshot().await;
        let (method, path, body) = spec.request(PASSWORD);
        let reply = b
            .api
            .call(method, &path)
            .token(&who.token)
            .json(&body)
            .send()
            .await;
        if matches!(kind, ActKind::AttackModeEnable | ActKind::AttackModeDisable) {
            // Le mode attaque garde sa forme à plat (usage `0x03`), aussi stricte : sans preuve, refus.
            assert_refused_with(
                &reply,
                StatusCode::CONFLICT,
                "POST_NOT_RECOGNIZED",
                "proof_missing",
                &format!("{kind:?}"),
            );
        } else {
            assert_eq!(
                reply.status,
                StatusCode::UPGRADE_REQUIRED,
                "{kind:?} : {:?}",
                reply.body
            );
            assert_eq!(reply.code(), "INCOMPATIBLE_VERSION");
            assert_eq!(reply.body["error"]["details"]["upgrade"], "client");
            assert_eq!(reason(&reply), "reauth_required");
            assert!(
                b.entries(kind, "denied", "confirmation absente").await >= 1,
                "{kind:?}"
            );
        }
        assert_eq!(b.snapshot().await, before, "{kind:?} : rien n'a changé");
    }
}

#[tokio::test]
async fn the_removal_of_a_device_keeps_its_delivered_contract_even_when_the_agent_requires_the_confirmation()
 {
    let b = bench().await;
    let second = DeviceKey::new();
    login_token(&b.api, &second, "marie", PASSWORD).await;
    let devices = b.api.get("/me/devices").token(&b.marie.token).send().await;
    let target = devices.body["devices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|device| device["current"] == false)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let body = support::device::removal_body(
        &b.api,
        &b.marie.key,
        "marie",
        &b.marie.token,
        &target,
        PASSWORD,
    )
    .await;
    let reply = b
        .api
        .delete(&format!("/me/devices/{target}"))
        .token(&b.marie.token)
        .json(&body)
        .send()
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{:?}", reply.body);
}

// ---------------------------------------------------------------------------------------------
// Tranche D1 : le réglage et l'élévation de 5 minutes
// ---------------------------------------------------------------------------------------------

fn readonly_account(name: &str) -> Spec {
    Spec::Create {
        username: name.into(),
        role: RoleName::Readonly,
    }
}

async fn elevated_for(b: &Bench) -> u64 {
    let reply = b.api.get("/security").token(&b.marie.token).send().await;
    reply.body["admin_reauth"]["elevated_for_s"]
        .as_u64()
        .unwrap()
}

#[tokio::test]
async fn a_right_password_opens_five_minutes_where_a_covered_act_needs_only_the_proof_and_a_use_never_extends_it()
 {
    let b = bench().await;
    let marie = &b.marie;
    assert_eq!(elevated_for(&b).await, 0);
    let reply = b.act(marie, &readonly_account("usr1"), PASSWORD).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
    assert!(elevated_for(&b).await >= 299);
    let verifications = b.env.hasher.verifications();
    // La preuve seule suffit pour un acte couvert, sans appel au hacheur.
    let reply = b.act(marie, &readonly_account("usr2"), "").await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
    assert_eq!(
        b.env.hasher.verifications(),
        verifications,
        "aucune vérification de mot de passe sous élévation"
    );
    // Quatre minutes après la saisie : toujours couvert, malgré l'usage entre-temps.
    b.env.monotonic.advance(Duration::minutes(4));
    let left = elevated_for(&b).await;
    assert!((55..=60).contains(&left), "{left}");
    let reply = b.act(marie, &readonly_account("usr3"), "").await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
    // Cinq minutes depuis la saisie : fermée, l'usage n'a rien prolongé.
    b.env
        .monotonic
        .advance(Duration::minutes(1) + Duration::milliseconds(1));
    let reply = b.act(marie, &readonly_account("usr4"), "").await;
    assert_refused_with(
        &reply,
        StatusCode::CONFLICT,
        "POST_NOT_RECOGNIZED",
        "password_required",
        "expirée",
    );
    assert_eq!(elevated_for(&b).await, 0);
    // Une nouvelle saisie la relance.
    let reply = b.act(marie, &readonly_account("usr4"), PASSWORD).await;
    assert_eq!(reply.status, StatusCode::CREATED);
    assert!(elevated_for(&b).await >= 299);
}

#[tokio::test]
async fn under_the_elevation_the_proof_of_the_key_is_still_required_every_time() {
    let b = bench().await;
    b.act(&b.marie, &readonly_account("usr1"), PASSWORD).await;
    let before = b.snapshot().await;
    // Pas de membre `reauth` du tout n'est pas une confirmation (l'agent n'exige pas encore : l'acte
    // passerait comme avant ; mais un membre sans preuve est refusé).
    let reply = b
        .send(
            &b.marie,
            &readonly_account("usr2"),
            Some(json!({})),
            PASSWORD,
        )
        .await;
    assert_refused_with(
        &reply,
        StatusCode::CONFLICT,
        "POST_NOT_RECOGNIZED",
        "proof_missing",
        "sans preuve",
    );
    // Une preuve d'un autre acte non plus.
    let member = b.member(&b.marie, &Spec::Own, "").await;
    let reply = b
        .send(&b.marie, &readonly_account("usr2"), Some(member), PASSWORD)
        .await;
    assert_refused_with(
        &reply,
        StatusCode::CONFLICT,
        "POST_NOT_RECOGNIZED",
        "proof_invalid",
        "autre acte",
    );
    assert_eq!(b.snapshot().await, before);
}

#[tokio::test]
async fn the_acts_the_elevation_never_covers_ask_for_the_password_during_the_elevation() {
    let b = bench().await;
    for (index, kind) in [
        ActKind::AccountPassword,
        ActKind::AgentUpdate,
        ActKind::AttackModeEnable,
        ActKind::ReauthSetting,
        ActKind::AttackModeDisable,
    ]
    .into_iter()
    .enumerate()
    {
        let spec = b.spec(kind).await;
        // On rouvre l'élévation par un acte couvert avec le mot de passe, puis on tente l'acte exclu.
        let opened = b
            .act(
                &b.marie,
                &readonly_account(&format!("ouvre-{index}")),
                PASSWORD,
            )
            .await;
        assert_eq!(
            opened.status,
            StatusCode::CREATED,
            "{kind:?} : {:?}",
            opened.body
        );
        let before = b.snapshot().await;
        let reply = b.act(&b.marie, &spec, "").await;
        assert_refused_with(
            &reply,
            StatusCode::CONFLICT,
            "POST_NOT_RECOGNIZED",
            "password_required",
            &format!("{kind:?} sous élévation"),
        );
        assert_eq!(b.snapshot().await, before, "{kind:?}");
    }
    // Le dernier tour a allumé le mode attaque (pour pouvoir l'éteindre) : on l'éteint, les élévations
    // existent de nouveau.
    b.env
        .attack
        .change(
            false,
            by(),
            hearth_agent::domain::trust::attack_mode::EndHow::Manual,
        )
        .await
        .unwrap();
    // Donner le rôle Administrateur : par un changement de rôle et par une création.
    let target = b.victim("v-admin").await;
    for (index, spec) in [
        Spec::Role {
            target,
            role: RoleName::Admin,
        },
        Spec::Create {
            username: "futur-admin".into(),
            role: RoleName::Admin,
        },
    ]
    .into_iter()
    .enumerate()
    {
        b.act(
            &b.marie,
            &readonly_account(&format!("rouvre-{index}")),
            PASSWORD,
        )
        .await;
        let before = b.snapshot().await;
        let reply = b.act(&b.marie, &spec, "").await;
        assert_refused_with(
            &reply,
            StatusCode::CONFLICT,
            "POST_NOT_RECOGNIZED",
            "password_required",
            &format!("{spec:?}"),
        );
        assert_eq!(b.snapshot().await, before);
        // Et ce qui retire ou limite un accès reste couvert.
    }
    // Les quatre actes couverts passent sous l'élévation.
    b.act(&b.marie, &readonly_account("rouvre2"), PASSWORD)
        .await;
    for kind in [ActKind::AccountDelete, ActKind::SessionsRevoke] {
        let spec = b.spec(kind).await;
        let reply = b.act(&b.marie, &spec, "").await;
        assert!(reply.status.is_success(), "{kind:?} : {:?}", reply.body);
    }
    let limit = Spec::Role {
        target: b.victim("v-limit").await,
        role: RoleName::Readonly,
    };
    let reply = b.act(&b.marie, &limit, "").await;
    assert!(reply.status.is_success(), "{:?}", reply.body);
}

#[tokio::test]
async fn changing_your_own_password_asks_for_the_password_during_the_elevation_and_closes_it() {
    let b = bench().await;
    // Carl ouvre une élévation (le réglage demande le mot de passe et l'ouvre).
    let setting = Spec::Setting {
        mode: ReauthMode::Window,
    };
    let reply = b.act(&b.carl, &setting, PASSWORD).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    let reply = b.act(&b.carl, &Spec::Own, "").await;
    assert_refused_with(
        &reply,
        StatusCode::CONFLICT,
        "POST_NOT_RECOGNIZED",
        "password_required",
        "propre mot de passe",
    );
    let reply = b.act(&b.carl, &Spec::Own, PASSWORD).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert!(
        b.env.elevations.is_empty(),
        "le changement du mot de passe ferme l'élévation"
    );
}

/// Ouvre l'élévation de marie par un acte couvert et rend le numéro d'ordre pour nommer les comptes.
async fn open(b: &Bench) {
    let reply = b
        .act(&b.marie, &readonly_account("ouverture"), PASSWORD)
        .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
    assert!(b.env.elevations.covers(
        &session_of(b, &b.marie.token).await,
        &device_of(b, &b.marie.key).await,
        "10.0.0.7"
    ));
}

async fn session_of(b: &Bench, token: &str) -> hearth_agent::domain::sessions::SessionId {
    b.env.sessions.authenticate(token).await.unwrap().session_id
}

async fn device_of(b: &Bench, key: &DeviceKey) -> hearth_agent::domain::trust::DeviceId {
    let id: String = sqlx::query_scalar("SELECT id FROM trusted_devices WHERE key_id = ?")
        .bind(key.key_id())
        .fetch_one(b.env.db.pool())
        .await
        .unwrap();
    hearth_agent::domain::trust::DeviceId::new(id)
}

async fn assert_closed(b: &Bench, cause: &str) {
    let reply = b
        .act(&b.marie, &readonly_account(&format!("apres-{cause}")), "")
        .await;
    assert_refused_with(
        &reply,
        StatusCode::CONFLICT,
        "POST_NOT_RECOGNIZED",
        "password_required",
        cause,
    );
}

#[tokio::test]
async fn the_elevation_ends_with_the_session() {
    let b = bench().await;
    open(&b).await;
    let reply = b
        .api
        .delete("/sessions/current")
        .token(&b.marie.token)
        .send()
        .await;
    assert!(reply.status.is_success(), "{:?}", reply.body);
    assert!(b.env.elevations.is_empty());
    // Une nouvelle session du même poste n'hérite de rien.
    let marie = b.marie.reopened(&b.api).await;
    let spec = readonly_account("apres-session");
    let reply = b.act(&marie, &spec, "").await;
    assert_refused_with(
        &reply,
        StatusCode::CONFLICT,
        "POST_NOT_RECOGNIZED",
        "password_required",
        "nouvelle session",
    );
}

#[tokio::test]
async fn the_elevation_lives_in_memory_only_so_a_restart_of_the_service_starts_without_one() {
    let b = bench().await;
    open(&b).await;
    // Rien en base : aucune table, aucune colonne d'élévation.
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE name LIKE '%elev%'")
            .fetch_all(b.env.db.pool())
            .await
            .unwrap();
    assert!(tables.is_empty(), "{tables:?}");
    // Un service neuf (le redémarrage) ne connaît aucune élévation.
    let fresh = hearth_agent::application::elevation::Elevations::new(b.env.monotonic.clone());
    assert!(fresh.is_empty());
}

#[tokio::test]
async fn changing_the_password_or_the_role_of_the_account_closes_the_elevation() {
    let b = bench().await;
    open(&b).await;
    let marie = b.env.service.find("marie").await.unwrap().id;
    // Le titulaire change son mot de passe : sa session est gardée, son élévation non.
    let own = b
        .env
        .service
        .change_own_password(
            &marie,
            support::secret(PASSWORD),
            support::secret("Nouveau-Cheval-55"),
            Some(session_of(&b, &b.marie.token).await),
            by(),
        )
        .await;
    assert!(own.is_ok(), "{own:?}");
    assert_closed(&b, "mot-de-passe").await;

    let b = bench().await;
    open(&b).await;
    let marie = b.env.service.find("marie").await.unwrap().id;
    b.env
        .service
        .change_role(&marie, Role::Admin, by())
        .await
        .unwrap();
    assert_closed(&b, "role").await;
}

#[tokio::test]
async fn removing_the_device_of_the_elevation_closes_it() {
    let b = bench().await;
    let second = DeviceKey::new();
    login_token(&b.api, &second, "marie", PASSWORD).await;
    // L'élévation est ouverte avec la clé du second poste.
    let opened = reauth_member(
        &b.api,
        &second,
        "marie",
        &b.marie.token,
        &readonly_account("ouverture").act(),
        PASSWORD,
    )
    .await;
    let reply = b
        .send(
            &b.marie,
            &readonly_account("ouverture"),
            Some(opened),
            PASSWORD,
        )
        .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
    assert_eq!(b.env.elevations.len(), 1);
    // Le premier poste (courant) retire le second.
    let target = device_of(&b, &second).await;
    let body = support::device::removal_body(
        &b.api,
        &b.marie.key,
        "marie",
        &b.marie.token,
        target.as_str(),
        PASSWORD,
    )
    .await;
    let reply = b
        .api
        .delete(&format!("/me/devices/{}", target.as_str()))
        .token(&b.marie.token)
        .json(&body)
        .send()
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{:?}", reply.body);
    assert!(
        b.env.elevations.is_empty(),
        "l'élévation du poste retiré est fermée"
    );
}

#[tokio::test]
async fn the_attack_mode_closes_every_elevation_and_none_exists_while_it_lasts() {
    let b = bench().await;
    open(&b).await;
    let enable = Spec::Attack { enable: true };
    let reply = b.act(&b.marie, &enable, PASSWORD).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert!(
        b.env.elevations.is_empty(),
        "l'activation ferme toutes les élévations"
    );
    // Pendant le mode : la confirmation avec mot de passe réussit, mais n'ouvre rien.
    let reply = b
        .act(&b.marie, &readonly_account("pendant"), PASSWORD)
        .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
    assert!(b.env.elevations.is_empty());
    assert_closed(&b, "mode-attaque").await;
    assert_eq!(elevated_for(&b).await, 0);
    // Le mode éteint : aucune élévation n'est revenue d'elle-même.
    let off = b
        .act(&b.marie, &Spec::Attack { enable: false }, PASSWORD)
        .await;
    assert_eq!(off.status, StatusCode::OK, "{:?}", off.body);
    assert_closed(&b, "apres-extinction").await;
}

#[tokio::test]
async fn setting_each_closes_the_elevation_and_asks_the_password_for_every_act() {
    let b = bench().await;
    open(&b).await;
    let reply = b
        .act(
            &b.marie,
            &Spec::Setting {
                mode: ReauthMode::Each,
            },
            PASSWORD,
        )
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(reply.body["password"], "each");
    assert!(b.env.elevations.is_empty());
    assert_closed(&b, "reglage-each").await;
    // Deux actes avec mot de passe : le mot de passe est redemandé à chaque fois.
    for name in ["aaa1", "aaa2"] {
        let reply = b.act(&b.marie, &readonly_account(name), PASSWORD).await;
        assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
        assert!(
            b.env.elevations.is_empty(),
            "rien ne s'ouvre avec le réglage each"
        );
        assert_closed(&b, &format!("each-{name}")).await;
    }
    let reply = b.api.get("/security").token(&b.marie.token).send().await;
    assert_eq!(reply.body["admin_reauth"]["password"], "each");
    // Revenir à `window` demande aussi le mot de passe et la clé.
    let reply = b
        .act(
            &b.marie,
            &Spec::Setting {
                mode: ReauthMode::Window,
            },
            "",
        )
        .await;
    assert_eq!(reason(&reply), "password_required");
    let reply = b
        .act(
            &b.marie,
            &Spec::Setting {
                mode: ReauthMode::Window,
            },
            PASSWORD,
        )
        .await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(
        b.entries(ActKind::ReauthSetting, "denied", "mot de passe requis")
            .await,
        1
    );
}

#[tokio::test]
async fn the_first_wrong_password_of_the_account_at_a_confirmation_closes_the_elevation() {
    let b = bench().await;
    open(&b).await;
    let reply = b
        .act(
            &b.marie,
            &Spec::Setting {
                mode: ReauthMode::Each,
            },
            WRONG,
        )
        .await;
    assert_eq!(reply.code(), "WRONG_PASSWORD");
    assert!(b.env.elevations.is_empty());
    assert_closed(&b, "mauvais-mot-de-passe").await;
}

#[tokio::test]
async fn the_elevation_is_not_valid_for_another_session_another_device_or_another_address() {
    let b = bench().await;
    open(&b).await;
    // Une autre session du même poste.
    let other = b.marie.reopened(&b.api).await;
    let reply = b.act(&other, &readonly_account("autre-session"), "").await;
    assert_eq!(reason(&reply), "password_required", "{:?}", reply.body);
    // Un autre poste (une autre clé inscrite) avec la session de l'élévation.
    let second = DeviceKey::new();
    login_token(&b.api, &second, "marie", PASSWORD).await;
    let spec = readonly_account("autre-poste");
    let member = reauth_member(&b.api, &second, "marie", &b.marie.token, &spec.act(), "").await;
    let reply = b.send(&b.marie, &spec, Some(member), PASSWORD).await;
    assert_eq!(reason(&reply), "password_required", "{:?}", reply.body);
    // Une autre adresse (le défi est demandé, et la preuve signée, depuis elle).
    let mut elsewhere = Api::new(&b.env);
    elsewhere.addr = std::net::SocketAddr::from(([10, 0, 0, 99], 40_000));
    let spec = readonly_account("autre-adresse");
    let member = reauth_member(
        &elsewhere,
        &b.marie.key,
        "marie",
        &b.marie.token,
        &spec.act(),
        "",
    )
    .await;
    let (method, path, mut body) = spec.request(PASSWORD);
    body["reauth"] = member;
    let reply = elsewhere
        .call(method, &path)
        .token(&b.marie.token)
        .json(&body)
        .send()
        .await;
    assert_eq!(reason(&reply), "password_required", "{:?}", reply.body);
    // Et celle d'origine tient toujours.
    let reply = b.act(&b.marie, &readonly_account("origine"), "").await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
}

#[tokio::test]
async fn setting_the_wall_clock_back_or_forward_never_moves_the_five_minutes() {
    let b = bench().await;
    open(&b).await;
    // L'heure murale avance d'un jour mais le temps réel d'une minute : toujours couvert.
    b.env.clock.advance(Duration::days(1));
    b.env.monotonic.advance(Duration::minutes(1));
    let reply = b.act(&b.marie, &readonly_account("avance"), "").await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
    // L'heure murale recule d'un jour, six minutes de temps réel : fermée.
    b.env.clock.advance(Duration::days(-2));
    b.env.monotonic.advance(Duration::minutes(5));
    assert_closed(&b, "heure-reculee").await;
}

#[tokio::test]
async fn the_setting_is_per_account_any_role_and_always_confirmed() {
    let b = bench().await;
    // Sans `reauth`, même quand l'agent n'exige pas encore : `426`.
    let reply = b
        .api
        .put("/me/reauth")
        .token(&b.carl.token)
        .json(&json!({ "password": "each" }))
        .send()
        .await;
    assert_eq!(
        reply.status,
        StatusCode::UPGRADE_REQUIRED,
        "{:?}",
        reply.body
    );
    assert_eq!(reason(&reply), "reauth_required");
    // Un compte en lecture seule règle le sien, avec mot de passe et clé.
    let reply = b
        .act(
            &b.carl,
            &Spec::Setting {
                mode: ReauthMode::Each,
            },
            PASSWORD,
        )
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    let carl = b.api.get("/security").token(&b.carl.token).send().await;
    let marie = b.api.get("/security").token(&b.marie.token).send().await;
    assert_eq!(carl.body["admin_reauth"]["password"], "each");
    assert_eq!(
        marie.body["admin_reauth"]["password"], "window",
        "le réglage est celui du compte"
    );
    // Le réglage change seulement avec le mot de passe ET la clé.
    let reply = b
        .send(
            &b.carl,
            &Spec::Setting {
                mode: ReauthMode::Window,
            },
            Some(json!({ "password": PASSWORD })),
            PASSWORD,
        )
        .await;
    assert_eq!(reason(&reply), "proof_missing");
    let reply = b
        .act(
            &b.carl,
            &Spec::Setting {
                mode: ReauthMode::Window,
            },
            "",
        )
        .await;
    assert_eq!(reason(&reply), "password_required");
    assert_eq!(b.entries(ActKind::ReauthSetting, "ok", "").await, 1);
}

// ---------------------------------------------------------------------------------------------
// Revue r1 de la PR #34
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn an_act_is_refused_when_its_challenge_cannot_be_retained_and_the_proof_never_replays() {
    // B1 : la part du compte dans la table des défis consommés est de 64 (sa connexion en a pris un).
    let b = bench().await;
    open_as(&b, "fx-ouverture").await;
    let mut created = 0;
    let mut refused_member = None;
    for index in 0..70 {
        let spec = readonly_account(&format!("fx{index:02}"));
        let member = b.member(&b.marie, &spec, "").await;
        let reply = b
            .send(&b.marie, &spec, Some(member.clone()), PASSWORD)
            .await;
        if reply.status == StatusCode::CREATED {
            created += 1;
        } else {
            assert_refused_with(
                &reply,
                StatusCode::CONFLICT,
                "POST_NOT_RECOGNIZED",
                "proof_invalid",
                "part pleine",
            );
            refused_member = Some((spec, member));
            break;
        }
    }
    let (spec, member) = refused_member.expect("la part du compte se remplit avant 70 actes");
    // La part du compte est de 64 défis consommés ; sa connexion en a pris un et l'ouverture un autre : les
    // 62 premiers actes passent, le 63e est refusé. Le compte est exact, pas une borne large (suivi r2).
    assert_eq!(
        created,
        hearth_agent::domain::trust::challenge::MAX_CONSUMED_PER_OWNER - 2,
        "{created}"
    );
    let before = b.snapshot().await;
    let again = b.send(&b.marie, &spec, Some(member), PASSWORD).await;
    assert_eq!(reason(&again), "proof_invalid");
    assert_eq!(
        b.snapshot().await,
        before,
        "l'acte refusé n'est pas fait, ni rejoué"
    );
}

#[tokio::test]
async fn the_hash_verified_by_the_confirmation_is_the_one_the_password_change_replaces() {
    let b = bench().await;
    let carl = b.env.service.find("carl").await.unwrap().id;
    let error = b
        .env
        .service
        .change_own_password_confirmed(
            &carl,
            &support::secret("un-ancien-hache"),
            support::secret(OTHER_PASSWORD),
            None,
            None,
            by(),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            hearth_agent::application::accounts::AccountError::PasswordChangedMeanwhile
        ),
        "{error:?}"
    );
}

#[tokio::test]
async fn the_flat_attack_mode_closes_the_elevations_when_it_turns_on() {
    let b = bench().await;
    open_as(&b, "flat-ouverte").await;
    assert!(!b.env.elevations.is_empty());
    let body = support::device::attack_mode_body(
        &b.api,
        &b.marie.key,
        "marie",
        &b.marie.token,
        true,
        PASSWORD,
    )
    .await;
    let reply = b
        .api
        .put("/security/attack-mode")
        .token(&b.marie.token)
        .json(&body)
        .send()
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert!(b.env.elevations.is_empty(), "forme à plat");
}

async fn open_as(b: &Bench, name: &str) {
    let reply = b.act(&b.marie, &readonly_account(name), PASSWORD).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
}

#[tokio::test]
async fn a_wrong_password_on_a_flat_form_closes_the_elevation_too() {
    let b = bench().await;
    open_as(&b, "ouverte").await;
    assert!(!b.env.elevations.is_empty());
    let body = support::device::attack_mode_body(
        &b.api,
        &b.marie.key,
        "marie",
        &b.marie.token,
        true,
        WRONG,
    )
    .await;
    let reply = b
        .api
        .put("/security/attack-mode")
        .token(&b.marie.token)
        .json(&body)
        .send()
        .await;
    assert_eq!(reply.code(), "WRONG_PASSWORD");
    assert!(b.env.elevations.is_empty());
}

#[tokio::test]
async fn deleting_the_account_or_closing_its_sessions_closes_its_elevations() {
    let b = bench().await;
    let marie = b.env.service.find("marie").await.unwrap().id;
    open_as(&b, "revoque").await;
    b.env.service.revoke_sessions(&marie, by()).await.unwrap();
    assert!(b.env.elevations.is_empty(), "fermeture des sessions");
    // Un compte qui a une élévation, puis qui est supprimé.
    let reply = b
        .act(
            &b.paul,
            &Spec::Setting {
                mode: ReauthMode::Window,
            },
            PASSWORD,
        )
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(b.env.elevations.len(), 1);
    let paul = b.env.service.find("paul").await.unwrap().id;
    b.env.service.delete(&paul, None, None, by()).await.unwrap();
    assert!(b.env.elevations.is_empty(), "suppression du compte");
}

#[tokio::test]
async fn a_body_with_two_reauth_members_is_unreadable_not_absent() {
    let b = bench().await;
    let before = b.snapshot().await;
    let reply = b
        .api
        .post("/accounts")
        .token(&b.marie.token)
        .raw_body(&format!(
            r#"{{"username":"double","password":"{OTHER_PASSWORD}","role":"readonly","reauth":{{}},"reauth":{{}}}}"#
        ))
        .send()
        .await;
    assert_eq!(
        reply.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{:?}",
        reply.body
    );
    assert_eq!(reply.code(), "VALIDATION_ERROR");
    assert_eq!(b.snapshot().await, before);
}

// ---------------------------------------------------------------------------------------------
// HRT-30, tranche C : l'agent EXIGE. Les voies de secours d'un compte sans poste inscrit (BR-TRUST-044),
// chacune avec son test, sur un agent qui exige la confirmation.
// ---------------------------------------------------------------------------------------------

/// La VRAIE ligne de commande du serveur (`hearth-agent --data-dir ... <sous-commande>`), lancée en
/// processus sur la base du banc, sans réseau : c'est la voie de secours que le runbook écrit (BR-TRUST-044).
fn server_command(b: &Bench, args: &[&str]) {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_hearth-agent"))
        .arg("--data-dir")
        .arg(b.env.dir.path())
        .args(args)
        .env_remove("HEARTH_CONFIG")
        .env_remove("HEARTH_DATA_DIR")
        .output()
        .expect("lancement de hearth-agent");
    assert!(
        output.status.success(),
        "{args:?} : {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn rescue_first_login_after_the_install_enrolls_the_device_and_the_first_act_passes() {
    let b = bench().await;
    // Base neuve : le compte de l'installation se connecte avec sa clé (inscription dans la transaction),
    // puis agit tout de suite.
    let first = actor(&b.env, &b.api, "install", Role::Admin).await;
    let reply = b.act(&first, &readonly_account("premier"), PASSWORD).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
}

#[tokio::test]
async fn rescue_a_lost_key_is_replaced_by_a_login_with_the_password_and_the_act_passes() {
    let b = bench().await;
    let lost = std::sync::Arc::new(DeviceKey::new());
    let token = login_token(&b.api, &lost, "marie", PASSWORD).await;
    let new_pc = Actor {
        name: "marie",
        key: lost,
        token,
    };
    let reply = b
        .act(&new_pc, &readonly_account("apres-perte"), PASSWORD)
        .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
}

#[tokio::test]
async fn rescue_an_account_at_eight_devices_is_unlocked_by_the_real_account_revoke_command_then_a_login()
 {
    let b = bench().await;
    b.env.create("plein", Role::Admin).await;
    for _ in 0..hearth_proto::api::devices::MAX_DEVICES_PER_ACCOUNT {
        login_token(&b.api, &DeviceKey::new(), "plein", PASSWORD).await;
    }
    // Le neuvième poste : connexion accordée mais NON inscrite ; ses actes sont refusés (aucun repli vers
    // « la session suffit »).
    let ninth = std::sync::Arc::new(DeviceKey::new());
    let body = device_login_body(&b.api, &ninth, "plein", PASSWORD).await;
    let reply = b.api.post("/sessions").json(&body).send().await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
    assert_eq!(reply.body["device"], "limit", "{:?}", reply.body);
    let stuck = Actor {
        name: "plein",
        key: ninth.clone(),
        token: reply.body["token"].as_str().unwrap().to_owned(),
    };
    let refused = b.act(&stuck, &readonly_account("bloque"), PASSWORD).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{:?}", refused.body);
    assert_eq!(reason(&refused), "proof_invalid");
    // `hearth-agent account revoke plein` (oublie les postes et les adresses), puis la connexion inscrit.
    server_command(&b, &["account", "revoke", "plein"]);
    let body = device_login_body(&b.api, &ninth, "plein", PASSWORD).await;
    let reply = b.api.post("/sessions").json(&body).send().await;
    assert_eq!(reply.body["device"], "enrolled", "{:?}", reply.body);
    let freed = Actor {
        name: "plein",
        key: ninth,
        token: reply.body["token"].as_str().unwrap().to_owned(),
    };
    let reply = b.act(&freed, &readonly_account("debloque"), PASSWORD).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
}

#[tokio::test]
async fn rescue_with_the_attack_mode_on_and_no_key_the_real_attack_mode_off_command_then_a_login_enrolls()
 {
    let b = bench().await;
    b.env
        .attack
        .change(
            true,
            by(),
            hearth_agent::domain::trust::attack_mode::EndHow::Manual,
        )
        .await
        .unwrap();
    // Une clé neuve pendant le mode attaque : l'inscription est gelée (rien n'est inscrit).
    let fresh = std::sync::Arc::new(DeviceKey::new());
    let body = device_login_body(&b.api, &fresh, "marie", PASSWORD).await;
    let frozen = b.api.post("/sessions").json(&body).send().await;
    assert_ne!(frozen.body["device"], "enrolled", "{:?}", frozen.body);
    // `hearth-agent attack-mode off` (la vraie sous-commande, sans réseau), puis la connexion inscrit.
    server_command(&b, &["attack-mode", "off"]);
    let body = device_login_body(&b.api, &fresh, "marie", PASSWORD).await;
    let reply = b.api.post("/sessions").json(&body).send().await;
    assert_eq!(reply.body["device"], "enrolled", "{:?}", reply.body);
    let enrolled = Actor {
        name: "marie",
        key: fresh,
        token: reply.body["token"].as_str().unwrap().to_owned(),
    };
    let reply = b
        .act(&enrolled, &readonly_account("apres-cli"), PASSWORD)
        .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
}

#[tokio::test]
async fn an_old_client_is_refused_on_every_act_with_the_typed_error_and_keeps_its_reads() {
    let b = bench().await;
    for kind in ActKind::ALL {
        if matches!(kind, ActKind::AttackModeEnable | ActKind::AttackModeDisable) {
            continue; // forme à plat tolérée : voir `when_the_agent_requires_...`
        }
        let spec = b.spec(kind).await;
        let who = b.actor_for(kind);
        let (method, path, body) = spec.request(PASSWORD);
        let reply = b
            .api
            .call(method, &path)
            .token(&who.token)
            .json(&body)
            .send()
            .await;
        assert_eq!(reply.status, StatusCode::UPGRADE_REQUIRED, "{kind:?}");
        assert_eq!(reply.code(), "INCOMPATIBLE_VERSION", "{kind:?}");
        assert_eq!(
            reply.body["error"]["details"]["upgrade"], "client",
            "{kind:?}"
        );
        // La session reste valable : la lecture passe (ni lien ni lecture perdus).
        let read = b.api.get("/accounts").token(&b.marie.token).send().await;
        assert_eq!(read.status, StatusCode::OK, "{kind:?}");
    }
}

/// Suivi r2 : sur une route sans corps attendu, ce que l'agent fait d'un corps `[]` ou `null` est écrit ici.
/// `[]` (une séquence vide) ne porte pas de `reauth` : « absent », donc, quand l'agent exige, un client
/// trop ancien (`426`). `null` n'est pas un objet : corps illisible (`422`). Dans les deux cas rien n'est
/// fait ; aucun des deux n'ouvre une voie vers « la session suffit ».
#[tokio::test]
async fn an_array_body_is_absent_and_a_null_body_is_unreadable_on_a_route_without_a_body_and_nothing_is_done()
 {
    let b = bench().await;
    let target = b.victim("v-corps").await;
    for (raw, status) in [
        ("[]", StatusCode::UPGRADE_REQUIRED),
        ("null", StatusCode::UNPROCESSABLE_ENTITY),
    ] {
        let before = b.snapshot().await;
        let reply = b
            .api
            .delete(&format!("/accounts/{target}/sessions"))
            .token(&b.marie.token)
            .raw_body(raw)
            .send()
            .await;
        assert_eq!(reply.status, status, "{raw} : {:?}", reply.body);
        assert_eq!(b.snapshot().await, before, "{raw}");
    }
}

// ---------------------------------------------------------------------------------------------
// HRT-18 tranche 3 : l'ouverture d'une élévation est consignée (BR-TRUST-053)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn opening_an_elevation_is_journaled_with_who_which_device_and_when_never_the_password() {
    let b = bench().await;
    // Un acte couvert sous élévation n'en ouvre pas une deuxième : une seule entrée par ouverture.
    open(&b).await;
    let reply = b.act(&b.marie, &readonly_account("deuxieme"), "").await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
    b.env.audit_recorder.flush_all().await;
    let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT COALESCE(account, ''), COALESCE(origin_name, ''), COALESCE(target, ''), outcome, action_label FROM audit_events WHERE action = 'reauth.elevation'",
    )
    .fetch_all(b.env.db.pool())
    .await
    .unwrap();
    assert_eq!(rows.len(), 1, "{rows:?}");
    let (account, _origin, target, outcome, label) = &rows[0];
    assert_eq!(account, "marie");
    assert!(
        target.starts_with("poste "),
        "le poste de la clé : {target}"
    );
    assert_eq!(outcome, "ok");
    assert_eq!(label, "Délai du mot de passe ouvert (5 minutes)");
    // L'horodatage est celui de l'entrée ; aucune ligne ne porte le mot de passe.
    let dump: String = sqlx::query_scalar(
        "SELECT json_group_array(json_object('a', account, 'n', origin_name, 'ad', origin_addr, 't', target, 'r', reason)) FROM audit_events WHERE action = 'reauth.elevation'",
    )
    .fetch_one(b.env.db.pool())
    .await
    .unwrap();
    assert!(!dump.contains(PASSWORD), "{dump}");
    // Un mot de passe juste ouvre le délai même pour un acte non couvert (ici le réglage, qui le referme
    // aussitôt) : l'ouverture est consignée, une entrée de plus, au nom de paul.
    let each = b
        .act(
            &b.paul,
            &Spec::Setting {
                mode: ReauthMode::Each,
            },
            PASSWORD,
        )
        .await;
    assert_eq!(each.status, StatusCode::OK, "{:?}", each.body);
    b.env.audit_recorder.flush_all().await;
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE action = 'reauth.elevation'")
            .fetch_one(b.env.db.pool())
            .await
            .unwrap();
    assert_eq!(count, 2);
    let who: Vec<String> = sqlx::query_scalar(
        "SELECT COALESCE(account, '') FROM audit_events WHERE action = 'reauth.elevation' ORDER BY id",
    )
    .fetch_all(b.env.db.pool())
    .await
    .unwrap();
    assert_eq!(who, ["marie", "paul"]);
}
