use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

declare_id!("GccA6L8BUnkZVeUAdSAeoiFFCVynf6GZbBTPZfCj7tpY");

#[program]
pub mod commons_rewards {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        msg!("Greetings from: {:?}", ctx.program_id);
        Ok(())
    }

    pub fn create_reward_epoch(
        ctx: Context<CreateRewardEpoch>,
        epoch_id: u64,
        total_tokens: u64,
        merkle_root: [u8; 32],
    ) -> Result<()> {
        let reward_epoch = &mut ctx.accounts.reward_epoch;
        reward_epoch.epoch_id = epoch_id;
        reward_epoch.total_tokens = total_tokens;
        reward_epoch.merkle_root = merkle_root;
        reward_epoch.reward_epoch_bump = ctx.bumps.reward_epoch;
        reward_epoch.reward_vault = ctx.accounts.reward_vault.key();
        reward_epoch.reward_vault_bump = ctx.bumps.reward_vault;
        reward_epoch.reward_vault_authority = ctx.accounts.reward_vault_authority.key();
        reward_epoch.reward_vault_authority_bump = ctx.bumps.reward_vault_authority;
        Ok(())
    }

    pub fn claim_reward(
        ctx: Context<ClaimReward>,
        epoch_id: u64,
        amount: u64,
        proof: Vec<[u8; 32]>,
    ) -> Result<()> {
        let reward_epoch = &ctx.accounts.reward_epoch;

        // Verify Merkle proof
        let leaf = get_leaf_from_claim(&ctx.accounts.authority.key(), amount);
        require!(
            verify_merkle_proof(proof, reward_epoch.merkle_root, leaf),
            RewardError::InvalidMerkleProof
        );

        // Transfer promised amount from Reward Pool PDA to user’s wallet
        let cpi_accounts = Transfer {
            from: ctx.accounts.reward_vault.to_account_info(),
            to: ctx.accounts.user_reward_token_account.to_account_info(),
            authority: ctx.accounts.reward_vault_authority.to_account_info(), // PDA authority
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let epoch_id_bytes = epoch_id.to_le_bytes();
        let bump_bytes = [reward_epoch.reward_vault_authority_bump];
        let authority_seeds: [&[u8]; 3] = [
            b"reward_vault_authority",
            epoch_id_bytes.as_ref(),
            &bump_bytes,
        ];
        let signer = &[&authority_seeds[..]];
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, amount)?;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize {}

#[derive(Accounts)]
#[instruction(epoch_id: u64, total_tokens: u64, merkle_root: [u8; 32])]
pub struct CreateRewardEpoch<'info> {
    #[account(init, payer = authority, space = 8 + 8 + 8 + 32 + 1, seeds = [b"reward_epoch", epoch_id.to_le_bytes().as_ref()], bump)]
    pub reward_epoch: Account<'info, RewardEpoch>,
    #[account(
        init,
        payer = authority,
        token::mint = reward_mint,
        token::authority = reward_vault_authority,
        seeds = [b"reward_vault", epoch_id.to_le_bytes().as_ref()],
        bump
    )]
    pub reward_vault: Account<'info, TokenAccount>,
    /// CHECK: PDA authority for the reward vault
    #[account(
        seeds = [b"reward_vault_authority", epoch_id.to_le_bytes().as_ref()],
        bump
    )]
    pub reward_vault_authority: UncheckedAccount<'info>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub reward_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(epoch_id: u64, amount: u64, proof: Vec<[u8; 32]>)]
pub struct ClaimReward<'info> {
    #[account(mut, seeds = [b"reward_epoch", epoch_id.to_le_bytes().as_ref()], bump = reward_epoch.reward_epoch_bump)]
    pub reward_epoch: Account<'info, RewardEpoch>,
    #[account(mut)]
    pub reward_vault: Account<'info, TokenAccount>,
    /// CHECK: This is the PDA authority for the reward vault
    #[account(
        seeds = [b"reward_vault_authority", epoch_id.to_le_bytes().as_ref()],
        bump = reward_epoch.reward_vault_authority_bump
    )]
    pub reward_vault_authority: UncheckedAccount<'info>,
    #[account(mut)]
    pub user_reward_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[account]
pub struct RewardEpoch {
    pub epoch_id: u64,
    pub total_tokens: u64,
    pub merkle_root: [u8; 32],
    pub reward_epoch_bump: u8,
    pub reward_vault: Pubkey,
    pub reward_vault_bump: u8,
    pub reward_vault_authority: Pubkey,
    pub reward_vault_authority_bump: u8,
}

// Helper function to create a leaf from the claimer's public key and amount
fn get_leaf_from_claim(claimer: &Pubkey, amount: u64) -> [u8; 32] {
    let mut data = claimer.to_bytes().to_vec();
    data.extend_from_slice(&amount.to_le_bytes());
    hash::hashv(&[&data]).to_bytes()
}

// Helper function to verify Merkle proof
fn verify_merkle_proof(proof: Vec<[u8; 32]>, root: [u8; 32], leaf: [u8; 32]) -> bool {
    let mut computed_hash = leaf;
    for proof_element in proof {
        if computed_hash <= proof_element {
            // Hash(computed_hash + proof_element)
            computed_hash = hash::hashv(&[&computed_hash, &proof_element]).to_bytes();
        } else {
            // Hash(proof_element + computed_hash)
            computed_hash = hash::hashv(&[&proof_element, &computed_hash]).to_bytes();
        }
    }
    computed_hash == root
}

#[error_code]
pub enum RewardError {
    #[msg("Invalid Merkle proof.")]
    InvalidMerkleProof,
}
