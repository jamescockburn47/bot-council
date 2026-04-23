use crate::db::queries_phase1;
use crate::types::Role;
use rand::seq::SliceRandom;
use sqlx::SqlitePool;

/// Assign roles to bots for a debate by uniform random shuffle.
///
/// Historical behaviour included a consecutive-role avoidance guard and a
/// counter-casting bonus. Both were removed: over any meaningful number of
/// debates pure random converges to uniform distribution, and the guard's
/// cost (DB read of `role_history`, up to 100 reshuffles) outweighed its
/// short-run benefit.
///
/// `role_history` is still written by `persist_role_assignments` below for
/// audit purposes but is no longer consulted here.
pub async fn assign_roles(
    _pool: &SqlitePool,
    bot_ids: &[String],
) -> Result<Vec<(String, Role)>, String> {
    if bot_ids.len() > 5 {
        return Err("maximum 5 bots per debate".into());
    }
    let mut roles: Vec<Role> = Role::ALL[..bot_ids.len()].to_vec();
    roles.shuffle(&mut rand::rng());
    Ok(bot_ids.iter().cloned().zip(roles.into_iter()).collect())
}

/// Persist role assignments to `debate_bots` and `role_history` tables.
/// Unchanged from the previous implementation.
pub async fn persist_role_assignments(
    pool: &SqlitePool,
    debate_id: &str,
    assignments: &[(String, Role)],
) -> Result<(), String> {
    for (bot_id, role) in assignments {
        queries_phase1::update_debate_bot_role(pool, debate_id, bot_id, role.as_str())
            .await
            .map_err(|e| format!("db error updating debate_bot role: {e}"))?;
        queries_phase1::insert_role_history(pool, bot_id, debate_id, role.as_str())
            .await
            .map_err(|e| format!("db error inserting role history: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn assign_roles_produces_uniform_distribution() {
        // With 5 bots × 5 roles, over 1000 shuffles each bot gets each role
        // approximately 200 times. Allow ±30% tolerance for statistical noise.
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let bot_ids: Vec<String> = (0..5).map(|i| format!("bot_{i}")).collect();
        let mut counts: HashMap<(String, Role), u32> = HashMap::new();
        for _ in 0..1000 {
            let assignments = assign_roles(&pool, &bot_ids).await.unwrap();
            for (bid, role) in assignments {
                *counts.entry((bid, role)).or_insert(0) += 1;
            }
        }
        for bot in &bot_ids {
            for role in Role::ALL {
                let c = counts.get(&(bot.clone(), role)).copied().unwrap_or(0);
                assert!(
                    (140..=260).contains(&c),
                    "bot {bot} got role {:?} {c} times (expected 200 ± 30%)",
                    role
                );
            }
        }
    }

    #[tokio::test]
    async fn assign_roles_rejects_more_than_five_bots() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let ids: Vec<String> = (0..6).map(|i| format!("b{i}")).collect();
        let err = assign_roles(&pool, &ids).await.unwrap_err();
        assert!(err.contains("maximum 5"));
    }
}
