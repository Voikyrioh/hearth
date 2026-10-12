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
Sans `--binary`, `--url` ni `HEARTH_RELEASE_URL`, `deploy/install.sh` lit `$BASE_URL/releases/latest/agent` (`BASE_URL` = `https://hearth.voikyrioh.fr/api`, surchargeable par `HEARTH_BASE_URL`, HTTPS seulement). La lecture est bornée puis mesurée : `curl --max-filesize 512 --max-time 30` (avant curl 8.4, la taille n'est bornée à la lecture que si le serveur l'annonce, sinon par les 30 s), ou `head -c` avec `wget` (une tentative, délai de 15 s par opération ; seul GNU Wget 1.x est pris en charge : `wget --version` doit commencer par « GNU Wget » suivi d'un chiffre, tout autre `wget`, BusyBox et Wget2 compris, arrête l'installation avec « Ce wget n'est pas GNU Wget 1 », avant toute requête). Une réponse trop longue est un pointeur mal formé, quel que soit l'outil. La réponse est UNE ligne, au plus 512 octets, `VERSION SOMME CHEMIN`, trois éléments séparés par une seule espace, et le fichier ne contient que les octets de la liste fermée `[A-Za-z0-9 ._/-]` plus le saut de ligne (ni retour chariot, ni octet nul, ni octet de contrôle) :

- `VERSION` : `X.Y.Z`, chiffres et points seulement ;
- `SOMME` : exactement 64 hexadécimaux ;
- `CHEMIN` : une constante dérivée de la version, exactement `VERSION/hearth-agent-linux-x86_64` (le nom du binaire est une constante du script, `AGENT_FILE`). Les contrôles de forme (`[A-Za-z0-9._/-]`, commence par un chiffre, sans `..`, sans `//`, sans `/` final, 200 caractères au plus) restent en ceinture. Le script affiche « Dernière version : X.Y.Z ».

Tout écart (ligne absente, plusieurs lignes, retour chariot, élément en trop ou en moins, format) arrête l'installation **avant toute écriture** : « Le pointeur de la dernière version est mal formé. », ou « …est absent » (404). Le binaire est ensuite téléchargé à `$BASE_URL/releases/CHEMIN` et sa somme SHA-256 comparée à celle du pointeur ; une somme donnée par `--sha256` doit être identique à celle du pointeur, et reste alors annoncée comme donnée par l'utilisateur. Quand `HEARTH_BASE_URL` n'est pas l'origine par défaut, le script l'annonce avant le premier téléchargement. Rien de ce qui vient du réseau n'est évalué par le shell.

## Application (code)
- `deploy/install.sh::resolve_pointer` (appelée par `main` quand aucune adresse n'est donnée).

## Vérification
- `deploy/e2e/scenario.sh` (`cargo xtask e2e-install`, section « install.sh par défaut ») : pointeur absent, quatre éléments, chemin manquant, version `X.Y`, somme de 63 caractères, `..` en tête et au milieu, `//`, chemin absolu, `;`, autre nom de fichier, autre version, deux lignes, ligne vide en plus, CRLF, octet nul, plus de 512 octets, origine injoignable, somme fausse, origine en clair, `--sha256` différente (refus avant le binaire), `--sha256` identique (acceptée, message « celle que tu as donnée »), installation réussie. Section « Lecture du pointeur sans curl » (HRT-59) : pointeur servi en morceaux sans longueur annoncée avec `curl`, puis, `curl` caché dans le conteneur, avec `wget` GNU : pointeur valide (installation complète), 404, plus de 512 octets et 100 Ko, redirection vers HTTP refusée puis vers HTTPS suivie, corps en morceaux sans longueur annoncée, corps SANS FIN (route `/endless/`, plafond serveur 64 Mio) coupé en moins de 15 s après moins de 8 Mio envoyés ; `wget` BusyBox (applet vérifiée) refusé clairement sans requête ; bannière « GNU Wget2 » simulée refusée. Les refus de redirection vers HTTP (curl comme wget, y compris `HEARTH_RELEASE_URL`) sont prouvés contre un serveur en CLAIR qui sert les mêmes fichiers : une redirection suivie ferait réussir l'installation, et son journal doit rester vide.

## Cas limites
- La somme du pointeur vient de la même origine que le binaire : elle écarte un téléchargement abîmé, pas une origine compromise (le script le dit). Une somme donnée par l'utilisateur avec `--sha256` doit correspondre.
- `--binary` reste la voie vérifiée à la main et n'utilise jamais le pointeur.
- L'absence de saut de ligne final est tolérée (une seule ligne, lue telle quelle).
- Hypothèse de publication : le dossier du binaire porte la version de l'AGENT (`agent.version`), pas celle d'une release du client. L'API n'impose pas encore cette égalité quand elle fabrique la ligne (BR-DIST-006 de `hearth-releases-api`) : un manifeste qui la romprait serait servi par l'API et refusé ici (« mal formé »). C'est la publication (`cargo xtask release-stage`, HRT-51) et l'API qui doivent la tenir.
- La branche `wget` de la lecture bornée est exercée par l'essai de bout en bout (HRT-59) : l'image porte GNU Wget et `busybox`, `curl` y est caché le temps de la section.
- `wget` BusyBox, Wget2 ou tout `wget` dont `--version` ne commence pas par « GNU Wget » et un chiffre n'est pas pris en charge : les options (`-S`, `--max-redirect`, `-t`) varient selon la version (BusyBox), et Wget2 change le format de `-S` et de la ligne d'état, que le script lit. Wget2 n'est PAS essayé dans l'image : le refus est une précaution, pas un constat ; le test n'en simule que la bannière. Un échec muet ou une lecture non bornée est pire qu'un refus net. Le contrôle est dans `download` : il vaut pour tout téléchargement par `wget` (pointeur, binaire, somme), `--url` compris. Le script s'arrête avant toute requête et propose curl, GNU Wget 1 ou `--binary`.

## Règles liées
- BR-INSTALL-006, BR-INSTALL-012

## Historique
- 2026-10-12 : création (HRT-50, session 2026-10-04-hearth-creation).
- 2026-10-12 : revue HRT-59 : Wget2 refusé (motif `^GNU Wget [0-9]`), corps sans fin et redirections vers HTTP prouvés contre un serveur en clair.
- 2026-10-12 : HRT-59, la branche `wget` est testée (curl caché), cas du corps en morceaux ajouté, `wget` autre que GNU refusé clairement.
