import type { Chain } from "@/bindings/Chain";

/** docs/SPEC.md item 11: the guided walkthroughs only ever run against
 * Regtest -- there's nothing to practice safely on any other chain.
 * `WALKTHROUGH_ENVIRONMENT_NAME` is `Environment::new_default`'s
 * display name for Regtest, used to filter `list_command_history`
 * (which is keyed by that name, not the chain literal). */
export const WALKTHROUGH_CHAIN: Chain = "regtest";
export const WALKTHROUGH_ENVIRONMENT_NAME = "Regtest";
