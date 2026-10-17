---
id: ADR-0050
titre: Fiches de serveur de jeu, format version 2 : deux fichiers TOML déclaratifs et fermés par jeu (conteneur d'un côté, catalogue de réglages de l'autre), embarqués dans l'agent, traduits en description de service ; tout élément incertain porte un statut de preuve
type: architecture
statut: proposée (non validée par Voiky ; posée avec HRT-113)
date: 2026-10-13
portee: repo
remplace: la version 1 de ce brouillon ; écarte toujours, pour ce lot, le catalogue distant signé de la feuille de route section 4
liens: [ADR-0008, ADR-0014, ADR-0036, ADR-0041, ADR-0044 (brouillon), ADR-0045 (brouillon), ADR-0046 (brouillon), ADR-0051, ADR-0052, ADR-0053 (brouillon), ADR-0054 (brouillon), conception technique 2026-10-13 (fiches de jeu) sections 5 et 6, BR-FICHE-005 à BR-FICHE-011, BR-FICHE-037 à FICHE-051, BR-JEU-002, BR-JEU-038, BR-JEU-039, MONDE-003, GRANT-018]
---

> Posée par HRT-113 (format version 2, domaine pur, agent). Réalisé ici : le schéma des deux fichiers, le chargeur, les neuf invariants, la traduction vers un `ServiceSpecDraft` local, `build.rs` (une fiche refusée fait échouer la compilation), `cargo xtask catalog-check` et la fiche Terraria en `preview`. La voie unique vers `ContainerPlan` (point 7) et le test « le plan est celui que le validateur du déploiement ordinaire construit » sont à HRT-101 : ce validateur n'existe pas encore dans `main`, le test est écrit et ignoré (interface `translate::DeploymentValidator`). Numéros : ADR-0053 et ADR-0054 sont des brouillons pas encore posés. Les identifiants de règles écrits sans préfixe `BR` (par exemple `FICHE-041`) sont des règles de la spec du hub (`contexts/hearth/`), sans fiche dans ce dépôt tant que leur ticket n'est pas livré.

# ADR-0050 : Fiches de serveur de jeu, format version 2

## Contexte
La version 1 décrivait une fiche à quelques questions, dont les fichiers de configuration étaient rendus en entier par Hearth avant chaque démarrage. Trois apports la rendent fausse. Voiky veut la configuration « le plus complet possible » et les jeux sans serveur pilotable livrés « tel quel avec le max de feat ». L'inventaire des réglages compte 36 réglages pour Terraria dont 5 en argument de commande, 58 actuels et 30 retirés par la version 3.0 pour 7 Days to Die, et relève que le fichier ne doit être écrit que sur un changement de l'utilisateur. L'inventaire des jeux demande : contrat à accepter, jeton de serveur, téléchargements de 8 à 60 Go, variantes, mods, console distante, délais d'arrêt de 5 minutes.

## Décision
1. **Deux fichiers par jeu**, dans `crates/hearth-agent/catalog/<id>/`, embarqués : `fiche.toml` (image, variantes, dossiers, ports, canaux, variables, secrets, console distante, contrat, premier démarrage, prêt, version du jeu, arrêt, ressources, mondes) et `settings.toml` (thèmes et catalogue de réglages). `format = 2`. Tout champ inconnu est refusé. L'empreinte de la fiche couvre les deux.
2. **Catalogue de réglages.** Chaque réglage : identifiant Hearth stable, nom pour le jeu, canal, thème, niveau (`essential` = posé à l'installation, huit au plus ; `common` ; `advanced`), libellé et aide en mots simples, type fermé (`text`, `secret`, `int`, `bool`, `choice`, `id_list`, `opaque`) avec ses bornes, défaut du jeu, valeur proposée, modifiable ou figé à la création du monde, imposé ou non avec sa raison, versions du jeu où il existe, source (`verified`, `partial`, `name_only`). Les réglages que le jeu n'a plus sont gardés en `[[retired]]` ; ceux que la fiche ne règle pas sont en `[[not_exposed]]` avec leur raison.
3. **Canaux.** Un réglage est reçu par `env`, `args` ou `file` (format `kv_lines` ou `xml_properties`). L'effet d'un changement se déduit du canal : redémarrage pour `file`, recréation pour `env` et `args`. La fiche ne le déclare pas.
4. **« Non écrit » est une valeur.** Sans valeur proposée, Hearth n'écrit pas le réglage et le jeu prend son défaut. Un défaut non vérifié n'est jamais inventé.
5. **Statut de preuve.** Séquence d'arrêt, séquence d'enregistrement, signe « prêt », lecture de la version : chacun porte `status = "tested" | "untested"`. **Un élément `untested` n'est jamais exécuté ni cru** ; l'écran montre le texte de remplacement. Les ressources portent `evidence = "measured" | "measured_idle" | "publisher" | "unmeasured"` ; un nombre accompagné de `unmeasured` est refusé.
6. **Fermé par construction.** Aucun champ pour : un chemin du serveur, une commande ou un argument saisi par l'utilisateur, un réseau, une capacité, un périphérique, un autre conteneur, un identifiant de compte, une adresse que Hearth contacterait, un script, une expression régulière, un gabarit de texte. Les signes de journal sont des sous-chaînes littérales ; la version du jeu est le texte entre deux littéraux ; une valeur dérivée est un préfixe, un réglage, un suffixe.
7. **Une seule voie.** Fiche, variante, réglages, secrets et contrats acceptés donnent une `ServiceSpec`, puis le constructeur unique de `ContainerPlan` (ADR-0041). Le plan gagne pour tout déploiement, sans privilège : image par empreinte, entrée standard ouverte, délai et signal d'arrêt (énumération), dossier géré en lecture seule, propriétaire numérique. **Plus aucun montage d'un fichier seul** : le dossier est monté.
8. **Ports.** Chacun a un rôle, une publication (`always`, `optional`, `never`) et un lien au réglage de port du jeu : `nat` (port du conteneur fixe) ou `same_number` (le jeu annonce son port : conteneur, serveur et réglage valent le même nombre).
9. **Variantes.** Une question figée après l'installation, qui ne change que l'étiquette de l'image (même dépôt), des variables fixes et quels éléments s'appliquent. Au-delà, c'est une autre fiche de la même `family`.
10. **Premier démarrage lourd.** Poids déclaré avec sa preuve, espace vérifié avant, repères d'étape littéraux, octets réellement écrits affichés, aucun pourcentage calculé, délai « prêt » jusqu'à 6 heures. Dépasser ce délai n'arrête ni ne relance rien.
11. **Neuf invariants vérifiables** (conception, section 6), une fiche piégée par invariant, et un test de propriété : le plan d'une fiche est toujours un plan que le déploiement ordinaire accepte.
12. **Origine** : inchangée. Embarquées, couvertes par la signature de l'agent, sans signature propre ; empreinte affichée, inscrite dans la description et signée dans l'acte de déploiement.

## Comment l'appliquer
- `domain/catalog/` est du Rust pur : schéma, chargeur, traduction, formats. Aucune E/S.
- `build.rs` de l'agent engendre la table des fiches et échoue si le chargeur réel en écarte une : une fiche fausse casse la compilation.
- Exemple complet : la fiche Terraria de la conception, section 5.4 (36 réglages classés, aucun passé sous silence).
- Bornes : 256 Kio, 400 réglages, 16 ports, 8 dossiers, 6 canaux, 6 étapes d'arrêt.

## Mise en œuvre (HRT-113)
- `crates/hearth-agent/src/domain/catalog/loading/` : schéma (`deny_unknown_fields` à tous les niveaux), chargeur, invariants I1 à I9, `ids.lock`. Dossier autonome (`super::` seulement), inclus par `#[path]` dans `build.rs` et dans `xtask` : le chargeur qui refuse une fiche à la compilation est celui de l'agent.
- Dépendances : aucune de plus dans l'arbre. `toml`, `serde` et `sha2` (déjà dépendances de l'agent) deviennent aussi `build-dependencies` de l'agent et dépendances de `xtask`. Pas de bibliothèque d'expressions régulières, de gabarit, XML ou INI (garde de test).
- L'empreinte est le SHA-256 de `u32(longueur) || fiche.toml || u32(longueur) || settings.toml` (longueurs gros-boutistes).
- `rerun-if-changed=catalog` : `build.rs` ne se relance que si le dossier du catalogue change.

## Évolution : mémoire recommandée et crans de mémoire (HRT-122)

Rien n'est retiré du format. Deux blocs facultatifs s'ajoutent sous `[resources]` : `[[resources.memory_tier]]` (zéro à trois entrées : `id` parmi `smooth`, `ultra`, `community` ; `from_bytes` ; `evidence`) et `[resources.tiers] hidden = [...]` (crans sans sens pour ce jeu, parmi les six identifiants `unstable`, `slow`, `recommended`, `smooth`, `ultra`, `community`). « Instable », « Lent » et « Recommandé » ne s'écrivent pas : ils se déduisent de `min_bytes` et `recommended_bytes` (`BR-RES-032`). Le champ `evidence` d'un cran reprend la forme de HRT-113 telle quelle : `"measured"` ou `{ publisher = "https://..." }` (l'adresse est dans la preuve, il n'y a donc pas de champ `url` séparé) ; `measured_idle` et `unmeasured` sont refusés (`CranPreuveInterdite`). Aucun champ de texte : les libellés et les textes sont ceux de Hearth. Refus typés : `CranInconnu`, `CranEnDouble`, `CranNonDeclarable`, `CranBorneInvalide` (non croissants, pas au-dessus de la mémoire recommandée, ou pas de mémoire recommandée), `CranPreuveInterdite`, `AdresseNonHttps` (I5).

Règle de publication (`BR-RES-038`) : une fiche `published` porte `memory.recommended_bytes` avec une preuve `measured` ou `publisher`. Le chargeur la refuse sinon (`PublieeSansMemoireRecommandee`) ; `cargo xtask catalog-check --release` dit en plus ce qui manque à une fiche `preview` (mémoire recommandée absente ou mesurée au repos seulement). Une mesure au repos est un plancher, jamais multipliée en recommandation. Terraria reste `preview` tant que HRT-149 n'a pas relevé son chiffre.

Mise en œuvre : `loading/memory.rs` (`memory_tiers`, `recommended_gap`), `invariants.rs::check_memory_tiers`. Premier usage servi : « Recommandé » ; les autres crans sont servis dès que la fiche leur donne un chiffre fondé. Servir les crans dans la réponse du catalogue et du service installé reste à faire (tranche suivante, hors domaine pur).

## Quand NE PAS l'appliquer / limites
- Le format borne ce que Hearth demande au moteur, pas ce que l'image fait une fois lancée ni ce qu'elle télécharge.
- Un service adopté n'a ni fiche ni réglages (`FICHE-054`).
- Un jeu qui exigerait le réseau de l'hôte n'est pas exprimable : une ADR par jeu et une valeur d'énumération fermée, jamais un champ libre.
- Les réglages reçus par variable n'ont pas de fichier de référence : leur source reste `partial` tant que le banc d'essai ne les a pas vus prendre effet (ADR-0053).

## Alternatives rejetées
- **Un seul fichier par fiche** : la surface de sécurité se noie dans cent réglages.
- **Fiche en Rust** : illisible pour qui l'écrit, et impossible à recevoir d'un catalogue distant.
- **Reprendre les scripts de Pelican ou de LinuxGSM** : ils expriment plus qu'un déploiement.
- **Rendre le fichier de configuration en entier** (version 1) : écrase ce que la fiche ne connaît pas (ADR-0051).
- **Champ « arguments supplémentaires » libre** : c'est la commande libre (ADR-0052).
- **Déclarer l'effet d'un réglage dans la fiche** : elle pourrait mentir ; il se déduit du canal.
- **Attendre l'essai pour écrire la fiche** (version 1) : contredit « tel quel avec le max de feat » ; le statut de preuve fait mieux.
- **Pourcentage de téléchargement** : aucune source vraie ; les octets écrits en sont une.

## Conséquences
- Capacités `catalog-2` et `game-settings`. Migration `0012`.
- Les sections 9.6 à 9.9 de la conception des lots suivants sont remplacées.
- Specs à corriger : `BR-FICHE-008`, `-011`, `-021`, `-024`, `-030`, `-042`, `-046`, `-052` ; `JEU-034` ; `MONDE-030` (conception, « Écarts »).
- Un ticket de plus, sans dépendance, pour le format dans le domaine (HRT-113).

## Références
- `2026-10-13-technique-fiches-de-jeu.md`, sections 5 et 6.
- Configurations d'installations de référence de Terraria et de 7 Days to Die (lecture seule).
- Sources des deux inventaires du 2026-10-13, avec leurs réserves.

## Historique

### Version 1 (2026-10-13, matin), remplacée

Titre : « Fiches de serveur de jeu : format TOML déclaratif et fermé, embarqué dans l'agent et couvert par sa signature, traduit en description de service ; l'ordre d'enregistrement du monde passe par l'entrée standard ou le signal, rien d'autre ».

Décision de la version 1, point par point, et son devenir :

| # | Version 1 | Devenir |
|---|---|---|
| 1 | Un fichier TOML par fiche, `format = 1`, sections identité, image, ports, dossiers, propriétaire, variables, fichiers produits, questions, ressources, arrêt, prêt, premier démarrage, mondes, réglages non repris, réglages imposés | Remplacé : deux fichiers, `format = 2`, catalogue de réglages et canaux |
| 2 | Fermé par construction ; fichier produit rendu par `ini_lines` ou `xml_properties` | Gardé pour la fermeture ; les formats deviennent `kv_lines` et `xml_properties`, lus et retouchés sans perte (ADR-0051) |
| 3 | Une seule voie vers `ContainerPlan` ; le plan gagne empreinte, `stdin_open`, `stop_grace_s`, `generated_files`, `owner` | Gardé ; `generated_files` (fichier monté seul) est retiré, le signal d'arrêt et le dossier en lecture seule sont ajoutés |
| 4 | Embarquées, pas de signature propre, empreinte affichée, `cargo xtask catalog-check` | Gardé ; le contrôle remonte à la compilation (`build.rs`) et s'étend (ADR-0053) |
| 5 | Fichiers produits `mount_ro` ou `in_folder`, **écrits avant chaque démarrage demandé par Hearth** | **Remplacé** : écrits à la création, puis seulement sur un enregistrement de réglages (ADR-0051, `BR-FICHE-048`) |
| 6 | `saves_on_stop`, un ordre d'enregistrement et un ordre de sortie par l'entrée standard, un délai | Remplacé par une suite bornée d'étapes avec statut de preuve (ADR-0052) |
| 7 | Terraria : `save` puis `exit` ; 7 Days to Die : signal seul, 120 s | Gardé comme contenu des deux fiches ; la séquence de Terraria est `untested` |
| 8 | Reprise d'un monde par copie vérifiée | Gardé, étendu aux sauvegardes existantes (conception, section 13) |

« Comment l'appliquer » de la version 1 disait : « Une fiche n'est publiée que pour un jeu essayé, avec ses ressources mesurées ». Remplacé par la règle de publication de l'ADR-0053 et la question 2 de la conception. Aujourd'hui (HRT-113) toute fiche est embarquée dans le binaire, `preview` comprise : la publication de l'agent n'est pas bloquée par une fiche en cours de preuve, et `catalog-check --release` n'est branché sur aucune publication. Ce qui est vrai : une fiche `preview` n'est jamais proposée, `embedded::offered()` (seule porte du catalogue vers l'application) ne rend que les fiches `published`. Le type de la traduction n'impose pas encore ce passage (suivi : `ValidatedSpec`).

Limites de la version 1 : « Aucune commande exécutée dans le conteneur, aucun RCON » : gardé (ADR-0052). « Les ordres et lignes de journal de Terraria ne sont pas vérifiés : la fiche n'est pas publiée avant un essai sur une copie » : remplacé (statut `untested`, décision A6 de Voiky).

Alternatives rejetées par la version 1, toujours rejetées : signature propre d'une fiche embarquée ; catalogue distant dès ce lot ; fiche en Compose ou en script ; gabarit de texte libre ; exécuter une commande dans le conteneur ; RCON piloté par Hearth.

Référence de la version 1 : `2026-10-13-technique-deploiements-lots-suivants.md`, sections 9.6 à 9.9.
