import { flushPromises } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import type { PackCheck, PackInspect, SimulatedLinkBridge } from "@/link";
import {
  answer,
  bootCatalogue,
  goNext,
  installPath,
  settle,
  useFicheTestClock,
} from "@/test/fiche";

// Installer avec un pack de mods (HRT-125, BR-MOD-006 à 009, 016) : la case, le choix du fichier, ce que l'agent a lu, les
// refus en phrases, et le pack qui part avec l'installation. Pont simulé : l'agent d'essai juge COMME LE VRAI.
useFicheTestClock();

const server: PackInspect = {
  kind: "server_pack",
  sizeBytes: 794_497_024,
  files: 2314,
  mods: 652,
  unpackedBytes: 1_650_000_000,
  topDirs: ["config", "mods"],
  modrinth: null,
};

const read = (pack: PackInspect): PackCheck => ({ kind: "read", pack });

const richWith =
  (setup: (bridge: SimulatedLinkBridge) => void) => (bridge: SimulatedLinkBridge) => {
    bridge.catalog.addRich();
    setup(bridge);
  };

async function choose(wrapper: Awaited<ReturnType<typeof bootCatalogue>>["wrapper"]) {
  await wrapper.get("[data-install-pack] input[type='checkbox']").setValue(true);
  await settle();
  await wrapper.get("[data-pack-choose]").trigger("click");
  await settle();
  await flushPromises();
}

describe("la carte « Mods »", () => {
  it("n'existe que pour une fiche qui offre le dépôt d'un pack", async () => {
    const { wrapper } = await bootCatalogue(undefined, installPath("terraria"));
    expect(wrapper.find("[data-install-pack]").exists()).toBe(false);
    wrapper.unmount();
    const rich = await bootCatalogue(
      richWith(() => undefined),
      installPath("essai"),
    );
    expect(rich.wrapper.find("[data-install-pack]").exists()).toBe(true);
    // La case n'est jamais cochée d'avance.
    expect(
      rich.wrapper.get<HTMLInputElement>("[data-install-pack] input[type='checkbox']").element
        .checked,
    ).toBe(false);
    expect(rich.wrapper.find("[data-pack-choose]").exists()).toBe(false);
    rich.wrapper.unmount();
  });

  it("montre le genre lu par l'agent et ce qu'il a compté, jamais un champ à remplir", async () => {
    const { wrapper } = await bootCatalogue(
      richWith((bridge) => bridge.packs.willDeposit("dep-1", "Prominence.zip", read(server))),
      installPath("essai"),
    );
    await choose(wrapper);
    expect(wrapper.get("[data-pack-file]").text()).toBe("Fichier choisi : Prominence.zip");
    expect(wrapper.get("[data-pack-genre]").text()).toBe("Pack serveur");
    const text = wrapper.get("[data-pack-read]").text();
    expect(text).toContain("652 mods");
    expect(text).toContain("Dossiers : config, mods");
    expect(wrapper.get("[data-pack-sentences]").text()).toContain(
      "Un mod est un programme écrit par quelqu'un d'autre.",
    );
    expect(wrapper.find("[data-pack-engine-note]").exists()).toBe(true);
    // Le genre n'est ni une liste ni un champ : seulement un texte.
    expect(wrapper.find("[data-pack-genre] input, [data-pack-genre] select").exists()).toBe(false);
    wrapper.unmount();
  });

  it("dit en trois phrases ce qu'est une liste CurseForge, et retire le fichier de la zone d'attente", async () => {
    const { wrapper } = await bootCatalogue(
      richWith((bridge) =>
        bridge.packs.willDeposit("dep-2", "liste.zip", {
          kind: "refused",
          refusal: { kind: "refused", reason: "PACK_IS_A_DOWNLOAD_LIST", mods: 652, host: null },
        }),
      ),
      installPath("essai"),
    );
    await choose(wrapper);
    const refusal = wrapper.get("[data-pack-refusal]");
    expect(refusal.findAll("p")).toHaveLength(3);
    expect(refusal.text()).toContain("seulement leur liste (652 mods)");
    expect(refusal.text()).toContain("« Server Pack »");
    expect(wrapper.find("[data-pack-read]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("dit honnêtement que l'envoi d'un fichier n'est pas encore possible, sans rien déposer", async () => {
    const { wrapper } = await bootCatalogue(
      richWith(() => undefined),
      installPath("essai"),
    );
    await choose(wrapper);
    expect(wrapper.get("[data-pack-unavailable]").text()).toContain("n'est pas encore possible");
    expect(wrapper.find("[data-pack-file]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("ne dit rien quand l'utilisateur annule le choix du fichier", async () => {
    const { wrapper } = await bootCatalogue(
      richWith((bridge) => bridge.packs.willCancel()),
      installPath("essai"),
    );
    await choose(wrapper);
    expect(wrapper.find("[data-pack-unavailable]").exists()).toBe(false);
    expect(wrapper.find("[data-pack-failed]").exists()).toBe(false);
    expect(wrapper.find("[data-pack-file]").exists()).toBe(false);
    wrapper.unmount();
  });
});

describe("le moteur et la suite", () => {
  it("choisit d'office le seul moteur que la fiche permet pour le genre du pack", async () => {
    const { wrapper } = await bootCatalogue(
      richWith((bridge) => bridge.packs.willDeposit("dep-1", "p.zip", read(server))),
      installPath("essai"),
    );
    await choose(wrapper);
    // La fiche d'essai propose « Sans mod » par défaut : un pack serveur y demande le moteur à mods, choisi d'office.
    expect(wrapper.get(".variant__card--on").text()).toContain("Avec mods");
    wrapper.unmount();
  });

  it("refuse de continuer quand le moteur choisi ne convient pas au genre du pack", async () => {
    const { wrapper } = await bootCatalogue(
      richWith((bridge) => bridge.packs.willDeposit("dep-1", "p.zip", read(server))),
      installPath("essai"),
    );
    await choose(wrapper);
    const plain = wrapper.findAll("[data-variant-card] input").find((_, index) => index === 0);
    await plain?.setValue(true);
    await settle();
    await answer(wrapper, "server_token", "jeton-du-serveur");
    await goNext(wrapper);
    expect(wrapper.get("[data-pack-issue]").text()).toContain(
      "ne s'installe pas avec le moteur choisi",
    );
    wrapper.unmount();
  });

  it("garde la case décochée par défaut et n'envoie aucun pack tant qu'elle n'est pas cochée", async () => {
    const { wrapper, bridge } = await bootCatalogue(
      richWith(() => undefined),
      installPath("essai"),
    );
    await answer(wrapper, "server_token", "jeton-du-serveur");
    await goNext(wrapper);
    expect(bridge.catalog.received).toHaveLength(0);
    wrapper.unmount();
  });
});
