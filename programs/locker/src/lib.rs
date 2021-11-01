use anchor_lang::{prelude::*, solana_program::clock::UnixTimestamp};
use anchor_spl::token::TokenAccount;

use borsh::{BorshDeserialize, BorshSerialize};

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

#[program]
pub mod locker {
    use super::*;
    pub fn create_lock(ctx: Context<CreateLock>, args: CreateLockArgs) -> ProgramResult {
        Ok(())
    }
}

#[account]
pub struct Locker {
    owner: Pubkey,
    linear_emission: Option<LinearEmission>,
    creator: Pubkey,
}

impl Default for Locker {
    fn default() -> Self {
        Self {
            owner: Default::default(),
            linear_emission: Default::default(),
            creator: Default::default(),
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct LinearEmission {
    emission_start: UnixTimestamp,
    emission_end: UnixTimestamp,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct CreateLockArgs {
    unlock_date: UnixTimestamp,
    country_code: u16,
    linear_emission: Option<LinearEmission>,
    locker_bump: u8,
    vault_bump: u8,
}

#[derive(Accounts)]
#[instruction(args: CreateLockArgs)]
pub struct CreateLock<'info> {
    #[account(
        init,
        payer = creator,
        seeds = [
            creator.key().as_ref(),
        ],
        bump = args.locker_bump
    )]
    locker: ProgramAccount<'info, Locker>,
    #[account(signer)]
    creator: AccountInfo<'info>,
    owner: AccountInfo<'info>,
    #[account(signer)]
    funding_wallet_authority: AccountInfo<'info>,
    funding_wallet: Account<'info, TokenAccount>,
    #[account(
        seeds = [
            locker.key().as_ref()
        ],
        bump = args.vault_bump
    )]
    vault_authority: AccountInfo<'info>,
    #[account(
        constraint = vault.mint == funding_wallet.mint
    )]
    vault: Account<'info, TokenAccount>,
    fee_wallet: Account<'info, TokenAccount>,

    system_program: Program<'info, System>,
}
