//! Regtest fixture for integration tests: starts real `bitcoind` and `ord`
//! in a temporary directory on random free ports, can mine blocks, and
//! always tears down (even on panic or test failure).
