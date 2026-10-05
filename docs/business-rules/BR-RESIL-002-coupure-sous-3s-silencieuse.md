---
id: BR-RESIL-002
domaine: RESIL
titre: Une coupure de moins de 3 secondes ne change rien à l'écran
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-002), ADR-0007
maj: 2026-10-05
---

# BR-RESIL-002 — Une coupure de moins de 3 secondes ne change rien à l'écran

## Règle
Pendant les 3 premières secondes d'une coupure, l'état du lien reste « Connecté » : le client réessaie seul, sans notification. Le seuil est exact à la milliseconde : 2 999 ms de coupure = « Connecté », 3 000 ms = « Reconnexion en cours ». La coupure est mesurée depuis l'erreur de transport, ou depuis le dernier message reçu quand c'est un silence qui la révèle.

## Application (code)
- `crates/hearth-link/src/domain/state.rs::LinkMachine::handle` (entrées `Input::TransportFailed`, `Input::Tick`) et `LinkMachine::derive_down` : état affiché dérivé de la durée de coupure ; `LinkMachine::deadline` donne l'échéance du `Tick` qui franchit le seuil.
- `crates/hearth-link/src/domain/state.rs::RECONNECTING_AFTER`, `Thresholds` (seuils injectables pour les tests de résilience).

## Vérification
- Tests : `domain::state::tests::row01_connected_cut_under_3s_changes_nothing`, `::row02_connected_cut_between_3s_and_30s_shows_reconnecting` (2 999 / 3 000 ms), `::a_cut_found_by_an_error_starts_at_the_error_not_at_the_last_message`.
- Intégration : `crates/hearth-link/tests/fault_proxy.rs::a_cut_shorter_than_the_threshold_is_invisible` (seuils du lien hors d'atteinte : la coupure dure ce qu'elle dure, aucune assertion de vitesse).
- Coquille, contre un vrai agent : `apps/desktop/src-tauri/tests/offline.rs``::a_cut_shorter_than_the_threshold_never_reaches_the_screen_nor_the_notifications` (aucun événement d'état à l'écran, aucune notification, icône inchangée).
- Interface : `apps/desktop/e2e/offline.spec.ts` (« une coupure courte ne change rien à l'écran »).

## Cas limites
- Au démarrage, ou juste après une connexion, rien n'a encore répondu : l'état affiché est « Reconnexion en cours » tout de suite (jamais un faux « Connecté »).
- Un silence de 3 s est déjà une coupure de 3 s (le début de la coupure est le dernier message) : l'état passe à « Reconnexion en cours » à cet instant.

## Règles liées
- BR-RESIL-003, BR-RESIL-005.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
- 2026-10-05 : tests de résilience rendus déterministes, test de la coquille et de l'écran (HRT-12).
