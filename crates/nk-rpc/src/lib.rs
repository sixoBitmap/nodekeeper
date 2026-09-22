//! Bitcoin Core JSON-RPC client. All calls are routed through `nk-exec` so
//! they are tagged, redacted, and streamed to the Live Command Monitor like
//! any other command.
