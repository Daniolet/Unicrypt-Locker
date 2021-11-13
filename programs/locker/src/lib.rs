use anchor_lang::{prelude::*, solana_program, AccountsClose};
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use az::CheckedAs;

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

mod fee {
    use super::*;

    declare_id!("7vPbNKWdgS1dqx6ZnJR8dU9Mo6Tsgwp3S5rALuANwXiJ");

    pub const FEE: u64 = 1 * solana_program::native_token::LAMPORTS_PER_SOL;
}

#[error]
pub enum ErrorCode {
    #[msg("The given unlock date is in the past")]
    UnlockInThePast,
    InvalidTimestamp,
    #[msg("The given fee wallet is not associated with required fee wallet")]
    InvalidFeeWallet,
    IntegerOverflow,
    NothingToLock,
    InvalidAmountTransferred,
    InvalidPeriod,
    CannotUnlockToEarlierDate,
    TooEarlyToWithdraw,
    InvalidAmount,
}

#[program]
pub mod locker {
    use super::*;

    pub fn create_locker(ctx: Context<CreateLocker>, args: CreateLockerArgs) -> Result<()> {
        let locker = &mut ctx.accounts.locker;

        let now = ctx.accounts.clock.unix_timestamp;
        require!(args.unlock_date > now, UnlockInThePast);
        // prevents errors when timestamp entered as milliseconds
        require!(args.unlock_date < 10000000000, InvalidTimestamp);

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

        require!(ctx.accounts.fee_wallet.key() == fee::ID, InvalidFeeWallet);

        solana_program::program::invoke(
            &solana_program::system_instruction::transfer(
                ctx.accounts.owner.to_account_info().key,
                ctx.accounts.fee_wallet.key,
                fee::FEE,
            ),
            &[
                ctx.accounts.owner.to_account_info(),
                ctx.accounts.fee_wallet.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;

        require!(args.amount > 0, NothingToLock);

        let amount_before = ctx.accounts.funding_wallet.amount;
        locker.deposited_amount = args.amount;

        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.funding_wallet.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
                authority: ctx.accounts.funding_wallet_authority.to_account_info(),
            },
        );
        token::transfer(cpi_ctx, args.amount)?;

        ctx.accounts.funding_wallet.reload()?;
        let amount_final = ctx.accounts.funding_wallet.amount;
        require!(
            amount_before - amount_final == args.amount,
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
        let locker = &ctx.accounts.locker;
        let vault = &mut ctx.accounts.vault;

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
                amount.min(vault.amount)
            }
        };

        require!(amount_to_transfer > 0, InvalidAmount);
        require!(amount_to_transfer <= vault.amount, InvalidAmount);

        let amount_before = vault.amount;

        let locker_key = locker.key();
        let seeds = &[locker_key.as_ref(), &[locker.vault_bump]];
        let signer = &[&seeds[..]];

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: vault.to_account_info(),
                to: ctx.accounts.target_wallet.to_account_info(),
                authority: ctx.accounts.vault_authority.to_account_info(),
            },
            signer,
        );
        token::transfer(cpi_ctx, amount_to_transfer)?;

        vault.reload()?;
        let amount_after = vault.amount;
        require!(
            amount_before - amount_after == amount_to_transfer,
            InvalidAmountTransferred
        );

        if vault.amount == 0 {
            vault.close(ctx.accounts.owner.to_account_info())?;
            locker.close(ctx.accounts.owner.to_account_info())?;
        }

        Ok(())
    }

    pub fn split_locker(ctx: Context<SplitLocker>, args: SplitLockerArgs) -> Result<()> {
        require!(args.amount > 0, InvalidAmount);

        let new_locker = &mut ctx.accounts.new_locker;
        let old_locker = &mut ctx.accounts.old_locker;
        let old_vault = &mut ctx.accounts.old_vault;

        require!(args.amount <= old_vault.amount, InvalidAmount);

        let amount_before = old_vault.amount;

        let locker_key = old_locker.key();
        let seeds = &[locker_key.as_ref(), &[old_locker.vault_bump]];
        let signer = &[&seeds[..]];

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: old_vault.to_account_info(),
                to: ctx.accounts.new_vault.to_account_info(),
                authority: ctx.accounts.old_vault_authority.to_account_info(),
            },
            signer,
        );
        token::transfer(cpi_ctx, args.amount)?;

        old_vault.reload()?;
        let amount_after = old_vault.amount;
        require!(
            amount_before - amount_after == args.amount,
            InvalidAmountTransferred
        );

        if old_vault.amount == 0 {
            old_vault.close(ctx.accounts.old_owner.to_account_info())?;
            old_locker.close(ctx.accounts.old_owner.to_account_info())?;
        }

        new_locker.owner = ctx.accounts.new_owner.key();
        new_locker.country_code = old_locker.country_code;
        new_locker.current_unlock_date = old_locker.current_unlock_date;
        new_locker.start_emission = old_locker.start_emission;

        new_locker.original_unlock_date = old_locker.current_unlock_date;
        new_locker.creator = ctx.accounts.old_owner.key();
        new_locker.bump = args.locker_bump;

        new_locker.deposited_amount = args.amount;
        new_locker.vault = ctx.accounts.new_vault.key();
        new_locker.vault_bump = args.vault_bump;

        Ok(())
    }

    pub fn close_locker(ctx: Context<CloseLocker>) -> Result<()> {
        let locker = &ctx.accounts.locker;
        let vault = &mut ctx.accounts.vault;

        let locker_key = locker.key();
        let seeds = &[locker_key.as_ref(), &[locker.vault_bump]];
        let signer = &[&seeds[..]];

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: vault.to_account_info(),
                to: ctx.accounts.target_wallet.to_account_info(),
                authority: ctx.accounts.vault_authority.to_account_info(),
            },
            signer,
        );
        token::transfer(cpi_ctx, vault.amount)?;

        vault.reload()?;
        require!(vault.amount == 0, InvalidAmountTransferred);

        vault.close(ctx.accounts.owner.to_account_info())?;
        locker.close(ctx.accounts.owner.to_account_info())?;

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
    #[account(mut)]
    fee_wallet: AccountInfo<'info>,

    clock: Sysvar<'info, Clock>,
    system_program: Program<'info, System>,
    token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Relock<'info> {
    #[account(mut)]
    locker: ProgramAccount<'info, Locker>,
    #[account(
        signer,
        constraint = locker.owner == owner.key()
    )]
    owner: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct TransferOwnership<'info> {
    #[account(mut)]
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

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct SplitLockerArgs {
    locker_bump: u8,
    vault_bump: u8,
    amount: u64,
}

#[derive(Accounts)]
#[instruction(args: SplitLockerArgs)]
pub struct SplitLocker<'info> {
    #[account(mut)]
    old_locker: ProgramAccount<'info, Locker>,
    #[account(
        signer,
        constraint = old_locker.owner == old_owner.key()
    )]
    old_owner: AccountInfo<'info>,
    old_vault_authority: AccountInfo<'info>,
    #[account(
        mut,
        constraint = old_vault.owner == old_vault_authority.key()
    )]
    old_vault: Account<'info, TokenAccount>,

    #[account(
        init,
        payer = old_owner,
        seeds = [
            old_owner.key().as_ref(),
            old_locker.current_unlock_date.to_be_bytes().as_ref(),
        ],
        bump = args.locker_bump,
    )]
    new_locker: ProgramAccount<'info, Locker>,
    new_owner: AccountInfo<'info>,
    #[account(
        seeds = [
            new_locker.key().as_ref()
        ],
        bump = args.vault_bump
    )]
    new_vault_authority: AccountInfo<'info>,
    #[account(
        mut,
        constraint = new_vault.mint == old_vault.mint
    )]
    new_vault: Account<'info, TokenAccount>,

    token_program: Program<'info, Token>,
    system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CloseLocker<'info> {
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
