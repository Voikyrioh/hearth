---
id: BR-ENGINE-023
domaine: ENGINE
titre: Agent trop ancien pour gérer les services : message et lien, rien d'autre demandé
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-023), HRT-62
maj: 2026-10-13
---

# BR-ENGINE-023 — Agent trop ancien pour gérer les services : message et lien, rien d'autre demandé

## Règle
Quand l'agent du serveur est trop ancien pour gérer les services (pas de capacité `services` dans `/hello`), la section Services affiche « Ce serveur doit être mis à jour pour gérer les services (agent {version}). » avec un lien « Voir la mise à jour de l'agent » ; rien d'autre n'est demandé à cet agent (ni sujet `services`, ni route). Ce n'est pas un état du moteur : ni carte du moteur, ni liste. Les mesures (`metrics`) continuent.

## Application (code)
- `crates/hearth-link/src/domain/services.rs::agent_serves_services`, `::subscription_topics` : le sujet n'est demandé qu'à un agent qui annonce la capacité.
- `apps/desktop/src/services/view.ts::show` (`old-agent`) ; `apps/desktop/src/pages/Services.vue`.
- HORS DOMAIN pour l'affichage : porté par l'interface.

## Vérification
- `crates/hearth-link/tests/services.rs::an_agent_without_the_capability_never_gets_the_services_topic_and_metrics_keep_flowing`
- `apps/desktop/src/pages/Services.test.ts` : « agent sans la capacité : le message avec la version, le lien vers la mise à jour ».

## Règles liées
- BR-ENGINE-021

## Historique
- 2026-10-13 — création (HRT-62, section Services du client, lecture seule).
