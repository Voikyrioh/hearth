---
id: BR-SVC-028
domaine: SVC
titre: La commande et les noms de variables ne sont montrés qu'au droit de gérer les services, et seulement s'ils ont été lus ; sinon une phrase dit pourquoi
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-028), BR-ADOPT-015, revue de la PR 93 (2026-10-14), HRT-63
maj: 2026-10-14
---

# BR-SVC-028 — Éléments réservés, et détails pas encore lus

## Règle
Dans le détail d'un service, la commande et les noms de variables sont remplacés, au même endroit, par une phrase quand le client ne peut pas les montrer, sans masquer le reste du détail :

- **Réservé** (« Réservé aux comptes qui peuvent gérer les services. ») : l'agent les a retirés (service adopté, compte sans le droit, BR-ADOPT-015, `details_reserved`), OU le compte n'a pas le droit de gérer les services.
- **Droit pas connu** (« Visible seulement pour les comptes qui peuvent gérer les services. Ton droit n'est pas connu pour l'instant. ») : lecture du droit en cours, en échec, agent ancien, pilotage désactivé. Par prudence, rien n'est montré.
- **Pas lus** (« Pas encore lue… » / « Pas encore lus… ») : la liste vient du disque. Elle y est écrite SANS la commande ni les noms de variables (BR-SVC-020, ils peuvent porter des secrets), donc leur absence n'y veut PAS dire « aucune variable » ni « commande par défaut ». La liaison le dit (`details_known`), l'écran le répète.

RÈGLE PRUDENTE, DÉCISION DU PROPRIÉTAIRE ATTENDUE (2026-10-14) : la commande peut porter un secret (`--token=…`). En attendant un filtre côté agent, le client ne la montre, avec les noms, qu'aux comptes qui ont le droit de gérer les services, QUEL QUE SOIT le service, y compris un service créé par Hearth (BR-SVC-007 et BR-SVC-008 les disaient visibles des deux rôles : écart assumé, à trancher). Ticket de suivi : l'agent doit appliquer ce filtre lui-même (`services_wire`), le client n'est pas l'arbitre.

## Application (code)
- `apps/desktop/src/services/detail.ts::detailsView`
- `apps/desktop/src/components/organisms/ServiceDetail.vue` (commande), `apps/desktop/src/components/organisms/EnvVarsSection.vue` (noms)
- `crates/hearth-link/src/domain/server.rs::LastKnown::{services_details, without_secrets}`, `crates/hearth-link/src/manager/task.rs` (Runner : `services_details`), `Event::Services { details_known }`
- `apps/desktop/src-tauri/src/services/dto.rs::ServicesEvent (details_known)`, `apps/desktop/src/link/services.ts::toServicesEvent`
- HORS DOMAIN pour l'affichage : règle portée par l'interface.

## Vérification
- `apps/desktop/src/services/detail.test.ts` (`detailsView`)
- `apps/desktop/src/pages/ServicesDetail.test.ts` (« liste relue du disque… », « réservés au droit de gérer les services »)
- `apps/desktop/e2e/hrt63-detail.spec.ts` (redémarrage hors ligne)
- `crates/hearth-link/src/adapters/file_store.rs` (« the_last_view_on_disk_carries_no_env_name_and_no_command » : une vue relue dit « pas lus »), `crates/hearth-link/tests/services.rs`
- `apps/desktop/src-tauri/tests/services_runtime.rs::a_list_read_back_from_the_disk_says_its_details_were_not_read`

## Règles liées
- BR-SVC-007, BR-SVC-008, BR-SVC-009, BR-SVC-020, BR-SVC-027, BR-ADOPT-015

## Historique
- 2026-10-14 — création (HRT-63 : fiche manquante réclamée par `br-check`) ; revue de la PR 93 : « pas lus » distingué de « aucun », commande et noms réservés au droit.
