---
id: BR-DEL-020
domaine: DEL
titre: Quand les données sont conservées, l'issue le dit avec leur emplacement
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-020), HRT-86
maj: 2026-11-14
---

# BR-DEL-020 : Quand les données sont conservées, l'issue le dit avec leur emplacement

## Règle
À la fin d'une suppression sans la case des données, l'issue écrit « Le service {nom} est supprimé. Ses données sont conservées dans {chemin}. » puis « Si vous créez plus tard un service nommé {nom}, il les retrouvera. ». Avec la case, elle écrit « Le service {nom} et ses données sont supprimés. ». Les chemins viennent de la fenêtre de confirmation, jamais d'une opération (qui n'en porte pas, BR-OPS-006).

## Application (code)
- `apps/desktop/src/components/organisms/DeleteProgressDialog.vue` (`data-delete-success`)
- `apps/desktop/src/composables/useServiceDelete.ts::perform` (les dossiers montrés sont gardés pour l'issue)

## Vérification
- `apps/desktop/src/pages/ServicesDelete.test.ts` (suppression sans case, avec les données)

## Cas limites
- L'emplacement des sauvegardes conservées (BR-DEL-024) n'est pas dit tant que l'agent n'a pas de sauvegardes.

## Règles liées
- BR-DEL-005
- BR-DEL-016

## Historique
- 2026-11-14 : création (HRT-86, côté client).
