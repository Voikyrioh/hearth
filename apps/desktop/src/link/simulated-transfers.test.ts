import { describe, expect, it } from "vitest";
import { SimulatedLinkBridge } from "./simulated";
import { LinkCommandError } from "./types";

// L'agent simulé des transferts juge COMME LE VRAI (`crates/hearth-agent/src/application/transfers.rs`) : le droit et la
// confirmation (couverte par l'élévation) avant le fichier ; une reprise est un geste ; rien n'avance seul.
const PASSWORD = "Correct-Horse-9";
const PACK = "C:\\Téléchargements\\Prominence_II_Server_Pack.zip";

function bridge() {
  return new SimulatedLinkBridge();
}

describe("ouvrir un dépôt", () => {
  it("démarre un envoi avec le dernier nom du fichier, la taille et zéro octet durable", async () => {
    const sim = bridge();
    const outcome = await sim.startDeposit("forge", PACK, PASSWORD);
    expect(outcome.kind).toBe("started");
    if (outcome.kind !== "started") return;
    expect(outcome.transfer).toMatchObject({
      serverId: "forge",
      name: "Prominence_II_Server_Pack.zip",
      sent: 0,
      status: "sending",
      refusal: null,
    });
    expect(outcome.transfer.id).toMatch(/^[0-9A-Z]{26}$/);
    expect(await sim.readTransfers()).toHaveLength(1);
  });

  it("un mot de passe absent ou faux refuse l'ouverture et rien ne démarre", async () => {
    const sim = bridge();
    expect(await sim.startDeposit("forge", PACK, null)).toEqual({
      kind: "refused",
      refusal: { kind: "password_required" },
    });
    expect(await sim.startDeposit("forge", PACK, "mauvais")).toEqual({
      kind: "refused",
      refusal: { kind: "wrong_password" },
    });
    expect(await sim.readTransfers()).toEqual([]);
  });

  it("l'élévation de cinq minutes couvre l'acte : le second dépôt ne redemande pas le mot de passe", async () => {
    const sim = bridge();
    expect((await sim.startDeposit("forge", PACK, PASSWORD)).kind).toBe("started");
    expect((await sim.startDeposit("forge", PACK, null)).kind).toBe("started");
  });

  it("un fichier illisible ou qui ne tient pas sur le disque du serveur est refusé avant tout envoi", async () => {
    const sim = bridge();
    expect(await sim.startDeposit("forge", "C:\\introuvable.zip", PASSWORD)).toEqual({
      kind: "refused",
      refusal: { kind: "source_unreadable" },
    });
    const full = await sim.startDeposit("forge", "C:\\gros.zip", PASSWORD);
    expect(full).toMatchObject({ kind: "refused", refusal: { kind: "no_space" } });
    expect(await sim.readTransfers()).toEqual([]);
  });

  it("un serveur inconnu est une erreur de commande", async () => {
    await expect(bridge().startDeposit("inconnu", PACK, PASSWORD)).rejects.toBeInstanceOf(
      LinkCommandError,
    );
  });
});

describe("la vie d'un envoi, pilotée par l'appelant", () => {
  async function started() {
    const sim = bridge();
    const outcome = await sim.startDeposit("forge", PACK, PASSWORD);
    if (outcome.kind !== "started") throw new Error("envoi attendu");
    return { sim, id: outcome.transfer.id, size: outcome.transfer.size };
  }

  it("les octets durables avancent sans jamais reculer ni dépasser le fichier ; la fin remplit la jauge", async () => {
    const { sim, id, size } = await started();
    sim.transfers.advance(id, 100);
    sim.transfers.advance(id, 40);
    expect((await sim.readTransfers())[0]?.sent).toBe(100);
    sim.transfers.advance(id, size * 2);
    expect((await sim.readTransfers())[0]?.sent).toBe(size);
    sim.transfers.complete(id);
    expect((await sim.readTransfers())[0]).toMatchObject({ status: "done", sent: size });
  });

  it("une coupure laisse « interrompu » ; rien ne repart seul, reprendre est un geste", async () => {
    const { sim, id } = await started();
    sim.transfers.advance(id, 1000);
    sim.transfers.interrupt(id);
    expect((await sim.readTransfers())[0]?.status).toBe("interrupted");
    // Rien n'avance tant qu'on n'a pas repris.
    sim.transfers.advance(id, 5000);
    expect((await sim.readTransfers())[0]?.sent).toBe(1000);
    await sim.resumeTransfer(id);
    expect((await sim.readTransfers())[0]).toMatchObject({ status: "sending", sent: 1000 });
  });

  it("le manque de place coupe l'envoi sans le perdre ; un rejet l'arrête pour de bon", async () => {
    const { sim, id } = await started();
    sim.transfers.refuse(id, { kind: "no_space", missing: 10, remaining: 1 });
    expect((await sim.readTransfers())[0]).toMatchObject({
      status: "interrupted",
      refusal: { kind: "no_space" },
    });
    await sim.resumeTransfer(id);
    sim.transfers.refuse(id, { kind: "rejected" });
    expect((await sim.readTransfers())[0]).toMatchObject({
      status: "failed",
      refusal: { kind: "rejected" },
    });
  });

  it("annuler retire la partie reçue ; un envoi encore ouvert ne se retire pas de la liste", async () => {
    const { sim, id } = await started();
    await sim.dismissTransfer(id);
    expect(await sim.readTransfers()).toHaveLength(1);
    await sim.cancelTransfer(id);
    expect((await sim.readTransfers())[0]?.status).toBe("cancelled");
    await sim.dismissTransfer(id);
    expect(await sim.readTransfers()).toEqual([]);
  });

  it("la boîte « Ouvrir » rend le chemin choisi, ou rien quand le compte l'annule", async () => {
    const sim = bridge();
    expect(await sim.pickDepositFile()).toBe(sim.transfers.picked);
    sim.transfers.picked = null;
    expect(await sim.pickDepositFile()).toBeNull();
  });
});
