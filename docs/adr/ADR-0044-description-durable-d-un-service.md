---
id: ADR-0044
titre: La description durable d'un service créé par Hearth vit dans la base de l'agent, secrets scellés (ChaCha20-Poly1305, clé dans services.key) ; le moteur reste la vérité de l'état observé
type: architecture
statut: proposée (non validée par Voiky ; posée avec HRT-83, moitié agent du socle)
date: 2026-10-13
portee: repo
remplace: les points 2 (« l'agent ne réconcilie rien ») et 6 (« jamais en base ») du brouillon ADR-0039 ; amende l'ADR-0037 (« la base ne retient que ce que l'agent seul sait ») et la ligne `ring` de l'ADR-0009 (chiffrement authentifié, en plus de la vérification et du HMAC)
liens: [ADR-0006, ADR-0009, ADR-0031, ADR-0034, ADR-0037, ADR-0039 (brouillon), ADR-0041, ADR-0045, BR-EDIT-024, BR-EDIT-034, BR-EDIT-035, BR-EDIT-036, BR-DEPLOY-037, BR-DEL-004, HRT-83, HRT-68]
---

> Posée par HRT-83. Le propriétaire a accepté le principe le 2026-10-13 : « la configuration des services est gardée chiffrée sur le serveur, la clé dans un fichier à côté ». Les choix de détail ci-dessous (algorithme, données associées, comportement sans clé) sont ceux de Claude et attendent sa relecture.
> Numérotation de la migration : `0012` (la `0011` est prise par la branche des services adoptés, pas encore fusionnée ; cette migration ne dépend d'aucune table de `0011`).

# ADR-0044 : La description durable d'un service

## Contexte
Le lot 1 ne stockait rien : « le moteur est la vérité ». Modifier par recréation, revenir en arrière, régler un service arrêté, installer par fiche (réponses, questions figées), retrouver un service après une purge ou une réinstallation du moteur et détecter une modification faite hors de Hearth demandent une description du service qui ne meure pas avec son conteneur. Aucun service n'a encore été créé par Hearth (HRT-68 n'est pas développé) : rien n'est à migrer.

## Décision
1. **`ServiceSpec`, versionnée par un entier, dans les tables `service_specs` et `service_spec_secrets` de la base de l'agent.** Une ligne par RÉVISION ; la courante porte `current = 1` ; les précédentes restent lisibles jusqu'à ce que l'ancien conteneur soit supprimé (`discard_before`). Elle contient de quoi recréer le service sans le moteur. Jamais un identifiant de conteneur, un état ni une mesure.
2. **Priorité** : la description dit ce que le service doit être ; le moteur dit ce qu'il est. Un écart est calculé par le domaine pur (`domain::services::drift::compare`), affiché, jamais corrigé seul.
3. **Secrets scellés valeur par valeur** (noms ET valeurs de variables, commande, réponses secrètes d'une fiche) :
   - **Algorithme** : ChaCha20-Poly1305 (RFC 8439), `ring::aead` (déjà compilé par `rustls`, aucune dépendance de plus, aucune pile native : ADR-0009).
   - **Nonce** : 96 bits tirés du hasard du système (`getrandom`) pour CHAQUE scellement, jamais réutilisé. Ceinture : la base refuse deux nonces identiques (index unique) ; elle ne connaît que les lignes vivantes (le nonce d'une révision purgée est oublié), donc la garantie réelle est la probabilité (2^-96 par paire), pas l'index.
   - **Données associées** : étiquette de domaine, version du format, clé du service, numéro de révision, nom du champ, chacun précédé de sa longueur. Un scellé recollé dans un autre service, une autre révision ou un autre champ ne s'ouvre pas. Rien ne lie un scellé à UNE BASE, seulement à la clé : même clé, même service, même révision, même champ, il s'ouvre (cas de la restauration, assumé).
   - **Manifeste (l'ensemble des champs est authentifié).** Chaque révision qui porte au moins un secret porte aussi un champ scellé `manifest` : étiquette `hearth/spec-manifest/1`, nombre de champs, puis les noms des autres champs dans l'ordre canonique (alphabétique du nom stocké), le tout scellé comme un secret et lié au service et à la révision par les données associées. À l'ouverture, le manifeste s'ouvre d'abord et doit décrire EXACTEMENT les champs trouvés en base : un scellé retiré (dernière variable, commande, réponse, ou le manifeste lui-même), ajouté, recopié, renommé ou permuté donne `Tampered` (« description altérée »), jamais une description tronquée rendue comme valide. Format 1 (aucune ligne n'existait encore). Limite : une description SANS secret n'a pas de manifeste (elle n'a pas besoin de clé) ; retirer d'un coup TOUTES les lignes de scellés d'une révision, manifeste compris, ressemble alors à une révision sans secret, ce que seule une clé disponible à l'écriture pourrait prouver autrement.
   - **Champ** : `env:<rang>` (nom ET valeur ensemble), `command`, `answer:<question>`, `manifest`. Le nom d'un champ ne dit rien du contenu : même un nom de variable (`STEAM_PASSWORD`) n'est pas en clair en base. C'est plus strict que la spec fonctionnelle (« noms en clair »), conformément à la contrainte du ticket ; le prix est que lire les noms demande la clé.
   - **Clé** : 32 octets du hasard du système dans `services.key`, dossier de données, `0600`. Jamais dans la base, jamais dans une trace, un message d'erreur ou le fil.
4. **Cycle de la clé** (`domain::sealed::decide_key`, appliqué par `application::secret_vault`) :
   - Aucune clé n'est créée au démarrage : **elle naît à la première nécessité** (un secret à sceller). Un agent mis à jour depuis `main` démarre sans clé et sans description ; une description sans secret n'en a pas besoin.
   - Création atomique et durable : fichier voisin ouvert en `0600` avant le premier octet, synchronisé (`sync_all`), posé par lien dur (échoue si le nom existe), puis le DOSSIER est synchronisé avant de rendre la clé (aucun scellé ni vérification en base ne dépend d'une entrée de dossier qui pourrait manquer après une coupure). Jamais de remplacement d'un fichier existant.
   - Lecture méfiante (code Linux) : ouverture avec `O_NOFOLLOW | O_NONBLOCK` (un lien symbolique n'est jamais suivi, un tube nommé ne bloque pas), puis, sur le DESCRIPTEUR ouvert (`fstat`) et avant toute lecture : fichier ordinaire, propriétaire = utilisateur EFFECTIF de l'agent (2e valeur de `Uid:` de `/proc/self/status`, comme `is_privileged` ; illisible = refus), droits sans accès pour les autres, taille 32 octets. Les constantes viennent de `libc` (déjà dans l'arbre par `tokio`, déclarée pour `cfg(unix)` seulement ; aucun `unsafe`, le lint `forbid` reste tenu).
   - **Une clé n'est créée que si rien dans la base n'a jamais dépendu d'une clé.** La base garde une vérification de clé (`meta.services_key_check`, HMAC-SHA256 d'une constante sous la clé : elle ne révèle rien de la clé) écrite à la première utilisation et jamais remplacée.
   - Fichier manquant alors que la base garde des scellés ou une vérification ; fichier illisible, de mauvaise taille, à droits ouverts, qui n'est pas un fichier ordinaire (lien symbolique, dossier, tube), d'un autre propriétaire ; fichier qui ne correspond pas à la vérification de la base : **l'agent démarre, dit clairement que les secrets des services sont illisibles, ne supprime rien, ne régénère PAS de clé, ne réécrit pas le fichier**. Seule la fonction qui avait besoin des secrets est refusée ; la description (sans ses secrets) reste lisible ; les services qui tournent ne sont pas touchés. Dès que l'opérateur remet le bon fichier, la fonction revient sans redémarrer l'agent.
   - **Aucune perte en silence** : écrire une révision SANS secret alors que la courante en garde est refusé (`SecretsUnreadable`) tant que la clé ne sert pas ; `discard_before` n'efface rien tant que la clé n'ouvre pas en entier ce qu'il va effacer (`SecretsUnreadable`, ou `Tampered` si la révision est altérée) ; la révision courante n'est jamais purgée. Retirer VOLONTAIREMENT les secrets, clé en main, reste permis, et `forget` (suppression du service demandée par le compte) reste libre. La garde est dans le cas d'usage (la condition est portée par une révision immuable, et `append_revision` revérifie `expected_rev` dans sa transaction) ; aucun déclencheur de base : il ne saurait pas si la clé sert.
   - **Rotation** : non prévue dans cette tranche. La colonne `format` et les données associées versionnées la rendent possible (re-sceller toutes les lignes sous une clé neuve dans une transaction, puis remplacer le fichier et la vérification). Elle n'est ni écrite ni exposée.
5. **Jamais vers le client** hors la révélation existante (qui lit le moteur) ; **jamais dans une sauvegarde** ; **jamais en clair dans la base, son journal d'écriture, une trace ou un message d'erreur**. Type en mémoire : `Secret` (`Debug` masqué, effacé à la libération).
6. **Étiquettes de rapprochement** sur le conteneur : `io.hearth.spec-rev` et `io.hearth.spec-mac` (HMAC-SHA256 de la description complète sous la clé) : le domaine ne reçoit que le résultat de la vérification.
7. **Trois écarts** : conforme, `modified_outside`, `container_missing`. Un service dont le conteneur a disparu reste listé, avec le seul geste « Recréer le service ».
8. **Copie d'export sans secret** (`.hearth/service.json`) : non écrite dans cette tranche.
9. Un service adopté n'a pas de description, seulement des réglages Hearth.

## Comment l'appliquer
- Toute création ou recréation de conteneur part d'une révision de la description (ADR-0041).
- Une révision s'écrit dans UNE transaction avec la mise hors-courant de la précédente, et dit la révision qu'elle a lue (`expected_rev`) : une modification concurrente est refusée, jamais écrasée.
- Test sentinelle (`tests/service_spec_secrets.rs`) : par le chemin d'initialisation du journal de production, au niveau le plus bavard, la valeur n'apparaît dans aucune trace, aucun `Debug`, aucun octet du fichier de base, de son journal d'écriture (`-wal`) ni de son index partagé (`-shm`) ; un canari prouve que le balayage voit une valeur en clair.

## Quand NE PAS l'appliquer / limites
- L'écran de modification ne peut ni lister les noms de variables ni enregistrer quoi que ce soit avec des secrets sans la clé, et chaque révision rescelle tout.
- Un agent déjà prêt garde la clé en mémoire et ne voit pas un fichier remplacé avant son redémarrage.
- La mise à zéro couvre les types du domaine (`ServicesKey`, `PlainBytes`), pas le processus : `ring` garde ses propres copies de la clé, et les tampons d'encodage laissent des copies à chaque réallocation.
- Le scellement protège d'une **copie de la base ou d'une sauvegarde de la base**. Il ne protège pas d'un compte qui a déjà tous les droits sur le serveur (root lit la clé à côté), ni d'une copie de tout `/var/lib/hearth` : la clé y est. Le moteur garde d'ailleurs les mêmes valeurs en clair dans son propre dossier.
- Perdre `services.key` rend les secrets illisibles : les services tournent encore, mais les recréer demande de ressaisir les valeurs. **La clé se sauvegarde AVEC la base, et séparément si possible** (une copie des deux au même endroit n'a plus l'effet voulu). Le runbook de sauvegarde le dit.
- La révélation des variables lit toujours le moteur (ce qui tourne, y compris pour un service adopté).

## Alternatives rejetées
- **Tout dans le moteur** (lot 1) : meurt avec le conteneur ; étiquettes immuables et lisibles ; rien pour les réglages de fiche.
- **Tout en étiquettes** : aucun secret possible, taille bornée.
- **Un fichier par service à côté des données** : pas de transaction commune avec le journal des opérations ; secrets à un pas d'une sauvegarde.
- **Clé scellée par systemd ou par un TPM** : absente de l'installation gérée ; durcissement futur.
- **Clé dérivée d'un mot de passe de compte** : l'agent doit recréer sans humain.
- **Ne sceller que les valeurs dont le nom « évoque un secret »** : une heuristique n'est pas une frontière.
- **Générer une clé neuve quand elle manque** : rendrait les secrets existants définitivement illisibles sans que personne le sache ; c'est exactement ce que la règle de la clé interdit.
- **Crates `chacha20poly1305` ou `aes-gcm`** : une pile cryptographique de plus dans l'arbre ; `ring` fait déjà le travail.

## Conséquences
- Migration `0012` (remplace aussi `service_actions` et `deployments`, voir ADR-0045). HRT-68 écrit la description dès le premier déploiement.
- `services.key` rejoint la liste des fichiers que Hearth écrit dans le dossier de données (`DATA_FILES`) : la désinstallation avec purge la retire.
- Specs à corriger : « Hearth ne garde rien » devient « garde scellé sur le serveur » dans les règles de modification, de déploiement et de suppression.
- Nouvel état « conteneur absent » et repère « Modifié hors de Hearth » à ajouter à la liste des services.

## Références
- `crates/hearth-agent/src/domain/sealed.rs`, `domain/services/{spec,drift}.rs`, `infrastructure/{sealer,services_key}.rs`, `infrastructure/sqlite/specs_repo.rs`, `application/{secret_vault,service_specs}.rs`, `migrations/0012_service_specs_and_ops.sql`.
- Modèle du fichier de clé : `infrastructure/fingerprint_secret.rs` (ADR-0034).
- RFC 8439 (ChaCha20-Poly1305) ; documentation de `ring` (`aead`, `hmac`).
