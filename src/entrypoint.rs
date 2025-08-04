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
/// Analogous to [`solana_program::entrypoint`] macro with additional handling
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
            // SAFETY: Caller promises this is safe.
            let parsed = unsafe { $crate::entrypoint::deserialize(input) };
            let (program_id, accounts, data) = match parsed {
                Ok(it) => it,
                Err(error) => return error.into(),
            };
            match $process_instruction(program_id, &accounts, data) {
                Ok(()) => $crate::entrypoint::__private::SUCCESS,
                Err(error) => error.into(),
            }
        }
        $crate::entrypoint::__private::solana_program::custom_heap_default!();
        $crate::entrypoint::__private::solana_program::custom_panic_default!();
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
            use core::mem::MaybeUninit;

            use $crate::entrypoint::__private;

            let mut accounts =
                [const { MaybeUninit::<__private::AccountInfo>::uninit() }; 64];
            // SAFETY: Caller promises this is safe.
            let parsed = unsafe {
                $crate::entrypoint::deserialize_into(input, &mut accounts)
            };
            let (program_id, num_accounts, instruction_data) = match parsed {
                Ok(it) => it,
                Err(error) => return error.into(),
            };
            let accounts = &accounts[..num_accounts]
                as *const [MaybeUninit<__private::AccountInfo>]
                as *const [AccountInfo];
            // SAFETY: deserialize_into initialised the first num_accounts
            // entries of the array and `MU<X>` has the same layout as `X`.
            let accounts = unsafe { &*accounts };

            // Make sure we have a new stack frame.  Solana stack frame sizes
            // are limited so the accounts array would eat into user’s available
            // stack space.
            #[inline(never)]
            fn inner(
                program_id: &__private::Pubkey,
                accounts: &[AccountInfo],
                data: &[u8],
            ) -> u64 {
                match $process_instruction(program_id, &accounts, data) {
                    Ok(()) => __private::SUCCESS,
                    Err(error) => error.into(),
                }
            }

            inner(program_id, accounts, instruction_data)
        }
        $crate::entrypoint::__private::solana_program::custom_heap_default!();
        $crate::entrypoint::__private::solana_program::custom_panic_default!();
    };
}


#[doc(hidden)]
pub mod __private {
    pub use solana_program;
    pub use solana_program::account_info::AccountInfo;
    pub use solana_program::entrypoint::SUCCESS;
    pub use solana_program::pubkey::Pubkey;
}

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
        assert_eq!(want, get_ix_data(acc));
    };

    check(Err(ProgramError::InvalidInstructionData), &[][..]);
    check(Ok(&[][..]), &[0, 0, 0, 0, 1, 2, 3, 4][..]);
    check(Ok(&[1][..]), &[1, 0, 0, 0, 1, 2, 3, 4][..]);
    check(Err(ProgramError::InvalidInstructionData), &[1, 0, 0, 0][..]);
}
