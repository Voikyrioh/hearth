"""Serveur HTTPS minimal des scénarios de bout en bout : sert un dossier, journalise les chemins
demandés dans /tmp/https.log, redirige /redirect vers une adresse en clair (le script
d'installation doit le refuser) et sert /slow/<fichier> lentement (une mise à jour y reste « en
cours » assez longtemps pour qu'une deuxième demande arrive). Certificat : /tmp/pki, approuvé par
le conteneur."""

import http.server
import os
import ssl
import sys
import time

port = int(sys.argv[1])
root = sys.argv[2]


class Handler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/redirect":
            self.send_response(302)
            self.send_header("Location", "http://127.0.0.1:1/hearth-agent")
            self.end_headers()
            return
        if self.path.startswith("/slow/"):
            self.serve_slowly(self.path[len("/slow/"):])
            return
        super().do_GET()

    def serve_slowly(self, name):
        path = os.path.join(root, os.path.basename(name))
        if not os.path.isfile(path):
            self.send_error(404)
            return
        with open(path, "rb") as source:
            data = source.read()
        self.send_response(200)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        step = max(1, len(data) // 10)
        for start in range(0, len(data), step):
            self.wfile.write(data[start:start + step])
            self.wfile.flush()
            time.sleep(0.7)

    def log_message(self, fmt, *args):
        with open("/tmp/https.log", "a", encoding="utf-8") as log:
            log.write(self.path + "\n")


os.chdir(root)
context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain("/tmp/pki/cert.pem", "/tmp/pki/key.pem")
server = http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler)
server.socket = context.wrap_socket(server.socket, server_side=True)
server.serve_forever()
