const PSEUDONYMS: &[&str] = &[
    "Agent A", "Agent B", "Agent C", "Agent D", "Agent E", "Agent F", "Agent G", "Agent H",
    "Agent I", "Agent J",
];

/// Assign a pseudonym to a bot based on its index in the debate.
pub fn assign_pseudonym(index: usize) -> String {
    if index < PSEUDONYMS.len() {
        PSEUDONYMS[index].to_string()
    } else {
        format!("Agent {}", index + 1)
    }
}
