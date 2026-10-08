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
Pendant les 3 premières secondes d'une coupure, l'état du lien reste « Connecté » : le client réessaie seul, sans notification. Le seuil est exact à la milliseconde pour la DATE de l'état : une coupure continue de 2 999 ms reste « Connecté », une coupure de 3 000 ms est « Reconnexion en cours », datée de 3 000 ms. L'AFFICHAGE, lui, arrive entre 3,0 et 3,5 s : tout de suite si la vérification du seuil est refusée, à la borne de la vérification (0,5 s) si elle reste sans réponse. La coupure est mesurée depuis l'erreur de transport, ou depuis le dernier message reçu quand c'est un silence qui la révèle.

**« Reconnexion en cours » n'est montrée qu'après 3 s de coupure CONTINUE, quelles que soient les tentatives en dessous.** Les tentatives gardent leurs délais (0,5 / 1 / 2 / 4 / 8 / 15 / 30 s), donc une coupure rétablie à 2,9 s ne serait retrouvée qu'à la tentative de 3,5 s : pour que l'écran ne clignote pas, une **vérification** part une fois, à 3 s exactement, hors de la suite des délais. Réussie : rien n'a été montré. Échouée : « Reconnexion en cours » est affichée, datée de 3 s, et la tentative planifiée garde sa place. **La vérification est bornée** : le sixième du seuil (0,5 s). Sans réponse à sa borne (serveur qu'on redémarre : refus, puis silence), la coupure est établie : « Reconnexion en cours » est émise à cet instant, datée de 3 s, sans attendre le délai de la tentative (8 s) ; la tentative continue comme une tentative ordinaire. Toute sortie de la vérification (borne, réponse « attends », tentative relancée par un réveil, un changement de réseau ou « Réessayer ») ne laisse rien derrière elle : l'échec suivant calcule son délai. « Hors ligne » reste à 30 s. Une tentative restée sans réponse au seuil, un silence de plus de 3 s, une réponse « attends » du serveur valent déjà preuve : « Reconnexion en cours » sans vérification.

## Application (code)
- `crates/hearth-link/src/domain/state.rs::LinkMachine::handle` (entrées `Input::TransportFailed`, `Input::Tick`) et `LinkMachine::derive_down` : état affiché dérivé de la durée de coupure ; `LinkMachine::deadline` donne l'échéance du `Tick` qui franchit le seuil ; `LinkMachine::check_threshold` fait la vérification du seuil (champs `confirmed`, `probe`, `resume_at` de l'`Outage`, FIX-01M4CJEQS88NZWCQ129XDP91MT).
- `crates/hearth-link/src/domain/state.rs::RECONNECTING_AFTER`, `Thresholds` (seuils injectables pour les tests de résilience).

## Vérification
- Tests : `domain::state::tests::row01_connected_cut_under_3s_changes_nothing`, `::row02_connected_cut_between_3s_and_30s_shows_reconnecting` (2 999 / 3 000 ms), `::a_cut_found_by_an_error_starts_at_the_error_not_at_the_last_message`, `::a_cut_healed_just_before_3s_shows_nothing_even_if_the_next_attempt_comes_later`, `::a_cut_still_there_at_3s_shows_reconnecting_then_connected_and_keeps_its_delays`, `::a_silent_probe_cannot_hold_the_screen_on_connected_past_its_bound`, `::a_probe_ended_by_a_wait_answer_does_not_make_the_next_failure_skip_its_delay`, `::a_trigger_during_the_probe_makes_it_an_ordinary_attempt`. Gestionnaire aux seuils du produit en temps virtuel : `crates/hearth-link/tests/link_timing.rs::a_cut_healed_at_2_9_seconds_shows_nothing`, `::a_cut_healed_at_3_1_seconds_shows_reconnecting_then_connected`, `::a_host_that_refuses_then_goes_silent_shows_reconnecting_at_3_seconds`.
- Intégration : `crates/hearth-link/tests/fault_proxy.rs::a_cut_healed_before_any_threshold_shows_nothing_and_the_stream_resumes` (aucun état émis, le flux repart ; le seuil de 3 s lui-même est prouvé en temps contrôlé par `domain::state::tests::row01…`/`row02…`, 2 999 / 3 000 ms), `::a_delayed_agent_keeps_its_stream_open_and_nothing_is_shown`.
- Coquille, contre un vrai agent : `apps/desktop/src-tauri/tests/offline.rs``::a_cut_shorter_than_the_threshold_never_reaches_the_screen_nor_the_notifications` (aucun seuil du lien ne peut être franchi : ce test prouve que la reprise n'affiche rien) (aucun événement d'état à l'écran, aucune notification, icône inchangée).
- Interface : `apps/desktop/e2e/offline.spec.ts` (« une coupure courte ne change rien à l'écran »).

## Cas limites
- Au démarrage, ou juste après une connexion, rien n'a encore répondu : l'état affiché est « Reconnexion en cours » tout de suite (jamais un faux « Connecté »).
- Un silence de 3 s est déjà une coupure de 3 s (le début de la coupure est le dernier message) : l'état passe à « Reconnexion en cours » à cet instant.

## Règles liées
- BR-RESIL-003, BR-RESIL-005.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
- 2026-10-08 : « Reconnexion » après 3 s de coupure continue seulement, vérification au seuil (HRT-18, FIX-01M4CJEQS88NZWCQ129XDP91MT).
- 2026-10-05 : tests de résilience rendus déterministes, test de la coquille et de l'écran (HRT-12).
