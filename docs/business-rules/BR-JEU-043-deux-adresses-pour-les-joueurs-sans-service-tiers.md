---
id: BR-JEU-043
domaine: JEU
titre: Un serveur de jeu donne deux adresses à ses joueurs, celle du réseau maison et celle d'internet ; l'adresse d'internet vient du propriétaire ou de la box, jamais d'un service tiers
statut: active
invariant: false
source: HRT-223 (smoke du 16/11/2026), BR-JEU-021, BR-JEU-022, ADR-0060
maj: 2026-11-16
---

# BR-JEU-043 : Deux adresses pour les joueurs, sans service tiers

## Règle
- La carte « Adresse » du détail d'un serveur de jeu montre deux blocs, chacun avec le port de base du jeu et un bouton « Copier » : « En réseau maison » (joueurs chez vous) et « Depuis internet » (vos amis ; le port est à rediriger sur la box).
- **Réseau maison** : l'adresse IPv4 privée que la machine de l'agent utilise sur son réseau (`GameView.lan_address`, lue sans envoyer de paquet). Absente (agent ancien, machine sans adresse privée) : le client montre l'adresse sous laquelle il joint le serveur.
- **Internet**, par ordre de priorité : (1) l'adresse que le propriétaire a renseignée (BR-JEU-022, `public_address`), (2) l'adresse d'internet que la box a dite à l'agent par UPnP (`GetExternalIPAddress`, `GameView.detected_address`), (3) aucune : la carte invite à renseigner un nom de domaine ou l'adresse de la box.
- **Visibilité** : les deux adresses sont visibles de TOUT compte qui voit le service, quel que soit son rôle (`GET /services/{key}/game` est ouverte à tout compte authentifié). Choix assumé : c'est l'écran qui sert à donner l'adresse aux joueurs ; l'adresse d'internet d'un foyer n'est pas un secret pour ses invités.
- **Seule la passerelle par défaut est interrogée** : l'agent la lit dans la table de routage du système (`/proc/net/route`). La réponse SSDP n'est crue, et la description de la box n'est suivie, que si son émetteur ET l'hôte de l'adresse sont exactement cette passerelle. Une autre machine du réseau ne peut ni imposer une adresse d'internet, ni faire viser une autre adresse (service de métadonnées `169.254.169.254` compris : le lien local n'est jamais un réseau maison). Aucune passerelle lisible : aucune interrogation, aucune adresse détectée.
- **Pare-feu à états** : un pare-feu qui refuse les entrées par défaut (y compris les réponses de multidiffusion) empêche la détection ; elle rend alors `None` en silence, sans erreur. Le propriétaire saisit son adresse (« Modifier l'adresse »).
- **Un seul appel à la fois, en arrière-plan** : la vue du jeu rend TOUT DE SUITE la dernière adresse connue (ou rien) ; une seule découverte part en tâche de fond quand la valeur est absente ou périmée, quel que soit le nombre de requêtes ; la première part au démarrage de l'agent.
- **Jamais un service tiers** : l'agent ne contacte aucun service d'écho d'IP, ni Cloudflare, ni autre. Seule la box du foyer est interrogée, sur le réseau local. Une adresse que la box donne et qui n'est pas publique (privée, double NAT `100.64.0.0/10`, boucle, lien local) est écartée.
- La réponse SSDP est hostile : la description de la box n'est suivie que si elle est en `http`, sur un chemin sûr, et que son émetteur et son hôte sont la passerelle par défaut ; chaque lecture est bornée (64 Kio, 2 s) ; la découverte est bornée à 6 s et son résultat gardé 10 minutes (2 minutes après un échec) ; la vue du jeu n'attend jamais.
- Hearth ne vérifie ni que le port est redirigé, ni que l'adresse est joignable : la carte le dit (BR-JEU-021). Pas d'indication « port redirigé » (aucun test externe, et la lecture des redirections UPnP n'est pas demandée à la box).

## Application (code)
- `crates/hearth-agent/src/application/ports/network.rs::NetworkAddresses`
- `crates/hearth-agent/src/infrastructure/network.rs::{SystemNetwork, default_gateway, location_of, control_url, external_ip_of, is_public}`
- `crates/hearth-agent/src/application/game_settings.rs::GameSettings::game_view`
- `apps/desktop/src/services/game.ts::addressesOf`, `components/molecules/GameAddressCard.vue`

## Vérification
- `crates/hearth-agent/src/infrastructure/network.rs` (tests : seule la passerelle par défaut est crue, lecture de la table de routage, une seule découverte pour N requêtes, description suivie ou refusée, lecture de la box, adresses non publiques, échange SOAP contre une fausse box)
- `crates/hearth-agent/tests/game_stop_https.rs::the_game_view_carries_the_home_network_address_and_the_one_the_box_gave`
- `apps/desktop/src/services/game.test.ts` (`addressesOf`), `apps/desktop/src/pages/ServicesGame.test.ts`

## Cas limites
- Box sans UPnP (ou UPnP désactivé), double NAT : pas d'adresse d'internet détectée ; le propriétaire la saisit.
- IPv6 : écrite entre crochets ; une adresse IPv6 d'internet n'est pas lue par la box (UPnP IGD v1).
- Les tests de l'agent n'interrogent jamais le réseau (`CatalogParts.network` vaut `None`).

## Règles liées
- BR-JEU-020, BR-JEU-021, BR-JEU-022

## Historique
- 2026-11-16 : création (HRT-223).
