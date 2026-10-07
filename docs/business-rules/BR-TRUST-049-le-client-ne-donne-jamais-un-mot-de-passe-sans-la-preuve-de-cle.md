---
id: BR-TRUST-049
domaine: TRUST
titre: Le client ne donne jamais un mot de passe de confirmation sans la preuve de la clé de ce poste ; sans clé au coffre, aucun appel d'administration ne part
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-07-technique-administration-mot-de-passe-et-cle.md ; contexts/hearth/tickets/hrt/HRT-30.md
maj: 2026-10-07
---

# BR-TRUST-049 : Le client ne donne jamais un mot de passe de confirmation sans la preuve de la clé de ce poste ; sans clé au coffre, aucun appel d'administration ne part

## Règle
- Un acte confirmé porte `reauth.password` (absent sous élévation, pour un acte couvert) ET `reauth.device` (la preuve d'usage `0x05`, signée par la clé de ce poste) : **jamais l'un sans l'autre**. C'est ce qui rend l'empreinte des requêtes (SHA-256 du corps retenu 24 h) inexploitable en attendant HRT-32.
- Sans clé au coffre (ou coffre en panne) : `LinkError::NoDeviceKey`, **ni défi ni requête d'écriture ne part**. L'interface le lit avant d'envoyer (`has_device_key`) et dit quoi faire : se reconnecter par mot de passe sur ce poste pour l'inscrire (bouton « Me reconnecter pour enregistrer ce poste »). L'administrateur n'est jamais enfermé dehors : voies de secours de BR-TRUST-044.
- Défi indisponible (coupure du seul défi, réponse illisible) : `DeviceChallengeUnavailable`, rien d'autre n'est parti, pas de repli sans preuve.
- Le mot de passe mémorisé au coffre n'est lu par aucun chemin de confirmation. Le mot de passe saisi vit le temps de l'appel : `Secret` effacé à la libération, copie du corps de requête effacée à la libération de `ActionRequest` et d'`ApiRequest` (suivi de la PR #32), copies des types du fil effacées après sérialisation dans la coquille.
- Un acte que l'élévation ne couvre jamais, sans mot de passe : refusé localement (`InvalidInput(Credentials)`), rien n'est envoyé.

## Application (code)
- `hearth-link` : `manager/reauth.rs::LinkManager::{send_confirmed, execute_act}` ; `domain/secret.rs::wipe_body` ; `manager/mod.rs::ActionRequest` (`Drop`). Coquille : `apps/desktop/src-tauri/src/accounts/wire.rs` (copies effacées).

## Vérification
- `crates/hearth-link/tests/admin_reauth.rs::{without_a_key_no_call_leaves_not_even_a_challenge, an_unavailable_challenge_is_said_so_and_nothing_else_leaves, an_act_without_a_password_and_not_covered_by_the_elevation_is_not_sent}` (requêtes d'écriture comptées par la sonde) ; `apps/desktop/src-tauri/tests/security_runtime.rs::without_a_key_no_act_leaves_and_the_state_says_so` ; unitaire `domain::secret::tests::a_request_body_is_wiped_in_every_text_however_deep` ; Playwright `e2e/reauth.spec.ts` (poste sans clé).

## Règles liées
- BR-TRUST-036, 040, 044, ADR-0033.

## Historique
- 2026-10-07 : création (HRT-30, tranche B).
