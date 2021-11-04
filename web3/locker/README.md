## Create Locker

```js
const lockerClient = require('unicrypt-locker');

const creator = provider.wallet.publicKey;

await lockerClient.createLocker(provider, {
      unlockDate: new anchor.BN(Date.now() + 20),
      countryCode: 54,
      startEmission: null,
      amount: new anchor.BN(10000),
      creator,
      owner: creator,
      fundingWalletAuthority: creator,
      fundingWallet,
    });
```

`createLocker(provider, args)`

* `provider` -- solana web3 provider
* `args`:

```js
{
    // unix timestamp of type anchor.BN
    unlockDate,
    // some number
    countryCode,
    // null for now
    startEmission,
    // amount to lock of type anchor.BN
    amount,
    // provider.wallet.publicKey
    creator,
    // public key of locker owner
    // (provider.wallet.publicKey in the simplest case)
    owner,
    // public key of funding wallet owner
    // (provider.wallet.publicKey in the simplest case)
    fundingWalletAuthority,
    // address of source SPL token account
    fundingWallet,
}
```

## Get Lockers

`getLockers(provider)` -- returns created lockers.

* `provider` -- solana web3 provider

`getLockersOwnerBy(provider, owner)` -- returns lockers owned by specific account.

* `provider` -- solana web3 provider
* `owner` -- account public key

## Relock

`relock(provider, unlockDate)`

* `provider` -- solana web3 provider
* `unlockDate` -- new unlock date
    - should be later than original one
    - anchor.BN

## Transfer Ownership

`transferOwnership(provider, args)`

* `provider` -- solana web3 provider
* `args`:

```js
{
    // Locker account as returned from `getLockers`
    locker,
    // Public key of a new owner
    newOwner,
}
```

## Withdraw Funds

`withdrawFunds(provider, args)`

* `provider` -- solana web3 provider
* `args`:

```js
{
    // Amount to withdraw. anchor.BN
    amount,
    // Locker account as returned from `getLockers`
    locker,
    // Public key of a wallet to transfer tokens to
    targetWallet,
}
```

## Split the Locker

`splitLocker(provider, args)`

* `provider` -- solana web3 provider
* `args`:

```js
{
    // Amount to deposit in a new locker. anchor.BN
    amount,
    // Locker account as returned from `getLockers`
    locker,
    // Public key of a new owner
    newOwner,
}
```
