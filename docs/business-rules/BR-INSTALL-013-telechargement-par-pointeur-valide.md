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
Sans `--binary`, `--url` ni `HEARTH_RELEASE_URL`, `deploy/install.sh` lit `$BASE_URL/releases/latest/agent` (`BASE_URL` = `https://hearth.voikyrioh.fr/api`, surchargeable par `HEARTH_BASE_URL`, HTTPS seulement). La lecture est bornée (512 octets, 30 s : `curl --max-filesize`, ou `head -c` avec `wget`) puis mesurée. La réponse est UNE ligne, au plus 512 octets, `VERSION SOMME CHEMIN`, trois éléments séparés par une seule espace, et le fichier ne contient que les octets de la liste fermée `[A-Za-z0-9 ._/-]` plus le saut de ligne (ni retour chariot, ni octet nul, ni octet de contrôle) :

- `VERSION` : `X.Y.Z`, chiffres et points seulement ;
- `SOMME` : exactement 64 hexadécimaux ;
- `CHEMIN` : une constante dérivée de la version, exactement `VERSION/hearth-agent-linux-x86_64` (le nom du binaire est une constante du script, `AGENT_FILE`). Les contrôles de forme (`[A-Za-z0-9._/-]`, commence par un chiffre, sans `..`, sans `//`, sans `/` final, 200 caractères au plus) restent en ceinture. Le script affiche « Dernière version : X.Y.Z ».

Tout écart (ligne absente, plusieurs lignes, retour chariot, élément en trop ou en moins, format) arrête l'installation **avant toute écriture** : « Le pointeur de la dernière version est mal formé. », ou « …est absent » (404). Le binaire est ensuite téléchargé à `$BASE_URL/releases/CHEMIN` et sa somme SHA-256 comparée à celle du pointeur ; une somme donnée par `--sha256` doit être identique à celle du pointeur, et reste alors annoncée comme donnée par l'utilisateur. Quand `HEARTH_BASE_URL` n'est pas l'origine par défaut, le script l'annonce avant le premier téléchargement. Rien de ce qui vient du réseau n'est évalué par le shell.

## Application (code)
- `deploy/install.sh::resolve_pointer` (appelée par `main` quand aucune adresse n'est donnée).

## Vérification
- `deploy/e2e/scenario.sh` (`cargo xtask e2e-install`, section « install.sh par défaut ») : pointeur absent, quatre éléments, chemin manquant, version `X.Y`, somme de 63 caractères, `..` en tête et au milieu, `//`, chemin absolu, `;`, autre nom de fichier, autre version, deux lignes, CRLF, plus de 512 octets, origine injoignable, somme fausse, origine en clair, `--sha256` différente (refus avant le binaire), `--sha256` identique (acceptée, message « celle que tu as donnée »), installation réussie.

## Cas limites
- La somme du pointeur vient de la même origine que le binaire : elle écarte un téléchargement abîmé, pas une origine compromise (le script le dit). Une somme donnée par l'utilisateur avec `--sha256` doit correspondre.
- `--binary` reste la voie vérifiée à la main et n'utilise jamais le pointeur.
- L'absence de saut de ligne final est tolérée (une seule ligne, lue telle quelle).

## Règles liées
- BR-INSTALL-006, BR-INSTALL-012

## Historique
- 2026-10-12 : création (HRT-50, session 2026-10-04-hearth-creation).
