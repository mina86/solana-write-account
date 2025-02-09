use core::str::FromStr;
use std::process::ExitCode;

use solana_client::rpc_client::RpcClient;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::message::Message;
use solana_sdk::program_error::ProgramError;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signer;
use solana_sdk::signer::keypair::Keypair;
use solana_sdk::transaction::Transaction;
use solana_transaction_status::option_serializer::OptionSerializer;
use solana_transaction_status::UiTransactionEncoding;


/// Hard-coded address of the chsum program.
const PROGRAM_ID: Pubkey = solana_sdk::pubkey!(
    "CjYnjL2CTRPfW2W1yfyUvAhRRkFr6xMTcUa3CHTUDZY8"
);

/// Hard-coded address of the write-account program.
#[cfg(feature = "use-write-account")]
const WRITE_ACCOUNT_PROGRAM_ID: Pubkey = solana_sdk::pubkey!(
    "C4kB14J8w4hnoCDhcgPupFJcnsaVVWEbDrxwW3vPFFmV"
);

/// Seed to use for the instruction data PDA.  Can be at most
/// 31-byte long.
const SEED: &[u8] = b"";


type Result<T = (), E = Error> = core::result::Result<T, E>;


/// `usage: chsum-cli <mult> <data>`
fn main() -> ExitCode {
    if let Err(err) = run() {
        eprintln!("{err}");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}


/// Executes the program.
fn run() -> Result {
    let data = parse_args()?;
    let keypair = read_keypair()?;
    let client = RpcClient::new("http://127.0.0.1:8899");

    #[cfg(feature = "use-write-account")]
    if data.len() > 1062 {
        return call_chsum_chunked(&client, &keypair, data);
    }
    call_chsum_simple(&client, &keypair, data)
}


/// Parses the command line arguments and returns Solana
/// program’s instruction data.
fn parse_args() -> Result<Vec<u8>> {
    let mut args = std::env::args();
    let mult = args.nth(1).ok_or(Error::Usage)?;
    let mult = u8::from_str(mult.as_str())
        .map_err(|_| Error::Usage)?;
    let data = args.next().ok_or(Error::Usage)?;
    Ok([&[mult][..], data.as_bytes()].concat())
}


/// Reads keypair from a hard-coded location.
fn read_keypair() -> Result<Keypair> {
    let home = std::env::var_os("HOME").unwrap();
    let mut path = std::path::PathBuf::from(home);
    path.push(".config/solana/id.json");
    solana_sdk::signer::keypair::read_keypair_file(path)
        .map_err(Error::from)
}


/// Sends a single transaction to the chsum program.
fn call_chsum_simple(
    client: &RpcClient,
    keypair: &Keypair,
    data: Vec<u8>,
) -> Result {
    call_chsum(client, keypair, Vec::new(), data)
}


/// Sends a transaction to the chsum program using chunking to
/// pass the instruction data.
#[cfg(feature = "use-write-account")]
fn call_chsum_chunked(
    client: &RpcClient,
    keypair: &Keypair,
    data: Vec<u8>,
) -> Result {
    // Send chunks
    eprintln!("Writing chunks into the data account…");
    let (chunks, account, bump) =
        write_account::instruction::WriteIter::new(
            &WRITE_ACCOUNT_PROGRAM_ID,
            keypair.pubkey(),
            SEED,
            data,
        )?;
    for instruction in chunks {
        send_and_confirm_instruction(
            client,
            keypair,
            instruction,
        )?;
        eprintln!();
    }

    // Call chsum
    eprintln!("Calling chsum program…");
    let accounts = vec![AccountMeta::new(account, false)];
    call_chsum(client, keypair, accounts, Vec::new())?;

    // Free the account
    eprintln!();
    eprintln!("Freeing instruction data account…");
    let instruction = write_account::instruction::free(
        WRITE_ACCOUNT_PROGRAM_ID,
        keypair.pubkey(),
        Some(account),
        SEED,
        bump,
    )?;
    send_and_confirm_instruction(client, keypair, instruction)
}


/// Do send transaction to the chsum program.
fn call_chsum(
    client: &RpcClient,
    keypair: &Keypair,
    accounts: Vec<AccountMeta>,
    data: Vec<u8>,
) -> Result {
    send_and_confirm_instruction(client, keypair, Instruction {
        program_id: PROGRAM_ID,
        accounts,
        data,
    })
}


/// Sends a transaction and logs result.
fn send_and_confirm_instruction(
    client: &RpcClient,
    keypair: &Keypair,
    instruction: Instruction,
) -> Result {
    let blockhash = client.get_latest_blockhash()?;
    eprintln!("Latest blockhash: {blockhash}");

    eprintln!(
        "Sending transaction to {}…",
        instruction.program_id
    );

    let message = Message::new_with_blockhash(
        core::slice::from_ref(&instruction),
        Some(&keypair.pubkey()),
        &blockhash,
    );
    let mut tx = Transaction::new_unsigned(message);
    tx.sign(&[&keypair], blockhash);

    let sig = client.send_and_confirm_transaction(&tx)?;
    eprintln!("Signature: {sig}");

    let encoding = UiTransactionEncoding::Binary;
    let resp = client.get_transaction(&sig, encoding)?;
    let (slot, tx) = (resp.slot, resp.transaction);
    eprintln!("Executed in slot: {slot}");

    // Print log messages
    let log_messages = tx
        .meta
        .map(|meta| meta.log_messages)
        .ok_or(Error::Msg("No transaction metadata"))?;
    if let OptionSerializer::Some(messages) = log_messages {
        for msg in messages {
            println!("{msg}");
        }
        Ok(())
    } else {
        Err(Error::Msg("No log message"))
    }
}


#[derive(derive_more::From, derive_more::Display)]
enum Error {
    #[display("usage: chsum-cli <mult> [<data>]")]
    #[from(ignore)]
    Usage,
    Msg(&'static str),
    Client(solana_client::client_error::ClientError),
    Prog(ProgramError),
    Box(Box<dyn std::error::Error>),
}
