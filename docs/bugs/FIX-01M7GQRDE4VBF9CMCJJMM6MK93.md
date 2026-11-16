---
id: FIX-01M7GQRDE4VBF9CMCJJMM6MK93
titre: Le verrou « Une action est en cours sur ce service » reste affiché après un arrêt réussi (HRT-219)
date_découverte: 2026-11-16
date_correction: 2026-11-16
---

# FIX-01M7GQRDE4VBF9CMCJJMM6MK93 : Verrou de service affiché après la clôture de l'action (HRT-219)

Sévérité : majeur (Démarrer et Supprimer restent grisés, seul le redémarrage de l'agent les libère).

## Symptôme
Smoke du 16/11/2026, service `minecraft-prominence-II` en boucle de plantage : arrêt réussi (opération `01M7GGTT2QR94KZJ5DW2B27B3R`, 10 ms), le conteneur est `exited`, mais le client affiche toujours « Une action est en cours sur ce service. Lancée par vous. ».

## Reproduction
Arrêter un service dont le moteur ne produit plus aucun événement après l'ordre (conteneur déjà arrêté ou en boucle de plantage), garder la liste des services ouverte : test `the_lock_shown_in_the_list_falls_when_the_action_closes_even_if_the_service_did_not_change` (`crates/hearth-agent/tests/service_lifecycle_https.rs`).

## Cause root
`busy` et `last_action` de la liste sont des PROJECTIONS du journal des opérations, lues à l'envoi d'un lot de la liste (`services_wire::with_operations`). Un lot n'existe que si le registre des services change. Pendant l'action, la relecture du conteneur et la fin de l'étape « Arrêt en cours » publient des lots où l'opération est encore ouverte (`busy` rempli) ; la clôture (`ServiceOps::close`) libère le verrou et met l'index à jour, mais ne déclenche aucun lot. Un service qui ne change plus d'état ensuite (aucun événement du moteur) gardait donc `busy` chez les abonnés. Le commentaire de FIX-01M5P0Q3T8ZK6V2R9XH4DWB7NC (« la publication qui suit la rafraîchit ») supposait ce lot ; il n'existait que par hasard, quand un événement du moteur arrivait après la clôture.

## Impacté
Toute opération (arrêt, démarrage, redémarrage, sauvegarde, mise à jour, déploiement) qui se clôt sans changement d'état du service qui suive.

## Workaround
Redémarrer l'agent (la liste est relue).

## Correction
`ServicesRegistry::republish` / `republish_all` re-publient la vue courante sans la changer (nouveau `rev`, même contenu) ; la tâche `spawn_busy_projection` (`entrypoint/tasks.rs`, tenue par `RunningAgent`) écoute le flux des opérations et re-publie le service de toute opération qui atteint un statut terminal (tout le flux après une perte de messages). Le lot repasse par `services_wire`, qui relit `busy` et `last_action` dans l'index déjà à jour (l'index est appliqué avant l'envoi au flux). Alternative écartée : un correctif côté client (effacer `busy` à la fin de l'opération vue) ; l'instantané et `GET /services` resteraient justes, mais un client qui se reconnecte avant tout lot garderait l'erreur côté liste.

## Références
- HRT-219 · BR-LIFE-013 · FIX-01M5P0Q3T8ZK6V2R9XH4DWB7NC
