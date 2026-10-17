import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { type EngineCapacity, GIB, MIB, type Resources, startingResources } from "@/resources";
import ResourceSliders from "./ResourceSliders.vue";

const capacity: EngineCapacity = {
  cpus: 16,
  memoryBytes: 64 * GIB,
  reserveBytes: 3 * GIB,
  committedMemoryBytes: 0,
};

type Recommended = { bytes: number; proof: "measured" | "publisher" } | null;

function open(
  over: { recommended?: Recommended; value?: Resources; capacity?: EngineCapacity | null } = {},
) {
  const cap = over.capacity === undefined ? capacity : over.capacity;
  const recommended = over.recommended ?? null;
  const value = over.value ?? startingResources(capacity, recommended?.bytes ?? null);
  return mount(ResourceSliders, { props: { capacity: cap, modelValue: value, recommended } });
}

type Opened = ReturnType<typeof open>;
const lastModel = (wrapper: Opened) =>
  wrapper.emitted("update:modelValue")?.at(-1)?.[0] as Resources;
const lastValid = (wrapper: Opened) => wrapper.emitted("update:valid")?.at(-1)?.[0];
const memorySlider = (wrapper: Opened) =>
  wrapper.get<HTMLInputElement>('input[aria-label="Mémoire, curseur"]');
const cpuSlider = (wrapper: Opened) => wrapper.get('input[aria-label="Processeur, curseur"]');
const fields = (wrapper: Opened) => wrapper.findAll<HTMLInputElement>('input[type="text"]');

describe("ResourceSliders (HRT-146)", () => {
  it("has no 'no limit' checkbox, switch or position", () => {
    const wrapper = open();
    expect(wrapper.find('input[type="checkbox"]').exists()).toBe(false);
    expect(wrapper.find('[role="switch"]').exists()).toBe(false);
    expect(wrapper.text().toLowerCase()).not.toContain("sans limite");
    expect(memorySlider(wrapper).attributes("min")).toBe("0");
  });

  it("starts on the value given by the caller and says it in clear units", () => {
    const wrapper = open();
    expect(memorySlider(wrapper).attributes("aria-valuetext")).toBe("2 Go");
    expect(cpuSlider(wrapper).attributes("aria-valuetext")).toBe("16 cœurs");
    expect(fields(wrapper)[0]?.element.value).toBe("2");
    expect(lastValid(wrapper)).toBe(true);
  });

  it("moves the memory slider by one stop and keeps the wire in bytes", async () => {
    const wrapper = open();
    const slider = memorySlider(wrapper);
    await slider.setValue(String(Number(slider.element.value) + 1));
    expect(lastModel(wrapper).memoryBytes).toBe(2 * GIB + 256 * MIB);
  });

  it("moves the processor slider in millicores", async () => {
    const wrapper = open();
    await cpuSlider(wrapper).setValue("3");
    expect(lastModel(wrapper).cpuMillis).toBe(1000);
  });

  it("takes an exact memory typed with a unit", async () => {
    const wrapper = open();
    await fields(wrapper)[0]?.setValue("1,5");
    await wrapper.get("select").setValue("go");
    expect(lastModel(wrapper).memoryBytes).toBe(1536 * MIB);
  });

  it("refuses an empty memory on the spot and is no longer valid", async () => {
    const wrapper = open();
    await fields(wrapper)[0]?.setValue("");
    expect(lastModel(wrapper).memoryBytes).toBeNull();
    expect(wrapper.text()).toContain("Indiquez la mémoire du service.");
  });

  it("refuses a memory out of bounds with the range", async () => {
    const wrapper = open();
    await fields(wrapper)[0]?.setValue("100");
    await wrapper.get("select").setValue("go");
    expect(lastModel(wrapper).memoryBytes).toBeNull();
    expect(wrapper.text()).toContain("La mémoire se règle entre 128 Mo et 64 Go.");
  });

  it("refuses a processor that is not a multiple of 0.25", async () => {
    const wrapper = open();
    await fields(wrapper)[1]?.setValue("2,3");
    expect(lastModel(wrapper).cpuMillis).toBeNull();
    expect(wrapper.text()).toContain(
      "Le processeur se règle entre 0,25 et 16 cœurs, par pas de 0,25.",
    );
  });

  it("becomes invalid when a value is emptied, valid again when it is fixed", async () => {
    const wrapper = open();
    await wrapper.setProps({ modelValue: { memoryBytes: null, cpuMillis: 4000 } });
    expect(lastValid(wrapper)).toBe(false);
    await wrapper.setProps({ modelValue: { memoryBytes: 2 * GIB, cpuMillis: 4000 } });
    expect(lastValid(wrapper)).toBe(true);
  });

  it("cannot be sent without the bounds, and says so", () => {
    const wrapper = open({ capacity: null });
    expect(wrapper.text()).toContain("Hearth n'a pas pu lire les limites de ce serveur.");
    expect(wrapper.find('input[type="range"]').exists()).toBe(false);
    expect(lastValid(wrapper)).toBe(false);
  });

  it("shows the recommendation and its proof under the memory slider", () => {
    const measured = open({ recommended: { bytes: 4 * GIB, proof: "measured" } });
    expect(measured.text()).toContain("Recommandé : 4 Go (mesuré).");
    const publisher = open({ recommended: { bytes: 4 * GIB, proof: "publisher" } });
    expect(publisher.text()).toContain("Recommandé : 4 Go (chiffre de l'éditeur).");
  });

  it("says Hearth has no figure for an application without a card, and what it proposes", () => {
    const wrapper = open();
    expect(wrapper.text()).toContain("Hearth n'a pas de chiffre pour ce programme.");
    expect(wrapper.text()).toContain(
      "Hearth propose 2 Go parce qu'il ne connaît pas cette application.",
    );
  });

  it("warns under the recommendation without blocking", () => {
    const warned = open({
      recommended: { bytes: 4 * GIB, proof: "measured" },
      value: { memoryBytes: 2 * GIB, cpuMillis: 4000 },
    });
    expect(warned.text()).toContain(
      "Sous la mémoire recommandée : le serveur peut ralentir ou s'arrêter.",
    );
    expect(lastValid(warned)).toBe(true);
    const fine = open({ recommended: { bytes: 4 * GIB, proof: "measured" } });
    expect(fine.text()).not.toContain("Sous la mémoire recommandée");
  });

  it("follows the caller when the value changes from outside", async () => {
    const wrapper = open();
    await wrapper.setProps({ modelValue: { memoryBytes: 6 * GIB, cpuMillis: 2000 } });
    expect(memorySlider(wrapper).attributes("aria-valuetext")).toBe("6 Go");
    expect(fields(wrapper)[0]?.element.value).toBe("6");
  });
});
