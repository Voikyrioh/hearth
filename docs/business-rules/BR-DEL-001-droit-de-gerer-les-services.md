---
id: BR-DEL-001
domaine: DEL
titre: Supprimer un service exige le droit de gérer les services, quel que soit le rôle
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-001), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §5 et §9, HRT-85
maj: 2026-11-14
---

# BR-DEL-001 : Supprimer un service exige le droit de gérer les services, quel que soit le rôle

## Règle
Supprimer un service est réservé aux comptes qui ont le droit de gérer les services (BR-GRANT-003), administrateurs ou lecture seule. L'agent refuse un compte sans le droit même si le client est modifié (`403`, le droit manquant en détail), inscrit le refus au journal d'activité (cause « droits insuffisants ») et ne dit rien du service : le droit est jugé avant que la clé du chemin soit lue. Vaut pour la fenêtre (`GET /services/{key}/delete-preview`) comme pour l'acte (`DELETE /services/{key}`).

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS` (niveau `Access::Services` des deux routes)
- `crates/hearth-agent/src/entrypoint/http/auth.rs` (refus faute du droit)
- `apps/desktop/src/services/delete.ts::deleteDoor` (le droit manquant bloque le bouton, sa raison est écrite par `ServiceRightReason`)
- `apps/desktop/src-tauri/src/service_ops/delete.rs::interpret` (le droit retiré entre-temps : `MissingServicesRight`)

## Vérification
- `crates/hearth-agent/tests/service_delete_https.rs::only_an_account_with_the_right_may_preview_or_delete`
- `apps/desktop/src/services/delete.test.ts` (la porte du bouton)
- `apps/desktop/src/pages/ServicesDelete.test.ts` (sans le droit)

## Cas limites
- Un compte en lecture seule qui a le droit peut supprimer : le droit est un axe indépendant du rôle.

## Règles liées
- BR-GRANT-001
- BR-DEL-012

## Historique
- 2026-10-24 : création (HRT-85, côté agent).
- 2026-11-14 : côté client (HRT-86).
