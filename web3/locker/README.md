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
      feeInSol: true,
    });
```

`createLocker(provider, args)`

* `provider` -- anchor web3 provider
* `args`:

```js
{
    // unix timestamp (seconds!) of type anchor.BN
    unlockDate,
    // 2 letter country code ("RU", "UK" etc)
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
    // boolean: if true then fee is paid in SOL,
    // else paid in locked token
    // if token is already whitelisted it's better to set this to true
    // to avoid any fees
    feeInSol,
}
```

## Get Lockers

`getLockers(provider)` -- returns created lockers.

* `provider` -- anchor web3 provider

`getLockersOwnerBy(provider, owner)` -- returns lockers owned by specific account.

* `provider` -- anchor web3 provider
* `owner` -- account public key

## Relock

`relock(provider, unlockDate)`

* `provider` -- anchor web3 provider
* `unlockDate` -- new unlock date
    - should be later than original one
    - anchor.BN

## Transfer Ownership

`transferOwnership(provider, args)`

* `provider` -- anchor web3 provider
* `args`:

```js
{
    // Locker account as returned from `getLockers`
    locker,
    // Public key of a new owner
    newOwner,
}
```

## Increment Lock

`incrementLock(provider, args)`

* `provider` -- anchor web3 provider
* `args`:

```js
{
    // Locker account as returned from `getLockers`
    locker,
    // Amount to lock. anchor.BN
    amount,
    // public key of funding wallet owner
    // (provider.wallet.publicKey in the simplest case)
    fundingWalletAuthority,
    // address of source SPL token account
    fundingWallet,
}
```


## Withdraw Funds

`withdrawFunds(provider, args)`

* `provider` -- anchor web3 provider
* `args`:

```js
{
    // Amount to withdraw. anchor.BN
    amount,
    // Locker account as returned from `getLockers`
    locker,
    // if true, `targetWallet` should ordinary account public key like provider.wallet.publicKey
    // if not, `targetWallet` should an SPL token account
    createAssociated,
    // Public key of a wallet to transfer tokens to
    // if `createAssociated`, then associated SPL token account will be
    // created for this ordinary solana account
    // if not, it should SPL token account
    targetWallet,
}
```

Returns resulting targetWallet (associated or original).

## Split the Locker

`splitLocker(provider, args)`

* `provider` -- anchor web3 provider
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

## Close locker (for tests only!)

`closeLocker(provider, args)`

* `provider` -- anchor web3 provider
* `args`:

```js
{
    // Locker account as returned from `getLockers`
    locker,
    // Public key of a wallet to transfer tokens to
    // Should be an SPL token account!
    targetWallet,
}
```

## Check if token is already whitelisted

`isMintWhitelisted(provider, mint)`

* `provider` -- as always
* `mint` -- token public key

Returns simple boolean.

## Find vault authority (owner) address

`vaultAuthorityAddress(provider, locker)`

* `provider` -- as always
* `locker` -- as returned from `getLockers`

## Client object

```js
const lockerClient = require('unicrypt-locker');
const client = new lockerClient.Client(provider, lockerClient.TOKEN_LOCKER, lockerClient.DEVNET);
```

* `provider` -- as always
* `lockerClient.TOKEN_LOCKER` or `lockerClient.LP_LOCKER`
* `lockerClient.DEVNET` or `lockerClient.LOCALNET` -- the 3rd argument is irrelevant for now,
if omitted, `DEVNET` is the default

The following methods use the same arguments as functions described above except the ones omitted.

- `client.createLocker(args)`
- `client.getLockers()`
- `client.getLockersOwnerBy(owner)`
- `client.relock(unlockDate)`
- `client.transferOwnership(args)`
- `client.incrementLock(args)`
- `client.withdrawFunds(args)`
- `client.splitLocker(args)`
- `client.closeLocker(args)`
- `client.isMintWhitelisted(mint)`
- `client.vaultAuthorityAddress(locker)`
