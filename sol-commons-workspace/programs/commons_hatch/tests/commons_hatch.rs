#![cfg(test)]

use anchor_lang::prelude::*;
use anchor_lang::{InstructionData, ToAccountMetas};
use commons_abc::ID as ABC_PROGRAM_ID;
use commons_hatch::{
    self, accounts as hatch_accounts, instruction as hatch_instruction, Contribution, HatchConfig,
    ID as HATCH_PROGRAM_ID,
};
use solana_program::{hash::hashv, program_pack::Pack, system_instruction, sysvar};
use solana_program_test::{processor, BanksClient, ProgramTest};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signature::Signer,
    transaction::Transaction,
};
use spl_token::{
    instruction as token_instruction,
    state::{Account as TokenAccountState, Mint},
};
use std::str::FromStr;

async fn process_transaction(
    banks_client: &mut BanksClient,
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
    banks_client: &mut BanksClient,
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
    let init_mint =
        token_instruction::initialize_mint(&spl_token::id(), &mint.pubkey(), authority, None, 6)
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

async fn create_user_token_account(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    owner: &Keypair,
    mint: &Pubkey,
) -> Pubkey {
    let ata = associated_token_address(&owner.pubkey(), mint);
    let create_ata =
        create_associated_token_account_instruction(&payer.pubkey(), &owner.pubkey(), mint);
    process_transaction(banks_client, payer, vec![create_ata], vec![]).await;
    ata
}

async fn mint_to_account(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    mint: &Pubkey,
    destination: &Pubkey,
    authority: &Keypair,
    amount: u64,
) {
    let mint_ix = token_instruction::mint_to(
        &spl_token::id(),
        mint,
        destination,
        &authority.pubkey(),
        &[],
        amount,
    )
    .unwrap();
    process_transaction(banks_client, payer, vec![mint_ix], vec![authority]).await;
}

fn merkle_leaf(contributor: &Pubkey, allocation: u64) -> [u8; 32] {
    let mut data = contributor.to_bytes().to_vec();
    data.extend_from_slice(&allocation.to_le_bytes());
    hashv(&[&data]).to_bytes()
}

fn associated_token_program_id() -> Pubkey {
    Pubkey::from_str("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL").unwrap()
}

fn associated_token_address(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[owner.as_ref(), spl_token::id().as_ref(), mint.as_ref()],
        &associated_token_program_id(),
    )
    .0
}

fn create_associated_token_account_instruction(
    payer: &Pubkey,
    owner: &Pubkey,
    mint: &Pubkey,
) -> Instruction {
    let associated_token_account = associated_token_address(owner, mint);
    Instruction {
        program_id: associated_token_program_id(),
        accounts: vec![
            AccountMeta::new(*payer, true),
            AccountMeta::new(associated_token_account, false),
            AccountMeta::new_readonly(*owner, false),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(sysvar::rent::ID, false),
        ],
        data: vec![],
    }
}

#[tokio::test]
async fn finalize_and_claim_mints_tokens() {
    let mut program = ProgramTest::new(
        "commons_hatch",
        HATCH_PROGRAM_ID,
        processor!(commons_hatch::entry),
    );
    program.add_program(
        "commons_abc",
        ABC_PROGRAM_ID,
        processor!(commons_abc::entry),
    );

    let (mut banks_client, payer, _recent_blockhash) = program.start().await;
    let user = Keypair::new();

    let transfer = system_instruction::transfer(&payer.pubkey(), &user.pubkey(), 5_000_000_000);
    process_transaction(&mut banks_client, &payer, vec![transfer], vec![]).await;

    let reserve_mint = create_mint(&mut banks_client, &payer, &payer.pubkey()).await;
    let hatch_config =
        Pubkey::find_program_address(&[b"hatch_config", reserve_mint.as_ref()], &HATCH_PROGRAM_ID)
            .0;
    let hatch_vault =
        Pubkey::find_program_address(&[b"hatch_vault", reserve_mint.as_ref()], &HATCH_PROGRAM_ID).0;

    let merkle_root = merkle_leaf(&user.pubkey(), 100);
    let init_accounts = hatch_accounts::InitializeHatch {
        hatch_config,
        reserve_asset_mint: reserve_mint,
        hatch_vault,
        authority: payer.pubkey(),
        system_program: system_program::ID,
        token_program: spl_token::id(),
        rent: sysvar::rent::ID,
    };
    let init_ix = Instruction {
        program_id: HATCH_PROGRAM_ID,
        accounts: init_accounts.to_account_metas(None),
        data: hatch_instruction::InitializeHatch {
            min_raise: 50,
            max_raise: 200,
            open_slot: 0,
            close_slot: 0,
            merkle_root,
        }
        .data(),
    };
    process_transaction(&mut banks_client, &payer, vec![init_ix], vec![]).await;

    let user_reserve_account =
        create_user_token_account(&mut banks_client, &payer, &user, &reserve_mint).await;
    mint_to_account(
        &mut banks_client,
        &payer,
        &reserve_mint,
        &user_reserve_account,
        &payer,
        120,
    )
    .await;

    let (contribution, _) = Pubkey::find_program_address(
        &[b"contribution", user.pubkey().as_ref()],
        &HATCH_PROGRAM_ID,
    );
    let contribute_accounts = hatch_accounts::Contribute {
        hatch_config,
        contribution,
        hatch_vault,
        user_reserve_token_account: user_reserve_account,
        authority: user.pubkey(),
        system_program: system_program::ID,
        token_program: spl_token::id(),
    };
    let contribute_ix = Instruction {
        program_id: HATCH_PROGRAM_ID,
        accounts: contribute_accounts.to_account_metas(None),
        data: hatch_instruction::Contribute {
            amount: 60,
            allowed_allocation: 100,
            proof: vec![],
        }
        .data(),
    };
    process_transaction(&mut banks_client, &payer, vec![contribute_ix], vec![&user]).await;

    let commons_token_mint = Pubkey::find_program_address(
        &[b"commons_token_mint", hatch_config.as_ref()],
        &HATCH_PROGRAM_ID,
    )
    .0;
    let curve_config = Pubkey::find_program_address(
        &[b"curve_config", commons_token_mint.as_ref()],
        &ABC_PROGRAM_ID,
    )
    .0;
    let reserve_vault = Keypair::new();
    let commons_treasury = Keypair::new();

    let finalize_accounts = hatch_accounts::FinalizeHatch {
        hatch_config,
        reserve_asset_mint: reserve_mint,
        authority: payer.pubkey(),
        curve_config,
        commons_token_mint,
        reserve_vault: reserve_vault.pubkey(),
        commons_treasury: commons_treasury.pubkey(),
        commons_abc_program: ABC_PROGRAM_ID,
        system_program: system_program::ID,
        token_program: spl_token::id(),
        rent: sysvar::rent::ID,
    };
    let finalize_ix = Instruction {
        program_id: HATCH_PROGRAM_ID,
        accounts: finalize_accounts.to_account_metas(None),
        data: hatch_instruction::FinalizeHatch {
            kappa: 1,
            exponent: 1,
            initial_price: 1,
            friction: 0,
        }
        .data(),
    };
    process_transaction(
        &mut banks_client,
        &payer,
        vec![finalize_ix],
        vec![&reserve_vault, &commons_treasury],
    )
    .await;

    let account = banks_client
        .get_account(hatch_config)
        .await
        .unwrap()
        .expect("failed to fetch hatch config");
    let mut data: &[u8] = &account.data;
    let config = HatchConfig::try_deserialize(&mut data).unwrap();
    assert!(config.finalized);
    assert!(!config.failed);

    let user_commons_account = associated_token_address(&user.pubkey(), &commons_token_mint);
    let claim_accounts = hatch_accounts::Claim {
        hatch_config,
        contribution,
        commons_token_mint,
        curve_config,
        user_commons_token_account: user_commons_account,
        authority: user.pubkey(),
        token_program: spl_token::id(),
        associated_token_program: associated_token_program_id(),
        system_program: system_program::ID,
        rent: sysvar::rent::ID,
    };
    let claim_ix = Instruction {
        program_id: HATCH_PROGRAM_ID,
        accounts: claim_accounts.to_account_metas(None),
        data: hatch_instruction::Claim {}.data(),
    };
    process_transaction(&mut banks_client, &payer, vec![claim_ix], vec![&user]).await;

    let commons_account_data = banks_client
        .get_account(user_commons_account)
        .await
        .unwrap()
        .expect("commons token account missing");
    let token_state = TokenAccountState::unpack(&commons_account_data.data).unwrap();
    assert_eq!(token_state.amount, 60);

    let contribution_account = banks_client
        .get_account(contribution)
        .await
        .unwrap()
        .expect("contribution account missing");
    let mut contrib_data: &[u8] = &contribution_account.data;
    let contribution_state = Contribution::try_deserialize(&mut contrib_data).unwrap();
    assert!(contribution_state.claimed);
}

#[tokio::test]
async fn finalize_failure_triggers_refund_and_close() {
    let mut program = ProgramTest::new(
        "commons_hatch",
        HATCH_PROGRAM_ID,
        processor!(commons_hatch::entry),
    );
    program.add_program(
        "commons_abc",
        ABC_PROGRAM_ID,
        processor!(commons_abc::entry),
    );

    let (mut banks_client, payer, _recent_blockhash) = program.start().await;
    let user = Keypair::new();

    let transfer = system_instruction::transfer(&payer.pubkey(), &user.pubkey(), 5_000_000_000);
    process_transaction(&mut banks_client, &payer, vec![transfer], vec![]).await;

    let reserve_mint = create_mint(&mut banks_client, &payer, &payer.pubkey()).await;
    let hatch_config =
        Pubkey::find_program_address(&[b"hatch_config", reserve_mint.as_ref()], &HATCH_PROGRAM_ID)
            .0;
    let hatch_vault =
        Pubkey::find_program_address(&[b"hatch_vault", reserve_mint.as_ref()], &HATCH_PROGRAM_ID).0;

    let merkle_root = merkle_leaf(&user.pubkey(), 100);
    let init_accounts = hatch_accounts::InitializeHatch {
        hatch_config,
        reserve_asset_mint: reserve_mint,
        hatch_vault,
        authority: payer.pubkey(),
        system_program: system_program::ID,
        token_program: spl_token::id(),
        rent: sysvar::rent::ID,
    };
    let init_ix = Instruction {
        program_id: HATCH_PROGRAM_ID,
        accounts: init_accounts.to_account_metas(None),
        data: hatch_instruction::InitializeHatch {
            min_raise: 50,
            max_raise: 200,
            open_slot: 0,
            close_slot: 0,
            merkle_root,
        }
        .data(),
    };
    process_transaction(&mut banks_client, &payer, vec![init_ix], vec![]).await;

    let user_reserve_account =
        create_user_token_account(&mut banks_client, &payer, &user, &reserve_mint).await;
    mint_to_account(
        &mut banks_client,
        &payer,
        &reserve_mint,
        &user_reserve_account,
        &payer,
        80,
    )
    .await;

    let (contribution, _) = Pubkey::find_program_address(
        &[b"contribution", user.pubkey().as_ref()],
        &HATCH_PROGRAM_ID,
    );
    let contribute_accounts = hatch_accounts::Contribute {
        hatch_config,
        contribution,
        hatch_vault,
        user_reserve_token_account: user_reserve_account,
        authority: user.pubkey(),
        system_program: system_program::ID,
        token_program: spl_token::id(),
    };
    let contribute_ix = Instruction {
        program_id: HATCH_PROGRAM_ID,
        accounts: contribute_accounts.to_account_metas(None),
        data: hatch_instruction::Contribute {
            amount: 40,
            allowed_allocation: 100,
            proof: vec![],
        }
        .data(),
    };
    process_transaction(&mut banks_client, &payer, vec![contribute_ix], vec![&user]).await;

    let commons_token_mint = Pubkey::find_program_address(
        &[b"commons_token_mint", hatch_config.as_ref()],
        &HATCH_PROGRAM_ID,
    )
    .0;
    let curve_config = Pubkey::find_program_address(
        &[b"curve_config", commons_token_mint.as_ref()],
        &ABC_PROGRAM_ID,
    )
    .0;
    let reserve_vault = Keypair::new();
    let commons_treasury = Keypair::new();

    let finalize_accounts = hatch_accounts::FinalizeHatch {
        hatch_config,
        reserve_asset_mint: reserve_mint,
        authority: payer.pubkey(),
        curve_config,
        commons_token_mint,
        reserve_vault: reserve_vault.pubkey(),
        commons_treasury: commons_treasury.pubkey(),
        commons_abc_program: ABC_PROGRAM_ID,
        system_program: system_program::ID,
        token_program: spl_token::id(),
        rent: sysvar::rent::ID,
    };
    let finalize_ix = Instruction {
        program_id: HATCH_PROGRAM_ID,
        accounts: finalize_accounts.to_account_metas(None),
        data: hatch_instruction::FinalizeHatch {
            kappa: 1,
            exponent: 1,
            initial_price: 1,
            friction: 0,
        }
        .data(),
    };
    process_transaction(
        &mut banks_client,
        &payer,
        vec![finalize_ix],
        vec![&reserve_vault, &commons_treasury],
    )
    .await;

    let account = banks_client
        .get_account(hatch_config)
        .await
        .unwrap()
        .expect("failed to fetch hatch config");
    let mut data: &[u8] = &account.data;
    let config = HatchConfig::try_deserialize(&mut data).unwrap();
    assert!(!config.finalized);
    assert!(config.failed);

    let refund_accounts = hatch_accounts::Refund {
        hatch_config,
        contribution,
        hatch_vault,
        user_reserve_token_account: user_reserve_account,
        authority: user.pubkey(),
        token_program: spl_token::id(),
    };
    let refund_ix = Instruction {
        program_id: HATCH_PROGRAM_ID,
        accounts: refund_accounts.to_account_metas(None),
        data: hatch_instruction::Refund {}.data(),
    };
    process_transaction(&mut banks_client, &payer, vec![refund_ix], vec![&user]).await;

    let commons_account_data = banks_client
        .get_account(user_reserve_account)
        .await
        .unwrap()
        .expect("reserve account missing");
    let token_state = TokenAccountState::unpack(&commons_account_data.data).unwrap();
    assert_eq!(token_state.amount, 80);

    let account_after_refund = banks_client
        .get_account(hatch_config)
        .await
        .unwrap()
        .expect("failed to fetch hatch config post refund");
    let mut data_after: &[u8] = &account_after_refund.data;
    let config_after = HatchConfig::try_deserialize(&mut data_after).unwrap();
    assert_eq!(config_after.total_refunded, config_after.total_raised);

    let close_accounts = hatch_accounts::CloseHatch {
        hatch_config,
        authority: payer.pubkey(),
    };
    let close_ix = Instruction {
        program_id: HATCH_PROGRAM_ID,
        accounts: close_accounts.to_account_metas(None),
        data: hatch_instruction::CloseHatch {}.data(),
    };
    process_transaction(&mut banks_client, &payer, vec![close_ix], vec![]).await;

    let closed_account = banks_client.get_account(hatch_config).await.unwrap();
    assert!(closed_account.is_none());
}
