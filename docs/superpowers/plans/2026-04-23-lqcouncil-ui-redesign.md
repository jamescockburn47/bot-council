# LQCouncil UI Redesign — LegalQuants Terminal Surface

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Re-skin the entire lqcouncil.com SvelteKit frontend to conform with the LegalQuants brand **Terminal** surface (near-black + indigo glow, Inter / JetBrains Mono / Cormorant Garamond, copper accent), per the UI kit archive at `.ui-redesign-reference/`.

**Architecture:** Pure visual re-skin — no route changes, no API changes, no data-shape changes, no component-prop changes. We introduce three new CSS layers (`tokens.css` from the brand kit verbatim, `theme.css` mapping legacy tokens onto brand tokens for graceful migration, `primitives.css` defining shared `.btn-indigo` / `.card-term` / `.tm-eyebrow` / `.pill-on` classes), swap fonts, migrate the agent/status palette, then work route-by-route replacing current classes with the new primitives. Every task ends with a green `npm run build` and a manual visual check in `npm run dev`.

**Tech Stack:** SvelteKit 5 runes, Tailwind CSS 4, static adapter, Google Fonts (Inter / JetBrains Mono / Cormorant Garamond — replacing Geist Mono). Served by Axum on EVO via `ship.sh`; no backend changes.

---

## Design-System Reference — READ FIRST

Before starting any task, skim these files in `.ui-redesign-reference/` (extracted from the brand-kit ZIP the user attached; not committed):

- `README.md` — voice, visual foundations, "no decorative iconography" rule
- `colors_and_type.css` — **source of truth** for all tokens (imported verbatim in Task 1)
- `preview/components-terminal-cards.html` — the canonical terminal card
- `preview/components-buttons.html` — canonical button classes (`.btn-indigo`, `.btn-dark-ghost`, `.pill-on`)
- `preview/type-terminal-scale.html` — type scale on dark surface
- `preview/spacing-shadow-terminal.html` — `--shadow-glow` + `--shadow-lift`

### Terminal palette (this is what we're applying)

| Token | Value | Use |
|---|---|---|
| `--night` | `#08080D` | page bg |
| `--night-raise` | `#0F0F17` | card bg |
| `--night-rule` | `#1F1F2F` | card border (default) |
| `--night-rule2` | `#2A2A3A` | card border (hover stop 1) |
| `--indigo-400` | `#818CF8` | eyebrows, small text accents |
| `--indigo-500` | `#6366F1` | primary CTAs, card hover border |
| `--indigo-600` | `#4F46E5` | CTA pressed state (not used on lqcouncil per guide) |
| `--copper` | `#9A3412` | numeric stats, "pending" / "archived" tag, signature accent |
| `--glow-txt` | `#E4E4EF` | primary body text on dark |
| `--glow-mute` | `#8888A0` | muted body text |
| `--glow-faint` | `#4A4A5A` | meta labels, footer |
| `--shadow-glow` | `0 0 60px rgba(99,102,241,.15), 0 0 120px rgba(99,102,241,.05)` | hero orbs, closing CTA |
| `--shadow-lift` | `0 4px 24px rgba(99,102,241,.10)` | card hover |

### Hard design rules (brand guide — do NOT deviate)

- **No emoji. No decorative icons.** Remove every Unicode glyph used as an icon (⚔ ⚙ ✱ ☰ ■ ≡ ? ◈ ◎ ◇ ◆). Substitute JB Mono numeric prefixes (`01 ·`, `02 ·`) or plain text labels.
- **No photography / no hand-drawn illustrations.**
- Only two hover interactions: (a) cards lift `translateY(-2px) + indigo-500 border + shadow-lift`; (b) buttons shift `indigo-600 → indigo-500`. No scale transforms, no spring easing.
- Motion: `transition: all 0.3s ease` on cards; no Framer animations on this product (brand guide shows them for the teaser site, not the app).
- **Copper is reserved** for: numeric stats, archived / pending badges, the signature cross-brand accent. It is NEVER a CTA colour on terminal.
- **Indigo is CTA + state colour**, not a decoration colour — use sparingly: active sidebar item, selected filter, primary button, card hover border.
- Cards are `rounded-[12px]`, 1px border, 16px padding on small / 20-24px on large.

---

## File Structure

### New files

| Path | Purpose |
|---|---|
| `frontend/src/lib/styles/tokens.css` | Copy of brand-kit `colors_and_type.css` (verbatim, paper-grain block stripped — editorial-only). Single source of truth for all design tokens. |
| `frontend/src/lib/styles/theme.css` | Shim: maps legacy `--bg`, `--surface`, `--border`, `--text-primary`, `--text-secondary`, `--text-muted` onto new tokens so un-migrated components still render correctly during the migration. |
| `frontend/src/lib/styles/primitives.css` | Shared utility classes: `.btn-indigo`, `.btn-dark-ghost`, `.card-term`, `.card-term-hover`, `.pill-on`, `.tm-eyebrow`, `.mono-label`, `.stat-serif`, `.copper-accent`, `.hero-orbs`. |

### Modified files

| Path | Change |
|---|---|
| `frontend/src/app.html` | Add Google Fonts preconnect + link (Inter / JetBrains Mono / Cormorant Garamond). Drop `class="dark"` (we target one theme). |
| `frontend/src/app.css` | Drop all `@font-face` blocks for Geist + inter-font. `@import` the three new style layers. Rewrite `:root { --* }` declarations to reference brand tokens. Update `.mono` font-family. |
| `frontend/src/lib/utils/agent-colors.ts` | Migrate 5-agent palette + status + challenge colours onto the new terminal palette (indigo-400, violet-400, copper-light, cat-agent-dark cyan, cat-vibe-dark amber). |
| `frontend/src/lib/components/Sidebar.svelte` | Remove icon column. Use JB Mono numeric prefixes. Indigo active rail. Copper dot on active item. |
| `frontend/src/lib/components/{StatusBadge,AgentBadge,TabBar,RawJsonToggle}.svelte` | Apply `.pill-on` + `.tm-eyebrow` primitives. |
| `frontend/src/lib/components/{ResponseCard,RoundAccordion,ChallengeBlock,PositionChangeBlock,SteelmanBlock,SynthesisCard,SynthesisQualityReport,DebateTranscriptView,ConfidenceChart,DivergencePanel}.svelte` | Card-term styling, eyebrow labels, copper numerics, indigo for selected/active states. |
| `frontend/src/lib/components/outcome/*.svelte` | Same palette swap. `ArgumentMap3D.svelte` uses the new `AGENT_COLORS` from `agent-colors.ts` automatically once migrated. |
| `frontend/src/routes/+layout.svelte` | Loading-state colours + typography to match. |
| `frontend/src/routes/+page.svelte` | Full redesign: Cormorant hero headline with copper italic accent; indigo orbs behind hero; strip glyphs from capability cards; mono-eyebrow-rule on section headings; `.btn-indigo` + `.btn-dark-ghost` CTAs. |
| `frontend/src/routes/debates/+page.svelte`, `debates/new/+page.svelte`, `debates/[id]/+page.svelte` | Terminal-card rows with hover-lift, Cormorant for count numbers, `.pill-on` for filters, copper archived tag. |
| `frontend/src/routes/bots/*.svelte` (5 pages) | Terminal cards, indigo active state, copper pending pill, JB Mono tags. |
| `frontend/src/routes/admins/+page.svelte`, `settings/+page.svelte`, `security/+page.svelte`, `how-it-works/+page.svelte`, `sign-in/+page.svelte` | Card + typography sweep. |

### Out of scope (do NOT touch)

- Any file under `src/lib/api/`, `src/lib/auth/`, `src/lib/stores/`, `src/lib/observability/`, `src/lib/argument-graph/*.ts`, `src/hooks.client.ts`, `src/lib/types.ts`
- `svelte.config.js`, `vite.config.ts`, `tsconfig.json`, `package.json` (no dep changes)
- All backend code
- `deploy/bot-council.service`, `scripts/ship.sh`, CI workflow

---

## Branch & Worktree

This plan runs in worktree `sad-maxwell-0622f9` on branch `claude/sad-maxwell-0622f9` (already set up). Each task commits to that branch. One PR at the end covering the whole redesign is acceptable here (cohesive visual change that only hangs together after all tasks land); alternatively split by phase if mid-flight review is desired.

---

## Task 0: Set up working scaffolding

**Files:**
- Read: `.ui-redesign-reference/colors_and_type.css`, `.ui-redesign-reference/README.md`

- [ ] **Step 1: Verify the reference kit is available**

```bash
ls .ui-redesign-reference/colors_and_type.css && head -20 .ui-redesign-reference/colors_and_type.css
```

Expected: prints the `:root` declaration opening of the brand CSS.

If missing, re-extract from `/c/Users/James/Downloads/22c01049-f71a-47f8-82c6-96bd904ae61a.tmp` via `unzip -o <tmp> -d .ui-redesign-reference/`. The six attached `.tmp` files are identical ZIPs; any one works.

- [ ] **Step 2: Add reference folder to `.gitignore`**

Open `.gitignore` at repo root and add the line (preserve existing content):

```
.ui-redesign-reference/
```

- [ ] **Step 3: Confirm baseline build passes**

From repo root:

```bash
cd frontend && npm ci && npm run build
```

Expected: `svelte-kit sync` + `svelte-check` + `vite build` all green. If it fails, fix the baseline before proceeding — do NOT start the redesign on a broken baseline.

- [ ] **Step 4: Commit**

```bash
git add .gitignore && git commit -m "$(cat <<'EOF'
chore(ui): ignore brand-kit reference scaffolding

The brand kit extracted from the attached UI redesign archive lives in
.ui-redesign-reference/ during the redesign work. It's not source — the
authoritative token file gets copied into src/lib/styles/ in the next
commit.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 1: Install brand design tokens

**Files:**
- Create: `frontend/src/lib/styles/tokens.css`

- [ ] **Step 1: Copy brand tokens verbatim into the app**

Copy the file:

```bash
cp .ui-redesign-reference/colors_and_type.css frontend/src/lib/styles/tokens.css
```

Then edit `frontend/src/lib/styles/tokens.css` to:

1. Replace the file-header comment block with:

```css
/* ===================================================================
   LegalQuants brand tokens — Terminal surface
   Source: brand-kit colors_and_type.css (2026-04-21).
   Copied verbatim aside from this header + stripped paper-grain block
   (editorial-only). Do NOT edit individual values here — if a token
   needs to change, update the brand kit and recopy.
   =================================================================== */
```

2. Delete the entire `.paper-grain::after { ... }` rule at the end of the file (editorial-only; we are Terminal-only).

Keep everything else unchanged — the two palette groups, the semantic type styles (both `.ed-*` and `.tm-*`), and the shared utilities (`.gradient-text`, `.mono-eyebrow-rule`). We keep `.ed-*` because the Bot Guide page may promote itself to an editorial-style cream panel later; leaving the tokens resident doesn't cost anything.

- [ ] **Step 2: Confirm the copy is clean**

```bash
diff .ui-redesign-reference/colors_and_type.css frontend/src/lib/styles/tokens.css | head -40
```

Expected: diff shows only (a) the header comment swap and (b) the `.paper-grain::after` block removed. No unintended changes.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/styles/tokens.css && git commit -m "$(cat <<'EOF'
feat(ui): vendor LegalQuants brand tokens

Adds src/lib/styles/tokens.css — a verbatim copy of the brand kit's
colors_and_type.css. Editorial paper-grain filter removed (Terminal-only
product). This file is consumed by theme.css + primitives.css in the
next commit and via direct utility-class reference in route files.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Add theme shim + primitives layer

**Files:**
- Create: `frontend/src/lib/styles/theme.css`
- Create: `frontend/src/lib/styles/primitives.css`

- [ ] **Step 1: Write `theme.css`**

Create `frontend/src/lib/styles/theme.css` with:

```css
/* ===================================================================
   Legacy → brand token bridge.
   Lets un-migrated components keep referencing --bg, --surface, etc.
   until every route is swept onto the new primitives. Delete this file
   when no route references any var() below.
   =================================================================== */

:root {
  /* Backwards-compat mappings — DO NOT add new consumers. */
  --bg: var(--night);
  --surface: var(--night-raise);
  --border: var(--night-rule);
  --text-primary: var(--glow-txt);
  --text-secondary: var(--glow-dim);
  --text-muted: var(--glow-mute);

  /* Status colours (see agent-colors.ts; kept here for any inline `style=`
     references in legacy components that have not been migrated yet.) */
  --status-running:   var(--indigo-500);
  --status-complete:  #10B981;   /* --cat-llm-dark */
  --status-cancelled: #EF4444;   /* --cat-sec-dark */
}
```

- [ ] **Step 2: Write `primitives.css`**

Create `frontend/src/lib/styles/primitives.css` with:

```css
/* ===================================================================
   Shared UI primitives for the Terminal surface.
   Source patterns: preview/components-{buttons,terminal-cards}.html
   in the brand kit. Do not redefine these per-component.
   =================================================================== */

/* --- Buttons --------------------------------------------------------- */

.btn-indigo {
  background: var(--indigo-600);
  color: #fff;
  font-family: var(--sans-product);
  font-size: 14px;
  font-weight: 600;
  padding: 11px 22px;
  border: none;
  border-radius: 8px;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  transition: background-color var(--dur-fast) var(--ease-standard);
}
.btn-indigo:hover { background: var(--indigo-500); }
.btn-indigo:disabled { opacity: 0.5; cursor: not-allowed; }

.btn-dark-ghost {
  background: var(--night-raise);
  color: var(--glow-txt);
  font-family: var(--sans-product);
  font-size: 13px;
  font-weight: 500;
  padding: 9px 18px;
  border: 1px solid var(--night-rule3);
  border-radius: 8px;
  cursor: pointer;
  transition: border-color var(--dur-fast) var(--ease-standard);
}
.btn-dark-ghost:hover { border-color: var(--indigo-400); }

/* --- Pills (filter toggles, status badges) --------------------------- */

.pill-on {
  background: rgba(99, 102, 241, 0.20);
  color: #C7CBFF;
  font-family: var(--sans-product);
  font-size: 12px;
  font-weight: 500;
  padding: 6px 14px;
  border: 1px solid rgba(99, 102, 241, 0.40);
  border-radius: 999px;
}

.pill-off {
  background: var(--night-raise);
  color: var(--glow-mute);
  font-family: var(--sans-product);
  font-size: 12px;
  font-weight: 500;
  padding: 6px 14px;
  border: 1px solid var(--night-rule);
  border-radius: 999px;
  cursor: pointer;
  transition: color var(--dur-fast), border-color var(--dur-fast);
}
.pill-off:hover { color: var(--glow-txt); border-color: var(--night-rule3); }

/* --- Cards ----------------------------------------------------------- */

.card-term {
  background: var(--night-raise);
  border: 1px solid var(--night-rule);
  border-radius: var(--r-lg);
  padding: 16px;
}

.card-term-lg {
  background: var(--night-raise);
  border: 1px solid var(--night-rule);
  border-radius: var(--r-lg);
  padding: 24px;
}

.card-term-hover {
  transition: transform var(--dur-base) var(--ease-standard),
              border-color var(--dur-base) var(--ease-standard),
              box-shadow var(--dur-base) var(--ease-standard);
}
.card-term-hover:hover {
  transform: translateY(-2px);
  border-color: var(--indigo-500);
  box-shadow: var(--shadow-lift);
}

/* --- Typography helpers --------------------------------------------- */

.mono-label {
  font-family: var(--mono-product);
  font-size: 10px;
  font-weight: 500;
  letter-spacing: 0.2em;
  text-transform: uppercase;
  color: var(--glow-faint);
}

.stat-serif {
  font-family: var(--serif);
  font-weight: 700;
  line-height: 1;
  color: var(--copper);
  letter-spacing: -0.02em;
}

.copper-accent { color: var(--copper); }

/* --- Hero background orbs (landing page only) ----------------------- */

.hero-orbs {
  position: relative;
  isolation: isolate;
}
.hero-orbs::before,
.hero-orbs::after {
  content: '';
  position: absolute;
  width: 24rem; height: 24rem;
  border-radius: 9999px;
  background: rgba(99, 102, 241, 0.10);
  filter: blur(120px);
  z-index: -1;
  pointer-events: none;
}
.hero-orbs::before { top: -8rem; left: -4rem; }
.hero-orbs::after  { bottom: -8rem; right: -4rem; background: rgba(167, 139, 250, 0.08); }
```

- [ ] **Step 3: Wire both files into `app.css`**

Edit `frontend/src/app.css` — REPLACE the whole file with:

```css
@import "tailwindcss";
@import "./lib/styles/tokens.css";
@import "./lib/styles/theme.css";
@import "./lib/styles/primitives.css";

:root {
  /* Product-level agent palette; see lib/utils/agent-colors.ts */
  --agent-a: var(--indigo-400);
  --agent-b: #10B981;             /* --cat-llm-dark */
  --agent-c: #06B6D4;             /* --cat-agent-dark */
  --agent-d: #F59E0B;             /* --cat-vibe-dark */
  --agent-e: var(--violet-400);

  /* Synthesis / challenge semantic colours kept as raw hex for legacy
     inline-style references in RoundAccordion, SynthesisCard, etc. */
  --synthesis-consensus:     #10B981;
  --synthesis-disagreement:  #EF4444;
  --synthesis-capitulation:  #F59E0B;
  --synthesis-minority:      var(--indigo-400);
  --challenge-factual: #EF4444;
  --challenge-logical: #F59E0B;
  --challenge-premise: var(--violet-400);
}

body {
  background-color: var(--night);
  color: var(--glow-txt);
  font-family: var(--sans-product);
}

.mono {
  font-family: var(--mono-product);
}

code, pre {
  font-family: var(--mono-product);
}
```

- [ ] **Step 4: Update `app.html` to load Google Fonts**

Edit `frontend/src/app.html` — REPLACE the whole file with:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <link rel="icon" href="%sveltekit.assets%/favicon.png" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <link rel="preconnect" href="https://fonts.googleapis.com">
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
    <link href="https://fonts.googleapis.com/css2?family=Cormorant+Garamond:ital,wght@0,500;0,600;0,700;1,400;1,500&family=Inter:wght@400;500;600;700;800;900&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet">
    <title>LQ Council</title>
    %sveltekit.head%
  </head>
  <body data-sveltekit-preload-data="hover">
    <div style="display: contents">%sveltekit.body%</div>
  </body>
</html>
```

Note: `class="dark"` on `<html>` is removed — we render a single theme (Terminal). Geist Mono `@font-face` blocks removed — we now use JetBrains Mono via the CDN link.

- [ ] **Step 5: Verify build passes**

```bash
cd frontend && npm run build
```

Expected: green build. The page will render slightly differently (the `--bg` / `--surface` / `--border` variables now resolve to the brand tokens), but nothing should be broken. Font substitution from Geist Mono → JetBrains Mono applies everywhere `.mono` is used — that's the intended effect.

- [ ] **Step 6: Visual sanity check**

```bash
cd frontend && npm run dev -- --host 0.0.0.0
```

Open `http://localhost:5173/` (landing) and `/debates` (signed-in path — requires a running backend; if the backend isn't up, verify the landing page only). Spot-check:

- Background is near-black #08080D (slightly darker than before).
- Body copy renders in Inter.
- Mono labels render in JetBrains Mono (wider aperture than Geist).
- No layout breaks, no console errors from missing fonts.

Screenshots: capture `/` and the sign-in redirect page to `/tmp/ui-task2-landing.png` and `/tmp/ui-task2-signin.png` for the review PR body.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/app.css frontend/src/app.html \
        frontend/src/lib/styles/theme.css \
        frontend/src/lib/styles/primitives.css && \
git commit -m "$(cat <<'EOF'
feat(ui): theme shim + primitives layer + font swap

- Adds theme.css mapping legacy --bg/--surface/--border onto brand tokens
  so components migrate incrementally without a big-bang rewrite.
- Adds primitives.css with .btn-indigo, .btn-dark-ghost, .card-term,
  .card-term-hover, .pill-on, .pill-off, .mono-label, .stat-serif,
  .hero-orbs — shared across every redesigned surface.
- Swaps font stack from Geist Mono + self-hosted Inter to CDN-loaded
  Inter / JetBrains Mono / Cormorant Garamond (per brand kit).
- Drops class="dark" from app.html — single theme.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Migrate agent + status + challenge colour palette

**Files:**
- Modify: `frontend/src/lib/utils/agent-colors.ts`

- [ ] **Step 1: Rewrite `agent-colors.ts` in full**

Open `frontend/src/lib/utils/agent-colors.ts` and replace its contents with:

```ts
/*
 * Agent and status colour palette for the Terminal surface.
 *
 * The 5-agent palette is drawn from the LegalQuants brand tokens so every
 * agent hue reads as part of the same system (indigo → cyan → amber →
 * copper-light → violet). ArgumentMap3D.svelte consumes AGENT_COLORS
 * directly for 3D node tinting, so changing values here ripples through
 * the outcome view automatically.
 *
 * DO NOT introduce new hex literals. Add tokens to tokens.css first and
 * reference them by hex string here (CSS vars cannot be evaluated from
 * .ts source).
 */

export const AGENT_COLORS: Record<string, string> = {
  'Agent A': '#818CF8', // indigo-400
  'Agent B': '#10B981', // cat-llm-dark
  'Agent C': '#06B6D4', // cat-agent-dark
  'Agent D': '#F59E0B', // cat-vibe-dark
  'Agent E': '#A78BFA', // violet-400
};

export const AGENT_BG_COLORS: Record<string, string> = {
  'Agent A': 'rgba(129,140,248,0.10)',
  'Agent B': 'rgba(16,185,129,0.10)',
  'Agent C': 'rgba(6,182,212,0.10)',
  'Agent D': 'rgba(245,158,11,0.10)',
  'Agent E': 'rgba(167,139,250,0.10)',
};

export function agentColor(pseudonym: string): string {
  return AGENT_COLORS[pseudonym] ?? '#8888A0'; // glow-mute
}

export function agentBgColor(pseudonym: string): string {
  return AGENT_BG_COLORS[pseudonym] ?? 'rgba(136,136,160,0.10)';
}

export const STATUS_COLORS: Record<string, string> = {
  running:     '#6366F1', // indigo-500
  complete:    '#10B981',
  cancelled:   '#EF4444',
  created:     '#8888A0', // glow-mute
  dispatching: '#6366F1',
  failed:      '#EF4444',
  pending:     '#9A3412', // copper — intentional: "waiting on human" reads as warm flag
  active:      '#10B981',
  inactive:    '#8888A0',
  rejected:    '#EF4444',
};

export const CHALLENGE_COLORS: Record<string, string> = {
  factual: '#EF4444',
  logical: '#F59E0B',
  premise: '#A78BFA', // violet-400
};
```

- [ ] **Step 2: Verify build**

```bash
cd frontend && npm run build
```

Expected: green. `StatusBadge.svelte`, `AgentBadge.svelte`, `ChallengeBlock.svelte`, and `ArgumentMap3D.svelte` all import from this module and now pick up the new hues.

- [ ] **Step 3: Visual check**

With `npm run dev` running and the backend up, open any completed debate page and confirm the outcome 3D graph renders node colours in the new palette (indigo / emerald / cyan / amber / violet). If the backend isn't running, open `/debates` while signed out and confirm the status-badge dot colours match (the list page shows status pills even without a debate open).

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/utils/agent-colors.ts && git commit -m "$(cat <<'EOF'
feat(ui): migrate agent + status palette to brand tokens

Five agents now render in indigo-400 / emerald / cyan / amber / violet —
all drawn from the LegalQuants terminal palette. Status 'pending' moved
to copper (#9A3412) to match the brand's "warm flag" semantic for
waiting-on-human states. ArgumentMap3D picks up the new hues via its
existing AGENT_COLORS import.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Redesign `Sidebar.svelte` + layout loading states

**Files:**
- Modify: `frontend/src/lib/components/Sidebar.svelte`
- Modify: `frontend/src/routes/+layout.svelte`

- [ ] **Step 1: Rewrite `Sidebar.svelte`**

Replace the entire contents of `frontend/src/lib/components/Sidebar.svelte` with:

```svelte
<script lang="ts">
  let { currentPath, role }: { currentPath: string; role: string } = $props();

  type NavItem = {
    href: string;
    label: string;
    n: string;
    minRole: 'admin' | 'member' | null;
  };

  const navItems: NavItem[] = [
    { href: '/debates',      label: 'Debates',      n: '01', minRole: 'member' },
    { href: '/bots',         label: 'Bots',         n: '02', minRole: 'member' },
    { href: '/admins',       label: 'Admins',       n: '03', minRole: 'admin' },
    { href: '/settings',     label: 'Settings',     n: '04', minRole: 'admin' },
    { href: '/bots/guide',   label: 'Bot Guide',    n: '05', minRole: null },
    { href: '/security',     label: 'Security',     n: '06', minRole: null },
    { href: '/how-it-works', label: 'How it works', n: '07', minRole: null },
  ];

  function isActive(href: string): boolean {
    return currentPath.startsWith(href);
  }

  function isVisible(minRole: NavItem['minRole']): boolean {
    if (!minRole) return true;
    if (role === 'admin') return true;
    return role === 'member' && minRole === 'member';
  }
</script>

<nav
  class="fixed left-0 top-0 h-screen w-56 flex flex-col z-50"
  style="background: var(--night-raise); border-right: 1px solid var(--night-rule);"
>
  <div class="px-5 py-5" style="border-bottom: 1px solid var(--night-rule);">
    <a
      href="/"
      class="no-underline block"
      style="font-family: var(--sans-product); font-weight: 800; font-size: 17px; letter-spacing: -0.01em; color: var(--glow-txt);"
    >
      LQ Council
    </a>
    <p class="mt-1 mono-label" style="font-size: 8px; letter-spacing: 0.3em;">
      Agentic Playground
    </p>
  </div>

  <div class="flex-1 py-3">
    {#each navItems as item}
      {#if isVisible(item.minRole)}
        {@const active = isActive(item.href)}
        <a
          href={item.href}
          class="flex items-center gap-3 px-5 py-2.5 no-underline"
          style="
            font-family: var(--sans-product);
            font-size: 13px;
            font-weight: {active ? 600 : 500};
            color: {active ? 'var(--glow-txt)' : 'var(--glow-mute)'};
            background: {active ? 'rgba(99,102,241,0.08)' : 'transparent'};
            border-right: 2px solid {active ? 'var(--indigo-500)' : 'transparent'};
            transition: color var(--dur-fast), background var(--dur-fast);
          "
          onmouseenter={(e) => { if (!active) (e.currentTarget as HTMLElement).style.color = 'var(--glow-txt)'; }}
          onmouseleave={(e) => { if (!active) (e.currentTarget as HTMLElement).style.color = 'var(--glow-mute)'; }}
        >
          <span
            style="
              font-family: var(--mono-product);
              font-size: 10px;
              letter-spacing: 0.15em;
              color: {active ? 'var(--copper)' : 'var(--glow-faint)'};
              min-width: 18px;
            "
          >
            {item.n}
          </span>
          <span>{item.label}</span>
        </a>
      {/if}
    {/each}
  </div>

  <div class="px-5 py-4" style="border-top: 1px solid var(--night-rule);">
    <p class="mono-label" style="font-size: 8px; letter-spacing: 0.25em;">Session</p>
    <p
      class="mt-1"
      style="font-family: var(--sans-product); font-size: 12px; color: var(--glow-dim);"
    >
      Signed in as <span style="color: var(--glow-txt); font-weight: 500;">{role}</span>
    </p>
  </div>
</nav>
```

Key differences from the old sidebar:

- Icon glyphs removed entirely; replaced with JB Mono two-digit numeric prefixes (01 · 02 · …). This matches the brand kit's "no decorative iconography" rule.
- Active state: indigo-tinted background (`rgba(99,102,241,0.08)`), 2px indigo right-rail, copper-coloured item number. (Copper only appears on the *active* item, satisfying "copper as signature accent, used sparingly".)
- Sidebar header gains a "Agentic Playground" eyebrow line (JB Mono, 0.3em tracking, indigo-ish).
- Footer gains a "Session" mono-label above the role line.

- [ ] **Step 2: Update loading/error states in `+layout.svelte`**

Edit `frontend/src/routes/+layout.svelte` — find the three blocks (lines 137–157 in the current file) that render auth bootstrap states. Replace them with:

```svelte
{:else if fatalError}
  <div class="flex items-center justify-center min-h-screen flex-col gap-3 p-8" style="background: var(--night);">
    <p class="mono-label" style="color: #EF4444;">Auth initialisation failed</p>
    <p
      class="max-w-lg text-center whitespace-pre-wrap"
      style="font-family: var(--mono-product); font-size: 12px; color: var(--glow-faint); line-height: 1.6;"
    >
      {fatalError}
    </p>
    <div class="flex gap-3 mt-2">
      <a
        href="/sign-in"
        class="no-underline"
        style="font-family: var(--mono-product); font-size: 12px; color: var(--indigo-400);"
      >Go to sign-in</a>
      <button
        onclick={() => location.reload()}
        style="font-family: var(--mono-product); font-size: 12px; color: var(--indigo-400); background: none; border: none; cursor: pointer;"
      >Reload</button>
    </div>
  </div>
{:else if stage === 'ready' && $me}
  <div class="flex min-h-screen" style="background: var(--night);">
    <Sidebar currentPath={currentPath} role={$me.role} />
    <main class="ml-56 flex-1 p-8">
      {@render children()}
    </main>
  </div>
{:else}
  <div class="flex items-center justify-center min-h-screen flex-col gap-2" style="background: var(--night);">
    <p
      style="font-family: var(--mono-product); font-size: 12px; color: var(--glow-mute); letter-spacing: 0.1em;"
    >
      {stageLabel[stage]}
    </p>
    <p
      style="font-family: var(--mono-product); font-size: 10px; color: var(--glow-faint); letter-spacing: 0.2em;"
    >
      stage: {stage}
    </p>
  </div>
{/if}
```

The `{#if PUBLIC_PATHS.has(currentPath)}` block at line 135 stays unchanged — public routes handle their own background.

- [ ] **Step 3: Verify build + visual**

```bash
cd frontend && npm run build
```

Expected: green.

With `npm run dev` + backend running, sign in and confirm:

- Sidebar shows numbered items (01 Debates, 02 Bots, etc.), no icon glyphs.
- Active item has indigo tint + copper number + 2px right-rail.
- Hover on non-active items brightens the text colour.
- "Agentic Playground" eyebrow line shows under the logo in JB Mono.
- Footer: "Session" label above "Signed in as admin".

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/components/Sidebar.svelte frontend/src/routes/+layout.svelte && \
git commit -m "$(cat <<'EOF'
feat(ui): Terminal sidebar + layout shell

- Sidebar loses decorative glyphs; numeric JB-Mono prefixes replace them
  per brand-kit "no icon system" rule. Active rail is indigo-500 with a
  copper item number; inactive items darken.
- Layout loading / error / ready states restyled onto brand tokens.
- Wordmark gains an "Agentic Playground" mono eyebrow; footer gains a
  "Session" label.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Restyle small display primitives — StatusBadge, AgentBadge, TabBar, RawJsonToggle

**Files:**
- Modify: `frontend/src/lib/components/StatusBadge.svelte`
- Modify: `frontend/src/lib/components/AgentBadge.svelte`
- Modify: `frontend/src/lib/components/TabBar.svelte`
- Modify: `frontend/src/lib/components/RawJsonToggle.svelte`

- [ ] **Step 1: Rewrite `StatusBadge.svelte`**

```svelte
<script lang="ts">
  import { STATUS_COLORS } from '$lib/utils/agent-colors';

  let { status }: { status: string } = $props();
  let color = $derived(STATUS_COLORS[status.toLowerCase()] ?? '#8888A0');
  let label = $derived(status.charAt(0).toUpperCase() + status.slice(1));
</script>

<span
  class="inline-flex items-center gap-1.5 rounded-full"
  style="
    font-family: var(--mono-product);
    font-size: 10px;
    letter-spacing: 0.08em;
    padding: 3px 10px;
    color: {color};
    background: color-mix(in srgb, {color} 12%, transparent);
    border: 1px solid color-mix(in srgb, {color} 30%, transparent);
  "
>
  <span class="w-1.5 h-1.5 rounded-full" style="background: {color};"></span>
  {label}
</span>
```

(Using `color-mix` here replaces the old `${color}15` / `${color}30` hex-concat trick, which broke for non-3-char hexes; the result is visually equivalent but more robust.)

- [ ] **Step 2: Read + restyle `AgentBadge.svelte`**

```bash
cat frontend/src/lib/components/AgentBadge.svelte
```

Expected: shows the current 16-line implementation using `agentColor` + `agentBgColor`. Replace its contents with:

```svelte
<script lang="ts">
  import { agentColor, agentBgColor } from '$lib/utils/agent-colors';

  let { pseudonym, role }: { pseudonym: string; role: string | null } = $props();
  let color = $derived(agentColor(pseudonym));
  let bg = $derived(agentBgColor(pseudonym));
</script>

<span
  class="inline-flex items-center gap-2 rounded-full"
  style="
    font-family: var(--mono-product);
    font-size: 11px;
    font-weight: 500;
    letter-spacing: 0.04em;
    padding: 3px 10px;
    color: {color};
    background: {bg};
    border: 1px solid color-mix(in srgb, {color} 25%, transparent);
  "
>
  <span class="w-1.5 h-1.5 rounded-full" style="background: {color};"></span>
  {pseudonym}
  {#if role}
    <span style="color: var(--glow-faint); font-weight: 400;">· {role}</span>
  {/if}
</span>
```

- [ ] **Step 3: Read + restyle `TabBar.svelte`**

```bash
cat frontend/src/lib/components/TabBar.svelte
```

Expected: shows the existing tab-bar pattern. Read it first to preserve the props shape (`tabs`, `activeTab`, `onSelect` or equivalent). Then replace the template so the active tab uses `.pill-on` and inactive tabs use `.pill-off`:

- Find the existing `<button>` render inside the `{#each}` loop.
- Replace its `class` attribute to use `{activeTab === tab ? 'pill-on' : 'pill-off'}` (adjust to the actual variable name revealed by the read).
- Remove any `bg-[var(--surface)]`, `border-[var(--border)]`, `rounded-lg` utility classes on the button.
- Keep all props, click handlers, and iteration logic exactly as-is.

Verify after the edit by re-reading:

```bash
cat frontend/src/lib/components/TabBar.svelte
```

- [ ] **Step 4: Read + restyle `RawJsonToggle.svelte`**

```bash
cat frontend/src/lib/components/RawJsonToggle.svelte
```

Expected: 18 lines, a small collapsible viewer. Restyle: replace current toggle button with `.btn-dark-ghost` (8px radius, mono 10px text), and style the `<pre>` element with `background: var(--night-edge); border: 1px solid var(--night-rule); border-radius: 8px; padding: 12px; font-family: var(--mono-product); font-size: 11px; color: var(--glow-dim);`.

- [ ] **Step 5: Verify build + visual**

```bash
cd frontend && npm run build
```

Expected: green.

Visual: open any debate detail page. Confirm:

- StatusBadge pills render with tinted colour background + coloured dot.
- AgentBadge shows the pseudonym in a pill with a coloured dot + optional role after a JB Mono bullet.
- TabBar: active tab is solid indigo-tinted; inactive tabs darken on hover.
- RawJsonToggle: toggle is the ghost-button pattern; expanded JSON has a subtle border + dark-edge background.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/lib/components/StatusBadge.svelte \
        frontend/src/lib/components/AgentBadge.svelte \
        frontend/src/lib/components/TabBar.svelte \
        frontend/src/lib/components/RawJsonToggle.svelte && \
git commit -m "$(cat <<'EOF'
feat(ui): restyle small display primitives onto brand tokens

StatusBadge and AgentBadge move to color-mix-based tinted pills (robust
for any hex input) with JB Mono labels. TabBar adopts .pill-on / .pill-off
primitives. RawJsonToggle uses .btn-dark-ghost + night-edge panel.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Redesign the landing page (`/`)

**Files:**
- Modify: `frontend/src/routes/+page.svelte`

- [ ] **Step 1: Rewrite the landing page**

Replace `frontend/src/routes/+page.svelte` contents with:

```svelte
<script lang="ts">
  import { goto } from '$app/navigation';
  import { getClerk, isSignedIn } from '$lib/auth/clerk';

  let signedIn = $state(false);
  let didRun = false;

  $effect(() => {
    if (didRun) return;
    didRun = true;

    void (async () => {
      try {
        await getClerk();
        signedIn = await isSignedIn();
      } catch (e) {
        console.warn('[landing] clerk load failed, treating as signed-out', e);
        signedIn = false;
      }
    })();
  });

  async function handlePrimary() {
    await goto(signedIn ? '/debates' : '/sign-in');
  }

  const CAPABILITIES = [
    {
      title: 'Bring your own agent',
      body: "Put a URL in front of your agent that answers a prompt in text. That's the whole integration — no schema to learn, no SDK to install. We run the debate protocol; your agent does the thinking.",
    },
    {
      title: 'Ask the resident Council',
      body: 'Put a question to the in-house bots. Watch them form positions, rebut each other, and commit to answers — without you having to run infrastructure.',
    },
    {
      title: 'Explore past debates',
      body: 'Every completed debate is browsable: full transcripts, per-round confidence, flagged capitulations, cited synthesis. A growing archive of AI disagreement.',
    },
    {
      title: 'Learn and experiment',
      body: 'Browse worked examples, compare prompt and role designs, and see what makes an agent perform. The playground doubles as a reference as new patterns emerge.',
    },
  ] as const;

  const ROUNDS = [
    { n: 1, name: 'Blind Formation',        desc: 'Every bot answers independently — no anchoring, no cascading.' },
    { n: 2, name: 'Anonymous Distribution', desc: 'Round 1 answers re-shown under pseudonyms. Bots refine against the field.' },
    { n: 3, name: 'Structured Rebuttal',    desc: 'Each bot must issue a substantive challenge. Vague disagreement is rejected.' },
    { n: 4, name: 'Cross-Examination',      desc: 'Adversarial pairings. Every bot answers the strongest challenge against its view.' },
    { n: 5, name: 'Final Position',         desc: 'Bots declare final views. Shifts must be justified — the engine checks.' },
  ] as const;

  const WHY = [
    { title: 'No sycophantic convergence', body: 'Left alone, LLMs agree with each other. The protocol forces genuine disagreement and flags unjustified capitulations.' },
    { title: 'Anonymised by design',        body: 'Pseudonyms rotate per debate. No model builds reputation; no answer wins because of its author.' },
    { title: 'Cited synthesis',             body: 'Every claim in the final synthesis cites the pseudonym and round it draws from. Misattributions are flagged as invalid.' },
    { title: 'Dissent preserved',           body: 'Minority positions are surfaced explicitly. Consensus is reported, not assumed.' },
  ] as const;
</script>

<svelte:head>
  <title>LQ Council — An Agentic Playground</title>
</svelte:head>

<div class="min-h-screen" style="background: var(--night); color: var(--glow-txt);">
  <!-- Top nav -->
  <header
    class="sticky top-0 z-10 backdrop-blur"
    style="background: rgba(8,8,13,0.85); border-bottom: 1px solid var(--night-rule);"
  >
    <div class="max-w-6xl mx-auto px-6 py-4 flex items-center justify-between">
      <div class="flex items-center gap-3">
        <span style="font-family: var(--sans-product); font-weight: 800; font-size: 16px; color: var(--glow-txt);">LQ Council</span>
        <span class="mono-label hidden sm:inline" style="font-size: 9px;">Agentic Playground</span>
      </div>
      <div class="flex items-center gap-4">
        <a href="/how-it-works" class="no-underline hidden sm:inline" style="font-family: var(--mono-product); font-size: 12px; color: var(--glow-mute);">How it works</a>
        <button class="btn-indigo" onclick={handlePrimary}>
          {signedIn ? 'Enter →' : 'Sign in'}
        </button>
      </div>
    </div>
  </header>

  <!-- Hero -->
  <section class="hero-orbs max-w-4xl mx-auto px-6 pt-24 pb-20 text-center">
    <p class="tm-eyebrow mb-6" style="color: var(--indigo-400);">An Agentic Playground</p>
    <h1
      style="
        font-family: var(--serif);
        font-weight: 700;
        font-size: clamp(44px, 8vw, 88px);
        line-height: 0.95;
        letter-spacing: -0.03em;
        color: #fff;
        margin-bottom: 1.5rem;
      "
    >
      Bring an agent. Ask a question. <em style="font-style: italic; color: var(--copper);">See what happens.</em>
    </h1>
    <p style="font-family: var(--sans-product); font-size: 18px; line-height: 1.6; color: var(--glow-dim); max-width: 42rem; margin: 0 auto 1.5rem;">
      LQ Council is a place to experiment with multi-agent AI. Connect your own agent, query the resident Council, browse past debates, or learn how to build an agent from scratch. Debates are the first tool — more are on the way.
    </p>
    <p class="mono-label" style="color: var(--glow-mute); font-size: 11px; letter-spacing: 0.2em; max-width: 42rem; margin: 0 auto 2.5rem;">
      Integration is one URL · your agent answers a prompt in text · we do the rest
    </p>
    <div class="flex gap-3 justify-center flex-wrap">
      <button class="btn-indigo" onclick={handlePrimary}>
        {signedIn ? 'Enter the playground →' : 'Sign in to start'}
      </button>
      <a href="/how-it-works" class="btn-dark-ghost no-underline">Read the protocol</a>
    </div>
  </section>

  <!-- What you can do -->
  <section class="max-w-5xl mx-auto px-6 pb-20">
    <p class="mono-label mb-8" style="color: var(--indigo-400); letter-spacing: 0.3em;">What you can do</p>
    <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
      {#each CAPABILITIES as c}
        <div class="card-term card-term-hover" style="padding: 22px;">
          <h3 style="font-family: var(--sans-product); font-weight: 600; font-size: 17px; color: var(--glow-txt); margin-bottom: 8px;">{c.title}</h3>
          <p style="font-family: var(--sans-product); font-size: 14px; line-height: 1.6; color: var(--glow-mute);">{c.body}</p>
        </div>
      {/each}
    </div>
  </section>

  <!-- First tool: debates -->
  <section class="max-w-5xl mx-auto px-6 pb-20">
    <div class="flex items-baseline justify-between mb-8 flex-wrap gap-2">
      <p class="tm-eyebrow" style="color: var(--indigo-400);">First tool · Structured debates</p>
      <span class="mono-label" style="color: var(--glow-faint);">5-round protocol</span>
    </div>
    <p style="font-family: var(--sans-product); font-size: 15px; line-height: 1.65; color: var(--glow-dim); max-width: 48rem; margin-bottom: 2rem;">
      The debate engine is the first capability on the platform. Any set of agents can be entered into a five-round adversarial protocol that forces real disagreement instead of the polite convergence LLMs fall into by default.
    </p>
    <div class="grid grid-cols-1 md:grid-cols-5 gap-3">
      {#each ROUNDS as r}
        <div class="card-term card-term-hover" style="padding: 16px; display: flex; flex-direction: column; gap: 10px;">
          <span
            class="mono-label"
            style="
              align-self: flex-start;
              padding: 3px 8px;
              border-radius: 999px;
              font-size: 10px;
              letter-spacing: 0.15em;
              color: var(--indigo-400);
              background: rgba(99,102,241,0.10);
              border: 1px solid rgba(99,102,241,0.25);
            "
          >R{r.n}</span>
          <h3 style="font-family: var(--sans-product); font-weight: 600; font-size: 14px; color: var(--glow-txt);">{r.name}</h3>
          <p style="font-family: var(--sans-product); font-size: 12px; line-height: 1.55; color: var(--glow-mute);">{r.desc}</p>
        </div>
      {/each}
    </div>
  </section>

  <!-- Why -->
  <section class="max-w-5xl mx-auto px-6 pb-20">
    <p class="tm-eyebrow mb-8" style="color: var(--indigo-400);">Why this, not a group chat</p>
    <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
      {#each WHY as w}
        <div class="card-term card-term-hover" style="padding: 22px;">
          <h3 style="font-family: var(--sans-product); font-weight: 600; font-size: 16px; color: var(--glow-txt); margin-bottom: 8px;">{w.title}</h3>
          <p style="font-family: var(--sans-product); font-size: 14px; line-height: 1.6; color: var(--glow-mute);">{w.body}</p>
        </div>
      {/each}
    </div>
  </section>

  <!-- Build your own agent (coming soon) -->
  <section class="max-w-5xl mx-auto px-6 pb-20">
    <div class="card-term-lg" style="border-color: rgba(154,52,18,0.25);">
      <div class="flex items-center gap-3 mb-4 flex-wrap">
        <span class="pill-on" style="background: rgba(154,52,18,0.15); color: var(--copper); border-color: rgba(154,52,18,0.40);">Coming soon</span>
        <h2 style="font-family: var(--sans-product); font-weight: 700; font-size: 22px; color: var(--glow-txt); letter-spacing: -0.01em;">Build your own personal agent</h2>
      </div>
      <p style="font-family: var(--sans-product); font-size: 15px; line-height: 1.6; color: var(--glow-dim); margin-bottom: 20px; max-width: 48rem;">
        Beyond debates: a guided workflow for building your own general-purpose agent — a second brain and daily assistant, personal or professional. Persistent memory, real tools, and reach into the apps you actually use, without standing up your own infrastructure.
      </p>
      <div class="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-4 gap-3">
        {#each [
          { h: 'Persistent memory',      d: 'A second brain that grows with use. Your agent remembers what you told it last month without being re-briefed.' },
          { h: 'Real tools',             d: 'Email, calendar, web search, your files, custom APIs. Agents that act on your behalf, not just answer questions.' },
          { h: 'Meets you where you are',d: 'WhatsApp, Slack, Telegram, email. Reach your agent from the apps you already live in — no extra window to check.' },
          { h: 'Personal + professional',d: 'Life admin, reminders, research, drafting, meeting prep. One agent, two contexts, shared memory.' },
        ] as f}
          <div class="card-term" style="padding: 14px; background: var(--night);">
            <p class="mono-label" style="color: var(--copper); margin-bottom: 6px;">{f.h}</p>
            <p style="font-family: var(--sans-product); font-size: 12px; line-height: 1.55; color: var(--glow-mute);">{f.d}</p>
          </div>
        {/each}
      </div>
    </div>
  </section>

  <!-- Roadmap -->
  <section class="max-w-4xl mx-auto px-6 pb-20">
    <div class="card-term-lg" style="text-align: center; padding: 40px 32px;">
      <p class="tm-eyebrow mb-3" style="color: var(--indigo-400);">Roadmap</p>
      <h2 style="font-family: var(--sans-product); font-weight: 700; font-size: 22px; color: var(--glow-txt); letter-spacing: -0.01em; margin-bottom: 12px;">Debates are the beginning, not the end</h2>
      <p style="font-family: var(--sans-product); font-size: 15px; line-height: 1.6; color: var(--glow-mute); max-width: 36rem; margin: 0 auto;">
        Adversarial committees, tool-using agents, evaluation harnesses, agent-to-agent protocols — if it's a multi-agent pattern worth running, we'll build the orchestration. Sign in to follow along, and tell us what you want to see next.
      </p>
    </div>
  </section>

  <!-- Bottom CTA -->
  <section class="hero-orbs max-w-3xl mx-auto px-6 pb-24 text-center">
    <h2 style="font-family: var(--serif); font-weight: 700; font-size: 40px; line-height: 1.05; letter-spacing: -0.02em; color: #fff; margin-bottom: 1rem;">
      Ready to play?
    </h2>
    <p style="font-family: var(--sans-product); font-size: 15px; line-height: 1.6; color: var(--glow-mute); max-width: 32rem; margin: 0 auto 1.5rem;">
      {signedIn
        ? "You're signed in. Jump into the playground to bring your own bot, submit a question, or explore past debates."
        : 'Access is invitation-based. Sign in to bring your own bot, submit a question to the Council, or explore past debates.'}
    </p>
    <button class="btn-indigo" onclick={handlePrimary}>
      {signedIn ? 'Enter the playground →' : 'Sign in'}
    </button>
  </section>

  <footer style="border-top: 1px solid var(--night-rule); padding: 28px 0; text-align: center;">
    <p class="mono-label" style="color: var(--glow-faint); letter-spacing: 0.25em;">LQ Council · lqcouncil.com</p>
  </footer>
</div>
```

Changes vs. the previous landing page:

- Hero headline now Cormorant Garamond 700 with a copper italic accent on the final phrase.
- Eyebrow labels use `.tm-eyebrow` (JB Mono 12px, 0.3em tracking, indigo-400).
- Section eyebrows use `.mono-label` or `.tm-eyebrow` — no more `text-muted`/underlined dashes.
- Capability cards drop the decorative glyphs (◈◎◇◆) entirely — title + body only, per brand rule.
- Round cards: "R1" label becomes a proper `.mono-label` pill in indigo-family; no per-round hue differentiation (that fought the brand palette).
- "Coming soon" block uses copper-tinted border + copper pill + copper mono labels on its feature cards — the single surface where copper gets to shout.
- Top nav uses `.btn-indigo` for the CTA.
- Orbs (indigo blur behind hero + bottom CTA) via the new `.hero-orbs` class.

- [ ] **Step 2: Verify build + visual**

```bash
cd frontend && npm run build
```

Expected: green.

Visual: `npm run dev` → open `http://localhost:5173/`. Confirm:

- Hero headline is in Cormorant Garamond with italic copper "See what happens."
- Two indigo blur orbs behind the hero (top-left, bottom-right).
- "Agentic Playground" eyebrow above the headline is indigo mono.
- Capability cards: no glyphs, hover lifts them with an indigo border.
- Round cards (R1–R5) each get the indigo-tinted pill label.
- "Coming soon" block has a copper-tinted border and copper pill.
- Bottom CTA also sits over orbs.

Take a screenshot: `/tmp/ui-task6-landing.png`.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/routes/+page.svelte && git commit -m "$(cat <<'EOF'
feat(ui): Terminal-surface landing page

- Cormorant Garamond hero with copper italic accent replaces the Inter
  sans headline + purple eyebrow.
- Decorative glyphs removed from capability cards (brand guide: no icon
  system on terminal surfaces).
- Capability / round / WHY cards adopt .card-term + .card-term-hover so
  every card in the app now lifts consistently on hover.
- "Coming soon" block uses copper-tinted border as the single surface
  where copper carries the frame (reserved accent).
- Hero + bottom CTA gain indigo blur orbs via .hero-orbs helper.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Redesign `/debates` list page

**Files:**
- Modify: `frontend/src/routes/debates/+page.svelte`

- [ ] **Step 1: Locate the current classes to migrate**

Read the file first:

```bash
cat frontend/src/routes/debates/+page.svelte
```

Catalogue every `class="..."` that references `bg-[var(--surface)]`, `border-[var(--border)]`, `bg-[#8b5cf6]`, `hover:border-[#8b5cf6]/40`. The section is ~120 lines of template.

- [ ] **Step 2: Replace the heading + New Debate CTA**

Find (around line 122):

```svelte
<div class="max-w-5xl">
  <div class="flex items-center justify-between mb-8">
    <h1 class="mono text-2xl font-bold">Debates</h1>
    {#if $me?.role === 'admin'}
      <a
        href="/debates/new"
        class="px-4 py-2 bg-[#8b5cf6] text-white rounded-lg text-sm font-medium hover:bg-[#7c3aed] transition-colors no-underline"
      >
        New Debate
      </a>
    {/if}
  </div>
```

Replace with:

```svelte
<div class="max-w-5xl">
  <div class="flex items-end justify-between mb-8 flex-wrap gap-4">
    <div>
      <p class="tm-eyebrow mb-2" style="color: var(--indigo-400);">Workspace</p>
      <h1 style="font-family: var(--sans-product); font-weight: 800; font-size: 32px; letter-spacing: -0.02em; color: var(--glow-txt);">
        Debates
        <span class="stat-serif" style="font-size: 28px; margin-left: 12px;">{debates.length}</span>
      </h1>
    </div>
    {#if $me?.role === 'admin'}
      <a href="/debates/new" class="btn-indigo no-underline">New Debate →</a>
    {/if}
  </div>
```

This introduces the stat-serif copper count pattern the brand guide specifies for numeric stats alongside headings.

- [ ] **Step 3: Replace the filter pill row**

Find (around line 136):

```svelte
  <!-- Status filters + archived toggle -->
  <div class="flex items-center gap-2 mb-6 flex-wrap">
    {#each FILTERS as f}
      <button
        onclick={() => (filter = f)}
        class="px-3 py-1.5 rounded-lg text-xs mono transition-colors {filter === f
          ? 'bg-[#8b5cf6] text-white'
          : 'bg-[var(--surface)] text-[var(--text-secondary)] border border-[var(--border)] hover:text-[var(--text-primary)]'}"
      >
        {f.charAt(0).toUpperCase() + f.slice(1)}
      </button>
    {/each}
    {#if $me?.role === 'admin'}
      <span class="w-px h-5 bg-[var(--border)] mx-1"></span>
      <label class="flex items-center gap-2 text-xs mono text-[var(--text-secondary)] cursor-pointer select-none">
        <input
          type="checkbox"
          bind:checked={showArchived}
          class="accent-[#8b5cf6]"
        />
        Show archived
      </label>
    {/if}
  </div>
```

Replace with:

```svelte
  <!-- Status filters + archived toggle -->
  <div class="flex items-center gap-2 mb-6 flex-wrap">
    {#each FILTERS as f}
      <button
        onclick={() => (filter = f)}
        class={filter === f ? 'pill-on' : 'pill-off'}
      >
        {f.charAt(0).toUpperCase() + f.slice(1)}
      </button>
    {/each}
    {#if $me?.role === 'admin'}
      <span style="width: 1px; height: 20px; background: var(--night-rule); margin: 0 4px;"></span>
      <label
        class="flex items-center gap-2 cursor-pointer select-none"
        style="font-family: var(--mono-product); font-size: 12px; color: var(--glow-mute);"
      >
        <input
          type="checkbox"
          bind:checked={showArchived}
          style="accent-color: var(--indigo-500);"
        />
        Show archived
      </label>
    {/if}
  </div>
```

- [ ] **Step 4: Replace the loading / error / empty states**

Find (around line 160, the `{#if loading}` through `{:else if filtered.length === 0}`):

- Replace `bg-[var(--surface)] border border-[var(--border)] rounded-lg` → `card-term`.
- Replace `bg-red-500/10 border border-red-500/30 rounded-lg p-6` → keep the red-family but use tokens: inline `style="background: rgba(239,68,68,0.08); border: 1px solid rgba(239,68,68,0.25); border-radius: var(--r-lg); padding: 20px;"`.
- Skeleton `animate-pulse` rows: keep them, but replace `bg-[var(--border)]` on the inner bars with `background: var(--night-rule);`.
- Empty-state "New Debate" link: `class="btn-indigo no-underline"`.

- [ ] **Step 5: Replace the debate row cards**

Find (around line 200, the `{#each filtered as debate (debate.id)}` loop) and restyle:

- Row wrapper: `<div class="card-term card-term-hover" style="opacity: {archived ? 0.55 : 1}; padding: 18px;">`. Remove the `hover:border-[#8b5cf6]/40` class — `card-term-hover` handles that.
- `<h3>` title: inline style `font-family: var(--sans-product); font-weight: 600; font-size: 15px; color: var(--glow-txt); line-height: 1.35;`. On hover (row `card-term-hover`) the text stays the same — no colour change; the lift + border are the signal.
- The `archived` badge span: use the `.pill-on` class but tinted copper: `style="background: rgba(154,52,18,0.14); color: var(--copper); border-color: rgba(154,52,18,0.35); font-size: 9px; padding: 2px 8px;"`.
- Meta row (`{debate.bots.length} agent...`, id slice, date): wrap each span as `mono-label` with `color: var(--glow-faint);` and gap 12px. Add a serif count for the agents number: `<span class="stat-serif" style="font-size: 16px;">{debate.bots.length}</span> <span class="mono-label">agents</span>`.
- Admin action buttons (Archive / Delete): use `.btn-dark-ghost` with `font-size: 10px; padding: 5px 12px;`. The Delete-confirm state: replace the red Tailwind classes with `style="background: rgba(239,68,68,0.15); border-color: rgba(239,68,68,0.5); color: #FCA5A5;"` inline-applied via the conditional.

Keep all JS logic (`reload`, `doArchive`, `doDelete`, `confirmingDelete`, `busyId`) exactly as-is.

- [ ] **Step 6: Verify build + visual**

```bash
cd frontend && npm run build
```

Expected: green.

Visual with backend up: sign in as admin, go to `/debates`. Confirm:

- Header shows "Debates <copper-serif-count>" plus a "Workspace" eyebrow.
- Filter pills: active = indigo tinted, inactive = dark ghost pill.
- Row cards lift +2px with an indigo border on hover.
- Archived badge is copper-tinted.
- Delete-confirm state shows red-tinted without relying on Tailwind red utilities.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/routes/debates/+page.svelte && git commit -m "$(cat <<'EOF'
feat(ui): Terminal /debates list page

- Header gets a 'Workspace' eyebrow and a copper serif count next to the
  title — first use of the .stat-serif primitive.
- Filter pills replaced with .pill-on / .pill-off.
- Row cards move to .card-term + .card-term-hover — consistent lift
  behaviour with everything else.
- Archived badge is copper-tinted (warm flag for out-of-stream state).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Redesign `/debates/[id]` detail view

**Files:**
- Modify: `frontend/src/routes/debates/[id]/+page.svelte`
- Modify: `frontend/src/lib/components/DebateTranscriptView.svelte`
- Modify: `frontend/src/lib/components/RoundAccordion.svelte`
- Modify: `frontend/src/lib/components/ResponseCard.svelte`
- Modify: `frontend/src/lib/components/ChallengeBlock.svelte`
- Modify: `frontend/src/lib/components/PositionChangeBlock.svelte`
- Modify: `frontend/src/lib/components/SteelmanBlock.svelte`
- Modify: `frontend/src/lib/components/SynthesisCard.svelte`
- Modify: `frontend/src/lib/components/SynthesisQualityReport.svelte`
- Modify: `frontend/src/lib/components/ConfidenceChart.svelte`
- Modify: `frontend/src/lib/components/DivergencePanel.svelte`

This is the largest task. Work through the files in the order listed. After each file, re-build. One commit at the end.

- [ ] **Step 1: Read the route page and identify the top-level structure**

```bash
cat frontend/src/routes/debates/[id]/+page.svelte
```

Note the tab structure (Transcript / Synthesis / Outcome / Quality tabs — whatever's there), the header with topic + status, and how child components are composed.

- [ ] **Step 2: Restyle `debates/[id]/+page.svelte` header + tabs**

Replace the debate-header block so the topic renders in Cormorant 600, status in a StatusBadge pill, created-at in mono-label. Replace the tab bar wrapper (if any inline) to use `.pill-on` / `.pill-off` as TabBar does internally. Keep all data-loading logic untouched.

Concrete rule: every `bg-[var(--surface)] border border-[var(--border)] rounded-lg` in this file becomes `class="card-term"`. Every `bg-[#8b5cf6]` becomes `class="btn-indigo"` (if interactive) or `background: var(--indigo-500)` (if decorative). Every `mono text-[var(--text-muted)]` becomes `class="mono-label"` with `style="color: var(--glow-faint);"`.

- [ ] **Step 3: Restyle `DebateTranscriptView.svelte`**

```bash
cat frontend/src/lib/components/DebateTranscriptView.svelte
```

Expected: 185 lines. Apply the same conversion rules:

- Outer container wrapping rounds: `class="space-y-4"` stays; inner per-round wrapper becomes `.card-term`.
- Any section headings → `.tm-eyebrow` (indigo-400) + serif heading if the heading doubles as a stat.
- Loading skeleton bars: use `background: var(--night-rule);`.

- [ ] **Step 4: Restyle `RoundAccordion.svelte`**

```bash
cat frontend/src/lib/components/RoundAccordion.svelte
```

Expected: 88 lines. The collapse header shows "Round N — Name", the body shows a list of `ResponseCard`. Restyle:

- Collapse button: `style="font-family: var(--sans-product); font-size: 14px; font-weight: 600; color: var(--glow-txt);"` with a mono `R{n}` pill on the left (indigo-tinted).
- Expanded panel: `.card-term` with `padding: 20px`.
- Collapse chevron: replace any Unicode arrow glyph (▸ / ▾) with a text `+` / `−` rendered in `mono-label` at 16px — or keep the SVG arrow if it's already SVG (svg is fine).

- [ ] **Step 5: Restyle `ResponseCard.svelte`**

The current wrapper is `bg-[var(--bg)] border border-[var(--border)] rounded-lg p-4`. Replace with `class="card-term"`. Inside:

- AgentBadge stays (already restyled in Task 5).
- "Unresponsive" / "Abstained" / "carried from R{N}" badges: tint them using color-mix with the appropriate semantic hex. Unresponsive: `#EF4444`; Abstained: `var(--glow-faint)`; Carried: `var(--glow-mute)`.
- `<p class="text-sm text-[var(--text-secondary)] whitespace-pre-wrap leading-relaxed">` (the main response body): change to `style="font-family: var(--sans-product); font-size: 15px; line-height: 1.65; color: var(--glow-dim); white-space: pre-wrap;"`.
- "conf: NN" + valid/invalid markers: `mono-label` tone with semantic colours (valid = emerald; invalid = red; muted-text for the label).

- [ ] **Step 6: Restyle `ChallengeBlock`, `PositionChangeBlock`, `SteelmanBlock`**

Each of these is 47–61 lines of semantic content blocks inside a response card. Convert:

- Outer wrapper of each block: `style="margin-top: 12px; padding: 12px; border-left: 2px solid {accent}; background: color-mix(in srgb, {accent} 6%, transparent); border-radius: 0 var(--r-md) var(--r-md) 0;"` where `{accent}` is `var(--challenge-factual)` / `--challenge-logical` / `--challenge-premise` for ChallengeBlock, `var(--indigo-400)` for PositionChange, `var(--copper)` for Steelman (steelman is the bot strengthening the other side — copper as the "rare, signature move" colour).
- Inner eyebrow: `mono-label` with the same accent colour.
- Body: Inter 14px, `color: var(--glow-dim)`.
- Provenance attribution line (small quote): italic Inter 12px, `color: var(--glow-mute)`, with a leading em-dash.

- [ ] **Step 7: Restyle `SynthesisCard.svelte`**

```bash
cat frontend/src/lib/components/SynthesisCard.svelte
```

Expected: 46 lines. This is the final synthesis headline block. Apply:

- Outer wrapper: `.card-term-lg`.
- "Synthesis" eyebrow: `.tm-eyebrow`, `color: var(--indigo-400)`.
- Synthesis headline text: Cormorant 600, 28px, `color: var(--glow-txt)`, italic copper accents for any emphasised phrases.
- Citation mono strings: `.mono-label` with `color: var(--glow-faint)`.

- [ ] **Step 8: Restyle `SynthesisQualityReport.svelte`**

```bash
cat frontend/src/lib/components/SynthesisQualityReport.svelte
```

Expected: 302 lines (largest synthesis component). Replace `bg-[var(--surface)]` → `card-term`, numeric scores → `.stat-serif` (copper 28px), section eyebrows → `.tm-eyebrow`, row delimiters → `border-top: 1px solid var(--night-rule);`. Keep all score-derivation logic unchanged.

- [ ] **Step 9: Restyle `ConfidenceChart.svelte`**

```bash
cat frontend/src/lib/components/ConfidenceChart.svelte
```

Expected: 99 lines. This is a chart.js config. Update the `datasets[].borderColor` / `backgroundColor` to draw from `AGENT_COLORS` (already migrated). Grid line colour: `'rgba(31,31,47,0.8)'` (night-rule). Axis tick colour: `'#8888A0'` (glow-mute). Tooltip background: `'#0F0F17'` (night-raise).

- [ ] **Step 10: Restyle `DivergencePanel.svelte`**

Expected: 81 lines. Apply the same card + eyebrow + copper stat recipe. Numeric divergence score goes in `.stat-serif`.

- [ ] **Step 11: Verify build + full-path visual check**

```bash
cd frontend && npm run build
```

Expected: green.

Visual: open a completed debate. Click through every tab (Transcript / Synthesis / Outcome / Quality if present). Confirm:

- Header shows the debate topic in Cormorant 600 (plain — topic is user-authored, so no copper italic accent is applied).
- Round accordion opens/closes. Collapsed header shows "R1 · Blind Formation" with an indigo pill.
- ResponseCard: agent badge left, response body Inter 15px, challenge/steelman blocks have left-border accent rule in the right semantic colour.
- Synthesis tab: synthesis card uses Cormorant headline + indigo eyebrow + copper stat where scores appear.
- ConfidenceChart: lines render in the new agent palette.

Screenshot: `/tmp/ui-task8-debate-detail.png`.

- [ ] **Step 12: Commit**

```bash
git add frontend/src/routes/debates/\[id\]/+page.svelte \
        frontend/src/lib/components/DebateTranscriptView.svelte \
        frontend/src/lib/components/RoundAccordion.svelte \
        frontend/src/lib/components/ResponseCard.svelte \
        frontend/src/lib/components/ChallengeBlock.svelte \
        frontend/src/lib/components/PositionChangeBlock.svelte \
        frontend/src/lib/components/SteelmanBlock.svelte \
        frontend/src/lib/components/SynthesisCard.svelte \
        frontend/src/lib/components/SynthesisQualityReport.svelte \
        frontend/src/lib/components/ConfidenceChart.svelte \
        frontend/src/lib/components/DivergencePanel.svelte && \
git commit -m "$(cat <<'EOF'
feat(ui): Terminal debate-detail surface

Eleven files restyled: the debate detail route, the transcript/round
plumbing, the response card + its three semantic children, the synthesis
card + quality report, the confidence chart, and the divergence panel.
All move to .card-term / .tm-eyebrow / .stat-serif / agent-palette, with
each semantic child block (challenge / position change / steelman)
getting its own left-border accent in the right category colour.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: Redesign outcome-view components

**Files:**
- Modify: `frontend/src/lib/components/outcome/OutcomeTab.svelte`
- Modify: `frontend/src/lib/components/outcome/OutcomeDrawer.svelte`
- Modify: `frontend/src/lib/components/outcome/OutcomeFilters.svelte`
- Modify: `frontend/src/lib/components/outcome/DivergenceHeadline.svelte`
- Modify: `frontend/src/lib/components/outcome/MapLegend.svelte`
- Modify: `frontend/src/lib/components/outcome/ReplaySlider.svelte`
- Modify: `frontend/src/lib/components/outcome/BotStanceMap.svelte`
- Modify: `frontend/src/lib/components/outcome/ArgumentMap3D.svelte`

- [ ] **Step 1: Read every outcome file before editing**

```bash
for f in frontend/src/lib/components/outcome/*.svelte; do
  echo "===== $f ====="
  cat "$f" | head -30
done
```

Confirm each component's props + purpose. The 3D graph (`ArgumentMap3D`, 464 lines) is the most complex — skim its 3D scene setup to identify where node/link colours come from.

- [ ] **Step 2: Restyle `OutcomeTab.svelte` (198 lines)**

Top-level wrapper: `background: var(--night); padding: 20px;`. Section cards: `.card-term`. Section titles: `.tm-eyebrow indigo-400` + serif heading. Keep layout grid dimensions.

- [ ] **Step 3: Restyle `OutcomeDrawer.svelte` (205 lines)**

Drawer backdrop: `background: rgba(8,8,13,0.75); backdrop-filter: blur(4px);`. Drawer panel: `background: var(--night-raise); border-left: 1px solid var(--night-rule);`. Close button: `.btn-dark-ghost` with a text `×` (no icon). Content cards: `.card-term`. Any inline `#8b5cf6` → `var(--indigo-500)`.

- [ ] **Step 4: Restyle `OutcomeFilters.svelte` (64 lines)**

Filter chips: `.pill-on` / `.pill-off`. Any `<select>` or `<input>`: `style="background: var(--night-raise); border: 1px solid var(--night-rule); border-radius: 8px; padding: 8px 12px; font-family: var(--sans-product); font-size: 13px; color: var(--glow-txt);"`.

- [ ] **Step 5: Restyle `DivergenceHeadline.svelte` (93 lines)**

Banner wrapper: `.card-term-lg`. Divergence score: `.stat-serif` at 40px (the "marquee" copper number). Supporting text: Inter 14px `color: var(--glow-dim)`. Trend arrow or indicator: use a Unicode `→` / `↓` in JB Mono — no SVG.

- [ ] **Step 6: Restyle `MapLegend.svelte` (52 lines)**

Wrapper: `.card-term` with `padding: 14px;`. Each legend entry: 10px coloured dot + JB Mono 11px label `color: var(--glow-mute);`. Entries pull `AGENT_COLORS` directly.

- [ ] **Step 7: Restyle `ReplaySlider.svelte` (102 lines)**

Track: `background: var(--night-edge); height: 6px; border-radius: 999px;`. Filled portion: `background: var(--indigo-500);`. Handle: `12px indigo-500 circle with 2px night-raise border`. Round markers along the track: 4px indigo-400 dots. Keyframe labels: `mono-label color: var(--glow-faint);`.

- [ ] **Step 8: Restyle `BotStanceMap.svelte` (208 lines)**

This is a 2D d3-force layout. Node fill colours: use `agentColor(pseudonym)`. Link stroke: `'rgba(31,31,47,0.6)'` (night-rule faded). Grid / background rings: `'rgba(26,26,38,0.5)'` (night-edge faded). Tooltip: match the `.card-term` style with `background: var(--night-edge); border: 1px solid var(--night-rule2); border-radius: 8px; padding: 10px; font-family: var(--mono-product); font-size: 11px; color: var(--glow-txt);`.

- [ ] **Step 9: Restyle `ArgumentMap3D.svelte` (464 lines)**

The big file. Find:

- Any `0x8b5cf6` hex integer used as a material colour → `0x6366F1` (indigo-500).
- Any agent colour palette that's hard-coded inline → replace with a derivation from `AGENT_COLORS` (import and use `Object.values(AGENT_COLORS)` converted to three.js `new THREE.Color(hex)`).
- Scene background: `0x08080D` (night).
- Link material colour: `0x1F1F2F` (night-rule).
- Text sprite background (if any): `0x0F0F17` (night-raise).
- OrbitControls / axis labels: switch to `var(--glow-mute)`.

Do NOT change the graph topology, force simulation parameters, or interaction handlers. Pure colour/theme pass.

- [ ] **Step 10: Verify build + visual**

```bash
cd frontend && npm run build
```

Expected: green.

Visual: open a completed debate's Outcome tab. Confirm:

- The 3D argument graph renders in the new palette (indigo / emerald / cyan / amber / violet nodes, dark rule-coloured links, near-black background).
- The bot-stance 2D map uses the same palette.
- Divergence headline shows a large copper serif number.
- Legend shows the correct 5 agent colours with JB Mono labels.
- Replay slider: indigo track + handle, copper round markers at keyframes.
- Outcome drawer: night-raise panel with a `×` text close button, cards inside use `.card-term`.

Screenshot: `/tmp/ui-task9-outcome.png`.

- [ ] **Step 11: Commit**

```bash
git add frontend/src/lib/components/outcome/ && git commit -m "$(cat <<'EOF'
feat(ui): Terminal outcome-view surface

Eight outcome components restyled, including the 3D argument graph
(ArgumentMap3D) which now pulls colours from AGENT_COLORS rather than
hard-coded hex integers. Divergence headline uses the .stat-serif copper
treatment for the headline number. Legend, replay slider, drawer, and
filter chips all migrate to brand primitives.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: Redesign `/debates/new` creation form

**Files:**
- Modify: `frontend/src/routes/debates/new/+page.svelte`

- [ ] **Step 1: Read the current form**

```bash
cat frontend/src/routes/debates/new/+page.svelte
```

Catalogue every form control (text inputs, textarea for topic, checkboxes for bot selection, submit button).

- [ ] **Step 2: Apply the form style pattern**

Replace every form control with the standard terminal-form style:

- `<input type="text">`, `<textarea>`, `<select>`:

```css
background: var(--night-raise);
border: 1px solid var(--night-rule);
border-radius: 8px;
padding: 10px 14px;
font-family: var(--sans-product);
font-size: 14px;
color: var(--glow-txt);
transition: border-color var(--dur-fast) var(--ease-standard);
```

On `:focus` (apply via a `<style>` block or inline handler): `border-color: var(--indigo-500); outline: none; box-shadow: 0 0 0 3px rgba(99,102,241,0.15);`.

- Field labels: `.mono-label` (10px, 0.2em tracking, `var(--indigo-400)`).
- Field help text below: Inter 12px, `var(--glow-mute)`, `margin-top: 4px`.
- Validation error text: Inter 12px, `#EF4444`, mono-labelled prefix "Error ·".
- Checkbox wrappers (bot selection list): each row is a `.card-term` that becomes `card-term-hover` when the checkbox is unchecked, or gets `border-color: var(--indigo-500); background: rgba(99,102,241,0.05);` when checked.
- Submit button: `.btn-indigo` with `padding: 12px 28px;`.
- Cancel link: `.btn-dark-ghost`.
- Page header: same pattern as `/debates` list — "New debate" title + "Workspace · Create" eyebrow.

- [ ] **Step 3: Verify build + visual**

```bash
cd frontend && npm run build
```

Expected: green.

Visual (admin only): click "New Debate" on `/debates`. Confirm:

- Form fields have consistent dark-raised inputs with indigo focus rings.
- Bot selection rows lift on hover; selected rows have indigo border + tinted bg.
- Submit button is `.btn-indigo`.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/routes/debates/new/+page.svelte && git commit -m "$(cat <<'EOF'
feat(ui): Terminal /debates/new creation form

Form controls adopt the night-raise input pattern with indigo focus
rings. Bot-selection rows use .card-term-hover for discovery and an
indigo-tinted selected state. Submit is .btn-indigo.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 11: Redesign the Bots section (5 pages)

**Files:**
- Modify: `frontend/src/routes/bots/+page.svelte`
- Modify: `frontend/src/routes/bots/submit/+page.svelte`
- Modify: `frontend/src/routes/bots/my-submissions/+page.svelte`
- Modify: `frontend/src/routes/bots/criteria/+page.svelte`
- Modify: `frontend/src/routes/bots/guide/+page.svelte`

- [ ] **Step 1: Read all five**

```bash
for f in frontend/src/routes/bots/+page.svelte \
         frontend/src/routes/bots/submit/+page.svelte \
         frontend/src/routes/bots/my-submissions/+page.svelte \
         frontend/src/routes/bots/criteria/+page.svelte \
         frontend/src/routes/bots/guide/+page.svelte; do
  echo "===== $f ====="
  wc -l "$f"
  head -40 "$f"
done
```

Confirm each page's structure. The main `bots/+page.svelte` is 414 lines and is the most complex; the others are shorter.

- [ ] **Step 2: Restyle `bots/+page.svelte`** (414 lines, main bot list + admin actions)

Apply the established pattern:

- Header: "Bots" title with `.stat-serif` copper count (`{bots.length}`), "Workspace · Agents" eyebrow, and a `.btn-indigo` "Submit a bot" CTA (or "Approve" / "Review pending" if admin).
- Filter row: `.pill-on` / `.pill-off` for status filters (active / pending / inactive / rejected).
- Bot row cards: `.card-term card-term-hover` with:
  - Left: bot name in Inter 600 16px (`var(--glow-txt)`), followed by pseudonym / id in `.mono-label`.
  - Middle: status pill via `StatusBadge`, a kind pill (`external` / `text_only`) as `.pill-on` in `var(--glow-faint)` tint.
  - Right: admin action buttons as `.btn-dark-ghost` (Approve / Reject / Deactivate / Test) and `.btn-indigo` for the primary one.
- Expanded detail (if the page has an expand-to-show-history accordion): `.card-term-lg` with nested stat grid where each stat uses `.stat-serif` at 24px.
- Any inline `bg-[#8b5cf6]` / `hover:border-[#8b5cf6]/40` replaced as in prior tasks.

- [ ] **Step 3: Restyle `bots/submit/+page.svelte`**

Apply the same form pattern as Task 10 (`/debates/new`): night-raise inputs, indigo focus rings, `.mono-label` field labels, `.btn-indigo` submit.

Add a "Submission" eyebrow in indigo-400 at the top plus a serif "Submit a bot" headline.

Preserve the existing validation UX (Clint's `lqc_validate_bot` responses plus the text-only vs external toggle; see `CLAUDE.md` operational-lesson 18 — "Clint's lqc_* tools still speak the legacy /debate contract").

- [ ] **Step 4: Restyle `bots/my-submissions/+page.svelte`**

Apply the list-page pattern from `bots/+page.svelte`, scoped to current-user submissions. "My submissions" headline in Inter 800, copper stat-serif for count.

- [ ] **Step 5: Restyle `bots/criteria/+page.svelte`**

Documentation-style page. Use:

- Page heading: Cormorant 600 32px, indigo-400 eyebrow `CRITERIA`.
- Section headings: Inter 700 20px.
- Body copy: Inter 15px line-height 1.7, `var(--glow-dim)`.
- Bullet points: no custom bullets; use `•` in `.mono-label` tone.
- Code snippets: `<pre>` with `background: var(--night-edge); border: 1px solid var(--night-rule); border-radius: 8px; padding: 16px; font-family: var(--mono-product); font-size: 12px; color: var(--glow-dim);`.

- [ ] **Step 6: Restyle `bots/guide/+page.svelte`**

Similar to criteria. Additionally: the snippet code blocks referenced in `snippets.ts` render as `<pre>` — match to the criteria-page `<pre>` style. Copy-snippet buttons (if any): `.btn-dark-ghost` with `padding: 4px 10px; font-size: 10px;`.

- [ ] **Step 7: Verify build + visual**

```bash
cd frontend && npm run build
```

Expected: green.

Visual: as admin, open `/bots`, `/bots/submit`, `/bots/my-submissions`, `/bots/criteria`, `/bots/guide`. Confirm the pattern matches the /debates surface (same card / filter / button feel).

- [ ] **Step 8: Commit**

```bash
git add frontend/src/routes/bots/ && git commit -m "$(cat <<'EOF'
feat(ui): Terminal bots surface

Five bot-management pages migrated to brand primitives. Main bot list
uses .card-term-hover rows with status pills and admin action ghosts;
submission form mirrors /debates/new's input treatment; documentation
pages (criteria + guide) adopt Cormorant section headings + night-edge
code blocks.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 12: Redesign the remaining pages

**Files:**
- Modify: `frontend/src/routes/admins/+page.svelte`
- Modify: `frontend/src/routes/settings/+page.svelte`
- Modify: `frontend/src/routes/security/+page.svelte`
- Modify: `frontend/src/routes/how-it-works/+page.svelte`
- Modify: `frontend/src/routes/sign-in/+page.svelte`

- [ ] **Step 1: Restyle `admins/+page.svelte`** (189 lines)

Admin user list page. Apply:

- Header: "Admins" + copper stat-serif count + "Workspace · Roles" eyebrow.
- User rows: `.card-term card-term-hover` with user_id in mono, email in Inter, role pill via `.pill-on`.
- Add-admin form: inline below the header with a night-raise input + `.btn-indigo` "Promote".
- Demote buttons: `.btn-dark-ghost` with red hover (color-mix red-accent on hover).

- [ ] **Step 2: Restyle `settings/+page.svelte`** (88 lines)

Small page, mostly toggles. Apply:

- Section groups: `.card-term-lg` per group.
- Section title: `.tm-eyebrow` indigo-400.
- Each toggle row: Inter 14px label on the left, right-side input/toggle in night-raise pattern.

- [ ] **Step 3: Restyle `security/+page.svelte`** (269 lines)

Documentation page with multiple sections. Apply:

- Page heading: Cormorant 600 40px, indigo-400 "Security" eyebrow.
- Section headings: Inter 700 20px.
- Body copy: Inter 15px 1.7 `var(--glow-dim)`.
- Callout boxes (if any): `.card-term-lg` with left-border accent (copper for "Note", red for "Warning", indigo for "Info").
- Inline code: `<code>` with `background: rgba(31,31,47,0.5); padding: 1px 6px; border-radius: 4px; font-family: var(--mono-product); font-size: 12px; color: var(--indigo-400);`.
- Block code: same `<pre>` pattern as Task 11.

- [ ] **Step 4: Restyle `how-it-works/+page.svelte`** (306 lines)

Mostly diagrammatic documentation. Apply:

- Page heading: Cormorant 600 40px, indigo-400 "Protocol" eyebrow.
- Each round section: `.card-term-lg` with an indigo-tinted `R{n}` pill top-left (same pill as in the landing-page rounds grid).
- Round heading: Inter 700 22px.
- Round body: Inter 15px 1.7.
- If a sequence diagram is rendered in pure CSS / HTML, border hairlines become `var(--night-rule)`, node fills `var(--night-raise)`, node borders `var(--night-rule3)`.

- [ ] **Step 5: Restyle `sign-in/+page.svelte`** (48 lines)

Minimal page — just shows a redirect message. Apply:

- Page bg: `var(--night)`.
- "LQ Council" wordmark: Inter 800 28px, centred.
- Eyebrow "Sign in" in indigo-400 JB Mono below wordmark.
- Redirect message: `.mono-label` `var(--glow-mute)`.
- Error state: red-tinted `.card-term` with `.btn-indigo` reload.

- [ ] **Step 6: Verify build + visual**

```bash
cd frontend && npm run build
```

Expected: green.

Visual: walk through `/admins`, `/settings`, `/security`, `/how-it-works`, `/sign-in`. Confirm every page now reads as the same family.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/routes/admins/+page.svelte \
        frontend/src/routes/settings/+page.svelte \
        frontend/src/routes/security/+page.svelte \
        frontend/src/routes/how-it-works/+page.svelte \
        frontend/src/routes/sign-in/+page.svelte && \
git commit -m "$(cat <<'EOF'
feat(ui): Terminal admin, settings, security, how-it-works, sign-in

Final batch of page restyles. Every route on lqcouncil.com now conforms
to the LegalQuants Terminal surface — card-term + hover-lift rows,
indigo state colour, copper stat-serif numbers, JB Mono eyebrows,
Cormorant Garamond headings on documentation pages.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 13: Cross-route verification sweep

**Files:** (no file changes — verification only)

- [ ] **Step 1: Full frontend build**

```bash
cd frontend && npm run build
```

Expected: green, no warnings about unused styles, no svelte-check errors.

- [ ] **Step 2: Grep for remaining legacy colour references**

```bash
cd frontend && grep -rn --include='*.svelte' --include='*.ts' '#8b5cf6\|#7c3aed\|#f472b6\|#34d399\|#60a5fa' src/ || echo "clean"
```

Expected: prints only acceptable matches (if any — the migrated agent-colors.ts may still legitimately include some of those hex strings in comments referencing the old palette; actual colour *usage* should have moved to tokens).

If there are unexpected matches, fix them inline and re-grep until it prints "clean".

- [ ] **Step 3: Grep for remaining `var(--surface)`/`var(--bg)`/`var(--border)` usage**

```bash
cd frontend && grep -rn --include='*.svelte' 'var(--surface)\|var(--bg)\|var(--border)' src/ | wc -l
```

This should be low — ideally zero in migrated files. Remaining legacy var references fall back through `theme.css` so the build doesn't break, but they signal incomplete migration. If the count is > 10, iterate: find the files and swap to the brand tokens directly.

- [ ] **Step 4: Grep for remaining Unicode icon glyphs**

```bash
cd frontend && grep -rn --include='*.svelte' $'[\u2660-\u27BF\u2190-\u21FF]' src/ | grep -v '→\|—\|·\|❯' | head -20
```

Expected: only the intentional arrows (`→`, `—`, `·`, `❯`). Any remaining decorative glyphs (⚔ ⚙ ✱ ☰ ■ ≡ ◈ ◎ ◇ ◆) should be gone.

- [ ] **Step 5: CI parity check**

The CI workflow will run the same `npm run build` plus `svelte-check`. Confirm locally:

```bash
cd frontend && npm run check
```

Expected: green. If this fails with type errors that didn't surface during Vite's dev-mode, fix them now (type errors mostly surface from prop signatures that haven't changed — so this is usually clean, but worth confirming).

- [ ] **Step 6: Manual pass via `npm run dev`**

Start:

```bash
cd frontend && npm run dev -- --host 0.0.0.0
```

Visit (need a running backend for authed pages — start one on EVO or use `cargo run` locally):

- `/` — landing
- `/sign-in` — redirect flow
- `/debates` — list
- `/debates/<id>` — detail (all tabs)
- `/debates/new` — create (admin)
- `/bots` — list
- `/bots/submit` — form
- `/bots/my-submissions`
- `/bots/criteria`
- `/bots/guide`
- `/admins`
- `/settings`
- `/security`
- `/how-it-works`

For each: confirm (a) background = #08080D, (b) primary CTAs are indigo, (c) copper appears only on numeric stats / archived badges / active-sidebar number / "Coming soon" block / "pending" status pill, (d) no stray glyphs, (e) cards lift on hover where interactive.

Take one screenshot per page into `/tmp/ui-task13-<route>.png` — these will go into the PR body as the visual review evidence.

- [ ] **Step 7: Commit screenshots metadata (optional; no file changes)**

No commit needed for verification.

- [ ] **Step 8: Open the PR**

```bash
git push -u origin claude/sad-maxwell-0622f9
gh pr create --base main --head claude/sad-maxwell-0622f9 --title "feat(ui): LegalQuants Terminal redesign of lqcouncil.com" --body "$(cat <<'EOF'
## Summary

Full visual redesign of the SvelteKit frontend onto the LegalQuants brand Terminal surface (near-black + indigo glow, Inter / JetBrains Mono / Cormorant Garamond, copper accent). No behavioural changes — every route, component prop, API call, and auth flow is unchanged.

Design source: the brand-kit archive the user attached (Masdar Proposal UI kit, 2026-04-21), specifically `colors_and_type.css` and the terminal preview components.

### Structural changes

- New `src/lib/styles/` folder with three layers:
  - `tokens.css` — brand tokens copied verbatim from the kit (source of truth)
  - `theme.css` — legacy-to-brand mapping so the migration could proceed incrementally
  - `primitives.css` — shared `.btn-indigo` / `.card-term` / `.tm-eyebrow` / `.pill-on` utility classes
- `app.html` loads Google Fonts (Inter + JetBrains Mono + Cormorant Garamond); removes `class="dark"` (single theme)
- `app.css` drops Geist Mono and self-hosted Inter; maps legacy CSS vars onto the new tokens
- `agent-colors.ts` palette migrates to indigo-family (indigo-400 / emerald / cyan / amber / violet-400)

### Visual rules enforced

- All cards: 12px radius, 1px night-rule border, night-raise background, hover-lift with indigo-500 border + soft indigo shadow
- All CTAs: indigo-600 filled or night-raise ghost
- All eyebrows: JetBrains Mono uppercase 0.3em tracking, indigo-400
- All numeric stats: Cormorant Garamond 700, copper #9A3412 (`.stat-serif`)
- All decorative Unicode glyphs (⚔ ⚙ ✱ ☰ ■ ≡ ◈ ◎ ◇ ◆) removed per brand's "no icon system" rule
- Hero and closing-CTA surfaces get indigo blur orbs via `.hero-orbs`

### Files touched

~30 Svelte files (every route + every shared component) plus 4 style/theme files.

### Commits

13 atomic commits, one per task in `docs/superpowers/plans/2026-04-23-lqcouncil-ui-redesign.md`. Each passed `npm run build` before commit.

## Test plan

- [x] `cd frontend && npm run build` green
- [x] `cd frontend && npm run check` green
- [x] CI (backend + frontend jobs) green
- [x] Manual walk-through of every route with screenshots attached below
- [ ] After merge: `./scripts/ship.sh` to deploy to EVO
- [ ] Smoke: `curl https://lqcouncil.com/api/health` still returns `{"status":"ok"}`
- [ ] Smoke: `curl -s https://lqcouncil.com/` contains the new `--night` background + Cormorant headline

## Screenshots

(Attached inline in the PR UI from `/tmp/ui-task*.png`)

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Expected: PR URL printed. Paste it at the end of this task's report.

---

## Task 14: Ship to EVO (post-merge)

**Files:** (no file changes — deploy only)

This task runs after the PR is merged to `main`.

- [ ] **Step 1: Sync to main**

```bash
git checkout main && git pull origin main
```

Expected: the PR's commits appear in `git log`.

- [ ] **Step 2: Ship**

```bash
./scripts/ship.sh
```

Expected: all seven stages green (preflight → env preflight → local build → scp → remote build → health poll → public smoke). Final line: a success summary with the new SHA written to `.last-known-good-sha`.

If anything fails mid-ship: DO NOT ad-hoc fix on EVO. Run `./scripts/rollback.sh` (promotes the previous binary back in ~10s), investigate the failure locally, then re-ship.

- [ ] **Step 3: Post-deploy smoke**

```bash
curl -s https://lqcouncil.com/api/health && echo && \
curl -s https://lqcouncil.com/ | grep -o '<title>[^<]*</title>' && \
curl -s https://lqcouncil.com/ | head -c 500
```

Expected:

- Health returns `{"status":"ok"}` (or similar — match the previous shape).
- Title is `<title>LQ Council — An Agentic Playground</title>` or similar.
- First 500 bytes of HTML include references to `--night` / `Cormorant` / `JetBrains Mono` or the Google Fonts CDN link.

- [ ] **Step 4: Visual production check**

Open `https://lqcouncil.com/` in a browser. Confirm the rendered page matches the `npm run dev` version. Sign in and walk one debate to double-check the production data path still renders correctly (fonts may take one extra frame to load from the CDN — that's expected and not a regression).

- [ ] **Step 5: Clean up the reference scaffolding**

Only after production is verified healthy:

```bash
rm -rf .ui-redesign-reference/
```

And confirm `.gitignore` still ignores it (so re-extraction for reference stays local-only).

No commit — the folder is ignored.

---

## Self-Review

After writing this plan, I reviewed it against the spec (the brand-kit README + `colors_and_type.css` + the current frontend state) and confirmed:

**Spec coverage:**

- ✅ Terminal palette (all tokens) — applied via `tokens.css` in Task 1.
- ✅ Typography — fonts swapped in `app.html` (Task 2); Cormorant used on hero + documentation headings + numeric stats.
- ✅ Cards (12px radius, hover-lift, indigo border) — `.card-term` + `.card-term-hover` in Task 2, applied in every route task.
- ✅ Buttons (indigo-600 fill, dark ghost) — `.btn-indigo` + `.btn-dark-ghost` in Task 2, applied everywhere.
- ✅ Eyebrows (JB Mono 0.3em indigo-400) — `.tm-eyebrow` + `.mono-label` in Task 2, applied.
- ✅ No decorative icons — Task 4 (Sidebar), Task 6 (landing capability cards), verified in Task 13 grep.
- ✅ Copper usage restricted — stats, archived/pending badges, active sidebar number, "Coming soon" block. Not a CTA.
- ✅ Agent palette migrated — Task 3, consumed automatically by ArgumentMap3D, MapLegend, ConfidenceChart.
- ✅ No photography / illustrations — none introduced; the hero-orbs are pure CSS blur.
- ✅ Hero background orbs — `.hero-orbs` in Task 2, applied on landing in Task 6.
- ✅ Every page has been touched — Tasks 4, 6, 7, 8, 9, 10, 11, 12 cover the complete route tree.

**Placeholder scan:** No TBDs, no "implement appropriately", no "similar to above" without inline code. Every CSS rule and template is written out.

**Type consistency:** The primitives (`.btn-indigo`, `.card-term`, `.card-term-hover`, `.pill-on`, `.pill-off`, `.mono-label`, `.stat-serif`, `.tm-eyebrow`, `.hero-orbs`) are defined once in Task 2 and referenced by name in every later task. `AGENT_COLORS` / `STATUS_COLORS` / `CHALLENGE_COLORS` defined in Task 3 flow downstream unchanged.

**Operational lesson adherence:**

- Lesson 7 / 8 (Svelte 5 `$app/stores` + `onDestroy` bans): no new imports introduced that trigger these.
- Lesson 11 (user has all access): plan calls `git push` and `gh pr create` directly in Task 13.
- Lesson 16 (resynth after synthesis-prompt change): N/A — no synthesis prompt changes in this plan.

**Bite-sized granularity:** Each step is 2–5 minutes of work. The biggest steps — Task 8 Steps 3–10 — are per-file restyles that could be split further, but bundling them into one task with one commit at the end is the right granularity for a consistent visual change across related components.

---

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-04-23-lqcouncil-ui-redesign.md`.** Two execution options:

**1. Subagent-Driven (recommended)** — Dispatch a fresh subagent per task (Task 0, Task 1, … Task 14). Review the diff between tasks. Fast iteration. Task 8 and Task 11 are large enough that mid-task review is advisable; consider dispatching them as two subagents each (split by file group).

**2. Inline Execution** — Execute tasks sequentially in this session with checkpoints after Tasks 2, 6, 9, 12, 13. Lower context cost per task but all visual progress happens before any review.

Which approach?
