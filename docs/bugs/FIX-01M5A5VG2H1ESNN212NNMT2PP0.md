---
id: FIX-01M5A5VG2H1ESNN212NNMT2PP0
titre: Le client proposait Arrêter, Redémarrer et Démarrer sur un service créé hors de Hearth et non adopté, puis répondait « Ce service n'existe plus. » (HRT-65)
date_découverte: 2026-10-19
date_correction: 2026-10-19
---

# FIX-01M5A5VG2H1ESNN212NNMT2PP0 : Le client proposait Arrêter, Redémarrer et Démarrer sur un service créé hors de Hearth et non adopté, puis répondait « Ce service n'existe plus. » (HRT-65)

## Symptôme
Sur un service « Créé hors de Hearth » non adopté (8 services sur 11 du jeu simulé), le détail et le menu « … » proposaient Arrêter, Redémarrer, Démarrer. Après confirmation, une alerte disait « Ce service n'existe plus. » alors que le service était toujours là, en marche, avec les mêmes boutons.

## Reproduction
`apps/desktop/src/pages/ServicesLifecycle.test.ts` (« un service créé hors de Hearth et non adopté n'a ni bouton ni menu ») et `apps/desktop/e2e/hrt65-lifecycle.spec.ts` ; `src/services/lifecycle.test.ts` pour le texte du refus.

## Cause root
`controlsOf` ne regardait que l'état (BR-LIFE-003), jamais l'origine. L'agent, lui, ne pilote que les services publiés, créés par Hearth ou adoptés (BR-LIFE-024), et répond à un non adopté comme à une clé inconnue (`service_not_found`), que le client traduisait en « Ce service n'existe plus. ».

## Impacté
Tout service `origin = external` sans `adopted`, sur le client.

## Workaround
Aucun côté client ; l'agent refusait sans rien modifier.

## Correction
`isNotAdopted` (`origin === "external" && adopted === null`) : `controlsOf` ne propose aucun geste, la note « Hearth ne pilote pas ce service : il a été créé hors de Hearth et ne lui a pas été confié… » tient leur place ; la ligne n'a donc plus de menu « … » (aucun geste, aucun menu). Si un refus `service_not_found` arrive quand même pour un service non adopté, `refusalText` dit cette raison.

## Références
- Ticket : HRT-65, PR #108 ; revue UX Nora, constat 1 ; BR-LIFE-024, BR-LIFE-003
