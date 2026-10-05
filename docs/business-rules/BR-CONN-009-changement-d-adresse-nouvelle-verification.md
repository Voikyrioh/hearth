---
id: BR-CONN-009
domaine: CONN
titre: Modifier l'adresse d'un serveur enregistré impose une nouvelle vérification de l'empreinte
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-connecter-serveur.md (BR-CONN-009)
maj: 2026-10-05
---

# BR-CONN-009 — Modifier l'adresse d'un serveur enregistré impose une nouvelle vérification de l'empreinte

## Règle
Quand l'adresse (hôte ou port) d'un serveur enregistré change, l'empreinte du serveur trouvé à la nouvelle adresse doit être relue (`probe`) et confirmée de nouveau par l'utilisateur avant que la modification soit enregistrée ; sans empreinte confirmée, la bibliothèque refuse (`VerificationRequired`). Les identifiants mémorisés sont conservés ; la connexion repart vers la nouvelle adresse, épinglée sur la nouvelle empreinte. Renommer ou recolorer sans changer l'adresse ne demande rien.

## Bibliothèque
- `crates/hearth-link/src/domain/book.rs::address_changed` ; `crates/hearth-link/src/manager/mod.rs::LinkManager::update_server` (`ServerUpdate`).

## Interface (coquille et vue)
- `apps/desktop/src/components/organisms/ServerEditForm.vue` : une autre adresse passe par la sonde puis l'écran « Vérifie l'identité du serveur » (« Refuser » revient au formulaire, « Confirmer » enregistre) ; coquille `LinkRuntime::update_server`.

## Vérification
- Tests : `crates/hearth-link/tests/pinning.rs::changing_the_address_demands_a_new_fingerprint_and_keeps_the_remembered_password` ; `apps/desktop/src-tauri/tests/link_runtime.rs::moving_a_server_asks_for_a_new_fingerprint_and_keeps_the_stored_login` ; `src/pages/Servers.test.ts::asks to verify the fingerprint again when the address changes…`.

## Cas limites
- Une adresse déjà prise par un autre serveur du carnet est refusée (`AlreadyExists`).
- Une adresse injoignable : message sous le formulaire, rien n'est modifié.

## Règles liées
- BR-CONN-001, BR-CONN-002, BR-CONN-008, BR-CONN-011.

## Historique
- 2026-10-05 : création, à venir (HRT-07, review Stephen round 1).
- 2026-10-05 : livrée (HRT-10).
