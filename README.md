# Ami

**Addressable Market Identifier** — a deterministic market-intelligence CLI
for discovering, scoring, and tracking likely adopters of a project.

Ami reads the project you point it at, derives search hypotheses, discovers
public prospects, scores the matches, and writes a portable CSV dataset. Groq
inference is optional: when `GROQ_API_KEY` is available, it contributes extra
search hypotheses; the deterministic pipeline remains the fallback.

## At a glance

| Stage | What Ami does | Output |
|---|---|---|
| Understand | Reads README, Cargo/npm metadata, and Git state | Project profile |
| Hypothesize | Generates deterministic phrases, optionally enriched by Groq | Ranked phrases |
| Discover | Queries enabled public sources | Prospect records |
| Score | Applies keyword, technology, domain, activity, and audience signals | Confidence scores |
| Package | Saves CSV data and segmented mailing lists | `ami-data/` |

## Install

```bash
cargo install --path .
```

For development, use `cargo run --` in place of `ami`.

## Quick start

```bash
# Analyze the current project; GitHub discovery is enabled by default.
ami analyze --path . --output ./ami-data

# Inspect the project profile without doing discovery.
ami show-project --path .

# Rebuild lists or inspect a previous run.
ami generate-lists --data ./ami-data
ami stats --data ./ami-data
```

### Optional Groq inference

Set `GROQ_API_KEY` to add model-generated market hypotheses through Groq’s
OpenAI-compatible chat-completions API. `GROQ_MODEL` is optional and defaults
to `llama-3.3-70b-versatile`.

```bash
export GROQ_API_KEY="your-key"
export GROQ_MODEL="llama-3.3-70b-versatile"  # optional
ami analyze --path ./my-project
```

The key is read only from the environment and is never printed. If the key is
missing or the request fails, Ami reports a warning and continues with its
deterministic phrases.

## Commands and options

### `ami analyze`

Runs the complete analysis pipeline.

| Option | Default | Meaning |
|---|---:|---|
| `--path <DIR>` | `.` | Project directory to inspect |
| `--output <DIR>` | `./ami-data` | Destination for CSV results |
| `--github <BOOL>` | `true` | Enable GitHub repository discovery |
| `--reddit <BOOL>` | `false` | Enable Reddit discovery |
| `--hackernews <BOOL>` | `false` | Enable Hacker News discovery |
| `--devto <BOOL>` | `false` | Enable Dev.to article discovery |
| `--stackoverflow <BOOL>` | `false` | Enable Stack Overflow discovery |
| `--max-phrases <N>` | `20` | Maximum phrases sent to discovery |

Boolean options accept explicit values, for example:

```bash
ami analyze --github=false --reddit=true --max-phrases 10
```

### Supporting commands

| Command | Purpose |
|---|---|
| `ami show-project --path <DIR>` | Display the detected profile, capabilities, and keywords |
| `ami generate-lists --data <DIR>` | Rebuild all mailing-list CSVs from `prospects.csv` |
| `ami stats --data <DIR>` | Show prospect, confidence, contact, and list counts |
| `ami brand status --path <DIR>` | Inspect `.brandi/` completeness and audience coverage |
| `ami brand monitor --path <DIR> --data <DIR>` | Compare research with the Brandi audience genome |
| `ami brand maintain --path <DIR> --data <DIR> --apply` | Apply evidence-backed audience/narrative additions |

## Terminal output

Human-readable commands use consistent headings, status markers, aligned
Unicode tables, and ANSI colour when stdout is an interactive terminal.
Colour is automatically disabled when output is piped, when `TERM=dumb`, or
when `NO_COLOR` is set. Force or disable it explicitly with:

```bash
AMI_COLOR=1 ami show-project --path .       # force colour
NO_COLOR=1 ami show-project --path .        # plain output
ami analyze --path . | tee analysis.log     # plain output for logs
```

The tabular output is intentionally plain-text compatible, so it remains
readable in CI logs and can be copied into issue trackers.

## Pipeline

```text
Project files
    │
    ▼
Profile + capability extraction
    │
    ▼
Deterministic phrases ──┐
                        ├── ranked search phrases
Optional Groq phrases ──┘
    │
    ▼
Public-source discovery
    │
    ▼
Scoring and market metrics
    │
    ▼
CSV database + mailing lists
```

## Brandi alignment

When a project contains a Brandi genome, Ami can monitor and maintain the
audience side of `.brandi/` from current market evidence. Maintenance is
plan-first: `brand monitor` and `brand maintain` without `--apply` only report
pending segments and narratives. `--apply` writes `.brandi/audience.yaml`,
preserving existing entries and adding only missing evidence-derived entries.

```bash
ami brand status --path ./my-project
ami brand monitor --path ./my-project --data ./ami-data
ami brand maintain --path ./my-project --data ./ami-data --apply
```

Goblin is used, when installed, for bounded deterministic classification of
research signals. Missing Goblin or an ambiguous classification falls back to
Ami's platform-derived segment mapping; Goblin is never used to invent
prospects or write files.

## Research database

Each analysis attempts to record the project, search phrases, prospects, and
provenance edges in Padagonia. Configure the local HTTP graph store with:

```bash
export PADAGONIA_URL="http://127.0.0.1:7373"
export PADAGONIA_API_KEY="..."       # if the server requires bearer auth
```

The local Padagonia server requires a non-empty API key before it will bind:

```bash
PADAGONIA__SERVER__API_KEY="a-local-key-of-at-least-16-bytes" \
  padagonia server
```

Padagonia failures are reported without discarding Ami's CSV output. CSV is a
portable export; Padagonia is the provenance-rich research database when the
service is available.

Every prospect record includes its source URL, platform, match reasons,
confidence, audience estimate, contact channels, and timestamps. Ami does not
invent prospects: they must come from an enabled public discovery source.

## Data layout

```text
ami-data/
├── prospects.csv
└── lists/
    ├── alpha.csv
    ├── beta.csv
    ├── enterprise.csv
    ├── education.csv
    ├── gaming.csv
    ├── terminal.csv
    └── opensource.csv
```

## Scoring model

| Signal | Weight | Purpose |
|---|---:|---|
| Keyword overlap | 30% | Matches project keywords to prospect text |
| Technology overlap | 20% | Matches languages, frameworks, and technologies |
| Domain overlap | 25% | Matches project domains and use cases |
| Repository activity | 15% | Uses audience/activity size with logarithmic scaling |
| Audience relevance | 10% | Rewards available, usable contact channels |
| Prior interactions | 0% | Reserved for future relationship history |

## Development stages

Stages are inferred from project state and capabilities and can guide list
selection:

| Stage | Typical audience |
|---|---|
| Prototype | Friends, hackers, and early communities |
| MVP | Hackers, Reddit, Discord, and early adopters |
| Alpha | Hackers, communities, and early adopters |
| Beta | Maintainers, companies, newsletters, and Discord |
| Release Candidate | Maintainers, companies, newsletters, and bloggers |
| Stable | Enterprise, conferences, universities, and companies |

## Current source coverage

| Source | Status | Notes |
|---|---|---|
| GitHub | Available | Repository search with stars and owner links |
| Reddit | Available | Public JSON search endpoint |
| Hacker News | Available | Algolia search over stories |
| Dev.to | Available | Public article search |
| Stack Overflow | Available | Stack Exchange relevance search |

## Development checks

```bash
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Kaptaind can track and analyze changes before committing:

```bash
kaptaind-cli analyze -r .
```

## Related tooling

```text
Cambrian  → roadmap evolution
Deliver   → release validation
Kaptaind  → semantic versioning and change tracking
Ami       → market intelligence
```

## License

MIT
# ami
