---
id: BR-SVC-009
domaine: SVC
titre: Seul un administrateur révèle les valeurs des variables ; chaque révélation est consignée, sans nom ni valeur
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-009), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, HRT-63
maj: 2026-10-13
---

# BR-SVC-009 — Révélation des valeurs réservée aux administrateurs

## Règle
`GET /api/v1/services/{key}/env` rend les variables du service avec leurs valeurs.

- **Rôle** : la route est `Access::Admin`. La couche d'accès refuse un compte en lecture seule (`403 FORBIDDEN_ROLE`, « Réservé aux administrateurs. ») AVANT tout handler, même si le client est modifié, et CONSIGNE le refus (`service.env.reveal`, résultat « refusé », cible : la route `/services/{key}/env`). Une session absente ou inconnue est refusée sans journal, comme partout.
- **À la demande seulement** : aucune valeur n'est lue en arrière-plan ni envoyée d'avance ; une relecture de la liste, un événement du moteur, un abonnement au flux ne lisent jamais `Config.Env` au-delà des noms (BR-SVC-008). Le moteur n'est interrogé (`ContainerEngine::env`) que par cette route.
- **Journal** : une révélation réussie écrit UNE entrée `service.env.reveal` (« Valeurs des variables révélées », cible : « service {nom} », résultat « réussi »), avant que les valeurs partent. L'entrée ne porte ni nom ni valeur de variable (`Target::Service` n'a pas de champ pour eux). Ce n'est pas un acte d'administration du socle : aucun mot de passe n'est demandé, aucune clé d'opération. Un échec (service inconnu, moteur muet) n'écrit pas de réussite.
- **Types** : côté agent, un nom est un `Hidden` et une valeur un `Secret` (effacé de la mémoire à la libération, `Debug` masqué). Sur le fil, `Concealed` : `Debug` masqué. Jamais dans une trace, un `Debug`, la base ni un message d'erreur ; la clé du chemin n'est pas répétée dans les erreurs.
- **Réponse non stockable** : `Cache-Control: no-store` et `Pragma: no-cache`.
- **Erreurs** : clé mal formée ou service inconnu `404 NOT_FOUND` ; service supervisé dont le conteneur n'existe pas (listé d'après son unité) `404` avec un texte propre ; moteur muet `503 BUSY`.
- **Remasquage** (fermeture du détail, changement de service, lien perdu, rôle retiré) : porté par l'interface.
- **Traces de dépendances** : `tungstenite` et `tokio_tungstenite` ne sortent jamais sous `warn` (`TransportGuard`) : elles écrivent chaque message WebSocket au niveau `trace`, donc les noms et la commande du flux.

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/services.rs::env`
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS (`GET /services/{key}/env`, `Access::Admin`)`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::reveal_env`
- `crates/hearth-agent/src/application/ports/container_engine.rs::{ContainerEngine::env, EnvEntry}`
- `crates/hearth-agent/src/infrastructure/engine/translate.rs::env_entries_of`
- `crates/hearth-agent/src/domain/audit/action.rs::AuditAction::ServiceEnvReveal`
- `crates/hearth-agent/src/domain/audit/event.rs::Target::Service`
- `crates/hearth-agent/src/infrastructure/logging.rs::GUARDED_TARGETS`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::only_an_administrator_reveals_the_values_and_every_attempt_is_journaled_without_them`
- `crates/hearth-agent/tests/services_https.rs::a_revealed_value_reaches_only_the_admin_response_never_a_trace_the_journal_or_the_database`
- `crates/hearth-agent/src/application/services.rs::tests::the_values_are_read_only_on_request_and_the_success_is_journaled_without_them`
- `crates/hearth-agent/src/application/services.rs::tests::a_reveal_that_fails_reads_nothing_and_journals_no_success`
- `crates/hearth-agent/tests/http_api.rs::every_admin_route_refuses_a_read_only_account_and_changes_nothing`
- `crates/hearth-agent/src/infrastructure/logging.rs::tests::transport_libraries_never_speak_below_warn_whatever_the_filter`

## Cas limites
- Un compte rétrogradé pendant que le détail est ouvert : la route est `Admin`, le serveur refuse au prochain appel même si le bouton est encore affiché.
- La réponse contient les valeurs en clair sur le fil (TLS) : un administrateur qui révèle est, en pratique, `root` du serveur (SECURITY.md).

## Règles liées
- BR-SVC-008, BR-SVC-020, BR-SVC-024, BR-AUDIT-005

## Historique
- 2026-10-13 — création (HRT-63, tranche 2 des déploiements : détail, superviseur, révélation des variables).
