---
id: ADR-0054
titre: Contrats de licence, jetons de serveur et mods : un contrat ne s'accepte que par un geste du compte, refusé par l'agent sans lui (posé par HRT-101) ; le jeton, le téléchargeur de mods et les packs sont décidés ici et posés avec leurs tickets
type: securite
statut: proposée (la partie contrat est réalisée par HRT-101 ; les autres points sont posés avec HRT-116, HRT-120 et HRT-124 à HRT-128)
date: 2026-10-24
portee: repo
remplace: la version 1 du brouillon (hub, 2026-10-13) ; amende `BR-FICHE-011` et la limite « aucun identifiant de compte » de la version 1 de l'ADR-0050
liens: [ADR-0031, ADR-0041, ADR-0044, ADR-0046, ADR-0050, ADR-0055, BR-FICHE-007, BR-FICHE-011, BR-FICHE-058]
---

> Version 3 du brouillon `contexts/hearth/conceptions/2026-10-13-adr-contrats-jetons-et-mods-brouillon.md` du hub (privé), reprise ici à la livraison de HRT-101. **Ce ticket pose le point 1 (le contrat).** Les points 2 à 8 sont des décisions déjà prises par Voiky (réponses du 2026-10-13 et 2026-10-14) mais SANS code dans ce dépôt : chacun est réalisé, et son statut passe de « décidé » à « réalisé », par le ticket qui l'écrit. Le brouillon du hub garde le détail complet (surface du téléchargeur, table des plateformes, alternatives rejetées).

# ADR-0054 : Contrats de licence, jetons de serveur et mods

## Contexte
Certains jeux exigent que celui qui fait tourner le serveur accepte un contrat de licence (Minecraft), ou un jeton de serveur obtenu sur un site tiers ; beaucoup se jouent avec des mods ou des packs. Hearth lance une image choisie par une fiche embarquée (ADR-0050) : il ne doit jamais accepter à la place de l'utilisateur, ne doit pas garder d'identifiant de compte, et tout ce que l'agent télécharge lui-même est la partie la plus exposée du produit.

## Décision
1. **Contrat de licence (réalisé par HRT-101, BR-FICHE-058).** Une fiche déclare `[[agreement]]` : titre, adresse en `https`, phrase neutre, clé d'une variable d'environnement et valeur d'acceptation. **Jamais par défaut** : la valeur d'acceptation ne peut venir d'aucune variable fixe, variante ni valeur proposée (le chargeur refuse la fiche, invariant I8) ; la traduction ne la pose que si la demande porte l'identifiant du contrat (`translate::translate`, `TranslateError::AgreementRequired`). **L'agent refuse l'installation sans l'acceptation de chaque contrat**, même si le client est modifié : `422 AGREEMENT_REQUIRED`, `details.ids` = les contrats manquants. **Consigné** : la description du service garde, par contrat, son identifiant, le compte qui l'a accepté et l'instant (`SpecFiche.agreements`, `SpecAgreement`) ; le journal d'activité garde le titre de chaque contrat accepté (`Target::GameInstall`, BR-FICHE-032) ; la signature de l'acte `ServiceDeploy` porte l'empreinte des choix de la fiche. Hearth n'embarque ni n'affiche le texte du contrat (il change, et le redistribuer est une question de droit par contrat) : le client ouvre l'adresse dans le navigateur de l'utilisateur et montre une fenêtre à deux boutons de même poids (HRT-129).
2. **Jeton de serveur (HRT-120).** Permis : un réglage `secret` de genre `server_token`, saisi dans le client, scellé par l'agent avec la description du service (ADR-0044), **jamais réaffiché** (« Remplacer », « Retirer »). Aucune connexion Steam, aucune clé d'API de compte.
3. **Identifiant de compte : toujours interdit.** Aucun type de réglage ne représente un nom d'utilisateur et un mot de passe, ni une clé d'API de compte (BR-FICHE-011).
4. **Mods, trois voies par priorité (HRT-124 à HRT-126).** `ids_by_game` (le programme du jeu télécharge, réservé aux plateformes où lui seul le peut), `fetch_by_agent` (l'agent télécharge, point 6), `drop_files` et `drop_pack` (l'utilisateur dépose des fichiers ou un pack serveur ; lecture par le module de l'ADR-0055).
5. **Table fermée des plateformes**, compilée dans l'agent, jamais dans une fiche : Modrinth (API et CDN, SHA-512 exigé) ; l'atelier Steam (par le programme du jeu) ; **aucune clé CurseForge, nulle part** (un fichier venu de CurseForge est déposé par l'utilisateur).
6. **Le téléchargeur de l'agent est borné (HRT-126)** : hôtes figés de la table, jamais une adresse venue du client, redirections suivies à la main (trois au plus, mêmes règles), TLS vérifié, tailles bornées avant allocation, empreinte forte obligatoire, jamais d'exécution ni d'ouverture de ce qui est téléchargé, écriture par `SafeFs` dans le seul dossier de mods du service, acte signé `ServiceModFetch`, coupe-circuit `services.mod_fetch`.
7. **Mise à jour du jeu avec l'accord de l'utilisateur (HRT-128)** : un réglage booléen sans valeur proposée, « non » tant que l'utilisateur n'a pas répondu oui.
8. **Ce que le programme du jeu télécharge est dit, pas vérifié.**

## Comment l'appliquer
- Fiches piégées (déjà en place pour le point 1) : valeur d'acceptation en variable fixe, en variante, en valeur proposée ; contrat lié à autre chose qu'une variable d'environnement.
- Refus `AGREEMENT_REQUIRED` côté agent, testé avec un client qui n'envoie aucun contrat (`tests/fiche_install_https.rs`).
- Une mise à jour de fiche qui change l'identifiant ou l'adresse d'un contrat redemande l'acceptation (HRT-121) ; recréer ou modifier un service la garde.

## Quand NE PAS l'appliquer / limites
- Les conditions de Steam, de Modrinth et de CurseForge n'ont été lues que par extraits résumés : à relire en entier avant de publier la première fiche qui s'en sert.
- Un fichier dont l'empreinte est juste peut être un mod malveillant : l'empreinte prouve que c'est le fichier publié, pas qu'il est sain.
- Hearth ne garantit rien de ce qu'un mod fait dans le conteneur ni de ce que le programme du jeu télécharge.
- L'acceptation d'un contrat est celle du COMPTE qui installe, pas de l'ensemble des joueurs du serveur.

## Alternatives rejetées
- **Une case à cocher dans le récapitulatif** (version 1) : trop facile à passer sans lire ; la fenêtre dédiée à deux boutons l'a remplacée.
- **Accepter le contrat dans la fiche, ou une fois pour tout le serveur** : c'est accepter à la place de celui qui installe.
- **Afficher le texte du contrat dans Hearth** : redistribution d'un texte qui change.
- **Hearth ne télécharge jamais rien** (version 1) : écarté par Voiky ; ses raisons deviennent les garde-fous du point 6.
- **Embarquer ou demander une clé CurseForge** : non transférable ; écartée par Voiky.
- **Garder le jeton dans le client** : un serveur de jeu ne pourrait plus redémarrer seul.

## Conséquences
- `BR-FICHE-058` réalisée ; les règles du jeton, des versions du jeu et des mods sont créées avec leurs tickets.
- L'invariant I5 de l'ADR-0050 (« Hearth ne contacte aucune adresse pour une fiche ») sera reformulé avec HRT-126 : aucune adresse venue d'une fiche ou d'un compte ; les hôtes de la table du point 5 sont les seuls contactés pour un service.
- Nouvel acte `ServiceModFetch` et capacité `mod-fetch` avec HRT-126.

## Références
- `contexts/hearth/conceptions/2026-10-13-adr-contrats-jetons-et-mods-brouillon.md` (hub, privé) : le brouillon complet.
- `docs/open-api/services.md` : `POST /deployments` avec un membre `source` (`AGREEMENT_REQUIRED`).
