const solana_web3 = require('@solana/web3.js');
const spl = require('@solana/spl-token');
const anchor = require('@project-serum/anchor');

const utils = require('./utils');

const lockerIdl = require('./locker.json');
const lockerIdlDevnet = require('./locker.devnet.json');

const programIdLocalnet = new solana_web3.PublicKey(lockerIdl.metadata.address);
const programIdDevnet = new solana_web3.PublicKey(lockerIdlDevnet.metadata.address);

const feeWallet = new anchor.web3.PublicKey("7vPbNKWdgS1dqx6ZnJR8dU9Mo6Tsgwp3S5rALuANwXiJ");

const LOCALNET = 'localnet';
const DEVNET = 'devnet';

function initProgram(cluster, provider) {
  switch (cluster) {
    case LOCALNET:
      return new anchor.Program(lockerIdl, programIdLocalnet, provider);

    case DEVNET:
    default:
      return new anchor.Program(lockerIdlDevnet, programIdDevnet, provider);
  }
}

async function createLocker(provider, args, cluster) {
  const program = initProgram(cluster, provider);

  const [locker, lockerBump] = await anchor.web3.PublicKey.findProgramAddress(
    [
      args.creator.toBuffer(),
      args.unlockDate.toBuffer('be', 8),
    ],
    program.programId
  );

  const [vaultAuthority, vaultBump] = await anchor.web3.PublicKey.findProgramAddress(
    [
      locker.toBuffer()
    ],
    program.programId,
  );

  const vaultAccount = await utils.getTokenAccount(provider, args.vault);
  const vaultMint = new spl.Token(
    provider.connection,
    vaultAccount.mint,
    utils.TOKEN_PROGRAM_ID,
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
        tokenProgram: utils.TOKEN_PROGRAM_ID,
      }
    }
  );
}

async function relock(provider, args, cluster) {
  const program = initProgram(cluster, provider);

  await program.rpc.relock(
    args.unlockDate,
    {
      accounts: {
        locker: args.locker,
        owner: args.owner,
      }
    }
  );
}

module.exports = {
  LOCALNET,
  DEVNET,
  createLocker,
  relock,
  feeWallet,
  utils,
};
