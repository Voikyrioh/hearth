---
id: ADR-0039
titre: Services créés par Hearth : un plan de conteneur fermé construit depuis la description durable, preuve d'origine par consignation de l'identifiant du conteneur, limites obligatoires, politique unless-stopped tenue par le moteur, dossiers gérés sous la racine des services
type: securite
statut: proposée (non validée par Voiky ; posée avec HRT-68)
date: 2026-10-17
portee: repo
remplace: aucune
liens: [ADR-0012, ADR-0036, ADR-0037, ADR-0041, ADR-0043, ADR-0044, ADR-0045, ADR-0046, BR-SVC-003, BR-DEPLOY-010, BR-DEPLOY-012, BR-DEPLOY-021, BR-DEPLOY-035, BR-DEPLOY-037, BR-DEPLOY-038, BR-LIFE-010, HRT-68]
---

> Posée par HRT-68 (déployer une image, côté agent). Reprend le brouillon `2026-10-13-adr-services-crees-par-hearth-brouillon` du hub et le corrige sur quatre points, tranchés par les tickets posés depuis : la racine des données est celle de `SafeFs` (ADR-0046), l'origine ne tient plus à une étiquette (ADR-0037), la description durable est tenue par l'agent (ADR-0044), les limites sont obligatoires (BR-DEPLOY-038).

# ADR-0039 : Services créés par Hearth

## Contexte
Un service déployé depuis Hearth doit être reconnu comme tel, repartir après un redémarrage du serveur sauf arrêt voulu, ranger ses données à un endroit prévisible, et ne jamais recevoir de quoi prendre la main sur la machine. L'agent tourne en root et parle au moteur : le plan de conteneur qu'il construit est la barrière.

## Décision
1. **Un plan de conteneur FERMÉ** (`domain/services/plan.rs::ContainerPlan`, ADR-0041). Il n'a aucun champ pour un mode privilégié, une capacité, le réseau ou les processus du serveur, un périphérique, une option de sécurité, le socket du moteur, ni un chemin du serveur autre que le dossier géré de ce service : ce qui ne s'exprime pas ne se demande pas. Une demande qui porte un membre inconnu est refusée (`PLAN_SETTING_REFUSED`, « Ce réglage n'est pas permis pour un service créé par Hearth. », sans nommer le réglage). **Un seul constructeur** (`plan_from_spec`) : un formulaire et, plus tard, une fiche passent par lui. Le conteneur se crée DEPUIS la description durable relue en base (ADR-0044, BR-DEPLOY-037), jamais de la demande brute.
2. **Origine par consignation.** Les étiquettes `io.hearth.managed`, `io.hearth.service-id`, `io.hearth.install-id`, `io.hearth.created-by` et `io.hearth.spec-rev` sont posées à la création, jamais sur un conteneur existant. **Mais une étiquette ne prouve rien** (celle d'une image publique peut la porter) : un conteneur n'est « créé par Hearth » que si son identifiant, rendu par le moteur à la création, est consigné par l'agent pour une clé `h_` connue (table `service_containers`, migration `0015`). Le registre ne reconnaît que ceux-là (`hearth_ids`), et la reprise des opérations les compte (`SpecRecordedContainers`).
3. **Limites obligatoires** (BR-DEPLOY-038) : un service se crée avec une limite de mémoire et une limite de processeur (`LIMITS_REQUIRED` sinon), au moins 0,25 cœur et 128 Mo, au plus la machine (lue sur le moteur, pas dans un conteneur : `GET /engine/capacity`). Le swap est égal à la mémoire : la limite est réelle.
4. **État voulu dans le moteur.** `RestartPolicy = unless-stopped` : le moteur relance le service au démarrage du serveur et retient lui-même un arrêt manuel. L'agent ne réconcilie rien, ne génère ni unité systemd ni module de distribution.
5. **Données.** Sous `<dossier de données de l'agent>/services/<clé>/<chemin du montage aplati>/`, créées par `SafeFs` (aucun lien suivi, ADR-0046), montées par des montages structurés de type `bind` (jamais une chaîne `source:cible` à découper). L'aplatissement est injectif (`+` entre composants, `%XX` pour le reste). Un « autre dossier du serveur » est refusé (`FOLDER_REFUSED`) jusqu'à HRT-70 ; la liste des dossiers protégés (BR-DEPLOY-012) en juge la raison.
6. **Secrets.** Valeur et nom de variable, commande : type `Secret`, écrits seulement scellés dans la description (ADR-0044) puis chez le moteur ; jamais dans une trace, une erreur, le journal d'activité, une ligne de l'opération, ni la signature de l'acte (qui ne porte que leur nombre et un « il y a une commande »).
7. **Un échec avant le lancement ne laisse rien** (BR-DEPLOY-019) : description retirée, dossier de données retiré, conteneur créé puis refusé supprimé. Un résultat inconnu (moteur muet après l'ordre) ne retire rien : l'opération est « interrompue ».
8. **Annulation** par la route des opérations (`DELETE /operations/services/{id}`, acte `0x10`), tant que l'image se récupère ; le point de non-retour est la création (`TOO_LATE_TO_CANCEL`). Le téléchargement abandonné ferme la connexion au moteur.

## Comment l'appliquer
- `domain/services/{deploy,plan,ports,command,pull}.rs`, `application/deployments.rs`, ports `HostPorts`, `ServiceContainers`, `ContainerEngine` (`image_present`, `pull`, `create`, `remove`, `info`), adaptateur `infrastructure/engine/bollard.rs` et sa table d'erreurs `errors.rs`.
- Toute vérification du formulaire est refaite au lancement par la même fonction de domaine (`validate`).
- Test sentinelle : une valeur, un nom de variable et la commande ne se retrouvent ni en base, ni dans les traces, ni au journal, ni dans ce que rendent les routes (`tests/service_deploy_https.rs`).
- Ajouter un réglage au plan est une décision (ADR) : un test de forme casse tant qu'un champ apparaît sans lecture de cette liste.

## Quand NE PAS l'appliquer / limites
- **La liste des dossiers refusés est un garde-fou contre l'erreur, pas une barrière** : un administrateur choisit l'image, donc ce qu'elle fait des dossiers accordés.
- Le dossier monté est inscriptible par tous les comptes DU CONTENEUR (`0777`, propriétaire root) : Hearth ne connaît pas le compte du programme. Le dossier du service reste à l'agent seul (`0700`), donc inaccessible du serveur.
- `unless-stopped` relance aussi un programme qui se termine avec le code 0 : il redémarre en boucle jusqu'à ce qu'on l'arrête (BR-DEPLOY-020).
- Le service ne repart au démarrage du serveur que si le moteur démarre avec lui, et un nettoyage automatique des conteneurs arrêtés (`docker system prune` planifié, `virtualisation.docker.autoPrune`) le supprimerait : à dire dans le runbook.
- La création et l'écriture de la description ne sont pas UNE transaction : un agent qui s'arrête entre les deux laisse une opération « interrompue » et le démarrage retire la description (BR-DEPLOY-019).

## Alternatives rejetées
- **Une unité systemd ou un module NixOS par service** : propre à une distribution, impossible sans reconstruire le système sur NixOS.
- **État voulu seulement en base, réconcilié par l'agent** : deux vérités. La base garde la description (ADR-0044), le moteur garde l'observé.
- **Origine par étiquette seule** : forgeable par l'auteur d'une image.
- **`Binds` (chaînes `source:cible[:ro]`)** : un chemin qui contient `:` change le sens du montage.
- **`on-failure` / `always`** : ne garantit pas le retour, ou relance un service arrêté à la main.

## Conséquences
- Migration `0015` (`service_containers`). Aucune directive de l'unité systemd ne change.
- Causes d'opération ajoutées : `image_not_found`, `tag_not_found`, `image_private_or_missing`, `registry_unreachable`, `registry_rate_limited`, `platform_mismatch`, `disk_full`, `name_taken`.
- `SECURITY.md` : le pilotage du moteur donne root à tout compte qui a le droit de gérer les services.
