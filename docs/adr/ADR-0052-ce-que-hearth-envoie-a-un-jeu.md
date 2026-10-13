---
id: ADR-0052
titre: Ce que Hearth envoie à un jeu : des arguments en jetons d'alphabet fermé, des ordres littéraux de la fiche par l'entrée standard, un arrêt toujours conclu par le moteur ; la console distante est déclarée, jamais publiée, et Hearth ne s'y connecte pas dans ce format
type: securite
statut: proposée (non validée par Voiky ; posée avec HRT-113)
date: 2026-10-13
portee: repo
remplace: le point 6 de la version 1 de l'ADR-0050 (un ordre d'enregistrement et un ordre de sortie)
liens: [ADR-0036, ADR-0041, ADR-0044 (brouillon), ADR-0045 (brouillon), ADR-0050, conception technique 2026-10-13 (fiches de jeu) sections 6.2, 6.3 et 11.3, 2026-10-13-reglages-des-jeux (T32 à T36), 2026-10-13-inventaire-des-jeux (section 5, points 6 et 7), BR-JEU-002 à JEU-016, JEU-034, BR-JEU-038, BR-JEU-039, GRANT-018]
---

> Posée par HRT-113 pour ses parties « arguments » (points 1 à 3) et « console déclarée » (points 7 à 9, la déclaration seulement). Les points 4 à 6 (ordres par l'entrée standard, séquence bornée, arrêt toujours conclu par le moteur) sont posés pour leur PLAN (`stop_plan.rs`, qui ne déroule rien) ; leur déroulé est à HRT-103. Les identifiants de règles écrits sans préfixe `BR` (par exemple `FICHE-041`) sont des règles de la spec du hub (`contexts/hearth/`), sans fiche dans ce dépôt tant que leur ticket n'est pas livré.

# ADR-0052 : Ce que Hearth envoie à un jeu

## Contexte
Trois choses atteignent le programme d'un jeu depuis Hearth : sa ligne de commande, son entrée standard, et le signal d'arrêt. Cinq réglages de Terraria n'existent qu'en argument, et d'autres jeux en ont. Presque tous les jeux de l'inventaire ont une console distante protégée par mot de passe. La version 1 n'avait aucun champ d'argument, écartait la console sans dire où vivrait son secret, et attendait la fin du conteneur après un ordre de sortie, ce qui le ferait relancer par sa règle de redémarrage.

## Décision
1. **Arguments : un canal `args`, jamais un texte.** La fiche déclare le nom de chaque argument (littéral) et sa forme (`flag_if_true`, ou `flag_value` en deux jetons). Le plan reçoit une liste de jetons (forme tableau de la commande, déjà dans le plan du lot 1). L'utilisateur ne fournit jamais un nom d'argument.
2. **Alphabet fermé pour une valeur d'argument** : lettres, chiffres, point, tiret, souligné, deux-points ; 64 caractères au plus ; ne commence ni par `-` ni par `+`, hors le signe d'un entier (`-` suivi de chiffres, revalidé comme entier à la traduction : il ne nomme pas une option). La même fermeture vaut pour le texte de la FICHE : une valeur dérivée imposée n'a pour cible qu'un réglage `text`, et son préfixe et son suffixe tiennent dans le jeu de caractères de la cible (`arg_safe` sur le canal des arguments), refusé au chargement (`DeriveInvalide`). Aucun caractère de cet alphabet n'est traité par un interpréteur : une image qui recolle ses arguments sans guillemets ne peut pas être détournée par une valeur.
3. **Jamais un secret en argument** : les arguments d'un conteneur sont lisibles par tout lecteur du moteur.
4. **Ordres par l'entrée standard** : seulement des littéraux de la fiche (64 caractères ASCII imprimables, sans emplacement à remplir), envoyés par `ContainerEngine::send_stdin_line` dans une séquence d'arrêt ou d'enregistrement confirmée par un compte qui a le droit. Aucune route ne porte de texte vers l'entrée d'un jeu.
5. **Séquence bornée et déclarée** : au plus six étapes parmi `stdin_line`, `wait_log` (sous-chaîne littérale, dans les seules lignes postérieures), `wait` ; 20 minutes au total ; délai d'arrêt de 10 à 600 secondes ; signal d'arrêt dans une énumération (celui de l'image, `SIGTERM`, `SIGINT`). Une étape n'est jamais renvoyée. Une séquence `untested` n'est jamais exécutée : décision « Arrêter quand même » ou « Annuler » (`BR-JEU-038`).
6. **L'arrêt est toujours conclu par le moteur.** Après un ordre de sortie, l'agent demande aussitôt l'arrêt au moteur avec le délai de la fiche : le conteneur est ainsi noté « arrêté sur demande » et sa règle `unless-stopped` ne le relance pas.
7. **Console distante : déclarée, jamais publiée.** `[remote_console]` nomme le protocole (énumération : `source_rcon`, `telnet_line`), le port et le secret. Un port de ce rôle a `publish = "never"`, sans exception. Son mot de passe est un secret **généré par l'agent** à l'installation (32 caractères), scellé, écrit dans le canal où le jeu le lit, jamais affiché ni révélable. Les réglages qui l'activent sont imposés.
8. **Hearth ne se connecte pas à la console dans ce format.** Les images qui s'en servent pour s'arrêter proprement le font de l'intérieur ; le signal suffit. La déclaration est posée maintenant pour que le format ne soit pas rouvert quand la console de Hearth arrivera : elle se connectera à l'adresse du conteneur sur le réseau du moteur, sans publication de port, avec ce secret, et un genre d'étape `console_command` à texte de fiche s'ajoutera.
9. **Aucune commande exécutée dans un conteneur.** Le port du moteur n'a pas de fonction pour cela.

## Comment l'appliquer
- Chargeur : un réglage du canal `args` est `int`, `bool`, `choice` ou `text` de jeu de caractères `arg_safe` ; jamais `secret`.
- Test par fiche au banc d'essai : la valeur la plus hostile permise par l'alphabet est passée à la vraie image, qui démarre.
- Job `engine` : `no_auto_restart` (un conteneur arrêté par la séquence reste arrêté), `send_stdin_line` vers un conteneur qui recopie son entrée.
- Fiche piégée : console publiée, mot de passe de console non généré, ordre avec retour à la ligne, étape de plus de 300 secondes.

## Mise en œuvre (HRT-113)
- Alphabet fermé d'une valeur d'argument : `loading/value.rs::first_refused_char` avec `Charset::ArgSafe` (lettres ASCII, chiffres, `.`, `-`, `_`, `:` ; 64 caractères ; ni `-` ni `+` en tête). Le refus cite le caractère fautif, jamais la valeur (`Refusal.character`, `Motif::ArgumentHorsAlphabet`).
- Jetons séparés : `translate.rs` remplit `ServiceSpecDraft.command` (un drapeau, puis sa valeur) ; aucune chaîne de commande n'existe.
- Console : `[remote_console]`, port de rôle `remote_console` en `publish = "never"`, secret généré par l'appelant et vérifié (24 à 64 caractères alphanumériques).

## Quand NE PAS l'appliquer / limites
- Un texte libre (nom de serveur avec des espaces) ne passe pas par un argument : par une variable ou un fichier, ou il est `not_exposed`.
- Un arrêt qui ne passe pas par Hearth (redémarrage du serveur) n'envoie aucun ordre (`JEU-010`).
- Un service adopté ne reçoit rien (`JEU-001`).
- Si un jeu exige un terminal pour lire son entrée (non vérifié pour Terraria), le plan fermé devra porter un booléen de plus, sans privilège : à décider alors.

## Alternatives rejetées
- **Champ « arguments supplémentaires » saisi par l'utilisateur** : c'est la commande libre.
- **Laisser hors fiche les réglages qui n'existent qu'en argument** : contredit « le plus complet possible ».
- **Échapper les valeurs au lieu de borner l'alphabet** : dépend de la façon dont chaque image traite ses arguments.
- **Client RCON dans l'agent dès maintenant** : un protocole par famille de jeux à durcir, sans gain pour les quatre premiers jeux.
- **Lancer l'outil de console de l'image dans le conteneur** : exécution de commande.
- **Publier la console sur la boucle locale du serveur** : lisible de tout processus du serveur.
- **Attendre la fin du conteneur après l'ordre de sortie** (version 1) : relance par la règle de redémarrage.

## Conséquences
- `BR-JEU-002` : une suite d'étapes au lieu d'un ordre et d'un signe. `JEU-034` : l'arrêt du moteur suit aussitôt l'ordre de sortie.
- `BR-JEU-039` devient le cas particulier d'un mécanisme général (secret généré, console déclarée).
- L'essai des ordres de Terraria se fait au banc d'essai en CI (ADR-0053), plus seulement sur forge.

## Références
- `2026-10-13-technique-fiches-de-jeu.md`, sections 6.2, 6.3 et 11.3.
- Comportement du moteur sur un arrêt demandé pendant que le processus sort : de mémoire, à prouver par le job `engine`.
