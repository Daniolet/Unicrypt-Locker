use std::ops::DerefMut;

use anchor_lang::{
    prelude::*,
    solana_program::{self, log::sol_log_64},
    AccountsClose,
};
use anchor_spl::{
    associated_token::get_associated_token_address,
    token::{self, CloseAccount, Mint, Token, TokenAccount, Transfer},
};

use az::CheckedAs;

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

mod fee {
    use super::*;

    declare_id!("7vPbNKWdgS1dqx6ZnJR8dU9Mo6Tsgwp3S5rALuANwXiJ");

    pub const FEE_SOL: u64 = 1 * solana_program::native_token::LAMPORTS_PER_SOL;
    pub const FEE_PERMILLE: u64 = 35;
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

    pub fn init_mint_info(ctx: Context<InitMintInfo>, bump: u8) -> Result<()> {
        let mint_info = ctx.accounts.mint_info.deref_mut();

        *mint_info = MintInfo {
            bump,
            fee_paid: false,
        };

        Ok(())
    }

    pub fn create_locker(ctx: Context<CreateLocker>, args: CreateLockerArgs) -> Result<()> {
        let now = ctx.accounts.clock.unix_timestamp;
        require!(args.unlock_date > now, UnlockInThePast);
        // prevents errors when timestamp entered as milliseconds
        require!(args.unlock_date < 10000000000, InvalidTimestamp);

        if let Some(start_emission) = args.start_emission {
            require!(args.unlock_date > start_emission, InvalidPeriod);
        }

        let mint_info = &mut ctx.accounts.mint_info;

        let (amount_before, amount_to_lock) = if args.fee_in_sol {
            require!(ctx.accounts.fee_wallet.key() == fee::ID, InvalidFeeWallet);

            ctx.accounts.owner.key().log();
            ctx.accounts.fee_wallet.key().log();

            if !mint_info.fee_paid {
                solana_program::program::invoke(
                    &solana_program::system_instruction::transfer(
                        ctx.accounts.owner.to_account_info().key,
                        ctx.accounts.fee_wallet.key,
                        fee::FEE_SOL,
                    ),
                    &[
                        ctx.accounts.owner.to_account_info(),
                        ctx.accounts.fee_wallet.to_account_info(),
                        ctx.accounts.system_program.to_account_info(),
                    ],
                )?;
                mint_info.fee_paid = true;
            }

            (ctx.accounts.funding_wallet.amount, args.amount)
        } else {
            let associated_token_account =
                get_associated_token_address(&fee::ID, &ctx.accounts.funding_wallet.mint);

            require!(
                associated_token_account == ctx.accounts.fee_wallet.key(),
                InvalidFeeWallet
            );

            let amount_before = ctx.accounts.funding_wallet.amount;

            let lock_fee =
                mul_div(args.amount, fee::FEE_PERMILLE, 10000).ok_or(ErrorCode::IntegerOverflow)?;

            ctx.accounts.funding_wallet.key().log();
            ctx.accounts.fee_wallet.key().log();
            ctx.accounts.funding_wallet_authority.key().log();

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

            sol_log_64(
                args.amount,
                amount_before,
                lock_fee,
                amount_after_fee,
                args.amount - lock_fee,
            );

            require!(
                amount_before - amount_after_fee == lock_fee,
                InvalidAmountTransferred
            );

            (amount_after_fee, args.amount - lock_fee)
        };

        require!(amount_to_lock > 0, NothingToLock);

        let locker = ctx.accounts.locker.deref_mut();

        *locker = Locker {
            owner: ctx.accounts.owner.key(),
            country_code: args.country_code,
            current_unlock_date: args.unlock_date,
            start_emission: args.start_emission,
            deposited_amount: amount_to_lock,
            vault: ctx.accounts.vault.key(),
            vault_bump: args.vault_bump,
            creator: ctx.accounts.creator.key(),
            original_unlock_date: args.unlock_date,
            bump: args.locker_bump,
        };

        ctx.accounts.funding_wallet.key().log();
        ctx.accounts.vault.key().log();
        ctx.accounts.funding_wallet_authority.key().log();

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
            amount_before - amount_final == amount_to_lock,
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

    pub fn increment_lock(ctx: Context<IncrementLock>, amount: u64) -> Result<()> {
        let locker = &mut ctx.accounts.locker;
        let mint_info = &ctx.accounts.mint_info;

        let amount_to_lock = if mint_info.fee_paid {
            amount
        } else {
            let lock_fee =
                mul_div(amount, fee::FEE_PERMILLE, 10000).ok_or(ErrorCode::IntegerOverflow)?;

            let associated_token_account =
                get_associated_token_address(&fee::ID, &ctx.accounts.funding_wallet.mint);

            require!(
                associated_token_account == ctx.accounts.fee_wallet.key(),
                InvalidFeeWallet
            );

            let cpi_ctx = CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.funding_wallet.to_account_info(),
                    to: ctx.accounts.fee_wallet.to_account_info(),
                    authority: ctx.accounts.funding_wallet_authority.to_account_info(),
                },
            );
            token::transfer(cpi_ctx, lock_fee)?;

            amount - lock_fee
        };

        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.funding_wallet.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
                authority: ctx.accounts.funding_wallet_authority.to_account_info(),
            },
        );
        token::transfer(cpi_ctx, amount_to_lock)?;

        locker.deposited_amount = locker
            .deposited_amount
            .checked_add(amount_to_lock)
            .ok_or(ErrorCode::IntegerOverflow)?;

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
        sol_log_64(
            amount,
            amount_before,
            amount_to_transfer,
            amount_after,
            amount_before - amount,
        );
        require!(
            amount_before - amount_after == amount_to_transfer,
            InvalidAmountTransferred
        );

        if vault.amount == 0 {
            let cpi_ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                CloseAccount {
                    account: vault.to_account_info(),
                    destination: ctx.accounts.owner.to_account_info(),
                    authority: ctx.accounts.vault_authority.to_account_info(),
                },
                signer,
            );
            token::close_account(cpi_ctx)?;

            locker.close(ctx.accounts.owner.to_account_info())?;
        }

        Ok(())
    }

    pub fn split_locker(ctx: Context<SplitLocker>, args: SplitLockerArgs) -> Result<()> {
        require!(args.amount > 0, InvalidAmount);

        let new_locker = ctx.accounts.new_locker.deref_mut();
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

        old_locker.deposited_amount = old_locker
            .deposited_amount
            .checked_sub(args.amount)
            .ok_or(ErrorCode::IntegerOverflow)?;

        if old_vault.amount == 0 {
            let cpi_ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                CloseAccount {
                    account: old_vault.to_account_info(),
                    destination: ctx.accounts.old_owner.to_account_info(),
                    authority: ctx.accounts.old_vault_authority.to_account_info(),
                },
                signer,
            );
            token::close_account(cpi_ctx)?;

            old_locker.close(ctx.accounts.old_owner.to_account_info())?;
        }

        *new_locker = Locker {
            owner: ctx.accounts.new_owner.key(),
            country_code: old_locker.country_code,
            current_unlock_date: old_locker.current_unlock_date,
            start_emission: old_locker.start_emission,
            deposited_amount: args.amount,
            vault: ctx.accounts.new_vault.key(),
            vault_bump: args.vault_bump,
            creator: ctx.accounts.old_owner.key(),
            original_unlock_date: old_locker.current_unlock_date,
            bump: args.locker_bump,
        };

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

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            CloseAccount {
                account: vault.to_account_info(),
                destination: ctx.accounts.owner.to_account_info(),
                authority: ctx.accounts.vault_authority.to_account_info(),
            },
            signer,
        );
        token::close_account(cpi_ctx)?;

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

#[account]
pub struct MintInfo {
    bump: u8,
    fee_paid: bool,
}

impl Default for MintInfo {
    fn default() -> Self {
        Self {
            bump: Default::default(),
            fee_paid: Default::default(),
        }
    }
}

#[derive(Accounts)]
#[instruction(bump: u8)]
pub struct InitMintInfo<'info> {
    #[account(signer)]
    payer: AccountInfo<'info>,
    #[account(
        init,
        payer = payer,
        seeds = [
            mint.key().as_ref(),
        ],
        bump = bump
    )]
    mint_info: ProgramAccount<'info, MintInfo>,
    mint: Account<'info, Mint>,

    system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct CreateLockerArgs {
    amount: u64,
    unlock_date: i64,
    country_code: u16,
    start_emission: Option<i64>,
    locker_bump: u8,
    vault_bump: u8,
    fee_in_sol: bool,
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
            args.amount.to_be_bytes().as_ref(),
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
    #[account(
        mut,
        seeds = [
            vault.mint.key().as_ref()
        ],
        bump = mint_info.bump
    )]
    mint_info: ProgramAccount<'info, MintInfo>,

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
pub struct IncrementLock<'info> {
    #[account(mut)]
    locker: ProgramAccount<'info, Locker>,
    #[account(
        mut,
        constraint = vault.mint == funding_wallet.mint,
        constraint = locker.vault == vault.key()
    )]
    vault: Account<'info, TokenAccount>,
    #[account(
        seeds = [
            vault.mint.key().as_ref()
        ],
        bump = mint_info.bump
    )]
    mint_info: ProgramAccount<'info, MintInfo>,
    #[account(signer)]
    funding_wallet_authority: AccountInfo<'info>,
    #[account(mut)]
    funding_wallet: Account<'info, TokenAccount>,
    #[account(mut)]
    fee_wallet: AccountInfo<'info>,

    token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct WithdrawFunds<'info> {
    #[account(mut)]
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
            old_locker.key().as_ref(),
            old_locker.current_unlock_date.to_be_bytes().as_ref(),
            args.amount.to_be_bytes().as_ref()
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
    #[account(mut)]
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
