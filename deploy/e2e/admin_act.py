#!/usr/bin/env python3
"""Confirmation d'un acte d'administration pour les scénarios de bout en bout (HRT-30, ADR-0033).

L'agent EXIGE la confirmation (mot de passe + preuve de la clé d'un poste inscrit, usage 0x05) : un
scénario en shell + curl ne peut plus lancer la mise à jour de l'agent avec la session seule. Ce script fait
ce que fait la liaison cliente, avec `openssl` pour Ed25519 :

    admin_act.py enroll  HOST PORT USER PASSWORD KEYFILE      inscrit la clé de ce « poste » (connexion avec preuve)
    admin_act.py confirm HOST PORT TOKEN USER PASSWORD KEYFILE   lit un corps JSON de mise à jour sur l'entrée
                                                               standard et rend le même corps + `reauth`

Seule la mise à jour de l'agent (code d'acte 0x06) est prise en charge : c'est le seul acte que les scénarios
envoient. La disposition du message signé est celle de `hearth_proto::device_proof::signing_bytes`.
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


def main():
    command, args = sys.argv[1], sys.argv[2:]
    if command == "enroll":
        enroll(*args)
    elif command == "confirm":
        confirm(*args)
    else:
        sys.exit("commande inconnue")


if __name__ == "__main__":
    main()
