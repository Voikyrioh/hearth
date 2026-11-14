---
id: ADR-0059
titre: Reprendre un monde existant par copie vérifiée : source en lecture seule par le type, dossier d'import nommé au journal, mise en place par renommage, jamais par-dessus un contenu
type: securite
statut: acceptée
date: 2026-11-14
portee: projet
remplace: — (complète l'ADR-0046 pour la lecture d'un dossier ouvert, l'ADR-0045 pour le plan et la reprise, l'ADR-0050 pour la reprise d'un monde)
liens: [ADR-0045, ADR-0046, ADR-0050, ADR-0055, HRT-105, BR-MONDE-002 à BR-MONDE-016, BR-MONDE-023, BR-MONDE-024, BR-MONDE-026, BR-MONDE-039, SECURITY.md]
---

# ADR-0059 : Reprendre un monde par copie vérifiée

## Contexte

Le propriétaire a déjà des serveurs de jeu faits à la main (Terraria, 7 Days to Die). La bascule vers Hearth passe par l'installation d'une fiche **en reprenant le monde existant**. L'agent est root ; la source est un dossier du serveur que l'administrateur a ouvert aux services ; le monde peut peser des dizaines de Go et être en cours d'écriture par l'ancien service. Il faut qu'une reprise, réussie ou non, laisse la source exactement comme elle l'était.

## Décision

1. **La source est lue par un type qui ne sait pas écrire.** Le port `WorldSource` (`list`, `stat`, `open`) n'a aucune méthode d'écriture : le refus d'écrire est une propriété du type (BR-MONDE-004), pas une promesse de l'adaptateur. L'adaptateur Unix ouvre le dossier de la configuration en lecture seule puis descend par `openat(O_DIRECTORY | O_NOFOLLOW)` sur le périphérique de la racine ; un fichier est désigné (`O_PATH`) et jugé AVANT d'être ouvert (jamais un tube ni un périphérique). Un dossier `ro` suffit, un dossier `rw` n'est pas écrit non plus.
2. **Dossiers ouverts : la configuration, rien d'autre.** `[services] allowed_roots` d'`agent.toml` (chemin seul = lecture seule, ou `{ path, access = "ro" | "rw" }`). Le client désigne un monde par le dossier tel qu'il est écrit et un chemin relatif (`RelPath`) ; un chemin absolu, `..`, un dossier hors liste sont refusés. Aucun écran n'ouvre un dossier (BR-MONDE-002).
3. **Ce qu'est un monde vient de la fiche** (`[worlds]`, BR-MONDE-039) ; la même définition sert la recherche (trois niveaux, 5 000 entrées par dossier, noms seulement), l'inspection et la copie. Les noms qui évoquent une sauvegarde (« backup », « sauvegarde », « .bak ») ne sont ni des mondes ni des dossiers où descendre.
4. **La copie va dans un dossier d'import neuf, jamais dans le dossier du service.** `<données>/staging/import-<op>/` (dossier d'attente de l'agent, `0700`, de l'agent seul), propriétaire et droits de la fiche posés avant la première donnée. L'objet est nommé au journal des opérations (`intent` de l'étape `world`, avec périphérique et inode) AVANT la première donnée : un redémarrage le retire après vérification de l'inode (BR-MONDE-026, ADR-0045).
5. **Chaque fichier est relu.** SHA-256 à la lecture de la source ; relevé de la taille et de la date avant et après (BR-MONDE-009) ; puis relecture de la copie et comparaison (BR-MONDE-010). Une copie qui diffère ne crée rien : le dossier d'import est retiré, pas de description, pas de conteneur.
6. **Mise en place par renommage, jamais par-dessus un contenu.** Une fois le dossier géré du monde créé et AVANT la création du conteneur, `renameat` du dossier d'import vers `services/<clé>/<dossier>`, qui doit être vide (sinon `Exists`). Même système de fichiers requis (le dossier de données de l'agent) ; sinon la reprise échoue sans rien créer.
7. **Une place pour les opérations lourdes.** L'installation avec un monde prend `heavy_io` à son acceptation (BR-MONDE-013) : refus `HEAVY_IO_BUSY`, jamais de file. La copie tourne sur un fil créé pour elle (« nice » 19), publie sa progression, s'annule (le dossier d'import seul part) et s'arrête à six heures.
8. **Le journal ne porte que des nombres et le dossier ouvert.** `WorldNote` n'a aucun champ pour un sous-chemin, un nom de monde ni un nom de fichier (BR-MONDE-023) ; `WorldRef` n'affiche rien en `Debug`. Les consultations (liste, inspection) ne sont ni journalisées ni diffusées.
9. **Le nom du monde vient du monde, le geste « Je comprends » vient du compte.** L'agent pose lui-même la réponse du réglage de nom, refuse une réponse différente et refuse sans `version_understood` (BR-MONDE-014, BR-MONDE-016). La demande entière, monde compris, est signée.

## Alternatives rejetées

- **Copier directement dans le dossier géré** : un échec ou une annulation laisserait un dossier à moitié rempli sous le service ; le renommage donne un « tout ou rien ».
- **Réutiliser `ServiceRoot` de `SafeFs` pour la destination** : il n'écrit que des tampons entiers en mémoire (`write_atomic`) ; un monde de 40 Go exige du flux. Un port distinct (`WorldSink`) évite aussi de changer un contrat que d'autres chantiers modifient.
- **Lister les voisins d'un monde pour trouver ses sauvegardes** : la règle est de ne rien lire hors du monde choisi (BR-MONDE-005).
- **Empreinte seule, sans relecture** : elle prouve ce qui a été lu, pas ce qui est écrit.
- **Reprendre une copie interrompue** : le dossier d'import est retiré, jamais repris.

## Conséquences

- Capacité `world-import` ; routes `GET /catalog/{id}/worlds` et `POST /catalog/{id}/worlds/inspect` (niveau `Services`), champ `source.world` de `POST /deployments`.
- L'étape `world` entre `pull` et `create` du plan d'un déploiement qui reprend un monde ; aucune migration (l'issue « incomplet » passe par `Cause::StepFailed` et le journal d'activité).
- Limites dites : le « nice » n'est pas une priorité d'entrée-sortie (comme l'effacement) ; le retrait du dossier d'import vérifie l'inode puis supprime par chemin (dossier d'attente à l'agent seul) ; un monde « dossier » se liste et s'inspecte mais ne s'installe pas tant qu'aucune fiche n'en déclare ; le repère « Peut être incohérente » d'une copie gardée n'est écrit qu'au journal d'activité.
- Les deux fichiers de test Unix (`infrastructure/world_fs/unix.rs`) ne tournent qu'en CI Linux.
