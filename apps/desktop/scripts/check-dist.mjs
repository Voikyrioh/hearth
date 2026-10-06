// Vérifie que le build livré (dist/) ne contient ni le pont de liaison simulé ni le panneau
// de développement ni leurs données d'exemple (ADR-0010). À lancer après `npm run build`.
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const FORBIDDEN = [
  "SimulatedLinkBridge",
  "__hearthSim",
  "SimulatedUpdateBridge",
  "__hearthUpdateSim",
  "DevLinkPanel",
  "Simulation du lien",
  "nas-salon",
  "192.168.1.120",
  // Page de diagnostic du build de test (mode e2e) : jamais dans le build livré.
  "diagnostic-crash",
  "Diagnostic rétabli",
  "volontairement cassé",
];

const dist = new URL("../dist/", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const files = [];
(function walk(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) walk(path);
    else if (/\.(js|css|html|map)$/.test(entry.name)) files.push(path);
  }
})(decodeURIComponent(dist));

const found = [];
for (const file of files) {
  const text = readFileSync(file, "utf8");
  for (const word of FORBIDDEN) if (text.includes(word)) found.push(`${file} : « ${word} »`);
}

if (found.length > 0) {
  console.error("Le build livré contient du code ou des données de simulation :");
  for (const line of found) console.error(`  ${line}`);
  process.exit(1);
}
console.log(`dist/ propre (${files.length} fichiers vérifiés, aucun code de simulation).`);
