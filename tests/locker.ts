import * as anchor from '@project-serum/anchor';
import * as spl from "@solana/spl-token";
import { Locker } from '../target/types/locker';

import lockerClient from "../web3/locker/index";

import * as assert from 'assert';

describe('locker', () => {
  const provider = anchor.Provider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.Locker as anchor.Program<Locker>;
  const creator = provider.wallet.publicKey;
  const unlockDate = new anchor.BN(Date.now() + 20);
  let
    mint: spl.Token,
    fundingWallet: anchor.web3.PublicKey,
    vault: anchor.web3.PublicKey;

  it('Creates locker', async () => {
    mint = await lockerClient.utils.createMint(provider);
    fundingWallet = await lockerClient.utils.createTokenAccount(provider, mint.publicKey);
    vault = await lockerClient.utils.createTokenAccount(provider, mint.publicKey);

    await mint.mintTo(fundingWallet, provider.wallet.publicKey, [], 10000);

    await lockerClient.createLocker(provider, {
      unlockDate,
      countryCode: 54,
      linearEmission: null,
      amount: new anchor.BN(10000),
      creator,
      owner: creator,
      fundingWalletAuthority: creator,
      fundingWallet,
      vault,
    },
      lockerClient.LOCALNET
    );

    const lockers = await program.account.locker.all();

    const lockerAccount = lockers[0];
    console.log('Locker: ', lockerAccount);

    assert.ok(lockerAccount.account.owner.equals(creator));
    assert.ok(lockerAccount.account.creator.equals(creator));
    assert.deepStrictEqual(lockerAccount.account.linearEmission, null);
    assert.deepStrictEqual(lockerAccount.account.countryCode, 54);
    assert.ok(lockerAccount.account.currentUnlockDate.eq(unlockDate));
    assert.ok(lockerAccount.account.originalUnlockDate.eq(unlockDate));

    const fundingWalletAccount = await lockerClient.utils.getTokenAccount(provider, fundingWallet);
    assert.ok(fundingWalletAccount.amount.eqn(0));

    const feeWallet = await mint.getOrCreateAssociatedAccountInfo(lockerClient.feeWallet);
    const feeWalletAccount = await lockerClient.utils.getTokenAccount(provider, feeWallet.address);
    assert.ok(feeWalletAccount.amount.eqn(35));

    const vaultAccount = await lockerClient.utils.getTokenAccount(provider, vault);
    assert.ok(vaultAccount.amount.eqn(9965));
  });

  it('Relocks the locker', async () => {
    const lockers = await program.account.locker.all();
    const lockerAccountBefore = lockers[0];

    const newUnlockDate = unlockDate.addn(20);

    await lockerClient.relock(provider, {
      unlockDate: newUnlockDate,
      locker: lockerAccountBefore.publicKey,
      owner: lockerAccountBefore.account.owner,
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

    const newOwner = anchor.web3.Keypair.generate();

    await lockerClient.transferOwnership(provider, {
      locker: lockerAccountBefore.publicKey,
      owner: lockerAccountBefore.account.owner,
      newOwner: newOwner.publicKey,
    },
      lockerClient.LOCALNET
    );

    const lockerAccountAfter = await program.account.locker.fetch(lockerAccountBefore.publicKey);
    assert.ok(lockerAccountAfter.owner.equals(newOwner.publicKey));

    await lockerClient.transferOwnership(provider, {
      locker: lockerAccountBefore.publicKey,
      owner: newOwner.publicKey,
      newOwner: lockerAccountBefore.account.owner,
      signers: [newOwner],
    },
      lockerClient.LOCALNET
    );

    const lockerAccountFinal = await program.account.locker.fetch(lockerAccountBefore.publicKey);
    assert.ok(lockerAccountFinal.owner.equals(lockerAccountBefore.account.owner));
  });
});
