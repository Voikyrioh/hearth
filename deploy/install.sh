#!/bin/sh
# Installe l'agent Hearth sur ce serveur Linux, en une commande.
#
#   curl -fsSL <adresse-du-script> | sudo sh
#   sudo sh install.sh --binary ./hearth-agent
#   curl -fsSL <adresse-du-script> | sudo HEARTH_ADMIN_USER=marie HEARTH_ADMIN_PASSWORD=... sh -s -- --yes
#
# Le script détecte l'architecture, prend le binaire (fichier local, ou téléchargement), vérifie
# sa somme SHA-256 quand elle est fournie, puis lance `hearth-agent install` : droits, port,
# premier compte, service, empreinte. Rien d'autre n'est fait ici.
#
# Options du script (les autres options, par exemple --port 7342 --managed --yes, sont
# transmises telles quelles à `hearth-agent install`) :
#   --binary CHEMIN   utilise ce binaire au lieu d'en télécharger un
#   --url ADRESSE     télécharge le binaire à cette adresse (ou HEARTH_RELEASE_URL)
#   --sha256 SOMME    somme SHA-256 attendue du binaire (ou HEARTH_SHA256)
#   --help            affiche cette aide
#
# Variables transmises à l'installation : HEARTH_ADMIN_USER, HEARTH_ADMIN_PASSWORD (ou
# HEARTH_ADMIN_PASSWORD_HASH, haché Argon2id), HEARTH_PORT, HEARTH_MANAGED.
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
    # Les lignes de commentaire du début, sans le dièse.
    sed -n '2,/^$/p' "$0" 2>/dev/null | sed 's/^# \{0,1\}//' || true
}

# shellcheck disable=SC2329 # appelée par `trap`
cleanup() {
    if [ -n "$WORKDIR" ] && [ -d "$WORKDIR" ]; then
        rm -rf "$WORKDIR"
    fi
}

# Retire le dossier temporaire à la sortie, et aussi sur Ctrl+C ou arrêt : rien ne reste.
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

# ---------------------------------------------------------------------------------------------
# Options
# ---------------------------------------------------------------------------------------------
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
            # Transmis à `hearth-agent install` (entre guillemets, un par un).
            FORWARD="$FORWARD
$1"
            shift
            ;;
    esac
done

say "Installation de l'agent Hearth"

# ---------------------------------------------------------------------------------------------
# Droits (BR-INSTALL-001) : avant de télécharger quoi que ce soit
# ---------------------------------------------------------------------------------------------
if [ "$(id -u)" -ne 0 ]; then
    say "Droits d'administration requis. Relance cette commande avec les droits d'administration."
    fail "Exemple : curl -fsSL <adresse-du-script> | sudo sh"
fi

# ---------------------------------------------------------------------------------------------
# Architecture (BR-INSTALL-012)
# ---------------------------------------------------------------------------------------------
say "Vérification de l'architecture système..."
if [ "$(uname -s)" != "Linux" ]; then
    fail "L'installation est prise en charge sous Linux seulement."
fi
case "$(uname -m)" in
    x86_64 | amd64)
        ARCH="x86_64"
        ;;
    aarch64 | arm64)
        ARCH="arm64"
        ;;
    *)
        fail "Cette architecture n'est pas prise en charge. Architectures supportées : x86_64, arm64."
        ;;
esac

# ---------------------------------------------------------------------------------------------
# Le binaire : fichier local, ou téléchargement
# ---------------------------------------------------------------------------------------------
sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d ' ' -f 1
    else
        fail "Aucun outil de somme SHA-256 (sha256sum ou shasum) : vérification impossible."
    fi
}

download() {
    # $1 adresse, $2 fichier de sortie. Rend 0 si le téléchargement a abouti, 2 si aucune version
    # n'est publiée à cette adresse (404), 1 sinon.
    if command -v curl >/dev/null 2>&1; then
        code="$(curl -sSL --proto '=https,http' --connect-timeout 15 --max-time 600 \
            -o "$2" -w '%{http_code}' "$1" 2>/dev/null)" || return 1
        case "$code" in
            200) return 0 ;;
            404) return 2 ;;
            *) return 1 ;;
        esac
    elif command -v wget >/dev/null 2>&1; then
        wget -q -T 15 -O "$2" "$1" 2>/dev/null || return 1
        return 0
    fi
    fail "Ni curl ni wget : téléchargement impossible. Fournis un binaire avec --binary."
}

WORKDIR="$(mktemp -d)"

if [ -n "$BINARY" ]; then
    [ -f "$BINARY" ] || fail "Le fichier $BINARY n'existe pas."
    [ -x "$BINARY" ] || fail "Le fichier $BINARY n'est pas exécutable (chmod +x)."
    AGENT="$BINARY"
else
    if [ -z "$URL" ]; then
        # Dernière version publiée du dépôt. La publication des versions n'existe pas encore :
        # tant qu'aucune version n'est publiée, ce téléchargement échoue avec un message clair.
        URL="https://github.com/$REPO/releases/latest/download/hearth-agent-linux-$ARCH"
    fi
    say "Vérification de la connexion réseau..."
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

# Somme SHA-256, quand elle est fournie.
if [ -n "$SHA256" ]; then
    expected="$(printf '%s' "$SHA256" | tr 'A-F' 'a-f')"
    actual="$(sha256_of "$AGENT")"
    if [ "$actual" != "$expected" ]; then
        fail "La somme SHA-256 du binaire ne correspond pas (attendue $expected, trouvée $actual). Installation abandonnée, aucune modification n'a été apportée à ta machine."
    fi
    say "Somme SHA-256 vérifiée."
else
    say "Aucune somme SHA-256 fournie : le binaire n'est pas vérifié (--sha256 ou HEARTH_SHA256)."
fi

# Point d'accroche : la vérification de signature minisign (ADR-0008, mises à jour signées) se
# branchera ici, entre la somme et le lancement. Elle n'est pas faite aujourd'hui.
verify_signature() {
    :
}
verify_signature "$AGENT"

# Un fichier qui n'est pas un agent (une page d'erreur, un autre binaire) ne se lance pas.
"$AGENT" --version >/dev/null 2>&1 || fail "Le fichier obtenu n'est pas un agent Hearth utilisable sur cette machine. Aucune modification n'a été apportée à ta machine."

# ---------------------------------------------------------------------------------------------
# Lancement de l'installation
# ---------------------------------------------------------------------------------------------
# Les options transmises, une par ligne (sans interpréteur : `set --` les remet en arguments).
OLD_IFS="$IFS"
IFS='
'
set -f # pas de développement de motifs dans les options transmises
# shellcheck disable=SC2086 # le découpage par lignes est voulu
set -- $FORWARD
set +f
IFS="$OLD_IFS"

# Lu depuis un tube (`curl ... | sh`), l'entrée standard est le script lui-même : les questions
# se posent alors sur le terminal.
interactive_stdin() {
    [ -t 0 ] && return 0
    ( : </dev/tty ) 2>/dev/null
}

status=0
if [ -t 0 ]; then
    "$AGENT" install "$@" || status=$?
elif interactive_stdin; then
    "$AGENT" install "$@" </dev/tty || status=$?
else
    "$AGENT" install "$@" </dev/null || status=$?
fi
exit "$status"
