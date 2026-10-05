#!/bin/sh
# Installe l'agent Hearth sur ce serveur Linux (x86_64), en une commande.
#
# Tout le script est dans la fonction `main`, appelée à la toute dernière ligne : lu depuis un
# tube (`curl ... | sh`), un téléchargement tronqué n'exécute rien.
#
# POSIX sh : pas de bashisme.

set -eu

REPO="Voikyrioh/hearth"

BINARY=""
URL="${HEARTH_RELEASE_URL:-}"
SHA256="${HEARTH_SHA256:-}"
WORKDIR=""

say() {
    printf '%s\n' "$*"
}

fail() {
    printf '%s\n' "$*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Installe l'agent Hearth sur ce serveur Linux (x86_64 seulement pour l'instant).

Utilisation :
  sudo sh install.sh --binary ./hearth-agent
  sudo sh install.sh --url https://exemple.org/hearth-agent --sha256 SOMME
  curl -fsSL https://exemple.org/install.sh | sudo sh -s -- --url https://exemple.org/hearth-agent

Options du script (les autres, par exemple --port 7342, --managed, --yes, sont transmises
telles quelles à `hearth-agent install`) :
  --binary CHEMIN   utilise ce fichier local (sa somme n'est vérifiée que si --sha256 est donnée)
  --url ADRESSE     télécharge le binaire à cette adresse HTTPS (ou HEARTH_RELEASE_URL)
  --sha256 SOMME    somme SHA-256 attendue (ou HEARTH_SHA256). Pour un téléchargement, elle est
                    OBLIGATOIRE : donnée ici, ou publiée à côté du binaire (ADRESSE.sha256, en
                    HTTPS depuis la même origine). Sans somme, rien n'est téléchargé.
  --help            affiche cette aide

Mot de passe du premier compte : jamais sur une ligne de commande. Trois voies :
  1. interactive : le script pose les questions, la saisie est sans écho ;
  2. HEARTH_ADMIN_PASSWORD_HASH (haché fabriqué par `hearth-agent hash-password --user NOM`)
     avec HEARTH_ADMIN_USER, transmis par l'environnement préservé de sudo
     (`sudo --preserve-env=HEARTH_ADMIN_USER,HEARTH_ADMIN_PASSWORD_HASH sh install.sh ...`) ;
  3. HEARTH_ADMIN_PASSWORD lu au clavier (`read -rs`), exporté, puis transmis de la même façon.
Autres variables : HEARTH_PORT, HEARTH_MANAGED.
EOF
}

# shellcheck disable=SC2329 # appelée par `trap`
cleanup() {
    if [ -n "$WORKDIR" ] && [ -d "$WORKDIR" ]; then
        rm -rf "$WORKDIR"
    fi
}

# Retire le dossier temporaire à la sortie, et aussi sur Ctrl+C, arrêt ou fin de session : rien
# ne reste.
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d ' ' -f 1
    else
        fail "Aucun outil de somme SHA-256 (sha256sum ou shasum) : vérification impossible."
    fi
}

# Dossier temporaire créé de façon sûre (mktemp, droits 0700), où l'on peut exécuter : un /tmp
# monté sans droit d'exécution est contourné.
make_workdir() {
    for base in "${TMPDIR:-/tmp}" /var/tmp /root /usr/local/lib; do
        [ -d "$base" ] && [ -w "$base" ] || continue
        candidate="$(mktemp -d "$base/hearth-install.XXXXXX" 2>/dev/null)" || continue
        printf '#!/bin/sh\nexit 0\n' >"$candidate/probe"
        chmod 0700 "$candidate/probe"
        if "$candidate/probe" >/dev/null 2>&1; then
            WORKDIR="$candidate"
            return 0
        fi
        rm -rf "$candidate"
    done
    fail "Aucun dossier temporaire où exécuter le binaire (/tmp, /var/tmp, /root et /usr/local/lib sont absents, protégés ou montés sans droit d'exécution). Fournis un binaire avec --binary : il sera lancé depuis sa place."
}

# Téléchargement en HTTPS seulement, redirections comprises et bornées. Rend 0 si abouti, 2 si
# l'adresse n'existe pas (404), 1 sinon.
download() {
    case "$1" in
        https://*) ;;
        *) fail "L'adresse $1 n'est pas en HTTPS : un binaire lancé en root ne se télécharge pas en clair. Aucune modification n'a été apportée à ta machine." ;;
    esac
    if command -v curl >/dev/null 2>&1; then
        code="$(curl -sSL --proto '=https' --proto-redir '=https' --max-redirs 5 --tlsv1.2 \
            --connect-timeout 15 --max-time 600 -o "$2" -w '%{http_code}' "$1" 2>/dev/null)" || return 1
        case "$code" in
            200) return 0 ;;
            404) return 2 ;;
            *) return 1 ;;
        esac
    elif command -v wget >/dev/null 2>&1; then
        wget -q --https-only --max-redirect=5 -T 15 -O "$2" "$1" 2>/dev/null || return 1
        return 0
    fi
    fail "Ni curl ni wget : téléchargement impossible. Fournis un binaire avec --binary."
}

main() {
    FORWARD=""
    while [ $# -gt 0 ]; do
        case "$1" in
            --binary)
                [ $# -ge 2 ] || fail "L'option --binary attend un chemin."
                BINARY="$2"
                shift 2
                ;;
            --url)
                [ $# -ge 2 ] || fail "L'option --url attend une adresse."
                URL="$2"
                shift 2
                ;;
            --sha256)
                [ $# -ge 2 ] || fail "L'option --sha256 attend une somme."
                SHA256="$2"
                shift 2
                ;;
            --help | -h)
                usage
                exit 0
                ;;
            *)
                # Transmis à `hearth-agent install`, un par ligne.
                FORWARD="$FORWARD
$1"
                shift
                ;;
        esac
    done

    say "Installation de l'agent Hearth"

    # Droits (BR-INSTALL-001) : avant de télécharger quoi que ce soit.
    if [ "$(id -u)" -ne 0 ]; then
        say "Droits d'administration requis. Relance cette commande avec les droits d'administration."
        fail "Exemple : curl -fsSL <adresse-du-script> | sudo sh"
    fi

    # Architecture (BR-INSTALL-012) : x86_64 seulement pour l'instant.
    say "Vérification de l'architecture système..."
    if [ "$(uname -s)" != "Linux" ]; then
        fail "L'installation est prise en charge sous Linux seulement."
    fi
    case "$(uname -m)" in
        x86_64 | amd64) ;;
        aarch64 | arm64)
            fail "Cette architecture n'est pas prise en charge pour l'instant : seul x86_64 l'est (arm64 viendra plus tard)."
            ;;
        *)
            fail "Cette architecture n'est pas prise en charge. Architecture supportée : x86_64 (arm64 viendra plus tard)."
            ;;
    esac

    if [ -n "$BINARY" ]; then
        # Un fichier local : chemin absolu, jamais de recherche dans le PATH (celui qui est
        # vérifié est celui qui est lancé).
        case "$BINARY" in
            /*) ;;
            *) BINARY="$PWD/$BINARY" ;;
        esac
        [ -f "$BINARY" ] || fail "Le fichier $BINARY n'existe pas."
        [ -x "$BINARY" ] || fail "Le fichier $BINARY n'est pas exécutable (chmod +x)."
        AGENT="$BINARY"
        make_workdir
    else
        make_workdir
        if [ -z "$URL" ]; then
            # Dernière version publiée du dépôt. La publication des versions n'existe pas
            # encore : tant qu'aucune n'est publiée, ce téléchargement échoue avec un message
            # clair.
            URL="https://github.com/$REPO/releases/latest/download/hearth-agent-linux-x86_64"
        fi
        case "$URL" in
            https://*) ;;
            *) fail "L'adresse $URL n'est pas en HTTPS : un binaire lancé en root ne se télécharge pas en clair. Aucune modification n'a été apportée à ta machine." ;;
        esac
        say "Vérification de la connexion réseau..."
        # La somme SHA-256 est obligatoire pour un téléchargement : donnée, ou publiée à côté du
        # binaire (même origine, HTTPS). Sans elle, rien n'est téléchargé.
        if [ -z "$SHA256" ]; then
            status=0
            download "$URL.sha256" "$WORKDIR/expected.sha256" || status=$?
            if [ "$status" -eq 0 ]; then
                SHA256="$(cut -d ' ' -f 1 "$WORKDIR/expected.sha256" | head -n 1)"
            elif [ "$status" -eq 2 ]; then
                fail "Aucune somme SHA-256 n'est fournie ni publiée à côté du binaire ($URL.sha256), et aucune version de l'agent n'est peut-être publiée à cette adresse (la publication des versions n'est pas encore disponible). Rien n'est téléchargé : un binaire lancé en root ne s'installe pas sans vérification. Donne la somme avec --sha256, ou fournis un binaire avec --binary. Aucune modification n'a été apportée à ta machine."
            else
                fail "Le téléchargement n'a pas abouti. Relance l'installation."
            fi
        fi
        say "Téléchargement en cours..."
        AGENT="$WORKDIR/hearth-agent"
        status=0
        download "$URL" "$AGENT" || status=$?
        case "$status" in
            0) ;;
            2)
                fail "Aucune version de l'agent n'est publiée à l'adresse $URL. La publication des versions n'est pas encore disponible : fournis un binaire avec --binary, ou une adresse avec --url (ou HEARTH_RELEASE_URL). Aucune modification n'a été apportée à ta machine."
                ;;
            *)
                fail "Le téléchargement n'a pas abouti. Relance l'installation."
                ;;
        esac
        chmod 0755 "$AGENT"
    fi

    # Somme SHA-256 : obligatoire pour un téléchargement, facultative pour un fichier local.
    if [ -n "$SHA256" ]; then
        expected="$(printf '%s' "$SHA256" | tr 'A-F' 'a-f')"
        case "$expected" in
            *[!0-9a-f]* | "") fail "La somme SHA-256 attendue n'est pas valide (64 caractères hexadécimaux)." ;;
        esac
        [ "${#expected}" -eq 64 ] || fail "La somme SHA-256 attendue n'est pas valide (64 caractères hexadécimaux)."
        actual="$(sha256_of "$AGENT")"
        if [ "$actual" != "$expected" ]; then
            fail "La somme SHA-256 du binaire ne correspond pas (attendue $expected, trouvée $actual). Installation abandonnée, aucune modification n'a été apportée à ta machine."
        fi
        say "Somme SHA-256 vérifiée."
    else
        say "Attention : ce binaire local n'a pas de somme SHA-256 à comparer, il n'est pas vérifié. Il va être lancé avec les droits d'administration : interromps maintenant (Ctrl+C) si tu ne lui fais pas confiance."
    fi

    # Point d'accroche : la vérification de signature minisign (ADR-0008, mises à jour signées)
    # se branchera ici, entre la somme et le lancement. Elle n'est pas faite aujourd'hui.
    verify_signature() {
        :
    }
    verify_signature "$AGENT"

    # Un fichier qui n'est pas un agent (une page d'erreur, un autre binaire) ne se lance pas.
    "$AGENT" --version >/dev/null 2>&1 || fail "Le fichier obtenu n'est pas un agent Hearth utilisable sur cette machine. Aucune modification n'a été apportée à ta machine."

    # Les options transmises, une par ligne, sans développement de motifs.
    OLD_IFS="$IFS"
    IFS='
'
    set -f
    # shellcheck disable=SC2086 # le découpage par lignes est voulu
    set -- $FORWARD
    set +f
    IFS="$OLD_IFS"

    # Lu depuis un tube, l'entrée standard est le script lui-même : les questions se posent alors
    # sur le terminal.
    status=0
    if [ -t 0 ]; then
        "$AGENT" install "$@" || status=$?
    elif ( : </dev/tty ) 2>/dev/null; then
        "$AGENT" install "$@" </dev/tty || status=$?
    else
        "$AGENT" install "$@" </dev/null || status=$?
    fi
    exit "$status"
}

main "$@"
