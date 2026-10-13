---
id: BR-SVC-009
domaine: SVC
titre: Seul un compte qui a le droit de gérer les services (quel que soit son rôle) révèle les valeurs des variables ; chaque révélation est consignée, sans nom ni valeur
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-009), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, HRT-63
maj: 2026-10-13
---

# BR-SVC-009 — Révélation des valeurs réservée au droit de gérer les services

## Règle
`GET /api/v1/services/{key}/env` rend les variables du service avec leurs valeurs.

- **Droit, pas rôle (amendé par HRT-76, ADR-0041, BR-GRANT-003 et BR-GRANT-004)** : la route est `Access::Services`. La couche d'accès refuse tout compte qui n'a pas le droit « gérer les services », administrateur compris (`403 FORBIDDEN_ROLE`, `details.missing: "services"`, « Tu n'as pas le droit de gérer les services de ce serveur. ») AVANT tout handler et avant la lecture de la clé, même si le client est modifié, et CONSIGNE le refus (`service.env.reveal`, résultat « refusé », cause « droits insuffisants », cible : la route `/services/{key}/env`). Un compte en lecture seule qui a le droit révèle ; un administrateur sans le droit ne révèle pas. Le droit est relu en base à chaque requête. Une session absente ou inconnue est refusée sans journal, comme partout. Avant tout octroi par HRT-77, personne n'a le droit : plus personne ne révèle les valeurs.
- **À la demande seulement** : aucune valeur n'est lue en arrière-plan ni envoyée d'avance ; une relecture de la liste, un événement du moteur, un abonnement au flux ne lisent jamais `Config.Env` au-delà des noms (BR-SVC-008). Le moteur n'est interrogé (`ContainerEngine::env`) que par cette route.
- **Journal, échec fermé** : une révélation réussie écrit UNE entrée `service.env.reveal` PAR révélation (« Valeurs des variables révélées », cible : « service {nom} », résultat « réussi »), et cette écriture doit RÉUSSIR avant que les valeurs partent (`AuditSink::record_required`, sans regroupement des répétitions, contrairement aux refus qui, eux, passent par le filtre de répétitions du journal : même compte, même adresse, même résultat dans la minute = une entrée plus une synthèse). Si l'écriture échoue (base verrouillée, disque plein), AUCUNE valeur n'est rendue : `503 BUSY`, texte fixe « La révélation n'a pas pu être consignée au journal : aucune valeur n'est rendue. Réessaie dans un instant. », sans détail de la base. L'entrée ne porte ni nom ni valeur de variable (`Target::Service` n'a pas de champ pour eux). Ce n'est pas un acte d'administration du socle : aucun mot de passe n'est demandé, aucune clé d'opération. Un échec (service inconnu, moteur muet) n'écrit pas de réussite.
- **Types** : côté agent, un nom est un `Hidden` et une valeur un `Secret` (le `Secret` est effacé à sa libération, `Debug` masqué des deux). Les AUTRES copies de la valeur ne sont PAS effacées de la mémoire : la réponse d'origine de `bollard`, la copie portée par `Concealed` sur le fil, le JSON sérialisé et les tampons de `hyper`. Sur le fil, `Concealed` : `Debug` masqué. Jamais dans une trace, un `Debug`, la base ni un message d'erreur ; la clé du chemin n'est pas répétée dans les erreurs.
- **Réponse non stockable** : `Cache-Control: no-store` et `Pragma: no-cache`.
- **Erreurs** : clé mal formée ou service inconnu `404 NOT_FOUND` ; service supervisé dont le conteneur n'existe pas (listé d'après son unité) `404` avec un texte propre ; moteur muet `503 BUSY` ; journal non écrit `503 BUSY` (aucune valeur rendue).
- **Remasquage** (fermeture du détail, changement de service, lien perdu, rôle retiré) : porté par l'interface.
- **Traces de dépendances** : `tungstenite` et `tokio_tungstenite` ne sortent jamais sous `warn` (`TransportGuard`) : elles écrivent chaque message WebSocket au niveau `trace`, donc les noms et la commande du flux.

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/services.rs::env`
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS (`GET /services/{key}/env`, `Access::Services`)`
- `crates/hearth-agent/src/entrypoint/http/auth.rs::{allows, refusal}`
- `crates/hearth-agent/src/domain/accounts/rights.rs::ServicesRight::can_manage_services`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::reveal_env`
- `crates/hearth-agent/src/application/ports/audit_sink.rs::AuditSink::record_required`
- `crates/hearth-agent/src/application/audit.rs::AuditRecorder`
- `crates/hearth-agent/src/application/ports/container_engine.rs::{ContainerEngine::env, EnvEntry}`
- `crates/hearth-agent/src/infrastructure/engine/translate.rs::env_entries_of`
- `crates/hearth-agent/src/domain/audit/action.rs::AuditAction::ServiceEnvReveal`
- `crates/hearth-agent/src/domain/audit/event.rs::Target::Service`
- `crates/hearth-agent/src/infrastructure/logging.rs::GUARDED_TARGETS`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::only_an_account_with_the_right_reveals_the_values_whatever_its_role_and_every_attempt_is_journaled_without_them`
- `crates/hearth-agent/tests/services_https.rs::a_withdrawal_takes_effect_on_the_next_request_without_closing_the_session`
- `crates/hearth-agent/tests/services_https.rs::the_kill_switch_wins_over_the_right_with_two_values_of_the_setting`
- `crates/hearth-agent/tests/services_https.rs::a_revealed_value_reaches_only_the_admin_response_never_a_trace_the_journal_or_the_database`
- `crates/hearth-agent/tests/services_https.rs::no_value_leaves_when_the_journal_cannot_be_written_and_each_reveal_is_its_own_entry`
- `crates/hearth-agent/src/application/services.rs::tests::no_value_is_returned_when_the_journal_write_fails`
- `crates/hearth-agent/src/application/services.rs::tests::the_values_are_read_only_on_request_and_the_success_is_journaled_without_them`
- `crates/hearth-agent/src/application/services.rs::tests::a_reveal_that_fails_reads_nothing_and_journals_no_success`
- `crates/hearth-agent/tests/http_api.rs::every_services_route_refuses_an_account_without_the_right_even_an_administrator`
- `crates/hearth-agent/tests/http_api.rs::a_read_only_account_with_the_right_reaches_nothing_but_the_services`
- `crates/hearth-agent/src/infrastructure/logging.rs::tests::transport_libraries_never_speak_below_warn_whatever_the_filter`

## Cas limites
- Un droit retiré pendant que le détail est ouvert : le serveur refuse au prochain appel même si le bouton est encore affiché (BR-GRANT-010). Un administrateur rétrogradé qui a le droit le garde.
- La réponse contient les valeurs en clair sur le fil (TLS) : un compte qui révèle est, en pratique, `root` du serveur tant que le plan fermé des conteneurs n'est pas livré (SECURITY.md, BR-GRANT-022).
- Le pilotage désactivé (`engine.enabled = false`) l'emporte sur le droit : `404`, le moteur n'est pas ouvert (BR-GRANT-015).

## Règles liées
- BR-SVC-008, BR-SVC-020, BR-SVC-024, BR-AUDIT-005, BR-GRANT-001, BR-GRANT-003, BR-GRANT-004, BR-GRANT-010, BR-GRANT-015

## Historique
- 2026-10-13 — création (HRT-63, tranche 2 des déploiements : détail, superviseur, révélation des variables).
- 2026-10-13 — revue de la PR 81 : la consignation conditionne la réponse (échec fermé, `503`), une entrée par révélation (hors regroupement des répétitions) ; textes corrigés (« une entrée » et effacement de la mémoire).
- 2026-10-13 — HRT-76 (ADR-0041) : la révélation passe de « administrateur » à « compte qui a le droit de gérer les services », `Access::Admin` devient `Access::Services`, refus « Tu n'as pas le droit de gérer les services de ce serveur. », cause « droits insuffisants ». Le correctif de la PR 81 (consignation AVANT la réponse, échec fermé) est conservé.
