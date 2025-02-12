use core::num::NonZeroU16;

use solana_program::instruction::{AccountMeta, Instruction};
use solana_program::program_error::ProgramError;
use solana_program::pubkey::Pubkey;

type Result<T = (), E = ProgramError> = core::result::Result<T, E>;

/// Maximum chunk size sent to the write-account program.
///
/// The size utilises all of the space available in a single Solana transaction.
/// This is normally desired except if the Write instructions need to be
/// executed with other instructions (such as those setting priority fees).
///
/// [`WriteIter`] uses this as the default chunk size with additional adjustment
/// for the seed length.  To adjust the size use the [`WriteIter::chunk_size`]
/// method.
pub const MAX_CHUNK_SIZE: NonZeroU16 = match NonZeroU16::new(988) {
    Some(value) => value,
    None => unreachable!(),
};

/// Maximum possible data length.
///
/// This corresponds directly to the maximum Solana account size which is 10
/// MiB, see [`solana_program::system_instruction::MAX_PERMITTED_DATA_LENGTH`]
const MAX_DATA_SIZE: u32 =
    solana_program::system_instruction::MAX_PERMITTED_DATA_LENGTH as u32;

/// Iterator generating Solana instructions calling the write-account program
/// filling given account with given data.
pub struct WriteIter<'a> {
    write_program: &'a Pubkey,
    payer: Pubkey,
    write_account: Pubkey,
    seed: &'a [u8],
    bump: u8,
    data: Vec<u8>,
    position: usize,
    chunk_size: NonZeroU16,
}

impl<'a> WriteIter<'a> {
    /// Constructs a new iterator generating Write instructions writing
    /// length-prefixed data.
    ///
    /// `write_program` is the address of the write-account program used to fill
    /// account with the data.  `payer` is the account which signs and pays for
    /// the transaction and rent on the write account.  `seed` is seed used as
    /// part of the PDA of the write account.
    ///
    /// A length-prefixed `data` is write into the account.  The length-prefix
    /// uses 4-byte little-endian encoding for the length.  This is the same
    /// format Borsh uses for array serialisation.  The length-prefixed data is
    /// what [`crate::entrypoint`] macro expects.
    ///
    /// Returns an `ArithmeticOverflow` error if the resulting data exceeds
    /// maximum Solana account size (which is 10 MiB).  If the write account
    /// already exists and is larger than data’s length, the remaining bytes of
    /// the account will be untouched.  The length-prefix allows extracting the
    /// actual data length.
    ///
    /// Note that `seed` can be at most 31 bytes long which is one-less than
    /// normally allowed for seeds.
    ///
    /// On success, returns iterator which generates Write instructions calling
    /// `write_program` and the address and bump of the write account where the
    /// data will be written to.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let (mut chunks, chunk_account, _) = WriteIter::new(
    ///     &write_account_program_id,
    ///     authority.pubkey(),
    ///     b"",
    ///     instruction_data,
    /// ).unwrap();
    /// for instruction in chunks {
    ///     let transaction = Transaction::new_signed_with_payer(
    ///         &[instruction],
    ///         Some(&chunks.payer),
    ///         &[&authority],
    ///         blockhash,
    ///     );
    ///     sol_rpc_client
    ///         .send_and_confirm_transaction_with_spinner(&transaction)
    ///         .unwrap();
    /// }
    /// ```
    pub fn new(
        write_program: &'a Pubkey,
        payer: Pubkey,
        seed: &'a [u8],
        mut data: Vec<u8>,
    ) -> Result<(Self, Pubkey, u8)> {
        let len = u32::try_from(data.len())
            .ok()
            .filter(|len| *len <= MAX_DATA_SIZE - 4)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        data.splice(0..0, len.to_le_bytes());
        Self::new_impl(write_program, payer, seed, data)
    }

    /// Constructs a new iterator generating Write instructions writing raw
    /// data.
    ///
    /// Just like [`WriteIter::new`] creates an iterator which generates Write
    /// instructions calling the write-account program.  The difference is that
    /// it does not length-prefix the `data`.
    pub fn new_raw(
        write_program: &'a Pubkey,
        payer: Pubkey,
        seed: &'a [u8],
        data: Vec<u8>,
    ) -> Result<(Self, Pubkey, u8)> {
        u32::try_from(data.len())
            .ok()
            .filter(|len| *len <= MAX_DATA_SIZE)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        Self::new_impl(write_program, payer, seed, data)
    }

    fn new_impl(
        write_program: &'a Pubkey,
        payer: Pubkey,
        seed: &'a [u8],
        data: Vec<u8>,
    ) -> Result<(Self, Pubkey, u8)> {
        check_seed(seed)?;
        let (write_account, bump) = Pubkey::find_program_address(
            &[payer.as_ref(), seed],
            write_program,
        );
        let mut iter = Self {
            write_program,
            payer,
            write_account,
            seed,
            bump,
            data,
            position: 0,
            chunk_size: NonZeroU16::MAX,
        };
        iter.chunk_size(usize::MAX);
        Ok((iter, write_account, bump))
    }

    /// Sets maximum chunk size.
    ///
    /// By default the maximum chunk size is set to value which utilises full
    /// space available in Solana transaction.  This is normally desired since
    /// it reduces total number of transactions needed, but it doesn’t allow any
    /// other instructions (such as setting priority fees or tipping) to be
    /// executed together with the Write instructions.
    ///
    /// The `chunk_size` argument is clamped between 1 and [`MAX_CHUNK_SIZE`] -
    /// seed length.
    pub fn chunk_size(&mut self, chunk_size: usize) {
        let max = MAX_CHUNK_SIZE.get() - self.seed.len() as u16;
        let chunk_size = chunk_size.min(usize::from(max)) as u16;
        self.chunk_size = NonZeroU16::new(chunk_size)
            .unwrap_or(NonZeroU16::MIN);
    }

    /// Consumes the iterator and returns Write account address and bump.
    pub fn into_account(self) -> (Pubkey, u8) {
        (self.write_account, self.bump)
    }
}

impl core::iter::Iterator for WriteIter<'_> {
    type Item = solana_program::instruction::Instruction;

    fn next(&mut self) -> Option<Self::Item> {
        let len = self.data.len();
        let start = self.position;
        if start >= len {
            return None;
        }
        let end = start.saturating_add(self.chunk_size.get().into()).min(len);
        self.position = end;
        let chunk = &self.data[start..end];

        let data = [
            /* discriminant: */ b"\0",
            /* seed_len: */ &[self.seed.len() as u8][..],
            /* seed: */ self.seed,
            /* bump: */ &[self.bump],
            /* offset: */
            &u32::try_from(start).unwrap().to_le_bytes()[..],
            /* data: */ chunk,
        ]
        .concat();

        Some(solana_program::instruction::Instruction {
            program_id: *self.write_program,
            accounts: vec![
                AccountMeta::new(self.payer, true),
                AccountMeta::new(self.write_account, false),
                AccountMeta::new(solana_program::system_program::ID, false),
            ],
            data,
        })
    }
}

/// Generates instruction data for Free operation.
///
/// `seed` and `bump` specifies seed and bump of the Write PDA.  Note that the
/// actual seed used to create the PDA is `[payer.key, seed]` rather than just
/// `seed`.
///
/// If `write_account` is not given, it’s going to be generated from provided
/// Write program id, Payer account, seed and bump.
pub fn free(
    write_program_id: Pubkey,
    payer: Pubkey,
    write_account: Option<Pubkey>,
    seed: &[u8],
    bump: u8,
) -> Result<Instruction> {
    let mut buf = [0; { solana_program::pubkey::MAX_SEED_LEN + 2 }];
    buf[1] = check_seed(seed)?;
    buf[2..seed.len() + 2].copy_from_slice(seed);
    buf[seed.len() + 2] = bump;

    let write_account = match write_account {
        None => Pubkey::create_program_address(
            &[payer.as_ref(), seed, &[bump]],
            &write_program_id,
        )?,
        Some(acc) => acc,
    };

    Ok(Instruction {
        program_id: write_program_id,
        accounts: vec![
            AccountMeta::new(payer, true),
            AccountMeta::new(write_account, false),
            AccountMeta::new(solana_program::system_program::ID, false),
        ],
        data: buf[..seed.len() + 3].to_vec(),
    })
}

/// Checks that seed is below the maximum length; returns length cast to `u8`.
fn check_seed(seed: &[u8]) -> Result<u8> {
    if seed.len() < solana_program::pubkey::MAX_SEED_LEN {
        Ok(seed.len() as u8)
    } else {
        Err(ProgramError::MaxSeedLengthExceeded)
    }
}
