import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { TauriLinkBridge } from "./tauri";

// Déployer une image (HRT-69) : la moitié « interface » est livrée sur le pont simulé ; les commandes de la coquille
// Rust n'existent pas encore. Le pont réel ne simule JAMAIS un succès : il échoue en « incident interne » et n'envoie
// rien à la coquille, de sorte que le bouton « Déployer un service » ne ment pas tant que la liaison n'est pas écrite.
afterEach(() => clearMocks());

describe("le pont réel, déploiement sans coquille", () => {
  it("échoue sans rien envoyer, sur les quatre appels", async () => {
    const sent: string[] = [];
    mockIPC((cmd) => {
      sent.push(cmd);
      return null;
    });
    const bridge = new TauriLinkBridge();
    const request = {
      name: "web",
      image: "nginx",
      tag: null,
      ports: [],
      env: [{ name: "A", value: "SENTINEL" }],
      command: null,
      resources: { memoryBytes: 1024 ** 3, cpuMillis: 1000 },
      accept: { reservedPorts: false, lowDisk: false },
    };
    const failure = { failure: { kind: "internal" } };
    await expect(
      bridge.checkDeployment("forge", { name: "web", image: "nginx", tag: null, ports: [] }),
    ).rejects.toMatchObject(failure);
    await expect(bridge.deployService("forge", request, "mot de passe")).rejects.toMatchObject(
      failure,
    );
    await expect(bridge.cancelServiceOperation("forge", "01OP", null)).rejects.toMatchObject(
      failure,
    );
    await expect(bridge.readEngineCapacity("forge")).rejects.toMatchObject(failure);
    expect(sent).toEqual([]);
  });
});
