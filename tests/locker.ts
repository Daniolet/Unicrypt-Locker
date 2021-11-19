import * as anchor from '@project-serum/anchor';
import * as spl from "@solana/spl-token";
import * as serumCmn from "@project-serum/common";
import * as assert from 'assert';

import { Locker } from '../target/types/locker';
import { CountryList } from '../target/types/country_list';
import lockerClient from "../web3/locker/index";

async function createMint(provider: anchor.Provider, authority?: anchor.web3.PublicKey) {
  if (authority === undefined) {
    authority = provider.wallet.publicKey;
  }
  const mint = await spl.Token.createMint(
    provider.connection,
    provider.wallet.payer,
    authority,
    null,
    6,
    lockerClient.utils.TOKEN_PROGRAM_ID,
  );
  return mint;
}

describe('locker', () => {
  const provider = anchor.Provider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.Locker as anchor.Program<Locker>;
  const countryListProgram = anchor.workspace.CountryList as anchor.Program<CountryList>;
  const creator = provider.wallet.publicKey;
  const unlockDate = new anchor.BN(Date.now() / 1000 + 4);
  const newOwner = anchor.web3.Keypair.generate();
  const countryList = anchor.web3.Keypair.generate();

  let
    mint: spl.Token,
    fundingWallet: anchor.web3.PublicKey;

  it('Creates locker', async () => {
    mint = await createMint(provider);
    fundingWallet = await serumCmn.createTokenAccount(
      provider,
      mint.publicKey,
      provider.wallet.publicKey,
    );

    await countryListProgram.rpc.initialize(
      [
        new TextEncoder().encode("RU")
      ],
      {
        accounts: {
          countryBanlist: countryList.publicKey,
          admin: provider.wallet.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        },
        signers: [countryList]
      }
    );

    await mint.mintTo(fundingWallet, provider.wallet.publicKey, [], 11000);

    await lockerClient.createLocker(provider,
      {
        unlockDate,
        countryCode: "RU",
        startEmission: null,
        amount: new anchor.BN(10000),
        creator,
        owner: creator,
        fundingWalletAuthority: creator,
        fundingWallet,
        countryBanlist: countryList.publicKey,
        feeInSol: true,
      },
      lockerClient.LOCALNET
    );

    const lockers = await program.account.locker.all();

    const lockerAccount = lockers[0];
    console.log('Locker: ', lockerAccount);

    assert.ok(lockerAccount.account.owner.equals(creator));
    assert.ok(lockerAccount.account.creator.equals(creator));
    assert.deepStrictEqual(lockerAccount.account.startEmission, null);
    assert.deepStrictEqual(lockerAccount.account.countryCode, [82, 85]);
    assert.ok(lockerAccount.account.currentUnlockDate.eq(unlockDate));
    assert.ok(lockerAccount.account.originalUnlockDate.eq(unlockDate));

    const fundingWalletAccount = await serumCmn.getTokenAccount(provider, fundingWallet);
    assert.ok(fundingWalletAccount.amount.eqn(1000));

    assert.ok(await lockerClient.isMintWhitelisted(provider, mint.publicKey, lockerClient.LOCALNET));

    const vaultAccount = await serumCmn.getTokenAccount(provider, lockerAccount.account.vault);
    assert.ok(vaultAccount.amount.eqn(10000));
  });

  it('Fails to withdraw funds if it is too early', async () => {
    const lockers = await program.account.locker.all();
    const lockerAccount = lockers[0];

    assert.rejects(
      async () => await lockerClient.withdrawFunds(provider,
        {
          amount: new anchor.BN(100),
          locker: lockerAccount,
          targetWallet: fundingWallet,
        },
        lockerClient.LOCALNET
      ),
      (err) => {
        assert.equal(err.code, 307);
        return true;
      }
    )
  });

  it('Relocks the locker', async () => {
    const lockers = await program.account.locker.all();
    const lockerAccountBefore = lockers[0];

    const newUnlockDate = unlockDate.addn(1);

    await lockerClient.relock(provider,
      {
        unlockDate: newUnlockDate,
        locker: lockerAccountBefore,
      },
      lockerClient.LOCALNET
    );

    const lockerAccountAfter = await program.account.locker.fetch(lockerAccountBefore.publicKey);
    assert.ok(!lockerAccountAfter.currentUnlockDate.eq(lockerAccountAfter.originalUnlockDate));
    assert.ok(lockerAccountAfter.currentUnlockDate.eq(newUnlockDate));
  });

  it('Transfers the ownership', async () => {
    const lockers = await program.account.locker.all();
    const lockerAccountBefore = lockers[0];

    await lockerClient.transferOwnership(provider,
      {
        locker: lockerAccountBefore,
        newOwner: newOwner.publicKey,
      },
      lockerClient.LOCALNET
    );

    const lockerAccountAfter = await program.account.locker.fetch(lockerAccountBefore.publicKey);
    assert.ok(lockerAccountAfter.owner.equals(newOwner.publicKey));

    await lockerClient.transferOwnership(provider,
      {
        locker: {
          publicKey: lockerAccountBefore.publicKey,
          account: lockerAccountAfter,
        },
        newOwner: lockerAccountBefore.account.owner,
        signers: [newOwner],
      },
      lockerClient.LOCALNET
    );

    const lockerAccountFinal = await program.account.locker.fetch(lockerAccountBefore.publicKey);
    assert.ok(lockerAccountFinal.owner.equals(lockerAccountBefore.account.owner));
  });

  it('Increments the lock', async () => {
    const lockers = await program.account.locker.all();
    const lockerAccountBefore = lockers[0];

    await lockerClient.incrementLock(provider, {
      amount: new anchor.BN(1000),
      locker: lockerAccountBefore,
      fundingWallet,
      fundingWalletAuthority: provider.wallet.publicKey,
    }, lockerClient.LOCALNET)

    const lockerAccountFinal = await program.account.locker.fetch(lockerAccountBefore.publicKey);
    assert.ok(lockerAccountFinal.depositedAmount.eqn(11000));
  });

  it('Splits the locker', async () => {
    let lockers = await program.account.locker.all();
    const locker = lockers[0];

    const amount = new anchor.BN(1000);

    await lockerClient.splitLocker(provider,
      {
        amount,
        locker,
        newOwner: newOwner.publicKey,
      },
      lockerClient.LOCALNET
    );

    lockers = await lockerClient.getLockersOwnedBy(provider, newOwner.publicKey, lockerClient.LOCALNET);
    const newLocker = lockers[0];

    assert.ok(newLocker.account.depositedAmount.eq(amount));

    const oldVaultAccount = await serumCmn.getTokenAccount(provider, locker.account.vault);
    assert.ok(oldVaultAccount.amount.eqn(10000));
  });

  it('Withdraws the funds', async () => {
    const lockers = await lockerClient.getLockersOwnedBy(provider, provider.wallet.publicKey, lockerClient.LOCALNET);
    const lockerAccount = lockers[0];

    const amount = new anchor.BN(1000);

    while (true) {
      try {
        await lockerClient.withdrawFunds(provider, {
          amount,
          locker: lockerAccount,
          targetWallet: provider.wallet.publicKey,
          createAssociated: true,
        },
          lockerClient.LOCALNET
        );
        break;
      } catch (err) {
        assert.equal(err.code, 308); // TooEarlyToWithdraw
        await lockerClient.utils.sleep(1000);
      }
    }

    const targetWalletAddress = await anchor.utils.token.associatedAddress({ mint: mint.publicKey, owner: provider.wallet.publicKey });
    const targetWallet = await serumCmn.getTokenAccount(provider, targetWalletAddress);
    assert.ok(targetWallet.amount.eq(amount));

    const vaultWallet = await serumCmn.getTokenAccount(provider, lockerAccount.account.vault);
    // 10000 - 1000 (gone in a split) - 1000 (withdraw amount)
    assert.ok(vaultWallet.amount.eqn(9000));

    await lockerClient.withdrawFunds(provider, {
      amount: new anchor.BN(9000),
      locker: lockerAccount,
      targetWallet: provider.wallet.publicKey,
      createAssociated: true,
    },
      lockerClient.LOCALNET
    );

    assert.rejects(
      async () => {
        await serumCmn.getTokenAccount(provider, lockerAccount.account.vault);
      },
      (err) => {
        assert.ok(err.message == "Failed to find token account");
      }
    );
  });

  it('Creates locker with linear emission', async () => {
    await mint.mintTo(fundingWallet, provider.wallet.publicKey, [], 1000);

    const now = new anchor.BN(Date.now()).divn(1000);
    const unlockDate = now.addn(20);

    const locker = await lockerClient.createLocker(provider,
      {
        unlockDate,
        countryCode: "RU",
        startEmission: now,
        amount: new anchor.BN(1000),
        creator,
        owner: creator,
        fundingWalletAuthority: creator,
        fundingWallet,
        countryBanlist: countryList.publicKey,
        feeInSol: true,
      },
      lockerClient.LOCALNET
    );

    await lockerClient.utils.sleep(5000);

    let lockerAccount = await program.account.locker.fetch(locker);

    await lockerClient.withdrawFunds(provider, {
      amount: new anchor.BN(900), // some number, it will not play any role at all
      locker: {
        publicKey: locker,
        account: lockerAccount,
      },
      targetWallet: fundingWallet,
      createAssociated: false,
    },
      lockerClient.LOCALNET
    );

    const fundingWalletAccount = await serumCmn.getTokenAccount(provider, fundingWallet);
    // should be 250 but it's hard to guarantee the exact value
    assert.ok(fundingWalletAccount.amount.gten(245) && fundingWalletAccount.amount.lten(255));

    lockerAccount = await program.account.locker.fetch(locker);
    const vaultWallet = await serumCmn.getTokenAccount(provider, lockerAccount.vault);
    // should be 750 but it's hard to guarantree the exact value
    assert.ok(vaultWallet.amount.gten(745) && fundingWalletAccount.amount.lten(755));
  })
});
