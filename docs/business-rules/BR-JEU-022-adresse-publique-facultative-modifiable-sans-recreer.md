---
id: BR-JEU-022
domaine: JEU
titre: L'adresse publique d'un serveur de jeu est facultative et se modifie sans recréer le service
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-021, 022), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (section 0 point 2), HRT-215
maj: 2026-11-15
---

# BR-JEU-022 : L'adresse publique d'un serveur de jeu est facultative et se modifie sans recréer le service

## Règle
Un serveur de jeu peut porter une adresse publique : le nom de domaine ou l'adresse IP par lesquels les joueurs le joignent depuis Internet, sans protocole ni port, 253 caractères au plus, sans espace. Elle est gardée dans la description durable du service (`settings.public_address`), rendue par `GET /services/{key}/game` quand elle existe, et se change (ou s'efface avec une valeur vide) par la modification du service, qui ne recrée pas le conteneur. Ce n'est pas un secret. Hearth ne vérifie pas qu'elle est joignable depuis Internet. Le client refuse un protocole, un port, un espace ou plus de 253 caractères avant d'envoyer ; l'agent juge la forme de ce qu'il garde (longueur, caractères de contrôle, espace).

## Application (code)
- `crates/hearth-agent/src/application/game_settings.rs::GameSettings::game_view`
- `crates/hearth-agent/src/domain/services/update.rs::apply` (la modification d'un service de fiche ne change que cette adresse sans recréer)
- `apps/desktop/src/services/game.ts::publicAddressIssue, ::addressOf`
- `apps/desktop/src/components/organisms/PublicAddressDialog.vue`

## Vérification
- `crates/hearth-agent/tests/game_stop_https.rs::the_public_address_is_kept_with_the_service_shown_by_the_game_view_and_changed_without_recreating`
- `apps/desktop/src/pages/ServicesGame.test.ts` (l'adresse publique)

## Cas limites
- La modification est un acte confirmé (mot de passe demandé, jamais couvert par l'élévation) : c'est la modification d'un service (BR-EDIT-017), pas une route plus légère. Écart avec le critère « sans confirmation » du ticket, tranché en faveur de la règle existante (aucune nouvelle route sans acte).
- Aucun champ dans le formulaire d'installation : l'adresse se renseigne après.

## Règles liées
- BR-JEU-020, BR-JEU-021, BR-EDIT-037

## Historique
- 2026-11-15 : création (HRT-215).
