//! Helper smart contract and library functions to support calling Solana
//! programs with instruction data read from an account.
//!
//! Solana limits transaction size to at most 1232 bytes.  This includes all
//! accounts participating in the transaction as well as all the instruction
//! data.  Unfortunately, some programs may need to encode instructions which
//! don’t fit in that limit.
//!
//! To address this, Solana program may support reading instruction data from an
//! account.  This library provides a helper Solana program which allows
//! populating the account with overlong instruction data, client helper
//! functions for invoking program with instruction stored in an account (when
//! built with `client` feature) and helper [`entrypoint`] module for Solana
//! programs which want to support reading instruction data from an account
//! (when built with `lib` feature).
//!
//! The account data must be a length-prefixed slice of bytes.  In other words,
//! borsh-serialised `Vec<u8>`.  The account may contain trailing bytes which
//! are ignored.
//!
//! This module provides types to help use this feature of the Solana IBC
//! contract.  [`Accounts`] is used to add the account with instruction data to
//! an instruction and [`Instruction`] constructs an empty instruction data to
//! call the contract with.

#[cfg(feature = "client")]
pub mod instruction;

#[cfg(feature = "lib")]
pub mod entrypoint;

#[cfg(not(any(feature = "client", feature = "lib")))]
mod program;
