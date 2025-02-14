# Solana `write-account`

Solana has a transaction size limit of 1232 bytes.  This may be
insufficient to more complex smart contracts which need to ingest
comparatively large amounts of input data.

Possible way to address this is by introducing a helper contract which
can read *chunked* instruction data and concatenate it inside
of an account such that the target smart contract can read the
overlarge payload from said account.

This repository introduces a `solana-write-account` crate which
defines
* a smart contract which allows writing data into accounts,
* RPC client library functions facilitating invocation of that smart
  contract (requires `client` Cargo feature), and
* smart contract library functions which enable target smart contract
  to read its instruction data from an account rather than
  transaction’s payload (requires `lib` Cargo feature).

A more detailed description of the approach is available in [Solana
transaction size
limit](https://mina86.com/2025/solana-tx-size-limits/) article.
Furthermore, the `examples` directory contains an example smart
contract and RPC client which take advantage of the *chunking*
approach.
