#![cfg(test)]

use anchor_lang::prelude::*;
use anchor_lang::{InstructionData, ToAccountMetas};
use commons_rewards::{
    self, accounts as rewards_accounts, instruction as rewards_instruction, RewardError, RewardEpoch,
    ClaimStatus, ID as REWARDS_ID,
};
use solana_program::system_instruction;
use solana_program_test::{processor, ProgramTest};
use solana_sdk::{
    instruction::Instruction, pubkey::Pubkey, signature::Keypair, signature::Signer, transaction::Transaction,
};
use spl_associated_token_account::{
    create_associated_token_account, get_associated_token_address, id as associated_token_program_id,
};
use spl_token::{instruction as token_instruction, state::Account as TokenAccountState, Mint};

async fn process_transaction(
    banks_client: &mut solana_program_test::BanksClient,
    payer: &Keypair,
    instructions: Vec<Instruction>,
    signers: Vec<&Keypair>,
) {
    let recent_blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut all_signers = vec![payer];
    all_signers.extend(signers);
    let tx = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &all_signers,
        recent_blockhash,
    );
    banks_client.process_transaction(tx).await.unwrap();
}

async fn create_mint(
    banks_client: &mut solana_program_test::BanksClient,
    payer: &Keypair,
    authority: &Pubkey,
) -> Pubkey {
    let mint = Keypair::new();
    let rent = banks_client.get_rent().await.unwrap();
    let lamports = rent.minimum_balance(Mint::LEN);
    let create_account = system_instruction::create_account(
        &payer.pubkey(),
        &mint.pubkey(),
        lamports,
        Mint::LEN as u64,
        &spl_token::id(),
    );
    let init_mint = token_instruction::initialize_mint(
        &spl_token::id(),
        &mint.pubkey(),
        authority,
        None,
        6,
    )
    .unwrap();
    process_transaction(
        banks_client,
        payer,
        vec![create_account, init_mint],
        vec![&mint],
    )
    .await;
    mint.pubkey()
}

async fn mint_to_account(
    banks_client: &mut solana_program_test::BanksClient,
    payer: &Keypair,
    mint: &Pubkey,
    destination: &Pubkey,
    authority: &Pubkey,
    amount: u64,
) {
    let mint_ix = token_instruction::mint_to(
        &spl_token::id(),
        mint,
        destination,
        authority,
        &[],
        amount,
    )
    .unwrap();
    process_transaction(banks_client, payer, vec![mint_ix], vec![]).await;
}

#[tokio::test]
async fn claim_reward_prevents_double_claim() {
    let mut program = ProgramTest::new(
        "commons_rewards",
        REWARDS_ID,
        processor!(commons_rewards::entry),
    );
    let (mut banks_client, payer, _recent_blockhash) = program.start().await;

    let epoch_id = 1;
    let mint = create_mint(&mut banks_client, &payer, &payer.pubkey()).await;
    let reward_vault_authority = Pubkey::find_program_address(
        &[b"reward_vault_authority", epoch_id.to_le_bytes().as_ref()],
        &REWARDS_ID,
    )
    .0;
    let reward_vault = anchor_spl::associated_token::get_associated_token_address(
        &reward_vault_authority,
        &mint,
    );
    let reward_epoch = Pubkey::find_program_address(
        &[b"reward_epoch", epoch_id.to_le_bytes().as_ref()],
        &REWARDS_ID,
    )
    .0;

    let create_epoch_accounts = rewards_accounts::CreateRewardEpoch {
        reward_epoch,
        reward_vault,
        reward_vault_authority,
        authority: payer.pubkey(),
        reward_mint: mint,
        token_program: spl_token::id(),
        system_program: system_program::ID,
    };
    let create_epoch_ix = Instruction {
        program_id: REWARDS_ID,
        accounts: create_epoch_accounts.to_account_metas(None),
        data: rewards_instruction::CreateRewardEpoch {
            epoch_id,
            total_tokens: 1_000,
            merkle_root: [0u8; 32],
        }
        .data(),
    };
    process_transaction(&mut banks_client, &payer, vec![create_epoch_ix], vec![]).await;

    let recipient = Keypair::new();
    let recipient_account =
        create_associated_token_account(&payer.pubkey(), &recipient.pubkey(), &mint);
    process_transaction(&mut banks_client, &payer, vec![recipient_account], vec![]).await;

    let claim_ix = Instruction {
        program_id: REWARDS_ID,
        accounts: rewards_accounts::ClaimReward {
            reward_epoch,
            reward_vault,
            reward_vault_authority,
            user_reward_token_account: get_associated_token_address(&recipient.pubkey(), &mint),
            claim_status: Pubkey::find_program_address(
                &[b"reward_claim", epoch_id.to_le_bytes().as_ref(), recipient.pubkey().as_ref()],
                &REWARDS_ID,
            )
            .0,
            authority: recipient.pubkey(),
            token_program: spl_token::id(),
            system_program: system_program::ID,
            rent: sysvar::rent::ID,
        }
        .to_account_metas(None),
        data: rewards_instruction::ClaimReward {
            epoch_id,
            amount: 0,
            proof: vec![],
        }
        .data(),
    };

    let tx = Transaction::new_signed_with_payer(
        &[claim_ix.clone()],
        Some(&payer.pubkey()),
        &[&payer, &recipient],
        banks_client.get_latest_blockhash().await.unwrap(),
    );
    banks_client.process_transaction(tx).await.unwrap();

    let tx2 = Transaction::new_signed_with_payer(
        &[claim_ix],
        Some(&payer.pubkey()),
        &[&payer, &recipient],
        banks_client.get_latest_blockhash().await.unwrap(),
    );
    let err = banks_client.process_transaction(tx2).await.unwrap_err();
    match err {
        TransportError::TransactionError(tx_err) => match tx_err {
            TransactionError::InstructionError(_, InstructionError::Custom(code)) => {
                assert_eq!(code, RewardError::AlreadyClaimed as u32);
            }
            _ => panic!("unexpected tx error: {:?}", tx_err),
        },
        _ => panic!("expected transaction failure, got {:?}", err),
    }
}
