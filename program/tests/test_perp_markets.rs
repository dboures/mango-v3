mod program_test;
use fixed::types::I80F48;
use mango::matching::{BookSide, OrderType, Side};
use program_test::assertions::*;
use program_test::cookies::*;
use program_test::scenarios::*;
use program_test::*;
use solana_program_test::*;

#[tokio::test]
async fn test_place_perp_order() {
    // === Arrange ===
    let config = MangoProgramTestConfig::default_two_mints();
    let mut test = MangoProgramTest::start_new(&config).await;

    let mut mango_group_cookie = MangoGroupCookie::default(&mut test).await;
    mango_group_cookie.full_setup(&mut test, config.num_users, config.num_mints - 1).await;

    // General parameters
    let user_index: usize = 0;
    let mint_index: usize = 0;
    let base_price: f64 = 10_000.0;
    let base_size: f64 = 1.0;

    // Set oracles
    mango_group_cookie.set_oracle(&mut test, mint_index, base_price).await;

    // Deposit amounts
    let user_deposits = vec![
        (user_index, test.quote_index, base_price * base_size),
        (user_index, mint_index, base_size),
    ];

    // Perp Orders
    let user_perp_orders = vec![
        (user_index, mint_index, Side::Bid, 1.0, base_price),
        (user_index, mint_index, Side::Ask, 1.0, base_price * 2.0),
    ];

    // === Act ===
    // Step 1: Make deposits
    deposit_scenario(&mut test, &mut mango_group_cookie, &user_deposits).await;

    // Step 2: Place perp orders
    place_perp_order_scenario(&mut test, &mut mango_group_cookie, &user_perp_orders).await;

    // === Assert ===
    mango_group_cookie.run_keeper(&mut test).await;

    assert_open_perp_orders(&mango_group_cookie, &user_perp_orders, STARTING_PERP_ORDER_ID);
}

#[tokio::test]
async fn test_match_perp_order() {
    // === Arrange ===
    let config = MangoProgramTestConfig::default_two_mints();
    let mut test = MangoProgramTest::start_new(&config).await;

    let mut mango_group_cookie = MangoGroupCookie::default(&mut test).await;
    mango_group_cookie.full_setup(&mut test, config.num_users, config.num_mints - 1).await;

    // General parameters
    let bidder_user_index: usize = 0;
    let asker_user_index: usize = 1;
    let mint_index: usize = 0;
    let base_price: f64 = 10_000.0;
    let base_size: f64 = 1.0;

    // Set oracles
    mango_group_cookie.set_oracle(&mut test, mint_index, base_price).await;

    // Deposit amounts
    let user_deposits = vec![
        (bidder_user_index, test.quote_index, base_price),
        (asker_user_index, mint_index, 1.0),
    ];

    // Matched Perp Orders
    let matched_perp_orders = vec![vec![
        (asker_user_index, mint_index, mango::matching::Side::Ask, base_size, base_price),
        (bidder_user_index, mint_index, mango::matching::Side::Bid, base_size, base_price),
    ]];

    // === Act ===
    // Step 1: Make deposits
    deposit_scenario(&mut test, &mut mango_group_cookie, &user_deposits).await;

    // Step 2: Place and match spot order
    match_perp_order_scenario(&mut test, &mut mango_group_cookie, &matched_perp_orders).await;

    // Step 3: Settle pnl
    mango_group_cookie.run_keeper(&mut test).await;
    for matched_perp_order in matched_perp_orders {
        mango_group_cookie.settle_perp_funds(&mut test, &matched_perp_order).await;
    }

    // === Assert ===
    mango_group_cookie.run_keeper(&mut test).await;

    // assert_matched_perp_orders(&mango_group_cookie, &user_perp_orders);

    let bidder_base_position = mango_group_cookie.mango_accounts[bidder_user_index]
        .mango_account
        .perp_accounts[mint_index]
        .base_position as f64;
    let bidder_quote_position = mango_group_cookie.mango_accounts[bidder_user_index]
        .mango_account
        .perp_accounts[mint_index]
        .quote_position;
    let asker_base_position =
        mango_group_cookie.mango_accounts[asker_user_index].mango_account.perp_accounts[mint_index]
            .base_position as f64;
    let asker_quote_position =
        mango_group_cookie.mango_accounts[asker_user_index].mango_account.perp_accounts[mint_index]
            .quote_position;

    println!("bidder_base_position: {}", bidder_base_position);
    println!(
        "bidder_quote_position: {}",
        bidder_quote_position.checked_round().unwrap().to_string()
    );
    println!("asker_base_position: {}", asker_base_position);
    println!("asker_quote_position: {}", asker_quote_position.checked_round().unwrap().to_string());

    // assert!(bidder_base_position == base_position);
    // assert!(bidder_quote_position == quote_position);
    // assert!(asker_base_position == -base_position);
    // assert!(asker_quote_position <= quote_position); // TODO Figure this out...
}

#[tokio::test]
async fn test_place_perp_against_expired_orders() {
    // === Arrange ===
    let config = MangoProgramTestConfig {
        // Use intentionally low CU: this test wants to verify the limit is sufficient
        compute_limit: 50_000,
        num_users: 3,
        ..MangoProgramTestConfig::default_two_mints()
    };
    let mut test = MangoProgramTest::start_new(&config).await;

    let mut mango_group_cookie = MangoGroupCookie::default(&mut test).await;
    mango_group_cookie.full_setup(&mut test, config.num_users, config.num_mints - 1).await;

    // General parameters
    let asker_user_index: usize = 2;
    let mint_index: usize = 0;
    let base_price: f64 = 10_000.0;

    // Set oracles
    mango_group_cookie.set_oracle(&mut test, mint_index, base_price).await;

    // Deposit amounts
    let user_deposits = vec![
        (0, test.quote_index, 1000.0 * base_price),
        (1, test.quote_index, 1000.0 * base_price),
        (asker_user_index, mint_index, 1000.0),
    ];

    // === Act ===
    // Step 1: Make deposits
    deposit_scenario(&mut test, &mut mango_group_cookie, &user_deposits).await;

    // Step 2: Place many expiring perp bid orders
    use mango::matching::Side;
    let clock = test.get_clock().await;
    let mut perp_market_cookie = mango_group_cookie.perp_markets[mint_index];
    for bidder_user_index in 0..2 {
        for i in 0..64 {
            perp_market_cookie
                .place_order(
                    &mut test,
                    &mut mango_group_cookie,
                    bidder_user_index,
                    Side::Bid,
                    1.0,
                    (9930 + i) as f64,
                    PlacePerpOptions {
                        expiry_timestamp: Some(clock.unix_timestamp as u64 + 2),
                        ..PlacePerpOptions::default()
                    },
                )
                .await;
        }
    }

    // Step 3: Advance time, so they are expired
    let clock = test.get_clock().await;
    test.advance_clock_past_timestamp(clock.unix_timestamp + 10).await;
    mango_group_cookie.run_keeper(&mut test).await;

    // Step 4: Place an ask that matches against the expired orders
    perp_market_cookie
        .place_order(
            &mut test,
            &mut mango_group_cookie,
            asker_user_index,
            Side::Ask,
            1.0,
            9_950.0,
            PlacePerpOptions::default(),
        )
        .await;
    // TODO: Would be very nice to be able to access compute units, stack use, heap use in the test!

    // deleted three expired bids
    let bids = test.load_account::<BookSide>(perp_market_cookie.bids_pk).await;
    assert_eq!(bids.iter_all_including_invalid().count(), 128 - 5);

    // the new ask landed on the book
    let asks = test.load_account::<BookSide>(perp_market_cookie.asks_pk).await;
    assert_eq!(asks.iter_all_including_invalid().count(), 1);
}

#[tokio::test]
async fn test_perp_matching_limit() {
    // === Arrange ===
    let config = MangoProgramTestConfig {
        // Use intentionally low CU: this test wants to verify the limit is sufficient
        compute_limit: 100_000,
        num_users: 2,
        ..MangoProgramTestConfig::default_two_mints()
    };
    let mut test = MangoProgramTest::start_new(&config).await;

    let mut mango_group_cookie = MangoGroupCookie::default(&mut test).await;
    mango_group_cookie.full_setup(&mut test, config.num_users, config.num_mints - 1).await;

    // General parameters
    let asker_user_index: usize = 0;
    let mint_index: usize = 0;
    let base_price: f64 = 10_000.0;

    // Set oracles
    mango_group_cookie.set_oracle(&mut test, mint_index, base_price).await;

    // Create 8 users who spam 1 lot orders and one regular taker
    let mut user_deposits = vec![(asker_user_index, mint_index, 1000.0)];
    for i in 1..config.num_users {
        user_deposits.push((i, test.quote_index, 1000.0 * base_price))
    }

    // === Act ===
    // Step 1: Make deposits
    deposit_scenario(&mut test, &mut mango_group_cookie, &user_deposits).await;

    // Step 2: Create a lot of small orders on the bid book
    use mango::matching::Side;
    let mut perp_market_cookie = mango_group_cookie.perp_markets[mint_index];
    for bidder_user_index in 1..config.num_users {
        for i in 0..64 {
            perp_market_cookie
                .place_order(
                    &mut test,
                    &mut mango_group_cookie,
                    bidder_user_index,
                    Side::Bid,
                    0.0001 * ((i + 1) as f64),
                    base_price,
                    PlacePerpOptions::default(),
                )
                .await;
        }
    }

    // Step 3: Place an ask that matches against the orders and would consume them all
    perp_market_cookie
        .place_order(
            &mut test,
            &mut mango_group_cookie,
            asker_user_index,
            Side::Ask,
            1.0,
            9_950.0,
            PlacePerpOptions {
                limit: 18, // stays barely below 100k CU
                ..PlacePerpOptions::default()
            },
        )
        .await;
}

#[tokio::test]
async fn test_perp_order_max_quote() {
    // === Arrange ===
    let config = MangoProgramTestConfig { ..MangoProgramTestConfig::default_two_mints() };
    let mut test = MangoProgramTest::start_new(&config).await;

    let mut mango_group_cookie = MangoGroupCookie::default(&mut test).await;
    mango_group_cookie.full_setup(&mut test, config.num_users, config.num_mints - 1).await;

    // General parameters
    let bidder_user_index: usize = 0;
    let asker_user_index: usize = 1;
    let mint_index: usize = 0;
    let base_price: f64 = 10_000.0;
    let mint = test.mints[mint_index];

    // Set oracles
    mango_group_cookie.set_oracle(&mut test, mint_index, base_price).await;

    // === Act ===
    // Step 1: Make deposits
    let user_deposits = vec![
        (bidder_user_index, test.quote_index, 1000.0 * base_price),
        (asker_user_index, mint_index, 1000.0),
    ];
    deposit_scenario(&mut test, &mut mango_group_cookie, &user_deposits).await;

    // Step 2: Setup ask orders
    use mango::matching::Side;
    let mut perp_market_cookie = mango_group_cookie.perp_markets[mint_index];
    perp_market_cookie
        .place_order(
            &mut test,
            &mut mango_group_cookie,
            asker_user_index,
            Side::Ask,
            0.1,
            10_000.0,
            PlacePerpOptions::default(),
        )
        .await;
    perp_market_cookie
        .place_order(
            &mut test,
            &mut mango_group_cookie,
            asker_user_index,
            Side::Ask,
            1.0,
            10_100.0,
            PlacePerpOptions::default(),
        )
        .await;
    perp_market_cookie
        .place_order(
            &mut test,
            &mut mango_group_cookie,
            asker_user_index,
            Side::Ask,
            1.0,
            10_200.0,
            PlacePerpOptions::default(),
        )
        .await;

    // Step 4: Place an immediate order that includes a quote limit
    let max_quote = 0.1 * 10_000.0 + 0.5 * 10_100.0; // first ask order, plus half of the second one
    perp_market_cookie
        .place_order(
            &mut test,
            &mut mango_group_cookie,
            bidder_user_index,
            Side::Bid,
            999999.0, // no max_base_quantity
            90_000.0, // no price limit
            PlacePerpOptions {
                max_quote_size: Some(max_quote),
                order_type: mango::matching::OrderType::ImmediateOrCancel,
                ..PlacePerpOptions::default()
            },
        )
        .await;
    mango_group_cookie.users_with_perp_event[mint_index].push(asker_user_index);
    mango_group_cookie.users_with_perp_event[mint_index].push(bidder_user_index);
    mango_group_cookie.consume_perp_events(&mut test).await;
    // bought 1000 + 5000 MNGO
    let mut expected_mngo = 6000;
    let bidder_account = mango_group_cookie.mango_accounts[bidder_user_index].mango_account;
    assert_eq!(bidder_account.perp_accounts[mint_index].base_position, expected_mngo);
    // cost was 6050 + 1% taker fees
    let mut expected_usdc_base = 1000.0 + 5050.0;
    assert!(
        (bidder_account.perp_accounts[mint_index].quote_position
            + I80F48::from_num(expected_usdc_base * 1000000.0 * 1.01))
        .abs()
            < 1.0
    );

    // Step 5: Place an quote_limit order that ends up partially on the book
    let max_quote = 0.5 * 10_100.0 + 0.7 * 10_150.0; // remaining half of the second one plus some extra
    perp_market_cookie
        .place_order(
            &mut test,
            &mut mango_group_cookie,
            bidder_user_index,
            Side::Bid,
            999999.0, // no max_base_quantity
            10_150.0,
            PlacePerpOptions { max_quote_size: Some(max_quote), ..PlacePerpOptions::default() },
        )
        .await;
    mango_group_cookie.users_with_perp_event[mint_index].push(asker_user_index);
    mango_group_cookie.users_with_perp_event[mint_index].push(bidder_user_index);
    mango_group_cookie.consume_perp_events(&mut test).await;
    // bought 5000 MNGO
    expected_mngo += 5000;
    let bidder_account = mango_group_cookie.mango_accounts[bidder_user_index].mango_account;
    assert_eq!(bidder_account.perp_accounts[mint_index].base_position, expected_mngo);
    // cost was 5050 + 1% taker fees
    expected_usdc_base += 5050.0;
    assert!(
        (bidder_account.perp_accounts[mint_index].quote_position
            + I80F48::from_num(expected_usdc_base * 1000000.0 * 1.01))
        .abs()
            < 1.0
    );
    // the remainder was placed as a bid, as expected
    let bids = test.load_account::<BookSide>(perp_market_cookie.bids_pk).await;
    let top_order = bids.get_max().unwrap();
    assert_eq!(top_order.price(), test.price_number_to_lots(&mint, 10_150.0) as i64);
    assert_eq!(top_order.quantity, test.base_size_number_to_lots(&mint, 0.7) as i64);
}

#[tokio::test]
async fn test_perp_order_types() {
    // === Arrange ===
    let config = MangoProgramTestConfig {
        compute_limit: 100_000,
        num_users: 2,
        num_mints: 3,
        ..MangoProgramTestConfig::default()
    };
    let mut test = MangoProgramTest::start_new(&config).await;

    let mut mango_group_cookie = MangoGroupCookie::default(&mut test).await;
    mango_group_cookie.full_setup(&mut test, config.num_users, config.num_mints - 1).await;

    // General parameters
    let book_user_index: usize = 0;
    let test_user_index: usize = 1;
    let mint_index0: usize = 0;
    let mint_index1: usize = 1;
    let base_price: f64 = 10_000.0;
    let to_lots: i64 = 10000;

    // Set oracles
    mango_group_cookie.set_oracle(&mut test, mint_index0, base_price).await;
    mango_group_cookie.set_oracle(&mut test, mint_index1, base_price).await;

    // Deposit amounts
    let user_deposits = vec![
        (test_user_index, test.quote_index, 1000.0 * base_price),
        (book_user_index, test.quote_index, 1000000.0),
    ];

    // === Act ===
    // Step 1: Make deposits
    deposit_scenario(&mut test, &mut mango_group_cookie, &user_deposits).await;

    for side in [Side::Bid, Side::Ask] {
        let (side_direction, mint) =
            if side == Side::Bid { (-1.0, mint_index0) } else { (1.0, mint_index1) };

        // Step 2: Place a bid and ask on the order book
        use mango::matching::Side;
        let mut perp_market_cookie = mango_group_cookie.perp_markets[mint];
        perp_market_cookie
            .place_order(
                &mut test,
                &mut mango_group_cookie,
                book_user_index,
                Side::Bid,
                10.0,
                base_price - 100.0,
                PlacePerpOptions::default(),
            )
            .await;
        perp_market_cookie
            .place_order(
                &mut test,
                &mut mango_group_cookie,
                book_user_index,
                Side::Ask,
                10.0,
                base_price + 100.0,
                PlacePerpOptions::default(),
            )
            .await;

        // Step 3: Place bids and asks of all order types
        // Ideally there'd be more detailed checks of the results.
        // For now there's just a basic sanity check.

        // fully executes against existing order on book
        perp_market_cookie
            .place_order(
                &mut test,
                &mut mango_group_cookie,
                test_user_index,
                side,
                1.0,
                base_price - side_direction * 150.0,
                PlacePerpOptions {
                    order_type: OrderType::ImmediateOrCancel,
                    ..PlacePerpOptions::default()
                },
            )
            .await;
        assert_eq!(
            mango_group_cookie.mango_accounts[test_user_index].mango_account.perp_accounts[mint]
                .taker_base,
            1 * to_lots * (-side_direction as i64)
        );

        // fully executes against existing order on book
        perp_market_cookie
            .place_order(
                &mut test,
                &mut mango_group_cookie,
                test_user_index,
                side,
                1.0,
                base_price + side_direction * 50.0,
                PlacePerpOptions { order_type: OrderType::Market, ..PlacePerpOptions::default() },
            )
            .await;
        assert_eq!(
            mango_group_cookie.mango_accounts[test_user_index].mango_account.perp_accounts[mint]
                .taker_base,
            2 * to_lots * (-side_direction as i64)
        );

        // places on book
        perp_market_cookie
            .place_order(
                &mut test,
                &mut mango_group_cookie,
                test_user_index,
                side,
                1.0,
                base_price + side_direction * 75.0,
                PlacePerpOptions { order_type: OrderType::PostOnly, ..PlacePerpOptions::default() },
            )
            .await;
        // nothing got taken
        assert_eq!(
            mango_group_cookie.mango_accounts[test_user_index].mango_account.perp_accounts[mint]
                .taker_base,
            2 * to_lots * (-side_direction as i64)
        );

        // places on book, as close to the opposing side as possible
        perp_market_cookie
            .place_order(
                &mut test,
                &mut mango_group_cookie,
                test_user_index,
                side,
                1.0,
                base_price - side_direction * 500.0,
                PlacePerpOptions {
                    order_type: OrderType::PostOnlySlide,
                    ..PlacePerpOptions::default()
                },
            )
            .await;
        // nothing got taken
        assert_eq!(
            mango_group_cookie.mango_accounts[test_user_index].mango_account.perp_accounts[mint]
                .taker_base,
            2 * to_lots * (-side_direction as i64)
        );

        // places deep in the book
        perp_market_cookie
            .place_order(
                &mut test,
                &mut mango_group_cookie,
                test_user_index,
                side,
                1.0,
                base_price + side_direction * 125.0,
                PlacePerpOptions { order_type: OrderType::Limit, ..PlacePerpOptions::default() },
            )
            .await;
        // nothing got taken
        assert_eq!(
            mango_group_cookie.mango_accounts[test_user_index].mango_account.perp_accounts[mint]
                .taker_base,
            2 * to_lots * (-side_direction as i64)
        );
    }
}

#[tokio::test]
async fn test_edit_perp_order_by_client_id() {
    // === Arrange ===
    let config = MangoProgramTestConfig::default_two_mints();
    let mut test = MangoProgramTest::start_new(&config).await;

    let mut mango_group_cookie = MangoGroupCookie::default(&mut test).await;
    mango_group_cookie.full_setup(&mut test, config.num_users, config.num_mints - 1).await;

    // General parameters
    let user_index: usize = 0;
    let mint_index: usize = 0;
    let base_price: f64 = 10_000.0;
    let base_size: f64 = 1.0;
    let mint = test.mints[mint_index];

    // Set oracles
    mango_group_cookie.set_oracle(&mut test, mint_index, base_price).await;

    // Deposit amounts
    let user_deposits = vec![
        (user_index, test.quote_index, base_price * base_size),
        (user_index, mint_index, base_size),
    ];

    // Perp Orders
    let user_perp_orders = vec![(user_index, mint_index, Side::Bid, 1.0, base_price)];

    // === Act ===
    // Step 1: Make deposits
    deposit_scenario(&mut test, &mut mango_group_cookie, &user_deposits).await;

    // Step 2: Place perp orders
    place_perp_order_scenario(&mut test, &mut mango_group_cookie, &user_perp_orders).await;

    // === Assert ===
    mango_group_cookie.run_keeper(&mut test).await;
    assert_open_perp_orders(&mango_group_cookie, &user_perp_orders, STARTING_PERP_ORDER_ID);

    let perp_market_cookie = mango_group_cookie.perp_markets[mint_index];
    let bids = test.load_account::<BookSide>(perp_market_cookie.bids_pk).await;
    let top_order = bids.get_max().unwrap();
    assert_eq!(top_order.price(), test.price_number_to_lots(&mint, base_price) as i64);
    assert_eq!(top_order.quantity, test.base_size_number_to_lots(&mint, base_size) as i64);

    // Step 3: Edit the order

    let updated_perp_orders = vec![(user_index, mint_index, Side::Bid, 1.0, base_price * 0.5)];

    edit_perp_order_by_client_id_scenario(
        &mut test,
        &mut mango_group_cookie,
        &updated_perp_orders,
        STARTING_PERP_ORDER_ID,
        base_size,
    )
    .await;

    // === Assert ===
    mango_group_cookie.run_keeper(&mut test).await;
    assert_open_perp_orders(&mango_group_cookie, &updated_perp_orders, STARTING_PERP_ORDER_ID);

    let updated_bids = test.load_account::<BookSide>(perp_market_cookie.bids_pk).await;
    let updated_top_order = updated_bids.get_max().unwrap();
    assert_eq!(
        updated_top_order.price(),
        test.price_number_to_lots(&mint, base_price * 0.5) as i64
    );
    assert_eq!(updated_top_order.quantity, test.base_size_number_to_lots(&mint, base_size) as i64);
}

#[tokio::test]
async fn test_edit_perp_order_by_client_id_will_adjust_order_size_if_user_passes_stale_order() {
    // === Arrange ===
    let config = MangoProgramTestConfig::default_two_mints();
    let mut test = MangoProgramTest::start_new(&config).await;

    let mut mango_group_cookie = MangoGroupCookie::default(&mut test).await;
    mango_group_cookie.full_setup(&mut test, config.num_users, config.num_mints - 1).await;

    // General parameters
    let bidder_user_index: usize = 0;
    let asker_user_index: usize = 1;
    let mint_index: usize = 0;
    let base_price: f64 = 10_000.0;
    let base_size: f64 = 1.0;
    let mint = test.mints[mint_index];

    // Set oracles
    mango_group_cookie.set_oracle(&mut test, mint_index, base_price).await;

    // Deposit amounts
    let user_deposits = vec![
        (bidder_user_index, test.quote_index, base_price),
        (asker_user_index, mint_index, 1.0),
    ];

    // Matched Perp Orders
    let matched_perp_orders = vec![vec![
        (asker_user_index, mint_index, mango::matching::Side::Ask, base_size * 0.5, base_price),
        (bidder_user_index, mint_index, mango::matching::Side::Bid, base_size, base_price),
    ]];

    // === Act ===
    // Step 1: Make deposits
    deposit_scenario(&mut test, &mut mango_group_cookie, &user_deposits).await;

    // Step 2: Place and match spot order
    match_perp_order_scenario(&mut test, &mut mango_group_cookie, &matched_perp_orders).await;

    // Step 3: Settle pnl
    mango_group_cookie.run_keeper(&mut test).await;
    for matched_perp_order in matched_perp_orders {
        mango_group_cookie.settle_perp_funds(&mut test, &matched_perp_order).await;
    }

    // === Assert ===
    mango_group_cookie.run_keeper(&mut test).await;

    // assert_matched_perp_orders(&mango_group_cookie, &user_perp_orders);

    let perp_market_cookie = mango_group_cookie.perp_markets[mint_index];
    let bids = test.load_account::<BookSide>(perp_market_cookie.bids_pk).await;
    let top_order = bids.get_max().unwrap();
    assert_eq!(top_order.price(), test.price_number_to_lots(&mint, base_price) as i64);
    assert_eq!(top_order.quantity, test.base_size_number_to_lots(&mint, base_size * 0.5) as i64);

    // Step 4: Edit the order
    let updated_perp_orders =
        vec![(bidder_user_index, mint_index, Side::Bid, 1.0, base_price * 0.5)];

    edit_perp_order_by_client_id_scenario(
        &mut test,
        &mut mango_group_cookie,
        &updated_perp_orders,
        STARTING_PERP_ORDER_ID + 1,
        base_size, // client expects to cancel all of the original order
    )
    .await;

    // === Assert ===
    mango_group_cookie.run_keeper(&mut test).await;
    assert_open_perp_orders(&mango_group_cookie, &updated_perp_orders, STARTING_PERP_ORDER_ID + 1);

    let updated_bids = test.load_account::<BookSide>(perp_market_cookie.bids_pk).await;
    let updated_top_order = updated_bids.get_max().unwrap();
    assert_eq!(
        updated_top_order.price(),
        test.price_number_to_lots(&mint, base_price * 0.5) as i64
    );
    assert_eq!(
        updated_top_order.quantity,
        test.base_size_number_to_lots(&mint, base_size * 0.5) as i64
    );
}

#[tokio::test]
async fn test_edit_perp_order() {
    // === Arrange ===
    let config = MangoProgramTestConfig::default_two_mints();
    let mut test = MangoProgramTest::start_new(&config).await;

    let mut mango_group_cookie = MangoGroupCookie::default(&mut test).await;
    mango_group_cookie.full_setup(&mut test, config.num_users, config.num_mints - 1).await;

    // General parameters
    let user_index: usize = 0;
    let mint_index: usize = 0;
    let base_price: f64 = 10_000.0;
    let base_size: f64 = 1.0;
    let mint = test.mints[mint_index];

    // Set oracles
    mango_group_cookie.set_oracle(&mut test, mint_index, base_price).await;

    // Deposit amounts
    let user_deposits = vec![
        (user_index, test.quote_index, base_price * base_size),
        (user_index, mint_index, base_size),
    ];

    // Perp Orders
    let user_perp_orders = vec![(user_index, mint_index, Side::Bid, 1.0, base_price)];

    // === Act ===
    // Step 1: Make deposits
    deposit_scenario(&mut test, &mut mango_group_cookie, &user_deposits).await;

    // Step 2: Place perp orders
    place_perp_order_scenario(&mut test, &mut mango_group_cookie, &user_perp_orders).await;

    // === Assert ===
    mango_group_cookie.run_keeper(&mut test).await;
    assert_open_perp_orders(&mango_group_cookie, &user_perp_orders, STARTING_PERP_ORDER_ID);

    let perp_market_cookie = mango_group_cookie.perp_markets[mint_index];
    let bids = test.load_account::<BookSide>(perp_market_cookie.bids_pk).await;
    let top_order = bids.get_max().unwrap();
    let cancel_order_id = top_order.key;
    assert_eq!(top_order.price(), test.price_number_to_lots(&mint, base_price) as i64);
    assert_eq!(top_order.quantity, test.base_size_number_to_lots(&mint, base_size) as i64);

    // Step 3: Edit the order

    let updated_perp_orders = vec![(user_index, mint_index, Side::Bid, 1.0, base_price * 0.5)];

    edit_perp_order_scenario(
        &mut test,
        &mut mango_group_cookie,
        &updated_perp_orders,
        cancel_order_id,
        base_size,
    )
    .await;

    // === Assert ===
    mango_group_cookie.run_keeper(&mut test).await;
    assert_open_perp_orders(&mango_group_cookie, &updated_perp_orders, STARTING_PERP_ORDER_ID + 1);

    let updated_bids = test.load_account::<BookSide>(perp_market_cookie.bids_pk).await;
    let updated_top_order = updated_bids.get_max().unwrap();
    assert_eq!(
        updated_top_order.price(),
        test.price_number_to_lots(&mint, base_price * 0.5) as i64
    );
    assert_eq!(updated_top_order.quantity, test.base_size_number_to_lots(&mint, base_size) as i64);
}

#[tokio::test]
async fn test_edit_perp_order_will_adjust_order_size_if_user_passes_stale_order() {
    // === Arrange ===
    let config = MangoProgramTestConfig::default_two_mints();
    let mut test = MangoProgramTest::start_new(&config).await;

    let mut mango_group_cookie = MangoGroupCookie::default(&mut test).await;
    mango_group_cookie.full_setup(&mut test, config.num_users, config.num_mints - 1).await;

    // General parameters
    let bidder_user_index: usize = 0;
    let asker_user_index: usize = 1;
    let mint_index: usize = 0;
    let base_price: f64 = 10_000.0;
    let base_size: f64 = 1.0;
    let mint = test.mints[mint_index];

    // Set oracles
    mango_group_cookie.set_oracle(&mut test, mint_index, base_price).await;

    // Deposit amounts
    let user_deposits = vec![
        (bidder_user_index, test.quote_index, base_price),
        (asker_user_index, mint_index, 1.0),
    ];

    // Matched Perp Orders
    let matched_perp_orders = vec![vec![
        (asker_user_index, mint_index, mango::matching::Side::Ask, base_size * 0.5, base_price),
        (bidder_user_index, mint_index, mango::matching::Side::Bid, base_size, base_price),
    ]];

    // === Act ===
    // Step 1: Make deposits
    deposit_scenario(&mut test, &mut mango_group_cookie, &user_deposits).await;

    // Step 2: Place and match spot order
    match_perp_order_scenario(&mut test, &mut mango_group_cookie, &matched_perp_orders).await;

    // Step 3: Settle pnl
    mango_group_cookie.run_keeper(&mut test).await;
    for matched_perp_order in matched_perp_orders {
        mango_group_cookie.settle_perp_funds(&mut test, &matched_perp_order).await;
    }

    // === Assert ===
    mango_group_cookie.run_keeper(&mut test).await;

    // assert_matched_perp_orders(&mango_group_cookie, &user_perp_orders);

    let perp_market_cookie = mango_group_cookie.perp_markets[mint_index];
    let bids = test.load_account::<BookSide>(perp_market_cookie.bids_pk).await;
    let top_order = bids.get_max().unwrap();
    let cancel_order_id = top_order.key;
    assert_eq!(top_order.price(), test.price_number_to_lots(&mint, base_price) as i64);
    assert_eq!(top_order.quantity, test.base_size_number_to_lots(&mint, base_size * 0.5) as i64);

    // Step 4: Edit the order
    let updated_perp_orders =
        vec![(bidder_user_index, mint_index, Side::Bid, 1.0, base_price * 0.5)];

    edit_perp_order_scenario(
        &mut test,
        &mut mango_group_cookie,
        &updated_perp_orders,
        cancel_order_id,
        base_size, // client expects to cancel all of the original order
    )
    .await;

    // === Assert ===
    mango_group_cookie.run_keeper(&mut test).await;
    assert_open_perp_orders(&mango_group_cookie, &updated_perp_orders, STARTING_PERP_ORDER_ID + 2);

    let updated_bids = test.load_account::<BookSide>(perp_market_cookie.bids_pk).await;
    let updated_top_order = updated_bids.get_max().unwrap();
    assert_eq!(
        updated_top_order.price(),
        test.price_number_to_lots(&mint, base_price * 0.5) as i64
    );
    assert_eq!(
        updated_top_order.quantity,
        test.base_size_number_to_lots(&mint, base_size * 0.5) as i64
    );
}
