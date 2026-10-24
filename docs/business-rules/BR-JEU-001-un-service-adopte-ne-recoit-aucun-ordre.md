---
id: BR-JEU-001
domaine: JEU
titre: Un service adopté, ou lancé par une unité du système, ne reçoit aucun ordre et s'arrête comme avant
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-001), HRT-103
maj: 2026-10-24
---

# BR-JEU-001 — Un service adopté, ou lancé par une unité du système, ne reçoit aucun ordre et s'arrête comme avant

## Règle
Les règles d'arrêt d'un serveur de jeu valent pour un service créé depuis une fiche. Un service adopté, même s'il fait tourner le même jeu, ne reçoit AUCUN ordre : l'agent ne cherche même pas de fiche pour lui (sa configuration appartient à son créateur, BR-ADOPT-014) et son arrêt est celui du moteur ou de l'unité du système (BR-LIFE-008), sans garantie d'enregistrement.

## Application (code)
- `crates/hearth-agent/src/application/service_lifecycle.rs::ServiceLifecycle::game_of`
- `crates/hearth-agent/src/application/game_stop.rs::GameStop::lookup`

## Vérification
- `crates/hearth-agent/tests/game_stop_https.rs::an_adopted_service_receives_no_order_even_when_it_runs_the_same_game`

## Cas limites
- La phrase de BR-JEU-007 (« Hearth ne peut pas demander à {jeu} d'enregistrer son monde. ») et le détail d'un service adopté sans adresse ni mot de passe sont côté client (HRT-104).

## Règles liées
- BR-JEU-007, BR-ADOPT-014, BR-LIFE-008

## Historique
- 2026-10-24 — création (HRT-103, côté agent).
