---
id: BR-SVC-029
domaine: SVC
titre: La bande « Opérations en cours » est visible de tous les comptes et se reconstruit depuis le serveur
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-029), contexts/hearth/conceptions/2026-10-13-design-deploiements-lots-suivants.md §8, HRT-84
maj: 2026-10-17
---

# BR-SVC-029 — La bande « Opérations en cours »

## Règle
La bande est le lieu où retrouver une opération longue une fois sa fenêtre fermée. Elle est visible de tous les comptes du serveur ; chaque ligne dit le service, l'opération, son étape, sa progression et « Lancée par {compte}. ». Les comptes sans le droit de gérer les services lisent « Effacement des données de {nom} en cours. » sans chemin ni progression. Une opération toujours en cours au retour du lien est reconstruite depuis l'état du serveur, sans message ; l'issue d'une opération terminée est annoncée une seule fois (BR-RESIL-010). Une opération qui attend une décision porte le bouton de cette décision, réservé au droit : sans lui le bouton est désactivé avec sa raison. La décision est un acte d'administration confirmé, mot de passe redemandé à chaque réponse. Aucun bouton d'annulation dans la bande.

## Application (code)
- `crates/hearth-link/src/domain/service_op_tracker.rs::OpTracker` (une issue une fois, reconstruction sans message)
- `crates/hearth-link/src/manager/task.rs` (relecture des opérations non redites au retour du lien)
- `apps/desktop/src-tauri/src/service_ops/book.rs::OpsBook` (état tenu, rejeu, acquittement)
- `apps/desktop/src-tauri/src/service_ops/service.rs::decide`
- `apps/desktop/src/stores/serviceOps.ts`
- `apps/desktop/src/components/organisms/OperationsStrip.vue`

## Vérification
- `crates/hearth-link/src/domain/service_op_tracker.rs` (tests)
- `crates/hearth-link/tests/service_ops.rs::after_a_cut_a_running_operation_is_rebuilt_silently_and_a_finished_one_announces_once`
- `apps/desktop/src-tauri/tests/service_ops_runtime.rs`
- `apps/desktop/src/pages/ServicesOperations.test.ts` (quatre profils)
- `apps/desktop/e2e/hrt84-operations.spec.ts`

## Cas limites
- Le nom d'un service vient de la liste des services, jamais de l'opération (qui n'en porte pas) ; un service supprimé par son opération garde son nom jusqu'à la fin.
- Un compte qui ne peut pas lire les opérations d'un service adopté ne la voit pas (BR-OPS-001, filtré par l'agent).

## Historique
- 2026-10-17 — création (HRT-84, suivi uniforme côté client).
