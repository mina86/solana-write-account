//! Functions for the smart contract to allow parsing the serialised program
//! arguments and read instruction data from an account.

use solana_program::account_info::AccountInfo;
use solana_program::program_error::ProgramError;
use solana_program::pubkey::Pubkey;


/// Deserialize the input arguments.
///
/// Behaves like [`solana_program::entrypoint::deserialize`] except for special
/// handling of empty instruction data.
///
/// If the instruction data is empty, the instruction data is read from the last
/// account passed to the instruction.  The data of the account is interpreted
/// as length-prefixed sequence of bytes with length being an unsigned 32-bit
/// integer using little endian encoding.  The account used to read the account
/// data is not returned with the rest of the accounts.
///
/// # Safety
///
/// Must be called with pointer to properly serialised instruction such as done
/// by the Solana runtime.  See [`solana_program::entrypoint::deserialize`].
pub unsafe fn deserialize<'a>(
    input: *mut u8,
) -> Result<(&'a Pubkey, Vec<AccountInfo<'a>>, &'a [u8]), ProgramError> {
    // SAFETY: Caller promises this is safe.
    let (program_id, mut accounts, mut instruction_data) =
        unsafe { solana_program::entrypoint::deserialize(input) };

    // If instruction data is empty, the actual instruction data comes from the
    // last account passed in the call.
    if instruction_data.is_empty() {
        let ix_acc =
            accounts.pop().ok_or(ProgramError::NotEnoughAccountKeys)?;
        instruction_data = get_ix_data(ix_acc)?;
    }

    Ok((program_id, accounts, instruction_data))
}

/// Deserialize the input arguments.
///
/// Behaves like [`solana_program::entrypoint::deserialize_into`] except for
/// special handling of empty instruction data.  Differs from [`deserialize`] by
/// writing the account infos into an uninitialised slice rather than allocating
/// a new vector.
///
/// Panics if the input slice is not large enough.
///
/// # Safety
///
/// Must be called with pointer to properly serialised instruction such as done
/// by the Solana runtime.  See [`solana_program::entrypoint::deserialize`].
pub unsafe fn deserialize_into<'a>(
    input: *mut u8,
    accounts: &mut [core::mem::MaybeUninit<AccountInfo<'a>>],
) -> Result<(&'a Pubkey, usize, &'a [u8]), ProgramError> {
    // SAFETY: Caller promises this is safe.
    let (program_id, mut count, mut instruction_data) = unsafe {
        solana_program::entrypoint::deserialize_into(input, accounts)
    };

    // If instruction data is empty, the actual instruction data comes from the
    // last account passed in the call.
    if instruction_data.is_empty() {
        count =
            count.checked_sub(1).ok_or(ProgramError::NotEnoughAccountKeys)?;
        // SAFETY: `deserialize_into` initialised the element.
        let ix_acc = unsafe { accounts[count].assume_init_read() };
        instruction_data = get_ix_data(ix_acc)?;
    }

    Ok((program_id, count, instruction_data))
}


/// Interprets data in the last account as instruction data.
fn get_ix_data<'a>(account: AccountInfo<'a>) -> Result<&'a [u8], ProgramError> {
    let data = std::rc::Rc::try_unwrap(account.data);
    let data = data.ok().unwrap().into_inner();
    if data.len() < 4 {
        return Err(ProgramError::InvalidInstructionData);
    }
    let (len, data) = data.split_at(4);
    let len = u32::from_le_bytes(len.try_into().unwrap());
    let len =
        usize::try_from(len).map_err(|_| ProgramError::ArithmeticOverflow)?;
    data.get(..len).ok_or(ProgramError::InvalidInstructionData)
}


/// Declare the program entrypoint and set up global handlers.
///
/// Analogous to [`solana_program::entrypoint!`] macro with additional handling
/// of empty instruction data as described in [`deserialize`].
#[macro_export]
macro_rules! entrypoint {
    ($process_instruction:ident) => {
        /// Solana program entry point.
        ///
        /// # Safety
        ///
        /// Must be called with pointer to properly serialised instruction such
        /// as done by the Solana runtime.
        #[no_mangle]
        pub unsafe extern "C" fn entrypoint(input: *mut u8) -> u64 {
            // SAFETY: Caller guarantees it’s safe.
            unsafe {
                $crate::entrypoint::__private::entrypoint_impl(
                    input,
                    |pid, accs, data| $process_instruction(pid, &accs, data),
                )
            }
        }
        $crate::entrypoint::__private::custom_heap_default!();
        $crate::entrypoint::__private::custom_panic_default!();
    };
}


/// Declare the program entrypoint and set up global handlers.
///
/// Analogous to [`solana_program::entrypoint_no_alloc`] macro with additional
/// handling of empty instruction data as described in [`deserialize_into`].
#[macro_export]
macro_rules! entrypoint_no_alloc {
    ($process_instruction:ident) => {
        /// Solana program entry point.
        ///
        /// # Safety
        ///
        /// Must be called with pointer to properly serialised instruction such
        /// as done by the Solana runtime.
        #[no_mangle]
        pub unsafe extern "C" fn entrypoint(input: *mut u8) -> u64 {
            // SAFETY: Caller guarantees it’s safe.
            unsafe {
                $crate::entrypoint::__private::entrypoint_no_alloc_impl(
                    input,
                    |pid, accs, data| $process_instruction(pid, accs, data),
                )
            }
        }
        $crate::entrypoint::__private::custom_heap_default!();
        $crate::entrypoint::__private::custom_panic_default!();
    };
}


#[doc(hidden)]
pub mod __private {
    use core::mem::MaybeUninit;

    use solana_program::account_info::AccountInfo;
    use solana_program::entrypoint::SUCCESS;
    use solana_program::pubkey::Pubkey;
    pub use solana_program::{custom_heap_default, custom_panic_default};

    type Result =
        core::result::Result<(), solana_program::program_error::ProgramError>;

    #[inline(always)]
    pub unsafe fn entrypoint_impl(
        input: *mut u8,
        process: impl FnOnce(&Pubkey, Vec<AccountInfo>, &[u8]) -> Result,
    ) -> u64 {
        // SAFETY: Caller promises this is safe.
        unsafe { super::deserialize(input) }
            .and_then(|(pid, accs, data)| process(pid, accs, data))
            .map_or_else(|error| error.into(), |()| SUCCESS)
    }

    #[inline(always)]
    pub unsafe fn entrypoint_no_alloc_impl(
        input: *mut u8,
        process: impl FnOnce(&Pubkey, &[AccountInfo], &[u8]) -> Result,
    ) -> u64 {
        let mut accounts = [const { MaybeUninit::<AccountInfo>::uninit() }; 64];
        // SAFETY: Caller promises this is safe.
        let parsed = unsafe { super::deserialize_into(input, &mut accounts) };
        let (program_id, num_accounts, instruction_data) = match parsed {
            Ok(it) => it,
            Err(error) => return error.into(),
        };
        let accounts = &accounts[..num_accounts]
            as *const [MaybeUninit<AccountInfo>]
            as *const [AccountInfo];
        // SAFETY: deserialize_into initialised the first num_accounts entries
        // of the array and `MU<X>` has the same layout as `X`.
        let accounts = unsafe { &*accounts };

        // Make sure we have a new stack frame.  Solana stack frame sizes are
        // limited so the accounts array would eat into user’s available stack
        // space.
        #[inline(never)]
        fn inner(
            program_id: &Pubkey,
            accounts: &[AccountInfo],
            data: &[u8],
            process: impl FnOnce(&Pubkey, &[AccountInfo], &[u8]) -> Result,
        ) -> Result {
            process(program_id, accounts, data)
        }

        inner(program_id, accounts, instruction_data, process)
            .map_or_else(|error| error.into(), |()| SUCCESS)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use solana_program::entrypoint::{
        BPF_ALIGN_OF_U128, MAX_PERMITTED_DATA_INCREASE, NON_DUP_MARKER,
    };

    use super::*;

    #[test]
    fn test_get_ix_data() {
        let key = Pubkey::new_unique();

        fn account_info<'a>(
            key: &'a Pubkey,
            lamports: &'a mut u64,
            data: &'a mut [u8],
        ) -> AccountInfo<'a> {
            AccountInfo::new(key, false, false, lamports, data, key, false, 0)
        }

        let check = |want, data: &[u8]| {
            let mut lamports = 0u64;
            let mut data = data.to_vec();
            let acc = account_info(&key, &mut lamports, &mut data);
            assert_eq!(want, super::get_ix_data(acc));
        };

        check(Err(ProgramError::InvalidInstructionData), &[][..]);
        check(Ok(&[][..]), &[0, 0, 0, 0, 1, 2, 3, 4][..]);
        check(Ok(&[1][..]), &[1, 0, 0, 0, 1, 2, 3, 4][..]);
        check(Err(ProgramError::InvalidInstructionData), &[1, 0, 0, 0][..]);
    }

    #[derive(Debug)]
    struct TestAccount {
        key: Pubkey,
        owner: Pubkey,
        is_signer: bool,
        is_writable: bool,
        executable: bool,
        rent_epoch: u64,
        lamports: u64,
        data: Vec<u8>,
    }

    impl TestAccount {
        fn new(data: impl Into<Vec<u8>>) -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static CNT: AtomicU64 = AtomicU64::new(1);

            let bits = CNT.fetch_add(1, Ordering::SeqCst);
            Self {
                key: Pubkey::new_unique(),
                lamports: CNT.fetch_add(1, Ordering::SeqCst),
                data: data.into(),
                owner: Pubkey::new_unique(),
                rent_epoch: CNT.fetch_add(1, Ordering::SeqCst),
                is_signer: bits & 1 != 0,
                is_writable: bits & 2 != 0,
                executable: bits & 4 != 0,
            }
        }
    }

    impl PartialEq<AccountInfo<'_>> for TestAccount {
        fn eq(&self, rhs: &AccountInfo<'_>) -> bool {
            &self.key == rhs.key &&
                self.lamports == rhs.lamports() &&
                self.data.as_slice() == &rhs.try_borrow_data().unwrap()[..] &&
                &self.owner == rhs.owner &&
                self.rent_epoch == rhs.rent_epoch &&
                self.is_signer == rhs.is_signer &&
                self.is_writable == rhs.is_writable &&
                self.executable == rhs.executable
        }
    }

    fn serialise_input(
        accounts: &[TestAccount],
        instruction_data: &[u8],
    ) -> (Pubkey, Vec<u8>, usize) {
        let program_id = Pubkey::new_unique();
        let mut vec = Vec::<u8>::new();
        vec.extend_from_slice(&(accounts.len() as u64).to_le_bytes());

        fn align(addr: usize) -> usize {
            match addr % BPF_ALIGN_OF_U128 {
                0 => 0,
                n => BPF_ALIGN_OF_U128 - n,
            }
        }

        for account in accounts {
            vec.extend_from_slice(&[
                NON_DUP_MARKER,
                account.is_signer as u8,
                account.is_writable as u8,
                account.executable as u8,
                0,
                0,
                0,
                0,
            ]);
            vec.extend_from_slice(&account.key.as_ref());
            vec.extend_from_slice(&account.owner.as_ref());
            vec.extend_from_slice(&account.lamports.to_le_bytes());
            vec.extend_from_slice(&(account.data.len() as u64).to_le_bytes());
            vec.extend_from_slice(account.data.as_slice());
            let align_offset = align(account.data.len());
            let padding = MAX_PERMITTED_DATA_INCREASE + align_offset;
            vec.resize(vec.len() + padding, 0);
            vec.extend_from_slice(&account.rent_epoch.to_le_bytes());
        }

        vec.extend_from_slice(&(instruction_data.len() as u64).to_le_bytes());
        vec.extend_from_slice(instruction_data);
        vec.extend_from_slice(program_id.as_ref());
        vec.reserve(BPF_ALIGN_OF_U128 - 1);

        // Make sure the data is serialised.  We do it by inserting appropriate
        // number of bytes at the start of the vector.
        let pad = match vec.as_ptr().addr() % BPF_ALIGN_OF_U128 {
            0 => 0,
            n => {
                vec.splice(0..0, core::iter::repeat_n(0, 8 - n));
                8 - n
            }
        };
        assert_eq!(
            0,
            (vec.as_ptr().wrapping_add(pad)).addr() % BPF_ALIGN_OF_U128
        );

        (program_id, vec, pad)
    }

    /// Tests whether `serialise_input` is implemented correctly.  If this test
    /// fails, other tests are likely to fail as well.
    #[test]
    fn test_serialise() {
        let accounts = [
            TestAccount::new(b"raz"),
            TestAccount::new(b"dwa"),
            TestAccount::new(b"trzy"),
            TestAccount::new(b"cztery"),
        ];
        let (program, mut data, offset) =
            serialise_input(&accounts[..], b"data");

        // SAFETY: Data is correctly aligned and serialised.  (We assume).
        let (got_program, got_accounts, got_data) = unsafe {
            solana_program::entrypoint::deserialize(
                (&mut data[offset..]).as_mut_ptr(),
            )
        };

        assert_eq!(&program, got_program);
        assert_eq!(&b"data"[..], got_data);
        assert_eq!(accounts.len(), got_accounts.len());
        for (acc, got) in accounts.iter().zip(got_accounts.iter()) {
            assert_eq!(acc, got);
        }
    }

    fn do_test_entrypoint(
        accounts: &[TestAccount],
        instruction_data: &[u8],
        want: Result<(usize, &[u8]), u64>,
    ) {
        let (program_id, mut data, offset) =
            serialise_input(accounts, instruction_data);

        let check =
            |got_id: &Pubkey, got_accounts: &[AccountInfo], got_data: &[u8]| {
                assert_eq!(&program_id, got_id);
                assert_eq!(want, Ok((got_accounts.len(), got_data)));
                for (acc, got) in accounts.iter().zip(got_accounts.iter()) {
                    assert_eq!(acc, got);
                }
            };

        let input = data.as_mut_ptr().wrapping_add(offset);
        let want_result = want.clone().err().unwrap_or(0);
        assert_eq!(want_result, unsafe {
            __private::entrypoint_impl(input, |id, accounts, data| {
                Ok(check(id, accounts.as_slice(), data))
            })
        });
        assert_eq!(want_result, unsafe {
            __private::entrypoint_no_alloc_impl(input, |id, accounts, data| {
                Ok(check(id, accounts, data))
            })
        });
    }

    #[test]
    fn test_entrypoint_normal() {
        let accounts = [TestAccount::new(b"raz")];
        do_test_entrypoint(&accounts, b"data", Ok((1, b"data")));
    }

    #[test]
    fn test_entrypoint_staged() {
        let data = b"\x04\x00\x00\x00data";
        let accounts = [TestAccount::new(b"raz"), TestAccount::new(data)];
        do_test_entrypoint(&accounts[1..], b"", Ok((0, b"data")));
        do_test_entrypoint(&accounts, b"", Ok((1, b"data")));
    }

    #[test]
    fn test_entrypoint_long_staged() {
        let data = b"\x04\x00\x00\x00datagarbage";
        let accounts = [TestAccount::new(b"raz"), TestAccount::new(data)];
        do_test_entrypoint(&accounts[1..], b"", Ok((0, b"data")));
        do_test_entrypoint(&accounts, b"", Ok((1, b"data")));
    }

    #[test]
    fn test_entrypoint_short_staged() {
        let data = b"\x04\x00\x00\x00dat";
        do_test_entrypoint(&[TestAccount::new(data)], b"", Err(12884901888));
    }
}
