# Bot Council Frontend — Design Specification

> v1.0 — 2026-04-15 — Phase 1.5a: Debate Viewer + Admin Panel

## Overview

A standalone SvelteKit + TypeScript SPA that provides a visual interface for the Bot Council. Deployed on Cloudflare Pages, communicating with the Axum backend API exposed via Cloudflare Tunnel. Authenticated via Clerk with two tiers: admin (LQ core members) and member (everyone else).

The frontend's primary purpose is threefold:
1. **Review debate outcomes** — synthesis-first report with full data drill-down
2. **Manage bots** — application flow with admin approval
3. **Configure and launch debates** — topic, bot selection, format options

## Architecture

```
┌─────────────────────┐     ┌──────────────────────┐     ┌─────────────────┐
│  Cloudflare Pages   │     │  Cloudflare Tunnel    │     │   Evo X2        │
│  (SvelteKit SPA)    │────▶│  api.council.domain   │────▶│   Axum :3100    │
│  council.domain     │     │                       │     │   SQLite        │
└─────────────────────┘     └──────────────────────┘     └─────────────────┘
         │
         ▼
   ┌───────────┐
   │   Clerk   │
   │   (Auth)  │
   └───────────┘
```

### Stack

| Component | Choice | Rationale |
|-----------|--------|-----------|
| Framework | SvelteKit + TypeScript | Minimal boilerplate, built-in routing, small bundles, reactive by default |
| Styling | Tailwind CSS | Utility-first, dark theme trivial, no custom CSS architecture needed |
| Auth | Clerk | Managed auth with SvelteKit SDK, two-tier roles, no password infrastructure to build |
| Charts | Chart.js via svelte-chartjs | Confidence trajectory line charts. Lightweight, sufficient for the data complexity. D3 is overkill here. |
| Deployment | Cloudflare Pages (static adapter) | Free tier, automatic GitHub deploys, edge-cached |
| API exposure | Cloudflare Tunnel | Public API subdomain without exposing Evo directly |

### Project Location

Separate directory within the monorepo: `frontend/`. Own `package.json`, own build, own deployment pipeline. Not served from Axum.

```
bot-council/
  frontend/
    src/
      routes/           -- SvelteKit pages
      lib/
        api/            -- API client module
        components/     -- Shared UI components
        stores/         -- Svelte stores
        types/          -- TypeScript types mirroring backend DTOs
      app.html          -- Shell
    static/             -- Static assets
    svelte.config.js
    tailwind.config.js
    package.json
    tsconfig.json
  src/                  -- Existing Rust backend
  ...
```

## Authentication

### Provider: Clerk

Clerk handles signup, login, session management, and role assignment. The SvelteKit app uses Clerk's SvelteKit SDK for route protection and session access.

### Tiers

| Tier | Who | Capabilities |
|------|-----|-------------|
| **Admin** | 5 LQ core members (James, LQ_Alice, Artur, + 2) | Create debates, configure protocol, manage bots (approve/reject/deactivate), view all data, access settings |
| **Member** | Anyone who signs up | Submit bot applications, spectate debates, view transcripts and synthesis, view own bot submission status |
| **Public** | Unauthenticated visitors | View "How It Works" page only |

### Implementation

- Clerk JWT included in all API requests as `Authorization: Bearer <clerk_jwt>`
- Backend verifies Clerk JWT (JWKS endpoint) and extracts role from Clerk metadata
- Role stored as Clerk public metadata: `{ "role": "admin" }` or `{ "role": "member" }`
- Admin role assigned manually by James via Clerk dashboard (5 users, no self-service admin promotion)
- SvelteKit route guards check role client-side for UI gating; backend enforces authz server-side

### Backend Changes Required

- New middleware: Clerk JWT verification (replaces or sits alongside existing bearer token auth)
- `GET /me` endpoint: returns `{ user_id, role, email }` from verified JWT
- Existing bearer token auth preserved for bot-to-harness communication (bots don't use Clerk)

## Pages and Routes

### Navigation

Persistent dark sidebar:
- **Bot Council** wordmark/logo at top
- **Debates** — visible to all authenticated users
- **Bots** — admin sees management view, member sees submission view
- **Settings** — admin only
- **How It Works** — visible to all (including unauthenticated)
- Clerk user widget at bottom (avatar, name, sign out)

### Route Table

| Route | Auth | Tier | Page |
|-------|------|------|------|
| `/` | No | Public | Landing page with login CTA, or redirect to `/debates` if authenticated |
| `/how-it-works` | No | Public | Protocol explainer |
| `/debates` | Yes | Member+ | Debate list |
| `/debates/[id]` | Yes | Member+ | Debate report (synthesis-first) |
| `/debates/new` | Yes | Admin | Create debate form |
| `/bots` | Yes | Admin | Bot management (active/pending/inactive tabs) |
| `/bots/submit` | Yes | Member | Submit bot application |
| `/bots/my-submissions` | Yes | Member | View own submissions and their status |
| `/settings` | Yes | Admin | Protocol config viewer (read-only for 1.5a) |

## Page Designs

### Debate Report (`/debates/[id]`)

The centrepiece page. Synthesis-first layout with full data transparency — every data point the backend stores is exposed, layered for readability.

#### Information Hierarchy

**Layer 1 — Immediately visible:**

- **Header**: debate topic, status badge, debate ID (truncated), created/completed timestamps, agent count, round count
- **Synthesis cards**: four cards in a 2x2 grid
  - Consensus points (green accent): list of agreed points with supporting agents and evidence citations
  - Live disagreements (red accent): issue, side A vs side B with agents and best arguments
  - Flagged capitulations (amber accent): agent, from/to position, justification assessment, flag reason
  - Minority positions (blue accent): agent, position, key argument with citation, confidence score
- **Confidence trajectory chart**: line chart, one line per agent (colour-coded), x-axis = rounds, y-axis = confidence 0-100. Null for Round 0 (no confidence reported).
- **Meta observations**: the synthesis engine's structural commentary on debate quality (max 200 words)

**Layer 2 — One click (expandable sections):**

- **Round transcript**: accordion, one section per round (0-4). Each round expands to show:
  - Round name and status
  - All agent responses for that round, each showing:
    - Pseudonym + role badge (colour-coded)
    - Response text (full, not truncated)
    - Confidence score (Rounds 1-4)
    - Validation status (valid/invalid/abstained)
  - Round 2 responses additionally show:
    - Challenge block: claim targeted, counter-evidence, challenge type (factual/logical/premise), MiniMax validation result
  - Round 3 responses additionally show:
    - Cross-examination pairing indicator (who they were paired with)
    - The question posed and the answer given
  - Round 4 responses additionally show:
    - Position change declaration: changed (yes/no), from summary, to summary, reason
- **Divergence analysis**: expandable section showing per-agent divergence results:
  - Shifted (yes/no), magnitude (none/minor/major/reversal), what changed, justification adequate (yes/no), flags
- **Anonymisation log**: expandable table mapping pseudonym → role. Does NOT reveal bot identity (that's admin-only in the debate metadata, not the transcript)

**Layer 3 — Drill-down (modals or sub-panels):**

- **Synthesis citation links**: clicking a citation like "[Agent A, Round 2]" in a consensus/disagreement point scrolls to and highlights that specific response in the transcript
- **Challenge validation detail**: clicking a challenge block opens a detail panel showing MiniMax's reasoning for valid/invalid determination. Requires backend change: the transcript endpoint must include validation reasoning from the `analyses` table (currently only `valid: bool` is returned). See Backend Changes section.
- **Raw JSON toggle**: "View raw" button at the bottom of each section shows the raw API JSON response. Available for: transcript, synthesis, divergence analysis. For technically inclined users and bot operators debugging their bots' outputs.

#### Visual Design

- Dark background (#0a0a0f), card surfaces (#12121f), borders (#1e1e3a)
- Agent colours are consistent and persistent:
  - Agent A: pink (#f472b6)
  - Agent B: green (#34d399)
  - Agent C: blue (#60a5fa)
  - Agent D: amber (#f59e0b)
  - Agent E: purple (#8b5cf6)
- Role badges: small pill with role name, same colour as agent
- Challenge blocks: coloured left border matching challenge type (red for factual, amber for logical, purple for premise)
- Position change blocks: highlighted callout with arrow icon (from → to)
- Monospace font for agent pseudonyms and round labels; sans-serif for body text

### Debate List (`/debates`)

- Card or table list, sorted by most recent first
- Each entry shows: topic (truncated to ~80 chars), status badge (running/complete/cancelled), agent count, round progress (e.g. "3/5" or "Complete"), created date
- Filter bar: status dropdown (all/running/complete/cancelled), text search on topic
- Click row → navigates to `/debates/[id]`
- Admin: "New Debate" button in header
- Pagination or infinite scroll for long lists

### Create Debate (`/debates/new` — admin only)

- **Topic**: text input, required
- **Bot selection**: checkbox list of active approved bots, showing name and model family. Minimum 3 required (enforced client-side, validated server-side). Select all / deselect all.
- **Goal mode**: radio group. "Adversarial (default)" is selectable. Other modes (Consensus-seeking, Winner-takes-all, Devil's Advocate Stress Test) shown but greyed out with "Coming soon" label.
- **Advanced** (collapsed by default):
  - Round count: displayed as "5 (fixed)" — read-only for 1.5a, note: "Configurable in future release"
  - Role definitions: read-only display of the 5 constitutional roles with descriptions
  - Prompt templates: read-only display of all 5 round prompt templates. Collapsible per round.
- **Launch Debate** button: creates the debate and redirects to `/debates/[id]` (which will show "Running" status)

### Bot Management (`/bots` — admin only)

Three-tab layout:

**Active tab:**
- Table: bot name, endpoint URL, model family, date registered, last debate date
- Row actions: Deactivate button

**Pending Applications tab:**
- Card layout (more detail per item): bot name, endpoint URL, model family, description, submitted by (Clerk username/email), submitted date
- Card actions: Approve / Reject buttons
- Approve: bot moves to Active
- Reject: bot marked rejected, submitter can see status on their submissions page

**Inactive tab:**
- Same columns as Active
- Row actions: Reactivate button

### Submit Bot (`/bots/submit` — member)

- Form fields: bot name, endpoint URL, model family (dropdown: claude, gpt4, llama, minimax, gemini, other), brief description (textarea, max 500 chars)
- Submit button: creates bot in "pending" status
- Success state: "Your bot has been submitted for review. You'll see the status on your submissions page."
- Link to `/bots/my-submissions`

### My Submissions (`/bots/my-submissions` — member)

- List of bots submitted by the current user
- Each shows: name, endpoint, status badge (pending/approved/rejected), submitted date
- No actions — view only

### Settings (`/settings` — admin only)

- Read-only display of current protocol configuration:
  - Round count: 5
  - Roles: table of 5 roles with descriptions and enforcement rules
  - Timeouts: 5 min per bot per round
  - Quorum: minimum 3 bots
  - Synthesis: Opus, temperature 0
  - Max retries: 2 (validation failures)
- Prompt template viewer: all 5 round prompt templates displayed in monospace, collapsible per round
- Banner at top: "Protocol configuration is read-only in this release. Editing will be available in a future update."

### How It Works (`/how-it-works` — public)

Long-form reference page explaining the full protocol. Same dark aesthetic. Sticky side nav with anchor links for section jumping.

**Sections:**

1. **The Protocol** — 5-round structure explained step by step. Visual flow diagram (inline SVG Svelte component, not a static image) showing: Blind Formation → Anonymous Distribution → Structured Rebuttal → Cross-Examination → Final Position → Divergence Analysis → Synthesis. Each step explained: what happens, why, what structural enforcement is applied.

2. **Constitutional Roles** — the 5 roles in a table: role name, function, enforcement mechanism. Plain-language explanation of what each role does and how the harness enforces role-consistent behaviour.

3. **Anti-Sycophancy Mechanisms** — the 6 mechanisms explained:
   - Anchoring prevention (Round 0: empty context, concurrent dispatch)
   - Confidence laundering prevention (Rounds 1-2: identity stripped, stable pseudonyms)
   - Cascade prevention (Round 2: mandatory challenge, MiniMax validation)
   - Capitulation detection (post-Round 4: divergence analysis)
   - False consensus prevention (synthesis schema separates categories)
   - Role enforcement (all rounds: re-prompting on violation)

4. **Analysis & Validation** — how MiniMax is used: challenge validation (what "valid" means), divergence pairing (how cross-examination partners are chosen by maximum semantic divergence), position shift detection (what gets flagged and why).

5. **Synthesis** — how Opus produces the output. The schema explained: consensus points (all must explicitly agree), live disagreements (issue + two sides + best arguments), flagged capitulations (unexplained shifts), minority positions (preserved with dignity). Why temperature 0. Why every claim must cite [Agent, Round].

6. **Anonymisation** — how identity is stripped: bot identity replaced with stable pseudonyms (Agent A-E), maintained across all rounds. What the anonymisation log reveals (pseudonym → role) and what it doesn't (role → bot identity). Why this matters for preventing bias.

7. **Reading a Debate Report** — guided walkthrough of the report page UI. What each section means, how to interpret the confidence chart, what a flagged capitulation indicates, how to use citation links, how to access raw data.

## API Client

### Module: `frontend/src/lib/api/`

Typed API client wrapping `fetch`. All backend DTOs mirrored as TypeScript types in `frontend/src/lib/types/`.

```typescript
// Core client pattern
const api = {
  debates: {
    list(params?: { status?: string; limit?: number }): Promise<DebateResponse[]>,
    get(id: string): Promise<DebateResponse>,
    create(req: CreateDebateRequest): Promise<DebateResponse>,
    transcript(id: string): Promise<TranscriptResponse>,
    synthesis(id: string): Promise<SynthesisResponse>,
  },
  bots: {
    list(): Promise<BotResponse[]>,
    create(req: CreateBotRequest): Promise<BotResponse>,
    approve(id: string): Promise<BotResponse>,
    reject(id: string): Promise<BotResponse>,
    deactivate(id: string): Promise<void>,
    reactivate(id: string): Promise<void>,
    mySubmissions(): Promise<BotResponse[]>,
  },
  me(): Promise<UserInfo>,
};
```

### Authentication Flow

1. Clerk SDK initialises on app load
2. On each API call, Clerk session token is retrieved via `getToken()`
3. Token sent as `Authorization: Bearer <clerk_jwt>`
4. Backend verifies via Clerk JWKS endpoint, extracts `user_id` and `role` from token metadata
5. 401 → redirect to login. 403 → show "insufficient permissions" message.

### Error Handling

- Network errors: toast notification with retry option
- 4xx: display error message from backend JSON response body
- 5xx: generic "Something went wrong" with retry
- Loading states: skeleton screens matching the final layout shape (not spinners)

## Backend Changes (Phase 1.5a)

Minimal changes to the existing Rust backend. These are additive — no existing behaviour is modified.

### 1. CORS Middleware

Add `tower-http` CORS layer to the Axum router. Allow the Cloudflare Pages origin. Configurable via `config/default.toml`:

```toml
[server]
cors_origins = ["https://council.yourdomain.com"]
```

### 2. Clerk JWT Verification

New auth middleware that verifies Clerk JWTs alongside the existing bearer token auth. The two coexist:
- Clerk JWTs: used by the frontend (human users)
- Bearer tokens: used by bots (machine-to-machine)

JWT verification uses Clerk's JWKS endpoint to validate signatures. Role extracted from Clerk public metadata in the JWT claims.

### 3. New Endpoints

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/me` | Clerk JWT | Returns user info: `{ user_id, role, email }` |
| PATCH | `/bots/{id}/approve` | Admin | Move bot from pending to active |
| PATCH | `/bots/{id}/reject` | Admin | Mark bot as rejected |
| PATCH | `/bots/{id}/deactivate` | Admin | Soft-deactivate an active bot |
| PATCH | `/bots/{id}/reactivate` | Admin | Reactivate a deactivated bot |
| GET | `/bots/my-submissions` | Member+ | List bots submitted by the authenticated user |

### 4. Bot Application Flow

New fields on the `bots` table:
- `status TEXT NOT NULL DEFAULT 'pending'` — pending / active / rejected / inactive
- `submitted_by TEXT` — Clerk user ID of the submitter
- `description TEXT` — brief description from the application form
- `reviewed_at TEXT` — timestamp of approval/rejection
- `reviewed_by TEXT` — Clerk user ID of the reviewing admin

The existing `active` boolean is replaced by the `status` field. Migration preserves existing bots as `status = 'active'`.

#### Bot Endpoint Conformance Check

When an admin approves a bot, the harness fires a lightweight smoke test before transitioning to active:
1. `POST /debate` to the bot's endpoint with a minimal Round 0 payload (`session_id: "smoke-test"`, `round: 0`, `role: "proponent"`, `context: []`, `prompt: "Smoke test: respond with any valid JSON."`)
2. Verify the response parses as valid JSON with a `response` field (string)
3. Timeout: 30 seconds (shorter than the 5-minute debate timeout)

If the smoke test fails, the bot stays `pending` and the admin sees the failure reason (timeout, invalid JSON, HTTP error, missing `response` field). The admin can retry or reject.

This prevents malformed bots from entering live debates and stalling or corrupting rounds.

### 4a. Bot Count Cap

Debates are capped at **5 bots maximum** (matching the 5 constitutional roles). The create debate form enforces min 3, max 5. The backend validates this in `POST /debates`. This cap ensures:
- Every bot gets a unique constitutional role (no duplicate or unassigned roles)
- The design token colour palette (5 agent colours) is sufficient
- The confidence trajectory chart remains readable

If a future phase supports variable bot counts, it must simultaneously extend the role system and the colour palette.

### 5. Transcript Response Extension

The `TranscriptEntry` currently returns `valid: bool` but not the validation reasoning. Extend it to include:

```json
{
  "pseudonym": "Agent A",
  "response": "...",
  "valid": true,
  "validation_reasoning": "The challenge targets a specific factual claim...",
  "abstained": false
}
```

The `validation_reasoning` field is populated from the `analyses` table (`result_json`) for Round 2 entries where challenge validation was performed. Null for other rounds. This enables the Layer 3 drill-down into challenge validation detail.

Additionally, include divergence analysis results in the transcript response:

```json
{
  "divergence_analyses": [
    {
      "pseudonym": "Agent A",
      "shifted": true,
      "magnitude": "minor",
      "what_changed": "...",
      "justification_adequate": true,
      "flags": []
    }
  ]
}
```

### 6. Synthesis Citation Validation

After the Opus synthesis call completes, the harness runs a lightweight citation-consistency check:

1. Parse every `[Agent X, Round N]` citation in the synthesis output (consensus_points.evidence, live_disagreements.best_argument, minority_positions.key_argument, flagged_capitulations)
2. For each citation, verify:
   - Agent X exists in the debate's anonymisation log
   - Round N exists (0-4)
   - Agent X actually submitted a response in Round N (not abstained)
3. Produce a validation result:

```json
{
  "citations_total": 12,
  "citations_valid": 11,
  "citations_invalid": [
    { "citation": "[Agent C, Round 3]", "reason": "Agent C abstained in Round 3", "location": "consensus_points[1].evidence" }
  ]
}
```

This is stored alongside the synthesis in the `syntheses` table (new column: `citation_check_json TEXT`). The frontend displays a citation validity indicator on the synthesis section — green if all valid, amber with details if any are invalid.

This is not a blocking check — invalid citations don't prevent the synthesis from being stored or displayed. It's a transparency mechanism: if the synthesis hallucinates a consensus point or misattributes a minority position, the report flags it rather than presenting it as authoritative.

### 7. CreateDebateRequest Extension

Add optional fields (all ignored if absent, preserving backward compatibility):

```json
{
  "topic": "string",
  "bot_ids": ["string"],
  "goal_mode": "adversarial",
  "round_count": 5,
  "role_overrides": {}
}
```

For 1.5a, `goal_mode` is stored but only `"adversarial"` is acted upon. `round_count` is stored but the orchestrator ignores it (always runs 5). These fields exist so the frontend can send them and the backend can persist them for future use.

## Deployment

### Frontend: Cloudflare Pages

- GitHub integration: push to `main` triggers build
- Build command: `cd frontend && npm run build`
- Output directory: `frontend/build`
- Environment variables: `PUBLIC_API_URL`, `PUBLIC_CLERK_PUBLISHABLE_KEY`
- Custom domain: `council.yourdomain.com` (or similar)

### API: Cloudflare Tunnel

- `cloudflared` daemon running on Evo X2
- Tunnel routes `api.council.yourdomain.com` → `localhost:3100`
- TLS terminated at Cloudflare edge
- No ports exposed on Evo X2 directly

### Environment Configuration

```
# Frontend (.env)
PUBLIC_API_URL=https://api.council.yourdomain.com
PUBLIC_CLERK_PUBLISHABLE_KEY=pk_live_...

# Backend (config/default.toml additions)
[server]
cors_origins = ["https://council.yourdomain.com"]

[auth]
clerk_jwks_url = "https://your-clerk-instance.clerk.accounts.dev/.well-known/jwks.json"
clerk_issuer = "https://your-clerk-instance.clerk.accounts.dev"
```

## Not In Phase 1.5a

These are explicitly deferred. The UI is designed to accommodate them, but no implementation work is done:

| Feature | Deferred To | UI Treatment |
|---------|-------------|-------------|
| WebSocket/SSE live streaming | Phase 1.5b | No live indicator; debates refresh on page load |
| Configurable round count | Future backend work | Shown as "5 (fixed)" in create form and settings |
| Goal modes (non-adversarial) | Future backend work | Greyed out in create form with "Coming soon" |
| Editable prompt templates | Future backend work | Read-only display in settings and create form |
| Editable role definitions | Future backend work | Read-only display in settings |
| Judge scores | Phase 2 backend | Not shown in report (backend doesn't produce them yet) |
| Reputation / Elo | Phase 2 backend | Not shown |
| Bot PATCH/DELETE for fields other than status | Future | Only status changes (approve/reject/deactivate/reactivate) |

## Design Tokens

Consistent visual language across all pages:

```
Background:        #0a0a0f
Surface:           #12121f
Border:            #1e1e3a
Text primary:      #e2e8f0
Text secondary:    #94a3b8
Text muted:        #4a4a6a

Agent A:           #f472b6 (pink)
Agent B:           #34d399 (green)
Agent C:           #60a5fa (blue)
Agent D:           #f59e0b (amber)
Agent E:           #8b5cf6 (purple)

Status - running:  #8b5cf6 (purple)
Status - complete: #22c55e (green)
Status - cancelled:#ef4444 (red)

Synthesis:
  Consensus:       #22c55e (green)
  Disagreement:    #ef4444 (red)
  Capitulation:    #f59e0b (amber)
  Minority:        #60a5fa (blue)

Challenge type:
  Factual:         #ef4444 (red)
  Logical:         #f59e0b (amber)
  Premise:         #8b5cf6 (purple)

Font - headings:   monospace (system)
Font - body:       sans-serif (system)
Font - code/data:  monospace (system)
```
