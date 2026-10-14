---
id: BR-SVC-027
domaine: SVC
titre: Le détail d'un service adopté dit qui l'a adopté et que sa configuration reste celle de son créateur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-027), contexts/hearth/conceptions/2026-10-13-amendements-droit-et-adoption.md, HRT-63
maj: 2026-10-14
---

# BR-SVC-027 — Détail d'un service adopté

## Règle
Le détail d'un service adopté montre « Adopté par {compte} le {date} » et, dans la carte de l'image, « Ce service a été créé hors de Hearth : sa configuration reste celle de son créateur. » (configuration en lecture seule).

- Le compte vient de l'agent : il n'est écrit que s'il a la forme d'un identifiant Hearth (`isHearthUsername`), sinon la phrase est écrite sans compte (« Adopté le {date} »), ou sans date si elle manque (« Adopté par {compte} »). Rien n'est inventé.
- « Ne plus gérer ce service » (BR-ADOPT-013, BR-ADOPT-014) n'est PAS livré par ce ticket : le bouton est absent, pas désactivé. Cette fiche sera complétée avec lui.

## Application (code)
- `apps/desktop/src/services/detail.ts::adoptedLine`
- `apps/desktop/src/components/organisms/ServiceDetail.vue` (ligne « Adopté par », note de configuration)
- HORS DOMAIN : règle d'affichage portée par l'interface ; le fait (`adopted`) vient de l'agent, `crates/hearth-proto/src/api/services.rs::ServiceItem`.

## Vérification
- `apps/desktop/src/services/detail.test.ts` (`adoptedLine`)
- `apps/desktop/src/pages/ServicesDetail.test.ts` (service adopté)

## Règles liées
- BR-SVC-007, BR-SVC-028, BR-ADOPT-013, BR-ADOPT-014

## Historique
- 2026-10-14 — création (HRT-63 : fiche manquante, la CI `br-check` la réclamait).
