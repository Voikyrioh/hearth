---
id: ADR-0043
titre: Par défaut Hearth ne gère que les services qu'il a créés ; un conteneur créé hors de Hearth n'est ni détaillé ni piloté avant d'avoir été adopté, service par service, par un acte consigné
type: securite
statut: proposée (non validée par Voiky ; posée avec HRT-79, ce qui sort de l'agent ; l'acte d'adopter et de relâcher livré par HRT-80, moitié agent et protocole)
date: 2026-10-13
portee: repo
remplace: aucune (amende l'ADR-0037 sur ce qui est publié, l'ADR-0038 sur le contenu du sujet `services`, et la contrainte « visibles et pilotables » de l'épic déploiements)
liens: [ADR-0031, ADR-0036, ADR-0037, ADR-0038, ADR-0041, BR-ADOPT-001, BR-ADOPT-002, BR-ADOPT-004, BR-ADOPT-015, BR-ADOPT-016, BR-ADOPT-017, BR-ADOPT-018, BR-ADOPT-022, BR-ADOPT-023, BR-SVC-001, BR-SVC-003, BR-SVC-007, BR-SVC-008, BR-SVC-009, BR-ADOPT-005, BR-ADOPT-008, BR-ADOPT-013, BR-ADOPT-019, BR-SLOG-024]
---


> Posée par HRT-79 (ce qui sort de l'agent). L'acte qui adopte et relâche, la liste des candidats, le coupe-circuit `services.adoption` et le client sont les tickets suivants (HRT-80 pour l'agent, HRT-81 pour le client) : ce ticket publie moins, il ne permet pas encore d'adopter. HRT-80 livre l'acte d'adopter et de relâcher (section « Livré par HRT-80 ») : une version qui porte HRT-79 SANS HRT-80 a une liste vide sur un serveur dont tous les conteneurs sont hors Hearth, et ne doit pas être laissée en service sur forge ; inversement, une branche sans HRT-79 publie tout à tous.
> Instruit à partir de la réponse de Voiky du 2026-10-13 matin : « En soi gérer les conteneurs déjà présents c'est un peu moyen en terme de sécu, je sais pas si ça vaut le coup de gérer ça par défaut sans demande de l'utilisateur. » Rien ici n'est validé par lui.
> Numéro de migration corrigé : le brouillon annonçait la `0010` pour la table des adoptés, la même que la colonne du droit (ADR-0041). Les deux tickets se fusionnent séparément : la colonne est la `0010`, la table est la `0011`.

# ADR-0043 : Services adoptés

## Contexte

L'épic demandait que les conteneurs existants soient « visibles et pilotables sans être détruits ni recréés ». Le lot 1 l'a pris au mot : la liste publie tous les conteneurs du serveur, à tous les comptes, avec image, ports, montages, noms des variables et commande de lancement ; la révélation des valeurs est ouverte aux administrateurs.

Ce que cela expose sans que personne l'ait demandé : la commande d'un conteneur peut contenir un secret et un compte en lecture seule la voit (`SECURITY.md` le dit) ; installer Hearth sur un serveur qui fait déjà tourner autre chose donne à ses comptes une vue, puis une main, sur des programmes qui n'ont rien à voir avec lui.

Le cas du propriétaire est l'inverse du cas général : sur forge, **tous** les services actuels sont des conteneurs hors Hearth, lancés par des unités systemd (`docker-terraria.service`, `docker-7dtd.service`, `docker-forge-studio.service`). Il voudra les voir et les piloter. La décision doit donc rendre ce geste simple, pas l'interdire.

## Variantes instruites

| Variante | Ce que voit un compte | Constat |
|---|---|---|
| Invisibles | Rien, pas même leur existence | Un port refusé « déjà pris » devient incompréhensible ; le propriétaire ne découvre jamais qu'il peut les adopter. |
| **Comptés sans détail** | « 3 conteneurs créés hors de Hearth ne sont pas gérés » | Dit la vérité sans rien exposer. |
| Visibles en lecture seule pour tous | Nom, image, état, ports de chaque conteneur | C'est l'existant, moins les actions. Noms et images renseignent déjà sur ce que fait le serveur ; rien ne l'a demandé. |
| **Adoption service par service** | Le service adopté devient un service comme un autre, avec son repère d'origine | Un geste voulu, borné à ce que l'on choisit, consigné. |
| Activation globale par serveur | Tout ce qui existe et tout ce qui apparaîtra | Un seul clic, mais il engage aussi les conteneurs futurs, que personne n'a regardés. |

## Décision

1. **Par défaut, Hearth ne publie et ne pilote que les services qu'il a créés** (clé `h_`, étiquettes de l'ADR-0039) **et les services adoptés.**
2. **Un conteneur hors Hearth non adopté est compté, jamais détaillé.** Le fil porte un nombre (`EngineView.external_count`), rien d'autre : ni nom, ni image, ni port, ni variable, ni commande, ni journal. Les routes qui visent sa clé répondent `404 SERVICE_NOT_FOUND`, sans distinguer « inconnu » de « non adopté ».
3. **L'adoption est un acte, service par service.**
   - Qui : un compte qui a le droit de gérer les services (ADR-0041). Le rôle seul ne suffit pas.
   - Comment : la fenêtre « Adopter un service existant » liste les candidats (nom, image, unité systemd s'il y en a une, état). Cette liste n'est lue qu'à l'ouverture de la fenêtre, par `GET /services/external`, niveau `Access::Services`. On coche, on confirme.
   - Acte `ServicesAdopt` (code `0x11`), cible signée : le nombre de services et l'empreinte de leurs clés triées ; 50 services au plus par acte. **Jamais couvert par l'élévation de 5 minutes** : le mot de passe est demandé à chaque adoption.
   - Acte `ServiceRelease` (code `0x12`) : rend un service à l'état « non géré ». Couvert par l'élévation (il retire un accès, il n'en donne pas).
   - Journal d'activité : `service.adopt` et `service.release`, avec le nom du service et son unité, le compte et l'origine. Les refus sont consignés comme pour tout acte.
4. **Ce que l'adoption ne fait pas.** Elle ne pose aucune étiquette, ne recrée rien, n'écrit rien dans le moteur ni dans systemd. C'est une ligne dans la base de l'agent. La relâcher ne touche pas le service.
5. **Un service adopté est piloté par son pilote** (ADR-0037) : superviseur systemd s'il en a un, moteur sinon. Tout le lot 1 s'applique à lui : état, détail, démarrer, arrêter, redémarrer, journaux, révélation des variables (avec le droit), et plus tard console.
6. **Ce qu'un service adopté ne reçoit jamais de Hearth** : modification de ses réglages, de ses ressources, de son image, suppression. Sa configuration appartient à qui l'a créé (sur forge : la configuration du système). Les écrans l'affichent en lecture seule avec la raison.
7. **Fichiers et sauvegardes d'un service adopté** : seulement pour ses dossiers situés sous une racine de `services.allowed_roots` (ADR-0041). Sinon l'onglet dit quel réglage du serveur l'ouvrirait.
8. **Comptes en lecture seule et comptes sans le droit** : ils voient la même liste que les autres (services Hearth et services adoptés) et le même compte de conteneurs non gérés. Ils ne voient pas les candidats et ne peuvent pas adopter. Pour un service adopté, la commande de lancement, les noms de variables et les journaux ne sont ouverts qu'aux comptes qui ont le droit (voir « Conséquences » : c'est un changement par rapport au lot 1, limité aux services adoptés).
9. **Coupe-circuit du serveur** : `services.adoption = false` dans `agent.toml` (`HEARTH_SERVICES_ADOPTION=off`). L'agent ne propose alors aucun candidat et refuse l'acte. Vrai par défaut.
10. **Pas d'activation globale.** La fenêtre a une case « Tout cocher » ; elle n'engage que les conteneurs présents et listés à l'écran.

## Identité d'un service adopté

- Service supervisé : la clé `u_<unité>` est stable. L'adoption survit à chaque recréation du conteneur par le système, au redémarrage de l'agent et du serveur.
- Conteneur sans superviseur : la clé `c_<identifiant>` meurt avec le conteneur. Si un tiers le recrée (pile Compose mise à jour), il redevient « non géré ». C'est dit dans la fenêtre d'adoption, pour ces candidats seulement. Adopter par nom de conteneur est écarté : l'ADR-0037 a refusé l'identité par nom pour ne pas confondre un homonyme.

## Comment l'appliquer

- **Base** (migration `0011`, distincte de la `0010` de la colonne du droit) : `adopted_services (service_key PK, name, unit, adopted_by, adopted_at)`, clé `u_<unité>` ou `c_<identifiant>` (jamais `h_`, jamais un nom : la base le refuse). La table `supervised_services` de la `0009` ne reçoit plus que des services adoptés : la migration efface toutes ses lignes existantes (aucun service n'est adopté à ce moment), la base refuse d'y écrire un service non adopté, et relâcher une adoption efface ce qu'on en retenait (déclencheurs).
- **Domaine** : `domain/services/visibility.rs`, fonction pure `visibility_of(clé, adoptés)` (publié de droit si `h_`, publié si adopté, sinon caché), `may_read_details` (commande et noms de variables d'un adopté), `lapsed` (une adoption sans unité tombe avec son conteneur) et `Holder` (qui tient un port ou un nom, sans charge utile pour un conteneur caché). `visibility_of` n'est appelée qu'à un seul endroit, `ServicesRegistry::publish`, à la fin de chaque recalcul : l'instantané, les lots de changements et toute recherche par clé ne voient que ce qui a passé ce filtre ; un changement qui ne touche qu'un conteneur caché n'avance donc pas `rev`.
- **Application** : le registre garde en mémoire **tous** les conteneurs observés. Il en a besoin pour les conflits de ports et de noms, pour le total de ressources engagées, et pour proposer les candidats. Le filtre s'applique à la publication : sujet `services`, `GET /services`, toute route par clé.
- **Protocole** (additif ; livré par HRT-79 : les trois premiers et la capacité) : `EngineView.external_count`, `ServiceItem.adopted: { by, at }` (absent pour un service Hearth), `ServiceItem.details_reserved` (le compte qui lit n'a pas le droit : commande et noms de variables retirés), capacité `service-adoption` dans `/hello` (elle dit que l'agent filtre, pas que l'adoption est possible). Livré par HRT-80 : `GET /services/external` (`Access::Services`, lecture, non journalisée), `POST /services/adopt`, `POST /services/{key}/release`.
- **Conflit de port avec un conteneur non adopté** : le formulaire de déploiement dit « Port tenu par un conteneur créé hors de Hearth », sans le nommer.
- **Client** : sous la liste, la ligne « {n} conteneurs créés hors de Hearth ne sont pas gérés. » avec le bouton « Adopter un service existant » pour qui a le droit ; fenêtre d'adoption ; dans le détail d'un service adopté, « Adopté par {compte} le {date} » et « Ne plus gérer ce service ».
- **Test de garde** : aucun message du sujet `services`, aucune réponse, aucune trace (chemin d'initialisation du journal de la production, niveau le plus bavard) ne contient le nom, l'image, un port, une variable, la commande ni l'unité d'un conteneur non adopté (sentinelle dans le faux démon, cherchée dans tout ce qui sort ; un témoin adopté prouve que le test n'est pas aveugle).

## Livré par HRT-80

- **Les trois routes** : `GET /services/external` (niveau `services`, lecture, consignée seulement quand elle est refusée), `POST /services/adopt` (acte `0x11`, droit + preuve de clé + mot de passe À CHAQUE FOIS, jamais l'élévation ; cible signée = nombre et empreinte SHA-256 des clés triées, au plus 50), `POST /services/{key}/release` (acte `0x12`, couvert par l'élévation, cible signée = la clé). La liste fermée des actes passe à douze (`admin_act::ROUTES`), `covered_by_elevation` est étendu, la table `CLASSES` classe les trois routes au niveau `Service`.
- **Ce que la lecture des candidats expose** : la clé, le nom, l'image, l'unité et l'état de chaque conteneur non géré, et rien d'autre (`BR-ADOPT-006`) ; elle est réservée au droit, recalculée à chaque appel, jamais diffusée par le flux ni gardée par l'agent. C'est le seul endroit où un nom de conteneur caché sort, et la décision du propriétaire.
- **Écriture** : une ligne de `adopted_services` et l'entrée du journal d'activité dans la MÊME transaction (`AdoptionTx`, port d'écriture de l'unité de travail) ; un verrou sérialise les actes et la base refuse la double écriture (`ON CONFLICT DO NOTHING` sous `BEGIN IMMEDIATE`). Aucun appel d'écriture au moteur : le port n'en a pas.
- **Identité** : seulement pour un conteneur que le registre voit à cet instant, par la clé que le registre lui donne (`u_<unité>`, sinon `c_<identifiant>`). Une clé `c_` du conteneur d'une unité, une clé `h_`, une clé inventée ou disparue ont la même issue `not_found`.
- **Fin d'une adoption par unité** (écart relevé par la revue de HRT-79) : elle tient tant que le système connaît l'unité. Une unité écrite dans un fichier reste `loaded/inactive` une fois arrêtée (établi par la CI sur un vrai systemd) : l'adoption reste. Elle tombe quand le système répond clairement « introuvable » (`LoadState=not-found`, `NotLoaded`) et qu'aucun conteneur ne la porte, sur trois lectures consécutives : unité supprimée, ou transitoire déchargée à l'arrêt. Tout autre état (erreur, fichier invalide, masquée) et toute réponse que le système ne donne pas (`Unavailable`) ne font jamais rien tomber. Sans entrée de journal (aucun compte n'agit).
- **Journaux d'un adopté lus sans le droit** : refus explicite `MISSING_RIGHT` (la décision `may_read` rend `MissingRight` pour l'origine « adopté » seulement) et entrée `service.logs.read` ; un conteneur caché ou inconnu garde `SERVICE_NOT_FOUND`, sans écrire au journal : aucun oracle (le service refusé est déjà dans la liste du compte refusé).
- **Coupe-circuit** : `[services] adoption = false` (`HEARTH_SERVICES_ADOPTION=off`) ; `409 CONFLICT` (`adoption_disabled`) pour la liste et l'acte, « ne plus gérer » reste possible.
- Reste pour les tickets suivants : l'écran et les textes du client (HRT-81), le refus de relâcher pendant une action de service en cours (règle d'une action déjà en cours sur le service, avec les actions de service).

## Impact sur le code déjà écrit

| Élément | Devenir |
|---|---|
| Détection du superviseur (`infrastructure/supervisor/proc_systemd.rs`, `ServiceSupervisor`) | **Reste.** Sert à classer les candidats et à piloter les services adoptés. |
| Clés `u_` et `c_` (`domain/services/key.rs`) | **Restent.** |
| Registre complet en mémoire (`application/services.rs`) | **Reste**, avec un filtre de publication ajouté. |
| `GET /services`, sujet `services` | **Passent derrière l'adoption** pour tout service qui n'est pas `h_`. |
| `GET /services/{key}/env` | **Passe derrière l'adoption** et derrière le droit (ADR-0041). |
| `supervised_services` (mémoire d'un service dont le conteneur a disparu) | **Passe derrière l'adoption** : persistée pour les seuls services adoptés. |
| Écran « Services » (`ServiceTable`, `EnginePanel`) | **Reste.** La liste se vide des services non adoptés ; le repère « Créé hors de Hearth » reste pour les adoptés ; les compteurs du moteur ne comptent que ce qui est publié, plus la ligne des non gérés. |
| Repère « Géré par le système ({unité}) » | **Reste**, pour les adoptés. |
| Noms de variables et commande publiés à tous les comptes | **À retirer pour les services adoptés** (réservés au droit). Inchangé pour les services créés par Hearth, dont le créateur a choisi le contenu. |
| Limite « un service supervisé jamais vu en marche est invisible » (ADR-0037) | Vaut désormais pour les candidats : un service arrêté dont le conteneur est supprimé n'est pas proposé tant qu'il n'a pas tourné. Sur forge, il suffit qu'il tourne au moment d'adopter. |

Rien n'est supprimé du dépôt. Tant que la tranche d'adoption n'est pas fusionnée, la branche principale publie tout à tous : elle ne doit pas être publiée ni laissée en service sur forge au-delà d'un essai.

## Le cas de forge, pas à pas

1. Voiky met l'agent à jour, puis accorde le droit à son compte sur le serveur (ADR-0041).
2. L'écran Services affiche une liste vide et « 3 conteneurs créés hors de Hearth ne sont pas gérés ».
3. Il ouvre « Adopter un service existant », voit `terraria`, `7dtd`, `forge-studio` avec leur unité, coche ceux qu'il veut, confirme avec son mot de passe.
4. Ils apparaissent, « Géré par le système », pilotés par `systemctl`, avec leur vrai délai d'arrêt.

## Ce qui reste observable d'un conteneur non adopté

Dit sans détour, parce que « rien ne sort » est une affirmation à borner :

- **Le nombre** de conteneurs non gérés (`external_count`), arrêtés compris, et sa variation : un conteneur qui apparaît ou disparaît fait bouger le compte d'un, et fait partir un lot. C'est voulu.
- **Un bit sur la fraîcheur de la liste** : un conteneur caché dont l'inspection échoue lève `list_degraded` comme n'importe quel conteneur (« impossible de lire les services »), sans rien dire de lui. Ce n'est pas filtré.
- Rien d'autre n'a été trouvé : ni `rev` (un changement d'état d'un conteneur caché n'en avance pas), ni la taille ou la cadence des messages, ni l'ordre de tri (les services publiés seuls sont triés), ni la réponse d'une route par clé (même code, même corps, même chemin de recherche qu'un service inconnu). La charge de l'agent et la durée d'un recalcul croissent avec le nombre de conteneurs lus : non filtré, non mesuré.

## Quand NE PAS l'appliquer / limites

- **Adopter donne de vrais pouvoirs** : arrêter un serveur de jeu, lire ses journaux, révéler ses variables. Et un service adopté garde ses privilèges d'origine (réseau de l'hôte, carte graphique, dossiers) : la politique de l'ADR-0041 ne s'applique qu'à ce que Hearth crée.
- **Le compte seul renseigne déjà un peu** (« il y a 3 conteneurs »). Assumé : c'est le prix d'un message de conflit de port compréhensible.
- **L'agent lit toujours tous les conteneurs.** La décision porte sur ce qui sort de l'agent, pas sur ce qu'il observe.
- Un conteneur d'une pile Compose perd son adoption à chaque recréation : gêne connue, sans réponse dans ce lot.

## Alternatives rejetées

- **Tout visible en lecture seule** : l'existant ; expose sans demande.
- **Tout invisible** : conflits de port muets, adoption introuvable.
- **Activation globale par serveur** : engage les conteneurs futurs.
- **Adoption par étiquette posée sur le conteneur** : impossible sans recréer le conteneur, et sur forge le système la retirerait au démarrage suivant.
- **Adoption depuis le serveur seulement (ligne de commande)** : le droit de gérer les services est déjà donné depuis le serveur ; exiger deux allers-retours sur le serveur pour trois services n'ajoute rien.
- **Révélation des variables d'un service non adopté pour un administrateur** : contredit le principe.

## Conséquences

- Les fiches `BR-SVC-001`, `003`, `007`, `008`, `009`, `013`, `014`, `016`, `020` et `BR-ENGINE-004` sont amendées par HRT-79 ; les specs des journaux (lot suivant) et la contrainte de l'épic restent à amender : « après adoption ».
- Deux actes de plus dans `admin_act` (`0x11`, `0x12`), `covered_by_elevation` étendu, `deploy/e2e/admin_act.py` à suivre.
- Le runbook de forge (HRT-71) commence par l'adoption.

## Références

- `crates/hearth-agent/src/application/services.rs` (registre, `publish`, `reveal_env`), `domain/services/{key,visibility}.rs`, `infrastructure/supervisor/proc_systemd.rs`, `migrations/0009_services.sql`, `migrations/0011_adopted_services.sql`.
- `SECURITY.md`, paragraphe « Variables d'environnement d'un service ».
- `forge-nixos` : `jeux/terraria.nix`, `jeux/7dtd.nix`, `forge-studio.nix`.
