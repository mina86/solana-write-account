# Solana `write-account` examples

Example code using the `solana-write-account` crate to implement
*chunking* in a smart contract.  To test in localnet, in a background
terminal execute `solana-test-validator` and then run:

```shell
$ cargo build-sbf
$ solana -u localhost program deploy \
      ../target/deploy/solana_write_account.so
# Make note of the program id

$ solana -u localhost program deploy \
      ./target/deploy/chsum.so
# Make note of the program id
```

Now, modify `chsum-client/src/main.rs` file by updating
`WRITE_ACCOUNT_PROGRAM_ID` and `PROGRAM_ID` addresses to the ones
noted above.  With that change done, you can test working of the
`chsum` program by executing the `chsum-client`:

```shell
$ data=abcdefghijklmnopqrstuvwxyz
$ cargo run -r -p chsum-client -- 2 "$data"
⋮
Program log: 4264
⋮

$ data=$data$data$data$data
$ data=$data$data$data$data
$ data=$data$data$data$data
$ cargo run -r -p chsum-client -- 2 "$data"
Writing chunks into the data account…
⋮
Program log: 272896
⋮
```

A more detailed description of the approach is available in [Solana
transaction size
limit](https://mina86.com/2025/solana-tx-size-limits/) article.
