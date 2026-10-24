import { describe, expect, it } from "vitest";
import { typeInto } from "@/test/confirm";
import {
  bootCatalogue,
  confirmTop,
  dialogText,
  goNext,
  installPath,
  settle,
  useFicheTestClock,
} from "@/test/fiche";

// « Voir tous les réglages » dans l'installation (HRT-109, BR-FICHE-038 à 040) : les huit questions au plus restent la
// page ; tous les autres réglages de la fiche se déplient dans la page, sur leurs valeurs proposées, et ceux qu'on change
// sont comptés dans le récapitulatif. Sur le pont simulé dont l'agent juge comme le vrai.
useFicheTestClock();

const rowInput = (id: string) =>
  document.querySelector<HTMLInputElement>(
    `[data-install-settings-body] [data-setting="${id}"] input`,
  );

describe("le dépliant (BR-FICHE-040)", () => {
  it("est replié, ne répète pas les questions et s'ouvre dans la page sans nouvelle étape", async () => {
    const { wrapper } = await bootCatalogue(undefined, installPath("terraria"));
    expect(wrapper.find("[data-install-settings-body]").exists()).toBe(false);
    expect(wrapper.findAll(".trail__step")).toHaveLength(3);
    await wrapper.get("[data-install-settings-toggle]").trigger("click");
    await settle();
    const body = wrapper.get("[data-install-settings-body]");
    // 28 réglages à régler, dont 6 demandés plus haut : les 22 autres sont là (ceux que la fiche impose ont leur carte).
    expect(body.text()).toContain("28 réglages, dont 6 demandés plus haut.");
    expect(body.findAll("[data-setting]")).toHaveLength(22);
    expect(body.find('[data-setting="port"]').exists()).toBe(false);
    expect(body.find('[data-setting="max_players"]').exists()).toBe(false);
    expect(wrapper.findAll(".trail__step")).toHaveLength(3);
    wrapper.unmount();
  });

  it("cherche dans tous les thèmes et garde la valeur proposée tant qu'on n'y touche pas", async () => {
    const { wrapper } = await bootCatalogue(undefined, installPath("terraria"));
    await wrapper.get("[data-install-settings-toggle]").trigger("click");
    await settle();
    typeInto(
      document.querySelector("[data-install-settings-body] [data-settings-search] input"),
      "fluidite",
    );
    await settle();
    expect(
      [...document.querySelectorAll("[data-install-settings-body] [data-setting]")].map((item) =>
        item.getAttribute("data-setting"),
      ),
    ).toEqual(["npc_stream"]);
    expect(wrapper.find("[data-install-settings-changed]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("dit que l'import attend le service installé, bouton présent et inactif", async () => {
    const { wrapper } = await bootCatalogue(undefined, installPath("terraria"));
    await wrapper.get("[data-install-settings-toggle]").trigger("click");
    await settle();
    const button = wrapper.get("[data-install-settings-import]");
    expect(button.attributes("aria-disabled") ?? button.attributes("disabled")).toBeDefined();
    expect(wrapper.get("[data-install-settings-body]").text()).toContain(
      "L'import de réglages se fait une fois le service installé, dans son onglet Réglages.",
    );
    wrapper.unmount();
  });
});

describe("un réglage changé part avec l'installation (BR-FICHE-040, BR-FICHE-016)", () => {
  it("est compté, listé dans le récapitulatif et envoyé à l'agent comme une réponse", async () => {
    const { wrapper, bridge } = await bootCatalogue(undefined, installPath("terraria"));
    await wrapper.get("[data-install-settings-toggle]").trigger("click");
    await settle();
    typeInto(rowInput("seed"), "graine-du-monde");
    await settle();
    expect(wrapper.get("[data-install-settings-changed]").text()).toBe(
      "1 réglage(s) différent(s) des valeurs proposées.",
    );
    await goNext(wrapper);
    await goNext(wrapper);
    expect(dialogText()).toContain("Graine du monde : graine-du-monde");
    expect(dialogText()).toContain("1 réglage(s) différent(s) des valeurs proposées.");
    await confirmTop("Installer le serveur de jeu");
    await settle();
    const received = bridge.catalog.received[0]?.request;
    expect(received?.answers).toContainEqual({ id: "seed", value: "graine-du-monde" });
    // Un réglage non touché ne part pas : la fiche garde sa valeur proposée.
    expect(received?.answers.some((item) => item.id === "secure")).toBe(false);
    // L'agent d'essai donne à ce service ses réglages, avec la valeur choisie.
    const service = bridge.deploys.running("forge")[0];
    expect(service).toBeDefined();
    if (service) {
      const read = bridge.gameSettings.read("forge", service.key);
      expect(read.kind === "found" && read.view.settings.find((s) => s.id === "seed")?.value).toBe(
        "graine-du-monde",
      );
    }
    wrapper.unmount();
  });

  it("refuse une valeur hors des bornes de la fiche avant de continuer", async () => {
    const { wrapper } = await bootCatalogue(undefined, installPath("terraria"));
    await wrapper.get("[data-install-settings-toggle]").trigger("click");
    await settle();
    typeInto(rowInput("announcement_box_range"), "abc");
    await settle();
    await wrapper.get("[data-install-next]").trigger("click");
    await settle();
    // L'étape ne change pas : la page « Réponses » montre le défaut.
    expect(wrapper.find("[data-install-resources]").exists()).toBe(false);
    wrapper.unmount();
  });
});
