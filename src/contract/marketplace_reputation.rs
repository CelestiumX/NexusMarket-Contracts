use cosmwasm_std::{Deps, DepsMut, Env, MessageInfo, Response, StdResult, StdError, Addr, Uint128};
use crate::reputation::{Review, UserReputation, ReputationParams, REVIEWS, USER_REVIEWS, USERS, REPUTATION_PARAMS};
use crate::reputation::{calculate_reputation_score, verify_transaction_proof, verify_signature};

pub fn submit_review(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    service: String,
    rating: u8,
    content: String,
    transaction_proof: String,
    signature: Vec<u8>,
) -> StdResult<Response> {
    // Validate rating range
    if rating < 1 || rating > 5 {
        return Err(StdError::generic_err("Rating must be between 1 and 5"));
    }

    // Verify transaction proof
    if !verify_transaction_proof(&transaction_proof) {
        return Err(StdError::generic_err("Invalid transaction proof"));
    }

    // Create review ID using timestamp and reviewer
    let review_id = format!("{}-{}", env.block.time.seconds(), info.sender);

    let review = Review {
        id: review_id.clone(),
        service: service.clone(),
        rating,
        content,
        reviewer: info.sender.clone(),
        timestamp: env.block.time,
        transaction_proof,
        signature,
        is_disputed: false,
        dispute_reason: None,
    };

    // Store review
    REVIEWS.save(deps.storage, review_id.clone(), &review)?;
    USER_REVIEWS.save(deps.storage, (&info.sender, review_id.clone()), &review_id)?;

    // Update user reputation
    update_user_reputation(deps, &info.sender, &env)?;

    Ok(Response::new()
        .add_attribute("action", "submit_review")
        .add_attribute("reviewer", info.sender)
        .add_attribute("service", service)
        .add_attribute("review_id", review_id))
}

pub fn flag_dispute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    review_id: String,
    reason: String,
) -> StdResult<Response> {
    let mut review = REVIEWS.load(deps.storage, review_id.clone())?;

    // Only service owner or admin can flag disputes
    // TODO: Add proper authorization check

    review.is_disputed = true;
    review.dispute_reason = Some(reason.clone());
    REVIEWS.save(deps.storage, review_id.clone(), &review)?;

    // Update reputation scores for the reviewer
    update_user_reputation(deps, &review.reviewer, &_env)?;

    Ok(Response::new()
        .add_attribute("action", "flag_dispute")
        .add_attribute("review_id", review_id)
        .add_attribute("flagger", info.sender)
        .add_attribute("reason", reason))
}

fn update_user_reputation(deps: DepsMut, user: &Addr, env: &Env) -> StdResult<()> {
    let mut user_rep = USERS.may_load(deps.storage, user)?.unwrap_or(UserReputation {
        address: user.clone(),
        reputation_score: Uint128::zero(),
        total_reviews: 0,
        disputed_reviews: 0,
        last_activity: env.block.time,
        transaction_volume: Uint128::zero(),
    });

    let params = REPUTATION_PARAMS.load(deps.storage)?;
    
    // Count total and disputed reviews
    let reviews: Vec<Review> = USER_REVIEWS
        .prefix(user)
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .map(|item| {
            let (_, review_id) = item?;
            REVIEWS.load(deps.storage, review_id)
        })
        .collect::<StdResult<Vec<Review>>>()?;

    user_rep.total_reviews = reviews.len() as u32;
    user_rep.disputed_reviews = reviews.iter().filter(|r| r.is_disputed).count() as u32;
    user_rep.last_activity = env.block.time;

    // Calculate new reputation score
    user_rep.reputation_score = calculate_reputation_score(&user_rep, &params, env.block.time)?;

    USERS.save(deps.storage, user, &user_rep)?;

    Ok(())
}