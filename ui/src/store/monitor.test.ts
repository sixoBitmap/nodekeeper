import { describe, expect, it } from "vitest";
import { filteredEntries, type MonitorEntry, type MonitorFilters } from "./monitor";

function entry(overrides: Partial<MonitorEntry>): MonitorEntry {
  return {
    id: "1",
    environment: "regtest",
    source: "bitcoincli",
    triggeringAction: "test",
    commandDisplay: "bitcoin-cli -regtest getblockchaininfo",
    startedAtMs: 0,
    status: "success",
    exitCode: 0,
    durationMs: 10,
    output: "",
    ...overrides,
  };
}

const NO_FILTERS: MonitorFilters = {
  environment: null,
  source: null,
  status: null,
  text: "",
  showBackgroundPolling: false,
};

describe("filteredEntries", () => {
  it("returns every entry when no filters are set", () => {
    const entries = [entry({ id: "1" }), entry({ id: "2" })];
    expect(filteredEntries(entries, NO_FILTERS)).toHaveLength(2);
  });

  it("filters by environment", () => {
    const entries = [
      entry({ id: "1", environment: "regtest" }),
      entry({ id: "2", environment: "mainnet" }),
    ];
    const result = filteredEntries(entries, { ...NO_FILTERS, environment: "mainnet" });
    expect(result.map((e) => e.id)).toEqual(["2"]);
  });

  it("filters by source", () => {
    const entries = [entry({ id: "1", source: "ordcli" }), entry({ id: "2", source: "rpc" })];
    const result = filteredEntries(entries, { ...NO_FILTERS, source: "rpc" });
    expect(result.map((e) => e.id)).toEqual(["2"]);
  });

  it("filters by status", () => {
    const entries = [entry({ id: "1", status: "success" }), entry({ id: "2", status: "error" })];
    const result = filteredEntries(entries, { ...NO_FILTERS, status: "error" });
    expect(result.map((e) => e.id)).toEqual(["2"]);
  });

  it("filters by text, case-insensitively, against the command display", () => {
    const entries = [
      entry({ id: "1", commandDisplay: "ord wallet balance" }),
      entry({ id: "2", commandDisplay: "bitcoin-cli getblockchaininfo" }),
    ];
    const result = filteredEntries(entries, { ...NO_FILTERS, text: "WALLET" });
    expect(result.map((e) => e.id)).toEqual(["1"]);
  });

  it("combines multiple active filters", () => {
    const entries = [
      entry({ id: "1", environment: "regtest", status: "success" }),
      entry({ id: "2", environment: "regtest", status: "error" }),
      entry({ id: "3", environment: "mainnet", status: "error" }),
    ];
    const result = filteredEntries(entries, { ...NO_FILTERS, environment: "regtest", status: "error" });
    expect(result.map((e) => e.id)).toEqual(["2"]);
  });
});
