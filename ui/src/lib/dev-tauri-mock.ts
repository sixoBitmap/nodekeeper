// Lets `npm run dev` be opened directly in a regular browser (no Tauri
// shell) for fast UI iteration, by mocking the handful of IPC commands
// the app calls. Only active in dev builds, and only when there's no
// real Tauri context already — `tauri dev`/`tauri build` never load this.
import { mockIPC } from "@tauri-apps/api/mocks";
import type { Chain } from "@/bindings/Chain";
import type { Environment } from "@/bindings/Environment";
import type { NodeStatus } from "@/bindings/NodeStatus";
import type { OrdStatus } from "@/bindings/OrdStatus";
import type { SystemCheck } from "@/bindings/SystemCheck";
import type { LogWindow } from "@/bindings/LogWindow";
import type { TypedError } from "@/bindings/TypedError";
import type { WalletBalance } from "@/bindings/WalletBalance";

const NO_INDEX_OPTIONS = { index_sats: false, index_runes: false, index_addresses: false };
const ALL_INDEX_OPTIONS = { index_sats: true, index_runes: true, index_addresses: true };

const MOCK_ENVIRONMENTS: Environment[] = [
  {
    chain: "mainnet",
    name: "Mainnet",
    rpc_port: 8332,
    p2p_port: 8333,
    ord_port: 8080,
    data_root: "data/mainnet",
    index_options: NO_INDEX_OPTIONS,
  },
  {
    chain: "regtest",
    name: "Regtest",
    rpc_port: 18443,
    p2p_port: 18444,
    ord_port: 8081,
    data_root: "data/regtest",
    // Matches Chain::default_index_options()'s real default: Regtest
    // is the only chain that enables everything out of the box.
    index_options: ALL_INDEX_OPTIONS,
  },
  {
    chain: "signet",
    name: "Signet",
    rpc_port: 38332,
    p2p_port: 38333,
    ord_port: 8082,
    data_root: "data/signet",
    index_options: NO_INDEX_OPTIONS,
  },
  {
    chain: "testnet4",
    name: "Testnet4",
    rpc_port: 48332,
    p2p_port: 48333,
    ord_port: 8083,
    data_root: "data/testnet4",
    index_options: NO_INDEX_OPTIONS,
  },
];

const MOCK_SYSTEM_CHECK: SystemCheck = {
  os: "windows",
  arch: "x86_64",
  cpu_cores: 8,
  total_memory_bytes: 17_179_869_184,
  available_memory_bytes: 8_589_934_592,
  disk_free_bytes: 256_060_514_304,
};

const settings = new Map<string, string>();

// A tiny fake node lifecycle so the Dashboard has something real-ish to
// show in the browser preview: "starting" (a few seconds of headers-
// only sync) then "ready", entirely client-side.
const runningSince = new Map<Chain, number>();
// Same idea for ord: "indexing" for a few seconds, then "caught up".
const ordRunningSince = new Map<Chain, number>();
// A tiny fake wallet per chain, keyed by whether create/restore has
// "run" -- just enough for the Wallet screen's create-vs-show branch
// to have something real-ish to render in the browser preview.
const wallets = new Map<Chain, { address: string; balance: WalletBalance }>();

const MOCK_MNEMONIC =
  "wolf tiger eagle river stone flame cloud brave delta ember frost glow";
// The one passphrase the mock "wallet_send" accepts -- lets the Send
// flow's passphrase prompt / wrong-passphrase / remember-for-session
// paths all be exercised in the browser preview.
const MOCK_WALLET_PASSPHRASE = "mock-passphrase";
const rememberedPassphrases = new Map<Chain, string>();

/** Same network-prefix check ord itself does live (DECISIONS.md Phase
 * 5 VERIFY: "address ... is not valid on <chain>") -- a simplified
 * version, prefix-only, good enough for the dev preview. */
function mockAddressValidation(
  chain: Chain,
  address: string,
): { code: null; message: string } | null {
  const mainnetPrefixes = ["bc1", "1", "3"];
  const testPrefixes = ["bcrt1", "tb1", "m", "n", "2"];
  const prefixes = chain === "mainnet" ? mainnetPrefixes : testPrefixes;
  if (!prefixes.some((p) => address.startsWith(p))) {
    return { code: null, message: `error: validation error\n\nbecause:\n- address ${address} is not valid on ${chain}` };
  }
  return null;
}

function mockWallet(chain: Chain): { address: string; balance: WalletBalance } {
  // Demonstrates Foundation F gating for both directions: regtest's
  // default environment enables index-runes (ALL_INDEX_OPTIONS above),
  // so it gets a fake nonempty `runes` map; every other chain gets
  // `null` (index disabled -- absent, not empty, matching the real ord
  // behavior confirmed in DECISIONS.md Phase 5 VERIFY).
  const runes =
    chain === "regtest"
      ? [{ name: "MOCKRUNE•EXAMPLE", raw: '{"amount":"1000","symbol":"$"}' }]
      : null;
  return {
    address: `bcrt1p${chain}mockaddress0000000000000000000000000000000000000000`,
    balance: { cardinal: 4_998_990_000, ordinal: 10_000, total: 4_999_000_000, runes },
  };
}

const MOCK_INSCRIPTIONS = [
  { id: "0000000000000000000000000000000000000000000000000000000000000000i0", postage: 10_000 },
  { id: "1111111111111111111111111111111111111111111111111111111111111111i0", postage: 546 },
];

const nowSeconds = () => Math.floor(Date.now() / 1000);
const MOCK_TRANSACTIONS = [
  {
    txid: "2222222222222222222222222222222222222222222222222222222222222222",
    amount_sats: 5_000_000_000,
    confirmations: 101,
    time: nowSeconds() - 3600,
    generated: true,
  },
  {
    txid: "3333333333333333333333333333333333333333333333333333333333333333",
    amount_sats: -500_000,
    confirmations: 3,
    time: nowSeconds() - 600,
    generated: false,
  },
  {
    txid: "4444444444444444444444444444444444444444444444444444444444444444",
    amount_sats: 10_000,
    confirmations: 0,
    time: nowSeconds() - 30,
    generated: false,
  },
];

const MOCK_LOG_LINES = [
  "2026-09-23T12:00:00Z Bitcoin Core version v31.1",
  "2026-09-23T12:00:00Z Using the 'x86_shani(1way,2way)' SHA256 implementation",
  "2026-09-23T12:00:01Z Config file: (none)",
  "2026-09-23T12:00:01Z Assuming ancestors of block ... have valid signatures.",
  "2026-09-23T12:00:02Z UpdateTip: new best=00000000 height=847213 version=0x20000000",
  "2026-09-23T12:00:05Z New outbound peer connected: version=70016",
];

function mockNodeStatus(chain: Chain): NodeStatus {
  const startedAt = runningSince.get(chain) ?? Date.now();
  const uptimeSeconds = Math.max(0, Math.floor((Date.now() - startedAt) / 1000));
  const syncing = uptimeSeconds < 8;
  return {
    blocks: syncing ? Math.min(847_213, uptimeSeconds * 100_000) : 847_213,
    headers: 847_213,
    verification_progress: syncing ? uptimeSeconds / 8 : 1,
    initial_block_download: syncing,
    peers: syncing ? 2 : 9,
    mempool_transactions: 1_284,
    mempool_bytes: 3_402_112,
    disk: { used_by_data_bytes: 612_040_192_000, free_on_volume_bytes: 128_849_018_880 },
    uptime_seconds: uptimeSeconds,
  };
}

function mockOrdStatus(chain: Chain): OrdStatus {
  const startedAt = ordRunningSince.get(chain) ?? Date.now();
  const uptimeSeconds = Math.max(0, Math.floor((Date.now() - startedAt) / 1000));
  const indexing = uptimeSeconds < 6;
  const nodeHeight = mockNodeStatus(chain).blocks;
  const indexOptions =
    MOCK_ENVIRONMENTS.find((e) => e.chain === chain)?.index_options ?? NO_INDEX_OPTIONS;
  return {
    index_height: indexing ? Math.floor(nodeHeight * (uptimeSeconds / 6)) : nodeHeight,
    node_height: nodeHeight,
    caught_up: !indexing,
    index_sats: indexOptions.index_sats,
    index_runes: indexOptions.index_runes,
    index_addresses: indexOptions.index_addresses,
    uptime_seconds: uptimeSeconds,
  };
}

function notRunningError(chain: Chain): TypedError {
  return { code: null, message: `${chain} is not running` };
}

export function installDevTauriMockIfNeeded() {
  if (!import.meta.env.DEV || "__TAURI_INTERNALS__" in window) return;

  mockIPC((cmd, args) => {
    switch (cmd) {
      case "list_default_environments":
        return MOCK_ENVIRONMENTS;
      case "system_check":
        return MOCK_SYSTEM_CHECK;
      case "get_setting":
        return settings.get((args as { key: string }).key) ?? null;
      case "set_setting": {
        const { key, value } = args as { key: string; value: string };
        settings.set(key, value);
        return undefined;
      }
      case "is_node_running":
        return runningSince.has((args as { chain: Chain }).chain);
      case "start_node":
        runningSince.set((args as { chain: Chain }).chain, Date.now());
        return undefined;
      case "stop_node":
        runningSince.delete((args as { chain: Chain }).chain);
        return undefined;
      case "restart_node":
        runningSince.set((args as { chain: Chain }).chain, Date.now());
        return undefined;
      case "node_status": {
        const { chain } = args as { chain: Chain };
        if (!runningSince.has(chain)) return Promise.reject(notRunningError(chain));
        return mockNodeStatus(chain);
      }
      case "tail_debug_log": {
        const window: LogWindow = {
          lines: MOCK_LOG_LINES,
          start_offset: 0,
          reached_start_of_file: true,
        };
        return window;
      }
      case "page_debug_log_before": {
        const window: LogWindow = { lines: [], start_offset: 0, reached_start_of_file: true };
        return window;
      }
      case "search_debug_log": {
        const { query } = args as { query: string };
        return MOCK_LOG_LINES.filter((line) => line.includes(query));
      }
      case "list_command_history":
        return [];
      case "is_ord_running":
        return ordRunningSince.has((args as { chain: Chain }).chain);
      case "start_ord":
        ordRunningSince.set((args as { chain: Chain }).chain, Date.now());
        return undefined;
      case "stop_ord":
        ordRunningSince.delete((args as { chain: Chain }).chain);
        return undefined;
      case "restart_ord":
        ordRunningSince.set((args as { chain: Chain }).chain, Date.now());
        return undefined;
      case "ord_status": {
        const { chain } = args as { chain: Chain };
        if (!ordRunningSince.has(chain)) return Promise.reject(notRunningError(chain));
        return mockOrdStatus(chain);
      }
      case "wallet_exists":
        return wallets.has((args as { chain: Chain }).chain);
      case "create_wallet": {
        const { chain, passphrase } = args as { chain: Chain; passphrase: string | null };
        if (chain === "mainnet" && !passphrase) {
          return Promise.reject({
            code: null,
            message: "Mainnet wallets must be encrypted -- enter a passphrase.",
          });
        }
        wallets.set(chain, mockWallet(chain));
        return { mnemonic: MOCK_MNEMONIC };
      }
      case "restore_wallet": {
        const { chain, passphrase } = args as { chain: Chain; passphrase: string | null };
        if (chain === "mainnet" && !passphrase) {
          return Promise.reject({
            code: null,
            message: "Mainnet wallets must be encrypted -- enter a passphrase.",
          });
        }
        wallets.set(chain, mockWallet(chain));
        return undefined;
      }
      case "wallet_balance": {
        const { chain } = args as { chain: Chain };
        const wallet = wallets.get(chain);
        if (!wallet) {
          return Promise.reject({ code: null, message: `${chain} has no wallet yet` });
        }
        return wallet.balance;
      }
      case "wallet_receive_address": {
        const { chain } = args as { chain: Chain };
        const wallet = wallets.get(chain);
        if (!wallet) {
          return Promise.reject({ code: null, message: `${chain} has no wallet yet` });
        }
        return wallet.address;
      }
      case "wallet_inscriptions": {
        const { chain } = args as { chain: Chain };
        if (!wallets.has(chain)) {
          return Promise.reject({ code: null, message: `${chain} has no wallet yet` });
        }
        // Real ids so the grid layout/ids render; the iframes themselves
        // can't load real content in this browser-only preview -- there's
        // no real ord server behind it, same limitation as every other
        // IPC-only mock here.
        return MOCK_INSCRIPTIONS;
      }
      case "wallet_transaction_history": {
        const { chain } = args as { chain: Chain };
        if (!wallets.has(chain)) {
          return Promise.reject({ code: null, message: `${chain} has no wallet yet` });
        }
        return MOCK_TRANSACTIONS;
      }
      case "wallet_fee_estimate": {
        const { chain } = args as { chain: Chain };
        // Demonstrates both UI paths: a real estimate on mainnet, none
        // elsewhere (matching the real VERIFY finding that regtest
        // never has one -- DECISIONS.md Phase 5).
        return chain === "mainnet" ? 14.5 : null;
      }
      case "wallet_send_dry_run": {
        const { chain, address, feeRate } = args as {
          chain: Chain;
          address: string;
          feeRate: number;
        };
        const rejection = mockAddressValidation(chain, address);
        if (rejection) return Promise.reject(rejection);
        return { txid: "mock-dry-run-txid", fee: Math.round(feeRate * 200) };
      }
      case "wallet_send": {
        const { chain, address, feeRate, passphrase, remember } = args as {
          chain: Chain;
          address: string;
          feeRate: number;
          passphrase: string | null;
          remember: boolean;
        };
        const rejection = mockAddressValidation(chain, address);
        if (rejection) return Promise.reject(rejection);

        const effectivePassphrase = passphrase ?? rememberedPassphrases.get(chain);
        if (!effectivePassphrase) {
          return Promise.reject({
            code: "WALLET_LOCKED",
            message: "This wallet is locked; enter its passphrase to continue.",
          });
        }
        if (effectivePassphrase !== MOCK_WALLET_PASSPHRASE) {
          return Promise.reject({
            code: null,
            message: `rpc error -14: the wallet passphrase entered was incorrect (dev mock -- try "${MOCK_WALLET_PASSPHRASE}")`,
          });
        }
        if (remember) rememberedPassphrases.set(chain, effectivePassphrase);
        return { txid: "mock-send-txid", fee: Math.round(feeRate * 200) };
      }
      case "inscribe_file_preview": {
        const { path } = args as { path: string };
        return {
          size_bytes: 128,
          content_type: "image/png",
          // A real 1x1 transparent PNG, so the sandboxed preview
          // iframe in the dev browser preview has actual content to
          // render, same reasoning as MOCK_INSCRIPTIONS' real-shaped ids.
          data_url:
            "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=",
          path,
        };
      }
      case "wallet_inscribe_dry_run": {
        return { id: "mockinscriptionidmockinscriptionidmockinscriptionidmocki0", location: "mocktxidmocktxidmocktxidmocktxidmocktxidmocktxidmocktxidmocktx:0:0", fee: 500 };
      }
      case "wallet_inscribe": {
        const { chain, passphrase, remember } = args as {
          chain: Chain;
          passphrase: string | null;
          remember: boolean;
        };
        const effectivePassphrase = passphrase ?? rememberedPassphrases.get(chain);
        if (!effectivePassphrase) {
          return Promise.reject({
            code: "WALLET_LOCKED",
            message: "This wallet is locked; enter its passphrase to continue.",
          });
        }
        if (effectivePassphrase !== MOCK_WALLET_PASSPHRASE) {
          return Promise.reject({
            code: null,
            message: `rpc error -14: the wallet passphrase entered was incorrect (dev mock -- try "${MOCK_WALLET_PASSPHRASE}")`,
          });
        }
        if (remember) rememberedPassphrases.set(chain, effectivePassphrase);
        return {
          id: "mockinscriptionidmockinscriptionidmockinscriptionidmocki0",
          location: "mocktxidmocktxidmocktxidmocktxidmocktxidmocktxidmocktxidmocktx:0:0",
          fee: 500,
        };
      }
      case "inscription_detail": {
        const { id } = args as { id: string };
        // The second mock inscription demonstrates the Foundation F
        // "sats index is off" path (`sat: null`); the first has a real
        // sat number so the sat-history display can be exercised too.
        const sat = id === MOCK_INSCRIPTIONS[1].id ? null : 5_000_000_000;
        return {
          id,
          satpoint: `mocksatpointmocksatpointmocksatpointmocksatpointmocksatpointmo:0:0`,
          sat,
          number: id === MOCK_INSCRIPTIONS[0].id ? 0 : 1,
        };
      }
      case "sat_inscriptions": {
        return [MOCK_INSCRIPTIONS[0].id];
      }
      case "wallet_inscribe_batch_dry_run": {
        const { filePaths } = args as { filePaths: string[] };
        return filePaths.map((_, i) => ({
          id: `mockbatchinscriptionidmockbatchinscriptionidmockbatchi${i}`,
          location: `mockbatchtxidmockbatchtxidmockbatchtxidmockbatchtxidmockbatchtx:${i}:0`,
          fee: 500,
        }));
      }
      case "wallet_inscribe_batch": {
        const { chain, filePaths, passphrase, remember } = args as {
          chain: Chain;
          filePaths: string[];
          passphrase: string | null;
          remember: boolean;
        };
        const effectivePassphrase = passphrase ?? rememberedPassphrases.get(chain);
        if (!effectivePassphrase) {
          return Promise.reject({
            code: "WALLET_LOCKED",
            message: "This wallet is locked; enter its passphrase to continue.",
          });
        }
        if (effectivePassphrase !== MOCK_WALLET_PASSPHRASE) {
          return Promise.reject({
            code: null,
            message: `rpc error -14: the wallet passphrase entered was incorrect (dev mock -- try "${MOCK_WALLET_PASSPHRASE}")`,
          });
        }
        if (remember) rememberedPassphrases.set(chain, effectivePassphrase);
        return filePaths.map((_, i) => ({
          id: `mockbatchinscriptionidmockbatchinscriptionidmockbatchi${i}`,
          location: `mockbatchtxidmockbatchtxidmockbatchtxidmockbatchtxidmockbatchtx:${i}:0`,
          fee: 500,
        }));
      }
      default:
        throw new Error(`dev-tauri-mock: no mock for IPC command "${cmd}"`);
    }
  });
}
