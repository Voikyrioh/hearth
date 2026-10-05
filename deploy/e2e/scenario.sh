#!/bin/sh
# Scénario de bout en bout de l'installation, exécuté DANS le conteneur jetable de
# `cargo xtask e2e-install` (Debian + systemd). Monté en lecture seule :
#   /dist/hearth-agent   le binaire statique construit par `cargo xtask agent`
#   /deploy/install.sh   le script d'installation
# Chaque vérification affiche « ok - … » ; la première qui échoue arrête tout (code 1).
set -eu

BIN=/dist/hearth-agent
PW='Cheval-Agrafe-42'
USER_NAME=marie
PORT=7341
BASE="https://127.0.0.1:$PORT/api/v1"
INSTALLED=/usr/local/bin/hearth-agent
UNIT=/etc/systemd/system/hearth-agent.service
# Tout ce que l'agent peut laisser sur le système (le verrou d'installation vit dans /run, volatil).
TRACES="$INSTALLED /usr/local/bin/.hearth-agent.previous /etc/hearth /var/lib/hearth $UNIT"
OUT=/tmp/out

ok() { printf 'ok - %s\n' "$*"; }
die() {
    printf 'ECHEC - %s\n' "$*" >&2
    [ -f "$OUT" ] && { echo '--- dernière sortie' >&2; cat "$OUT" >&2; }
    exit 1
}

# Lance une commande, garde sa sortie dans $OUT, rend son code sans arrêter le script.
run() {
    status=0
    "$@" >"$OUT" 2>&1 || status=$?
    return "$status"
}

must_say() { grep -qF -- "$1" "$OUT" || die "la sortie devrait contenir : $1"; }
must_not_say() { ! grep -qF -- "$1" "$OUT" || die "la sortie ne devrait pas contenir : $1"; }

nothing_left() {
    for path in $TRACES; do
        [ ! -e "$path" ] || die "$1 : $path existe encore"
    done
}

hello() { curl -sk --max-time 5 "$BASE/hello"; }
login() {
    curl -sk --max-time 10 -o "$OUT.login" -w '%{http_code}' -X POST "$BASE/sessions" \
        -H 'content-type: application/json' -H 'x-hearth-api: 1' -H 'x-hearth-client: e2e/1' \
        -d "{\"username\":\"$USER_NAME\",\"password\":\"$PW\"}"
}

# Empreinte du certificat réellement servi, au format de l'affichage (8 groupes de 4, majuscules).
served_fingerprint() {
    openssl s_client -tls1_3 -connect "127.0.0.1:$PORT" </dev/null 2>/dev/null \
        | openssl x509 -outform DER | sha256sum | cut -c 1-32 | tr 'a-f' 'A-F' \
        | sed 's/\(....\)/\1 /g; s/ $//'
}

printed_fingerprint() {
    sed -n 's/^Empreinte du serveur : //p' "$OUT" | tr -d '\r' | head -n 1
}

echo "== système : $(uname -m), systemd $(systemctl --version | head -n 1)"
[ -d /run/systemd/system ] || die "systemd n'est pas le système d'initialisation du conteneur"
nothing_left "conteneur propre au départ"
ok "conteneur propre, systemd démarré"

# --------------------------------------------------------------------------------------------
# Prérequis : rien n'est écrit
# --------------------------------------------------------------------------------------------
cp "$BIN" /tmp/agent-unprivileged && chmod 0755 /tmp/agent-unprivileged
if run su -s /bin/sh nobody -c "HEARTH_ADMIN_USER=$USER_NAME HEARTH_ADMIN_PASSWORD=$PW /tmp/agent-unprivileged install --yes"; then
    die "l'installation sans droits devrait échouer"
fi
must_say "Droits d'administration requis. Relance cette commande avec les droits d'administration."
must_say "sudo"
nothing_left "sans droits"
ok "sans droits d'administration : refus propre, rien d'écrit"

nc -l 7341 >/dev/null 2>&1 &
LISTENER=$!
sleep 1
if run env HEARTH_ADMIN_USER=$USER_NAME HEARTH_ADMIN_PASSWORD=$PW "$BIN" install --yes; then
    die "l'installation avec le port occupé devrait échouer"
fi
must_say "Le port configuré est déjà utilisé. Relance en choisissant un autre port."
must_say "--port 7342"
nothing_left "port occupé"
kill "$LISTENER" 2>/dev/null || true
ok "port occupé : refus propre avec la commande à relancer, rien d'écrit"

# Un identifiant ou un mot de passe invalide est refusé avant toute écriture.
if run env HEARTH_ADMIN_USER=$USER_NAME HEARTH_ADMIN_PASSWORD=court "$BIN" install --yes; then
    die "un mot de passe trop court devrait être refusé"
fi
must_say "Le mot de passe est trop court."
nothing_left "mot de passe trop court"
ok "mot de passe invalide : refus avant toute écriture"

# --------------------------------------------------------------------------------------------
# Retour en arrière : une erreur au milieu laisse la machine comme avant
# --------------------------------------------------------------------------------------------
touch /var/lib/hearth # un fichier à la place du dossier de données : la création échouera après la copie du binaire
if run env HEARTH_ADMIN_USER=$USER_NAME HEARTH_ADMIN_PASSWORD=$PW "$BIN" install --yes; then
    die "l'installation devrait échouer quand /var/lib/hearth est un fichier"
fi
must_say "Une erreur s'est produite : "
must_say "Aucune modification n'a été apportée à ta machine."
[ ! -e "$INSTALLED" ] || die "retour en arrière : le binaire est resté"
[ ! -e "$UNIT" ] || die "retour en arrière : l'unité est restée"
[ ! -e /etc/hearth ] || die "retour en arrière : la configuration est restée"
[ -f /var/lib/hearth ] || die "ce qui existait avant (/var/lib/hearth) a disparu"
rm -f /var/lib/hearth
nothing_left "après erreur"
ok "erreur à mi-parcours : tout est défait, ce qui existait avant est intact"

# --------------------------------------------------------------------------------------------
# install.sh : architecture, pas de version publiée, somme fausse
# --------------------------------------------------------------------------------------------
mkdir -p /tmp/fakebin
# shellcheck disable=SC2016 # le script écrit garde ses $ littéraux
printf '#!/bin/sh\nif [ "$1" = "-m" ]; then echo riscv64; else /usr/bin/uname "$@"; fi\n' >/tmp/fakebin/uname
chmod 0755 /tmp/fakebin/uname
if run env PATH="/tmp/fakebin:$PATH" sh /deploy/install.sh --binary "$BIN" --yes; then
    die "install.sh devrait refuser riscv64"
fi
must_say "Cette architecture n'est pas prise en charge. Architectures supportées : x86_64, arm64."
nothing_left "architecture"
ok "install.sh : architecture non prise en charge refusée"

mkdir -p /tmp/www/empty
(cd /tmp/www/empty && python3 -m http.server 8001 --bind 127.0.0.1 >/dev/null 2>&1) &
HTTP_EMPTY=$!
sleep 1
if run env HEARTH_RELEASE_URL=http://127.0.0.1:8001/hearth-agent sh /deploy/install.sh --yes; then
    die "install.sh sans version publiée devrait échouer"
fi
must_say "Aucune version de l'agent n'est publiée"
nothing_left "aucune version"
kill "$HTTP_EMPTY" 2>/dev/null || true
if run env HEARTH_RELEASE_URL=http://127.0.0.1:9/hearth-agent sh /deploy/install.sh --yes; then
    die "install.sh sans réseau devrait échouer"
fi
must_say "Le téléchargement n'a pas abouti. Relance l'installation."
nothing_left "sans réseau"
ok "install.sh : aucune version publiée / téléchargement impossible : messages clairs, rien d'écrit"

mkdir -p /tmp/www/release
cp "$BIN" /tmp/www/release/hearth-agent
SUM=$(sha256sum "$BIN" | cut -d ' ' -f 1)
(cd /tmp/www/release && python3 -m http.server 8002 --bind 127.0.0.1 >/dev/null 2>&1) &
HTTP_RELEASE=$!
sleep 1
if run env HEARTH_ADMIN_USER=$USER_NAME HEARTH_ADMIN_PASSWORD=$PW sh /deploy/install.sh --url http://127.0.0.1:8002/hearth-agent --sha256 0000 --yes; then
    die "install.sh avec une somme fausse devrait échouer"
fi
must_say "La somme SHA-256 du binaire ne correspond pas"
nothing_left "somme fausse"
ok "install.sh : somme SHA-256 fausse refusée, rien d'écrit"

# --------------------------------------------------------------------------------------------
# Première installation : par le script lu depuis un tube, binaire téléchargé et vérifié
# --------------------------------------------------------------------------------------------
cat /deploy/install.sh | run env HEARTH_ADMIN_USER=$USER_NAME HEARTH_ADMIN_PASSWORD=$PW \
    HEARTH_RELEASE_URL=http://127.0.0.1:8002/hearth-agent HEARTH_SHA256="$SUM" \
    sh -s -- --yes || die "la première installation a échoué"
kill "$HTTP_RELEASE" 2>/dev/null || true
must_say "Somme SHA-256 vérifiée."
must_say "Installation en cours..."
must_say "Démarrage du service..."
must_say "Installation réussie. L'agent démarre automatiquement avec ton serveur."
must_say "Note cette empreinte."
must_not_say "$PW"
FIRST_FP=$(printed_fingerprint)
[ -n "$FIRST_FP" ] || die "aucune empreinte affichée"
ok "première installation (script lu depuis un tube, binaire téléchargé, somme vérifiée) : empreinte $FIRST_FP"

[ "$(systemctl is-enabled hearth-agent)" = enabled ] || die "le service n'est pas activé au démarrage"
[ "$(systemctl is-active hearth-agent)" = active ] || die "le service ne tourne pas"
ok "le service est actif et activé au démarrage"
grep -q '^Restart=always' "$UNIT" || die "l'unité n'a pas Restart=always"
grep -q '^WantedBy=multi-user.target' "$UNIT" || die "l'unité n'a pas WantedBy=multi-user.target"
! grep -qF "$PW" "$UNIT" || die "le mot de passe est dans l'unité"
ok "unité : Restart=always, WantedBy=multi-user.target, aucun secret"

hello | grep -q '"product":"hearth"' || die "/hello ne répond pas"
ok "/api/v1/hello répond"
[ "$(login)" = 201 ] || die "la connexion avec le compte créé échoue : $(cat "$OUT.login")"
grep -q '"token"' "$OUT.login" || die "pas de jeton"
ok "la connexion avec le compte créé réussit"
[ "$(served_fingerprint)" = "$FIRST_FP" ] || die "l'empreinte affichée ($FIRST_FP) n'est pas celle du certificat servi ($(served_fingerprint))"
ok "l'empreinte affichée est celle du certificat servi"

[ "$(stat -c %a /var/lib/hearth)" = 700 ] || die "dossier de données : droits $(stat -c %a /var/lib/hearth)"
[ "$(stat -c %a /var/lib/hearth/key.pem)" = 600 ] || die "key.pem : droits $(stat -c %a /var/lib/hearth/key.pem)"
[ "$(stat -c %a /var/lib/hearth/hearth.db)" = 600 ] || die "hearth.db : droits $(stat -c %a /var/lib/hearth/hearth.db)"
[ "$(stat -c %a $INSTALLED)" = 755 ] || die "binaire : droits $(stat -c %a $INSTALLED)"
ok "droits : données 0700, clé et base 0600, binaire 0755"
! journalctl -u hearth-agent --no-pager 2>/dev/null | grep -qF "$PW" || die "le mot de passe est dans les journaux"
ok "aucun mot de passe dans les journaux du service"

# --------------------------------------------------------------------------------------------
# Réinstallation : mêmes comptes, même empreinte, service non interrompu
# --------------------------------------------------------------------------------------------
PID_BEFORE=$(systemctl show -p MainPID --value hearth-agent)
run "$INSTALLED" install --yes || die "la réinstallation a échoué"
must_say "Mise à jour de l'agent Hearth"
must_say "Agent détecté. Vérification de la version..."
must_say "L'agent est déjà à jour. Le service n'a pas été interrompu."
[ "$(printed_fingerprint)" = "$FIRST_FP" ] || die "l'empreinte a changé à la réinstallation"
[ "$(systemctl show -p MainPID --value hearth-agent)" = "$PID_BEFORE" ] || die "le service a été redémarré inutilement"
"$INSTALLED" account list | grep -q "^$USER_NAME " || die "le compte $USER_NAME a disparu"
[ "$(login)" = 201 ] || die "la connexion échoue après la réinstallation"
ok "réinstallation à l'identique : mêmes comptes, même empreinte, service non interrompu (PID $PID_BEFORE)"

# Même version, autres octets : le service redémarre, rien n'est perdu.
cp "$BIN" /tmp/agent-rebuilt && printf '\0' >>/tmp/agent-rebuilt && chmod 0755 /tmp/agent-rebuilt
run /tmp/agent-rebuilt install --yes || die "la réinstallation avec un autre binaire a échoué"
must_say "L'agent est déjà à jour. Service redémarré."
[ "$(systemctl show -p MainPID --value hearth-agent)" != "$PID_BEFORE" ] || die "le service aurait dû redémarrer"
[ "$(printed_fingerprint)" = "$FIRST_FP" ] || die "l'empreinte a changé"
[ "$(login)" = 201 ] || die "la connexion échoue après le redémarrage"
ok "même version, autre binaire : service redémarré, mêmes comptes, même empreinte"

# --------------------------------------------------------------------------------------------
# Désinstallation avec conservation, puis réinstallation : les données sont retrouvées
# --------------------------------------------------------------------------------------------
run "$INSTALLED" uninstall --keep-data --yes || die "la désinstallation a échoué"
must_say "Désinstallation de l'agent Hearth"
must_say "Service arrêté. Tes comptes et données sont conservés sur ta machine."
[ ! -e "$INSTALLED" ] || die "le binaire est resté"
[ ! -e "$UNIT" ] || die "l'unité est restée"
[ "$(systemctl is-active hearth-agent 2>/dev/null || true)" != active ] || die "le service tourne encore"
[ -f /var/lib/hearth/hearth.db ] && [ -f /var/lib/hearth/cert.pem ] && [ -f /etc/hearth/agent.toml ] || die "les données n'ont pas été conservées"
! curl -sk --max-time 3 "$BASE/hello" >/dev/null 2>&1 || die "l'agent répond encore"
ok "désinstallation avec conservation : service et binaire retirés, données et configuration conservées"

run "$BIN" install --yes || die "la réinstallation après désinstallation a échoué"
must_say "Installation de l'agent Hearth"
[ "$(printed_fingerprint)" = "$FIRST_FP" ] || die "l'empreinte a changé après désinstallation et réinstallation"
"$INSTALLED" account list | grep -q "^$USER_NAME " || die "le compte a disparu"
[ "$(login)" = 201 ] || die "la connexion échoue après réinstallation"
ok "réinstallation après désinstallation : données retrouvées (compte, empreinte), aucun compte redemandé"

# --------------------------------------------------------------------------------------------
# Désinstallation avec purge : plus rien sur le système
# --------------------------------------------------------------------------------------------
run "$INSTALLED" uninstall --purge --yes || die "la désinstallation avec purge a échoué"
must_say "Service arrêté. Tous les comptes, le journal et la configuration ont été supprimés. Aucune trace de l'agent ne reste sur ta machine."
nothing_left "après purge"
[ -z "$(systemctl list-unit-files --no-legend 'hearth-agent*' 2>/dev/null)" ] || die "systemd connaît encore l'unité"
! pgrep -f hearth-agent >/dev/null || die "un processus hearth-agent tourne encore"
! curl -sk --max-time 3 "$BASE/hello" >/dev/null 2>&1 || die "l'agent répond encore"
ok "purge : aucun des chemins ($TRACES) n'existe, plus d'unité, plus de processus"

run "$BIN" uninstall --yes || die "la désinstallation d'une machine propre devrait réussir"
must_say "L'agent n'est pas installé sur ce serveur. Rien à désinstaller."
ok "rien d'installé : « Rien à désinstaller »"

# --------------------------------------------------------------------------------------------
# Installation interactive, sur un vrai terminal (pty) : port par défaut, nom, mot de passe
# sans écho avec confirmation, après deux saisies invalides
# --------------------------------------------------------------------------------------------
# Réponses : port (vide), nom vide, nom invalide, nom correct, mot de passe trop court,
# mot de passe correct, confirmation différente, mot de passe correct, confirmation correcte.
# Une ligne par seconde : le terminal n'affiche pas ce qui est tapé pendant la saisie du mot de
# passe, à condition que la réponse n'arrive pas avant la question.
feed() {
    for line in "$@"; do
        sleep 1
        printf '%s\n' "$line"
    done
}
feed "" "" "Marie Dupont" "$USER_NAME" court "$PW" autre-chose "$PW" "$PW" \
    | script -qec "$BIN install" /dev/null >"$OUT" 2>&1 || die "l'installation interactive a échoué"
must_say "Numéro de port [7341]:"
must_say "Le nom du compte est requis."
must_say "Le nom du compte contient des caractères non autorisés."
must_say "Le mot de passe est trop court."
must_say "Les deux mots de passe ne correspondent pas."
must_say "Installation réussie. L'agent démarre automatiquement avec ton serveur."
must_not_say "$PW"
INTERACTIVE_FP=$(printed_fingerprint)
[ -n "$INTERACTIVE_FP" ] || die "aucune empreinte affichée (installation interactive)"
[ "$(login)" = 201 ] || die "la connexion échoue après l'installation interactive"
[ "$(served_fingerprint)" = "$INTERACTIVE_FP" ] || die "empreinte affichée et empreinte servie diffèrent (installation interactive)"
ok "installation interactive (pty) : questions, erreurs réaffichées sans quitter, mot de passe jamais affiché, compte créé"
# Désinstallation interactive : la question est reposée jusqu'à une réponse valide.
feed peut-etre supprimer | script -qec "$INSTALLED uninstall" /dev/null >"$OUT" 2>&1 || die "la désinstallation interactive a échoué"
must_say "Supprimer aussi les comptes, le journal et la configuration ? (conserver/supprimer)"
must_say "Réponds 'conserver' ou 'supprimer'."
must_say "Aucune trace de l'agent ne reste sur ta machine."
nothing_left "après désinstallation interactive"
ok "désinstallation interactive : question reposée jusqu'à une réponse valide, purge complète"

# --------------------------------------------------------------------------------------------
# Installation gérée par le système : aucune unité, aucun binaire copié
# --------------------------------------------------------------------------------------------
run env HEARTH_ADMIN_USER=$USER_NAME HEARTH_ADMIN_PASSWORD=$PW "$BIN" install --managed --yes || die "l'installation gérée a échoué"
must_say "aucune unité n'a été écrite"
must_say "Empreinte du serveur : "
[ ! -e "$UNIT" ] || die "une unité a été écrite en installation gérée"
[ ! -e "$INSTALLED" ] || die "le binaire a été copié en installation gérée"
grep -q '^managed = true' /etc/hearth/agent.toml || die "la configuration ne dit pas managed = true"
MANAGED_FP=$(printed_fingerprint)
"$BIN" serve >/tmp/serve.log 2>&1 &
SERVE=$!
n=0
until hello | grep -q '"product":"hearth"'; do
    n=$((n + 1))
    [ "$n" -lt 30 ] || die "l'agent lancé à la main ne répond pas : $(cat /tmp/serve.log)"
    sleep 1
done
hello | grep -q '"managed":true' || die "/hello ne dit pas managed"
[ "$(served_fingerprint)" = "$MANAGED_FP" ] || die "empreinte affichée et empreinte servie diffèrent (installation gérée)"
[ "$(login)" = 201 ] || die "connexion impossible (installation gérée)"
kill "$SERVE" 2>/dev/null || true
wait "$SERVE" 2>/dev/null || true
ok "installation gérée : aucune unité, aucun binaire copié, agent lancé à la main, managed vrai, empreinte et compte vérifiés"
run "$BIN" uninstall --managed --purge --yes || die "la purge de l'installation gérée a échoué"
nothing_left "après purge (installation gérée)"
ok "purge de l'installation gérée : plus rien"

echo "== tout est vert"
