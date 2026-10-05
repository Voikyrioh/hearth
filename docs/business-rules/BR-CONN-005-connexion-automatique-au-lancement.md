---
id: BR-CONN-005
domaine: CONN
titre: Au lancement, les serveurs dont les identifiants sont mémorisés se reconnectent seuls
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-connecter-serveur.md (BR-CONN-005)
maj: 2026-10-05
---

# BR-CONN-005 — Connexion automatique au lancement

## Règle
Au lancement de l'application, chaque serveur du carnet qui a un jeton ou un mot de passe mémorisé (BR-CONN-004) se connecte tout de suite, sans saisie ; un serveur sans secret attend sa première connexion (« Session expirée », raison `NoSession`). Un serveur dont l'utilisateur s'était déconnecté ne se reconnecte pas (BR-CONN-016). Si le mot de passe mémorisé est refusé, l'échec est silencieux et le formulaire de connexion s'ouvre (BR-CONN-017).

## Bibliothèque
- `crates/hearth-link/src/manager/mod.rs::initial_start` (choix de l'état de départ), `LinkManager::start` (une tâche par serveur du carnet) ; `domain/state.rs::Start`.

## Interface (coquille et vue)
- `apps/desktop/src-tauri/src/lib.rs::install_link` : ouvre le `LinkManager` au démarrage de l'application ; l'interface rejoue l'état courant à l'abonnement (`list_link_states`).

## Vérification
- Tests : `crates/hearth-link/tests/pinning.rs::the_application_resumes_a_saved_session_after_a_restart`, `tests/tracking.rs::after_a_panic_a_disconnected_server_stays_disconnected`.
- À vérifier au smoke test : fermer puis rouvrir l'application avec « Se souvenir de moi » cochée.

## Cas limites
- Aucun serveur enregistré : rien ne démarre.
- Réseau absent au lancement : « Reconnexion… » puis tentatives sans fin (BR-RESIL-005).

## Règles liées
- BR-CONN-004, BR-CONN-016, BR-CONN-017, BR-RESIL-013.

## Historique
- 2026-10-05 — création (HRT-10).
