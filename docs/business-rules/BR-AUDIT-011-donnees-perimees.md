---
id: BR-AUDIT-011
domaine: AUDIT
titre: Données périmées quand le lien est coupé
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-011), HRT-05
maj: 2026-10-06
---

# BR-AUDIT-011 — Données périmées quand le lien est coupé

## Règle
Côté interface et bibliothèque de liaison : les événements déjà chargés restent affichés, marqués périmés ; au retour du lien, la liste se met à jour avec les événements manqués. L'agent n'a rien de propre à faire : la page se recharge par curseur, sans état serveur.

## Application (code)
- Sans objet côté agent : `GET /audit` est sans état.
- `apps/desktop/src/stores/audit.ts::useAuditStore::{resume, catchUp}`
- `apps/desktop/src/pages/Audit.vue` (étiquette « Périmé », message, surveillance du retour du lien)
- `apps/desktop/src/layouts/ServerLayout.vue` (le gabarit désature et date la page : une seule fois, la page ne s'enveloppe pas)

## Interface
Hors « Connecté » : la liste reste, désaturée et datée par le gabarit (`StaleSurface`) ; la page ajoute l'étiquette « Périmé » et « Données périmées, serveur injoignable ». Au retour du lien : relecture de la tête du journal jusqu'à retrouver ce qui est déjà affiché (les entrées manquées s'ajoutent, sans doublon ni trou ; plus de 10 pages à relire : on repart de la tête), puis « Lien rétabli, données à jour ».

## Vérification
- `apps/desktop/src/stores/audit.test.ts` (« coupure et retour du lien »).
- `apps/desktop/src/pages/audit.test.ts` (« hors Connecté : étiquette Périmé… »).
- `apps/desktop/e2e/audit.spec.ts` (« perte du lien… »).

## Cas limites
- Règle de l'interface : fiche tenue ici pour mémoire.

## Règles liées
- BR-AUDIT-010, BR-AUDIT-020, BR-RESIL-008.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
- 2026-10-06 — section « Interface » et pointeurs du client (HRT-14, session 2026-10-04-hearth-creation).
