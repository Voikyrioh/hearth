//! Quel acte d'administration une requête porte (HRT-30, BR-TRUST-036, 037, 039) : l'acte est
//! RECONSTRUIT depuis la requête qui va partir (méthode, chemin, corps), comme l'agent le fait de son
//! côté. Ce que la clé signe est donc, par construction, ce qui est envoyé : aucune description d'acte
//! à part qui pourrait diverger de la requête.
//!
//! La liste des routes est celle de `hearth_proto::admin_act::ROUTES` (source unique, tenue par le test
//! de garde de l'agent) : ce module ne la recopie pas, il la lit. Une route de la liste que ce module ne
//! sait pas reconstruire fait échouer `every_route_of_the_closed_list_is_understood`.
//!
//! Pur : aucune E/S.

use std::borrow::Cow;

use hearth_proto::admin_act::{
    ActKind, AdminAct, Contract, ROUTES, RouteAct, decode_segment, is_service_key,
};
use hearth_proto::api::accounts::RoleName;
use hearth_proto::api::reauth::ReauthMode;
use hearth_proto::api::service_ops::OperationChoice;
use serde_json::Value;

/// Un identifiant de compte de l'agent : lettres et chiffres, rien d'autre (jamais `/`, `..`, `?`).
const ID_MAX_LEN: usize = 64;

/// Ce que dit une requête sur l'administration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route<'a> {
    /// Hors de la liste fermée : une lecture, ou une route que l'agent ne garde pas par `reauth`.
    Free,
    /// Le retrait d'un poste de confiance : il garde son contrat livré (usage `0x04`, clé du poste
    /// courant, champs à plat, Q18).
    LegacyRemoval,
    /// Un acte du contrat unique (usage `0x05`).
    Act(AdminAct<'a>),
}

/// La requête vise une route d'acte mais son corps ne dit pas l'acte (champ absent ou illisible).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unreadable;

fn is_id(text: &str) -> bool {
    !text.is_empty() && text.len() <= ID_MAX_LEN && text.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// La clé d'un service lue dans un segment du chemin (`{key}`, HRT-80), décodée comme le fait l'agent
/// (`decode_segment`, source unique). Le segment ne contient, tel qu'il part, que des caractères qui
/// s'écrivent sans encodage ou l'encodage en pourcentage : une barre oblique inverse nue n'est pas permise
/// (les URL la lisent comme une `/`) ; elle voyage en `%5C`. Le segment est découpé AVANT le décodage :
/// un `%2F` décodé donne une clé invalide (rejetée ici), jamais un autre segment. `None` : pas une clé,
/// la requête part comme une route inconnue de l'agent, sans pouvoir se faire passer pour un acte.
fn key_of(segment: &str) -> Option<Cow<'_, str>> {
    if !segment
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':' | b'@' | b'%'))
    {
        return None;
    }
    let key = if segment.contains('%') {
        Cow::Owned(decode_segment(segment)?)
    } else {
        Cow::Borrowed(segment)
    };
    is_service_key(&key).then_some(key)
}

/// La ligne de la liste fermée que vise cette requête, avec l'identifiant du chemin quand le motif en a
/// un (`{id}` d'un compte, ou `{key}` d'un service). Un chemin dont l'identifiant n'a pas la forme d'un identifiant n'est PAS reconnu : il part, s'il
/// part, comme une route inconnue de l'agent (qui répondra `404`), sans pouvoir se faire passer pour
/// un autre acte.
pub fn route_of<'a>(method: &str, path: &'a str) -> Option<(&'static RouteAct, Cow<'a, str>)> {
    let wanted: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    ROUTES.iter().find_map(|route| {
        if route.method != method {
            return None;
        }
        let pattern: Vec<&str> = route.pattern.trim_start_matches('/').split('/').collect();
        if pattern.len() != wanted.len() {
            return None;
        }
        let mut id = Cow::Borrowed("");
        for (expected, got) in pattern.iter().zip(&wanted) {
            if *expected == "{id}" {
                if !is_id(got) {
                    return None;
                }
                id = Cow::Borrowed(*got);
            } else if *expected == "{key}" {
                id = key_of(got)?;
            } else if expected != got {
                return None;
            }
        }
        Some((route, id))
    })
}

fn text<'a>(body: Option<&'a Value>, field: &str) -> Result<&'a str, Unreadable> {
    body.and_then(|body| body.get(field))
        .and_then(Value::as_str)
        .ok_or(Unreadable)
}

fn role(body: Option<&Value>) -> Result<RoleName, Unreadable> {
    match text(body, "role")? {
        "admin" => Ok(RoleName::Admin),
        "readonly" => Ok(RoleName::Readonly),
        _ => Err(Unreadable),
    }
}

fn mode(body: Option<&Value>) -> Result<ReauthMode, Unreadable> {
    match text(body, "password")? {
        "window" => Ok(ReauthMode::Window),
        "each" => Ok(ReauthMode::Each),
        _ => Err(Unreadable),
    }
}

/// Le choix d'une décision d'opération (`choice`). Un choix que ce client ne connaît pas n'est pas un acte :
/// il ne se signe pas (l'agent ne l'a jamais offert).
fn choice(body: Option<&Value>) -> Result<OperationChoice, Unreadable> {
    match text(body, "choice")? {
        "continue" => Ok(OperationChoice::Continue),
        "cancel" => Ok(OperationChoice::Cancel),
        "revert" => Ok(OperationChoice::Revert),
        "keep" => Ok(OperationChoice::Keep),
        "verified" => Ok(OperationChoice::Verified),
        _ => Err(Unreadable),
    }
}

/// Les clés d'une adoption (`keys`, un tableau de textes) : un corps dont une entrée n'est pas un texte
/// n'est pas un acte.
fn keys(body: Option<&Value>) -> Result<Vec<&str>, Unreadable> {
    body.and_then(|body| body.get("keys"))
        .and_then(Value::as_array)
        .ok_or(Unreadable)?
        .iter()
        .map(|key| key.as_str().ok_or(Unreadable))
        .collect()
}

/// L'acte d'un genre donné, lu dans le chemin (`id`) et le corps. Correspondance EXHAUSTIVE : un genre
/// d'acte de plus ne compile pas tant qu'on ne dit pas comment le reconstruire.
fn build<'a>(
    kind: ActKind,
    id: Cow<'a, str>,
    body: Option<&'a Value>,
) -> Result<AdminAct<'a>, Unreadable> {
    // Seule la clé d'un service peut être décodée (texte possédé) ; l'identifiant d'un compte est emprunté.
    let id_ref = || match &id {
        Cow::Borrowed(text) => Ok(*text),
        Cow::Owned(_) => Err(Unreadable),
    };
    let active = body
        .and_then(|body| body.get("active"))
        .and_then(Value::as_bool);
    Ok(match kind {
        ActKind::AccountCreate => AdminAct::AccountCreate {
            username: text(body, "username")?,
            role: role(body)?,
        },
        ActKind::AccountRole => AdminAct::AccountRole {
            target: id_ref()?,
            role: role(body)?,
        },
        ActKind::AccountPassword => AdminAct::AccountPassword { target: id_ref()? },
        ActKind::AccountDelete => AdminAct::AccountDelete { target: id_ref()? },
        ActKind::SessionsRevoke => AdminAct::SessionsRevoke { target: id_ref()? },
        ActKind::AgentUpdate => AdminAct::AgentUpdate {
            version: text(body, "version")?,
            sha256: text(body, "sha256")?,
        },
        ActKind::AttackModeEnable if active == Some(true) => AdminAct::AttackMode { enable: true },
        ActKind::AttackModeDisable if active == Some(false) => {
            AdminAct::AttackMode { enable: false }
        }
        ActKind::AttackModeEnable | ActKind::AttackModeDisable => return Err(Unreadable),
        ActKind::AccountPasswordOwn => AdminAct::AccountPasswordOwn,
        ActKind::ReauthSetting => AdminAct::ReauthSetting { mode: mode(body)? },
        ActKind::ServicesAdopt => AdminAct::services_adopt(keys(body)?),
        ActKind::ServiceRelease => AdminAct::ServiceRelease { target: id },
        // Démarrer, arrêter, redémarrer (HRT-64, BR-LIFE-016) : la clé du service, lue du chemin, est la cible
        // signée ; le corps ne porte que la confirmation.
        ActKind::ServiceStart => AdminAct::ServiceStart { target: id },
        ActKind::ServiceStop => AdminAct::ServiceStop { target: id },
        ActKind::ServiceRestart => AdminAct::ServiceRestart { target: id },
        // Annuler et décider une opération de service (HRT-144, ADR-0045) : l'identifiant vient du chemin,
        // le choix du corps.
        ActKind::OperationCancel => AdminAct::OperationCancel { target: id_ref()? },
        ActKind::OperationDecide => AdminAct::OperationDecide {
            target: id_ref()?,
            choice: choice(body)?,
        },
        // Déclarés pour les tranches suivantes, sans route dans `ROUTES` : aucune requête ne peut les viser
        // (`route_of` ne lit que `ROUTES`). Le lot qui ajoute leur route écrit ici comment les reconstruire.
        ActKind::ServiceDeploy
        | ActKind::ServiceDelete
        | ActKind::BackupCreate
        | ActKind::BackupDelete
        | ActKind::GameSettingsSave => return Err(Unreadable),
    })
}

/// Classe une requête : libre, retrait d'un poste, ou acte reconstruit.
pub fn classify<'a>(
    method: &str,
    path: &'a str,
    body: Option<&'a Value>,
) -> Result<Route<'a>, Unreadable> {
    let Some((route, id)) = route_of(method, path) else {
        return Ok(Route::Free);
    };
    if route.contract == Contract::LegacyRemoval {
        return Ok(Route::LegacyRemoval);
    }
    route
        .kinds
        .iter()
        .find_map(|kind| build(*kind, id.clone(), body).ok())
        .map(Route::Act)
        .ok_or(Unreadable)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Un chemin et un corps d'exemple pour chaque route de la liste.
    fn sample(route: &RouteAct) -> (String, Value) {
        let path = route
            .pattern
            .replace("{id}", "01ABC")
            .replace("{key}", "u_docker-terraria.service");
        let body = match route.pattern {
            "/accounts" => json!({ "username": "paul", "role": "readonly" }),
            "/accounts/{id}" if route.method == "PATCH" => json!({ "role": "admin" }),
            "/agent/update" => json!({ "version": "0.2.0", "sha256": "ab" }),
            "/security/attack-mode" => json!({ "active": true }),
            "/me/reauth" => json!({ "password": "each" }),
            "/services/adopt" => json!({ "keys": ["c_0123abcd", "u_docker-x.service"] }),
            "/operations/services/{id}/decision" => json!({ "choice": "keep" }),
            _ => json!({}),
        };
        (path, body)
    }

    #[test]
    fn every_route_of_the_closed_list_is_understood() {
        for route in ROUTES {
            let (path, body) = sample(route);
            let classified = classify(route.method, &path, Some(&body))
                .unwrap_or_else(|_| panic!("{route:?} : corps d'exemple illisible"));
            match route.contract {
                Contract::LegacyRemoval => assert_eq!(classified, Route::LegacyRemoval),
                Contract::Reauth => {
                    let Route::Act(act) = classified else {
                        panic!("{route:?} : pas un acte");
                    };
                    assert!(route.kinds.contains(&act.kind()), "{route:?}");
                }
            }
        }
    }

    #[test]
    fn both_gestures_of_the_attack_mode_are_rebuilt_from_the_body() {
        let on = json!({ "active": true });
        let off = json!({ "active": false });
        assert_eq!(
            classify("PUT", "/security/attack-mode", Some(&on)),
            Ok(Route::Act(AdminAct::AttackMode { enable: true }))
        );
        assert_eq!(
            classify("PUT", "/security/attack-mode", Some(&off)),
            Ok(Route::Act(AdminAct::AttackMode { enable: false }))
        );
        assert_eq!(
            classify("PUT", "/security/attack-mode", Some(&json!({}))),
            Err(Unreadable)
        );
    }

    #[test]
    fn what_is_signed_is_what_is_sent() {
        let body = json!({ "role": "readonly" });
        let Ok(Route::Act(act)) = classify("PATCH", "/accounts/01XYZ", Some(&body)) else {
            panic!("acte attendu");
        };
        assert_eq!(act.target(), "01XYZ");
        assert_eq!(act.params(), vec![b"readonly".to_vec()]);
        let update =
            json!({ "version": "0.3.0", "sha256": "AB", "url": "https://x", "signature": "s" });
        let Ok(Route::Act(act)) = classify("POST", "/agent/update", Some(&update)) else {
            panic!("acte attendu");
        };
        assert_eq!(act.params(), vec![b"0.3.0".to_vec(), b"ab".to_vec()]);
    }

    #[test]
    fn the_lifecycle_acts_sign_the_key_of_the_path() {
        let empty = json!({});
        for (verb, expected) in [
            (
                "start",
                AdminAct::ServiceStart {
                    target: "u_docker-7dtd.service".into(),
                },
            ),
            (
                "stop",
                AdminAct::ServiceStop {
                    target: "u_docker-7dtd.service".into(),
                },
            ),
            (
                "restart",
                AdminAct::ServiceRestart {
                    target: "u_docker-7dtd.service".into(),
                },
            ),
        ] {
            let path = format!("/services/u_docker-7dtd.service/{verb}");
            assert_eq!(
                classify("POST", &path, Some(&empty)),
                Ok(Route::Act(expected)),
                "{verb}"
            );
        }
        // Une clé qui n'en est pas une n'est jamais un acte : la requête part comme une route inconnue.
        assert_eq!(
            classify("POST", "/services/abc/stop", Some(&empty)),
            Ok(Route::Free)
        );
        assert_eq!(
            classify("POST", "/services/u_a%2fb.service/start", Some(&empty)),
            Ok(Route::Free)
        );
    }

    #[test]
    fn the_adoption_signs_the_keys_of_the_body_and_the_release_signs_the_key_of_the_path() {
        let body = json!({ "keys": ["u_b.service", "c_aa"], "reauth": { "password": "x" } });
        let Ok(Route::Act(act)) = classify("POST", "/services/adopt", Some(&body)) else {
            panic!("acte attendu");
        };
        assert_eq!(act, AdminAct::services_adopt(["c_aa", "u_b.service"]));
        // Une entrée qui n'est pas un texte, ou pas de clés : pas un acte.
        for bad in [json!({ "keys": [1] }), json!({}), json!({ "keys": "c_aa" })] {
            assert_eq!(
                classify("POST", "/services/adopt", Some(&bad)),
                Err(Unreadable)
            );
        }
        let empty = json!({});
        let Ok(Route::Act(act)) = classify(
            "POST",
            "/services/u_docker-7dtd.service/release",
            Some(&empty),
        ) else {
            panic!("acte attendu");
        };
        assert_eq!(
            act,
            AdminAct::ServiceRelease {
                target: "u_docker-7dtd.service".into()
            }
        );
        // Une unité au nom échappé par le système (`-` pour un tiret) voyage encodée (`%5C`) : la
        // liaison la décode comme l'agent et signe la clé en clair, pour toute clé que l'agent peut émettre.
        for (wire, key) in [
            (
                "/services/u_docker-my%5Cx2dapp.service/release",
                "u_docker-my\\x2dapp.service",
            ),
            (
                "/services/u_docker-my%5cx2dapp.service/release",
                "u_docker-my\\x2dapp.service",
            ),
            ("/services/u_a%40b.service/release", "u_a@b.service"),
            ("/services/u_a@b:c.service/release", "u_a@b:c.service"),
        ] {
            let Ok(Route::Act(act)) = classify("POST", wire, Some(&empty)) else {
                panic!("acte attendu pour {wire}");
            };
            assert_eq!(act.target(), key, "{wire}");
            assert!(is_service_key(act.target()));
            assert!(act.params().is_empty());
        }
        // Un segment qui n'a pas la forme d'une clé n'est jamais pris pour un acte.
        for path in [
            "/services/abc/release",
            "/services/c_a%2Fb/release",
            "/services/../release",
            "/services//release",
            "/services/c_/release",
            // La clé ne sort jamais de son segment : `/`, `..` et `%2f` encodés, ou une barre oblique
            // inverse nue (que les URL lisent comme `/`), ne donnent pas une clé.
            "/services/u_a%2fb.service/release",
            "/services/u_a%2Fb.service/release",
            "/services/%2e%2e/release",
            "/services/u_..%2f..%2faccounts/release",
            "/services/u_a\\b.service/release",
            "/services/u_a%5/release",
            "/services/u_a%zz/release",
            "/services/u_a%FF.service/release",
            "/services/u_a%00.service/release",
            // Deux chiffres hexadécimaux exactement : ni signe, ni espace, ni `%` doublé.
            "/services/u_a%+f.service/release",
            "/services/u_a%-1.service/release",
            "/services/u_a%%41.service/release",
        ] {
            assert_eq!(classify("POST", path, None), Ok(Route::Free), "{path}");
        }
    }

    #[test]
    fn a_read_or_an_unknown_route_is_free_and_a_malformed_act_is_unreadable() {
        for (method, path) in [
            ("GET", "/accounts"),
            ("GET", "/security"),
            ("DELETE", "/sessions/current"),
            ("POST", "/accounts/01ABC"),
            ("PUT", "/accounts/01ABC"),
            ("DELETE", "/accounts"),
            ("PUT", "/me"),
        ] {
            assert_eq!(
                classify(method, path, None),
                Ok(Route::Free),
                "{method} {path}"
            );
        }
        assert_eq!(classify("POST", "/accounts", None), Err(Unreadable));
        assert_eq!(
            classify(
                "POST",
                "/accounts",
                Some(&json!({ "username": "a", "role": "root" }))
            ),
            Err(Unreadable)
        );
        assert_eq!(
            classify(
                "PUT",
                "/me/reauth",
                Some(&json!({ "password": "sometimes" }))
            ),
            Err(Unreadable)
        );
    }

    #[test]
    fn an_identifier_that_is_not_one_never_matches_a_route() {
        for path in [
            "/accounts/",
            "/accounts/../password",
            "/accounts/a b",
            "/accounts/a%2Fb",
            "/accounts/a?x=1",
            "/accounts//sessions",
        ] {
            assert_eq!(classify("DELETE", path, None), Ok(Route::Free), "{path}");
        }
        assert_eq!(
            classify("DELETE", "/me/devices/01ABC", None),
            Ok(Route::LegacyRemoval)
        );
    }

    #[test]
    fn cancelling_and_deciding_an_operation_are_rebuilt_from_the_path_and_the_body() {
        let empty = json!({});
        let Ok(Route::Act(act)) = classify("DELETE", "/operations/services/01OP", Some(&empty))
        else {
            panic!("acte attendu");
        };
        assert_eq!(act, AdminAct::OperationCancel { target: "01OP" });
        let body = json!({ "choice": "revert", "reauth": { "password": "x" } });
        let Ok(Route::Act(act)) =
            classify("POST", "/operations/services/01OP/decision", Some(&body))
        else {
            panic!("acte attendu");
        };
        assert_eq!(
            act,
            AdminAct::OperationDecide {
                target: "01OP",
                choice: OperationChoice::Revert
            }
        );
        // Un choix inconnu, absent, ou qui n'est pas un texte : pas un acte ; il ne se signe jamais.
        for bad in [
            json!({ "choice": "dance" }),
            json!({ "choice": "unknown" }),
            json!({ "choice": 1 }),
            json!({}),
        ] {
            assert_eq!(
                classify("POST", "/operations/services/01OP/decision", Some(&bad)),
                Err(Unreadable),
                "{bad}"
            );
        }
        // L'identifiant d'une opération n'est jamais un chemin.
        for path in [
            "/operations/services/../decision",
            "/operations/services/a%2Fb",
            "/operations/services/a b",
        ] {
            assert_eq!(classify("DELETE", path, None), Ok(Route::Free), "{path}");
        }
    }

    #[test]
    fn an_act_declared_without_a_route_cannot_be_produced_by_any_request() {
        for kind in ActKind::DECLARED_WITHOUT_ROUTE {
            assert!(
                ROUTES.iter().all(|route| !route.kinds.contains(&kind)),
                "{kind:?}"
            );
            assert!(
                build(kind, Cow::Borrowed("x"), None).is_err(),
                "{kind:?} sait se reconstruire sans route"
            );
        }
    }
}
