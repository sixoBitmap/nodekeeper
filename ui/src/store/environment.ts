import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { Environment } from "@/bindings/Environment";

interface EnvironmentState {
  environments: Environment[];
  /** Which environment the UI currently shows. Switching only changes
   * this — it never starts or stops anything (docs/SPEC.md item 10). */
  selectedChain: Environment["chain"] | null;
  loaded: boolean;
  load: () => Promise<void>;
  select: (chain: Environment["chain"]) => void;
}

export const useEnvironmentStore = create<EnvironmentState>()((set, get) => ({
  environments: [],
  selectedChain: null,
  loaded: false,
  load: async () => {
    const environments = await invoke<Environment[]>("list_default_environments");
    set({
      environments,
      loaded: true,
      selectedChain: get().selectedChain ?? environments[0]?.chain ?? null,
    });
  },
  select: (chain) => set({ selectedChain: chain }),
}));

export function selectedEnvironment(state: EnvironmentState): Environment | undefined {
  return state.environments.find((e) => e.chain === state.selectedChain);
}
