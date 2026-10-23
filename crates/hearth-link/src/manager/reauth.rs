//! La confirmation d'un acte d'administration côté client (HRT-30, ADR-0031, ADR-0033, BR-TRUST-036,
//! 039, 040, 048) : le membre `reauth` (mot de passe + preuve de la clé de CE poste, usage `0x05`) ajouté
//! au corps de l'acte, et la lecture de ce que l'agent annonce (capacité, réglage, élévation).
//!
//! Règles de ce module :
//! - **aucun acte ne part sans sa confirmation** : `LinkManager::execute` refuse une requête de la liste
//!   fermée (`LinkError::ActionUnconfirmed`) ; `execute_act` est la seule porte, et l'acte qu'elle signe
//!   est reconstruit depuis la requête qui part (`domain::act`) ;
//! - **jamais de mot de passe sans la preuve de clé** : sans clé au coffre, ni défi ni acte ne part
//!   (`NoDeviceKey`) ; un mot de passe n'est mis dans la requête qu'avec la preuve signée à côté ;
//! - **un défi neuf à chaque essai** : chaque appel demande son défi et prend sa clé d'opération. Un
//!   refus de confirmation est retenu côté agent sous la clé d'opération ; un essai qui rejouerait l'ancienne
//!   clé ou l'ancien défi rendrait le même refus (BR-TRUST-046) ;
//! - **le mot de passe ne vit que le temps de l'appel** : `Secret` effacé à la libération, copie du corps
//!   effacée à la libération de la requête (`ActionRequest`, `ApiRequest`) ;
//! - **un agent qui n'annonce pas `admin_reauth`** n'est pas de cette famille (aucune version d'avant n'a
//!   été publiée) : `Incompatible(UpdateAgent)` (« agent trop ancien » à l'écran), rien n'est envoyé.

use hearth_proto::admin_act::{AdminAct, PasswordNeed, TrustLevel, password_need_announced};
use hearth_proto::api::reauth::AdminReauthInfo;
use hearth_proto::api::security::SecurityResponse;
use hearth_proto::api::sessions::ChallengePurpose;
use hearth_proto::device_proof::Binding;
use hearth_proto::error::ErrorCode;
use serde_json::{Map, Value, json};

use super::device::{ChallengeAnswer, KeyState, ask_challenge, load_key, pinned, token_hash};
use super::{ActionOutcome, ActionRequest, LinkManager};
use crate::domain::act::{self, Route};
use crate::domain::compat::Compatibility;
use crate::domain::secret::Secret;
use crate::domain::server::ServerId;
use crate::error::{InputField, LinkError};
use crate::ports::transport::Target;

const SECURITY_PATH: &str = "/security";

/// Cet acte exige-t-il le mot de passe à chaque fois, d'après l'état de confiance que l'agent annonce
/// (HRT-194, BR-GRANT-017) ? Un agent qui n'annonce pas l'état (d'avant HRT-194) garde la règle d'avant :
/// la table de l'élévation seule (BR-COMPAT-001). Une valeur inconnue vaut le mode attaque (`Yes`).
fn password_always_needed(act: &AdminAct<'_>, trust: Option<TrustLevel>) -> bool {
    password_need_announced(act, trust) == PasswordNeed::Yes
}

/// Ce que l'interface doit savoir pour construire sa fenêtre : ce que l'agent annonce, et si ce PC a une
/// clé au coffre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReauthState {
    /// `None` : l'agent n'annonce pas la confirmation des actes (aucun acte ne lui part).
    pub agent: Option<AdminReauthInfo>,
    /// Ce PC a une clé d'appareil au coffre pour ce serveur (un booléen : la clé ne sort pas).
    pub has_device_key: bool,
}

impl LinkManager {
    /// Ce que l'agent annonce de la confirmation des actes (`GET /security`, champ `admin_reauth`) :
    /// capacité, réglage du compte, secondes d'élévation restantes. Une LECTURE faite à l'ouverture d'une
    /// fenêtre : l'interface ne devine pas l'élévation, elle la lit de l'agent. Un agent d'avant
    /// `/security`, ou sans le champ : `None`.
    pub async fn admin_reauth(&self, id: &ServerId) -> Result<ReauthState, LinkError> {
        let (target, token) = self.credentials(id)?;
        let agent = self.read_admin_reauth(&target, &token).await?;
        Ok(ReauthState {
            agent,
            has_device_key: self.has_device_key(id),
        })
    }

    pub(super) async fn read_admin_reauth(
        &self,
        target: &Target,
        token: &Secret,
    ) -> Result<Option<AdminReauthInfo>, LinkError> {
        match self
            .typed_get(target, token, SECURITY_PATH.to_owned())
            .await
        {
            Ok(body) => serde_json::from_value::<SecurityResponse>(body)
                .map(|response| response.admin_reauth)
                .map_err(|e| LinkError::Protocol(format!("état de sécurité illisible : {e}"))),
            // Un agent d'avant `/security` n'a pas non plus la confirmation des actes.
            Err(LinkError::Rejected(Some(ErrorCode::NotFound))) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Envoie un ACTE D'ADMINISTRATION confirmé. L'acte est reconstruit depuis `action` (méthode, chemin,
    /// corps) ; face à un agent qui annonce `admin_reauth`, le corps reçoit `reauth` : le mot de passe
    /// actuel de l'appelant (absent sous élévation, pour un acte couvert) ET la preuve de la clé de ce
    /// poste liée à l'acte. Puis c'est une action ordinaire (clé d'opération neuve, jamais rejouée, issue
    /// `ResultUnknown` si le lien tombe).
    ///
    /// - hors « Connecté » : `NotConnected` sans rien envoyer ;
    /// - agent sans `admin_reauth` : `Incompatible(UpdateAgent)`, rien n'est envoyé ;
    /// - sans clé au coffre : `NoDeviceKey`, ni défi ni acte ne part ;
    /// - un acte non couvert par l'élévation sans mot de passe : `InvalidInput(Credentials)` ;
    /// - défi indisponible : `DeviceChallengeUnavailable`, rien d'autre n'est parti.
    ///
    /// Le mot de passe n'est jamais gardé.
    pub async fn execute_act(
        &self,
        id: &ServerId,
        action: ActionRequest,
        password: Option<&Secret>,
    ) -> Result<ActionOutcome, LinkError> {
        let (target, token) = self.credentials(id)?;
        match act::classify(action.method.as_str(), &action.path, action.body.as_ref()) {
            Ok(Route::Act(_)) => {}
            // Le retrait d'un poste garde son contrat livré et sa fonction (`remove_trusted_device`) ;
            // une requête libre n'a rien à confirmer : ce n'est pas une porte pour elles.
            Ok(Route::Free | Route::LegacyRemoval) | Err(_) => {
                return Err(LinkError::UnreadableAct);
            }
        }
        let Some(info) = self.read_admin_reauth(&target, &token).await? else {
            return Err(LinkError::Incompatible(Compatibility::UpdateAgent));
        };
        // L'état de confiance annoncé sert à la ceinture seulement : l'agent le recalcule à l'acte.
        self.send_confirmed(id, &target, &token, action, password, info.trust)
            .await
    }

    /// Le cœur de la confirmation, une fois la capacité de l'agent lue : clé, défi neuf, preuve liée à
    /// l'acte, `reauth` dans le corps, envoi.
    pub(super) async fn send_confirmed(
        &self,
        id: &ServerId,
        target: &Target,
        token: &Secret,
        mut action: ActionRequest,
        password: Option<&Secret>,
        trust: Option<TrustLevel>,
    ) -> Result<ActionOutcome, LinkError> {
        let deps = &self.inner.deps;
        // Sans clé : AUCUN appel, pas même un défi (BR-TRUST-044 : la voie de secours est de se
        // reconnecter par mot de passe sur ce poste).
        let KeyState::Present(key) = load_key(deps, id) else {
            return Err(LinkError::NoDeviceKey);
        };
        let password = password.filter(|password| !password.is_empty());
        let proof = {
            let route = act::classify(action.method.as_str(), &action.path, action.body.as_ref())
                .map_err(|_| LinkError::UnreadableAct)?;
            let Route::Act(act) = route else {
                return Err(LinkError::UnreadableAct);
            };
            // Un acte qui exige le mot de passe à chaque fois ne part pas sans : l'interface le demande
            // toujours, ceci n'est que la ceinture (HRT-194 : la grille de l'état de confiance annoncé).
            if password.is_none() && password_always_needed(&act, trust) {
                return Err(LinkError::InvalidInput(InputField::Credentials));
            }
            let (_, shared) = self.handle(id)?;
            let username = shared.record().username;
            let fingerprint = pinned(target)
                .ok_or_else(|| LinkError::Protocol("empreinte du serveur non confirmée".into()))?;
            let hash = token_hash(token.expose())
                .ok_or_else(|| LinkError::Protocol("jeton illisible".into()))?;
            // Un défi NEUF à chaque essai. Un défi indisponible (coupure du seul défi, délai, réponse
            // illisible) n'est pas « serveur injoignable » : rien n'est parti, ni mot de passe ni preuve.
            let challenge =
                match ask_challenge(deps, target, &username, ChallengePurpose::AdminAct).await {
                    ChallengeAnswer::Issued(challenge) => challenge,
                    ChallengeAnswer::Unsupported | ChallengeAnswer::Unavailable => {
                        return Err(LinkError::DeviceChallengeUnavailable);
                    }
                    ChallengeAnswer::Mismatch(error) => return Err(error.into()),
                };
            key.prove(
                Binding::AdminAct {
                    token_hash: &hash,
                    act: &act,
                },
                &fingerprint,
                &username,
                &challenge,
            )
            .ok_or(LinkError::DeviceChallengeUnavailable)?
        };
        let mut object = match action.body.take() {
            Some(Value::Object(map)) => map,
            None => Map::new(),
            Some(_) => return Err(LinkError::UnreadableAct),
        };
        let mut reauth = Map::new();
        if let Some(password) = password {
            reauth.insert("password".into(), json!(password.expose()));
        }
        reauth.insert("device".into(), json!(proof));
        object.insert("reauth".into(), Value::Object(reauth));
        action.body = Some(Value::Object(object));
        self.execute_unchecked(id, action).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start() -> AdminAct<'static> {
        AdminAct::ServiceStart {
            target: "c_ab".into(),
        }
    }

    #[test]
    fn the_belt_follows_the_announced_trust_and_keeps_the_old_rule_without_it() {
        // Annoncé : la grille du pilotage.
        assert!(!password_always_needed(&start(), Some(TrustLevel::Normal)));
        assert!(!password_always_needed(&start(), Some(TrustLevel::Alert)));
        assert!(password_always_needed(&start(), Some(TrustLevel::Attack)));
        // Une valeur inconnue vaut le mode attaque.
        assert!(password_always_needed(&start(), Some(TrustLevel::Unknown)));
        // Agent d'avant HRT-194 : la table de l'élévation seule (démarrer y est couvert).
        assert!(!password_always_needed(&start(), None));
        let decide = AdminAct::OperationDecide {
            target: "01OP",
            choice: hearth_proto::api::service_ops::OperationChoice::Keep,
        };
        assert!(
            password_always_needed(&decide, None),
            "comportement d'avant"
        );
        assert!(!password_always_needed(&decide, Some(TrustLevel::Normal)));
        // Hors pilotage, l'état ne change rien.
        let delete = AdminAct::AccountDelete { target: "01ABC" };
        assert!(!password_always_needed(&delete, Some(TrustLevel::Attack)));
        let own = AdminAct::AccountPasswordOwn;
        assert!(password_always_needed(&own, Some(TrustLevel::Normal)));
    }
}
