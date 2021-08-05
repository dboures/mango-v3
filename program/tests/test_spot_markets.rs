// Tests related to spot markets
mod program_test;
use program_test::*;
use program_test::cookies::*;
use solana_program::pubkey::Pubkey;
use solana_program_test::*;
use fixed::types::I80F48;

#[tokio::test]
async fn test_list_spot_market_on_serum() {
    // Arrange
    let config = MangoProgramTestConfig::default();
    let mut test = MangoProgramTest::start_new(&config).await;
    // Supress some of the logs
    solana_logger::setup_with_default(
        "solana_rbpf::vm=info,\
             solana_runtime::message_processor=debug,\
             solana_runtime::system_instruction_processor=info,\
             solana_program_test=info",
    );
    // Disable all logs except error
    // solana_logger::setup_with("error");

    let mint_index: usize = 0;
    // Act
    let spot_market_cookie = test.list_spot_market(mint_index).await;
    // Assert
    println!("Serum Market PK: {}", spot_market_cookie.market.to_string());
    // Todo: Figure out how to assert this
}

#[tokio::test]
async fn test_init_spot_markets() {

    // Arrange
    let config = MangoProgramTestConfig::default();
    let mut test = MangoProgramTest::start_new(&config).await;
    // Supress some of the logs
    solana_logger::setup_with_default(
        "solana_rbpf::vm=info,\
             solana_runtime::message_processor=debug,\
             solana_runtime::system_instruction_processor=info,\
             solana_program_test=info",
    );
    // Disable all logs except error
    // solana_logger::setup_with("error");
    let mut mango_group_cookie = MangoGroupCookie::default(&mut test).await;

    // Act
    test.add_oracles_to_mango_group(&mango_group_cookie.address).await;
    mango_group_cookie.add_spot_markets(&mut test, config.num_mints - 1).await;

    // Assert
    // TODO: Figure out how to assert

}

#[tokio::test]
async fn test_place_spot_order() {
    // Arrange
    let config = MangoProgramTestConfig { compute_limit: 200_000, num_users: 2, num_mints: 2 };
    let mut test = MangoProgramTest::start_new(&config).await;
    // Supress some of the logs
    solana_logger::setup_with_default(
        "solana_rbpf::vm=info,\
             solana_runtime::message_processor=debug,\
             solana_runtime::system_instruction_processor=info,\
             solana_program_test=info",
    );
    // Disable all logs except error
    // solana_logger::setup_with("error");

    let mut mango_group_cookie = MangoGroupCookie::default(&mut test).await;
    mango_group_cookie.full_setup(&mut test, config.num_users, config.num_mints - 1).await;

    let user_index: usize = 0;
    let mint_index: usize = 0;
    let base_price = 10000;

    mango_group_cookie.set_oracle(&mut test, mint_index, base_price).await;

    // Act
    // Step 1: Deposit 10_000 USDC into mango account
    mango_group_cookie.run_keeper(&mut test).await;

    let deposit_amount = (base_price * test.quote_mint.unit) as u64;
    test.perform_deposit(
        &mango_group_cookie,
        user_index,
        test.quote_index,
        deposit_amount,
    ).await;

    // Step 2: Place a spot order for BTC
    mango_group_cookie.run_keeper(&mut test).await;

    let starting_spot_order_id = 1000;
    let mut spot_market_cookie = mango_group_cookie.spot_markets[mint_index];
    spot_market_cookie.place_order(
        &mut test,
        &mut mango_group_cookie,
        user_index,
        starting_spot_order_id + mint_index as u64,
        serum_dex::matching::Side::Bid,
        1,
        base_price,
    ).await;

    // Assert
    mango_group_cookie.run_keeper(&mut test).await;

    let mango_account = mango_group_cookie.mango_accounts[user_index].mango_account;
    assert_ne!(mango_account.spot_open_orders[mint_index], Pubkey::default());
    // TODO: More assertions
}

#[tokio::test]
async fn test_match_spot_order() {
    // Arrange
    let config = MangoProgramTestConfig { compute_limit: 200_000, num_users: 2, num_mints: 2 };
    let mut test = MangoProgramTest::start_new(&config).await;
    // Supress some of the logs
    solana_logger::setup_with_default(
        "solana_rbpf::vm=info,\
             solana_runtime::message_processor=debug,\
             solana_runtime::system_instruction_processor=info,\
             solana_program_test=info",
    );

    let mut mango_group_cookie = MangoGroupCookie::default(&mut test).await;
    mango_group_cookie.full_setup(&mut test, config.num_users, config.num_mints - 1).await;

    let bidder_user_index: usize = 0;
    let asker_user_index: usize = 1;
    let mint_index: usize = 0;
    let base_mint = test.with_mint(mint_index);
    let base_price = 10_000;

    mango_group_cookie.set_oracle(&mut test, mint_index, base_price).await;

    // Act
    // Step 1: Make deposits from 2 accounts (Bidder / Asker)
    mango_group_cookie.run_keeper(&mut test).await;

    // Deposit 10_000 USDC as the bidder
    let quote_deposit_amount = (10_000 * test.quote_mint.unit) as u64;
    test.perform_deposit(
        &mango_group_cookie,
        bidder_user_index,
        test.quote_index,
        quote_deposit_amount,
    ).await;

    // Deposit 1 BTC as the asker
    let base_deposit_amount = (1 * base_mint.unit) as u64;
    test.perform_deposit(
        &mango_group_cookie,
        asker_user_index,
        mint_index,
        base_deposit_amount,
    ).await;

    // Step 2: Place a bid for 1 BTC @ 10_000 USDC
    mango_group_cookie.run_keeper(&mut test).await;

    let mut spot_market_cookie = mango_group_cookie.spot_markets[mint_index];
    let starting_spot_order_id = 1000;
    spot_market_cookie.place_order(
        &mut test,
        &mut mango_group_cookie,
        bidder_user_index,
        starting_spot_order_id as u64,
        serum_dex::matching::Side::Bid,
        1,
        base_price,
    ).await;


    // Step 3: Place an ask for 1 BTC @ 10_000 USDC
    mango_group_cookie.run_keeper(&mut test).await;

    spot_market_cookie.place_order(
        &mut test,
        &mut mango_group_cookie,
        asker_user_index,
        starting_spot_order_id + 1 as u64,
        serum_dex::matching::Side::Ask,
        1,
        base_price,
    ).await;

    // Step 4: Consume events
    mango_group_cookie.run_keeper(&mut test).await;

    test.consume_events(
        &spot_market_cookie,
        vec![
            &mango_group_cookie.mango_accounts[bidder_user_index].mango_account.spot_open_orders[0],
            &mango_group_cookie.mango_accounts[asker_user_index].mango_account.spot_open_orders[0],
        ],
        bidder_user_index,
        mint_index,
    ).await;

    // Step 5: Settle funds so that deposits get updated
    mango_group_cookie.run_keeper(&mut test).await;

    // Settling bidder
    spot_market_cookie.settle_funds(
        &mut test,
        &mut mango_group_cookie,
        bidder_user_index,
    ).await;

    // Settling asker
    spot_market_cookie.settle_funds(
        &mut test,
        &mut mango_group_cookie,
        asker_user_index,
    ).await;

    // Assert
    mango_group_cookie.run_keeper(&mut test).await;

    let bidder_base_deposit =
        &mango_group_cookie.mango_accounts[bidder_user_index].mango_account
        .get_native_deposit(&mango_group_cookie.mango_cache.root_bank_cache[mint_index], mint_index).unwrap();
    let asker_base_deposit =
        &mango_group_cookie.mango_accounts[asker_user_index].mango_account
        .get_native_deposit(&mango_group_cookie.mango_cache.root_bank_cache[mint_index], mint_index).unwrap();

    // let bidder_quote_deposit =
    //     &mango_group_cookie.mango_accounts[bidder_user_index].mango_account
    //     .get_native_deposit(&mango_group_cookie.mango_cache.root_bank_cache[QUOTE_INDEX], QUOTE_INDEX).unwrap();
    // let asker_quote_deposit =
    //     &mango_group_cookie.mango_accounts[asker_user_index].mango_account
    //     .get_native_deposit(&mango_group_cookie.mango_cache.root_bank_cache[QUOTE_INDEX], QUOTE_INDEX).unwrap();

    assert_eq!(bidder_base_deposit.to_string(), I80F48::from_num(1000000).to_string());
    assert_eq!(asker_base_deposit.to_string(), I80F48::from_num(0).to_string());

    // TODO: Figure out if the weird quote deposits should be asserted

}
