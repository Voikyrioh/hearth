"""Serveur HTTPS minimal du scénario de bout en bout : sert un dossier, journalise les chemins
demandés dans /tmp/https.log, et redirige /redirect vers une adresse en clair (le script
d'installation doit le refuser). Certificat : /tmp/pki, approuvé par le conteneur."""

import http.server
import os
import ssl
import sys

port = int(sys.argv[1])
root = sys.argv[2]


class Handler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/redirect":
            self.send_response(302)
            self.send_header("Location", "http://127.0.0.1:1/hearth-agent")
            self.end_headers()
            return
        super().do_GET()

    def log_message(self, fmt, *args):
        with open("/tmp/https.log", "a", encoding="utf-8") as log:
            log.write(self.path + "\n")


os.chdir(root)
context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain("/tmp/pki/cert.pem", "/tmp/pki/key.pem")
server = http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler)
server.socket = context.wrap_socket(server.socket, server_side=True)
server.serve_forever()
