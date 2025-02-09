use solana_program::account_info::AccountInfo;
use solana_program::program_error::ProgramError;
use solana_program::pubkey::Pubkey;

#[cfg(not(feature = "use-write-account"))]
solana_program::entrypoint!(process_instruction);

#[cfg(feature = "use-write-account")]
write_account::entrypoint!(process_instruction);

fn process_instruction<'a>(
    _program_id: &'a Pubkey,
    _accounts: &'a [AccountInfo],
    instruction: &'a [u8],
) -> Result<(), ProgramError> {
    let (mult, data) = instruction
        .split_first()
        .ok_or(ProgramError::InvalidInstructionData)?;
    let sum = data.chunks(2).map(|pair| {
        u64::from(pair[0]) * u64::from(*mult) +
            pair.get(1).copied().map_or(0, u64::from)
    }).fold(0, u64::wrapping_add);
    solana_program::msg!("{}", sum);
    Ok(())
}
