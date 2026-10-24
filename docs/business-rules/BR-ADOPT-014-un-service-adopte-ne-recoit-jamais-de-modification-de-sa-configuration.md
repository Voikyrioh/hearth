---
id: BR-ADOPT-014
domaine: ADOPT
titre: Un service adopté ne reçoit jamais de Hearth une modification, un redémarrage de réglages, un ordre ou une suppression ; sa configuration reste celle de son créateur
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-014), HRT-103
maj: 2026-10-24
---

# BR-ADOPT-014 — Un service adopté ne reçoit jamais de Hearth une modification de sa configuration

## Règle
Un service adopté ne reçoit jamais de Hearth : modification de ses réglages (variables, ports, dossiers, commande, redémarrage), de ses ressources, de son image ou de sa version, duplication, suppression. Sa configuration appartient à qui l'a créé. Les écrans l'affichent en lecture seule avec la raison « Ce service a été créé hors de Hearth : sa configuration reste celle de son créateur. ». Un service adopté n'a pas de description durable (BR-EDIT-035) : Hearth ne tient de lui que des réglages propres à Hearth.

## Application (code)
- `crates/hearth-agent/src/application/service_lifecycle.rs::ServiceLifecycle::game_of` (HRT-103 : un service adopté ne reçoit aucun ordre d'arrêt de jeu, jamais d'écriture sur son entrée)
- `crates/hearth-agent/src/application/game_stop.rs::GameStop::lookup` (un service sans description n'est pas un serveur de jeu)

## Vérification
- `crates/hearth-agent/tests/game_stop_https.rs::an_adopted_service_receives_no_order_even_when_it_runs_the_same_game`

## Cas limites
- Cette fiche est créée par HRT-103 pour le seul volet « aucun ordre de jeu » ; les autres volets (réglages, ressources, image, suppression, écrans en lecture seule) relèvent des tickets d'adoption, de modification et de suppression, qui ajouteront leur code et leurs tests ici.

## Règles liées
- BR-JEU-001, BR-ADOPT-013, BR-SVC-027

## Historique
- 2026-10-24 — création de la fiche (HRT-103), volet « aucun ordre de jeu ».
