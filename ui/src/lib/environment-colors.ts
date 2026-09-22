import type { Chain } from "@/bindings/Chain";

// Color is presentational-only and intentionally lives here, not in the
// Rust Environment type (see ARCHITECTURE.md) — these are the
// `--color-env-*` tokens registered in ui/src/index.css's `@theme` block.
//
// Every class name below is written out in full (not built with template
// strings / concatenation): Tailwind's build-time scanner only picks up
// class names it can find as literal text in the source, so
// `` `bg-env-${chain}` `` would silently produce no CSS at all.
const CHAIN_BG_CLASS: Record<Chain, string> = {
  mainnet: "bg-env-mainnet",
  regtest: "bg-env-regtest",
  signet: "bg-env-signet",
  testnet4: "bg-env-testnet4",
};

const CHAIN_TEXT_CLASS: Record<Chain, string> = {
  mainnet: "text-env-mainnet",
  regtest: "text-env-regtest",
  signet: "text-env-signet",
  testnet4: "text-env-testnet4",
};

const CHAIN_BORDER_CLASS: Record<Chain, string> = {
  mainnet: "border-env-mainnet",
  regtest: "border-env-regtest",
  signet: "border-env-signet",
  testnet4: "border-env-testnet4",
};

export function chainBgClass(chain: Chain): string {
  return CHAIN_BG_CLASS[chain];
}

export function chainTextClass(chain: Chain): string {
  return CHAIN_TEXT_CLASS[chain];
}

export function chainBorderClass(chain: Chain): string {
  return CHAIN_BORDER_CLASS[chain];
}
