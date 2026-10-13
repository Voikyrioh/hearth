---
id: ADR-0041
titre: Le droit de gérer les services est distinct du rôle, donné depuis le serveur, et l'agent n'accepte de créer qu'un conteneur borné (plan fermé, dossiers sur liste d'autorisation)
type: securite
statut: proposée (non validée par Voiky ; posée avec HRT-76, moitié agent, première partie)
date: 2026-10-13
portee: repo
remplace: aucune (amende le point 1 de l'ADR-0012 sur ses conséquences, les « limites » de l'ADR-0036 et le point 4 du brouillon ADR-0039)
liens: [ADR-0012, ADR-0031, ADR-0032, ADR-0033, ADR-0036, ADR-0037, ADR-0038, ADR-0039 (brouillon), ADR-0042 (brouillon), ADR-0043 (brouillon), BR-GRANT-001, BR-GRANT-002, BR-GRANT-003, BR-GRANT-004, BR-GRANT-005, BR-GRANT-010, BR-GRANT-012, BR-GRANT-015, BR-GRANT-016, BR-GRANT-022, BR-ENGINE-021, BR-SVC-009]
---

> Posée par HRT-76 (le droit, côté agent). Numérotation : ADR-0039 et ADR-0040 sont réservés à deux brouillons du même jour, pas encore posés dans ce dépôt. Les points 5 à 7 (plan fermé, liste d'autorisation, carte graphique) sont portés par d'autres tickets et ne sont pas codés ici ; le point 2 (commande et question d'installation) est le ticket HRT-77.
> Instruit à partir de la réponse de Voiky du 2026-10-13 matin : « Pour le rôle root sur le serveur, je propose de créer un rôle à l'installation qui permet des actions docker. » La phrase a deux lectures ; les deux sont retenues, mais pas sous la forme la plus littérale. Rien ici n'est validé par lui.

# ADR-0041 : Le droit de gérer les services, et ce que l'agent accepte de créer

## Contexte

Le lot 1 des déploiements a posé un constat, écrit dans `SECURITY.md` et dans l'ADR-0036 : l'agent tourne en root (ADR-0012), le socket du moteur équivaut à root, donc dès que Hearth sait déployer, tout compte Administrateur devient en pratique root du serveur. Le seul garde-fou prévu était un interrupteur global (`engine.enabled`) et une liste de dossiers refusés, dont la conception dit elle-même qu'elle n'est « pas une barrière ».

Vérifié dans le code de la branche de la PR 81 : la liste refuse `/`, `/etc`, `/boot`, `/proc`, `/sys`, `/dev`, `/root`, `/tmp`, `/var/tmp`, le socket et les données du moteur, les dossiers de l'agent. Elle laisse passer `/usr`, `/nix`, `/run`, `/var/spool`, `/home/<compte>/.ssh`. Un conteneur tourne en root par défaut et, sans espace de noms d'utilisateur, ce root est celui du serveur sur les dossiers montés : écrire une clé dans `authorized_keys` ou un binaire dans `/usr/local/bin` suffit. Le constat « un administrateur Hearth est root » est donc exact avec la conception du lot 1.

Le modèle de comptes actuel (`domain/accounts/role.rs`) n'a que deux rôles, `Admin` et `ReadOnly`, et les routes n'ont que quatre niveaux d'accès (`Public`, `FirstMessage`, `Authenticated`, `Admin`).

## Les deux lectures, instruites

### Lecture 1, côté système : « l'agent ne pilote pas le moteur avec tous les pouvoirs de root »

| Piste | Ce qu'elle réduit vraiment | Verdict |
|---|---|---|
| Un compte ou un groupe dédié, créé par l'installation, membre du groupe `docker` | Rien. Le groupe `docker` équivaut à root (monter `/` suffit). L'ADR-0012 l'a déjà écrit. Et l'agent reste root pour l'alimentation, sa mise à jour et `systemctl`. | **Écartée** comme mesure de sécurité. Ce serait une étiquette, pas une barrière. |
| Mandataire de socket à liste blanche (du type `docker-socket-proxy`) | Filtre par famille de routes (`CONTAINERS`, `IMAGES`, `POST`, `EXEC`), pas le contenu d'une création : il laisse passer un conteneur privilégié ou un montage de `/` dès que la création est permise (lu dans la documentation du projet). C'est aussi un conteneur de plus, lui-même branché sur le socket. | **Écartée**. L'agent est déjà l'unique client du socket, et il peut filtrer le contenu, ce que le mandataire ne sait pas faire. |
| **Politique appliquée par l'agent sur ce qu'il accepte de créer** | Tout ce qu'un compte Hearth peut obtenir du moteur passe par un type fermé. Sans mode privilégié, sans espace de noms de l'hôte, sans capacité ajoutée, sans montage hors d'une liste d'autorisation, un service créé par Hearth n'a plus de chemin direct vers root. | **Retenue, tout de suite.** C'est la seule piste qui réduit la surface sans rien demander au serveur. |
| Moteur sans privilèges (Docker rootless, Podman rootless) sous un compte dédié | La vraie barrière : le moteur lui-même n'est plus root, donc même une évasion de conteneur ou un bogue de la politique ne donne que ce compte. | **Retenue comme mode durci, plus tard** (ADR-0042) : elle coûte la carte graphique à revalider, les ports sous 1024, la délégation des limites de ressources, et une déclaration côté système sur NixOS. |
| Séparer l'agent en deux processus, dont un seul parle au moteur | Borne la compromission du processus exposé au réseau. Mais l'agent a besoin de root ailleurs (alimentation, mise à jour, `systemctl`) : il faudrait un processus réseau sans privilèges et un assistant root à commandes typées, soit une réécriture de l'ADR-0012. | **Écartée pour l'instant.** C'est la bonne réponse à « le processus de l'agent est compromis », pas à « un compte Hearth a trop de pouvoir ». À rouvrir avec le mode durci. |

### Lecture 2, côté Hearth : « un droit distinct, qui n'est pas donné d'office à tout administrateur »

| Variante | Constat | Verdict |
|---|---|---|
| Garder « Administrateur = tout » avec l'interrupteur global `engine.enabled` | Tout ou rien par serveur ; un second administrateur créé pour gérer des comptes reçoit le moteur. | Insuffisant, l'interrupteur reste comme coupe-circuit. |
| Un troisième rôle « Gestionnaire de services » dans l'énumération | Les rôles deviennent une échelle alors que les deux pouvoirs sont indépendants : gérer les comptes et gérer les services. Où ranger « un ami qui peut relancer le serveur de jeu mais pas toucher aux comptes » ? | Écarté. |
| **Un droit porté par le compte, indépendant du rôle** | Deux axes : le rôle (comptes, journal, mise à jour) et le droit « gérer les services ». Le cas de l'ami est couvert sans rien inventer de plus. | **Retenu.** |
| Le droit donné depuis le client par un administrateur | Un administrateur se le donne à lui-même : c'est une marque d'intention, pas une barrière. | Écarté par défaut (voir « À confirmer » de la feuille de route). |
| **Le droit donné depuis le serveur seulement** (installation, ligne de commande) | Personne n'obtient par le réseau un pouvoir sur le moteur que le root du serveur n'a pas accordé. C'est le sens de « créer un rôle à l'installation ». | **Retenu.** |

## Décision

1. **Deux axes.** Le rôle reste `Admin` ou `ReadOnly`. Un compte porte en plus un droit `services` : `none` ou `manage`. Les quatre combinaisons existent ; « Lecture seule + gérer les services » est permise (pilote les services, ne touche ni aux comptes ni au journal).
2. **Le droit se donne sur le serveur, jamais par le réseau.**
   - À l'installation : `hearth-agent install` pose la question pour le premier compte quand un moteur est détecté (« Ce compte pourra-t-il déployer et piloter des services sur ce serveur ? »). En mode interactif, Entrée vaut oui ; sans terminal ou avec `--yes`, la réponse vaut non sauf option `--services` (ou `HEARTH_SERVICES=manage`). `deploy/install.sh` transmet l'option comme les autres.
   - Ensuite : `hearth-agent account services <compte> --grant | --revoke`, exécuté en root sur le serveur. Consigné au journal d'activité (`account.services.grant` ou `account.services.revoke`, origine « ligne de commande du serveur »).
   - Aucune route ne modifie ce droit. Un compte créé depuis le client naît avec `none`.
3. **Ce que le droit ouvre.** Tout acte qui touche le moteur ou un service : démarrer, arrêter, redémarrer, déployer, annuler un déploiement, adopter et relâcher un service (ADR-0043), révéler les valeurs de variables, et tous les actes des lots suivants (supprimer, modifier, ressources, console, fichiers, sauvegardes). Niveau d'accès nouveau `Access::Services` dans la table des routes. Le rôle seul n'ouvre plus aucun de ces actes.
4. **Ce qui ne change pas.** Le contrat des actes (ADR-0031 : preuve de clé liée à l'acte, mot de passe par le chemin de la connexion), l'élévation de 5 minutes (ADR-0032) et la porte unique du client (ADR-0033) s'appliquent tels quels : ils prouvent qui agit, le droit dit s'il peut. L'interrupteur `engine.enabled` reste le coupe-circuit du serveur.
5. **L'agent ne crée que des conteneurs bornés.** Le port `ContainerEngine::create` ne reçoit qu'un `ContainerPlan` fermé, construit par le domaine. Ce type n'a aucun champ pour : mode privilégié, capacité ajoutée, réseau, PID, IPC, UTS ou espace d'utilisateur de l'hôte, périphérique brut, option de sécurité désactivée (`seccomp=unconfined`, `apparmor=unconfined`), montage du socket du moteur, `volumes-from`, nom d'un autre conteneur comme espace de noms. Il pose d'office `no-new-privileges`. Les capacités sont celles par défaut du moteur, ni plus ni moins (les retirer toutes casserait les images qui changent d'utilisateur au démarrage, dont celles des serveurs de jeu de forge).
6. **Dossiers : une liste d'autorisation remplace la liste de refus.** Un montage n'est accepté que sous la racine de données Hearth (`services.data_root`) ou sous un dossier nommé dans `services.allowed_roots` de `agent.toml` (vide par défaut, écrit sur le serveur par son root). La liste de refus du lot 1 reste appliquée par-dessus, en ceinture. Un chemin est comparé après résolution des liens symboliques.
7. **La carte graphique** passe par un champ typé du plan (lot ressources), jamais par un périphérique libre.
8. **`SECURITY.md` dit la vérité nouvelle** : « Un compte qui a le droit de gérer les services choisit des images et les fait tourner sur le serveur. Il n'obtient pas root par ce que Hearth crée, sauf si le serveur a ouvert un dossier sensible dans `services.allowed_roots`, sauf faille du moteur ou du noyau, et sauf pour un service adopté dont la configuration le permet déjà. Le processus de l'agent, lui, reste root. »

## Comment l'appliquer

- **Base** : migration `0010`, colonne `accounts.services_right TEXT NOT NULL DEFAULT 'none' CHECK (services_right IN ('none','manage'))`. Les comptes existants restent à `none` : rien n'est accordé en silence.
- **Domaine** : `domain/accounts/rights.rs` (`ServicesRight`, `can_manage_services`), sans toucher à `Role`. `domain/services/plan.rs` (`ContainerPlan`, constructeur unique depuis `DeploySpec` validé). `domain/services/folders.rs` : `is_allowed(canonique, racines)` avant `is_refused`.
- **Protocole** (additif, `API_VERSION` inchangée) : `rights: ["services"]` dans la description du compte courant et dans chaque ligne de la liste des comptes (absent d'un agent ancien : lu comme vide) ; refus `403 FORBIDDEN` avec `details.missing: "services"` ; capacité `service-rights` dans `/hello`.
- **Routes** : `GET /services/{key}/env` passe de `Access::Admin` à `Access::Services` ; toutes les routes d'actes de service naissent en `Access::Services`. Le test de balayage des routes gagne un cas : un administrateur sans le droit ne modifie aucun service.
- **Ligne de commande** : `account services`, question de `install`, options `--services` et `--no-services`.
- **Client** : dans Comptes, un repère « Peut gérer les services » en lecture seule, avec la commande à lancer sur le serveur pour l'accorder (nom du compte validé avant d'entrer dans la commande, comme pour HRT-74 S7). Dans Services, un compte sans le droit voit les boutons désactivés avec la raison écrite dessous.
- **Journal** : `account.services.grant` et `account.services.revoke` (accordé, retiré), refus d'un acte faute de droit consigné sous l'action de l'acte, comme les refus de rôle.

## Mise en œuvre de la moitié agent (HRT-76)

Ce que ce ticket a posé, et les écarts avec le texte ci-dessus.

- **Fait** : migration `0010` (`accounts.services_right`, défaut `none`, aucun compte existant ne l'a), `domain/accounts/rights.rs` (`ServicesRight`), niveau d'accès `Access::Services` jugé par `auth::allows` (point unique, relu en base à chaque requête), `rights` sur `AccountInfo` (compte courant, connexion) et `AccountItem` (liste des comptes), capacité `service-rights` dans `/hello`, refus avec le droit manquant en détail, `GET /services/{key}/env` passée de `Admin` à `Services`. Aucune route n'écrit la colonne : test de balayage par déclencheurs SQLite et test sur les sources.
- **Écart 1, code du refus** : le texte parle d'un `403 FORBIDDEN` ; ce code n'existe pas. Le refus faute du droit réutilise `403 FORBIDDEN_ROLE` avec `details.missing: "services"` (un client ancien le lit comme un refus, aucune variante de plus à décoder). La cause consignée est « droits insuffisants » (`Reason::InsufficientRights`), distincte de « lecture seule ».
- **Écart 2, pas d'oracle** : le droit est jugé avant la lecture de la clé du chemin. Un compte sans le droit reçoit la même réponse pour un service connu, inconnu ou une clé mal formée.
- **Point 2 livré par HRT-77** : commande `account services {compte} --grant | --revoke` (root seulement, une seule requête d'écriture, un seul cas d'usage `ServicesRightService` hors de tout état remis au serveur HTTP), question de l'installation et options `--services` / `--no-services` / `HEARTH_SERVICES`, y compris l'installation gérée. Écart avec le texte de la décision : le journal compte DEUX codes, `account.services.grant` et `account.services.revoke` (un libellé par code : « Droit de gérer les services accordé » / « retiré »), au lieu d'un seul `account.services` ; ils forment le type d'action « Droits sur les services » du filtre. Voir BR-GRANT-006 à BR-GRANT-009 et `docs/runbooks/accorder-le-droit-de-gerer-les-services.md`.
- **Le droit ne se prend pas non plus par le réseau (défaut à confirmer par le propriétaire, ajouté par la revue locale du 2026-10-13)** : sans cela, un administrateur sans le droit définissait le mot de passe d'un compte qui l'a, ouvrait une session à son nom et obtenait l'effet du droit par le réseau, alors que la décision 2 veut qu'on ne l'obtienne que depuis le serveur. Défaut retenu pour avancer, REFUS DANS LE CODE : un compte qui porte le droit ne voit ses moyens de connexion (son mot de passe) modifiés par le réseau que par lui-même ou par un compte qui porte lui aussi le droit ; sinon l'acte est refusé (`FORBIDDEN_ROLE`, `details.missing: "services"`, consigné « droits insuffisants ») avec le texte « Ce compte peut gérer les services de ce serveur : seul un compte qui a ce droit, ou la ligne de commande du serveur, peut modifier ses moyens de connexion. » La ligne de commande reste permise. Les autres actes du réseau sur un compte d'autrui ont été inventoriés (BR-GRANT-005) : changement de rôle, suppression, création, fermeture des sessions, mode attaque, mise à jour de l'agent sont sans danger pour le droit (ils n'ajoutent aucun moyen de connexion à qui les demande ; la suppression emporte le droit avec la ligne du compte et un compte recréé naît sans lui). Alternative non retenue : accepter que « un administrateur Hearth peut prendre le compte d'un titulaire du droit » et l'écrire dans `SECURITY.md` ; elle laisserait le droit sans protection contre le rôle qu'il veut justement séparer. À confirmer ou à renverser par le propriétaire.
- **Pas fait ici** : points 5 à 7 : autres tickets. Tant que le droit n'est pas accordé (installation ou commande), personne ne le détient : plus personne ne révèle les variables, comportement sûr voulu.
- **Le refus ne passe pas encore par la liaison cliente** : `hearth-link` ne transporte pas `details` d'une erreur (`crates/hearth-link/src/error.rs`) et le client de bureau traduit `FORBIDDEN_ROLE` en « rôle insuffisant ». La phrase « un client récent lit `details.missing` » ne sera vraie qu'après le ticket qui fait porter le détail par la liaison (HRT-78, hors de ce ticket) : d'ici là un administrateur sans le droit lit « rôle insuffisant ».
- **Ce qu'un compte sans le droit voit encore** : la liste et l'état des services publiés, les noms de variables et la commande de lancement du détail (`ServiceItem.env_names`, `ServiceItem.command`), les montages et les ports. Le filtrage de ce qui est publié et la restriction du détail d'un service adopté sont ADR-0043 (tickets suivants).

## Chemin de migration depuis l'existant

| Élément livré ou prêt | Ce qui change |
|---|---|
| PR 77 à 80 (liste et écran en lecture seule) | Rien pour le droit. Le filtrage des conteneurs hors Hearth relève de l'ADR-0043. |
| PR 81, `GET /services/{key}/env` en `Access::Admin` | Devient `Access::Services`. À faire dans la tranche du droit, avant toute publication. |
| Migration `0009` | Inchangée. La colonne arrive par `0010`. |
| Agent d'un serveur de démonstration | Après mise à jour : aucun compte n'a le droit. L'administrateur du serveur lance `hearth-agent account services <son compte> --grant` une fois. |
| Specs du lot 1 | `BR-ENGINE-021/022` (interrupteur) gardées ; partout où une règle dit « administrateur » pour un acte de service, lire « compte qui a le droit de gérer les services » ; règles de dossiers du déploiement (fiches pas encore posées dans ce dépôt, spec du lot 1) : « tout sauf une liste » devient « rien sauf une liste ». |
| Tickets HRT-64, 65, 68, 69, 70 | Critères d'accès à reformuler ; HRT-70 porte la liste d'autorisation. |
| Brouillon ADR-0039, point 4 | Amendé par le point 6 ci-dessus. |

## Installation gérée sur NixOS

- L'installation gérée crée le premier compte par `hearth-agent install --managed` : la question du droit s'y pose de la même façon.
- `services.allowed_roots` est un réglage de `agent.toml`, donc déclarable par le système. Pour un serveur de jeu, y inscrire le dossier de ses données n'est utile que pour parcourir ou sauvegarder ses fichiers depuis Hearth (ADR-0043).
- Le mode durci (moteur sans privilèges sous un compte dédié) demande une déclaration du système : compte, session persistante, moteur sans privilèges. Hearth ne l'écrit pas, il le détecte. Voir ADR-0042.

## Quand NE PAS l'appliquer / limites

- **La politique borne les comptes, pas le processus.** L'agent reste root et garde le socket. Qui prend le processus de l'agent a tout.
- **Un conteneur borné reste du code choisi par l'utilisateur**, lancé en root dans son compartiment : il consomme, écoute sur le réseau, et profite d'une faille du moteur ou du noyau s'il y en a une.
- **`services.allowed_roots` mal rempli rend la barrière vaine** (y mettre `/home` rouvre `authorized_keys`). Le texte du réglage et le runbook le disent.
- **Le droit donné sur le serveur demande un accès au serveur** pour l'accorder à un second compte. C'est voulu.
- **Un service adopté** garde la configuration que le système lui a donnée, privilégiée ou non (ADR-0043).
- `no-new-privileges` peut gêner une image qui s'appuie sur un binaire à bit setuid : non rencontré, non testé sur les images des fiches de jeu.

## Alternatives rejetées

- **Compte dédié dans le groupe `docker`** : équivalent à root, aucune réduction.
- **Mandataire de socket à liste blanche** : filtre les routes, pas le corps d'une création.
- **Troisième rôle** : mélange deux pouvoirs indépendants.
- **Droit accordé depuis le client par un administrateur** : auto-attribution.
- **Liste de refus seule** : contournable par tout dossier oublié (`/usr`, `/nix`, `~/.ssh`).
- **Retirer toutes les capacités par défaut** : casse les images courantes ; à offrir plus tard comme réglage par service.
- **Deux processus** : réécriture de l'ADR-0012, hors de l'objectif en cours.

## Conséquences

- Une migration, un niveau d'accès, une commande, une question d'installation, un repère d'écran.
- Les specs et tickets du lot 1 sont à amender sur le mot « administrateur ».
- L'« autre dossier » du formulaire de déploiement n'est proposé que si le serveur a ouvert au moins une racine ; sinon le formulaire n'offre que l'emplacement géré.
- Le découpage plus fin du droit (piloter seulement, ou piloter et déployer) reste possible sans migration de forme : la colonne est un texte.

## Références

- `crates/hearth-agent/src/domain/accounts/role.rs`, `entrypoint/http/mod.rs` (`Access`), `entrypoint/http/auth.rs` (`allows`).
- `SECURITY.md`, paragraphe « Le moteur de conteneurs ».
- Documentation de `Tecnativa/docker-socket-proxy` (granularité par préfixe d'adresse, lue le 2026-10-13).
- Documentation de Docker, mode sans privilèges (lue le 2026-10-13).
