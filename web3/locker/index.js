const solana_web3 = require('@solana/web3.js');
const anchor = require('@project-serum/anchor');
const serumCmn = require('@project-serum/common');

const utils = require('./utils');

const lockerIdl = require('./locker.json');
const lockerIdlDevnet = require('./locker.devnet.json');

const programIdLocalnet = new solana_web3.PublicKey(lockerIdl.metadata.address);
const programIdDevnet = new solana_web3.PublicKey(lockerIdlDevnet.metadata.address);

const countryListDevnet = new solana_web3.PublicKey("GkHZ3qzHwRZ4TrGQT57SgBNv4MsygBPaZzPmFu2757Vx");

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

function getCountryList(cluster) {
  switch (cluster) {
    case LOCALNET:
      return undefined;

    case DEVNET:
    default:
      return countryListDevnet;
  }
}

async function findMintInfoAddress(program, mint) {
  const [mintInfo, bump] = await anchor.web3.PublicKey.findProgramAddress(
    [
      mint.toBytes()
    ],
    program.programId
  );
  return [mintInfo, bump];
}

const FAILED_TO_FIND_ACCOUNT = "Account does not exist";

async function tryIfExists(program, account, address, found, notFound) {
  try {
    const accountInfo = await program.account[account].fetch(address);
    return found(accountInfo);
  } catch (err) {
    const errMessage = `${FAILED_TO_FIND_ACCOUNT} ${address.toString()}`;
    if (err.message === errMessage) {
      return notFound();
    } else {
      throw err;
    }
  }
}

async function isMintWhitelisted(provider, mint, cluster) {
  const program = initProgram(cluster, provider);
  const [mintInfo, _bump] = await findMintInfoAddress(program, mint);

  return await tryIfExists(
    program, "mintInfo", mintInfo,
    (mintInfoAccount) => mintInfoAccount.feePaid,
    () => false,
  );
}

async function getOrCreateMintInfo(program, mint, payer) {
  const [mintInfo, bump] = await findMintInfoAddress(program, mint);

  return await tryIfExists(
    program, "mintInfo", mintInfo,
    (_mintInfoAccount) => [mintInfo, []],
    () => {
      let initMintInfoInstr = program.instruction.initMintInfo(
        bump,
        {
          accounts: {
            payer,
            mintInfo,
            mint,
            systemProgram: anchor.web3.SystemProgram.programId,
          }
        }
      );
      return [mintInfo, [initMintInfoInstr]];
    }
  );
}

async function vaultAuthorityAddress(provider, locker, cluster) {
  const program = initProgram(cluster, provider);
  return await anchor.web3.PublicKey.createProgramAddress(
    [
      locker.publicKey.toBytes(),
      [locker.account.vaultBump]
    ],
    program.programId
  );
}

async function createLocker(provider, args, cluster) {
  const program = initProgram(cluster, provider);

  const [locker, lockerBump] = await anchor.web3.PublicKey.findProgramAddress(
    [
      args.creator.toBytes(),
      args.unlockDate.toArray('be', 8),
      args.amount.toArray('be', 8)
    ],
    program.programId
  );

  const [vaultAuthority, vaultBump] = await anchor.web3.PublicKey.findProgramAddress(
    [
      locker.toBytes()
    ],
    program.programId,
  );

  const fundingWalletAccount = await serumCmn.getTokenAccount(provider, args.fundingWallet);
  const vault = new anchor.web3.Account();
  const createTokenAccountInstrs = await serumCmn.createTokenAccountInstrs(
    provider,
    vault.publicKey,
    fundingWalletAccount.mint,
    vaultAuthority
  );

  const [mintInfo, initMintInfoInstrs] = await getOrCreateMintInfo(
    program,
    fundingWalletAccount.mint,
    args.creator
  );
  const [feeTokenWallet, createAssociatedTokenAccountInstrs] = await utils.getOrCreateAssociatedTokenAccountInstrs(
    provider, fundingWalletAccount.mint, feeWallet
  );

  const finalFeeWallet = args.feeInSol ? feeWallet : feeTokenWallet;
  const countryBanlist = args.countryBanlist === undefined ? getCountryList(cluster) : args.countryBanlist;

  await program.rpc.createLocker(
    {
      unlockDate: args.unlockDate,
      lockerBump,
      vaultBump,
      countryCode: args.countryCode,
      startEmission: args.startEmission,
      amount: args.amount,
      feeInSol: args.feeInSol,
    },
    {
      accounts: {
        locker,
        creator: args.creator,
        owner: args.owner,
        vault: vault.publicKey,
        vaultAuthority,
        fundingWalletAuthority: args.fundingWalletAuthority,
        fundingWallet: args.fundingWallet,
        feeWallet: finalFeeWallet,
        mintInfo,
        countryBanlist,

        clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: utils.TOKEN_PROGRAM_ID,
      },
      instructions: createTokenAccountInstrs
        .concat(initMintInfoInstrs)
        .concat(createAssociatedTokenAccountInstrs),
      signers: [vault],
    }
  );

  return locker;
}

async function getLockers(provider, cluster) {
  const program = initProgram(cluster, provider);
  return await program.account.locker.all();
}

async function getLockersOwnedBy(provider, owner, cluster) {
  const program = initProgram(cluster, provider);
  if (owner === undefined) {
    owner = provider.wallet.publicKey;
  }
  return await program.account.locker.all([
    {
      memcmp: {
        // 8 bytes for discriminator
        offset: 8,
        bytes: owner.toBase58(),
      },
    },
  ]);
}

async function relock(provider, args, cluster) {
  const program = initProgram(cluster, provider);

  await program.rpc.relock(
    args.unlockDate,
    {
      accounts: {
        locker: args.locker.publicKey,
        owner: args.locker.account.owner,
      }
    }
  );
}

async function transferOwnership(provider, args, cluster) {
  const program = initProgram(cluster, provider);

  const rpcArgs = {
    accounts: {
      locker: args.locker.publicKey,
      owner: args.locker.account.owner,
      newOwner: args.newOwner,
    }
  };

  if (args.signers !== undefined) {
    rpcArgs.signers = args.signers;
  }

  await program.rpc.transferOwnership(rpcArgs);
}

async function incrementLock(provider, args, cluster) {
  const program = initProgram(cluster, provider);

  const fundingWalletAccount = await serumCmn.getTokenAccount(provider, args.fundingWallet);
  const [mintInfo, initMintInfoInstrs] = await getOrCreateMintInfo(
    program,
    fundingWalletAccount.mint,
    args.fundingWalletAuthority
  );
  const [feeTokenWallet, createAssociatedTokenAccountInstrs] = await utils.getOrCreateAssociatedTokenAccountInstrs(
    provider, fundingWalletAccount.mint, feeWallet
  );

  await program.rpc.incrementLock(
    args.amount,
    {
      accounts: {
        locker: args.locker.publicKey,
        vault: args.locker.account.vault,
        fundingWallet: args.fundingWallet,
        fundingWalletAuthority: args.fundingWalletAuthority,
        feeWallet: feeTokenWallet,
        tokenProgram: utils.TOKEN_PROGRAM_ID,
        mintInfo,
      },
      instructions: initMintInfoInstrs
        .concat(createAssociatedTokenAccountInstrs)
    }
  );
}

async function withdrawFunds(provider, args, cluster) {
  const program = initProgram(cluster, provider);

  const vaultAuthority = await anchor.web3.PublicKey.createProgramAddress(
    [
      args.locker.publicKey.toBytes(),
      [args.locker.account.vaultBump]
    ],
    program.programId,
  );

  let targetWallet = args.targetWallet;
  let extraInstructions = [];

  if (args.createAssociated) {
    const vaultWalletAccount = await serumCmn.getTokenAccount(provider, args.locker.account.vault);
    const [targetTokenWallet, createAssociatedTokenAccountInstrs] = await utils.getOrCreateAssociatedTokenAccountInstrs(
      provider, vaultWalletAccount.mint, targetWallet
    );
    targetWallet = targetTokenWallet;
    extraInstructions.concat(createAssociatedTokenAccountInstrs);
  }

  await program.rpc.withdrawFunds(
    args.amount,
    {
      accounts: {
        locker: args.locker.publicKey,
        owner: args.locker.account.owner,
        vaultAuthority,
        vault: args.locker.account.vault,
        targetWallet,

        clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
        tokenProgram: utils.TOKEN_PROGRAM_ID,
      },
      instructions: extraInstructions
    }
  );

  return targetWallet;
}

async function closeLocker(provider, args, cluster) {
  const program = initProgram(cluster, provider);

  const vaultAuthority = await anchor.web3.PublicKey.createProgramAddress(
    [
      args.locker.publicKey.toBytes(),
      [args.locker.account.vaultBump]
    ],
    program.programId,
  );

  await program.rpc.withdrawFunds(
    {
      accounts: {
        locker: args.locker.publicKey,
        owner: args.locker.account.owner,
        vaultAuthority,
        vault: args.locker.account.vault,
        targetWallet: args.targetWallet,

        tokenProgram: utils.TOKEN_PROGRAM_ID,
      }
    }
  );
}

async function splitLocker(provider, args, cluster) {
  const program = initProgram(cluster, provider);

  const oldVaultAuthority = await anchor.web3.PublicKey.createProgramAddress(
    [
      args.locker.publicKey.toBytes(),
      [args.locker.account.vaultBump]
    ],
    program.programId,
  );

  const [newLocker, newLockerBump] = await anchor.web3.PublicKey.findProgramAddress(
    [
      args.locker.publicKey.toBytes(),
      args.locker.account.currentUnlockDate.toArray('be', 8),
      args.amount.toArray('be', 8),
    ],
    program.programId
  );

  const [newVaultAuthority, newVaultBump] = await anchor.web3.PublicKey.findProgramAddress(
    [
      newLocker.toBytes(),
    ],
    program.programId,
  );

  const vaultAccount = await serumCmn.getTokenAccount(provider, args.locker.account.vault);
  const newVault = new anchor.web3.Account();
  const createTokenAccountInstrs = await serumCmn.createTokenAccountInstrs(
    provider,
    newVault.publicKey,
    vaultAccount.mint,
    newVaultAuthority
  );

  await program.rpc.splitLocker(
    {
      amount: args.amount,
      lockerBump: newLockerBump,
      vaultBump: newVaultBump,
    },
    {
      accounts: {
        oldLocker: args.locker.publicKey,
        oldOwner: args.locker.account.owner,
        oldVaultAuthority,
        oldVault: args.locker.account.vault,

        newLocker,
        newOwner: args.newOwner,
        newVaultAuthority,
        newVault: newVault.publicKey,

        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: utils.TOKEN_PROGRAM_ID,
      },
      instructions: createTokenAccountInstrs,
      signers: [newVault],
    }
  );
}

module.exports = {
  LOCALNET,
  DEVNET,
  findMintInfoAddress,
  vaultAuthorityAddress,
  isMintWhitelisted,
  createLocker,
  getLockers,
  getLockersOwnedBy,
  relock,
  transferOwnership,
  incrementLock,
  withdrawFunds,
  closeLocker,
  splitLocker,
  feeWallet,
  utils,
};
