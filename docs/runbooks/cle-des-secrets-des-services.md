# La clé des secrets des services (`services.key`)

Cas : savoir où l'agent garde la clé qui scelle les secrets des services qu'il crée (valeurs et noms de variables, commande, mot de passe d'un serveur de jeu), la sauvegarder AVEC la base, la restaurer, ou comprendre le message « les secrets des services sont illisibles ». Décisions : ADR-0044 (description durable) et ADR-0045 (journal des opérations) ; règle : BR-EDIT-034.

## Où elle est

`services.key` dans le dossier de données de l'agent (`/var/lib/hearth` par défaut), à côté de `hearth.db`.

- 32 octets aléatoires, droits `0600`, propriétaire = l'utilisateur qui fait tourner le service (root pour le service installé par `hearth-agent install`) ; **l'agent le vérifie** (utilisateur effectif) et refuse un autre propriétaire.
- Doit être un **fichier ordinaire** : l'agent n'ouvre jamais un lien symbolique (même vers une bonne clé) et refuse un dossier ou un tube nommé, sans les lire. Les contrôles (genre, propriétaire, droits, taille) portent sur le fichier ouvert, pas sur le chemin.
- Un système de fichiers qui ne gère pas les droits Unix ou les liens durs (montage qui montre tout en `0777`, certains partages réseau) rend la clé « trop ouverte » ou impossible à créer : les secrets sont refusés, rien n'est détruit. Garde `/var/lib/hearth` sur un système de fichiers Linux ordinaire.
- À la création, le fichier PUIS le dossier sont synchronisés avant que la clé serve : une coupure de courant ne laisse pas une base qui dépend d'une clé dont l'entrée de dossier manque.
- **Créée à la première nécessité** (le premier secret à sceller), pas à l'installation ni au démarrage : un serveur qui n'a encore créé aucun service avec un secret n'a pas ce fichier, et c'est normal.
- **Jamais dans la base, ni dans le journal, ni dans une trace.** La base ne garde que des secrets scellés (avec, pour chaque description, un manifeste scellé qui liste ses champs) et une vérification de la clé (un code d'authentification qui ne révèle rien d'elle).
- Elle n'est pas dans la copie de la base que prend la mise à jour de l'agent (`update/hearth.db.before`). Un retour arrière de la base la laisse en place ; si la base revenue n'a rien scellé, l'agent la garde et retient sa vérification.

## Ce que l'agent refuse de faire quand la clé ne sert pas

En plus de refuser d'écrire une description avec secrets, l'agent refuse d'écrire une révision SANS secret quand la révision courante en garde (sinon ils disparaîtraient en silence), et de purger les anciennes révisions tant que la clé n'ouvre pas ce qu'elles portent. Clé remise : tout revient. Une description dont un scellé a été retiré, ajouté ou permuté en base est signalée « altérée » (le manifeste scellé ne correspond plus), jamais rendue tronquée. L'agent déjà prêt garde la clé en mémoire : il ne voit un fichier remplacé qu'à son redémarrage.

## Ce que le scellement protège, et ce qu'il ne protège pas

Protégé : une copie de la base ou une sauvegarde de la base (fichier `hearth.db`, son journal `-wal`) ne contient aucun secret en clair.

Pas protégé : quelqu'un qui est déjà `root` sur le serveur, ou qui copie TOUT le dossier de données (la clé est à côté). Le moteur de conteneurs garde d'ailleurs les mêmes valeurs en clair dans son propre dossier.

## Sauvegarder et restaurer

**Sauvegarde la clé avec la base, et séparément si possible.** Une base sans sa clé redonne des services dont les secrets sont illisibles ; une base et sa clé rangées au même endroit n'ont plus l'effet voulu (elles se volent ensemble). Un gestionnaire de mots de passe, un autre disque ou un autre compte conviennent.

Restaurer : remets `hearth.db` ET le `services.key` d'origine dans le dossier de données (droits `0600`, propriétaire du service), puis redémarre : `systemctl restart hearth-agent`. Si l'agent tournait déjà, il reprend les secrets tout seul dès que le bon fichier est là, sans redémarrage.

## Quand la clé manque, est fausse ou inutilisable

L'agent **démarre quand même**. Les services qui tournent ne sont pas touchés. Il écrit dans son journal (`journalctl -u hearth-agent`) une ligne « les secrets des services sont illisibles » avec la raison et le remède. **Rien n'est supprimé et aucune clé n'est jamais créée par-dessus** : seule la fonction qui a besoin des secrets (écrire ou relire une description avec ses secrets) est refusée.

| Raison dans le journal | Cause | Remède |
|---|---|---|
| `…services.key) manque, alors que la base en garde des secrets scellés` | Base restaurée sans sa clé, ou fichier supprimé | Remets le fichier d'origine. Ne le recrée pas : une clé neuve ne rouvre rien |
| `…ne correspond pas à cette base` | Base restaurée avec la clé d'une autre installation, ou fichier remplacé | Remets la clé qui allait avec CETTE base |
| `…est ouvert aux autres utilisateurs (droits 644)` | Droits élargis après coup (`cp` sans `-p`, restauration) | `chmod 600 /var/lib/hearth/services.key` |
| `…fait N octets au lieu de 32` | Fichier tronqué ou édité à la main | Remets l'original |
| `…est illisible` | Droits du système, disque, système de fichiers | Remets un fichier ordinaire lisible par l'agent |
| `…n'est pas un fichier ordinaire (lien symbolique, dossier, tube ou périphérique)` | Lien, dossier ou tube posé à la place (le lien n'est jamais suivi) | Remets le fichier ordinaire d'origine à cet emplacement |
| `…n'appartient pas à l'utilisateur de l'agent` | Restauration par un autre compte, `cp` en root pour un agent non root | `chown` au compte qui fait tourner l'agent |

Si la clé est définitivement perdue : les secrets scellés avec elle sont perdus (aucune porte dérobée). Les services eux-mêmes tournent encore ; les recréer demande de ressaisir les valeurs. Il n'existe pas encore de commande qui réinitialise les secrets : c'est un acte volontaire, qui viendra avec le ticket des modifications.

## Rotation

Non prévue pour l'instant. Ne remplace pas le fichier à la main : les secrets déjà scellés ne s'ouvriraient plus.

## Désinstaller

`hearth-agent uninstall --purge` efface le fichier (et ses temporaires `services.key.<pid>.<n>.tmp`). `--keep-data` le garde avec la base.
