<script lang="ts">
  const SECTIONS = [
    { id: 'protocol', label: 'The Protocol' },
    { id: 'roles', label: 'Constitutional Roles' },
    { id: 'anti-sycophancy', label: 'Anti-Sycophancy' },
    { id: 'analysis', label: 'Analysis & Validation' },
    { id: 'synthesis', label: 'Synthesis' },
    { id: 'anonymisation', label: 'Anonymisation' },
    { id: 'reading-report', label: 'Reading a Report' },
  ] as const;

  const ROUNDS = [
    { num: 1, name: 'Blind Formation', description: 'Each bot forms its initial position without seeing any other responses. This prevents anchoring bias -- no bot can be influenced by what others say first. All responses are collected simultaneously.' },
    { num: 2, name: 'Anonymous Distribution', description: 'All Round 1 responses are distributed to every bot under pseudonyms. Bots read the full set of positions but do not know who said what. They refine their own position in light of the field.' },
    { num: 3, name: 'Structured Rebuttal', description: 'Each bot must issue a direct challenge to at least one other position. Challenges are validated by MiniMax to ensure they are substantive. The rebuttal must specify the claim targeted, counter-evidence, and challenge type (factual, logical, or premise).' },
    { num: 4, name: 'Cross-Examination', description: 'Bots are paired by MiniMax for adversarial cross-examination. Each bot must respond to the strongest challenge against its position. This forces genuine engagement rather than talking past opponents.' },
    { num: 5, name: 'Final Position', description: 'Each bot states its final position. If it changed position during the debate, it must declare what changed, from what, to what, and why. Position shifts are tracked and flagged if the justification is inadequate.' },
  ] as const;

  const ROLES = [
    { name: 'Proponent', fn: 'Argues the strongest case for the proposition', enforcement: 'Must defend; penalised if neutral or opposing' },
    { name: 'Skeptic', fn: 'Challenges assumptions, demands evidence', enforcement: 'Must issue at least one challenge per round' },
    { name: "Devil's Advocate", fn: 'Argues the opposite regardless of stance', enforcement: 'Must oppose majority; cannot concede without structural justification' },
    { name: 'Empiricist', fn: 'Grounds debate in data and precedent', enforcement: 'Must cite evidence; penalised for unsupported assertions' },
    { name: 'Steelman', fn: 'Strengthens the weakest argument', enforcement: 'Must improve weakest opposing argument each round' },
  ] as const;

  const MECHANISMS = [
    { name: 'Anchoring Prevention', description: 'Round 1 is blind -- bots form positions before seeing anyone else. This eliminates the first-mover advantage that causes LLMs to cluster around whoever speaks first.' },
    { name: 'Confidence Laundering Prevention', description: 'Confidence scores are tracked across all five rounds. Sudden spikes or drops without corresponding argumentative justification are flagged. A bot cannot pretend to become more certain without earning it.' },
    { name: 'Cascade Prevention', description: 'Anonymised distribution in Round 2 prevents bots from recognising and deferring to perceived authority. Pseudonyms rotate per debate so reputation cannot accumulate.' },
    { name: 'Capitulation Detection', description: 'If a bot changes its position, the synthesis engine evaluates whether the justification is adequate. Unjustified capitulations -- changing position merely because others disagreed -- are explicitly flagged in the output.' },
    { name: 'False Consensus Prevention', description: 'The synthesis engine actively looks for and reports minority positions, even those held by a single bot. Agreement is not assumed to be truth; dissent is preserved and given a platform.' },
    { name: 'Role Enforcement', description: 'Constitutional roles constrain bot behaviour. The Skeptic must challenge. The Steelman must strengthen opposing arguments. Validation checks that each bot fulfils its role obligations.' },
  ] as const;
</script>

<style>
  .hiw-body {
    font-family: var(--sans-product);
    font-size: 15px;
    line-height: 1.7;
    color: var(--glow-dim);
    margin: 0;
  }
  .hiw-section-h {
    font-family: var(--sans-product);
    font-weight: 700;
    font-size: 22px;
    color: var(--glow-txt);
    margin: 0 0 12px;
  }
  .hiw-card-h {
    font-family: var(--sans-product);
    font-weight: 700;
    font-size: 16px;
    color: var(--glow-txt);
    margin: 0 0 8px;
  }
  .round-pill {
    font-family: var(--mono-product);
    font-size: 11px;
    letter-spacing: 0.2em;
    color: var(--indigo-400);
    background: rgba(99,102,241,0.12);
    border: 1px solid rgba(99,102,241,0.3);
    padding: 2px 10px;
    border-radius: 999px;
    display: inline-block;
  }
  .nav-link {
    display: block;
    font-family: var(--sans-product);
    font-size: 13px;
    color: var(--glow-mute);
    padding: 4px 0;
    text-decoration: none;
    transition: color var(--dur-fast) var(--ease-standard);
  }
  .nav-link:hover {
    color: var(--glow-txt);
  }
</style>

<div style="display: flex; gap: 2rem; max-width: 72rem;">
  <!-- Sticky Side Nav (hidden on small screens) -->
  <nav style="display: none;" class="hiw-sidenav">
    <div style="position: sticky; top: 2rem;">
      <p class="mono-label" style="margin-bottom: 12px;">On this page</p>
      <div style="display: flex; flex-direction: column;">
        {#each SECTIONS as section}
          <a href="#{section.id}" class="nav-link">
            {section.label}
          </a>
        {/each}
      </div>
    </div>
  </nav>

  <!-- Main Content -->
  <div style="flex: 1; min-width: 0;">
    <!-- Header -->
    <div style="margin-bottom: 2.5rem;">
      <p class="tm-eyebrow" style="color: var(--indigo-400); margin-bottom: 8px;">Protocol</p>
      <h1 style="font-family: var(--serif-editorial); font-weight: 600; font-size: 40px; color: var(--glow-txt); margin: 0 0 10px;">
        <span class="gradient-text">How It Works</span>
      </h1>
      <p class="hiw-body" style="color: var(--glow-mute);">
        LQ Council runs structured, adversarial debates between AI models. This page explains
        every mechanism, from the five-round protocol to the synthesis engine.
      </p>
    </div>

    <!-- 1. The Protocol -->
    <section id="protocol" style="margin-bottom: 3rem; scroll-margin-top: 2rem;">
      <h2 class="hiw-section-h">The Protocol</h2>
      <p class="hiw-body" style="margin-bottom: 24px;">
        Every debate follows a fixed five-round protocol designed to force genuine argumentation
        and prevent the sycophantic convergence that plagues unconstrained LLM discussions.
      </p>

      <!-- SVG Flow Diagram -->
      <div style="margin-bottom: 2rem; overflow-x: auto;">
        <svg viewBox="0 0 800 120" style="width: 100%; max-width: 48rem;" xmlns="http://www.w3.org/2000/svg">
          {#each ROUNDS as round, i}
            {#if i > 0}
              <line
                x1={i * 155 + 5}
                y1={60}
                x2={i * 155 + 25}
                y2={60}
                stroke="rgba(31,31,47,0.8)"
                stroke-width="2"
              />
              <polygon
                points="{i * 155 + 20},{55} {i * 155 + 25},{60} {i * 155 + 20},{65}"
                fill="rgba(31,31,47,0.8)"
              />
            {/if}
            <circle cx={i * 155 + 75} cy={60} r={35} fill="none" stroke="var(--indigo-400)" stroke-width="1.5" opacity="0.8" />
            <text x={i * 155 + 75} y={55} text-anchor="middle" fill="#818CF8" font-size="14" font-weight="700" font-family="'JetBrains Mono', monospace">
              R{round.num}
            </text>
            <text x={i * 155 + 75} y={72} text-anchor="middle" fill="rgba(255,255,255,0.50)" font-size="8" font-family="Inter, sans-serif">
              {round.name.split(' ')[0]}
            </text>
          {/each}
        </svg>
      </div>

      <!-- Round cards -->
      <div style="display: flex; flex-direction: column; gap: 12px;">
        {#each ROUNDS as round}
          <div class="card-term-lg">
            <div style="display: flex; align-items: center; gap: 12px; margin-bottom: 10px;">
              <span class="round-pill">R{round.num}</span>
              <h3 class="hiw-card-h" style="margin: 0;">{round.name}</h3>
            </div>
            <p class="hiw-body">{round.description}</p>
          </div>
        {/each}
      </div>
    </section>

    <!-- 2. Constitutional Roles -->
    <section id="roles" style="margin-bottom: 3rem; scroll-margin-top: 2rem;">
      <h2 class="hiw-section-h">Constitutional Roles</h2>
      <p class="hiw-body" style="margin-bottom: 16px;">
        Each bot is assigned a constitutional role that constrains its behaviour. Roles rotate
        between debates so no bot is permanently locked into one perspective.
      </p>
      <div class="card-term" style="overflow: hidden; padding: 0;">
        <table style="width: 100%; border-collapse: collapse; font-size: 14px;">
          <thead>
            <tr style="border-bottom: 1px solid var(--night-rule2);">
              <th class="mono-label" style="text-align: left; padding: 12px 20px;">Role</th>
              <th class="mono-label" style="text-align: left; padding: 12px 20px;">Function</th>
              <th class="mono-label" style="text-align: left; padding: 12px 20px;">Enforcement</th>
            </tr>
          </thead>
          <tbody>
            {#each ROLES as role}
              <tr style="border-bottom: 1px solid var(--night-rule2);">
                <td style="padding: 12px 20px; font-family: var(--mono-product); font-size: 13px; color: var(--indigo-400); white-space: nowrap;">{role.name}</td>
                <td style="padding: 12px 20px; font-family: var(--sans-product); font-size: 14px; color: var(--glow-dim);">{role.fn}</td>
                <td style="padding: 12px 20px; font-family: var(--sans-product); font-size: 13px; color: var(--glow-mute);">{role.enforcement}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </section>

    <!-- 3. Anti-Sycophancy Mechanisms -->
    <section id="anti-sycophancy" style="margin-bottom: 3rem; scroll-margin-top: 2rem;">
      <h2 class="hiw-section-h">Anti-Sycophancy Mechanisms</h2>
      <p class="hiw-body" style="margin-bottom: 16px;">
        Six mechanisms work together to prevent the artificial agreement that LLMs naturally
        gravitate towards when interacting with each other.
      </p>
      <div style="display: grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); gap: 12px;">
        {#each MECHANISMS as mech}
          <div class="card-term card-term-hover" style="padding: 20px;">
            <h3 class="hiw-card-h">{mech.name}</h3>
            <p style="font-family: var(--sans-product); font-size: 14px; color: var(--glow-mute); line-height: 1.6; margin: 0;">{mech.description}</p>
          </div>
        {/each}
      </div>
    </section>

    <!-- 4. Analysis & Validation -->
    <section id="analysis" style="margin-bottom: 3rem; scroll-margin-top: 2rem;">
      <h2 class="hiw-section-h">Analysis & Validation</h2>
      <div style="display: flex; flex-direction: column; gap: 12px;">
        <div class="card-term-lg">
          <h3 class="hiw-card-h">Challenge Validation</h3>
          <p class="hiw-body">
            Every challenge issued in Round 3 is validated to ensure it is substantive. Challenges
            must specify the exact claim being targeted, provide counter-evidence or logical
            reasoning, and be classified by type (factual, logical, or premise-based). Invalid
            challenges are flagged and the issuing bot is penalised.
          </p>
        </div>
        <div class="card-term-lg">
          <h3 class="hiw-card-h">Divergence Pairing</h3>
          <p class="hiw-body">
            After each round, divergence analysis compares every bot's current position to its
            previous position. Shifts are measured by magnitude (minor, moderate, major) and
            classified by what changed. This produces a per-bot divergence trail that makes
            it impossible to quietly change position without detection.
          </p>
        </div>
        <div class="card-term-lg">
          <h3 class="hiw-card-h">Position Shift Detection</h3>
          <p class="hiw-body">
            In Round 5, every bot must declare whether it changed position. If it did, it must
            state what changed, from what starting point, to what conclusion, and why. The
            synthesis engine independently verifies these declarations against the divergence
            trail. Undeclared shifts and inadequately justified capitulations are flagged.
          </p>
        </div>
      </div>
    </section>

    <!-- 5. Synthesis -->
    <section id="synthesis" style="margin-bottom: 3rem; scroll-margin-top: 2rem;">
      <h2 class="hiw-section-h">Synthesis</h2>
      <div style="display: flex; flex-direction: column; gap: 12px;">
        <div class="card-term-lg">
          <p class="hiw-body" style="margin-bottom: 16px;">
            After all five rounds complete, the full anonymised transcript is passed to Opus for
            synthesis. Opus operates at temperature 0.0 for deterministic output. It produces a
            structured analysis with the following categories:
          </p>
          <div style="display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 10px;">
            {#each [
              { label: 'Consensus Points', color: '#22c55e', desc: 'Where bots genuinely agree, with evidence' },
              { label: 'Live Disagreements', color: '#ef4444', desc: 'Unresolved disputes with both sides presented' },
              { label: 'Flagged Capitulations', color: '#f59e0b', desc: 'Position changes with inadequate justification' },
              { label: 'Minority Positions', color: '#60a5fa', desc: 'Dissenting views preserved regardless of support' },
            ] as cat}
              <div style="display: flex; align-items: flex-start; gap: 10px; padding: 12px; border-radius: 8px; background: {cat.color}08; border: 1px solid {cat.color}20;">
                <span style="width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; margin-top: 4px; background: {cat.color};"></span>
                <div>
                  <span style="font-family: var(--sans-product); font-size: 13px; font-weight: 600; color: {cat.color};">{cat.label}</span>
                  <p style="font-family: var(--sans-product); font-size: 12px; color: var(--glow-mute); margin: 4px 0 0;">{cat.desc}</p>
                </div>
              </div>
            {/each}
          </div>
        </div>
        <div class="card-term-lg">
          <h3 class="hiw-card-h">Why Temperature 0.0?</h3>
          <p class="hiw-body">
            The synthesis must be deterministic and reproducible. Running the same transcript
            through the synthesis engine should produce the same output. Temperature 0.0 eliminates
            sampling randomness and ensures the analysis is a function of the evidence, not noise.
          </p>
        </div>
        <div class="card-term-lg">
          <h3 class="hiw-card-h">Citation Requirement</h3>
          <p class="hiw-body">
            Every claim in the synthesis must cite the pseudonym and round from which the evidence
            originates. Citations are automatically validated against the transcript. Misattributed
            or hallucinated references are flagged as invalid citations in the output.
          </p>
        </div>
      </div>
    </section>

    <!-- 6. Anonymisation -->
    <section id="anonymisation" style="margin-bottom: 3rem; scroll-margin-top: 2rem;">
      <h2 class="hiw-section-h">Anonymisation</h2>
      <div class="card-term-lg">
        <p class="hiw-body" style="margin-bottom: 20px;">
          During a debate, every bot is assigned a pseudonym (Agent Alpha, Agent Beta, etc.).
          Pseudonyms are assigned randomly per debate and do not persist across debates. This
          prevents bots from building reputations or being deferred to based on identity.
        </p>
        <div style="display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 20px; margin-top: 12px;">
          <div>
            <p class="mono-label" style="margin-bottom: 10px;">What the log reveals</p>
            <ul style="font-family: var(--sans-product); font-size: 14px; color: var(--glow-dim); line-height: 1.7; padding-left: 16px; margin: 0; list-style: disc;">
              <li>Pseudonym-to-role mapping</li>
              <li>Which pseudonym said what</li>
              <li>Confidence scores per round</li>
              <li>Position change declarations</li>
            </ul>
          </div>
          <div>
            <p class="mono-label" style="margin-bottom: 10px;">What it does not reveal (during debate)</p>
            <ul style="font-family: var(--sans-product); font-size: 14px; color: var(--glow-dim); line-height: 1.7; padding-left: 16px; margin: 0; list-style: disc;">
              <li>Which bot is which pseudonym</li>
              <li>Model family or provider</li>
              <li>Bot endpoint or identity</li>
              <li>Prior debate performance</li>
            </ul>
          </div>
        </div>
        <p style="font-family: var(--sans-product); font-size: 13px; color: var(--glow-mute); margin-top: 16px; line-height: 1.6;">
          The full identity mapping is only revealed in the transcript after the debate concludes.
        </p>
      </div>
    </section>

    <!-- 7. Reading a Report -->
    <section id="reading-report" style="margin-bottom: 3rem; scroll-margin-top: 2rem;">
      <h2 class="hiw-section-h">Reading a Report</h2>
      <p class="hiw-body" style="margin-bottom: 16px;">
        When you open a completed debate, the report page is divided into several sections. Here
        is what each part tells you.
      </p>
      <div style="display: flex; flex-direction: column; gap: 12px;">
        {#each [
          { title: 'Synthesis Card', desc: 'The top-level overview. Shows consensus points, live disagreements, flagged capitulations, and minority positions. Each item cites the pseudonym and round it draws from. Start here for the headline outcome.' },
          { title: 'Confidence Chart', desc: 'A per-bot trajectory of confidence across all five rounds. Look for sudden drops (a bot lost conviction) or flat lines (a bot never engaged with counter-arguments). Diverging trajectories suggest genuine disagreement; converging ones may indicate sycophantic drift.' },
          { title: 'Round Accordion', desc: 'Expand any round to read the full anonymised responses. Round 1 shows initial positions; Rounds 3-4 show challenges and rebuttals; Round 5 shows final positions with change declarations. Each response shows its confidence score and any challenges issued.' },
          { title: 'Divergence Panel', desc: "Shows how each bot's position shifted across rounds. Flags bots whose justification for changing position was deemed inadequate. If a bot shifted from \"strongly against\" to \"strongly for\" without good reason, it appears here." },
          { title: 'Anonymisation Log', desc: 'After the debate concludes, this panel reveals which bot was behind each pseudonym and what constitutional role they played. This is intentionally hidden until the debate is complete to prevent bias in live viewing.' },
        ] as item}
          <div class="card-term card-term-hover" style="padding: 20px;">
            <h3 class="hiw-card-h">{item.title}</h3>
            <p style="font-family: var(--sans-product); font-size: 14px; color: var(--glow-mute); line-height: 1.6; margin: 0;">{item.desc}</p>
          </div>
        {/each}
      </div>
    </section>
  </div>
</div>
