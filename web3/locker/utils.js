const spl = require("@solana/spl-token");
const anchor = require('@project-serum/anchor');
const TokenInstructions = require("@project-serum/serum").TokenInstructions;
const serumCmn = require('@project-serum/common');

const TOKEN_PROGRAM_ID = new anchor.web3.PublicKey(
  TokenInstructions.TOKEN_PROGRAM_ID.toString()
);

async function createMint(provider, authority) {
  if (authority === undefined) {
    authority = provider.wallet.publicKey;
  }
  const mint = await spl.Token.createMint(
    provider.connection,
    provider.wallet.payer,
    authority,
    null,
    6,
    TOKEN_PROGRAM_ID
  );
  return mint;
}

async function createTokenAccount(provider, mint, owner) {
  if (owner === undefined) {
    owner = provider.wallet.publicKey;
  }
  const token = new spl.Token(
    provider.connection,
    mint,
    TOKEN_PROGRAM_ID,
    provider.wallet.payer
  );
  let vault = await token.createAccount(owner);
  return vault;
}

const FAILED_TO_FIND_ACCOUNT = 'Failed to find token account';
const INVALID_ACCOUNT_OWNER = 'Invalid account owner';

async function getOrCreateAssociatedTokenAccount(provider, mint, owner) {
  let associatedTokenAddress = await anchor.utils.token.associatedAddress({ mint, owner });

  try {
    return [associatedTokenAddress, await getTokenAccount(provider, associatedTokenAddress)];
  } catch (err) {
    // INVALID_ACCOUNT_OWNER can be possible if the associatedAddress has
    // already been received some lamports (= became system accounts).
    // Assuming program derived addressing is safe, this is the only case
    // for the INVALID_ACCOUNT_OWNER in this code-path
    if (
      err.message === FAILED_TO_FIND_ACCOUNT ||
      err.message === INVALID_ACCOUNT_OWNER
    ) {
      // as this isn't atomic, it's possible others can create associated
      // accounts meanwhile
      try {
        let createTokenAccountInstr = spl.Token.createAssociatedTokenAccountInstruction(
          spl.ASSOCIATED_TOKEN_PROGRAM_ID,
          TOKEN_PROGRAM_ID,
          mint,
          associatedTokenAddress,
          owner,
          provider.wallet.publicKey,
        );
        let createTokenAccountTx = new anchor.web3.Transaction().add(createTokenAccountInstr);
        await provider.send(createTokenAccountTx);
      } catch (err) {
        // ignore all errors; for now there is no API compatible way to
        // selectively ignore the expected instruction error if the
        // associated account is existing already.
      }

      // Now this should always succeed
      return [associatedTokenAddress, await getTokenAccount(provider, associatedTokenAddress)];
    } else {
      throw err;
    }
  }
}

async function getTokenAccount(provider, addr) {
  return await serumCmn.getTokenAccount(provider, addr);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

module.exports = {
  createMint,
  createTokenAccount,
  getTokenAccount,
  getOrCreateAssociatedTokenAccount,
  sleep,
  TOKEN_PROGRAM_ID,
};
