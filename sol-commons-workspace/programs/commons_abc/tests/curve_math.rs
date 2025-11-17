#![cfg(test)]

use anchor_lang::prelude::*;
use commons_abc::{self, accounts as abc_accounts, instruction as abc_instruction, CurveConfig, ID as ABC_ID};
use commons_abc::test_utils::{compute_fee, minted_tokens_for_deposit, reserve_delta_for_burn, split_with_friction};
use solana_program::program_pack::Pack;
use solana_program_test::{processor, ProgramTest};
use solana_sdk::{instruction::Instruction, pubkey::Pubkey, signature::Keypair, signature::Signer, transaction::Transaction};
use spl_associated_token_account::{
    get_associated_token_address, create_associated_token_account,
    id as associated_token_program_id,
};
use spl_token::{instruction as token_instruction, state::{Account as TokenAccountState, Mint}};
use serde::Deserialize;

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

async fn read_token_balance(
    banks_client: &mut solana_program_test::BanksClient,
    account: Pubkey,
) -> u64 {
    let account_data = banks_client
        .get_account(account)
        .await
        .unwrap()
        .expect("token account missing");
    TokenAccountState::unpack(&account_data.data).unwrap().amount
}

const SIMULATOR_CONFIG_JSON: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../offchain/simulator-pipeline/config.json"));

#[derive(Debug, Deserialize)]
struct CurveScenario {
    name: Option<String>,
    kappa: u64,
    exponent: u64,
    friction: u64,
    deposit: u64,
}

#[derive(Debug, Deserialize)]
struct SimulatorConfig {
    #[serde(rename = "curveScenarios")]
    curve_scenarios: Option<Vec<CurveScenario>>,
}

fn load_curve_scenarios() -> Vec<CurveScenario> {
    serde_json::from_str::<SimulatorConfig>(SIMULATOR_CONFIG_JSON)
        .ok()
        .and_then(|config| config.curve_scenarios)
        .unwrap_or_default()
}

struct ScenarioOutcome {
    minted_amount: u64,
    common_pool_share: u64,
    exit_tribute: u64,
    net_payout: u64,
}

async fn run_curve_round_trip(
    kappa: u64,
    exponent: u64,
    friction: u64,
    deposit_amount: u64,
) -> ScenarioOutcome {
    let mut program = ProgramTest::new(
        "commons_abc",
        ABC_ID,
        processor!(commons_abc::entry),
    );
    let (mut banks_client, payer, _recent_blockhash) = program.start().await;
    let user = Keypair::new();

    let transfer = system_instruction::transfer(&payer.pubkey(), &user.pubkey(), 5_000_000_000);
    process_transaction(&mut banks_client, &payer, vec![transfer], vec![]).await;

    let reserve_mint = create_mint(&mut banks_client, &payer, &payer.pubkey()).await;
    let commons_token_mint = create_mint(&mut banks_client, &payer, &payer.pubkey()).await;

    let (curve_config, curve_config_bump) = Pubkey::find_program_address(
        &[b"curve_config", commons_token_mint.as_ref()],
        &ABC_ID,
    );

    let reserve_vault = Keypair::new();
    let commons_treasury = Keypair::new();

    let init_accounts = abc_accounts::InitializeCurve {
        curve_config,
        commons_token_mint,
        reserve_mint,
        reserve_vault: reserve_vault.pubkey(),
        commons_treasury: commons_treasury.pubkey(),
        authority: payer.pubkey(),
        system_program: system_program::ID,
        token_program: spl_token::id(),
        rent: sysvar::rent::ID,
    };
    let init_ix = Instruction {
        program_id: ABC_ID,
        accounts: init_accounts.to_account_metas(None),
        data: abc_instruction::InitializeCurve {
            kappa,
            exponent,
            initial_price: 1,
            friction,
            initial_reserve: 1_000_000,
            initial_supply: 1_000_000,
        }
        .data(),
    };
    process_transaction(
        &mut banks_client,
        &payer,
        vec![init_ix],
        vec![&reserve_vault, &commons_treasury],
    )
    .await;

    let user_reserve_account = get_associated_token_address(&user.pubkey(), &reserve_mint);
    let create_ata_ix = create_associated_token_account(&payer.pubkey(), &user.pubkey(), &reserve_mint);
    process_transaction(&mut banks_client, &payer, vec![create_ata_ix], vec![]).await;
    mint_to_account(
        &mut banks_client,
        &payer,
        &reserve_mint,
        &user_reserve_account,
        &payer,
        deposit_amount,
    )
    .await;

    let user_commons_account = get_associated_token_address(&user.pubkey(), &commons_token_mint);

    let buy_accounts = abc_accounts::BuyTokens {
        curve_config,
        commons_token_mint,
        reserve_vault: reserve_vault.pubkey(),
        commons_treasury: commons_treasury.pubkey(),
        user_reserve_token_account: user_reserve_account,
        user_commons_token_account: user_commons_account,
        authority: user.pubkey(),
        system_program: system_program::ID,
        token_program: spl_token::id(),
        associated_token_program: associated_token_program_id(),
        rent: sysvar::rent::ID,
    };
    let buy_ix = Instruction {
        program_id: ABC_ID,
        accounts: buy_accounts.to_account_metas(None),
        data: abc_instruction::BuyTokens { amount: deposit_amount }.data(),
    };
    process_transaction(&mut banks_client, &payer, vec![buy_ix], vec![&user]).await;

    let curve_account = banks_client
        .get_account(curve_config)
        .await
        .unwrap()
        .expect("curve config missing");
    let mut curve_data: &[u8] = &curve_account.data;
    let curve_state = CurveConfig::try_deserialize(&mut curve_data).unwrap();
    assert_eq!(curve_state.curve_config_bump, curve_config_bump);

    let (reserve_share, common_pool_share) =
        split_with_friction(deposit_amount, curve_state.friction).unwrap();
    let minted_amount = minted_tokens_for_deposit(0, reserve_share, &curve_state).unwrap();

    let sell_accounts = abc_accounts::SellTokens {
        curve_config,
        commons_token_mint,
        reserve_vault: reserve_vault.pubkey(),
        commons_treasury: commons_treasury.pubkey(),
        user_reserve_token_account: user_reserve_account,
        user_commons_token_account: user_commons_account,
        authority: user.pubkey(),
        system_program: system_program::ID,
        token_program: spl_token::id(),
    };
    let sell_ix = Instruction {
        program_id: ABC_ID,
        accounts: sell_accounts.to_account_metas(None),
        data: abc_instruction::SellTokens { amount: minted_amount }.data(),
    };
    let balance_before_sell = read_token_balance(&mut banks_client, user_reserve_account).await;
    process_transaction(&mut banks_client, &payer, vec![sell_ix], vec![&user]).await;

    let final_balance = read_token_balance(&mut banks_client, user_reserve_account).await;
    let reserve_delta = reserve_delta_for_burn(minted_amount, 0, &curve_state).unwrap();
    let exit_tribute = compute_fee(reserve_delta, curve_state.friction).unwrap();
    let net_payout = reserve_delta - exit_tribute;

    assert_eq!(final_balance, balance_before_sell + net_payout);
    assert_eq!(read_token_balance(&mut banks_client, user_commons_account).await, 0);
    if deposit_amount > 1 {
        assert!(final_treasury >= common_pool_share);
    }

    ScenarioOutcome {
        minted_amount,
        common_pool_share,
        exit_tribute,
        net_payout,
    }
}

#[tokio::test]
async fn buy_and_sell_round_trip() {
    let outcome = run_curve_round_trip(2, 1, 50_000, 1_000_000).await;
    assert!(outcome.minted_amount > 0);
    assert!(outcome.common_pool_share > 0);
    assert!(outcome.net_payout > 0);
}

#[tokio::test]
async fn simulate_multiple_curve_parameters() {
    let scenarios = vec![
        (2, 1, 50_000, 1_000_000),
        (3, 2, 100_000, 2_000_000),
        (4, 2, 150_000, 3_000_000),
    ];
    for (kappa, exponent, friction, deposit) in scenarios {
        let outcome = run_curve_round_trip(kappa, exponent, friction, deposit).await;
        assert!(outcome.minted_amount > 0);
        assert!(outcome.common_pool_share > 0);
        assert!(outcome.net_payout > 0);
    }
}

#[tokio::test]
async fn simulate_configured_curve_scenarios() {
    let scenarios = load_curve_scenarios();
    assert!(
        !scenarios.is_empty(),
        "simulator configuration must define at least one curve scenario"
    );
    for scenario in scenarios {
        let outcome =
            run_curve_round_trip(scenario.kappa, scenario.exponent, scenario.friction, scenario.deposit)
                .await;
        assert!(outcome.minted_amount > 0, "{}", scenario.name.unwrap_or_default());
        assert!(outcome.common_pool_share > 0, "{}", scenario.name.unwrap_or_default());
        assert!(outcome.net_payout > 0, "{}", scenario.name.unwrap_or_default());
    }
}
