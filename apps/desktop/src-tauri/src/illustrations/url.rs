//! Validation PURE des adresses du protocole `hearth-illustration` (HRT-239, ADR-0061) : aucune E/S. Une adresse n'est
//! jamais transformée en chemin ni en adresse réseau avant d'avoir été jugée composant par composant ; tout ce qui n'a
//! pas exactement l'une des deux formes est refusé (`400`, rien ne part).
//!
//! - `/steam/{app_id}/header` : `app_id` = chiffres seulement, 1 à 10 caractères, `u32` non nul ;
//! - `/agent/{server_id}/{fiche_id}/{hex}` : serveur du carnet, identifiant stable de fiche, `hex` = 64 caractères
//!   `[0-9a-f]` (l'empreinte SHA-256 de l'illustration, donnée par la liste).

use hearth_link::is_stable_id;

/// Ce que l'adresse demande, une fois jugée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Steam {
        app_id: u32,
    },
    Agent {
        server_id: String,
        fiche_id: String,
        hex: String,
    },
}

/// Pourquoi une adresse est refusée (jamais répétée à l'appelant).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlError {
    Invalid,
}

/// Même règle que `hearth_link::domain::server::ServerId` : lettres, chiffres, tiret et souligné, 64 au plus.
const MAX_SERVER_ID: usize = 64;

fn server_id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_SERVER_ID
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

fn hex_ok(hex: &str) -> bool {
    hex.len() == 64
        && hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Juge le chemin d'une requête du protocole (sans requête ni fragment : le chemin seul).
pub fn parse(path: &str) -> Result<Target, UrlError> {
    let rest = path.strip_prefix('/').ok_or(UrlError::Invalid)?;
    let parts: Vec<&str> = rest.split('/').collect();
    match parts.as_slice() {
        ["steam", app_id, "header"] => {
            if app_id.is_empty() || app_id.len() > 10 || !app_id.bytes().all(|b| b.is_ascii_digit())
            {
                return Err(UrlError::Invalid);
            }
            let app_id: u32 = app_id.parse().map_err(|_| UrlError::Invalid)?;
            if app_id == 0 {
                return Err(UrlError::Invalid);
            }
            Ok(Target::Steam { app_id })
        }
        ["agent", server_id, fiche_id, hex] => {
            if server_id_ok(server_id) && is_stable_id(fiche_id) && hex_ok(hex) {
                Ok(Target::Agent {
                    server_id: (*server_id).to_owned(),
                    fiche_id: (*fiche_id).to_owned(),
                    hex: (*hex).to_owned(),
                })
            } else {
                Err(UrlError::Invalid)
            }
        }
        _ => Err(UrlError::Invalid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn a_steam_address_is_digits_only_and_a_non_zero_u32() {
        assert_eq!(
            parse("/steam/105600/header"),
            Ok(Target::Steam { app_id: 105_600 })
        );
        assert_eq!(
            parse("/steam/4294967295/header"),
            Ok(Target::Steam { app_id: u32::MAX })
        );
        for bad in [
            "/steam/0/header",
            "/steam/abc/header",
            "/steam/12345678901/header",
            "/steam/4294967296/header",
            "/steam/-1/header",
            "/steam/1.5/header",
            "/steam/+1/header",
            "/steam/1e3/header",
            "/steam//header",
            "/steam/1/library",
            "/steam/1/header/",
            "/steam/1",
            "/steam/１２３/header",
        ] {
            assert_eq!(parse(bad), Err(UrlError::Invalid), "{bad}");
        }
    }

    #[test]
    fn an_agent_address_needs_a_server_a_stable_fiche_and_a_lowercase_sha256() {
        assert_eq!(
            parse(&format!("/agent/forge/minecraft/{HEX}")),
            Ok(Target::Agent {
                server_id: "forge".into(),
                fiche_id: "minecraft".into(),
                hex: HEX.into()
            })
        );
        for bad in [
            format!("/agent/x/../y/{HEX}"),
            format!("/agent/../minecraft/{HEX}"),
            format!("/agent/./minecraft/{HEX}"),
            format!("/agent/a%2Fb/minecraft/{HEX}"),
            format!("/agent/forge/Minecraft/{HEX}"),
            format!("/agent/forge/mine craft/{HEX}"),
            format!("/agent/forge/minecraft/{}", HEX.to_uppercase()),
            format!("/agent/forge/minecraft/{}", &HEX[..63]),
            format!("/agent/forge/minecraft/{HEX}0"),
            format!("/agent/forge/minecraft/{}", "g".repeat(64)),
            "/agent/forge/minecraft".to_owned(),
            format!("/agent/{}/minecraft/{HEX}", "s".repeat(65)),
            format!("/agent/a.b/minecraft/{HEX}"),
            format!("/agent//minecraft/{HEX}"),
        ] {
            assert_eq!(parse(&bad), Err(UrlError::Invalid), "{bad}");
        }
    }

    #[test]
    fn anything_else_is_invalid() {
        for bad in [
            "",
            "/",
            "steam/1/header",
            "/library/1/header",
            "/steam/1/header?x=1",
            "//steam/1/header",
        ] {
            assert_eq!(parse(bad), Err(UrlError::Invalid), "{bad}");
        }
    }
}
