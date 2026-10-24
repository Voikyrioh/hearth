# API — Hearth

Endpoints `/api/v1` (JSON, HTTPS). Authentification par Bearer token en en-tête `Authorization: Bearer …`.

**Actes d'administration** (HRT-28) : les routes qui modifient (hors déconnexion et hors retrait d'un poste, qui garde son contrat) acceptent un membre `reauth` (mot de passe + preuve de clé d'usage `0x05`) ; l'agent l'EXIGE depuis HRT-30 (`GET /security` : `admin_reauth.required: true`) ; sans `reauth`, `426 INCOMPATIBLE_VERSION` « client trop ancien ». Voir [sécurité](./security.md).

47 endpoints : session, postes de confiance, sécurité, comptes, mesures, services (moteur de conteneurs), audit, mises à jour.

| Groupe | Méthode | Route | Rôle | Description |
|---|---|---|---|---|
| **Session** | GET | [`/hello`](./hello.md) | — | Identité de l'agent, plage de versions (l'empreinte se lit sur le certificat TLS) |
| **Session** | POST | [`/sessions`](./sessions.md) | — | Login (username + password, preuve de clé d'appareil optionnelle) → token |
| **Session** | POST | [`/sessions/challenge`](./sessions.md) | — | Défi pour la clé d'appareil (sans état, 60 s, identique pour tout identifiant) |
| **Session** | DELETE | [`/sessions/current`](./sessions.md) | user+ | Logout |
| **Session** | GET | [`/me`](./sessions.md) | user+ | Compte courant + rôle |
| **Session** | GET | [`/operations/{id}`](./operations.md) | user+ | Statut opération idempotente |
| **Postes** | GET | [`/me/devices`](./devices.md) | user+ | Mes postes de confiance (nom, dates, dernière adresse, poste courant) |
| **Postes** | DELETE | [`/me/devices/{id}`](./devices.md) | user+ | Retirer un poste (sa clé, son adresse retenue, ses sessions) |
| **Sécurité** | GET | [`/security`](./security.md) | user+ | État de sécurité du compte : alerte « attaque probable » (`own`, `since`, `others` pour un administrateur), mode attaque (`off`, `active`, `suspended` avec `resumes_in_s`, `last_end`) |
| **Sécurité** | PUT | [`/security/attack-mode`](./security.md) | admin | Activer ou désactiver le mode attaque : acte confirmé (membre `reauth` : mot de passe actuel et preuve d'une clé inscrite du compte, usage `0x05`) ; `426` sans `reauth`, `409 POST_NOT_RECOGNIZED` sans preuve valide |
| **Sécurité** | PUT | [`/me/reauth`](./security.md) | user+ | Réglage de la fréquence du mot de passe en administration (`window` : 5 minutes, ou `each`) ; toujours avec mot de passe et clé (HRT-28) |
| **Mesures** | GET | [`/machine`](./machine.md) | user+ | Identité machine, specs hardware |
| **Mesures** | GET | [`/stream`](./stream.md) | user+ (jeton dans le premier message) | WebSocket flux temps réel (snapshot + metrics chaque seconde) |
| **Mesures** | GET | [`/metrics/history?window=1m\|5m\|1h`](./metrics.md) | user+ | Historique rééchantillonné |
| **Services** | GET | [`/services`](./services.md) | user+ | Moteur de conteneurs (état en huit valeurs, nom, version, compte de l'agent, hôte) et les services publiés (créés par Hearth ou adoptés, HRT-79), arrêtés compris, triés par nom, les autres conteneurs comptés (`external_count`) ; lecture seule, détail (noms de variables, commande, délai d'arrêt, unité systemd), aucune valeur de variable |
| **Services** | GET | [`/services/{key}/env`](./services.md) | admin | Valeurs des variables d'un service (HRT-63, BR-SVC-009) : refus d'un compte en lecture seule et réussite consignés (`service.env.reveal`) sans nom ni valeur, `Cache-Control: no-store` |
| **Services** | GET | [`/services/{key}/logs`](./services.md) | user+ (jeton dans le premier message) ; service adopté : droit de gérer les services ; conteneur non adopté : réponse d'une clé inconnue | WebSocket d'une lecture de journaux (HRT-66, HRT-79) : 1 000 dernières lignes puis le direct, lots de 100 ms, anneau borné, perte comptée, reprise par curseur, dix lectures au plus ; lire n'est pas journalisé |
| **Services** | POST | [`/deployments`](./services.md) | droit « gérer les services » + acte d'administration `0x0F` (mot de passe à chaque fois, jamais l'élévation) | Déploie une image en service (HRT-68, ADR-0039) : plan fermé, limites de mémoire et de processeur obligatoires, `202 { operation_id, service_key }`, issue dans l'opération ; refus synchrones typés (`NAME_TAKEN`, `PORT_IN_USE`, `LIMITS_REQUIRED`, `PLAN_SETTING_REFUSED`, `DEPLOY_LIMIT`…) |
| **Services** | POST | [`/deployments/check`](./services.md) | droit « gérer les services » | Constats du formulaire (nom, ports, dossiers, disque, image présente), sans secret, sans écriture, non journalisée (BR-DEPLOY-033) |
| **Services** | GET | [`/engine/capacity`](./services.md) | user+ | Bornes de la machine (processeurs, mémoire, réserve) et limites déjà engagées, en totaux (HRT-68) |
| **Services** | GET | [`/catalog`](./catalog.md) | user+ | Les fiches de serveurs de jeu que l'agent propose (HRT-101, ADR-0050) : carte de chaque fiche, fiches écartées au chargement (dossier et invariant). Une fiche `preview` n'y est que dans une construction d'essai |
| **Services** | GET | [`/services/{key}/delete-preview`](./service-delete.md) | services | La fenêtre de suppression (HRT-85) : dossiers gérés par Hearth, taille mesurée en 10 s, sort de chacun (effaçable, partagé, non vérifiable), dossiers du serveur et volumes conservés |
| **Services** | DELETE | [`/services/{key}`](./service-delete.md) | services + acte `0x14` | Supprime un service créé par Hearth (HRT-85) : conteneur sans ses volumes, description et valeurs scellées ; les données ne partent que sur demande (nom retapé), après le conteneur, à faible priorité ; `202 { operation_id }` |
| **Services** | GET | [`/services/{key}/game-secret`](./game-services.md) | services | Le mot de passe du serveur de jeu, lu dans la description chiffrée (HRT-103, BR-JEU-025) : réussite consignée AVANT la valeur (`service.game.password.reveal`), refus faute du droit consigné, `Cache-Control: no-store`, absent si le serveur n'en a pas |
| **Services** | GET | [`/services/{key}/settings`](./game-settings.md) | services | Les valeurs voulues des réglages d'un serveur de jeu, l'état du fichier, les écarts avec le fichier du jeu et les réglages que la fiche ne connaît pas ; jamais la valeur d'un secret (HRT-108) |
| **Services** | POST | [`/services/{key}/settings/analyze`](./game-settings.md) | services | Lit un texte collé et juge chaque réglage ; n'écrit rien, ne garde rien (HRT-108) |
| **Services** | PUT | [`/services/{key}/settings`](./game-settings.md) | services + acte `0x1E` | Enregistre des réglages : fichier retouché sans perte, écart tranché réglage par réglage, effet signé ; ne redémarre rien (HRT-108) |
| **Services** | GET | [`/catalog/{id}`](./catalog.md) | user+ | Détail d'une fiche : empreinte, image, ports, dossiers gérés, contrats, questions, ressources avec leur preuve (HRT-101) ; `404 FICHE_UNKNOWN` si l'agent ne la propose pas |
| **Services** | GET | [`/services/external`](./services.md) | droit « gérer les services » | Les conteneurs non gérés que ce compte peut choisir d'adopter (HRT-80, BR-ADOPT-005, 006) : clé, nom, image, unité, état, rien d'autre ; jamais sur le flux, jamais gardée ; lecture réussie et refus consignés (`service.external.read`, sans nom, regroupés) |
| **Services** | POST | [`/services/adopt`](./services.md) | droit « gérer les services » + acte d'administration `0x11` (mot de passe à chaque fois, jamais l'élévation) | Adopte 1 à 50 conteneurs, un résultat par clé (HRT-80, BR-ADOPT-008 à 010) ; une ligne dans la base de l'agent, rien d'écrit sur le service ; une entrée de journal par service (`service.adopt`) |
| **Services** | POST | [`/services/{key}/release`](./services.md) | droit « gérer les services » + acte d'administration `0x12` (couvert par l'élévation) | « Ne plus gérer » (HRT-80, BR-ADOPT-013) : le service sort de la liste de tous, sans effet ni entrée si déjà relâché ; journal `service.release` |
| **Services** | POST | [`/services/{key}/start`](./services.md) | droit « gérer les services » + confirmation | Démarrer un service publié (HRT-64, acte `0x0C`, acte de pilotage : le mot de passe suit l'état de confiance, HRT-194) : `202 { action_id, grace_s }`, refus synchrones typés (`ENGINE_UNAVAILABLE`, `SERVICE_NOT_FOUND`, `SERVICE_BUSY`, `SERVICE_ALREADY_IN_STATE`, `SERVICE_NOT_DRIVABLE`), issue dans l'opération, consigné (`service.start`) |
| **Services** | POST | [`/services/{key}/stop`](./services.md) | droit « gérer les services » + confirmation | Arrêter un service publié (HRT-64, acte `0x0D`) : délai de grâce du service, arrêt voulu écrit avant l'ordre, arrêt forcé dit, un service géré par le système s'arrête par son unité ; consigné (`service.stop`) |
| **Services** | POST | [`/services/{key}/restart`](./services.md) | droit « gérer les services » + confirmation | Redémarrer un service en marche (HRT-64, acte `0x0E`) : un seul ordre, « Arrêt en cours » puis « Démarrage » ; jamais converti en démarrage ; consigné (`service.restart`) |
| **Services** | GET | [`/operations/services/{id}`](./service-ops.md) | user+ | Une opération de service (HRT-144, ADR-0045) : genre, statut, étapes, progression, décision offerte, cause ; 24 heures après sa fin ; une opération sur un service que le compte ne voit pas répond comme une inconnue (`404 OPERATION_NOT_FOUND`, BR-OPS-001) ; lecture non journalisée |
| **Services** | DELETE | [`/operations/services/{id}`](./service-ops.md) | droit « gérer les services » + acte d'administration `0x10` (couvert par l'élévation, défaut non validé) | Annule une opération avant son point de non-retour (`TOO_LATE_TO_CANCEL` après), idempotent, consigné sous l'action du genre avec l'auteur (BR-OPS-002) |
| **Services** | POST | [`/operations/services/{id}/decision`](./service-ops.md) | droit « gérer les services » + acte d'administration `0x1D` (acte de pilotage : le mot de passe suit l'état de confiance, HRT-194 ; le choix est signé) | Décide d'une opération qui attend : parmi les choix offerts, avant l'expiration de 15 minutes (`DECISION_NOT_OFFERED` sinon, BR-OPS-003) |
| **Services** | POST | [`/engine/recheck`](./services.md) | user+ | Relance la détection du moteur et rend son état : un `POST` qui ne modifie rien (`READ_ONLY_POSTS`) |
| **Comptes** | GET | [`/accounts`](./accounts.md) | admin | Liste comptes, sessions ouvertes, dernier login |
| **Comptes** | POST | [`/accounts`](./accounts.md) | admin | Créer compte (username, password, role) |
| **Comptes** | PATCH | [`/accounts/{id}`](./accounts.md) | admin | Modifier rôle |
| **Comptes** | PUT | [`/accounts/{id}/password`](./accounts.md) | admin | Changer mot de passe (ferme sessions compte) |
| **Comptes** | PUT | [`/me/password`](./accounts.md) | user+ | Changer son mot de passe (ferme autres sessions) |
| **Comptes** | DELETE | [`/accounts/{id}`](./accounts.md) | admin | Supprimer compte (refusé sur un titulaire du droit sauf par un titulaire) |
| **Comptes** | DELETE | [`/accounts/{id}/sessions`](./accounts.md) | admin | Fermer toutes les sessions du compte |
| **Audit** | GET | [`/audit?filters…`](./audit.md) | admin | Journal d'activité (filtres, recherche plein texte, pagination curseur) |
| **Audit** | GET | [`/audit/export?filters…`](./audit.md) | admin | Journal CSV (UTF-8 BOM, `;` séparateur) |
| **Mise à jour** | GET | [`/agent/update`](./agent-update.md) | user+ | Statut mise à jour (version, installation gérée, en cours, dernier résultat) |
| **Mise à jour** | GET | [`/agent/update/last`](./agent-update.md) | user+ | Dernier résultat (survit au redémarrage de l'agent) |
| **Mise à jour** | POST | [`/agent/update`](./agent-update.md) | admin | Lancer la mise à jour (version, URL HTTPS, signature minisign, somme SHA-256) : `202`, suivi par le flux |
| **CLI** | `hearth-agent install` | — | — | Installation interactive (env var override) |
| **CLI** | `hearth-agent uninstall` | — | — | Désinstallation |
| **CLI** | `hearth-agent account` | — | — | Sous-cmds : add, list, passwd, role, remove, revoke |
| **CLI** | `hearth-agent attack-mode` | — | — | Sous-cmds : status, off (voie de secours sans réseau, root) ; pas d'activation |
| **CLI** | `hearth-agent fingerprint` | — | — | Affiche empreinte certificat |

## Codes d'erreur transverses

- `401 UNAUTHENTICATED` : pas de token, ou token illisible
- `401 INVALID_CREDENTIALS` : identifiant ou mot de passe refusé, sans préciser lequel
- `401 SESSION_EXPIRED` : token expiré
- `401 SESSION_REVOKED` : session révoquée
- `403 FORBIDDEN_ROLE` : rôle insuffisant
- `409 OPERATION_IN_PROGRESS` : opération en cours (ex. mise à jour)
- `409 MANAGED_INSTALL` : installation gérée par le système, pas de mise à jour à distance ; `422 BAD_SIGNATURE` : signature de mise à jour refusée : voir [mise à jour de l'agent](./agent-update.md)
- `422 VALIDATION_ERROR` : paramètre invalide (détails champ)
- `426 INCOMPATIBLE_VERSION` : client/agent incompatibles
- `429 TOO_MANY_ATTEMPTS` : trop de tentatives (login verrouillé), `details.retry_after_s`
- `409 USERNAME_TAKEN`, `409 LAST_ADMIN`, `409 CONFLICT` ; `422 WEAK_PASSWORD` (`details.rules`), `422 WRONG_PASSWORD` : voir [comptes](./accounts.md)
- `409 POST_NOT_RECOGNIZED` : activer ou désactiver le mode attaque sans preuve valide d'une clé inscrite pour le compte appelant (`details.reason`) : voir [sécurité](./security.md)
- `422 IDEMPOTENCY_KEY_REUSED` : la clé d'opération a déjà servi pour une autre requête
- `413 PAYLOAD_TOO_LARGE` : corps d'une requête suivie au-delà de 1 Mio
- `503 BUSY` : agent saturé (calculs de mots de passe), en-tête `Retry-After`
- `500 INTERNAL_ERROR` : erreur serveur
- Opérations de service (HRT-144) : `404 OPERATION_NOT_FOUND` (inconnue, purgée, ou sur un service que le compte ne voit pas, sans distinction), `409 TOO_LATE_TO_CANCEL`, `409 DECISION_NOT_OFFERED` (`details.reason`) : voir [opérations de service](./service-ops.md). Les autres codes des tranches L1 à L3 sont déclarés (`hearth-proto::error::ErrorCode`) sans route encore : `ENGINE_UNAVAILABLE` 503, `SERVICE_NOT_FOUND` 404, `SERVICE_BUSY`, `SERVICE_ALREADY_IN_STATE`, `SERVICE_NOT_DRIVABLE`, `NAME_TAKEN`, `PORT_IN_USE`, `DEPLOY_LIMIT`, `FICHE_CHANGED`, `SETTINGS_CONFLICT`, `SETTINGS_FILE_CHANGED`, `RECREATE_NOT_AVAILABLE` 409 (utilisés par les [réglages d'un jeu](./game-settings.md), HRT-108, avec `SETTING_REFUSED` et `SETTINGS_FILE_UNREADABLE` 422), `FOLDER_REFUSED`, `LIMITS_REQUIRED`, `AGREEMENT_REQUIRED`, `SETTING_REFUSED` 422, `BACKUP_NOT_FOUND` 404 ; `FICHE_UNKNOWN` (404) tient lieu de « fiche inconnue ». Un code qu'un client ne connaît pas se lit `UNKNOWN` (jamais une erreur de lecture) : le statut HTTP dit l'essentiel.

## Conventions

- Idempotency : requêtes modifiantes portent `Idempotency-Key: <ULID>` (rejouer = même résultat, sans ré-exécuter ; en cours = `409` ; autre requête = `422 IDEMPOTENCY_KEY_REUSED`) : voir [opérations](./operations.md). `POST /sessions` n'est pas suivi (son résultat contient un jeton).
- Versioning : client envoie `X-Hearth-Api: <n>` (obligatoire sauf sur `/hello`) ; hors plage `426 INCOMPATIBLE_VERSION` avec `details.upgrade` ; l'agent répond `X-Hearth-Api-Range: min-max`.
- Client : `X-Hearth-Client: poste/version` (nom du poste, retenu avec la session).
- Accès : chaque route est déclarée dans `ENDPOINTS` (`entrypoint/http/mod.rs`) avec son niveau (public, authentifié, administrateur, ou « premier message » pour le flux, qui s'authentifie lui-même) son suivi par clé et son action de journal (les refus et les échecs sont consignés par la couche d'accès) ; le routeur pose la couche d'accès depuis la table et un test de balayage vérifie que les routes réservées refusent l'appelant sans droit.
- Réponses : succès = corps JSON propre à la route, sans enveloppe (ex. `/hello`) ; échec = `{ error: { code, message, details } }` (`hearth-proto::error::ErrorBody`).
- Fiches de route détaillées : [mise à jour de l'agent](./agent-update.md), [hello](./hello.md), [moteur de conteneurs et services](./services.md), [opérations de service](./service-ops.md), [sessions](./sessions.md), [postes de confiance](./devices.md), [sécurité](./security.md), [comptes](./accounts.md), [opérations](./operations.md), [machine](./machine.md), [historique des mesures](./metrics.md), [flux temps réel](./stream.md), [journal](./audit.md).
