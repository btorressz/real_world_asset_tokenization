describe("RealWorldAssetTokenization Tests", () => {
  it("Initialize a new tokenized asset", async () => {
    //  Define test parameters
    const assetName = "Gold-Bar-001";
    const symbol = "GLD";
    const uri = "" //"https://example.com/metadata.json";
    const decimals = 6;
    const totalSupply = new BN(1_000_000);

    // Find PDAs for assetAccount and mint
    //    (matches seeds = ["asset-metadata", payerPubkey, assetName] / ["asset-mint", ...])
    const [assetAccountPda] = await web3.PublicKey.findProgramAddress(
      [
        Buffer.from("asset-metadata"),
        pg.wallet.publicKey.toBuffer(),
        Buffer.from(assetName),
      ],
      pg.program.programId
    );
    const [mintPda] = await web3.PublicKey.findProgramAddress(
      [
        Buffer.from("asset-mint"),
        pg.wallet.publicKey.toBuffer(),
        Buffer.from(assetName),
      ],
      pg.program.programId
    );

    //  Send the initializeAsset transaction
    const txHash = await pg.program.methods
      .initializeAsset(assetName, symbol, uri, decimals, totalSupply)
      .accounts({
        payer: pg.wallet.publicKey,
        assetAccount: assetAccountPda,
        mint: mintPda,
        // The "destination_token_account" is automatically derived by Anchor
        // because `#[account(init_if_needed, associated_token::mint = mint, associated_token::authority = payer)]` is used.
        // If not, it might need to be passed explicitly or rely on the IDL.

        // Some instructions also require these (depending on the IDL):
        systemProgram: web3.SystemProgram.programId,
        tokenProgram: new web3.PublicKey(
          "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        ),
        associatedTokenProgram: new web3.PublicKey(
          "ATokenGPvubht1EAB9RXkZioeh4Gz6Er9w3iB77na4uT"
        ),
        rent: web3.SYSVAR_RENT_PUBKEY,
      })
      .rpc();

    console.log("initializeAsset txHash:", txHash);

    //  Confirm transaction
    await pg.connection.confirmTransaction(txHash);

    //  Fetch the new asset account to verify metadata
    const assetAccountData = await pg.program.account.assetAccount.fetch(assetAccountPda);
    console.log("Fetched Asset Account:", assetAccountData);

    // Check that the on-chain data matches what was sent
    assert.strictEqual(assetAccountData.assetName, assetName, "Asset name mismatch");
    assert.strictEqual(assetAccountData.symbol, symbol, "Symbol mismatch");
    assert.strictEqual(assetAccountData.uri, uri, "URI mismatch");

    //  Optionally, check the minted token balance in the "destination token account"
    const associatedTokenAddress = await anchor.utils.token.associatedAddress({
      mint: mintPda,
      owner: pg.wallet.publicKey,
    });
    const tokenBalance = await pg.connection.getTokenAccountBalance(associatedTokenAddress);
    console.log("Token balance:", tokenBalance.value.amount);

    // totalSupply was 1,000,000 => expecting "1000000"
    assert.strictEqual(tokenBalance.value.amount, "1000000", "Incorrect token balance after mint");
  });
});
