use crate::db::queries_phase1;
use crate::types::Role;
use rand::seq::SliceRandom;
use sqlx::SqlitePool;

/// Assign roles to bots for a debate, respecting the rotation constraint:
/// no bot gets the same role in consecutive debates.
///
/// Returns a `Vec` of `(bot_id, Role)` pairs. Falls back to a best-effort
/// random shuffle if the constraint cannot be satisfied after 100 attempts
/// (e.g., first debate or pathological case where all roles conflict).
pub async fn assign_roles(
    pool: &SqlitePool,
    bot_ids: &[String],
) -> Result<Vec<(String, Role)>, String> {
    if bot_ids.len() > 5 {
        return Err("maximum 5 bots per debate".into());
    }

    // Fetch each bot's last role from history
    let mut last_roles: Vec<(String, Option<Role>)> = Vec::new();
    for bot_id in bot_ids {
        let last = queries_phase1::get_last_role(pool, bot_id)
            .await
            .map_err(|e| format!("db error fetching last role: {e}"))?;
        let role = last.and_then(|s| Role::from_str(&s));
        last_roles.push((bot_id.clone(), role));
    }

    // Try up to 100 shuffles to find one that avoids consecutive same-role
    let mut roles: Vec<Role> = Role::ALL[..bot_ids.len()].to_vec();
    let mut rng = rand::rng();

    for _ in 0..100 {
        roles.shuffle(&mut rng);
        let conflict = last_roles
            .iter()
            .zip(roles.iter())
            .any(|((_, last), assigned)| last.as_ref() == Some(assigned));
        if !conflict {
            return Ok(bot_ids.iter().cloned().zip(roles.into_iter()).collect());
        }
    }

    // Fallback: accept whatever shuffle we have
    tracing::warn!(
        "role rotation constraint could not be satisfied after 100 attempts, using best-effort"
    );
    roles.shuffle(&mut rng);
    Ok(bot_ids.iter().cloned().zip(roles.into_iter()).collect())
}

/// Persist role assignments to `debate_bots` and `role_history` tables.
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
