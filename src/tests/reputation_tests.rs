use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{Addr, Uint128, Timestamp};

use crate::contract::marketplace_reputation::{submit_review, flag_dispute};
use crate::reputation::{ReputationParams, REPUTATION_PARAMS, REVIEWS, USERS};

#[test]
fn test_submit_review() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    let info = mock_info("user1", &[]);

    // Set up reputation parameters
    let params = ReputationParams {
        time_weight_factor: 10,
        volume_weight_factor: 5,
        dispute_penalty: 20,
        inactivity_decay_period: 2592000, // 30 days in seconds
        decay_rate: 95, // 95% retention rate
    };
    REPUTATION_PARAMS.save(deps.as_mut().storage, &params).unwrap();

    // Submit a review
    let res = submit_review(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        "service1".to_string(),
        5,
        "Great service!".to_string(),
        "tx_proof_123".to_string(),
        vec![1, 2, 3], // Mock signature
    ).unwrap();

    // Check response attributes
    assert_eq!(res.attributes.len(), 4);
    assert_eq!(res.attributes[0].key, "action");
    assert_eq!(res.attributes[0].value, "submit_review");

    // Verify review was stored
    let review_id = format!("{}-{}", env.block.time.seconds(), info.sender);
    let stored_review = REVIEWS.load(deps.as_ref().storage, review_id).unwrap();
    assert_eq!(stored_review.rating, 5);
    assert_eq!(stored_review.service, "service1");
    assert_eq!(stored_review.is_disputed, false);

    // Check user reputation was created
    let user_rep = USERS.load(deps.as_ref().storage, &Addr::unchecked("user1")).unwrap();
    assert_eq!(user_rep.total_reviews, 1);
    assert_eq!(user_rep.disputed_reviews, 0);
}

#[test]
fn test_flag_dispute() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    let reviewer = mock_info("user1", &[]);
    let admin = mock_info("admin", &[]);

    // Set up reputation parameters
    let params = ReputationParams {
        time_weight_factor: 10,
        volume_weight_factor: 5,
        dispute_penalty: 20,
        inactivity_decay_period: 2592000,
        decay_rate: 95,
    };
    REPUTATION_PARAMS.save(deps.as_mut().storage, &params).unwrap();

    // Submit a review first
    let review_id = format!("{}-{}", env.block.time.seconds(), reviewer.sender);
    submit_review(
        deps.as_mut(),
        env.clone(),
        reviewer.clone(),
        "service1".to_string(),
        5,
        "Great service!".to_string(),
        "tx_proof_123".to_string(),
        vec![1, 2, 3],
    ).unwrap();

    // Flag the review as disputed
    let res = flag_dispute(
        deps.as_mut(),
        env.clone(),
        admin,
        review_id.clone(),
        "Fake review".to_string(),
    ).unwrap();

    // Check response attributes
    assert_eq!(res.attributes.len(), 4);
    assert_eq!(res.attributes[0].key, "action");
    assert_eq!(res.attributes[0].value, "flag_dispute");

    // Verify review was updated
    let disputed_review = REVIEWS.load(deps.as_ref().storage, review_id).unwrap();
    assert_eq!(disputed_review.is_disputed, true);
    assert_eq!(disputed_review.dispute_reason, Some("Fake review".to_string()));

    // Check user reputation was updated
    let user_rep = USERS.load(deps.as_ref().storage, &reviewer.sender).unwrap();
    assert_eq!(user_rep.disputed_reviews, 1);
}

#[test]
fn test_reputation_calculation() {
    let mut deps = mock_dependencies();
    let mut env = mock_env();
    let info = mock_info("user1", &[]);

    // Set up reputation parameters
    let params = ReputationParams {
        time_weight_factor: 10,
        volume_weight_factor: 5,
        dispute_penalty: 20,
        inactivity_decay_period: 2592000,
        decay_rate: 95,
    };
    REPUTATION_PARAMS.save(deps.as_mut().storage, &params).unwrap();

    // Submit multiple reviews
    for i in 0..3 {
        env.block.time = Timestamp::from_seconds(env.block.time.seconds() + 86400); // Add 1 day
        submit_review(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            format!("service{}", i),
            5,
            "Great service!".to_string(),
            format!("tx_proof_{}", i),
            vec![1, 2, 3],
        ).unwrap();
    }

    // Check final reputation score
    let user_rep = USERS.load(deps.as_ref().storage, &info.sender).unwrap();
    assert_eq!(user_rep.total_reviews, 3);
    assert!(user_rep.reputation_score > Uint128::zero());
}