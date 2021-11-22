const solana_web3 = require('@solana/web3.js');
const anchor = require('@project-serum/anchor');
const serumCmn = require('@project-serum/common');

const utils = require('./utils');

const lockerIdl = require('./locker.json');
const lockerIdlDevnet = require('./locker.devnet.json');
const lpLockerIdlDevnet = require('./lp-locker.devnet.json');

const tokenLockerIdLocalnet = new solana_web3.PublicKey(lockerIdl.metadata.address);
const tokenLockerIdDevnet = new solana_web3.PublicKey(lockerIdlDevnet.metadata.address);
const lpLockerIdDevnet = new solana_web3.PublicKey(lpLockerIdlDevnet.metadata.address);

const LOCALNET = 'localnet';
const DEVNET = 'devnet';
const TOKEN_LOCKER = 'token-locker';
const LP_LOCKER = 'lp-locker';

class Client {
  constructor(provider, program, cluster) {
    this.cluster = cluster === undefined ? DEVNET : cluster;
    this.provider = provider;
    program = program === undefined ? TOKEN_LOCKER : LP_LOCKER;
    this.program = initProgram(cluster, provider, program);
  }
  async findMintInfoAddress(mint) {
    return await findMintInfoAddress(this.program, mint);
  }

  async findConfigAddress() {
    return await findConfigAddress(this.program);
  }

  async vaultAuthorityAddress(locker) {
    return await vaultAuthorityAddress(this.provider, locker, this.cluster);
  }

  async isMintWhitelisted(mint) {
    return await isMintWhitelisted(this.provider, mint, this.cluster);
  }

  async createLocker(args) {
    return await createLocker(this.provider, args, this.cluster);
  }

  async getLockers() {
    return await getLockers(this.provider, this.cluster);
  }

  async getLockersOwnedBy(owner) {
    return await getLockersOwnedBy(this.provider, owner, this.cluster);
  }

  async relock(args) {
    return await relock(this.provider, args, this.cluster);
  }

  async transferOwnership(args) {
    return await transferOwnership(this.provider, args, this.cluster);
  }

  async incrementLock(args) {
    return await incrementLock(this.provider, args, this.cluster);
  }

  async withdrawFunds(args) {
    return await withdrawFunds(this.provider, args, this.cluster);
  }

  async closeLocker(args) {
    return await closeLocker(this.provider, args, this.cluster);
  }

  async splitLocker(args) {
    return await splitLocker(this.provider, args, this.cluster);
  }
}

function initProgram(cluster, provider, program) {
  program = program === undefined ? TOKEN_LOCKER : LP_LOCKER;
  switch (cluster) {
    case LOCALNET:
      return new anchor.Program(lockerIdl, tokenLockerIdLocalnet, provider);

    case DEVNET:
    default:
      switch (program) {
        case LP_LOCKER:
          return new anchor.Program(lpLockerIdlDevnet, lpLockerIdDevnet, provider);

        case TOKEN_LOCKER:
        default:
          return new anchor.Program(lockerIdlDevnet, tokenLockerIdDevnet, provider);
      }
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

async function findConfigAddress(program) {
  const [config, bump] = await anchor.web3.PublicKey.findProgramAddress(
    [
      new TextEncoder().encode("config")
    ],
    program.programId
  );
  return [config, bump];
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

async function getOrCreateMintInfo(program, mint, payer, config) {
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
            config,
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

  const [config, _] = await findConfigAddress(program);
  const configAccount = await program.account.config.fetch(config);

  const [mintInfo, initMintInfoInstrs] = await getOrCreateMintInfo(
    program,
    fundingWalletAccount.mint,
    args.creator,
    config
  );
  const [feeTokenWallet, createAssociatedTokenAccountInstrs] = await utils.getOrCreateAssociatedTokenAccountInstrs(
    provider, fundingWalletAccount.mint, configAccount.feeWallet
  );

  const finalFeeWallet = args.feeInSol ? configAccount.feeWallet : feeTokenWallet;

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
        countryBanlist: configAccount.countryList,
        config,

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

  return await program.rpc.relock(
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

  return await program.rpc.transferOwnership(rpcArgs);
}

async function incrementLock(provider, args, cluster) {
  const program = initProgram(cluster, provider);

  const [config, _] = await findConfigAddress(program);
  const configAccount = await program.account.config.fetch(config);

  const fundingWalletAccount = await serumCmn.getTokenAccount(provider, args.fundingWallet);
  const [mintInfo, initMintInfoInstrs] = await getOrCreateMintInfo(
    program,
    fundingWalletAccount.mint,
    args.fundingWalletAuthority
  );
  const [feeTokenWallet, createAssociatedTokenAccountInstrs] = await utils.getOrCreateAssociatedTokenAccountInstrs(
    provider, fundingWalletAccount.mint, configAccount.feeWallet
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
        config
      },
      instructions: initMintInfoInstrs
        .concat(createAssociatedTokenAccountInstrs)
    }
  );

  return feeTokenWallet;
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

  return vaultAuthority;
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

  return newVault;
}

module.exports = {
  LOCALNET,
  DEVNET,
  LP_LOCKER,
  TOKEN_LOCKER,
  Client,
  findMintInfoAddress,
  findConfigAddress,
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
  utils,
};
