---
id: BR-FICHE-056
domaine: FICHE
titre: Un premier démarrage long, déclaré par la fiche, n'est pas une panne : le service est « En marche » en phase « Premier démarrage » jusqu'au signe ou au délai de la fiche
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-056), HRT-101
maj: 2026-10-24
---

# BR-FICHE-056

## Règle
Une fiche dont le premier démarrage est long (téléchargements déclarés, génération d'un grand monde) le dit ; l'installation se termine quand le service tourne (la surveillance de 10 secondes ne change pas), puis la phase est « Premier démarrage » jusqu'au signe « prêt » ou jusqu'au délai annoncé, après quoi elle devient « Hearth ne peut pas savoir si le jeu est prêt » sans que rien soit arrêté ni relancé.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/readiness.rs::{ReadyRule::of, phase_of}`
- `apps/desktop/src/fiche/firstStart.ts`
- `apps/desktop/src/components/organisms/FirstStartPanel.vue`

## Vérification
- `crates/hearth-agent/src/domain/catalog/readiness.rs::the_phases_follow_the_first_start_the_sign_and_the_delay_of_the_fiche`
- `apps/desktop/src/fiche/fiche.test.ts::le premier démarrage (BR-FICHE-056)`

## Cas limites
- Les octets écrits relevés toutes les 30 secondes et l'étape en cours (repères de journal) ne sont pas dans cette version.
- Le panneau n'est pas encore posé dans le détail d'un service : l'agent ne projette pas la phase (suivi HRT-197).

## Règles liées
- BR-FICHE-028, BR-FICHE-010

## Historique
- 2026-10-24 — création (HRT-101).
- 2026-10-24 — côté client (HRT-102).
