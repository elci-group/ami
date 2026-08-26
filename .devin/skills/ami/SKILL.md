# Ami Skill

Use the `ami` CLI tool for deterministic market intelligence - discovering, scoring, and tracking potential adopters for projects.

## When to Invoke

Invoke this skill when:

- User asks to analyze a project's market potential
- User wants to find potential adopters or users for a project
- User mentions "market intelligence", "addressable market", or "prospects"
- User wants to identify similar projects or communities
- User asks about who might use a specific tool or library
- User wants to generate mailing lists or contact databases
- User asks about target audience for a project

## Commands

### Analyze a Project

```bash
ami analyze --path <project-path> --output <output-dir>
```

Options:
- `--path`: Path to project directory (default: current directory)
- `--output`: Output directory for results (default: ./ami-data)
- `--github`: Enable GitHub discovery (default: true)
- `--reddit`: Enable Reddit discovery (default: false)
- `--hackernews`: Enable Hacker News discovery (default: false)
- `--max-phrases`: Maximum search phrases to use (default: 20)

### Show Project Information

```bash
ami show-project --path <project-path>
```

### Generate Mailing Lists

```bash
ami generate-lists --data <ami-data-dir>
```

### View Database Statistics

```bash
ami stats --data <ami-data-dir>
```

## How It Works

Ami deterministically discovers prospects by:

1. Reading project state (README, Cargo.toml, package.json, git history)
2. Extracting capabilities (features, technologies, domains)
3. Generating search phrases from capabilities
4. Searching public sources (GitHub, Reddit, HN)
5. Scoring prospects by relevance (keyword overlap, technology fit, audience size)
6. Building contact database (CSV storage)
7. Generating mailing lists by development stage

## Key Principles

- **Deterministic**: No hallucinated prospects - all from actual API calls
- **Project-aware**: Derives search terms from repository state
- **Stage-aware**: Adjusts contact strategies based on development maturity
- **Non-invasive**: Only uses publicly available information

## Output Structure

```
ami-data/
├── prospects.csv           # All discovered prospects
└── lists/
    ├── alpha.csv           # Early adopters
    ├── beta.csv            # Beta testers
    ├── enterprise.csv      # Enterprise prospects
    ├── education.csv       # Educational institutions
    ├── gaming.csv          # Gaming-related projects
    ├── terminal.csv        # Terminal/software projects
    └── opensource.csv      # Open source projects
```

## Integration with Other Tools

Ami fits naturally alongside deterministic tooling:
- Cambrian (roadmap evolution)
- Deliver (release validation)
- Kaptaind (semantic versioning)
- Ami (market intelligence)

## Example Workflow

```bash
# Analyze the current project
ami analyze

# Check the results
ami stats --data ./ami-data

# Generate mailing lists for different stages
ami generate-lists --data ./ami-data
```

## Notes

- GitHub discovery is enabled by default and works immediately
- Reddit and Hacker News discovery are placeholders (would require web scraping implementation)
- The tool respects rate limits and uses configurable delays
- All data is stored locally in CSV format for easy inspection
