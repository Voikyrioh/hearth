---
id: ADR-0051
titre: Réglages d'un jeu : le fichier du jeu est la vérité de ses octets, la description du service celle des valeurs voulues ; Hearth retouche le fichier sans le réécrire et ne l'écrit que sur un changement demandé
type: architecture
statut: proposée (non validée par Voiky ; posée avec HRT-113)
date: 2026-10-13
portee: repo
remplace: le point 5 de la version 1 de l'ADR-0050 (fichier produit rendu avant chaque démarrage)
liens: [ADR-0031, ADR-0044 (brouillon), ADR-0045 (brouillon), ADR-0046 (brouillon), ADR-0050, conception technique 2026-10-13 (fiches de jeu) sections 8, 9, 11.1 et 11.2, FICHE-041 à FICHE-053, JEU-035, JEU-036, BR-JEU-039, EDIT-035, EDIT-037, FILES-018]
---

> Posée par HRT-113 pour ses parties « formats » : les points 2 et 3 (retouche sans perte, formats écrits à la main), la forme de l'écart (point 5) et les limites. Les points 1, 4, 6, 7 et 8 (deux vérités en base, quatre moments d'écriture, entrée dans le plan, acte `GameSettingsSave`, scellement) sont posés par HRT-108 et ne sont pas codés ici : ce ticket ne fait aucune E/S, aucune route, aucune migration. Les identifiants de règles écrits sans préfixe `BR` (par exemple `FICHE-041`) sont des règles de la spec du hub (`contexts/hearth/`), sans fiche dans ce dépôt tant que leur ticket n'est pas livré.

# ADR-0051 : Réglages d'un jeu

## Contexte
Une fiche connaît tous les réglages utiles d'un jeu, mais pas tout ce qu'un fichier de réglages peut contenir : commentaires, ordre, lignes ajoutées par le jeu, par un mod ou à la main. `BR-FICHE-047` exige qu'un réglage inconnu ne soit jamais perdu ; `BR-FICHE-048` que le fichier ne soit écrit que quand l'utilisateur change un réglage. Un rendu complet du fichier depuis la description, comme en version 1, ne tient ni l'un ni l'autre.

## Décision
1. **Deux vérités, sans recouvrement.** Le fichier du jeu fait foi pour ses octets. La description du service (tables `game_services`, `service_spec_secrets`) fait foi pour les valeurs que le compte veut. Un écart entre les deux est montré, jamais corrigé seul.
2. **Retouche, pas rendu.** Chaque format de fichier (`kv_lines`, `xml_properties`) découpe le fichier en morceaux dont la concaténation redonne les octets d'origine. Régler une valeur ne change que les octets de cette valeur. Trois propriétés testées : identité (`render(parse(o)) == o`), localité, refus plutôt que perte (un fichier que le format ne sait pas découper est refusé entier, jamais réécrit).
3. **Formats écrits à la main**, sans bibliothèque XML ni INI : un balayage qui ne reconnaît que la forme de ligne utile et laisse le reste en octets bruts. Pas de `DOCTYPE`, pas d'entité hors des cinq prédéfinies, UTF-8 seulement.
4. **Quatre moments d'écriture, et aucun autre** : création à l'installation ; enregistrement de réglages (formulaire, import, remise aux valeurs proposées, choix « Remettre la mienne ») ; recréation au démarrage quand le fichier a disparu, dite à l'écran ; changement du réglage de port d'un jeu lié par `same_number`, dans l'opération de modification. Jamais à un démarrage ordinaire, un arrêt, une sauvegarde, l'ouverture de l'onglet, une mise à jour de l'agent ou de la fiche.
5. **Écart.** À chaque lecture et avant chaque écriture, le fichier est comparé à la description. Réglage connu différent : le compte choisit, « Garder la valeur du fichier » par défaut. Réglage inconnu : le fichier gagne toujours. Réglage imposé défait : repère `imposed_broken`, aucune écriture.
6. **Ce qui entre dans le plan.** Un réglage des canaux `env` et `args` fait partie de la `ServiceSpec` : le changer crée une révision et demande une recréation. Un réglage d'un canal `file` n'entre pas dans la partie couverte par l'étiquette `spec-mac` : le changer ne fait pas apparaître « Modifié hors de Hearth ».
7. **Un acte**, `GameSettingsSave` (`0x1E`), couvert par l'élévation. Il signe la révision de base, le nombre de changements, l'empreinte des identifiants changés, l'origine et l'effet. Jamais une valeur.
8. **Secrets.** Réglages `secret` et secrets générés scellés (ADR-0044). Toutes les valeurs de réglages inconnus sont scellées : Hearth ne sait pas ce qu'elles contiennent.

## Comment l'appliquer
- Interface `ConfigFormat { parse, entries, set, unset, render, create }` dans `domain/catalog/formats/` ; une suite de contrat unique passée par chaque format.
- Lecture par `SafeFs::read_text` (2 Mio au plus), écriture par `SafeFs::write_atomic` avec le repère attendu ; l'ancien contenu devient la version précédente du fichier (`FILES-018`).
- Un enregistrement du fichier brut par l'onglet Fichiers repasse par le format : illisible, il est refusé ; lisible, l'état est mis à jour.
- Test de non-régression de `BR-FICHE-047` : un réglage inventé dans un fichier ressort identique après enregistrement, remise aux valeurs proposées, import, restauration, mise à jour de la fiche.

## Mise en œuvre (HRT-113)
- `domain/catalog/formats/` : `ConfigFormat` (`parse`, `set`, `unset`, `create`), `Doc` (`render`, `entries`, `get`, `duplicate_keys`), `kv_lines.rs`, `xml_properties.rs`, et `contract.rs`, la suite de contrat unique passée par les deux formats (identité, localité, refus plutôt que perte, doublons, fins de ligne mêlées, BOM, fichier sans fin de ligne, valeur vide, 20 000 lignes, valeurs qui tentent d'injecter une ligne ou une propriété). Générateur à graine fixe, cas bornés.
- `domain/catalog/settings_state.rs` : les trois états d'un réglage, l'effet déduit du canal, l'écart (`drift`) et les réglages imposés défaits (`imposed_broken`).

## Quand NE PAS l'appliquer / limites
- Un canal dont l'image réécrit le fichier à chaque démarrage d'après ses variables (`raw_edit = false`) : la vérité est la variable ; le fichier n'est ni retouché ni proposé en édition brute.
- Une clé en double : Hearth lit et retouche la dernière, et le signale. Le comportement réel du jeu devant un doublon n'est pas vérifié.
- Hearth n'applique aucun réglage à chaud : tout passe par un redémarrage ou une recréation (`JEU-036`).

## Alternatives rejetées
- **Rendu complet depuis la description** : perd commentaires, ordre et lignes inconnues ; écrase ce que le jeu a écrit.
- **Bibliothèque XML ou INI avec arbre** : réécrit le fichier à sa façon ; surface d'analyse inutile.
- **Le fichier seul comme vérité, sans description** : impossible de recréer le service ou de garder un secret scellé.
- **Réécrire à chaque démarrage** (une installation de référence le fait pour 7 Days to Die) : annule toute modification du jeu ou d'une main.
- **Deux actes, enregistrer et importer** : même effet ; l'origine est un paramètre signé.

## Conséquences
- Specs à corriger : `BR-FICHE-046` (les commentaires sont conservés ; le fichier n'est pas un rendu), `FICHE-042` (quatrième mention pour les réglages reçus par variable ou argument), `FICHE-030` et `EDIT-037` (pas de recréation pour un réglage de fichier), `FICHE-052` (le texte collé est lu par l'agent).
- L'opération de modification gagne l'étape `write_channel_file`, avec son inverse.

## Références
- `2026-10-13-technique-fiches-de-jeu.md`, sections 11.1 et 11.2.
- Configuration d'une installation de référence de 7 Days to Die (recopie du fichier avant chaque démarrage : le comportement que Hearth ne reprend pas).
