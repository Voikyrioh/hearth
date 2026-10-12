---
id: BR-INSTALL-013
domaine: INSTALL
titre: Le téléchargement par défaut passe par un pointeur validé strictement
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-09-distribution-scaleway.md (sections 5.2, 5.5, 10.2), HRT-50
maj: 2026-10-12
---

# BR-INSTALL-013 : Le téléchargement par défaut passe par un pointeur validé strictement

## Règle
Sans `--binary`, `--url` ni `HEARTH_RELEASE_URL`, `deploy/install.sh` lit `$BASE_URL/releases/latest/agent` (`BASE_URL` = `https://hearth.voikyrioh.fr/api`, surchargeable par `HEARTH_BASE_URL`, HTTPS seulement). La réponse est UNE ligne, au plus 512 octets, `VERSION SOMME CHEMIN`, trois éléments séparés par une seule espace :

- `VERSION` : `X.Y.Z`, chiffres et points seulement ;
- `SOMME` : exactement 64 hexadécimaux ;
- `CHEMIN` : caractères `[A-Za-z0-9._/-]`, commence par un chiffre, sans `..`, sans `//`, sans `/` final.

Tout écart (ligne absente, plusieurs lignes, retour chariot, élément en trop ou en moins, format) arrête l'installation **avant toute écriture** : « Le pointeur de la dernière version est mal formé. », ou « …est absent » (404). Le binaire est ensuite téléchargé à `$BASE_URL/releases/CHEMIN` et sa somme SHA-256 comparée à celle du pointeur ; une somme donnée par `--sha256` doit être identique à celle du pointeur. Rien de ce qui vient du réseau n'est évalué par le shell.

## Application (code)
- `deploy/install.sh::resolve_pointer` (appelée par `main` quand aucune adresse n'est donnée).

## Vérification
- `deploy/e2e/scenario.sh` (`cargo xtask e2e-install`, section « install.sh par défaut ») : pointeur absent, quatre éléments, chemin manquant, version `X.Y`, somme de 63 caractères, `..` en tête et au milieu, `//`, chemin absolu, `;`, somme fausse, origine en clair, installation réussie.
- Essais du validateur seul (`sh`, `download` simulé) : valide, CRLF, `..`, `//`, caractères interdits.

## Cas limites
- La somme du pointeur vient de la même origine que le binaire : elle écarte un téléchargement abîmé, pas une origine compromise (le script le dit). Une somme donnée par l'utilisateur avec `--sha256` doit correspondre.
- `--binary` reste la voie vérifiée à la main et n'utilise jamais le pointeur.

## Règles liées
- BR-INSTALL-006, BR-INSTALL-012

## Historique
- 2026-10-12 : création (HRT-50, session 2026-10-04-hearth-creation).
