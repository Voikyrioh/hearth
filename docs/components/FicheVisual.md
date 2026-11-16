# FicheVisual

Atome · `apps/desktop/src/components/atoms/FicheVisual.vue`

L'image d'un jeu ou son visuel neutre (HRT-239, ADR-0061, BR-FICHE-062). La source est choisie ici : l'identifiant Steam du jeu (`steamAppId`, en-tête du magasin chargé et gardé par la COQUILLE), sinon l'illustration livrée avec l'agent, sinon le visuel neutre tout de suite. La page n'ouvre aucune connexion : l'image n'est qu'une `<img>` du protocole `hearth-illustration` (adresses construites par le pont, `LinkBridge.steamHeaderUrl` et `agentIllustrationUrl`). Une image qui échoue passe au visuel neutre pour toute la visite et n'est journalisée qu'une fois (`catalog:image`). Le rapport 460:215 est réservé d'avance. Le visuel neutre est décoratif (`alt` vide, jetons CSS) ; l'image a pour texte « Image du jeu {nom} ».

- Props : `serverId`, `card` (`FicheCard`), `variant` (`card` ou `banner` : l'image à sa taille, centrée, sur un fond flou de la même image)
- Notes : Jetons `--banner-height`, `--banner-blur`, `--ember-glow`. Fixtures neutres du mode simulé : `public/fixtures/illustrations/` (`scripts/build-fixture-illustrations.py`). Tests : `components/atoms/FicheVisual.test.ts`, `link/illustrations.test.ts`.
