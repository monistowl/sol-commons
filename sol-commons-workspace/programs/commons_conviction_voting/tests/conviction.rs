#![cfg(test)]

use anchor_lang::error::AnchorErrorCode;
use commons_conviction_voting::{
    self, accounts as cv_accounts, instruction as cv_instruction, CustomError, ID as CV_ID,
    Proposal, ProposalStatus,
};
use solana_program::{
    pubkey::Pubkey,
    system_instruction,
    sysvar::{self, clock},
};
use solana_program_test::{processor, ProgramTest};
use solana_sdk::{
    instruction::{Instruction, InstructionError},
    signature::Keypair,
    signature::Signer,
    transaction::Transaction,
    transport::TransportError,
};
use spl_associated_token_account::{
    create_associated_token_account, get_associated_token_address, id as associated_token_program_id,
};
use spl_token::{instruction as token_instruction, state::{Account as TokenAccountState, Mint}};
use solana_sdk::account::Account as SolanaAccount;

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

async fn create_token_account(
    banks_client: &mut solana_program_test::BanksClient,
    payer: &Keypair,
    owner: &Pubkey,
    mint: &Pubkey,
) -> Pubkey {
    let ata = get_associated_token_address(owner, mint);
    let create_ix =
        create_associated_token_account(&payer.pubkey(), owner, mint);
    process_transaction(banks_client, payer, vec![create_ix], vec![]).await;
    ata
}

async fn mint_to_account(
    banks_client: &mut solana_program_test::BanksClient,
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

async fn expect_cv_error(
    banks_client: &mut solana_program_test::BanksClient,
    payer: &Keypair,
    instructions: Vec<Instruction>,
    signers: Vec<&Keypair>,
    expected: CustomError,
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
    let err = banks_client.process_transaction(tx).await.unwrap_err();
    match err {
        TransportError::TransactionError(tx_err) => match tx_err {
            TransactionError::InstructionError(_, InstructionError::Custom(code)) => {
                assert_eq!(code, expected as u32);
            }
            _ => panic!("unexpected transaction error: {:?}", tx_err),
        },
        _ => panic!("expected transaction failure, got {:?}", err),
    }
}

async fn expect_anchor_account_constraint_has_one(
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
    let err = banks_client.process_transaction(tx).await.unwrap_err();
    match err {
        TransportError::TransactionError(tx_err) => match tx_err {
            TransactionError::InstructionError(_, InstructionError::Custom(code)) => {
                assert_eq!(code, AnchorErrorCode::AccountConstraintHasOne as u32);
            }
            _ => panic!("unexpected transaction error: {:?}", tx_err),
        },
        _ => panic!("expected transaction failure, got {:?}", err),
    }
}

async fn setup_conviction_env() -> (
    solana_program_test::BanksClient,
    Keypair,
    Pubkey,
    Pubkey,
    Pubkey,
    Pubkey,
) {
    let mut program = ProgramTest::new(
        "commons_conviction_voting",
        CV_ID,
        processor!(commons_conviction_voting::entry),
    );
    let (mut banks_client, payer, _) = program.start().await;

    let commons_token_mint = create_mint(&mut banks_client, &payer, &payer.pubkey()).await;
    let commons_treasury =
        create_token_account(&mut banks_client, &payer, &payer.pubkey(), &commons_token_mint).await;
    let cv_config = Pubkey::find_program_address(&[b"cv_config"], &CV_ID).0;
    let staking_vault =
        Pubkey::find_program_address(&[b"staking_vault", cv_config.as_ref()], &CV_ID).0;

    let init_accounts = cv_accounts::InitializeCvConfig {
        cv_config,
        commons_treasury,
        commons_token_mint,
        staking_vault,
        authority: payer.pubkey(),
        system_program: system_program::ID,
        token_program: spl_token::id(),
        rent: sysvar::rent::ID,
    };
    let init_ix = Instruction {
        program_id: CV_ID,
        accounts: init_accounts.to_account_metas(None),
        data: cv_instruction::InitializeCvConfig {
            decay_rate: 500_000,
            max_ratio: 750_000,
            weight_exponent: 1_000_000,
            min_threshold: 200_000,
        }
        .data(),
    };
    process_transaction(&mut banks_client, &payer, vec![init_ix], vec![]).await;

    (banks_client, payer, commons_token_mint, commons_treasury, cv_config, staking_vault)
}

#[tokio::test]
async fn check_and_execute_requires_threshold() {
    let (mut banks_client, payer, commons_token_mint, commons_treasury, cv_config, staking_vault) =
        setup_conviction_env().await;
    let user = Keypair::new();

    let transfer = system_instruction::transfer(&payer.pubkey(), &user.pubkey(), 5_000_000_000);
    process_transaction(&mut banks_client, &payer, vec![transfer], vec![]).await;

    let commons_token_mint = create_mint(&mut banks_client, &payer, &payer.pubkey()).await;
    let commons_treasury =
        create_token_account(&mut banks_client, &payer, &payer.pubkey(), &commons_token_mint).await;
    let authority_account = Pubkey::find_program_address(&[b"cv_config"], &CV_ID).0;
    let staking_vault = Pubkey::find_program_address(&[b"staking_vault", authority_account.as_ref()], &CV_ID).0;

    let init_accounts = cv_accounts::InitializeCvConfig {
        cv_config: authority_account,
        commons_treasury,
        commons_token_mint,
        staking_vault,
        authority: payer.pubkey(),
        system_program: system_program::ID,
        token_program: spl_token::id(),
        rent: sysvar::rent::ID,
    };
    let init_ix = Instruction {
        program_id: CV_ID,
        accounts: init_accounts.to_account_metas(None),
        data: cv_instruction::InitializeCvConfig {
            decay_rate: 500_000,
            max_ratio: 750_000,
            weight_exponent: 1_000_000,
            min_threshold: 200_000,
        }
        .data(),
    };
    process_transaction(&mut banks_client, &payer, vec![init_ix], vec![]).await;

    let proposal = Pubkey::find_program_address(
        &[b"proposal", payer.pubkey().as_ref(), &200_000u64.to_le_bytes()],
        &CV_ID,
    )
    .0;
    let create_proposal_accounts = cv_accounts::CreateProposal {
        proposal,
        authority: payer.pubkey(),
        system_program: system_program::ID,
        clock: sysvar::clock::ID,
    };
    let create_proposal_ix = Instruction {
        program_id: CV_ID,
        accounts: create_proposal_accounts.to_account_metas(None),
        data: cv_instruction::CreateProposal {
            requested_amount: 200_000,
            metadata_hash: "test".to_string(),
        }
        .data(),
    };
    process_transaction(&mut banks_client, &payer, vec![create_proposal_ix], vec![]).await;

    let user_commons_account =
        create_token_account(&mut banks_client, &payer, &user.pubkey(), &commons_token_mint).await;
    mint_to_account(
        &mut banks_client,
        &payer,
        &commons_token_mint,
        &user_commons_account,
        &payer,
        10_000,
    )
    .await;

    let stake_account = Pubkey::find_program_address(
        &[b"stake", user.pubkey().as_ref(), proposal.as_ref()],
        &CV_ID,
    )
    .0;
    let stake_accounts = cv_accounts::StakeTokens {
        stake_account,
        cv_config,
        proposal,
        commons_token_mint,
        user_commons_token_account: user_commons_account,
        staking_vault,
        authority: user.pubkey(),
        system_program: system_program::ID,
        token_program: spl_token::id(),
        clock: sysvar::clock::ID,
    };
    let stake_ix = Instruction {
        program_id: CV_ID,
        accounts: stake_accounts.to_account_metas(None),
        data: cv_instruction::StakeTokens { amount: 10_000 }.data(),
    };
    process_transaction(&mut banks_client, &user, vec![stake_ix], vec![&user]).await;

    let recipient_token_account =
        create_token_account(&mut banks_client, &payer, &payer.pubkey(), &commons_token_mint).await;
    mint_to_account(
        &mut banks_client,
        &payer,
        &commons_token_mint,
        &commons_treasury,
        &payer,
        200_000,
    )
    .await;

    let check_accounts = cv_accounts::CheckAndExecute {
        cv_config,
        proposal,
        commons_treasury,
        recipient_token_account,
        authority: payer.pubkey(),
        token_program: spl_token::id(),
        clock: sysvar::clock::ID,
    };
    let check_ix = Instruction {
        program_id: CV_ID,
        accounts: check_accounts.to_account_metas(None),
        data: cv_instruction::CheckAndExecute {}.data(),
    };

    expect_cv_error(
        &mut banks_client,
        &payer,
        vec![check_ix],
        vec![&payer],
        CustomError::ThresholdNotReached,
    )
    .await;
}

#[tokio::test]
async fn check_and_execute_transfers_treasury_amount_with_authorized_key() {
    let (mut banks_client, payer, commons_token_mint, commons_treasury, cv_config, staking_vault) =
        setup_conviction_env().await;
    let user = Keypair::new();

    let transfer = system_instruction::transfer(&payer.pubkey(), &user.pubkey(), 5_000_000_000);
    process_transaction(&mut banks_client, &payer, vec![transfer], vec![]).await;

    let proposal = Pubkey::find_program_address(
        &[b"proposal", payer.pubkey().as_ref(), &40_000u64.to_le_bytes()],
        &CV_ID,
    )
    .0;
    let create_proposal_accounts = cv_accounts::CreateProposal {
        proposal,
        authority: payer.pubkey(),
        system_program: system_program::ID,
        clock: sysvar::clock::ID,
    };
    let create_proposal_ix = Instruction {
        program_id: CV_ID,
        accounts: create_proposal_accounts.to_account_metas(None),
        data: cv_instruction::CreateProposal {
            requested_amount: 40_000,
            metadata_hash: "success".to_string(),
        }
        .data(),
    };
    process_transaction(&mut banks_client, &payer, vec![create_proposal_ix], vec![]).await;

    let user_commons_account =
        create_token_account(&mut banks_client, &payer, &user.pubkey(), &commons_token_mint).await;
    mint_to_account(
        &mut banks_client,
        &payer,
        &commons_token_mint,
        &user_commons_account,
        &payer,
        500_000,
    )
    .await;

    let stake_account = Pubkey::find_program_address(
        &[b"stake", user.pubkey().as_ref(), proposal.as_ref()],
        &CV_ID,
    )
    .0;
    let stake_accounts = cv_accounts::StakeTokens {
        stake_account,
        cv_config,
        proposal,
        commons_token_mint,
        user_commons_token_account: user_commons_account,
        staking_vault,
        authority: user.pubkey(),
        system_program: system_program::ID,
        token_program: spl_token::id(),
        clock: sysvar::clock::ID,
    };
    let stake_ix = Instruction {
        program_id: CV_ID,
        accounts: stake_accounts.to_account_metas(None),
        data: cv_instruction::StakeTokens { amount: 400_000 }.data(),
    };
    process_transaction(&mut banks_client, &user, vec![stake_ix], vec![&user]).await;

    let recipient_token_account =
        create_token_account(&mut banks_client, &payer, &payer.pubkey(), &commons_token_mint).await;
    mint_to_account(
        &mut banks_client,
        &payer,
        &commons_token_mint,
        &commons_treasury,
        &payer,
        100_000,
    )
    .await;

    let check_accounts = cv_accounts::CheckAndExecute {
        cv_config,
        proposal,
        commons_treasury,
        recipient_token_account,
        authority: payer.pubkey(),
        token_program: spl_token::id(),
        clock: sysvar::clock::ID,
    };
    let check_ix = Instruction {
        program_id: CV_ID,
        accounts: check_accounts.to_account_metas(None),
        data: cv_instruction::CheckAndExecute {}.data(),
    };

    process_transaction(&mut banks_client, &payer, vec![check_ix], vec![&payer]).await;

    let treasury_account = banks_client
        .get_account(commons_treasury)
        .await
        .unwrap()
        .expect("treasury missing");
    let treasury_state = TokenAccountState::unpack(&treasury_account.data).unwrap();
    assert_eq!(treasury_state.amount, 60_000);

    let recipient_account = banks_client
        .get_account(recipient_token_account)
        .await
        .unwrap()
        .expect("recipient missing");
    let recipient_state = TokenAccountState::unpack(&recipient_account.data).unwrap();
    assert_eq!(recipient_state.amount, 40_000);

    let proposal_account = banks_client
        .get_account(proposal)
        .await
        .unwrap()
        .expect("proposal missing");
    let mut proposal_data: &[u8] = &proposal_account.data;
    let proposal_state = Proposal::try_deserialize(&mut proposal_data).unwrap();
    assert_eq!(proposal_state.status, ProposalStatus::Approved);
}

#[tokio::test]
async fn check_and_execute_requires_authorized_signer() {
    let (mut banks_client, payer, commons_token_mint, commons_treasury, cv_config, staking_vault) =
        setup_conviction_env().await;
    let user = Keypair::new();

    let transfer = system_instruction::transfer(&payer.pubkey(), &user.pubkey(), 5_000_000_000);
    process_transaction(&mut banks_client, &payer, vec![transfer], vec![]).await;

    let proposal = Pubkey::find_program_address(
        &[b"proposal", payer.pubkey().as_ref(), &50_000u64.to_le_bytes()],
        &CV_ID,
    )
    .0;
    let create_proposal_accounts = cv_accounts::CreateProposal {
        proposal,
        authority: payer.pubkey(),
        system_program: system_program::ID,
        clock: sysvar::clock::ID,
    };
    let create_proposal_ix = Instruction {
        program_id: CV_ID,
        accounts: create_proposal_accounts.to_account_metas(None),
        data: cv_instruction::CreateProposal {
            requested_amount: 50_000,
            metadata_hash: "secure".to_string(),
        }
        .data(),
    };
    process_transaction(&mut banks_client, &payer, vec![create_proposal_ix], vec![]).await;

    let user_commons_account =
        create_token_account(&mut banks_client, &payer, &user.pubkey(), &commons_token_mint).await;
    mint_to_account(
        &mut banks_client,
        &payer,
        &commons_token_mint,
        &user_commons_account,
        &payer,
        1_000_000,
    )
    .await;

    let stake_account = Pubkey::find_program_address(
        &[b"stake", user.pubkey().as_ref(), proposal.as_ref()],
        &CV_ID,
    )
    .0;
    let stake_accounts = cv_accounts::StakeTokens {
        stake_account,
        cv_config,
        proposal,
        commons_token_mint,
        user_commons_token_account: user_commons_account,
        staking_vault,
        authority: user.pubkey(),
        system_program: system_program::ID,
        token_program: spl_token::id(),
        clock: sysvar::clock::ID,
    };
    let stake_ix = Instruction {
        program_id: CV_ID,
        accounts: stake_accounts.to_account_metas(None),
        data: cv_instruction::StakeTokens { amount: 1_000_000 }.data(),
    };
    process_transaction(&mut banks_client, &user, vec![stake_ix], vec![&user]).await;

    let recipient_token_account =
        create_token_account(&mut banks_client, &payer, &payer.pubkey(), &commons_token_mint).await;
    mint_to_account(
        &mut banks_client,
        &payer,
        &commons_token_mint,
        &commons_treasury,
        &payer,
        50_000,
    )
    .await;

    let wrong_authority = Keypair::new();
    let check_accounts = cv_accounts::CheckAndExecute {
        cv_config,
        proposal,
        commons_treasury,
        recipient_token_account,
        authority: wrong_authority.pubkey(),
        token_program: spl_token::id(),
        clock: sysvar::clock::ID,
    };
    let check_ix = Instruction {
        program_id: CV_ID,
        accounts: check_accounts.to_account_metas(None),
        data: cv_instruction::CheckAndExecute {}.data(),
    };

    expect_anchor_account_constraint_has_one(
        &mut banks_client,
        &payer,
        vec![check_ix],
        vec![&wrong_authority],
    )
    .await;
}
