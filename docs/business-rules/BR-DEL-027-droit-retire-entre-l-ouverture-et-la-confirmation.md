---
id: BR-DEL-027
domaine: DEL
titre: Un droit retiré entre l'ouverture de la fenêtre et la confirmation refuse la suppression
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-027), HRT-86
maj: 2026-11-14
---

# BR-DEL-027 : Un droit retiré entre l'ouverture de la fenêtre et la confirmation refuse la suppression

## Règle
Si le droit de gérer les services est retiré au compte entre l'ouverture de la fenêtre et la confirmation, l'agent refuse l'acte (`403 FORBIDDEN_ROLE`, `details.missing: services`) : la suppression n'a pas lieu, la fenêtre se ferme, « Vous n'avez plus le droit de gérer les services de ce serveur. » est dit une fois et le droit est relu (les boutons se désactivent, BR-GRANT-011). La même chose vaut pour la consultation des dossiers. Un droit retiré PENDANT la suppression ne l'arrête pas : elle va à son terme (BR-GRANT-010).

## Application (code)
- `apps/desktop/src-tauri/src/service_ops/delete.rs::interpret` (le refus de droit devient `MissingServicesRight`)
- `apps/desktop/src/composables/useServiceDelete.ts::{loadPreview, perform}` (« Vous n'avez plus le droit… », relecture du droit, fenêtre fermée)

## Vérification
- `apps/desktop/src-tauri/src/service_ops/delete.rs` (test du droit retiré entre-temps)
- `apps/desktop/src/pages/ServicesDelete.test.ts` (droit retiré avant la confirmation)

## Cas limites
- Aucun.

## Règles liées
- BR-DEL-001
- BR-GRANT-010
- BR-GRANT-011

## Historique
- 2026-11-14 : création (HRT-86, côté client).
