use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::get_associated_token_address,
    token::{self, Token, TokenAccount, Transfer},
};

use az::CheckedAs;

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

mod fee {
    use super::*;

    declare_id!("7vPbNKWdgS1dqx6ZnJR8dU9Mo6Tsgwp3S5rALuANwXiJ");

    pub const FEE: u64 = 35;
}

#[error]
pub enum ErrorCode {
    #[msg("The given unlock date is in the past")]
    UnlockInThePast,
    #[msg("The given fee wallet is not associated with required fee wallet")]
    InvalidFeeWallet,
    IntegerOverflow,
    NothingToLock,
    InvalidAmountTransferred,
    InvalidPeriod,
    CannotUnlockToEarlierDate,
    TooEarlyToWithdraw,
    NoFundsLeft,
}

#[program]
pub mod locker {
    use super::*;

    pub fn create_locker(ctx: Context<CreateLocker>, args: CreateLockerArgs) -> Result<()> {
        let locker = &mut ctx.accounts.locker;

        let now = ctx.accounts.clock.unix_timestamp;
        require!(args.unlock_date > now, UnlockInThePast);

        locker.original_unlock_date = args.unlock_date;
        locker.current_unlock_date = args.unlock_date;

        locker.country_code = args.country_code;

        if let Some(start_emission) = args.start_emission {
            require!(args.unlock_date > start_emission, InvalidPeriod);
        }
        locker.start_emission = args.start_emission;

        locker.owner = ctx.accounts.owner.key();
        locker.creator = ctx.accounts.creator.key();

        locker.bump = args.locker_bump;

        locker.vault = ctx.accounts.vault.key();
        locker.vault_bump = args.vault_bump;

        let associated_token_account =
            get_associated_token_address(&fee::ID, &ctx.accounts.funding_wallet.mint);

        require!(
            associated_token_account == ctx.accounts.fee_wallet.key(),
            InvalidFeeWallet
        );

        let lock_fee = mul_div(args.amount, fee::FEE, 10000).ok_or(ErrorCode::IntegerOverflow)?;

        let amount_before = ctx.accounts.funding_wallet.amount;

        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.funding_wallet.to_account_info(),
                to: ctx.accounts.fee_wallet.to_account_info(),
                authority: ctx.accounts.funding_wallet_authority.to_account_info(),
            },
        );
        token::transfer(cpi_ctx, lock_fee)?;

        ctx.accounts.funding_wallet.reload()?;
        let amount_after_fee = ctx.accounts.funding_wallet.amount;
        require!(
            amount_before - amount_after_fee == lock_fee,
            InvalidAmountTransferred
        );

        let amount_to_lock = args.amount - lock_fee;
        require!(amount_to_lock > 0, NothingToLock);

        locker.deposited_amount = amount_to_lock;
        locker.withdrawn_amount = 0;

        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.funding_wallet.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
                authority: ctx.accounts.funding_wallet_authority.to_account_info(),
            },
        );
        token::transfer(cpi_ctx, amount_to_lock)?;

        ctx.accounts.funding_wallet.reload()?;
        let amount_final = ctx.accounts.funding_wallet.amount;
        require!(
            amount_after_fee - amount_final == amount_to_lock,
            InvalidAmountTransferred
        );

        Ok(())
    }

    pub fn relock(ctx: Context<Relock>, unlock_date: i64) -> Result<()> {
        let locker = &mut ctx.accounts.locker;

        require!(
            unlock_date > locker.current_unlock_date,
            CannotUnlockToEarlierDate
        );

        locker.current_unlock_date = unlock_date;

        Ok(())
    }

    pub fn transfer_ownership(ctx: Context<TransferOwnership>) -> Result<()> {
        let locker = &mut ctx.accounts.locker;

        locker.owner = ctx.accounts.new_owner.key();

        Ok(())
    }

    pub fn withdraw_funds(ctx: Context<WithdrawFunds>, amount: u64) -> Result<()> {
        let now = ctx.accounts.clock.unix_timestamp;
        let locker = &mut ctx.accounts.locker;

        let balance = locker.deposited_amount - locker.withdrawn_amount;

        require!(balance > 0, NoFundsLeft);

        let amount_to_transfer = match locker.start_emission {
            Some(start_emission) => {
                let clamped_time = now.clamp(start_emission, locker.current_unlock_date);
                let elapsed = clamped_time - start_emission;
                let full_period = locker.current_unlock_date - start_emission;
                require!(full_period > 0, InvalidPeriod);

                mul_div(locker.deposited_amount, elapsed, full_period as u64)
                    .ok_or(ErrorCode::IntegerOverflow)?
                    .min(amount)
            }
            None => {
                require!(now > locker.current_unlock_date, TooEarlyToWithdraw);
                amount.min(balance)
            }
        };

        require!(amount_to_transfer > 0, InvalidAmountTransferred);
        require!(amount_to_transfer < balance, InvalidAmountTransferred);

        let amount_before = ctx.accounts.vault.amount;

        let locker_key = locker.key();
        let seeds = &[locker_key.as_ref(), &[locker.vault_bump]];
        let signer = &[&seeds[..]];

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.target_wallet.to_account_info(),
                authority: ctx.accounts.vault_authority.to_account_info(),
            },
            signer,
        );
        token::transfer(cpi_ctx, amount_to_transfer)?;

        ctx.accounts.vault.reload()?;
        let amount_after = ctx.accounts.vault.amount;
        require!(
            amount_before - amount_after == amount_to_transfer,
            InvalidAmountTransferred
        );

        locker.withdrawn_amount += amount_to_transfer;

        Ok(())
    }
}

#[account]
pub struct Locker {
    owner: Pubkey,
    country_code: u16,
    current_unlock_date: i64,
    start_emission: Option<i64>,
    deposited_amount: u64,
    withdrawn_amount: u64,
    vault: Pubkey,
    vault_bump: u8,
    // `creator` and `original_unlock_date` help to generate PDA
    creator: Pubkey,
    original_unlock_date: i64,
    bump: u8,
}

impl Default for Locker {
    fn default() -> Self {
        Self {
            owner: Default::default(),
            creator: Default::default(),
            current_unlock_date: Default::default(),
            start_emission: Default::default(),
            deposited_amount: Default::default(),
            withdrawn_amount: Default::default(),
            original_unlock_date: Default::default(),
            vault: Default::default(),
            vault_bump: Default::default(),
            country_code: Default::default(),
            bump: Default::default(),
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct CreateLockerArgs {
    amount: u64,
    unlock_date: i64,
    country_code: u16,
    start_emission: Option<i64>,
    locker_bump: u8,
    vault_bump: u8,
}

#[derive(Accounts)]
#[instruction(args: CreateLockerArgs)]
pub struct CreateLocker<'info> {
    #[account(
        init,
        payer = creator,
        seeds = [
            creator.key().as_ref(),
            args.unlock_date.to_be_bytes().as_ref(),
        ],
        bump = args.locker_bump
    )]
    locker: ProgramAccount<'info, Locker>,
    #[account(signer)]
    creator: AccountInfo<'info>,
    owner: AccountInfo<'info>,
    #[account(signer)]
    funding_wallet_authority: AccountInfo<'info>,
    #[account(mut)]
    funding_wallet: Account<'info, TokenAccount>,
    #[account(
        seeds = [
            locker.key().as_ref()
        ],
        bump = args.vault_bump
    )]
    vault_authority: AccountInfo<'info>,
    #[account(
        mut,
        constraint = vault.mint == funding_wallet.mint
    )]
    vault: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = fee_wallet.mint == funding_wallet.mint
    )]
    fee_wallet: Account<'info, TokenAccount>,

    clock: Sysvar<'info, Clock>,
    system_program: Program<'info, System>,
    token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Relock<'info> {
    #[account(
        mut,
        seeds = [
            locker.creator.key().as_ref(),
            locker.original_unlock_date.to_be_bytes().as_ref(),
        ],
        bump = locker.bump
    )]
    locker: ProgramAccount<'info, Locker>,
    #[account(
        signer,
        constraint = locker.owner == owner.key()
    )]
    owner: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct TransferOwnership<'info> {
    #[account(
        mut,
        seeds = [
            locker.creator.key().as_ref(),
            locker.original_unlock_date.to_be_bytes().as_ref(),
        ],
        bump = locker.bump
    )]
    locker: ProgramAccount<'info, Locker>,
    #[account(
        signer,
        constraint = locker.owner == owner.key()
    )]
    owner: AccountInfo<'info>,
    new_owner: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct WithdrawFunds<'info> {
    #[account(
        mut,
        seeds = [
            locker.creator.key().as_ref(),
            locker.original_unlock_date.to_be_bytes().as_ref(),
        ],
        bump = locker.bump
    )]
    locker: ProgramAccount<'info, Locker>,
    #[account(
        signer,
        constraint = locker.owner == owner.key()
    )]
    owner: AccountInfo<'info>,
    vault_authority: AccountInfo<'info>,
    #[account(
        mut,
        constraint = vault.owner == vault_authority.key()
    )]
    vault: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = target_wallet.mint == vault.mint
    )]
    target_wallet: Account<'info, TokenAccount>,

    clock: Sysvar<'info, Clock>,
    token_program: Program<'info, Token>,
}

/// floor(a * b / denominator)
fn mul_div<SrcA, SrcB, SrcD>(a: SrcA, b: SrcB, denominator: SrcD) -> Option<u64>
where
    SrcA: fixed::traits::ToFixed,
    SrcB: fixed::traits::ToFixed,
    SrcD: fixed::traits::ToFixed,
{
    use fixed::types::U64F64;

    let a = U64F64::from_num(a);
    let b = U64F64::from_num(b);
    let denominator = U64F64::from_num(denominator);

    a.checked_mul(b)
        .and_then(|r| r.checked_div(denominator))
        .and_then(|r| r.floor().checked_as::<u64>())
}
