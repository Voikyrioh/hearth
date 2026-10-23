#!/usr/bin/env python3
"""Confirmation d'un acte d'administration pour les scénarios de bout en bout (HRT-30, ADR-0033).

L'agent EXIGE la confirmation (mot de passe + preuve de la clé d'un poste inscrit, usage 0x05) : un
scénario en shell + curl ne peut plus lancer la mise à jour de l'agent avec la session seule. Ce script fait
ce que fait la liaison cliente, avec `openssl` pour Ed25519 :

    admin_act.py enroll  HOST PORT USER PASSWORD KEYFILE      inscrit la clé de ce « poste » (connexion avec preuve)
    admin_act.py confirm HOST PORT TOKEN USER PASSWORD KEYFILE   lit un corps JSON de mise à jour sur l'entrée
                                                               standard et rend le même corps + `reauth`
    admin_act.py confirm-adopt HOST PORT TOKEN USER PASSWORD KEYFILE
                                                               lit `{"keys": [...]}` (adopter des conteneurs,
                                                               code d'acte 0x11, HRT-80) et rend le corps + `reauth`
    admin_act.py confirm-release HOST PORT TOKEN USER PASSWORD KEYFILE KEY
                                                               lit un corps JSON (`{}`) et le rend + `reauth` pour
                                                               `POST /services/KEY/release` (code d'acte 0x12)
    admin_act.py confirm-service VERB HOST PORT TOKEN USER PASSWORD KEYFILE KEY
                                                               lit un corps JSON (`{}`) et le rend + `reauth` pour
                                                               `POST /services/KEY/VERB`, VERB étant `start`, `stop`
                                                               ou `restart` (codes d'acte 0x0C, 0x0D, 0x0E, HRT-64) ;
                                                               PASSWORD vide : le membre `reauth` n'a pas de mot de
                                                               passe (état normal, HRT-194)
    admin_act.py confirm-deploy HOST PORT TOKEN USER PASSWORD KEYFILE
                                                               lit le corps de `POST /deployments` (nom, image,
                                                               ports, montages, limites…) et le rend + `reauth`
                                                               (code d'acte 0x0F, HRT-68) ; jamais couvert par
                                                               l'élévation, ni valeur ni nom de variable signés
    admin_act.py confirm-cancel HOST PORT TOKEN USER PASSWORD KEYFILE ID
                                                               lit un corps JSON (`{}`) et le rend + `reauth` pour
                                                               `DELETE /operations/services/ID` (code d'acte 0x10,
                                                               annuler un déploiement, HRT-68)

Les actes pris en charge sont la mise à jour de l'agent (code 0x06), les deux actes d'adoption de services
(0x11, 0x12), les trois actes de cycle de vie d'un service (0x0C, 0x0D, 0x0E), déployer (0x0F) et annuler une
opération (0x10) : ce sont les seuls que les scénarios envoient. La disposition du message signé est celle de
`hearth_proto::device_proof::signing_bytes` ; la cible signée d'une adoption est le nombre et l'empreinte
des clés triées (`hearth_proto::admin_act::AdminAct::services_adopt`).
Aucun secret n'est écrit sur la sortie d'erreur.
"""

import base64
import hashlib
import http.client
import json
import os
import ssl
import subprocess
import sys
import tempfile

DOMAIN = b"hearth-device-proof/1"
USAGE_LOGIN = 0x01
USAGE_ADMIN_ACT = 0x05
ACT_AGENT_UPDATE = 0x06
# Déclarés d'un coup par HRT-144 (`hearth_proto::admin_act::ActKind`). Démarrer, arrêter et redémarrer un service
# ont leur route depuis HRT-64 (`confirm_service`) ; les autres n'ont pas encore de route, mais la table des codes
# est ici pour qui en ajoutera un (une route à venir par lot).
ACT_SERVICE_START = 0x0C
ACT_SERVICE_STOP = 0x0D
ACT_SERVICE_RESTART = 0x0E
ACT_SERVICE_DEPLOY = 0x0F
ACT_OPERATION_CANCEL = 0x10
ACT_SERVICES_ADOPT = 0x11
ACT_SERVICE_RELEASE = 0x12
ACT_SERVICE_DELETE = 0x14
ACT_BACKUP_CREATE = 0x19
ACT_BACKUP_DELETE = 0x1B
ACT_OPERATION_DECIDE = 0x1D
ACT_GAME_SETTINGS_SAVE = 0x1E


def connect(host, port):
    context = ssl.create_default_context()
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE
    connection = http.client.HTTPSConnection(host, int(port), context=context, timeout=30)
    connection.connect()
    return connection


def post(connection, path, body):
    connection.request(
        "POST",
        "/api/v1" + path,
        json.dumps(body),
        {"content-type": "application/json", "x-hearth-api": "1", "x-hearth-client": "e2e/1"},
    )
    response = connection.getresponse()
    return response.status, json.loads(response.read() or b"{}")


def fingerprint(connection):
    """L'empreinte SHA-256 du certificat présenté : celle que les preuves signent."""
    return hashlib.sha256(connection.sock.getpeercert(binary_form=True)).digest()


def ensure_key(keyfile):
    if not os.path.exists(keyfile):
        subprocess.check_call(["openssl", "genpkey", "-algorithm", "ed25519", "-out", keyfile])
        os.chmod(keyfile, 0o600)


def public_key(keyfile):
    der = subprocess.check_output(["openssl", "pkey", "-in", keyfile, "-pubout", "-outform", "DER"])
    return der[-32:]


def sign(keyfile, message):
    with tempfile.TemporaryDirectory() as folder:
        message_path = os.path.join(folder, "message")
        signature_path = os.path.join(folder, "signature")
        with open(message_path, "wb") as handle:
            handle.write(message)
        subprocess.check_call(
            ["openssl", "pkeyutl", "-sign", "-inkey", keyfile, "-rawin", "-in", message_path, "-out", signature_path]
        )
        with open(signature_path, "rb") as handle:
            return handle.read()


def head(usage, fp, user, challenge):
    identifier = user.strip().lower().encode()
    return DOMAIN + b"\x00" + bytes([usage]) + fp + len(identifier).to_bytes(2, "big") + identifier + challenge


def device_member(keyfile, message, challenge_text):
    return {
        "algorithm": "ed25519",
        "public_key": base64.b64encode(public_key(keyfile)).decode(),
        "challenge": challenge_text,
        "signature": base64.b64encode(sign(keyfile, message)).decode(),
    }


def challenge_for(connection, user, purpose):
    status, body = post(connection, "/sessions/challenge", {"username": user, "purpose": purpose})
    if status != 200:
        sys.exit(f"défi refusé : {status}")
    text = body["challenge"]
    return text, base64.b64decode(text)


def enroll(host, port, user, password, keyfile):
    ensure_key(keyfile)
    connection = connect(host, port)
    text, raw = challenge_for(connection, user, "login")
    message = head(USAGE_LOGIN, fingerprint(connection), user, raw)
    status, body = post(
        connection,
        "/sessions",
        {"username": user, "password": password, "device": device_member(keyfile, message, text)},
    )
    if status != 201:
        sys.exit(f"connexion avec preuve refusée : {status}")
    print(body.get("device", ""))


def confirm(host, port, token, user, password, keyfile):
    request = json.load(sys.stdin)
    connection = connect(host, port)
    text, raw = challenge_for(connection, user, "admin_act")
    token_hash = hashlib.sha256(bytes.fromhex(token)).digest()
    version = request["version"].encode()
    checksum = request["sha256"].lower().encode()
    message = head(USAGE_ADMIN_ACT, fingerprint(connection), user, raw)
    message += token_hash + bytes([ACT_AGENT_UPDATE]) + (0).to_bytes(2, "big") + bytes([2])
    for param in (version, checksum):
        message += len(param).to_bytes(2, "big") + param
    request["reauth"] = {"password": password, "device": device_member(keyfile, message, text)}
    print(json.dumps(request))


def act_tail(code, target, params):
    """Fin du message signé d'un acte : code, cible, paramètres (chacun précédé de sa longueur sur 2 octets)."""
    tail = bytes([code]) + len(target).to_bytes(2, "big") + target + bytes([len(params)])
    for param in params:
        tail += len(param).to_bytes(2, "big") + param
    return tail


def adoption_params(keys):
    """Le nombre de clés et l'empreinte SHA-256 de leurs clés triées, chacune précédée de sa longueur."""
    digest = hashlib.sha256()
    for key in sorted(keys, key=lambda k: k.encode()):
        raw = key.encode()
        digest.update(len(raw).to_bytes(2, "big") + raw)
    return [str(len(keys)).encode(), digest.hexdigest().encode()]


def signed_member(host, port, token, user, password, keyfile, tail):
    connection = connect(host, port)
    text, raw = challenge_for(connection, user, "admin_act")
    token_hash = hashlib.sha256(bytes.fromhex(token)).digest()
    message = head(USAGE_ADMIN_ACT, fingerprint(connection), user, raw) + token_hash + tail
    member = {"device": device_member(keyfile, message, text)}
    # Un mot de passe vide n'est pas envoyé (HRT-194 : piloter un service en état normal n'en demande pas).
    if password:
        member["password"] = password
    return (member,)


def confirm_adopt(host, port, token, user, password, keyfile):
    request = json.load(sys.stdin)
    tail = act_tail(ACT_SERVICES_ADOPT, b"", adoption_params(request["keys"]))
    (request["reauth"],) = signed_member(host, port, token, user, password, keyfile, tail)
    print(json.dumps(request))


def confirm_release(host, port, token, user, password, keyfile, key):
    request = json.load(sys.stdin)
    tail = act_tail(ACT_SERVICE_RELEASE, key.encode(), [])
    (request["reauth"],) = signed_member(host, port, token, user, password, keyfile, tail)
    print(json.dumps(request))


def put(digest, raw):
    """Un texte précédé de sa longueur sur 4 octets (`hearth_proto::api::service_spec::deploy_digest`)."""
    digest.update(len(raw).to_bytes(4, "big") + raw)


def split_image(image, tag):
    """`(dépôt, version)` comme `hearth_proto::api::service_spec::split_image` : la version écrite dans l'image
    passe dans le champ Version, une version vide vaut `latest`."""
    image = image.strip()
    after_slash = image.rfind("/") + 1
    colon = image[after_slash:].rfind(":")
    if colon >= 0:
        repository, embedded = image[: after_slash + colon], image[after_slash + colon + 1 :]
    else:
        repository, embedded = image, None
    chosen = embedded or (tag or "").strip() or "latest"
    return repository, chosen


def deploy_digest(spec):
    """L'empreinte de la partie non secrète d'un déploiement : la même que `deploy_digest` de `hearth-proto`."""
    digest = hashlib.sha256()
    put(digest, b"hearth/deploy-digest/1")
    put(digest, spec["name"].encode())
    repository, tag = split_image(spec.get("image", ""), spec.get("tag"))
    put(digest, repository.encode())
    put(digest, tag.encode())
    ports = sorted(
        (p["host_port"], p["container_port"], 1 if p.get("protocol", "tcp") == "udp" else 0)
        for p in spec.get("ports", [])
    )
    digest.update(len(ports).to_bytes(4, "big"))
    for host, container, udp in ports:
        digest.update(host.to_bytes(2, "big") + container.to_bytes(2, "big"))
        put(digest, b"udp" if udp else b"tcp")
    mounts = sorted(
        (
            m["target"],
            1 if m.get("place", "managed") == "host" else 0,
            m.get("host_path") is not None,
            m.get("host_path") or "",
            bool(m.get("read_only", False)),
            bool(m.get("create", False)),
            bool(m.get("confirmed", False)),
        )
        for m in spec.get("mounts", [])
    )
    digest.update(len(mounts).to_bytes(4, "big"))
    for target, host, _, host_path, read_only, create, confirmed in mounts:
        put(digest, target.encode())
        put(digest, b"host" if host else b"managed")
        put(digest, host_path.encode())
        digest.update(bytes([int(read_only), int(create), int(confirmed)]))
    resources = spec.get("resources", {})
    cpu, memory = resources.get("cpu_millis"), resources.get("memory_bytes")
    digest.update(b"\x00" if cpu is None else b"\x01" + cpu.to_bytes(4, "big"))
    digest.update(b"\x00" if memory is None else b"\x01" + memory.to_bytes(8, "big"))
    accept = spec.get("accept", {})
    digest.update(bytes([int(bool(accept.get("reserved_ports"))), int(bool(accept.get("low_disk")))]))
    source = spec.get("source")
    if source and source.get("type") == "fiche":
        # Une fiche (HRT-101) : sa référence, ses choix, les IDENTIFIANTS des réponses, jamais une valeur.
        put(digest, b"fiche/1")
        put(digest, source["id"].encode())
        digest.update(source["version"].to_bytes(4, "big"))
        put(digest, source["digest"].encode())
        put(digest, (source.get("variant") or "").encode())
        for items in (
            sorted(a["id"] for a in source.get("answers", [])),
            sorted(source.get("agreements", [])),
        ):
            digest.update(len(items).to_bytes(4, "big"))
            for item in items:
                put(digest, item.encode())
        chosen = sorted((p["id"], p["host_port"]) for p in source.get("ports", []))
        digest.update(len(chosen).to_bytes(4, "big"))
        for port_id, host in chosen:
            put(digest, port_id.encode())
            digest.update(host.to_bytes(2, "big"))
        optional = sorted(source.get("optional_ports", []))
        digest.update(len(optional).to_bytes(4, "big"))
        for port_id in optional:
            put(digest, port_id.encode())
        digest.update(bytes([int(bool(source.get("world_imported")))]))
    return digest.hexdigest()


def deploy_params(spec):
    """Les paramètres signés d'un déploiement : `dépôt:version`, l'empreinte, le NOMBRE de variables, `cmd:0|1`.
    Jamais une valeur, un nom de variable ni la commande."""
    source = spec.get("source")
    fiche = bool(source and source.get("type") == "fiche")
    if fiche:
        image = f"fiche:{source['id']}"
    else:
        repository, tag = split_image(spec["image"], spec.get("tag"))
        image = f"{repository}:{tag}"
    command = bool((spec.get("command") or "").strip())
    params = [
        image.encode(),
        deploy_digest(spec).encode(),
        str(len(spec.get("env", []))).encode(),
        b"1" if command else b"0",
    ]
    if fiche:
        params += [
            source["id"].encode(),
            str(source["version"]).encode(),
            source["digest"].encode(),
            str(len(source.get("answers", []))).encode(),
            b"1" if source.get("world_imported") else b"0",
        ]
    return params


def confirm_deploy(host, port, token, user, password, keyfile):
    """Déployer une image (HRT-68) : la cible signée est le nom du service."""
    request = json.load(sys.stdin)
    tail = act_tail(ACT_SERVICE_DEPLOY, request["name"].encode(), deploy_params(request))
    (request["reauth"],) = signed_member(host, port, token, user, password, keyfile, tail)
    print(json.dumps(request))


def confirm_cancel(host, port, token, user, password, keyfile, operation):
    """Annuler une opération de service (HRT-144) : la cible signée est son identifiant, sans paramètre."""
    request = json.load(sys.stdin)
    tail = act_tail(ACT_OPERATION_CANCEL, operation.encode(), [])
    (request["reauth"],) = signed_member(host, port, token, user, password, keyfile, tail)
    print(json.dumps(request))


SERVICE_ACTS = {
    "start": ACT_SERVICE_START,
    "stop": ACT_SERVICE_STOP,
    "restart": ACT_SERVICE_RESTART,
}


def confirm_service(verb, host, port, token, user, password, keyfile, key):
    """Démarrer, arrêter, redémarrer un service (HRT-64) : la cible signée est la clé du service, sans paramètre."""
    if verb not in SERVICE_ACTS:
        sys.exit("geste inconnu : start, stop ou restart")
    request = json.load(sys.stdin)
    tail = act_tail(SERVICE_ACTS[verb], key.encode(), [])
    (request["reauth"],) = signed_member(host, port, token, user, password, keyfile, tail)
    print(json.dumps(request))


def main():
    command, args = sys.argv[1], sys.argv[2:]
    if command == "enroll":
        enroll(*args)
    elif command == "confirm":
        confirm(*args)
    elif command == "confirm-adopt":
        confirm_adopt(*args)
    elif command == "confirm-release":
        confirm_release(*args)
    elif command == "confirm-service":
        confirm_service(*args)
    elif command == "confirm-deploy":
        confirm_deploy(*args)
    elif command == "confirm-cancel":
        confirm_cancel(*args)
    elif command == "deploy-digest":
        # Sans réseau : l'empreinte d'un corps lu sur l'entrée standard (le test de `hearth-proto` la compare).
        print(deploy_digest(json.load(sys.stdin)))
    else:
        sys.exit("commande inconnue")


if __name__ == "__main__":
    main()
