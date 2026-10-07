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

use hearth_proto::admin_act::{ActKind, AdminAct, Contract, ROUTES, RouteAct};
use hearth_proto::api::accounts::RoleName;
use hearth_proto::api::reauth::ReauthMode;
use serde_json::Value;

/// Un identifiant de compte de l'agent : lettres et chiffres, rien d'autre (jamais `/`, `..`, `?`).
const ID_MAX_LEN: usize = 64;

/// Ce que dit une requête sur l'administration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// La ligne de la liste fermée que vise cette requête, avec l'identifiant du chemin quand le motif en a
/// un. Un chemin dont l'identifiant n'a pas la forme d'un identifiant n'est PAS reconnu : il part, s'il
/// part, comme une route inconnue de l'agent (qui répondra `404`), sans pouvoir se faire passer pour
/// un autre acte.
pub fn route_of<'a>(method: &str, path: &'a str) -> Option<(&'static RouteAct, &'a str)> {
    let wanted: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    ROUTES.iter().find_map(|route| {
        if route.method != method {
            return None;
        }
        let pattern: Vec<&str> = route.pattern.trim_start_matches('/').split('/').collect();
        if pattern.len() != wanted.len() {
            return None;
        }
        let mut id = "";
        for (expected, got) in pattern.iter().zip(&wanted) {
            if *expected == "{id}" {
                if !is_id(got) {
                    return None;
                }
                id = got;
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

/// L'acte d'un genre donné, lu dans le chemin (`id`) et le corps. Correspondance EXHAUSTIVE : un genre
/// d'acte de plus ne compile pas tant qu'on ne dit pas comment le reconstruire.
fn build<'a>(
    kind: ActKind,
    id: &'a str,
    body: Option<&'a Value>,
) -> Result<AdminAct<'a>, Unreadable> {
    let active = body
        .and_then(|body| body.get("active"))
        .and_then(Value::as_bool);
    Ok(match kind {
        ActKind::AccountCreate => AdminAct::AccountCreate {
            username: text(body, "username")?,
            role: role(body)?,
        },
        ActKind::AccountRole => AdminAct::AccountRole {
            target: id,
            role: role(body)?,
        },
        ActKind::AccountPassword => AdminAct::AccountPassword { target: id },
        ActKind::AccountDelete => AdminAct::AccountDelete { target: id },
        ActKind::SessionsRevoke => AdminAct::SessionsRevoke { target: id },
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
        .find_map(|kind| build(*kind, id, body).ok())
        .map(Route::Act)
        .ok_or(Unreadable)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Un chemin et un corps d'exemple pour chaque route de la liste.
    fn sample(route: &RouteAct) -> (String, Value) {
        let path = route.pattern.replace("{id}", "01ABC");
        let body = match route.pattern {
            "/accounts" => json!({ "username": "paul", "role": "readonly" }),
            "/accounts/{id}" if route.method == "PATCH" => json!({ "role": "admin" }),
            "/agent/update" => json!({ "version": "0.2.0", "sha256": "ab" }),
            "/security/attack-mode" => json!({ "active": true }),
            "/me/reauth" => json!({ "password": "each" }),
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
}
