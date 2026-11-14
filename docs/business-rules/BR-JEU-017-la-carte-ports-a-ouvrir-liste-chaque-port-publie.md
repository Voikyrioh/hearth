---
id: BR-JEU-017
domaine: JEU
titre: Le détail d'un serveur de jeu porte une carte « Ports à ouvrir » : chaque port publié, son protocole et son rôle, pour tous les profils
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-017), HRT-104
maj: 2026-11-14
---

# BR-JEU-017 : Le détail d'un serveur de jeu porte une carte « Ports à ouvrir » : chaque port publié, son protocole et son rôle, pour tous les profils

## Règle
Le détail d'un service de jeu porte une carte « Ports à ouvrir », visible de tous les profils, qui liste chaque port publié avec son numéro, son protocole (TCP ou UDP) et son rôle (« Port du jeu »). Cette liste est distincte de la carte « Ports » du détail (port du serveur vers port du service) : les deux sont gardées, parce qu'elles servent à deux usages (relier un port, puis l'ouvrir sur la box). Sous la liste : « Pour que des joueurs hors de votre réseau vous rejoignent, ouvrez ces ports sur votre box (redirection de ports vers l'adresse de ce serveur) et, si votre serveur a un pare-feu, sur ce pare-feu. Hearth ne les ouvre pas lui-même. ». « Copier la liste » copie « {port}/{protocole} », une ligne par port. Les numéros viennent de l'agent (`GET /services/{key}/game`), qui les lit dans la description du service.

## Application (code)
- `apps/desktop/src/components/molecules/GamePortsCard.vue`
- `apps/desktop/src/services/game.ts::{portRows, portsListText}`
- `crates/hearth-agent/src/application/game_settings.rs::GameSettings::game_view`

## Vérification
- `apps/desktop/src/services/game.test.ts`
- `apps/desktop/src/pages/ServicesGame.test.ts`
- `crates/hearth-agent/tests/game_stop_https.rs::the_game_view_tells_the_fiche_the_stop_the_real_ports_and_a_password_yes_never_its_value`

## Cas limites
- Aucune.

## Règles liées
- BR-JEU-018, BR-JEU-019, BR-SVC-007

## Historique
- 2026-11-14 : création (HRT-104, côté client et agent).
