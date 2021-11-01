import * as anchor from '@project-serum/anchor';
import { Program } from '@project-serum/anchor';
import { Locker } from '../target/types/locker';

describe('locker', () => {

  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.Provider.env());

  const program = anchor.workspace.Locker as Program<Locker>;

  it('Is initialized!', async () => {
    // Add your test here.
    const tx = await program.rpc.initialize({});
    console.log("Your transaction signature", tx);
  });
});
