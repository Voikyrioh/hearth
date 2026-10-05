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

# Une ligne par seconde : le terminal n'affiche pas ce qui est tapé pendant la saisie du mot de
# passe, à condition que la réponse n'arrive pas avant la question.
feed() {
    for line in "$@"; do
        sleep 1
        printf '%s\n' "$line"
    done
}

echo "== système : $(uname -m), systemd $(systemctl --version | head -n 1)"
[ -d /run/systemd/system ] || die "systemd n'est pas le système d'initialisation du conteneur"
nothing_left "conteneur propre au départ"
ok "conteneur propre, systemd démarré"

# Serveur HTTPS local (certificat approuvé par le conteneur) pour tester le téléchargement.
mkdir -p /tmp/pki /tmp/www/release /tmp/www/nosum
openssl req -x509 -newkey rsa:2048 -nodes -keyout /tmp/pki/key.pem -out /tmp/pki/cert.pem -days 1 \
    -subj /CN=127.0.0.1 -addext "subjectAltName=IP:127.0.0.1" >/dev/null 2>&1
cp /tmp/pki/cert.pem /usr/local/share/ca-certificates/hearth-e2e.crt
update-ca-certificates >/dev/null 2>&1
SUM=$(sha256sum "$BIN" | cut -d ' ' -f 1)
cp "$BIN" /tmp/www/release/hearth-agent
printf '%s  hearth-agent\n' "$SUM" >/tmp/www/release/hearth-agent.sha256
cp "$BIN" /tmp/www/nosum/hearth-agent
python3 /deploy/e2e/https_server.py 8443 /tmp/www/release &
python3 /deploy/e2e/https_server.py 8444 /tmp/www/nosum &
sleep 2
REL=https://127.0.0.1:8443/hearth-agent

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

if run env HEARTH_ADMIN_USER=$USER_NAME HEARTH_ADMIN_PASSWORD=court "$BIN" install --yes; then
    die "un mot de passe trop court devrait être refusé"
fi
must_say "Le mot de passe est trop court."
nothing_left "mot de passe trop court"
for weak in 'm=8,t=1,p=1' 'm=4194304,t=2,p=1'; do
    if run env HEARTH_ADMIN_USER=$USER_NAME \
        "HEARTH_ADMIN_PASSWORD_HASH=\$argon2id\$v=19\$$weak\$c29tZXNhbHRzb21lc2FsdA\$aGFzaGhhc2hoYXNoaGFzaGhhc2hoYXNoaGFzaGhhc2g" \
        "$BIN" install --yes; then
        die "un haché hors des bornes ($weak) devrait être refusé"
    fi
    must_say "hors des bornes acceptées"
    nothing_left "haché hors bornes ($weak)"
done
ok "mot de passe faible et haché hors bornes (trop faible, trop gourmand) : refus avant toute écriture"

# Données du dossier de données : une identité à moitié là n'est jamais touchée.
mkdir -p /var/lib/hearth && chmod 700 /var/lib/hearth
printf 'mon-certificat' >/var/lib/hearth/cert.pem
printf 'ma-cle' >/var/lib/hearth/key.pem
if run env HEARTH_ADMIN_USER=$USER_NAME HEARTH_ADMIN_PASSWORD=$PW "$BIN" install --yes; then
    die "une identité incomplète devrait être refusée"
fi
must_say "identité du serveur est incomplète"
must_say "Rien n'a été modifié."
[ "$(cat /var/lib/hearth/cert.pem)" = "mon-certificat" ] || die "cert.pem a été touché"
[ "$(cat /var/lib/hearth/key.pem)" = "ma-cle" ] || die "key.pem a été touché"
# shellcheck disable=SC2012 # noms simples
[ "$(ls /var/lib/hearth | wc -l)" -eq 2 ] || die "des fichiers ont été ajoutés ou retirés"
[ ! -e "$INSTALLED" ] && [ ! -e /etc/hearth ] && [ ! -e "$UNIT" ] || die "quelque chose a été écrit malgré le refus"
rm -rf /var/lib/hearth
ok "identité partielle : refus avant toute écriture, aucun fichier supprimé ni ajouté"

# --------------------------------------------------------------------------------------------
# Retour en arrière : une erreur au milieu, puis une erreur TARDIVE, laissent la machine comme avant
# --------------------------------------------------------------------------------------------
# Un dossier de données ouvert aux autres (0755) ou qui n'est pas un dossier : refus AVANT toute
# écriture, ce qui existait n'est pas touché.
mkdir -m 755 /var/lib/hearth
if run env HEARTH_ADMIN_USER=$USER_NAME HEARTH_ADMIN_PASSWORD=$PW "$BIN" install --yes; then
    die "l'installation devrait refuser un dossier de données ouvert aux autres"
fi
must_say "ouvert à d'autres utilisateurs"
[ ! -e "$INSTALLED" ] && [ ! -e /etc/hearth ] && [ ! -e "$UNIT" ] || die "quelque chose a été écrit malgré le refus"
rmdir /var/lib/hearth
touch /var/lib/hearth
if run env HEARTH_ADMIN_USER=$USER_NAME HEARTH_ADMIN_PASSWORD=$PW "$BIN" install --yes; then
    die "l'installation devrait refuser un dossier de données qui est un fichier"
fi
[ ! -e "$INSTALLED" ] && [ ! -e /etc/hearth ] && [ ! -e "$UNIT" ] || die "quelque chose a été écrit malgré le refus"
[ -f /var/lib/hearth ] || die "ce qui existait avant (/var/lib/hearth) a disparu"
rm -f /var/lib/hearth
nothing_left "après refus du dossier de données"
ok "dossier de données ouvert ou qui n'est pas un dossier : refus avant toute écriture, rien de touché"

# Erreur TARDIVE : le service ne démarre pas (systemctl enable échoue), alors que le binaire, la
# configuration, l'identité, la base et le compte sont déjà écrits : tout est défait.
mkdir -p /tmp/failbin
cat >/tmp/failbin/systemctl <<'EOF'
#!/bin/sh
if [ "$1" = "enable" ]; then
    echo "échec simulé de systemctl enable" >&2
    exit 1
fi
exec /usr/bin/systemctl "$@"
EOF
chmod 0755 /tmp/failbin/systemctl
if run env PATH="/tmp/failbin:$PATH" HEARTH_ADMIN_USER=$USER_NAME HEARTH_ADMIN_PASSWORD=$PW "$BIN" install --yes; then
    die "l'installation devrait échouer quand le service ne démarre pas"
fi
must_say "Une erreur s'est produite : "
must_say "Aucune modification n'a été apportée à ta machine."
nothing_left "après échec tardif (service)"
ok "erreur tardive (service qui ne démarre pas, après configuration, identité et compte) : tout est défait, y compris le dossier de données"

# --------------------------------------------------------------------------------------------
# Interruption par signal (SIGHUP : une session ssh qui tombe) en plein milieu
# --------------------------------------------------------------------------------------------
cat >/tmp/failbin/systemctl <<'EOF'
#!/bin/sh
if [ "$1" = "enable" ]; then
    sleep 12
fi
exec /usr/bin/systemctl "$@"
EOF
env PATH="/tmp/failbin:$PATH" HEARTH_ADMIN_USER=$USER_NAME HEARTH_ADMIN_PASSWORD=$PW \
    "$BIN" install --yes >"$OUT" 2>&1 &
INSTALL_PID=$!
sleep 5
[ -d /var/lib/hearth ] || die "l'installation n'a pas commencé d'écrire avant le signal"
kill -HUP "$INSTALL_PID"
status=0
wait "$INSTALL_PID" || status=$?
[ "$status" -ne 0 ] || die "l'installation interrompue devrait échouer"
must_say "Installation interrompue. Aucune modification n'a été apportée à ta machine."
nothing_left "après SIGHUP"
[ "$(systemctl is-active hearth-agent 2>/dev/null || true)" != active ] || die "le service tourne encore après l'interruption"
ok "SIGHUP en plein milieu : retour en arrière complet, service retiré"

# --------------------------------------------------------------------------------------------
# install.sh : architecture, HTTPS, somme obligatoire, robustesse
# --------------------------------------------------------------------------------------------
mkdir -p /tmp/fakebin
# shellcheck disable=SC2016 # le script écrit garde ses $ littéraux
printf '#!/bin/sh\nif [ "$1" = "-m" ]; then echo %s; else /usr/bin/uname "$@"; fi\n' riscv64 >/tmp/fakebin/uname
chmod 0755 /tmp/fakebin/uname
if run env PATH="/tmp/fakebin:$PATH" sh /deploy/install.sh --binary "$BIN" --yes; then
    die "install.sh devrait refuser riscv64"
fi
must_say "Cette architecture n'est pas prise en charge. Architecture supportée : x86_64"
# shellcheck disable=SC2016
printf '#!/bin/sh\nif [ "$1" = "-m" ]; then echo %s; else /usr/bin/uname "$@"; fi\n' aarch64 >/tmp/fakebin/uname
if run env PATH="/tmp/fakebin:$PATH" sh /deploy/install.sh --binary "$BIN" --yes; then
    die "install.sh devrait refuser arm64"
fi
must_say "arm64 viendra plus tard"
nothing_left "architecture"
ok "install.sh : architecture non prise en charge (riscv64, arm64) refusée clairement"

if run sh /deploy/install.sh --url http://127.0.0.1:8443/hearth-agent --sha256 "$SUM" --yes; then
    die "install.sh devrait refuser une adresse en clair"
fi
must_say "n'est pas en HTTPS"
nothing_left "http"
if run env HEARTH_RELEASE_URL=https://127.0.0.1:8443/redirect sh /deploy/install.sh --sha256 "$SUM" --yes; then
    die "install.sh devrait refuser une redirection vers HTTP"
fi
must_say "Le téléchargement n'a pas abouti. Relance l'installation."
nothing_left "redirection"
ok "install.sh : adresse en clair refusée, redirection vers HTTP refusée"

: >/tmp/https.log
if run env HEARTH_RELEASE_URL=https://127.0.0.1:8444/hearth-agent sh /deploy/install.sh --yes; then
    die "install.sh sans somme devrait échouer"
fi
must_say "Aucune somme SHA-256 n'est fournie ni publiée"
! grep -q '^/hearth-agent$' /tmp/https.log || die "le binaire a été téléchargé alors qu'aucune somme n'était disponible"
nothing_left "sans somme"
ok "install.sh : sans somme (ni donnée, ni publiée à côté), refus AVANT de télécharger le binaire"

if run env HEARTH_RELEASE_URL=https://127.0.0.1:9/hearth-agent sh /deploy/install.sh --yes; then
    die "install.sh sans réseau devrait échouer"
fi
must_say "Le téléchargement n'a pas abouti. Relance l'installation."
nothing_left "sans réseau"
if run env HEARTH_ADMIN_USER=$USER_NAME HEARTH_ADMIN_PASSWORD=$PW sh /deploy/install.sh --url "$REL" --sha256 "$(printf '0%.0s' $(seq 1 64))" --yes; then
    die "install.sh avec une somme fausse devrait échouer"
fi
must_say "La somme SHA-256 du binaire ne correspond pas"
nothing_left "somme fausse"
ok "install.sh : téléchargement impossible, somme fausse : refus, rien d'écrit"

# /tmp sans droit d'exécution : repli sur un autre dossier (la somme fausse prouve que le binaire
# a bien été téléchargé et examiné).
mkdir -p /tmp/noexec && mount -t tmpfs -o noexec tmpfs /tmp/noexec
if run env TMPDIR=/tmp/noexec sh /deploy/install.sh --url "$REL" --sha256 "$(printf '1%.0s' $(seq 1 64))" --yes; then
    die "install.sh avec une somme fausse devrait échouer (TMPDIR sans exécution)"
fi
must_say "La somme SHA-256 du binaire ne correspond pas"
umount /tmp/noexec
ok "install.sh : TMPDIR monté sans droit d'exécution, repli sur un autre dossier temporaire"

# Un téléchargement tronqué n'exécute rien : tout est dans main, appelé à la dernière ligne.
head -c 3000 /deploy/install.sh | run sh -s -- --yes && die "un script tronqué ne devrait pas réussir"
must_not_say "Installation de l'agent Hearth"
nothing_left "script tronqué"
ok "install.sh tronqué : rien n'est exécuté"

# --binary relatif : celui qui est vérifié est celui qui est lancé, jamais un homonyme du PATH.
mkdir -p /tmp/decoy
printf '#!/bin/sh\ntouch /tmp/decoy-ran\nexit 0\n' >/tmp/decoy/hearth-agent
chmod 0755 /tmp/decoy/hearth-agent
(cd /dist && run env PATH="/tmp/decoy:$PATH" HEARTH_ADMIN_USER=$USER_NAME HEARTH_ADMIN_PASSWORD=$PW \
    sh /deploy/install.sh --binary hearth-agent --yes) || die "l'installation avec --binary relatif a échoué"
[ ! -e /tmp/decoy-ran ] || die "un homonyme du PATH a été lancé"
must_say "n'a pas de somme SHA-256 à comparer"
"$INSTALLED" uninstall --purge --yes >/dev/null 2>&1 || die "purge après --binary relatif"
nothing_left "après --binary relatif"
ok "install.sh --binary relatif : résolu en chemin absolu, aucun homonyme du PATH lancé ; binaire sans somme annoncé avant lancement"

# --------------------------------------------------------------------------------------------
# Première installation : script lu depuis un tube, binaire téléchargé en HTTPS, somme publiée à côté
# --------------------------------------------------------------------------------------------
# Le mot de passe est fabriqué par `hash-password` (saisie sans écho) : il ne figure sur aucune
# ligne de commande.
feed "$PW" "$PW" | script -qec "$BIN hash-password --user $USER_NAME" /dev/null >"$OUT" 2>&1 || die "hash-password a échoué"
HASH=$(tr -d '\r' <"$OUT" | grep '^[$]argon2id[$]' | head -n 1)
[ -n "$HASH" ] || die "hash-password n'a pas écrit de haché"
must_not_say "$PW"
ok "hash-password : haché Argon2id fabriqué à partir d'une saisie sans écho"

cat /deploy/install.sh | run env HEARTH_ADMIN_USER=$USER_NAME "HEARTH_ADMIN_PASSWORD_HASH=$HASH" \
    HEARTH_RELEASE_URL="$REL" \
    sh -s -- --yes || die "la première installation a échoué"
must_say "Somme SHA-256 vérifiée."
must_say "Installation en cours..."
must_say "Démarrage du service..."
must_say "Installation réussie. L'agent démarre automatiquement avec ton serveur."
must_say "Note cette empreinte."
must_not_say "$PW"
must_not_say "$HASH"
FIRST_FP=$(printed_fingerprint)
[ -n "$FIRST_FP" ] || die "aucune empreinte affichée"
ok "première installation (script lu depuis un tube, HTTPS, somme publiée à côté, haché fourni) : empreinte $FIRST_FP"

[ "$(systemctl is-enabled hearth-agent)" = enabled ] || die "le service n'est pas activé au démarrage"
[ "$(systemctl is-active hearth-agent)" = active ] || die "le service ne tourne pas"
ok "le service est actif et activé au démarrage"
grep -q '^Restart=always' "$UNIT" || die "l'unité n'a pas Restart=always"
grep -q '^WantedBy=multi-user.target' "$UNIT" || die "l'unité n'a pas WantedBy=multi-user.target"
grep -q '^CapabilityBoundingSet=' "$UNIT" || die "l'unité n'a pas de CapabilityBoundingSet"
! grep -qF "$PW" "$UNIT" || die "le mot de passe est dans l'unité"
! grep -qF "$HASH" "$UNIT" || die "le haché est dans l'unité"
ok "unité : Restart=always, WantedBy=multi-user.target, capacités bornées, aucun secret"

hello | grep -q '"product":"hearth"' || die "/hello ne répond pas (le service durci tourne-t-il ?)"
ok "/api/v1/hello répond (service durci : capacités, filtre d'appels système)"
[ "$(login)" = 201 ] || die "la connexion avec le compte créé échoue : $(cat "$OUT.login")"
grep -q '"token"' "$OUT.login" || die "pas de jeton"
ok "la connexion avec le compte créé (haché fourni) réussit"
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
# Mise à niveau depuis une AUTRE version, et refus de rétrograder
# --------------------------------------------------------------------------------------------
# Le binaire « ancien » est le même, dont la chaîne de version est remplacée par 0.0.9 (même
# longueur : le binaire reste valide).
LC_ALL=C sed 's/0\.1\.0/0.0.9/g' "$BIN" >/tmp/agent-old && chmod 0755 /tmp/agent-old
/tmp/agent-old --version | grep -q '0\.0\.9' || die "le binaire ancien ne dit pas 0.0.9 : $(/tmp/agent-old --version)"
"$INSTALLED" uninstall --keep-data --yes >/dev/null 2>&1 || die "désinstallation avant la mise à niveau"
run /tmp/agent-old install --yes || die "l'installation de la version 0.0.9 a échoué"
[ "$(printed_fingerprint)" = "$FIRST_FP" ] || die "l'empreinte a changé en réinstallant l'ancienne version"
"$INSTALLED" --version | grep -q '0\.0\.9' || die "la version installée n'est pas 0.0.9"
OLD_PID=$(systemctl show -p MainPID --value hearth-agent)
run "$BIN" install --yes || die "la mise à niveau a échoué"
must_say "Mise à jour de l'agent Hearth"
must_say "Mise à jour depuis la version 0.0.9..."
must_say "Mise à jour réussie. Tes comptes et données sont conservés."
"$INSTALLED" --version | grep -q '0\.1\.0' || die "la version installée n'est pas 0.1.0 après la mise à niveau"
[ "$(systemctl show -p MainPID --value hearth-agent)" != "$OLD_PID" ] || die "le service aurait dû redémarrer sur le nouveau binaire"
[ "$(printed_fingerprint)" = "$FIRST_FP" ] || die "l'empreinte a changé à la mise à niveau"
[ "$(login)" = 201 ] || die "la connexion échoue après la mise à niveau"
ok "mise à niveau 0.0.9 vers 0.1.0 : service redémarré sur le nouveau binaire, mêmes comptes, même empreinte"
CUR_PID=$(systemctl show -p MainPID --value hearth-agent)
if run /tmp/agent-old install --yes; then
    die "rétrograder devrait être refusé"
fi
must_say "plus récente"
[ "$(systemctl show -p MainPID --value hearth-agent)" = "$CUR_PID" ] || die "le service a été touché par le refus"
"$INSTALLED" --version | grep -q '0\.1\.0' || die "le binaire a été remplacé malgré le refus"
ok "version plus récente déjà installée : refus, rien n'est touché"

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
# Désinstallation avec purge : plus rien de Hearth, mais rien d'autre n'est touché
# --------------------------------------------------------------------------------------------
printf 'pas à Hearth' >/var/lib/hearth/notes.txt
run "$INSTALLED" uninstall --purge --yes || die "la désinstallation avec purge a échoué"
must_say "n'ont pas été touchés : notes.txt"
[ "$(cat /var/lib/hearth/notes.txt)" = "pas à Hearth" ] || die "un fichier qui n'est pas à Hearth a été touché par la purge"
for f in cert.pem key.pem install_id hearth.db; do
    [ ! -e "/var/lib/hearth/$f" ] || die "la purge a laissé $f"
done
[ ! -e "$INSTALLED" ] && [ ! -e /etc/hearth ] && [ ! -e "$UNIT" ] || die "la purge a laissé le binaire, la configuration ou l'unité"
rm -rf /var/lib/hearth
ok "purge d'un dossier de données partagé : les fichiers de Hearth partent, le fichier d'un autre reste"

run "$BIN" install --yes --port 7341 >/dev/null 2>&1 && die "sans compte ni variable, l'installation devrait demander un compte"
run env HEARTH_ADMIN_USER=$USER_NAME "HEARTH_ADMIN_PASSWORD_HASH=$HASH" "$BIN" install --yes || die "installation avant purge complète"
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
# sans écho avec confirmation, après des saisies invalides
# --------------------------------------------------------------------------------------------
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
