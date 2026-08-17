# McKinsey 方法论理论参考

> 本文件是 *McKinsey Consulting Methodology Training Manual* 的结构化纯文本提炼。
> Agent 在遇到方法论困惑、用户要求理论解释、或需要深度审视分析质量时按需 `file_read`。

---

## Phase I: Problem Structuring and Definition

### Problem Statement Worksheet

Elite strategy teams codify the objective using a rigorous Problem Statement Worksheet:

| Element | Purpose | Example |
|:--------|:--------|:--------|
| **Core Question** | SMART question (Specific, Measurable, Action-oriented, Relevant, Time-bound) | "How can the European retail division increase organic revenue by 15% over 18 months without increasing marketing spend?" |
| **Context & Complications** | Internal/external forces driving the need | "Market share eroding due to digital-first entrants offering lower prices and better UX." |
| **Success Criteria** | Binary definition of success (quantitative + qualitative) | "Achieving $100M in cost savings while maintaining CSAT > 85%." |
| **Scope & Constraints** | What is explicitly OUT of scope | "Excludes North American market; no capex > $5M." |
| **Stakeholder Map** | Decision-makers, blockers, execution teams | "Sponsor: CFO. Execution: VP Supply Chain + regional procurement." |

### The MECE Principle

- **Mutually Exclusive**: A single data point cannot logically reside in more than one category. Overlapping categories → double-counting and root-cause confusion.
- **Collectively Exhaustive**: The sum of parts must perfectly equal the whole. Revenue = Price × Volume is perfectly MECE for revenue analysis.
- **Binary MECE Pairs** (useful starting points): Financial/Non-Financial, Internal/External, Direct/Indirect, Supply-side/Demand-side.

### Issue Trees vs Hypothesis Trees

**Issue Tree (Diagnostic)**:
- Used when the team does NOT have enough data to form a hypothesis.
- Root = broad "How" or "Why" question.
- Branches = MECE sub-questions → follow data to isolate the mathematical driver.
- Example: "Why are profits declining?" → Revenues dropping? / Costs rising? → Price? Volume? / Fixed? Variable?

**Hypothesis Tree (Solution)**:
- Used when a "Day One Hypothesis" exists (educated postulation from preliminary data + expert interviews).
- Root = declarative statement (proposed answer).
- Branches = conditions that MUST be true to validate the root.
- If data invalidates ANY critical branch → discard hypothesis, formulate new one.
- Example: "Acquire Competitor X to dominate EU logistics" → Market attractive? / Target viable? / Integration capability?

### First Principles for Ambiguous Problems

When standard frameworks fail due to scale (e.g., Net Zero, Digital Transformation):
- Decompose into interdependent operational/socioeconomic requirements.
- Build a custom issue tree from scratch, not from memorized templates.
- Each branch must be discrete, assignable, and measurable.

---

## Phase II: Analytical Execution

### Custom Logic vs Standardized Frameworks

- Generic frameworks (Porter's Five Forces, SWOT, 3Cs) produce generic recommendations.
- Use standard frameworks as **mental checklists** to verify MECE completeness, NOT as the final structure.
- Build bespoke frameworks using First Principles: "What fundamental conditions must be true for THIS business to succeed in THIS market?"

**Common Baseline Frameworks** (adapt, don't copy):

| Framework | Application | MECE Components |
|:----------|:-----------|:----------------|
| Profitability | Margin erosion diagnosis | Revenue (Price × Volume × Mix) − Costs (Fixed + Variable) |
| Market Entry | New geography/product viability | Market Attractiveness, Competitive Landscape, Company Capabilities, Financial Implications |
| M&A | Acquisition rationale | Strategic Fit, Target Attractiveness, Synergies, Valuation |
| Growth Strategy | Revenue expansion | Organic (volume/price/adjacency) vs Inorganic (acquisitions/JVs) |

### The 80/20 Rule (Pareto Principle)

- 80% of strategic impact comes from 20% of variables.
- Use a **Prioritization Matrix**: Impact (magnitude) × Feasibility (data availability + implementation ease).
- High-Impact + High-Feasibility → aggressively resource.
- Low-Impact branches → ruthlessly prune ("don't boil the ocean").

### The "So What?" Test ⭐

Raw data is useless until interpreted. Every finding must pass:

```
Observation → "So What?" → Implication → Required Action

❌ "Competitor costs dropped 5%"
✅ "Competitor costs dropped 5%, MEANING they can initiate a price war,
    REQUIRING us to lock in fixed-rate supplier contracts immediately."
```

**Rule**: If an Action Title only describes data without directional implication, it fails the test.

### Data vs Assumptions

- Assumptions form scaffolding; data is the arbiter of truth.
- **Sanity Checks**: Back-of-envelope calculations to verify outputs are within logical bounds.
- If model shows 25% market capture in 12 months in a consolidated market → flag as absurd.

---

## Phase III: Executive Communication ⭐⭐

### The Pyramid Principle (Barbara Minto)

All professional communication must be structured **top-down** (not bottom-up):

```
┌───────────────────────────────────┐
│ PEAK: The Governing Thought       │  ← Final answer/recommendation
│ (e.g., "Acquire Competitor Y")    │
├───────────────────────────────────┤
│ 3 MECE Core Arguments             │  ← "Rule of 3" strictly enforced
│ 1. Market access  2. Tech IP      │
│ 3. 25% ROI in 3 years             │
├───────────────────────────────────┤
│ Evidence Base                      │  ← Raw data, charts, financials
│ (validates each core argument)     │
└───────────────────────────────────┘
```

**Resilience**: CEO has 30 seconds → reads Peak. 5 minutes → Peak + Arguments. The message is delivered regardless of time constraint.

### Horizontal Logic vs Vertical Logic

- **Horizontal Logic**: Read all slide Action Titles sequentially — do they tell a coherent story?
- **Vertical Logic**: For each slide, does the data on-page definitively prove the Action Title claim?
- Check both BEFORE generating any slides.

### SCQA / SCR Framework ⭐

Used to introduce the Pyramid Peak without triggering defensive skepticism:

| Element | Purpose | Example |
|:--------|:--------|:--------|
| **Situation** | Uncontroversial fact (puts audience in agreement) | "The global cybersecurity market is growing at 20% CAGR." |
| **Complication** | Disruption demanding action | "Our legacy on-prem architecture prevents capturing this growth; 5% share loss in 2 quarters." |
| **Question** | (Optional) The question the Complication raises | "How do we regain momentum?" |
| **Answer/Resolution** | = The Pyramid Peak | "Transition to cloud-based SaaS within 12 months via targeted acquisition." |

**Application**: Every Executive Summary and chapter opening should follow SCQA structure.

### Ghost Deck / Dot-Dash Workflow

Outline the entire narrative BEFORE opening presentation software:

- **Dots (•)**: Main storyline arguments → become slide Action Titles.
- **Dashes (-)**: Data/evidence needed to prove each Dot.

Review Dots sequentially = Horizontal Logic check.
Review Dashes under each Dot = Vertical Logic check.

### Action Titles

- ❌ Topic Title: "Q3 Revenue Analysis" / "Market Overview"
- ✅ Action Title: "Q3 revenue declined 12% due to supply chain bottlenecks in the European division"
- **Rules**: Complete declarative sentence, active voice, ≤15 words, max 2 lines.
- **If a title needs "and" to connect two thoughts** → split into two slides.

### Pre-Wiring

- Never present to a surprised room.
- Socialize findings 1-on-1 with key decision-makers before the formal presentation.
- Surface and address objections in private → formal presentation becomes ratification, not debate.

---

## Phase IV: Deliverable Anatomy

### Standard Deck Structure

1. **Executive Summary** (1-3 slides): SCQA + "Bold-Bullet" format. Must be completely self-sufficient.
2. **Body / Insights** (main content): Each slide = one supporting argument. Action Titles read as a continuous logical proof.
3. **Recommendations & Next Steps**: 100-day plans, assigned owners, capital requirements, implementation risks.
4. **Appendix**: Deep-dive models, market research, methodological explanations (keeps body clean).

### Formatting Discipline

- **Grid Alignment**: Action titles at exact same position across all slides (no "jumping").
- **Typography Hierarchy**: Title 18-20pt → Body 12-14pt → Source 8-10pt. Never shrink to fit; simplify or split.
- **Color Restraint**: Max 3-4 corporate colors. Color highlights focal point only (client = blue, competitors = gray).
- **Data-to-Ink Ratio**: No 3D charts, clip art, stock photos, decorative elements. Max signal, min noise.
- **Waterfall/Bridge Charts**: Show how metric A evolves to metric B, isolating variance drivers.
- **Source Lines**: Every chart must have clearly defined source at bottom (small font).

---

## Phase V: Meta-Level Thinking Patterns

### Synthesis vs Summary ⭐

| Type | Nature | Example |
|:-----|:-------|:--------|
| **Summary** | Passive, backward-looking recitation of facts | "Costs rose 10%, revenue fell 5%." |
| **Synthesis** | Active, forward-looking, decisive extraction of meaning | "Rising costs + falling revenue = structurally unsustainable model → requires 15% headcount reduction and pivot to premium." |

**Rule**: Every conclusion in the report must be Synthesis, not Summary.

### Obligation to Dissent

- Junior analysts are EXPECTED to challenge Senior Partners if data contradicts assumptions.
- Prevents confirmation bias and hierarchical groupthink.
- Final recommendation must be the product of intellectual pressure-testing, not consensus.

### Reusable Mental Models

- **Three Horizons of Growth**: H1 = optimize core. H2 = scale adjacencies (2-5 years). H3 = transformative innovation (5-10 years).
- **GE-McKinsey Matrix**: 9-box grid. Y-axis = Industry Attractiveness. X-axis = Competitive Strength. Guides invest/hold/divest decisions.

---

## Phase VI: Common Pitfalls

1. **Forced Frameworking**: Shoehorning nuanced problems into memorized frameworks (Porter's etc.) when context demands bespoke logic.
2. **False MECE Exclusivity**: Categories that appear distinct but overlap (e.g., "Millennials" + "High-Income" + "Urban" — one consumer belongs to all three).
3. **Inverting the Pyramid**: Building suspense to a dramatic conclusion at page 50. Executives interrupt, run out of time, or ask tangential questions before reaching it.
4. **Analysis Paralysis (Boiling the Ocean)**: Pursuing 100% certainty by analyzing every minor variable → blown deadlines, no strategic direction.
