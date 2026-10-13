---
id: ADR-0038
titre: Protocole des services : capacités annoncées par /hello, sujet services du flux, actes asynchrones (202), un WebSocket par lecture de journaux, secrets jamais signés
type: convention
statut: proposée (non validée par Voiky ; posée avec HRT-61)
date: 2026-10-13
portee: repo
remplace: aucune
liens: [ADR-0004, ADR-0007, ADR-0031, ADR-0032, ADR-0033, ADR-0034, conception technique 2026-10-13 (déploiements, lot 1) section 7, BR-TRUST-037, BR-RESIL-009, BR-RESIL-010, règles de la spec « journaux » (tranche 4)]
---

# ADR-0038 : Protocole des services

## Contexte
Le lot ajoute des routes, un sujet de flux, des actes d'administration longs (arrêt jusqu'à 150 s, déploiement jusqu'à 30 min) et un flux de journaux à 1 000 lignes par seconde. Contraintes lues dans le code : `API_VERSION` ne doit pas monter (ADR-0033) ; un `subscribe` qui contient un sujet inconnu de l'agent est refusé en entier ; la requête de la liaison expire à 10 s ; un abonné trop lent de `/stream` voit tout son flux fermé.

## Décision
1. **Capacités.** `GET /hello` gagne `capabilities: [String]` (absent = vide). Le client ne s'abonne à un sujet nouveau et n'appelle une route nouvelle que si la capacité est annoncée (`services` pour ce lot). `API_VERSION` reste à 1.
2. **Sujet `services`** de `/stream`, ouvert aux deux rôles : `services_snapshot` à l'abonnement, puis `services_delta` (au plus un par 500 ms) et `service_action`. Type à part (`ServicesMessage`) : un client qui ne le connaît pas ignore la trame. Un abonné en retard reçoit un nouvel instantané ; le client n'a pas de logique de trou.
3. **Actes longs en `202`.** Démarrer, arrêter, redémarrer, déployer : les refus connus d'avance sont synchrones et typés ; l'acte accepté rend un identifiant, s'exécute dans une tâche de l'agent, publie sa progression sur le sujet, et son issue reste lisible 24 h (`last_action` du service, `GET /deployments/{id}`). L'opération suivie par clé retient le `202`.
4. **Cinq actes de plus** dans `admin_act::ROUTES` : `ServiceStart 0x0C`, `ServiceStop 0x0D`, `ServiceRestart 0x0E`, `ServiceDeploy 0x0F`, `DeploymentCancel 0x10`. Couverts par l'élévation de 5 minutes : tous sauf `ServiceDeploy`. Aucune exception nommée de plus ; deux `POST` de lecture (`/engine/recheck`, `/deployments/check`) entrent dans `READ_ONLY_POSTS`.
5. **Jamais signés, jamais journalisés, jamais tracés** : valeurs de variables, noms de variables, commande. Le déploiement signe `image:tag`, le nombre de variables, la présence d'une commande et une **empreinte SHA-256 de la partie non secrète** (`hearth_proto::service_spec::signed_digest`), parce qu'un paramètre signé est borné à 65 535 octets.
6. **Révéler les valeurs** : `GET /services/{key}/env`, compte qui a le droit de gérer les services (ADR-0041 ; « administrateur » dans la première version de cette décision), consignée au journal en réussite comme en refus, sans nom ni valeur ; ce n'est pas un acte.
7. **Journaux : un WebSocket par lecture**, `GET /services/{key}/logs`, authentifié par son premier message comme `/stream`. Lots de 100 ms, 500 lignes ou 256 Ko. Par lecture, un anneau borné (5 000 lignes, 2 Mio) : un client lent perd les plus anciennes, comptées et signalées par `gap`. Dix lectures au plus pour tout l'agent. Reprise par curseur (horodatage du moteur et rang parmi les lignes de même horodatage).
8. **Nettoyage d'une ligne** : `hearth_proto::log_text::clean`, appliqué par l'agent seul.

## Comment l'appliquer
- Une route de plus = une ligne de `ENDPOINTS` ; si elle modifie, une ligne de `admin_act::ROUTES` et un cas de `covered_by_elevation` (correspondance exhaustive), le classement de `hearth-link/src/domain/act.rs` et `deploy/e2e/admin_act.py`.
- Un flux à fort débit ou bidirectionnel propre à un service (console du lot suivant) prend son propre WebSocket, jamais `/stream`.
- Toute fonctionnalité future du même ordre s'annonce par une capacité, pas par une hausse de `API_VERSION`.

## Quand NE PAS l'appliquer / limites
- Un acte court et borné (moins de 10 s garanties) reste synchrone, comme les actes de comptes.
- Deux connexions TLS épinglées par poste pendant une lecture de journaux.
- `rev` repart de zéro au redémarrage de l'agent : le client traite un `rev` plus petit comme une remise à zéro.

## Alternatives rejetées
- **Relever `API_VERSION`** : un client récent serait refusé par tout agent ancien et ne pourrait plus le mettre à jour.
- **Journaux dans `/stream`** : un lecteur lent ferait fermer le flux commun, donc tomber le lien.
- **Actes synchrones avec un délai de requête allongé** : la fenêtre de confirmation resterait figée, et toute coupure deviendrait « résultat inconnu ».
- **Signer la liste complète des chemins et des ports** : dépasse la borne d'un paramètre, tronqué en silence par `write_to`.
- **Révélation par `POST`** : une route qui modifie de plus à classer, pour une lecture.

## Mise en oeuvre (HRT-61, tranche 1 : lecture seule)
- `GET /hello` annonce `capabilities: ["services"]` (champ `#[serde(default)]` : absent chez un agent ancien, ignoré par un client ancien ; `API_VERSION` reste 1).
- `Topic::Services` dans `subscribe` ; `ServicesMessage` (`services_snapshot`, `services_delta`, `service_action` déclaré et jamais émis). Un `rev` plus petit que le dernier vu vaut remise à zéro. L'instantané de cette tranche porte `rev`, `engine` et `services` : la liste `deployments` de la conception s'ajoutera, facultative, avec la tranche 5.
- `GET /services` (même contenu que l'instantané) et `POST /engine/recheck` : deux lectures ouvertes aux deux rôles, sans journal ; le second est dans `READ_ONLY_POSTS` (ni acte, ni exception nommée).
- Un abonné lent (`RecvError::Lagged`, file de 64 lots) reçoit un nouvel instantané ; les lots plus anciens qui suivent sont écartés par `rev`.
- Le fil ne porte ni valeur ni nom de variable d'environnement, ni commande (champs absents de `ServiceItem` dans cette tranche ; les noms de variables viendront avec le détail, tranche 2).

## Conséquences
- `docs/open-api/` : `services.md`, `deployments.md`, `service-logs.md`, et `hello.md`, `stream.md` amendés.
- Catalogue du journal : `service.start`, `service.stop`, `service.restart`, `service.deploy`, `service.env.reveal` ; type « Gestion des services » dans le filtre du client (le test de `hearth-link` l'impose).
- Colonne `details` sur `audit_events` (migration `0009`).

## Références
- `crates/hearth-proto/src/{admin_act.rs,stream.rs,version.rs}`, `crates/hearth-agent/src/entrypoint/{http/mod.rs,ws/connection.rs}`, `crates/hearth-link/src/adapters/http_transport.rs`.
