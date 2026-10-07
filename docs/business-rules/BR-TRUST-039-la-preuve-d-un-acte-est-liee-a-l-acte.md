---
id: BR-TRUST-039
domaine: TRUST
titre: La preuve d'un acte est liée à l'acte, sa cible, ses paramètres non secrets, au compte, à la session, au serveur et à un défi frais ; elle ne sert qu'une fois
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-07-technique-administration-mot-de-passe-et-cle.md ; contexts/hearth/tickets/hrt/HRT-28.md
maj: 2026-10-07
---

# BR-TRUST-039 : La preuve d'un acte est liée à l'acte, sa cible, ses paramètres non secrets, au compte, à la session, au serveur et à un défi frais ; elle ne sert qu'une fois

## Règle
- Usage `0x05` du message signé : acte reconstruit par l'agent depuis la requête (code, cible = identifiant technique du compte visé, paramètres non secrets : compte et rôle d'une création, rôle d'un changement, version et somme d'une mise à jour, valeur du réglage), identifiant normalisé du compte, hachage du jeton de session, empreinte du certificat épinglé, défi de 60 s sur l'horloge monotone.
- **Aucun secret** dans le message signé (nouveau mot de passe d'un compte, mot de passe de confirmation) : une signature observée ne sert pas à tester des mots de passe hors ligne.
- Une preuve pour un autre acte, une autre cible, un autre paramètre, un autre compte, une autre session, périmée ou rejouée est refusée : `409 POST_NOT_RECOGNIZED`, `reason: proof_invalid`, base inchangée, aucun mot de passe essayé, entrée de journal.
- Le défi est consommé quand l'acte a réussi seulement ; deux requêtes simultanées avec le même défi : une seule réussit (réservation en vol).

## Application (code)
- `crates/hearth-proto/src/device_proof.rs::{signing_bytes, Binding::AdminAct}` et `admin_act.rs::AdminAct::write_to` ; `crates/hearth-agent/src/application/trust.rs::{TrustService::verify_act, reserve}`.

## Vérification
- `device_proof::tests::{the_admin_act_layout_is_the_documented_one, an_admin_proof_binds_the_act_the_target_the_params_the_token_and_the_account}` ; `tests/admin_reauth.rs::{a_proof_for_another_act_target_account_session_or_replayed_is_refused_and_changes_nothing, two_requests_with_the_same_proof_at_the_same_time_succeed_only_once}`.

## Règles liées
- BR-TRUST-005, 036, 040, ADR-0023, ADR-0031.

## Historique
- 2026-10-07 : création (HRT-28, tranche A).
