## Create Locker

```js
const lockerClient = require('unicrypt-locker');

const creator = provider.wallet.publicKey;

await lockerClient.createLocker(provider, {
      unlockDate: new anchor.BN(Date.now() + 20),
      countryCode: 54,
      linearEmission: null,
      amount: new anchor.BN(10000),
      creator,
      owner: creator,
      fundingWalletAuthority: creator,
      fundingWallet,
      vault,
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
    linearEmission,
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
    // address of target SPL token account
    vault,
}
```
