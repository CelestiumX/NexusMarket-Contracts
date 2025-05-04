use cosmwasm_std::{Deps, StdResult, Order, Uint128, Timestamp};
use cw_storage_plus::Bound;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::reputation::{Review, UserReputation, REVIEWS, USER_REVIEWS, USERS};

const DEFAULT_LIMIT: u32 = 10;
const MAX_LIMIT: u32 = 30;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ReviewResponse {
    pub id: String,
    pub service: String,
    pub rating: u8,
    pub content: String,
    pub reviewer: String,
    pub timestamp: String,
    pub is_disputed: bool,
    pub dispute_reason: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserReputationResponse {
    pub address: String,
    pub reputation_score: Uint128,
    pub total_reviews: u32,
    pub disputed_reviews: u32,
    pub last_activity: String,
    pub transaction_volume: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ReputationStatsResponse {
    pub total_reviews: u32,
    pub total_users: u32,
    pub average_rating: f64,
    pub disputed_reviews: u32,
}

pub fn query_reviews(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
    min_rating: Option<u8>,
    max_rating: Option<u8>,
    include_disputed: bool,
) -> StdResult<Vec<ReviewResponse>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(Bound::exclusive);

    let reviews: Vec<ReviewResponse> = REVIEWS
        .range(deps.storage, start, None, Order::Ascending)
        .filter(|r| {
            if let Ok((_, review)) = r {
                let rating_in_range = match (min_rating, max_rating) {
                    (Some(min), Some(max)) => review.rating >= min && review.rating <= max,
                    (Some(min), None) => review.rating >= min,
                    (None, Some(max)) => review.rating <= max,
                    (None, None) => true,
                };
                rating_in_range && (include_disputed || !review.is_disputed)
            } else {
                false
            }
        })
        .take(limit)
        .map(|item| {
            let (_, review) = item?;
            Ok(ReviewResponse {
                id: review.id,
                service: review.service,
                rating: review.rating,
                content: review.content,
                reviewer: review.reviewer.to_string(),
                timestamp: review.timestamp.to_string(),
                is_disputed: review.is_disputed,
                dispute_reason: review.dispute_reason,
            })
        })
        .collect::<StdResult<Vec<ReviewResponse>>>()?;

    Ok(reviews)
}

pub fn query_user_reputation(
    deps: Deps,
    user: String,
    start_time: Option<Timestamp>,
    end_time: Option<Timestamp>,
) -> StdResult<UserReputationResponse> {
    let user_addr = deps.api.addr_validate(&user)?;
    let user_rep = USERS.load(deps.storage, &user_addr)?;

    // Filter reviews by time range if specified
    let reviews: Vec<Review> = USER_REVIEWS
        .prefix(&user_addr)
        .range(deps.storage, None, None, Order::Ascending)
        .filter(|r| {
            if let Ok((_, review_id)) = r {
                if let Ok(review) = REVIEWS.load(deps.storage, review_id) {
                    match (start_time, end_time) {
                        (Some(start), Some(end)) => {
                            review.timestamp >= start && review.timestamp <= end
                        }
                        (Some(start), None) => review.timestamp >= start,
                        (None, Some(end)) => review.timestamp <= end,
                        (None, None) => true,
                    }
                } else {
                    false
                }
            } else {
                false
            }
        })
        .map(|item| {
            let (_, review_id) = item?;
            let review_id: String = review_id.to_string();
            REVIEWS.load(deps.storage, review_id)
        })
        .collect::<StdResult<Vec<Review>>>()?;

    Ok(UserReputationResponse {
        address: user_rep.address.to_string(),
        reputation_score: user_rep.reputation_score,
        total_reviews: reviews.len() as u32,
        disputed_reviews: reviews.iter().filter(|r| r.is_disputed).count() as u32,
        last_activity: user_rep.last_activity.to_string(),
        transaction_volume: user_rep.transaction_volume,
    })
}

pub fn query_reputation_stats(deps: Deps) -> StdResult<ReputationStatsResponse> {
    let mut total_reviews = 0u32;
    let mut total_rating = 0u32;
    let mut disputed_reviews = 0u32;

    // Calculate statistics
    let reviews: Vec<Review> = REVIEWS
        .range(deps.storage, None, None, Order::Ascending)
        .map(|item| {
            let (_, review) = item?;
            total_reviews += 1;
            total_rating += review.rating as u32;
            if review.is_disputed {
                disputed_reviews += 1;
            }
            Ok(review)
        })
        .collect::<StdResult<Vec<Review>>>()?;

    let average_rating = if total_reviews > 0 {
        total_rating as f64 / total_reviews as f64
    } else {
        0.0
    };

    let total_users = USERS
        .range(deps.storage, None, None, Order::Ascending)
        .count() as u32;

    Ok(ReputationStatsResponse {
        total_reviews,
        total_users,
        average_rating,
        disputed_reviews,
    })
}