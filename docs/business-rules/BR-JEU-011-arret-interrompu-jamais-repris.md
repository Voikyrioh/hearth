---
id: BR-JEU-011
domaine: JEU
titre: Une séquence d'arrêt interrompue n'est jamais reprise seule
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-JEU-011), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0, 0 bis et 6), HRT-83
maj: 2026-10-13
---

# BR-JEU-011 — Une séquence d'arrêt interrompue n'est jamais reprise seule

## Règle
L'arrêt d'un serveur de jeu qui enregistre son monde est une opération du journal : interrompue par un redémarrage de l'agent, elle est classée comme toute autre (BR-EDIT-031) et rien n'est rejoué ni renvoyé au jeu. L'ordre envoyé au jeu est HRT-103.

## Application (code)
- `crates/hearth-agent/src/domain/ops/recover.rs::recover`

## Vérification
- `crates/hearth-agent/src/domain/ops/recover.rs::tests::the_table_of_the_design`

## Cas limites
- L'envoi d'un ordre au jeu n'existe pas dans ce ticket.

## Règles liées
- BR-EDIT-031

## Historique
- 2026-10-13 — création (HRT-83, ADR-0044 et ADR-0045 : socle des services créés par Hearth, côté agent).
