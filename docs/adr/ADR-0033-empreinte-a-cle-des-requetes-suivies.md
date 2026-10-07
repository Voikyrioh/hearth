---
id: ADR-0033
titre: Empreinte à clé des requêtes suivies : HMAC-SHA-256 avec un secret d'installation tenu dans un fichier, hors de la base et de sa sauvegarde
type: securite
statut: acceptée
date: 2026-10-07
portee: projet
remplace: aucune
liens: [BR-RESIL-010, BR-RESIL-021, BR-UPDATE-029, BR-INSTALL-011, ADR-0006, ADR-0009, ADR-0014, FIX-01M4BZN31A8Z8WN0WKNTCRTFFN, HRT-32]
---

# ADR-0033 : L'empreinte des requêtes suivies est un HMAC à clé

## Contexte
Une requête qui porte `Idempotency-Key` est liée à sa clé par une empreinte de la méthode, du chemin et du corps, gardée 24 heures en base (BR-RESIL-010). C'était un SHA-256 sans clé. Le corps de `POST /accounts`, de `PUT /accounts/{id}/password` et de `PUT /me/password` ne contient, pour un client actuel, que des mots de passe et des champs connus : qui lit la base (sauvegarde volée, accès au disque) pouvait les chercher hors ligne à pleine vitesse, sans le ralentissement d'Argon2. Trouvé par la review r1 de la PR #34 (HRT-28), défaut antérieur (FIX-01M4BZN31A8Z8WN0WKNTCRTFFN).

## Décision
1. **HMAC-SHA-256** (`ring::hmac`, déjà dans l'arbre : ADR-0005, ADR-0009) de `domain::operations::canonical_request(méthode, chemin, corps)`, clé par un secret propre à l'installation. Aucune nouvelle dépendance.
2. **Encodage sans ambiguïté** : chaque partie est précédée de sa longueur (8 octets grand-boutiste). Déplacer une frontière (`"a"` + `"bc"` contre `"ab"` + `"c"`) change la suite d'octets, quel que soit le contenu.
3. **Comparaison en temps constant** (`subtle`) ; une empreinte effacée n'est égale à aucune empreinte calculée.
4. **Le secret** : 32 octets du hasard du système (`getrandom`), dans `request_fingerprint.key` du dossier de données (à côté de `key.pem` et `install_id`), droits 0600 posés à l'ouverture du fichier, avant d'y écrire le moindre octet, propriétaire = l'utilisateur qui lance le service. **Jamais** en base, au journal, dans une erreur, dans la sauvegarde de la base de la mise à jour (qui ne copie que `hearth.db` et son journal). Type de domaine `FingerprintSecret` : pas de `Clone`, `Debug` masqué, mémoire effacée à la libération.
5. **Création** au premier démarrage du service (pas par `install.sh` ni par l'unité, pas par les sous-commandes `account` ni `attack-mode` : un fichier créé par root ne serait pas lisible par un service lancé sous un autre compte). Fichier voisin ouvert en 0600, synchronisé, puis posé sous son nom par un **lien dur** : le lien échoue si le nom existe, donc deux démarrages simultanés ne créent jamais deux secrets (le perdant relit celui du gagnant) et le fichier final n'existe jamais à moitié écrit. Aucun verrou.
6. **Fichier présent mais inutilisable** (mauvaise taille, droits au-delà de 0600, illisible) : **l'agent refuse de démarrer** avec un message qui nomme le fichier et le remède. Jamais régénéré par-dessus. Fichier absent : créé.
7. **Anciennes empreintes** : la migration `0008` met `operations.request_hash` à la chaîne vide (les lignes restent). Une clé connue dont l'empreinte est vide donne `Replay::Unverifiable` : `409 CONFLICT`, **rien n'est exécuté**, quel que soit l'état de l'opération ; son résultat reste lisible par `GET /operations/{id}`.
8. **Ports** : `RequestFingerprinter` (calcule) et `FingerprintSecretStore` (charge ou crée) dans `application/ports/` ; adaptateurs `infrastructure/fingerprint.rs` (`HmacFingerprinter`, `NoFingerprint` pour les chemins sans service HTTP) et `infrastructure/fingerprint_secret.rs` ; le domaine ne sait que comparer des empreintes déjà calculées.

## Alternatives écartées
- Argon2 sur le corps : coût par requête, inutile (une fois la clé hors de portée, le corps n'a pas besoin d'être lent à deviner).
- Secret en base : la copie de la base le livrerait avec les empreintes, l'objet même du défaut.
- Dériver le secret de la clé TLS ou de `install_id` : `install_id` est public (`/hello`), la clé TLS ne doit être lue que par l'adaptateur TLS.
- Régénérer en silence un fichier illisible : les opérations en cours perdraient leur garde de rejeu sans que l'opérateur le sache.
- Supprimer les lignes d'`operations` à la migration : un rejeu serait alors **exécuté** une seconde fois (acte destructeur compris). On garde les lignes et on efface l'empreinte.

## Conséquences et limites
- **Pendant 24 heures au plus après la mise à jour** (durée de conservation des opérations), le rejeu d'une opération faite AVANT la mise à jour n'est plus reconnu comme le même acte : le client reçoit `409 CONFLICT` (« lis son état avant de relancer ») au lieu du premier résultat, et lit `GET /operations/{id}` ; il n'y a **aucune ré-exécution**. Les opérations faites après la mise à jour ne sont pas touchées.
- **Secret perdu ou changé** (fichier retiré, réinstallation avec purge, restauration d'une sauvegarde qui n'a pas le fichier, autre machine) : un nouveau est créé au démarrage suivant. Seule la reconnaissance des rejeux en cours est perdue : une clé déjà enregistrée dont l'empreinte a été calculée avec l'ancien secret ne correspond plus à rien et reçoit `422 IDEMPOTENCY_KEY_REUSED` (message : « autre requête »), sans exécution, au plus 24 heures. Comptes, sessions, journal et postes ne dépendent pas du secret.
- **Retour arrière d'une mise à jour (BR-UPDATE-029)** : le secret **ne suit pas la base** et ne bouge pas. Il n'est pas dans la base, la copie d'avant l'échange ne le contient pas, le retour arrière ne le touche pas. La base d'avant est remise avec ses anciennes empreintes, que l'ancien binaire (SHA-256 nu) sait relire ; le fichier du secret reste, inutilisé par l'ancien binaire, et resservira à la mise à jour suivante (qui purgera de nouveau par la `0008`). Rien à faire pour que le retour arrière marche avec la `0008` : c'est une migration comme les autres, rejouée par la mise à jour suivante sur la copie d'avant. L'essai réel est celui de la CI (`cargo xtask e2e-update`).
- `uninstall --purge` efface le fichier du secret et ses temporaires ; sans purge, il est conservé avec le reste du dossier de données (une réinstallation garde les opérations en cours reconnaissables).
- Un agent ancien (avant `0008`) lancé à la main sur une base déjà migrée est refusé par SQLx (« migration inconnue ») : comportement existant, inchangé.
