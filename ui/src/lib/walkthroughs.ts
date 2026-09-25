import type { Screen } from "@/types/screen";
import type { WalkthroughId } from "@/store/walkthrough";

/**
 * How a step's checkpoint is judged done:
 * - "none": no machine-checkable state change (e.g. "look at your
 *   receive address") -- the step's Continue button is always enabled.
 * - Everything else polls real backend state on Regtest; the banner
 *   (`WalkthroughBanner`) owns the actual polling/comparison logic,
 *   this just says which kind of check a step wants.
 */
export type CheckpointKind =
  | "none"
  | "walletExists"
  | "balancePositive"
  | "blocksIncreased"
  | "inscriptionsIncreased"
  | "satHasTwoInscriptions"
  | "consoleRanSince"
  | "scriptRanSince";

export interface WalkthroughStepDef {
  screen: Screen;
  checkpoint: CheckpointKind;
}

export interface WalkthroughDef {
  id: WalkthroughId;
  steps: WalkthroughStepDef[];
  /** Walkthrough "c" ("reinscribe that inscription") needs an existing
   * inscription to reinscribe -- picked once, before the walkthrough
   * starts (see `TestLabWalkthroughs`), not as a step of its own. */
  requiresExistingInscription: boolean;
}

/** docs/SPEC.md item 11's guided walkthroughs a, b, c, e (see
 * `WalkthroughId`'s comment for why "d" is missing). Step count and
 * screens mirror the spec's own written sequences exactly. */
export const WALKTHROUGHS: WalkthroughDef[] = [
  {
    id: "a",
    requiresExistingInscription: false,
    steps: [
      { screen: "wallet", checkpoint: "walletExists" },
      { screen: "wallet", checkpoint: "none" },
      { screen: "testLab", checkpoint: "blocksIncreased" },
      { screen: "wallet", checkpoint: "none" },
    ],
  },
  {
    id: "b",
    requiresExistingInscription: false,
    steps: [
      { screen: "inscribe", checkpoint: "none" },
      { screen: "testLab", checkpoint: "blocksIncreased" },
      { screen: "wallet", checkpoint: "inscriptionsIncreased" },
    ],
  },
  {
    id: "c",
    requiresExistingInscription: true,
    steps: [
      { screen: "inscribe", checkpoint: "none" },
      { screen: "testLab", checkpoint: "blocksIncreased" },
      { screen: "explorer", checkpoint: "satHasTwoInscriptions" },
    ],
  },
  {
    id: "e",
    requiresExistingInscription: false,
    steps: [
      { screen: "console", checkpoint: "consoleRanSince" },
      { screen: "scripts", checkpoint: "scriptRanSince" },
    ],
  },
];

export function walkthroughById(id: WalkthroughId): WalkthroughDef {
  const def = WALKTHROUGHS.find((w) => w.id === id);
  if (!def) throw new Error(`unknown walkthrough: ${id}`);
  return def;
}
