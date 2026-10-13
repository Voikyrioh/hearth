---
id: BR-GRANT-011
domaine: GRANT
titre: Le client lit le droit du compte connecté à l'ouverture de la section Services, à chaque reconnexion et après tout refus pour absence du droit
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-droit-de-gerer-les-services.md (BR-GRANT-011), HRT-78
maj: 2026-10-13
---

# BR-GRANT-011 — Le client lit le droit du compte connecté à l'ouverture de la section Services, à chaque reconnexion et après tout refus pour absence du droit

## Règle
Lecture seule (`GET /me` et capacité `service-rights` de `GET /hello`). Pendant la lecture (5 s au plus) les boutons sont désactivés sans raison ; en échec : « Impossible de lire ton droit sur les services. Réessaie. » et « Vérifier à nouveau ». Lien coupé : les derniers droits connus restent (boutons désactivés pour cause de lien coupé). Un refus `missing_services_right` relit le droit (`noteRefusal`) et dit « Tu n'as plus le droit de gérer les services de ce serveur. » ; une fenêtre d'acte ouverte se ferme (`ActReport right_lost`). Le serveur fait foi entre deux lectures.

## Application (code)
- `crates/hearth-link/src/manager/rights.rs::LinkManager::service_rights`
- `apps/desktop/src-tauri/src/services/commands.rs::read_service_rights`
- `apps/desktop/src/stores/serviceRights.ts::useServiceRightsStore`, `composables/useServiceRights.ts`, `composables/useServerAction.ts` (`rightLost`)

## Vérification
- `crates/hearth-link/tests/rights_read.rs`, `apps/desktop/src-tauri/tests/accounts_runtime.rs` (lecture contre un vrai agent), `tests/rights_wire.rs`
- `stores/serviceRights.test.ts`, `link/tauri-rights.test.ts`, `composables/rightLost.test.ts`

## Règles liées
BR-GRANT-004, BR-GRANT-010, BR-GRANT-014, BR-GRANT-016

## Historique
- 2026-10-13 — création (HRT-78, moitié client).
