// Bot types
export type BotStatus = 'pending' | 'smoke_test_failed' | 'active' | 'rejected' | 'inactive';

export type BotKind = 'external' | 'text_only';

export interface BotResponse {
  id: string;
  name: string;
  endpoint_url: string;
  model_family: string | null;
  status: BotStatus;
  description: string | null;
  submitted_by: string | null;
  rejection_reason: string | null;
  reviewed_at: string | null;
  reviewed_by: string | null;
  created_at: string;
  bot_kind: BotKind;
  /** Free-prose introduction captured during approval smoke (text-only bots only). */
  introduction: string | null;
}

export interface RejectBotRequest {
  reason: string;
}

export interface CreateBotRequest {
  name: string;
  endpoint_url: string;
  token: string;
  model_family?: string;
  description?: string;
  bot_kind?: BotKind;
}

// Debate types
export interface DebateResponse {
  id: string;
  topic: string;
  status: string;
  created_at: string;
  completed_at: string | null;
  /** Soft-delete timestamp; null for live debates. */
  archived_at: string | null;
  bots: DebateBotInfo[];
  results: DebateResults | null;
}

export interface DebateBotInfo {
  bot_id: string;
  bot_name: string;
  pseudonym: string;
  role: string | null;
}

export interface DebateResults {
  responses: AnonymisedResponse[];
  rankings: RankedArgument[];
}

export interface AnonymisedResponse {
  pseudonym: string;
  response: string;
  abstained: boolean;
}

export interface RankedArgument {
  pseudonym: string;
  avg_reasoning_quality: number;
  avg_factual_grounding: number;
  avg_overall: number;
  total_scores: number;
}

export interface CreateDebateRequest {
  topic: string;
  bot_ids?: string[];
  goal_mode?: string;
}

// Transcript types
export interface TranscriptResponse {
  debate_id: string;
  topic: string;
  rounds: TranscriptRound[];
  anonymisation_log: AnonymisationEntry[];
  divergence_analyses: DivergenceEntry[];
  /** The single most-divergent claim selected between R2 and R3 and injected
   *  into every bot's R3 prompt. Absent on debates where crux selection was
   *  skipped (pre-crux debates, or selector failure that fell back to the
   *  legacy cross-examination format). */
  crux?: CruxData;
}

export interface CruxData {
  claim: string;
  source_pseudonym: string;
  source_quote: string;
}

export interface TranscriptRound {
  round_number: number;
  status: string;
  responses: TranscriptEntry[];
}

export interface TranscriptEntry {
  pseudonym: string;
  response: string;
  confidence: number | null;
  challenge: ChallengeData | null;
  position_change: PositionChangeData | null;
  valid: boolean;
  abstained: boolean;
  validation_reasoning: string | null;
  /** Per-field extraction provenance for text-only bots.
   *  Keyed by field name ("challenge" | "position_change" | "steelman").
   *  Absent or null for bots that authored structured fields directly. */
  extraction_metadata: ExtractionMetadata | null;
  /** When populated, this response was carried forward from an earlier
   *  round (typically R0) because the bot failed to respond in the current
   *  round. Null when the bot responded directly. */
  fallback_from_round?: number | null;
  /** Re-dispatch attempts that landed this response. `0` = first attempt
   *  succeeded; `1` = dispatcher fell back to the simplified retry prompt. */
  retry_count: number;
}

export interface ExtractionMetadata {
  challenge?: ExtractionProvenance | null;
  position_change?: ExtractionProvenance | null;
  /** R4-only extraction of the steelman (strongest version of the opposing
   *  argument). `steelman` text is present only when `source === 'extracted'`. */
  steelman?: SteelmanProvenance | null;
}

export interface SteelmanProvenance extends ExtractionProvenance {
  /** The extracted 2–3 sentence steelman text. Null when extraction failed
   *  or was not attempted (e.g. external bot → source = 'authored'). */
  steelman?: string | null;
}

export interface ExtractionProvenance {
  /** 'authored' = bot returned the field directly; 'extracted' = pulled from prose
   *  by MiniMax with source-quote verification; 'extraction_failed' = attempted
   *  but couldn't verify, field left empty. */
  source: 'authored' | 'extracted' | 'extraction_failed';
  /** Verbatim substring of the bot's raw response that supports the extracted value.
   *  Non-null only when source = 'extracted'. */
  quote: string | null;
}

export interface ChallengeData {
  claim_targeted: string;
  counter_evidence: string;
  type: 'factual' | 'logical' | 'premise';
}

export interface PositionChangeData {
  changed: boolean;
  from_summary: string;
  to_summary: string;
  reason: string;
}

export interface AnonymisationEntry {
  pseudonym: string;
  role: string | null;
}

export interface DivergenceEntry {
  pseudonym: string;
  shifted: boolean | null;
  magnitude: string | null;
  what_changed: string | null;
  justification_adequate: boolean | null;
  flags: string[];
}

// Synthesis types
export interface SynthesisResponse {
  debate_id: string;
  synthesis: SynthesisData;
  model_used: string;
  created_at: string;
  citation_check: CitationCheckResult | null;
}

export interface SynthesisData {
  topic: string;
  consensus_points: ConsensusPoint[];
  live_disagreements: Disagreement[];
  flagged_capitulations: Capitulation[];
  minority_positions: MinorityPosition[];
  confidence_trajectories: Record<string, (number | null)[]>;
  meta_observations: string;
}

export interface ConsensusPoint {
  point: string;
  /** 3–6 word label from the synthesiser. Empty string on older rows
   *  that predate the headline prompt; frontend falls back to a
   *  truncation of `point`. */
  headline?: string;
  supporting_bots: string[];
  evidence: string;
}

export interface Disagreement {
  issue: string;
  side_a: DisagreementSide;
  side_b: DisagreementSide;
}

export interface DisagreementSide {
  position: string;
  /** 3–6 word label from the synthesiser. */
  headline?: string;
  bots: string[];
  best_argument: string;
}

export interface Capitulation {
  bot: string;
  from: string;
  to: string;
  justification_adequate: boolean;
  flag_reason: string;
}

export interface MinorityPosition {
  bot: string;
  position: string;
  /** 3–6 word label from the synthesiser. */
  headline?: string;
  key_argument: string;
  confidence: number;
}

export interface CitationCheckResult {
  citations_total: number;
  citations_valid: number;
  citations_invalid: InvalidCitation[];
}

export interface InvalidCitation {
  citation: string;
  reason: string;
  location: string;
}

export interface UserInfoResponse {
  user_id: string;
  role: string;
}

// Admin registry types
export interface AdminEntry {
  user_id: string;
  granted_at: string;
  granted_by: string | null;
}

export interface AddAdminRequest {
  user_id: string;
}

export interface SeenUserEntry {
  user_id: string;
  first_seen_at: string;
  last_seen_at: string;
  is_admin: boolean;
}
