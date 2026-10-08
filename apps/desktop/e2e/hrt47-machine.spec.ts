import { expect, type Page, test } from "@playwright/test";

// HRT-47 S5 (PROPOSITION, à montrer à Voiky) : la carte Machine en grille libellé / valeur, les disques en lignes
// (point de montage, taille), le processeur sur deux lignes (modèle ; cœurs et fréquence).
const SIZES = [
  { width: 1100, height: 680 },
  { width: 1280, height: 800 },
  { width: 1366, height: 800 },
  { width: 1920, height: 1080 },
  { width: 2560, height: 1440 },
] as const;
const TAG = process.env.MACHINE_TAG ?? "apres";

async function longDisk(page: Page) {
  await page.waitForFunction(() =>
    Boolean((window as unknown as { __hearthSim?: object }).__hearthSim),
  );
  await page.evaluate(() => {
    const sim = (
      window as unknown as {
        __hearthSim: {
          machine: {
            machineOf(id: string): { disks: unknown[] };
            setMachine(id: string, machine: unknown): void;
          };
        };
      }
    ).__hearthSim.machine;
    const machine = sim.machineOf("forge");
    sim.setMachine("forge", {
      ...machine,
      disks: [
        ...machine.disks,
        {
          name: "overlay",
          mount:
            "/var/lib/docker/rootfs/overlayfs/516ea538beeb5e0443f9fa80d43d3f8c4c7b5fb99d71715882d9cb491826482b",
          fs: "overlay",
          totalBytes: 438 * 1024 ** 3,
          removable: false,
        },
      ],
    });
  });
}

for (const size of SIZES) {
  test(`carte Machine à ${size.width}×${size.height} (${TAG})`, async ({ page }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/dashboard");
    await longDisk(page);
    await expect(page.locator(".disk").nth(2)).toBeVisible();
    const card = page
      .locator("section.card", { has: page.getByRole("heading", { name: "Machine" }) })
      .first();
    await expect(card).toBeVisible();
    await card.screenshot({
      path: `e2e/screenshots/smoke-machine-${TAG}-${size.width}x${size.height}.png`,
    });
    if (TAG === "avant") return;
    // Grille à deux colonnes : tous les libellés partagent un bord gauche, toutes les valeurs un autre.
    const m = await card.evaluate((el) => {
      const left = (selector: string) =>
        Array.from(el.querySelectorAll(selector)).map((node) =>
          Math.round(node.getBoundingClientRect().left),
        );
      const cardBox = el.getBoundingClientRect();
      const disks = Array.from(el.querySelectorAll("[data-machine-disk]")).map((row) => {
        const box = row.getBoundingClientRect();
        return { right: box.right - cardBox.right, height: box.height };
      });
      return {
        labels: left(".machine__rows dt"),
        values: left(".machine__rows dd"),
        disks,
        model: el.querySelector("[data-machine-cpu-model]")?.textContent ?? "",
        detail: el.querySelector("[data-machine-cpu-detail]")?.textContent ?? "",
      };
    });
    expect(new Set(m.labels).size, "libellés alignés").toBe(1);
    expect(new Set(m.values).size, "valeurs alignées").toBe(1);
    expect(m.disks.length, "un rang par disque").toBe(3);
    for (const disk of m.disks) {
      expect(disk.right, "rang de disque dans la carte").toBeLessThanOrEqual(1);
      expect(disk.height, "rang de disque sur une ligne").toBeLessThanOrEqual(26);
    }
    expect(m.model).toContain("Ryzen 9");
    expect(m.detail).toMatch(/16 c.urs/);
    expect(m.detail).toMatch(/GHz/);
  });
}

// Une valeur longue d'un seul tenant (nom de machine, modèle de processeur) est coupée proprement, jamais en débordement.
for (const size of SIZES) {
  test(`carte Machine à ${size.width}×${size.height} : une valeur longue sans coupure ne déborde pas, les disques sont une liste`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/dashboard");
    await longDisk(page);
    await page.evaluate(() => {
      const sim = (
        window as unknown as {
          __hearthSim: {
            machine: {
              machineOf(id: string): { name: string; cpu: { model: string } };
              setMachine(id: string, machine: unknown): void;
            };
          };
        }
      ).__hearthSim.machine;
      const machine = sim.machineOf("forge");
      sim.setMachine("forge", {
        ...machine,
        name: "serveur-de-stockage-principal-de-la-salle-informatique-du-premier-etage",
        cpu: {
          ...machine.cpu,
          model: "AMD-Ryzen-Threadripper-PRO-7995WX-96-Core-Processor-Workstation-Edition",
        },
      });
    });
    const card = page
      .locator("section.card", { has: page.getByRole("heading", { name: "Machine" }) })
      .first();
    await expect(card.getByText("serveur-de-stockage")).toBeVisible();
    await expect(card.locator("[data-machine-disk]")).toHaveCount(3);
    const m = await card.evaluate((el) => {
      const cardBox = el.getBoundingClientRect();
      const outside = Array.from(el.querySelectorAll("*")).filter((node) => {
        const box = node.getBoundingClientRect();
        return box.width > 0 && box.right > cardBox.right + 1;
      }).length;
      const list = el.querySelector("dd ul");
      return {
        outside,
        items: list ? Array.from(list.children).map((child) => child.tagName) : [],
      };
    });
    expect(m.outside, "éléments hors de la carte").toBe(0);
    expect(m.items, "une liste, un li par disque").toEqual(["LI", "LI", "LI"]);
  });
}
