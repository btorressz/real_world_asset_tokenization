use anchor_lang::prelude::*;
use anchor_lang::solana_program::{system_program, sysvar};
use anchor_spl::token::{
    self, 
    Token, 
    TokenAccount, 
    Mint, 
    MintTo, 
    FreezeAccount, 
    ThawAccount, 
    Burn, 
    Transfer, 
    SetAuthority,
    spl_token::instruction::AuthorityType,
};

declare_id!("C2A1Q5DjAnLbCAjWhJo3pXBi7UgnLYPc4TDJ5qTSNmpp");

#[program]
pub mod real_world_asset_tokenization {
    use super::*;

    // ───────────────────────────────────────────────────────────────
    //   Create a new asset with a fresh token mint + metadata
    // ───────────────────────────────────────────────────────────────
    pub fn initialize_asset(
        ctx: Context<InitializeAsset>,
        asset_name: String,
        symbol: String,
        uri: String,
        decimals: u8,
        total_supply: u64,
    ) -> Result<()> {
        //  Create (initialize) the SPL mint
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_accounts = token::InitializeMint {
            mint: ctx.accounts.mint.to_account_info(),
            rent: ctx.accounts.rent.to_account_info(),
        };
        token::initialize_mint(
            CpiContext::new(cpi_program, cpi_accounts),
            decimals,
            &ctx.accounts.payer.key(),
            Some(&ctx.accounts.payer.key()), // freeze authority = payer
        )?;

        // Create asset metadata
        let asset_account = &mut ctx.accounts.asset_account;
        asset_account.creator = ctx.accounts.payer.key();
        asset_account.mint = ctx.accounts.mint.key();
        asset_account.asset_name = asset_name;
        asset_account.symbol = symbol;
        asset_account.uri = uri;

        // Mint total_supply to the destination account
        let mint_cpi_accounts = MintTo {
            mint: ctx.accounts.mint.to_account_info(),
            to: ctx.accounts.destination_token_account.to_account_info(),
            authority: ctx.accounts.payer.to_account_info(),
        };
        let mint_cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), mint_cpi_accounts);
        token::mint_to(mint_cpi_ctx, total_supply)?;

        // Emit event
        emit!(InitializeAssetEvent {
            creator: ctx.accounts.payer.key(),
            mint: ctx.accounts.mint.key(),
            total_supply
        });

        Ok(())
    }

    // ───────────────────────────────────────────────────────────────
    //   Update Metadata (e.g., URI)
    // ───────────────────────────────────────────────────────────────
    pub fn update_metadata(ctx: Context<UpdateMetadata>, new_uri: String) -> Result<()> {
        let asset_account = &mut ctx.accounts.asset_account;
        // Only the original creator can update (naive check)
        require_keys_eq!(
            asset_account.creator, 
            ctx.accounts.updater.key(), 
            CustomError::Unauthorized
        );
        asset_account.uri = new_uri;
        Ok(())
    }

    // ───────────────────────────────────────────────────────────────
    //   Freeze a token account
    // ───────────────────────────────────────────────────────────────
    pub fn freeze_tokens(ctx: Context<FreezeTokens>) -> Result<()> {
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_accounts = FreezeAccount {
            account: ctx.accounts.token_account.to_account_info(),
            mint: ctx.accounts.mint.to_account_info(),
            authority: ctx.accounts.freezer.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::freeze_account(cpi_ctx)?;
        Ok(())
    }

    // ───────────────────────────────────────────────────────────────
    //   Thaw a frozen token account
    // ───────────────────────────────────────────────────────────────
    pub fn thaw_tokens(ctx: Context<ThawTokens>) -> Result<()> {
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_accounts = ThawAccount {
            account: ctx.accounts.token_account.to_account_info(),
            mint: ctx.accounts.mint.to_account_info(),
            authority: ctx.accounts.freezer.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::thaw_account(cpi_ctx)?;
        Ok(())
    }

    // ───────────────────────────────────────────────────────────────
    //   Burn tokens (e.g., for redemption)
    // ───────────────────────────────────────────────────────────────
    pub fn burn_tokens(ctx: Context<BurnTokens>, amount: u64) -> Result<()> {
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_accounts = Burn {
            from: ctx.accounts.from.to_account_info(),
            mint: ctx.accounts.mint.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::burn(cpi_ctx, amount)?;
        Ok(())
    }

    // ───────────────────────────────────────────────────────────────
    //   Stake tokens into an escrow account
    // ───────────────────────────────────────────────────────────────
    pub fn stake_tokens(ctx: Context<StakeTokens>, amount: u64) -> Result<()> {
        // Transfer tokens from user to escrow
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_accounts = Transfer {
            from: ctx.accounts.user_token_account.to_account_info(),
            to: ctx.accounts.escrow_token_account.to_account_info(),
            authority: ctx.accounts.staker.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, amount)?;

        // Update the staking account
        let staking_account = &mut ctx.accounts.staking_account;
        staking_account.staker = ctx.accounts.staker.key();
        staking_account.mint = ctx.accounts.mint.key();
        staking_account.staked_amount = staking_account
            .staked_amount
            .checked_add(amount)
            .ok_or(CustomError::Overflow)?;

        let clock = Clock::get()?;
        // If first stake, set last_claimed_time to now
        if staking_account.last_claimed_time == 0 {
            staking_account.last_claimed_time = clock.unix_timestamp;
        }

        // Emit event
        emit!(StakeEvent {
            staker: ctx.accounts.staker.key(),
            amount
        });

        Ok(())
    }

    // ───────────────────────────────────────────────────────────────
    //   Claim yield (naive example)
    // ───────────────────────────────────────────────────────────────
    pub fn claim_yield(ctx: Context<ClaimYield>) -> Result<()> {
        let staking_account = &mut ctx.accounts.staking_account;
        let clock = Clock::get()?;
        let now = clock.unix_timestamp;

        //  Calculate how much yield accrued
        let time_diff = (now - staking_account.last_claimed_time) as u64;
        let yield_rate_per_second: u64 = 10; // just a demonstration
        let accrued = time_diff
            .checked_mul(yield_rate_per_second)
            .ok_or(CustomError::Overflow)?;

        //  Mint new tokens to user_reward_ata
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_accounts = MintTo {
            mint: ctx.accounts.mint.to_account_info(),
            to: ctx.accounts.user_reward_ata.to_account_info(),
            authority: ctx.accounts.reward_mint_authority.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::mint_to(cpi_ctx, accrued)?;

        //  Update last_claimed_time
        staking_account.last_claimed_time = now;

        emit!(ClaimYieldEvent {
            staker: ctx.accounts.staker.key(),
            amount: accrued
        });

        Ok(())
    }

    // ───────────────────────────────────────────────────────────────
    //   Unstake tokens (withdraw from escrow)
    // ───────────────────────────────────────────────────────────────
    pub fn unstake_tokens(ctx: Context<UnstakeTokens>, amount: u64) -> Result<()> {
        let staking_account = &mut ctx.accounts.staking_account;

        // Ensure staked_amount >= amount
        require!(
            staking_account.staked_amount >= amount, 
            CustomError::InsufficientStakedBalance
        );

        // Transfer tokens from escrow back to user's token account
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_accounts = Transfer {
            from: ctx.accounts.escrow_token_account.to_account_info(),
            to: ctx.accounts.user_token_account.to_account_info(),
            authority: ctx.accounts.escrow_authority.to_account_info(),
        };

        // Instead of inline arrays referencing temporary values,
        //  create stable local variables:
        let bump = ctx.bumps.escrow_authority;
        let staking_key = staking_account.key(); // store the pubkey to avoid temps

        let seeds = &[
            b"escrow-authority",
            staking_key.as_ref(),
            &[bump],
        ];
        let signer_seeds = &[&seeds[..]];

        // Use the stable seeds array
        let cpi_ctx = CpiContext::new_with_signer(
            cpi_program,
            cpi_accounts,
            signer_seeds,
        );
        token::transfer(cpi_ctx, amount)?;

        // Decrease staked_amount
        staking_account.staked_amount = staking_account
            .staked_amount
            .checked_sub(amount)
            .ok_or(CustomError::Overflow)?;

        emit!(UnstakeEvent {
            staker: ctx.accounts.staker.key(),
            amount
        });

        Ok(())
    }

    // ───────────────────────────────────────────────────────────────
    //   Close the staking account (if zero balance)
    // ───────────────────────────────────────────────────────────────
    pub fn close_staking_account(ctx: Context<CloseStakingAccount>) -> Result<()> {
        let staking_account = &mut ctx.accounts.staking_account;
        // Must have zero staked tokens
        require!(
            staking_account.staked_amount == 0, 
            CustomError::NonZeroStakedBalance
        );

        // Transfer any rent-exempt SOL back to the staker
        let staker = &ctx.accounts.staker;
        **staker.to_account_info().try_borrow_mut_lamports()? += **staking_account.to_account_info().lamports.borrow();
        **staking_account.to_account_info().lamports.borrow_mut() = 0;

        Ok(())
    }

    // ───────────────────────────────────────────────────────────────
    //  Transfer mint authority (or freeze authority)
    // ───────────────────────────────────────────────────────────────
    pub fn transfer_mint_authority(ctx: Context<TransferAuthority>) -> Result<()> {
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_accounts = SetAuthority {
            account_or_mint: ctx.accounts.mint.to_account_info(),
            current_authority: ctx.accounts.current_authority.to_account_info(),
        };

        // Example: transferring Mint authority
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::set_authority(
            cpi_ctx, 
            AuthorityType::MintTokens, 
            Some(ctx.accounts.new_authority.key())
        )?;

        emit!(TransferAuthorityEvent {
            old_authority: ctx.accounts.current_authority.key(),
            new_authority: ctx.accounts.new_authority.key(),
        });

        Ok(())
    }

    // ───────────────────────────────────────────────────────────────
    // (Optional) 11) Update oracle price (placeholder)
    // ───────────────────────────────────────────────────────────────
    /// TODO: add pyth and/or switchboard
    pub fn update_price(ctx: Context<UpdatePrice>, new_price: u64) -> Result<()> {
        let price_feed = &mut ctx.accounts.price_feed;
        price_feed.price = new_price;
        price_feed.last_update = Clock::get()?.unix_timestamp;
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────
//  Accounts & Data
// ─────────────────────────────────────────────────────────────────────

/// (A) Initialize Asset
#[derive(Accounts)]
#[instruction(asset_name: String, symbol: String, uri: String, decimals: u8, total_supply: u64)]
pub struct InitializeAsset<'info> {
    /// Pays for creation of mint + metadata
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Stores metadata (creator, symbol, uri, etc.)
    #[account(
        init,
        payer = payer,
        space = 8 + 32 + (4 + 50) + (4 + 10) + (4 + 200) + 32,
        seeds = [b"asset-metadata", payer.key().as_ref(), asset_name.as_bytes()],
        bump
    )]
    pub asset_account: Account<'info, AssetAccount>,

    /// SPL Mint for the new token
    #[account(
        init,
        payer = payer,
        space = 82, // size of Mint
        seeds = [b"asset-mint", payer.key().as_ref(), asset_name.as_bytes()],
        bump
    )]
    pub mint: Account<'info, Mint>,

    /// The token account receiving the minted tokens
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint,
        associated_token::authority = payer
    )]
    pub destination_token_account: Account<'info, TokenAccount>,

    /// Programs / Sysvars
    pub token_program: Program<'info, Token>,
    #[account(address = sysvar::rent::ID)]
    pub rent: Sysvar<'info, Rent>,
    #[account(address = system_program::ID)]
    pub system_program: Program<'info, System>,
    #[account(address = anchor_spl::associated_token::ID)]
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
}

/// (AssetAccount)
#[account]
pub struct AssetAccount {
    pub creator: Pubkey,
    pub mint: Pubkey,
    pub asset_name: String,
    pub symbol: String,
    pub uri: String,
}

///  Update Metadata
#[derive(Accounts)]
pub struct UpdateMetadata<'info> {
    #[account(mut)]
    pub updater: Signer<'info>,
    #[account(mut)]
    pub asset_account: Account<'info, AssetAccount>,
}

///  Freeze Tokens
#[derive(Accounts)]
pub struct FreezeTokens<'info> {
    #[account(mut)]
    pub freezer: Signer<'info>,
    #[account(mut)]
    pub token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
}

///  Thaw Tokens
#[derive(Accounts)]
pub struct ThawTokens<'info> {
    #[account(mut)]
    pub freezer: Signer<'info>,
    #[account(mut)]
    pub token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
}

///  Burn Tokens
#[derive(Accounts)]
pub struct BurnTokens<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(mut)]
    pub from: Account<'info, TokenAccount>,
    #[account(mut)]
    pub mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
}

///  Staking Account
#[account]
pub struct StakingAccount {
    pub staker: Pubkey,
    pub mint: Pubkey,
    pub staked_amount: u64,
    pub last_claimed_time: i64,
}

///  Stake Tokens
#[derive(Accounts)]
pub struct StakeTokens<'info> {
    #[account(mut)]
    pub staker: Signer<'info>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,

    
    /// The escrow account owned by a PDA. In a live environment, 
    //it may be created through a separate instruction or ensured to be initialized beforehand.
    #[account(mut)]
    pub escrow_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub mint: Account<'info, Mint>,

    /// Tracks staked_amount, last_claimed_time, etc.
    #[account(
        init_if_needed,
        payer = staker,
        space = 8 + 32 + 32 + 8 + 8,
        seeds = [b"stake-account", staker.key().as_ref(), mint.key().as_ref()],
        bump
    )]
    pub staking_account: Account<'info, StakingAccount>,

    #[account(address = system_program::ID)]
    pub system_program: Program<'info, System>,

    pub token_program: Program<'info, Token>,
}

///  Claim Yield
#[derive(Accounts)]
pub struct ClaimYield<'info> {
    #[account(mut)]
    pub staker: Signer<'info>,
    #[account(mut)]
    pub staking_account: Account<'info, StakingAccount>,
    #[account(mut)]
    pub mint: Account<'info, Mint>,
    #[account(mut)]
    pub user_reward_ata: Account<'info, TokenAccount>,
    #[account(mut)]
    pub reward_mint_authority: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

///  Unstake Tokens
#[derive(Accounts)]
pub struct UnstakeTokens<'info> {
    #[account(mut)]
    pub staker: Signer<'info>,

    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,

    /// Program-owned escrow holding staked tokens
    #[account(mut)]
    pub escrow_token_account: Account<'info, TokenAccount>,

    /// PDA authority over escrow_token_account
    #[account(
        seeds = [b"escrow-authority", staking_account.key().as_ref()],
        bump
    )]
    pub escrow_authority: SystemAccount<'info>,

    #[account(mut)]
    pub staking_account: Account<'info, StakingAccount>,

    pub token_program: Program<'info, Token>,
}

///  Close Staking Account
#[derive(Accounts)]
pub struct CloseStakingAccount<'info> {
    #[account(mut)]
    pub staker: Signer<'info>,
    #[account(
        mut,
        close = staker, // returns rent to staker
        constraint = staking_account.staker == staker.key() @ CustomError::Unauthorized
    )]
    pub staking_account: Account<'info, StakingAccount>,
}

///  Transfer Mint Authority
#[derive(Accounts)]
pub struct TransferAuthority<'info> {
    #[account(mut)]
    pub current_authority: Signer<'info>,
    #[account(mut)]
    pub mint: Account<'info, Mint>,
    /// The new authority (could be a DAO or multi-sig, or another user)
    pub new_authority: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token>,
}

///  Price Feed Account (Optional)
#[account]
pub struct PriceFeedAccount {
    pub price: u64,       // e.g. commodity price
    pub last_update: i64, // timestamp
}

/// Update Price (Oracle) Context
#[derive(Accounts)]
pub struct UpdatePrice<'info> {
    #[account(mut)]
    pub oracle_updater: Signer<'info>,
    #[account(mut)]
    pub price_feed: Account<'info, PriceFeedAccount>,
}

// ─────────────────────────────────────────────────────────────────────
//   Custom Errors
// ─────────────────────────────────────────────────────────────────────
#[error_code]
pub enum CustomError {
    #[msg("You are not authorized to perform this action.")]
    Unauthorized,
    #[msg("Operation caused an overflow.")]
    Overflow,
    #[msg("Insufficient staked balance.")]
    InsufficientStakedBalance,
    #[msg("Staked balance must be zero before closing the account.")]
    NonZeroStakedBalance,
}

// ─────────────────────────────────────────────────────────────────────
//   Events (for off-chain indexing)
// ─────────────────────────────────────────────────────────────────────
#[event]
pub struct InitializeAssetEvent {
    pub creator: Pubkey,
    pub mint: Pubkey,
    pub total_supply: u64,
}

#[event]
pub struct StakeEvent {
    pub staker: Pubkey,
    pub amount: u64,
}

#[event]
pub struct UnstakeEvent {
    pub staker: Pubkey,
    pub amount: u64,
}

#[event]
pub struct ClaimYieldEvent {
    pub staker: Pubkey,
    pub amount: u64,
}

#[event]
pub struct TransferAuthorityEvent {
    pub old_authority: Pubkey,
    pub new_authority: Pubkey,
}
