---
id: ADR-0021
titre: Cible de la mise à jour de l'agent : fichier agent.json dans la release du client, lu par le client dans la même tentative, jamais choisie par l'interface
type: securite
statut: acceptée
date: 2026-10-06
portee: projet
remplace: —
liens: [ADR-0003, ADR-0008, ADR-0013, ADR-0014, ADR-0016, ADR-0017, conception technique 2026-10-04 sections 5, 7 et 9, HRT-17]
---

# ADR-0021 : Cible de la mise à jour de l'agent

## Contexte

L'agent sait se mettre à jour (ADR-0014) mais n'interroge jamais Internet de lui-même : c'est le CLIENT qui lui transmet la cible (`version`, `url`, `signature`, `sha256`), et l'agent vérifie tout contre SA clé embarquée avant d'écrire quoi que ce soit. Ce que le lot interface de HRT-17 devait trancher, et que l'ADR-0017 (décision 8) et l'amendement de l'ADR-0008 avaient laissé ouvert : **où et sous quelle forme la cible de l'agent est publiée, comment le client la lit sans dépasser sa règle de fréquence, et qui fait confiance à quoi.** Le flux du client (ADR-0017) est au format du greffon de Tauri, qui ne rend le manifeste que lorsqu'une version plus récente du CLIENT existe : une section `agent` du même fichier serait illisible quand le client est à jour.

C'est aussi une commande qui fait exécuter un nouveau binaire en root sur le serveur : la chaîne doit tenir de bout en bout, même si une partie (le flux, l'état du client, la WebView) est altérée.

## Décision

1. **Un fichier à part, `agent.json`, dans la MÊME release GitHub que `latest.json`.** `https://github.com/Voikyrioh/hearth/releases/latest/download/agent.json`, constante de la compilation (`agent_update/domain.rs::AGENT_FEED_URL`), pas un réglage, mêmes règles de redirection que le flux du client (HTTPS à chaque saut, au plus `MAX_REDIRECTS` = 5, `harden` partagé). Format : `{ "version", "pub_date", "platforms": { "linux-x86_64": { "url", "signature", "sha256" } } }`. `signature` est le contenu du `.minisig` (ou son base64), `sha256` la somme du binaire. Fichier à part plutôt qu'une section de `latest.json` : le greffon et `client-manifest` (qui réécrit tout le fichier) ne sont pas touchés, et un fichier de cibles n'a pas à suivre le cycle du client. Une release sans `agent.json` (404) ou sans entrée pour l'agent ne propose rien : ce n'est pas une panne.
2. **Le client le lit dans la MÊME tentative que son propre flux, et jamais seul.** Dans `UpdateService::check`, juste après la requête du flux du client, UNE requête de plus vers le même hôte (`Feed::check_agent`). Conséquences voulues et dites : (a) **aucune tentative de plus** : la règle d'au plus une vérification automatique par 24 h, le plafond dur de 3 tentatives par 24 h glissantes et la fenêtre de 30 s de « Vérifier maintenant » (ADR-0017, décision 6) comptent la tentative, pas les requêtes ; (b) **le nombre de requêtes HTTP par tentative passe de 1 à 2** (au plus 6 par 24 h au lieu de 3, vers `github.com`) : le quota de TENTATIVES n'est pas doublé, celui des REQUÊTES l'est, et c'est le coût assumé de ne pas toucher au format du greffon ; (c) si la tentative n'a émis aucune requête pour le client (pas de réseau, `no_request_sent`), le fichier de l'agent n'est pas tenté non plus ; (d) une erreur de lecture (muet, 5xx, trop gros, illisible) est SILENCIEUSE (BR-UPDATE-007, 008) et laisse la cible précédente ; elle ne consomme ni ne restaure le quota du client ; (e) rien n'est lu au clic « Mettre à jour l'agent » : la cible est celle de la dernière lecture, validée de nouveau. Une cible refusée par les règles ci-dessous EFFACE la précédente (un fichier qui annonce maintenant quelque chose de refusé ne laisse pas proposer l'ancien).
3. **La cible ne sort jamais de la coquille.** Retenue dans `update.json` (`agent`), relue et VALIDÉE de nouveau à chaque usage (le fichier se modifie à la main). L'interface ne reçoit que le NUMÉRO de la version disponible (`AgentUpdateView.available.version`). La commande `update_agent(server_id, version)` ne prend que le serveur et le numéro que l'utilisateur a sous les yeux ; elle refuse (`target_changed`) si ce n'est plus celui que la coquille retient, et construit elle-même la requête `POST /agent/update` (`agent_update/wire.rs`). `tests/capabilities.rs` reste sans exception : aucun paramètre libre (`path`, `method`, `body`, `url`).
4. **Qui fait confiance à quoi.** La SÉCURITÉ ne repose pas sur le fichier ni sur le client : l'agent vérifie la somme SHA-256 ET la signature minisign contre SA clé (distincte de celle du client), refuse une adresse non HTTPS, locale ou privée (BR-UPDATE-027, y compris après résolution et à chaque redirection), une version qui n'est pas plus récente, le rôle (BR-UPDATE-011). Le client n'a donc PAS besoin de faire confiance au fichier pour la sécurité. Ce que le client garantit en plus, pour ne jamais envoyer ce qu'il sait mauvais (`agent_update/domain.rs::validate_target`, `is_newer`) : version `X.Y.Z` stricte (ni préversion ni métadonnées), somme de 64 hexadécimaux, signature plausible et bornée, adresse HTTPS sans identifiant ni fragment, hôte public (tables de bouclage, privé, lien local, partagé, noms internes), et source permise par la même politique que l'installateur du client : `github.com`, `/Voikyrioh/hearth/releases/download/`. **Aucune rétrogradation proposée ni envoyée** : « disponible » veut dire STRICTEMENT plus récent que la version lue chez l'agent (`GET /agent/update`, relue juste avant d'envoyer) ; même version, version plus ancienne ou version d'agent illisible : rien n'est proposé (`not_newer` si on force). Un fichier de cibles forgé par quelqu'un qui contrôle `github.com/Voikyrioh/hearth` ou le chemin réseau ne peut donc faire exécuter qu'un binaire signé par la clé de l'agent ; sans cette clé, l'agent le refuse avant d'écrire.
5. **Le contrat d'événement rentre dans `ServerMessage`.** `ServerMessage::Update(UpdateProgress)` (mêmes champs à plat à côté de `type`) ; `UpdateMessage` reste le type d'ENVOI de l'agent tant que `hearth-agent` n'est pas touché pour autre chose (même forme sur le fil, prouvé par un test de `hearth-proto`) : la dette « `UpdateMessage` hors de `ServerMessage` » de l'ADR-0014 est résorbée côté lecture, sans changer ce qu'un agent déjà installé envoie, et un client plus ancien lit la trame comme inconnue et l'ignore (comportement existant de `hearth-link`). La variante est tolérante aux raisons nouvelles (`UpdateReason::Unknown`, déjà là).
6. **Une coupure attendue pendant le redémarrage, décidée par la bibliothèque de liaison.** L'agent annonce l'étape `restart` puis ferme le flux (code 1001, « l'agent s'arrête »). `hearth-link` (`domain/state.rs`) pose alors une fenêtre de 2 minutes (`RESTART_WINDOW` : les 60 s de contrôle du nouvel agent, un retour arrière éventuel, une marge) pendant laquelle la coupure est ATTENDUE : « Reconnexion… » dès la coupure, jamais « Hors ligne », aucun échec compté (donc ni notification Windows de panne, BR-RESIL-015, ni avis « reconnexion échouée », BR-RESIL-018). Le retour du lien ou l'étape `done` lève l'attente ; passé la fenêtre sans retour, c'est une panne comme une autre. Le sujet `update` est abonné par toute connexion (tout compte).

## Alternatives rejetées

- **Une section `agent` dans `latest.json`.** Illisible quand le client est à jour (le greffon jette le manifeste, ADR-0017 décision 8), et `client-manifest` réécrit le fichier entier.
- **Une seconde requête hors de la règle des 24 h** (à l'ouverture des réglages, au clic) : plus de requêtes et une règle de fréquence de plus ; la cible peut avoir 24 h : l'agent est l'arbitre.
- **Laisser l'interface fournir ou relire la cible** (adresse, signature, somme dans la commande) : c'est exactement la porte que l'ADR-0016 ferme.
- **Faire confiance au fichier** (ne pas valider côté client) : l'agent protège, mais un client qui envoie ce qu'il sait mauvais (rétrogradation, adresse locale) fait inutilement exécuter un téléchargement et brouille le journal d'activité du serveur.
- **Une release dédiée à l'agent.** `releases/latest` est unique : une release de l'agent seule sans `latest.json` rendrait le client muet (ADR-0017).
- **Un flux d'agent signé par le client.** Sans valeur : la signature qui compte est celle que l'agent vérifie.

## Conséquences

- **Publication** (`docs/runbooks/mettre-a-jour-agent.md`) : toute release porte `latest.json` ET `agent.json` ; `cargo xtask agent-manifest` fabrique le second après avoir vérifié la signature contre `crates/hearth-agent/update-key.pub`. La signature de l'agent reste faite à la main avec la clé secrète du détenteur (jamais dans la CI). Aucun flux de publication n'est ajouté ni modifié ; `publish-client` est inchangé et ne joint pas `agent.json` (ajout à la main au brouillon, voir le runbook).
- **Une release du client seul** (agent inchangé) peut reprendre l'`agent.json` de la précédente (même version d'agent : rien de « disponible »). Sans `agent.json`, plus rien n'est proposé pour l'agent : le client reste utilisable.
- **Versions incompatibles (BR-UPDATE-020, 021)** : le client trop ancien peut se mettre à jour (bouton du bandeau, HRT-16) ; l'agent trop ancien NE PEUT PAS l'être depuis l'application : il répond `426 INCOMPATIBLE_VERSION` à toute route sauf `/hello` (BR-CONN-014), donc ni session ni demande de mise à jour ne passe. Le message dit quoi faire (mettre à jour l'agent, ou demander à un administrateur) sans bouton. Lever cette limite demande un changement de l'agent (accepter `/agent/update` hors plage), hors du périmètre de ce lot.
- Tests : `tests/agent_update_domain.rs`, `agent_target_feed.rs` (service, adaptateur contre un serveur local, code de publication lu par le client), `agent_update_runtime.rs` (vrai agent), `hearth-link/tests/agent_update.rs` (vrai agent, coupure attendue), `domain::state::tests`.

## Quand NE PAS l'appliquer

- Un agent installé hors de la release du dépôt (miroir interne) : non pris en charge ici (adresses privées refusées, BR-UPDATE-027) ; mise à jour par `install.sh --binary`.
- Une plateforme autre que Linux x86_64 : l'entrée du fichier est `linux-x86_64` seulement (ADR-0003).

## Références

- ADR-0003, ADR-0008, ADR-0014, ADR-0016, ADR-0017 ; BR-UPDATE-011 à 029 ; `docs/open-api/agent-update.md` ; `docs/runbooks/mettre-a-jour-agent.md`.
