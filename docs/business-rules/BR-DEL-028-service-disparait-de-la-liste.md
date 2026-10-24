---
id: BR-DEL-028
domaine: DEL
titre: Après une suppression, le service disparaît de la liste pour tous les comptes
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-028), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §5 et §9, HRT-85
maj: 2026-10-24
---

# BR-DEL-028 : Après une suppression, le service disparaît de la liste pour tous les comptes

## Règle
Le conteneur supprimé est retiré du registre à l'instant (`forget_hearth_container`), donc le service quitte la liste publiée sans attendre le prochain événement du moteur ; sa description est oubliée, un détail ouvert sur lui devient « Ce service n'existe plus ». L'image reste sur le serveur mais n'est pas un service.

## Application (code)
- `crates/hearth-agent/src/application/delete.rs::ServiceDeletion::remove_containers`

## Vérification
- `crates/hearth-agent/tests/service_delete_https.rs::a_deletion_without_a_box_removes_the_service_and_keeps_every_byte_of_data`

## Cas limites
- Aucun.

## Règles liées
- BR-DEL-004

## Historique
- 2026-10-24 : création (HRT-85, côté agent).
