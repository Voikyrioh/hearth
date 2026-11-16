# ADR-0061 : Images des jeux du Catalogue : en-tête Steam chargé par la coquille à l'exécution, illustration embarquée pour un jeu absent de Steam

Statut : acceptée (2026-11-16, HRT-238, HRT-239, HRT-240). Le lecteur d'en-tête JPEG vit dans `hearth-proto` (`jpeg.rs`), partagé par `build.rs`, l'agent, la liaison et la coquille : une seule source.


## Contexte

Les maquettes validées le 2026-11-16 (Catalogue en grille, fiche à bandeau) montrent l'image de chaque jeu. Voiky veut les images du magasin Steam, comme les lanceurs Playnite ou Heroic. Hearth ne doit pas redistribuer les visuels des éditeurs : aucune image Steam dans le dépôt, dans le binaire de l'agent ni dans l'installateur du client. Minecraft Java n'est pas sur Steam. La règle du dépôt dit que l'interface web n'ouvre aucune connexion réseau : tout passe par la coquille et `hearth-link`.

Mesures du 2026-11-16 (curl) : pour les dix jeux du catalogue présents sur Steam, `https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/{id}/header.jpg` répond `200 image/jpeg` sans redirection, 37 à 70 Ko, 460 sur 215 pixels, `Cache-Control: public, max-age=315060005`, `ETag`, `Last-Modified` ; un identifiant inconnu répond `404` ; `library_600x900.jpg` existe aussi (portrait).

## Décision

1. **Une fiche présente sur Steam porte `steam_app_id`** (entier, identifiant du JEU dans le magasin, jamais celui du serveur dédié : Valheim = 892970, pas 896660). Le client affiche l'en-tête `header.jpg` de ce jeu, chargé à l'exécution depuis l'hôte `shared.akamai.steamstatic.com`, seul hôte permis.
2. **Le chargement et le cache vivent dans la coquille Rust, pas dans la WebView.** La page affiche `<img src="hearth-illustration://localhost/...">` (protocole d'URI de Tauri, sous Windows `http://hearth-illustration.localhost/...`) ; la coquille répond depuis son cache disque ou va chercher l'image.
3. **Une fiche absente de Steam peut porter une illustration originale embarquée** : champ `illustration = "header.jpg"` dans `fiche.toml` (le nom `image` est impossible : la table `[image]` désigne déjà l'image de conteneur et TOML refuse une clé en double), fichier dans `crates/hearth-agent/catalog/<id>/`, servi par l'agent sur `GET /catalog/{id}/illustration`, lu par `hearth-link`, passé par le même protocole et le même cache. Seule Minecraft Java en a une pour l'instant.
4. **Un réglage du client coupe les images distantes** : « Images des jeux depuis Steam » (activé par défaut). Désactivé, la coquille ne contacte plus jamais Valve, vide la partie Steam de son cache, et la page montre le visuel neutre ; l'illustration embarquée (venue de votre propre serveur) reste affichée.
5. **Repli neutre** pour toute image absente, refusée, hors ligne sans copie, ou coupée par le réglage : un visuel fait de jetons CSS, décoratif, sans texte.

## Comment l'appliquer

- Format de fiche (`domain/catalog/loading/`) : `steam_app_id` facultatif, entier de 1 à 4 294 967 295 (`u32`, jamais une chaîne, jamais 0) ; `illustration` facultatif, nom `[a-z0-9-]+\.jpg` ; une fiche ne porte pas les deux (refus `illustration_and_steam`). `build.rs` embarque l'illustration et refuse : signature autre que `FF D8 FF`, plus de 300 Kio, dimensions autres que 920 sur 430 (le double de l'en-tête Steam, même rapport), fichier `illustration.provenance.toml` absent ou incomplet (`tool`, `made`, `requested_by`, `prompt_ref`, `original = true`). Ni `steam_app_id` ni l'illustration n'entrent dans l'empreinte de la fiche : changer d'image ne déclenche jamais `FICHE_CHANGED`.
- API : `FicheCard` gagne `steam_app_id?: u32` et `illustration?: { digest, bytes, width, height, media_type }`. `GET /catalog/{id}/illustration` : `Access::Authenticated` (même garde que `/catalog`), non journalisé, `image/jpeg`, `nosniff`, `ETag` = empreinte, `Cache-Control: private, max-age=31536000, immutable`, `404 FICHE_UNKNOWN` ou `404 ILLUSTRATION_ABSENT`. Contrat complet : `docs/open-api/catalog.md`.
- Coquille, protocole (`src-tauri/src/illustrations/`) : deux formes d'URL, validées composant par composant avant toute E/S :
  - `hearth-illustration://localhost/steam/{app_id}/header` : `app_id` = chiffres seulement, 1 à 10 caractères, converti en `u32` non nul ; autre chose : `400`, rien ne part.
  - `hearth-illustration://localhost/agent/{server_id}/{fiche_id}/{hex}` : serveur du carnet, identifiant stable (`hearth_link::manager::catalog::is_stable_id`), `hex` = 64 caractères `[0-9a-f]`.
  - Méthode autre que GET : `405`. Réponse : `200 image/jpeg` avec `X-Content-Type-Options: nosniff`, ou `404` sans corps pour tout échec (la page passe au repli).
- Coquille, client HTTP vers Steam : un `reqwest::Client` dédié, HTTPS seulement, **aucune redirection suivie** (`redirect::Policy::none()` ; une `3xx` est un échec), hôte fixe compilé (`shared.akamai.steamstatic.com`), URL construite par la coquille depuis l'entier validé (jamais depuis un texte reçu), aucun cookie, en-tête `User-Agent: Hearth/{version}`, délai 10 s, deux lectures Steam en même temps au plus, corps lu en flux et coupé au-delà de 512 Kio, `Content-Type` `image/jpeg` exigé, signature JPEG vérifiée, dimensions lues dans l'en-tête et bornées (64 à 2048 pixels de côté).
- Coquille, lecture embarquée : `LinkManager::read_fiche_illustration(server, fiche, digest)` (`hearth-link`), GET binaire borné à 512 Kio, `image/jpeg` exigé, signature JPEG, SHA-256 égal à l'empreinte annoncée par la liste.
- Coquille, cache : dossier `catalog-illustrations/` du dossier de données local de l'application ; `steam/{app_id}-header.jpg` (+ `.meta` : `ETag`, `Last-Modified`, date de lecture) et `agent/{hex}.jpg` (adressé par contenu, partagé entre serveurs). Écriture atomique (temporaire puis renommage). Steam : copie servie telle quelle si elle a moins de 30 jours ; au-delà, relecture conditionnelle (`If-None-Match`, `If-Modified-Since`) et, si Steam ne répond pas, copie ancienne servie (hors ligne). Agent : empreinte revérifiée à la lecture du cache (fichier altéré supprimé puis relu). Plafond 32 Mio, purge des moins récemment servis.
- CSP de `tauri.conf.json` (`csp` et `devCsp`) : `img-src 'self' hearth-illustration: http://hearth-illustration.localhost`. **Aucun hôte Steam dans la CSP** et jamais `data:` en production : la WebView ne peut joindre ni Valve ni aucun autre hôte, même si une page était détournée.
- Pont TypeScript : `LinkBridge.steamHeaderUrl(appId: number): string` et `LinkBridge.agentIllustrationUrl(serverId, ficheId, digest): string` (purs, aucune E/S) ; en mode simulé : fixtures servies par Vite (aucune image Steam versionnée : les fixtures sont des images neutres générées pour les tests).
- Réglage : stocké dans les réglages du client (`src-tauri/src/settings.rs`), lu par le protocole à CHAQUE requête Steam (la page ne fait pas foi) ; carte dans la page Réglages (`SettingRow` + `HToggle`).
- Vie privée, à écrire dans `SECURITY.md` et sous le réglage : « Pour afficher l'image d'un jeu présent sur Steam, Hearth la demande au serveur d'images de Steam (Valve). Ce serveur voit l'adresse internet de ce PC et l'identifiant du jeu demandé, rien d'autre : ni compte, ni cookie, ni nom de serveur. Les images sont gardées sur ce PC pour ne pas être redemandées. Désactivez ce réglage pour ne plus jamais contacter Steam : un visuel neutre s'affiche à la place. »

## Quand NE PAS l'appliquer / limites

- Jamais d'image d'éditeur copiée dans le dépôt, les fixtures, le binaire de l'agent ou l'installateur ; jamais d'autre hôte que celui de la décision sans nouvelle ADR.
- L'agent ne télécharge jamais d'image (il peut tourner sans Internet ; son rôle est de servir ses fiches).
- Pas de portrait `library_600x900.jpg` dans ce lot (aucun écran ne l'utilise) ; l'ajouter = ajouter le genre `library` à la validation de l'URL, rien d'autre.
- Limite connue : Steam publie parfois les images récentes d'un jeu sous une adresse à empreinte (`.../apps/{id}/{hash}/header.jpg`) ; si l'adresse sans empreinte cessait de répondre ou redirigeait, la carte passe au repli neutre (aucune redirection suivie). Contournement possible plus tard : relire l'adresse dans l'API du magasin, autre hôte, nouvelle ADR.
- Limite connue : l'en-tête fait 460 sur 215 pixels ; le bandeau de la fiche l'affiche à sa taille, centré, sur un fond fait de la même image floutée et assombrie (CSS), jamais étiré au-delà de deux fois sa taille.
- Une illustration embarquée doit être originale (aucun logo, personnage officiel, visuel publicitaire, lettrage du nom du jeu) ; sa provenance est obligatoire.

## Alternatives rejetées

- **Chargement direct par la WebView avec la CSP ouverte sur l'hôte Steam** (`img-src https://shared.akamai.steamstatic.com`) : moins de code, cache du moteur de rendu, et la CSP bloque une redirection vers un autre hôte ; rejeté parce que la page ouvrirait une connexion réseau (règle du dépôt), que le réglage ne serait appliqué que par l'interface (une page modifiée l'ignorerait), que le cache dépend du profil WebView2 (vidé avec lui, durée non maîtrisée hors ligne) et que deux mécanismes coexisteraient (Steam par la WebView, Minecraft par la coquille).
- **Illustrations originales embarquées pour toutes les fiches** (version 1 de cette ADR) : abandonné par décision de Voiky (2026-11-16) ; gardé pour le seul jeu absent de Steam.
- **Copier les en-têtes Steam dans le dépôt ou l'agent** : redistribution de visuels d'éditeur, interdit.
- **URL `data:` ou `blob:` par une commande** : base64 de l'image dans l'IPC et le DOM, CSP élargie à `data:`.
- **Agent qui télécharge et sert les images Steam** : l'agent contacterait Valve depuis le serveur de l'utilisateur, sans réglage côté client, et deviendrait un relais d'Internet.

## Conséquences

- Code : domaine du catalogue (deux champs, refus typés, lecteur d'en-tête JPEG partagé avec la coquille si possible, sinon dupliqué et testé des deux côtés), `build.rs`, `application/catalog.rs`, route HTTP, `hearth-proto`, `hearth-link` (GET binaire borné), coquille (`illustrations/` : protocole, client Steam, cache, réglage), CSP, pont, atome `FicheVisual`.
- Tests : voir la conception technique (refus du chargeur, route, liaison, URL du protocole, aucune redirection suivie, réglage coupé = aucune requête, cache altéré, CSP sans hôte externe ni `data:`).
- Doc : `SECURITY.md` (contact de Valve, réglage), `docs/open-api/catalog.md`, `ADR-0050` (historique : deux champs), `ADR-0010` (commande ou protocole de la coquille), fiches `BR-FICHE-003`, `BR-FICHE-062`, `BR-CLIENT-015`.
- Revue de sécurité de Stephen sur HRT-238, HRT-239, HRT-240.

## Références

- Maquettes : `contexts/hearth/conceptions/2026-11-16-catalogue-et-ajout-de-serveur/maquettes/` (hub).
- Tauri 2, protocoles d'URI personnalisés : `Builder::register_asynchronous_uri_scheme_protocol` ; CSP : `app.security.csp`.
- reqwest, redirections : `reqwest::redirect::Policy::none()`.
- Mesures Steam du 2026-11-16 : section Contexte.
