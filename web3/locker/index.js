const solana_web3 = require('@solana/web3.js');
const spl = require('@solana/spl-token');
const anchor = require('@project-serum/anchor');

const utils = require('./utils');
const lockerIdl = require('./locker.json');
const { TOKEN_PROGRAM_ID } = require('@project-serum/serum/lib/token-instructions');

const programId = new solana_web3.PublicKey(lockerIdl.metadata.address);
const feeWallet = new anchor.web3.PublicKey("7vPbNKWdgS1dqx6ZnJR8dU9Mo6Tsgwp3S5rALuANwXiJ");

async function createLocker(provider, args) {
  const program = new anchor.Program(lockerIdl, programId, provider);

  const [locker, lockerBump] = await anchor.web3.PublicKey.findProgramAddress(
    [
      args.creator.toBuffer(),
      args.unlockDate.toBuffer('be', 8),
    ],
    programId
  );

  const [vaultAuthority, vaultBump] = await anchor.web3.PublicKey.findProgramAddress(
    [
      locker.toBuffer()
    ],
    programId,
  );

  const vaultAccount = await utils.getTokenAccount(provider, args.vault);
  const vaultMint = new spl.Token(
    provider.connection,
    vaultAccount.mint,
    TOKEN_PROGRAM_ID,
    provider.wallet.payer
  );

  const feeTokenWallet = await vaultMint.getOrCreateAssociatedAccountInfo(feeWallet);

  await program.rpc.createLocker(
    {
      unlockDate: args.unlockDate,
      lockerBump,
      vaultBump,
      countryCode: args.countryCode,
      linearEmission: args.linearEmission,
      amount: args.amount,
    },
    {
      accounts: {
        locker,
        creator: args.creator,
        owner: args.owner,
        vault: args.vault,
        vaultAuthority,
        fundingWalletAuthority: args.fundingWalletAuthority,
        fundingWallet: args.fundingWallet,
        feeWallet: feeTokenWallet.address,

        clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
      }
    }
  );
}

module.exports = {
  createLocker,
  feeWallet,
  utils,
};
