---
id: BR-BACKUP-001
domaine: BACKUP
titre: Créer, vérifier, supprimer une sauvegarde et régler les éléments ignorés sont réservés aux comptes qui ont le droit de gérer les services, quel que soit leur rôle
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-sauvegardes-service.md (BR-BACKUP-001), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 6, 7 et 9.4), HRT-97
maj: 2026-11-14
---

# BR-BACKUP-001 : Créer, vérifier, supprimer une sauvegarde et régler les éléments ignorés sont réservés aux comptes qui ont le droit de gérer les services, quel que soit leur rôle

## Règle
La **liste** des sauvegardes d'un service créé par Hearth est visible de tout compte authentifié (ses boutons sont désactivés côté client) ; l'estimation de la fenêtre, la création (`0x19`), la vérification, la suppression (`0x1B`) et le réglage des éléments ignorés (`0x1C`) exigent le droit « gérer les services » (niveau `Access::Services`, jugé avant de lire la clé du chemin). Le refus faute du droit est consigné sous l'action de la route. Pour un service adopté, la liste n'est pas proposée dans cette tranche (voir BR-BACKUP-028).

## Application (code)
- `crates/hearth-agent/src/application/backups.rs (list, preview, create, verify, delete, set_settings)`
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS (routes /services/{key}/backups*, /backups/{id}*)`
- `crates/hearth-agent/src/entrypoint/http/backups.rs`

## Vérification
- `crates/hearth-agent/tests/service_backups_https.rs::only_an_account_with_the_right_acts_whatever_its_role_and_everyone_reads_the_list`

## Cas limites
- Un compte en lecture seule qui a le droit agit : le droit est un axe indépendant du rôle.

## Règles liées
- BR-GRANT-003
- BR-BACKUP-028

## Historique
- 2026-11-14 : création (HRT-97, côté agent).
