// solana-write-account — helper program and library for handling Solana
//                        transaction size limit
// © 2024 by Composable Foundation
// © 2025 by Michał Nazarewicz <mina86@mina86.com>
//
// This program is free software; you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation; either version 2 of the License, or (at your option) any later
// version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along with
// this program; if not, see <https://www.gnu.org/licenses/>.

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
//! built with `client` feature) and helper [`mod@entrypoint`] module for Solana
//! programs which want to support reading instruction data from an account
//! (when built with `lib` feature).
//!
//! The account data must be a length-prefixed slice of bytes.  In other words,
//! borsh-serialised `Vec<u8>`.  The account may contain trailing bytes which
//! are ignored.

#[cfg(feature = "client")]
pub mod instruction;

#[cfg(feature = "lib")]
pub mod entrypoint;

#[cfg(not(any(feature = "client", feature = "lib")))]
mod program;
