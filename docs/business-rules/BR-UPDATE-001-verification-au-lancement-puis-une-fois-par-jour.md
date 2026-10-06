---
id: BR-UPDATE-001
domaine: UPDATE
titre: Le client vérifie les mises à jour au lancement, puis une fois par jour au plus
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-001), HRT-16
maj: 2026-10-05
---

# BR-UPDATE-001 : Le client vérifie les mises à jour au lancement, puis une fois par jour au plus

## Règle
Au plus UNE requête de vérification automatique par 24 h, jamais plus. Au lancement (10 s après), puis 24 h après la dernière REQUÊTE émise, le client interroge le flux de versions (GitHub Releases du dépôt public). L'instant de la requête (`lastRequestAt`) est écrit sur disque AVANT l'appel réseau (une application tuée pendant la vérification ne la refait pas). Trois cas : (a) la lettre est tenue : jamais deux requêtes automatiques en moins de 24 h ; (b) une vérification qui n'a PAS pu émettre de requête, c'est-à-dire STRICTEMENT qui n'a eu aucun échange HTTP avec le premier hôte (échec de résolution du nom ou de connexion TCP à l'adresse du flux, et rien d'autre), ne consomme pas le quota : elle est retentée au battement horaire suivant jusqu'à ce que le réseau soit là, dans la limite de (a) et du plafond dur ci-dessous ; un échec TLS, un échec à un saut suivant, un délai dépassé ou une réponse invalide consomment le quota (dans le doute : consommé) ; (c) un échec APRÈS émission (service muet, erreur, réponse invalide) consomme le quota : prochaine tentative le lendemain (BR-UPDATE-008). Un redémarrage dans la journée ne vérifie pas. La mémoire est un fichier du dossier de données (`update.json`), jamais la WebView. **Plafond dur : au plus 3 tentatives réseau automatiques par 24 h glissantes**, quel que soit le classement des échecs (comptées au départ de chaque tentative, sans réseau comprises, enregistrées dans `update.json`). « Vérifier maintenant » (BR-UPDATE-026) compte comme une requête et repart pour 24 h ; il est limité à une fois par 30 s.

## Application (code)
- `apps/desktop/src-tauri/src/update/domain.rs::{check_is_due, manual_check_allowed, CHECK_INTERVAL_MS}` : la règle, horloge injectée.
- `apps/desktop/src-tauri/src/update/service.rs::UpdateService::{check_if_due, tick, run_scheduler}` : première vérification 10 s après le lancement, puis un battement par heure sans réseau tant que la règle ne permet rien.
- `apps/desktop/src-tauri/src/update/store.rs::FileUpdateStore` : `update.json`, écriture atomique.

## Vérification
- `apps/desktop/src-tauri/tests/update_domain.rs` : `the_first_check_is_always_allowed`, `a_second_automatic_check_waits_a_full_day`, `a_last_check_in_the_future_means_the_clock_was_corrected`, `a_manual_check_waits_thirty_seconds_after_the_previous_attempt`, `at_most_three_automatic_attempts_per_sliding_day_whatever_their_outcome`.
- `tests/update_feed.rs` : `only_a_failed_name_or_connection_to_the_first_host_gives_the_quota_back` (premier hôte injoignable, second saut, TLS, délai dépassé, réponse invalide).
- `apps/desktop/src-tauri/tests/update_service.rs` : `an_attempt_that_could_not_send_a_request_does_not_use_the_daily_quota`, `an_offline_attempt_after_a_success_keeps_the_original_window`, `a_silent_service_or_a_bad_answer_after_the_request_used_the_quota`, `a_manual_check_cannot_be_looped_on`, `the_hard_cap_stops_automatic_attempts_even_when_none_sent_a_request`, `failures_that_reached_the_first_host_use_the_quota_and_the_cap_counts_them`, `the_first_launch_checks_then_the_next_automatic_check_waits_a_day`, `a_restart_within_the_day_does_not_check_again`, `a_failed_attempt_counts_it_is_not_retried_before_the_next_day`, `the_attempt_is_written_before_the_network_call`, `check_now_ignores_the_daily_limit_and_restarts_the_window`.
- `apps/desktop/src-tauri/tests/update_feed.rs` : `end_to_end_check_then_click_installs_only_a_verified_file` (une vérification = une requête, rien d'autre).

## Cas limites
- « Aucune requête » veut dire « aucune requête HTTP » : la résolution du nom et l'ouverture de connexion sortent, au plus 3 fois par 24 h.
- Pas de boucle de relance ni de sondage continu : le battement est horaire et ne fait aucune requête tant que la règle ne le permet pas. Un PC allumé chaque jour un peu plus tard, dont le réseau n'est pas prêt à la première tentative, voit sa vérification retentée à l'heure suivante, puis une fois le réseau là. Arbitrage de l'agent principal (ADR-0017 § 6), modifiable par Voiky.
- Horloge remise en arrière : une dernière tentative « dans le futur » est tenue pour fausse, la vérification est permise.
- Le battement horaire ne fait aucun appel réseau tant que 24 h ne sont pas écoulées.

## Règles liées
- BR-UPDATE-006, BR-UPDATE-007, BR-UPDATE-008, BR-UPDATE-026, ADR-0017

## Historique
- 2026-10-05 : création (HRT-16, session 2026-10-04-hearth-creation).
