use ami::brandi;
use ami::database::ContactDatabase;
use ami::discovery::{
    DevToDiscovery, DiscoveryEngine, GitHubDiscovery, HackerNewsDiscovery, RedditDiscovery,
    StackOverflowDiscovery,
};
use ami::frontier::CapabilityFrontier;
use ami::gates::{self, ValidationOutcome};
use ami::inference::RoutedInference;
use ami::output::Output;
use ami::padagonia::PadagoniaClient;
use ami::project_reader::ProjectReader;
use ami::scoring::ScoringModel;
use ami::search_generator::SearchGenerator;
use ami::trajectory::Trajectory;
use ami::transformation;
use ami::types::{
    CapabilityStatus, EvidenceConfidence, GateStatus, PrClassification, TransformationCandidate,
};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "ami")]
#[command(about = "Addressable Market Identifier - Deterministically discovers, scores, and tracks potential adopters for a project", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Analyze a project and generate market intelligence
    Analyze {
        /// Path to the project directory
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// Output directory for results
        #[arg(short, long, default_value = "./ami-data")]
        output: PathBuf,

        /// Enable GitHub discovery
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        github: bool,

        /// Enable Reddit discovery
        #[arg(long, default_value_t = false, action = clap::ArgAction::Set)]
        reddit: bool,

        /// Enable Hacker News discovery
        #[arg(long, default_value_t = false, action = clap::ArgAction::Set)]
        hackernews: bool,

        /// Enable Dev.to article discovery
        #[arg(long, default_value_t = false, action = clap::ArgAction::Set)]
        devto: bool,

        /// Enable Stack Overflow discovery
        #[arg(long, default_value_t = false, action = clap::ArgAction::Set)]
        stackoverflow: bool,

        /// Maximum number of search phrases to use
        #[arg(long, default_value = "20")]
        max_phrases: usize,
    },

    /// Generate mailing lists from existing prospect database
    GenerateLists {
        /// Path to the ami data directory
        #[arg(short, long, default_value = "./ami-data")]
        data: PathBuf,
    },

    /// Show database statistics
    Stats {
        /// Path to the ami data directory
        #[arg(short, long, default_value = "./ami-data")]
        data: PathBuf,
    },

    /// Show project information
    ShowProject {
        /// Path to the project directory
        #[arg(short, long, default_value = ".")]
        path: PathBuf,
    },

    /// Inspect, monitor, or maintain the project's Brandi genome
    Brand {
        #[command(subcommand)]
        command: BrandCommands,
    },

    /// Map the capability frontier and evaluate transformation candidates
    Evolve {
        #[command(subcommand)]
        command: EvolveCommands,
    },
}

#[derive(Subcommand)]
enum EvolveCommands {
    /// Retrieve recorded transformation proposals from Padagonia
    History {},
    /// Show existing, latent, and reachable capabilities
    Frontier {
        #[arg(short, long, default_value = ".")]
        path: PathBuf,
    },
    /// Evaluate a transformation candidate against the decision gates
    Propose {
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// What is being proposed
        #[arg(long)]
        description: String,

        /// The outcome this transformation is meant to produce
        #[arg(long)]
        outcome: String,

        /// The capability this transformation requires
        #[arg(long, default_value = "")]
        capability: String,

        /// Status of that capability: existing, latent, reachable, speculative
        #[arg(long, default_value = "speculative")]
        capability_status: String,

        /// How the market/demand evidence was obtained: observed, stated, inferred, predicted, none
        #[arg(long, default_value = "none")]
        evidence: String,

        /// Base git revision to diff from (defaults to HEAD)
        #[arg(long)]
        base: Option<String>,

        /// Head git revision to diff to (defaults to the working tree)
        #[arg(long)]
        head: Option<String>,

        /// Persist the proposal and gate results to Padagonia
        #[arg(long)]
        record: bool,

        /// Run `cargo test` in `path` and feed the result into gate 8
        #[arg(long)]
        validate: bool,
    },
    /// Show cumulative transformation distance across recent commits
    Trajectory {
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// Number of commits to walk back from HEAD
        #[arg(short, long, default_value_t = 10)]
        limit: usize,

        /// Stop walking at this revision (exclusive)
        #[arg(long)]
        since: Option<String>,
    },
}

#[derive(Subcommand)]
enum BrandCommands {
    /// Report `.brandi` completeness and audience/narrative counts
    Status {
        #[arg(short, long, default_value = ".")]
        path: PathBuf,
    },
    /// Compare addressable-market research with `.brandi/audience.yaml`
    Monitor {
        #[arg(short, long, default_value = ".")]
        path: PathBuf,
        #[arg(short, long, default_value = "./ami-data")]
        data: PathBuf,
    },
    /// Plan genome amendments; write them only with --apply
    Maintain {
        #[arg(short, long, default_value = ".")]
        path: PathBuf,
        #[arg(short, long, default_value = "./ami-data")]
        data: PathBuf,
        #[arg(long)]
        apply: bool,
    },
}

struct AnalyzeOptions {
    github: bool,
    reddit: bool,
    hackernews: bool,
    devto: bool,
    stackoverflow: bool,
    max_phrases: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Analyze {
            path,
            output,
            github,
            reddit,
            hackernews,
            devto,
            stackoverflow,
            max_phrases,
        } => {
            analyze_project(
                path,
                output,
                AnalyzeOptions {
                    github,
                    reddit,
                    hackernews,
                    devto,
                    stackoverflow,
                    max_phrases,
                },
            )
            .await
        }
        Commands::GenerateLists { data } => generate_mailing_lists(data),
        Commands::Stats { data } => show_stats(data),
        Commands::ShowProject { path } => show_project(path),
        Commands::Brand { command } => match command {
            BrandCommands::Status { path } => brand_status(path),
            BrandCommands::Monitor { path, data } => monitor_brand(path, data),
            BrandCommands::Maintain { path, data, apply } => maintain_brand(path, data, apply),
        },
        Commands::Evolve { command } => match command {
            EvolveCommands::History {} => {
                match PadagoniaClient::from_env() {
                    Ok(client) => match client.transformation_history().await {
                        Ok(history) => println!("{}", serde_json::to_string_pretty(&history)?),
                        Err(_) => {
                            eprintln!("Padagonia query failed; recorded history is unavailable")
                        }
                    },
                    Err(_) => {
                        eprintln!("Padagonia query failed; client configuration is unavailable")
                    }
                }
                Ok(())
            }
            EvolveCommands::Frontier { path } => evolve_frontier(path),
            EvolveCommands::Propose {
                path,
                description,
                outcome,
                capability,
                capability_status,
                evidence,
                base,
                head,
                record,
                validate,
            } => {
                evolve_propose(
                    path,
                    description,
                    outcome,
                    capability,
                    capability_status,
                    evidence,
                    base,
                    head,
                    record,
                    validate,
                )
                .await
            }
            EvolveCommands::Trajectory { path, limit, since } => {
                evolve_trajectory(path, limit, since)
            }
        },
    }
}

async fn analyze_project(path: PathBuf, output: PathBuf, options: AnalyzeOptions) -> Result<()> {
    let ui = Output::new();
    println!(
        "{}",
        ui.heading("🔍", format!("Analyzing project at: {}", path.display()))
    );

    // Read project
    println!("{}", ui.heading("📖", "Reading project state"));
    let project = ProjectReader::read_from_path(&path).context("Failed to read project")?;

    println!(
        "{}",
        ui.table(
            &[
                "Project",
                "Description",
                "Stage",
                "Capabilities",
                "Keywords"
            ],
            &[vec![
                project.name.clone(),
                project.description.clone(),
                format!("{:?}", project.development_stage),
                project.capabilities.len().to_string(),
                project.keywords.len().to_string(),
            ]],
        )
    );

    // Generate search phrases
    println!("{}", ui.heading("🔎", "Generating search phrases"));
    let mut all_phrases = SearchGenerator::generate(&project);

    // Add hypotheses through operator-defined inference routing. The
    // deterministic generator remains the fallback for local/offline runs.
    match RoutedInference::from_env().context("failed to configure inference routing")? {
        Some(inference) => match inference
            .infer_search_phrases(&project, options.max_phrases)
            .await
        {
            Ok(inferred) => {
                println!(
                    "{}",
                    ui.success(format!(
                        "Configured inference enabled · {} model-generated hypotheses",
                        inferred.len()
                    ))
                );
                all_phrases.extend(inferred);
            }
            Err(error) => {
                eprintln!(
                    "{}",
                    ui.warning(format!(
                        "Inference unavailable; using deterministic phrases: {error}"
                    ))
                );
            }
        },
        None => println!(
            "{}",
            ui.info("No inference credentials configured; using deterministic search phrases")
        ),
    }

    all_phrases.sort_by(|a, b| {
        b.weight
            .partial_cmp(&a.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut seen_phrases = HashSet::new();
    all_phrases.retain(|phrase| seen_phrases.insert(phrase.phrase.to_lowercase()));
    let phrases: Vec<_> = all_phrases.into_iter().take(options.max_phrases).collect();

    println!(
        "{}",
        ui.success(format!("Generated {} search phrases", phrases.len()))
    );
    let phrase_rows = phrases
        .iter()
        .take(5)
        .enumerate()
        .map(|(index, phrase)| {
            vec![
                (index + 1).to_string(),
                phrase.phrase.clone(),
                format!("{:.2}", phrase.weight),
            ]
        })
        .collect::<Vec<_>>();
    println!(
        "{}",
        ui.table(&["#", "Search phrase", "Weight"], &phrase_rows)
    );

    // Set up discovery engine
    println!("{}", ui.heading("🌐", "Setting up discovery sources"));
    let mut discovery_engine = DiscoveryEngine::new();

    if options.github {
        discovery_engine = discovery_engine.add_source(Box::new(GitHubDiscovery::new(true, 1000)));
        println!("   {} GitHub", ui.badge("enabled", true));
    }
    if options.reddit {
        discovery_engine = discovery_engine.add_source(Box::new(RedditDiscovery::new(true, 2000)));
        println!("   {} Reddit", ui.badge("enabled", true));
    }
    if options.hackernews {
        discovery_engine =
            discovery_engine.add_source(Box::new(HackerNewsDiscovery::new(true, 2000)));
        println!("   {} Hacker News", ui.badge("enabled", true));
    }
    if options.devto {
        discovery_engine = discovery_engine.add_source(Box::new(DevToDiscovery::new(true, 1500)));
        println!("   {} Dev.to", ui.badge("enabled", true));
    }
    if options.stackoverflow {
        discovery_engine =
            discovery_engine.add_source(Box::new(StackOverflowDiscovery::new(true, 1200)));
        println!("   {} Stack Overflow", ui.badge("enabled", true));
    }

    // Discover prospects
    println!("{}", ui.heading("🔍", "Discovering prospects"));
    let mut prospects = discovery_engine
        .discover(&phrases)
        .await
        .context("Failed to discover prospects")?;

    let mut seen_urls = HashSet::new();
    prospects.retain(|prospect| seen_urls.insert(prospect.url.clone()));

    println!(
        "{}",
        ui.success(format!("Found {} prospects", prospects.len()))
    );

    // Score prospects
    println!("{}", ui.heading("📊", "Scoring prospects"));
    let scoring_model = ScoringModel::with_default_weights();

    for prospect in &mut prospects {
        scoring_model.score_prospect(&project, prospect);
    }

    // Sort by confidence
    prospects.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    println!("{}", ui.heading("🏆", "Top prospects"));
    let prospect_rows = prospects
        .iter()
        .take(5)
        .enumerate()
        .map(|(index, prospect)| {
            vec![
                (index + 1).to_string(),
                prospect.name.clone(),
                format!("{:?}", prospect.platform),
                format!("{:.0}%", prospect.confidence * 100.0),
                prospect.url.clone(),
            ]
        })
        .collect::<Vec<_>>();
    println!(
        "{}",
        ui.table(
            &["#", "Prospect", "Platform", "Confidence", "URL"],
            &prospect_rows
        )
    );

    // Calculate market score
    println!(
        "{}",
        ui.heading("📈", "Calculating addressable market score")
    );
    let market_score = scoring_model.calculate_market_score(&prospects);

    println!(
        "{}",
        ui.table(
            &[
                "Potential users",
                "Reachable",
                "High confidence",
                "Immediate contacts"
            ],
            &[vec![
                market_score.potential_users.to_string(),
                market_score.reachable_users.to_string(),
                market_score.high_confidence.to_string(),
                market_score.immediate_contacts.to_string(),
            ]],
        )
    );

    // Save to database
    println!("{}", ui.heading("💾", "Saving to database"));
    let db = ContactDatabase::new(&output);
    db.initialize().context("Failed to initialize database")?;

    db.save_prospects(&prospects)
        .context("Failed to save prospects")?;

    // Generate mailing lists
    println!("{}", ui.heading("📋", "Generating mailing lists"));
    db.generate_all_lists(&prospects)
        .context("Failed to generate mailing lists")?;

    println!("{}", ui.heading("🧠", "Recording research in Padagonia"));
    let padagonia_result = match PadagoniaClient::from_env() {
        Ok(client) => client.record_analysis(&project, &phrases, &prospects).await,
        Err(error) => Err(error.context("failed to configure Padagonia")),
    };
    match padagonia_result {
        Ok(recorded) => println!(
            "{}",
            ui.success(format!("Recorded {} research nodes", recorded))
        ),
        Err(error) => eprintln!(
            "{}",
            ui.warning(format!(
                "Padagonia unavailable; CSV remains authoritative for this run: {error}"
            ))
        ),
    }

    println!("{}", ui.success("Analysis complete"));
    println!(
        "{}",
        ui.info(format!("Data saved to: {}", output.display()))
    );

    Ok(())
}

fn generate_mailing_lists(data: PathBuf) -> Result<()> {
    let ui = Output::new();
    println!(
        "{}",
        ui.heading(
            "📋",
            format!("Generating mailing lists from: {}", data.display())
        )
    );

    let db = ContactDatabase::new(&data);
    let prospects = db.load_prospects().context("Failed to load prospects")?;

    println!(
        "{}",
        ui.info(format!("Loaded {} prospects", prospects.len()))
    );

    db.generate_all_lists(&prospects)
        .context("Failed to generate mailing lists")?;

    println!("{}", ui.success("Mailing lists generated"));

    Ok(())
}

fn show_stats(data: PathBuf) -> Result<()> {
    let ui = Output::new();
    println!(
        "{}",
        ui.heading("📊", format!("Database statistics for: {}", data.display()))
    );

    let db = ContactDatabase::new(&data);
    let stats = db.get_stats().context("Failed to get database stats")?;

    println!(
        "{}",
        ui.table(
            &[
                "Prospects",
                "High confidence",
                "With contacts",
                "Mailing lists"
            ],
            &[vec![
                stats.total_prospects.to_string(),
                stats.high_confidence.to_string(),
                stats.with_contacts.to_string(),
                stats.mailing_list_count.to_string(),
            ]],
        )
    );

    Ok(())
}

fn show_project(path: PathBuf) -> Result<()> {
    let ui = Output::new();
    println!(
        "{}",
        ui.heading("📖", format!("Project information for: {}", path.display()))
    );

    let project = ProjectReader::read_from_path(&path).context("Failed to read project")?;

    println!(
        "{}",
        ui.table(
            &["Field", "Value"],
            &[
                vec!["Name".to_string(), project.name],
                vec!["Description".to_string(), project.description],
                vec![
                    "Repository".to_string(),
                    format!("{:?}", project.repository_url)
                ],
                vec!["Website".to_string(), format!("{:?}", project.website_url)],
                vec![
                    "Development stage".to_string(),
                    format!("{:?}", project.development_stage)
                ],
                vec![
                    "Last analyzed".to_string(),
                    project.last_analyzed.to_string()
                ],
                vec!["Commit".to_string(), format!("{:?}", project.commit_hash)],
            ],
        )
    );

    let capability_rows = project
        .capabilities
        .iter()
        .take(10)
        .enumerate()
        .map(|(index, cap)| {
            vec![
                (index + 1).to_string(),
                cap.name.clone(),
                format!("{:?}", cap.category),
                format!("{:.0}%", cap.confidence * 100.0),
            ]
        })
        .collect::<Vec<_>>();
    println!(
        "\n{}",
        ui.heading(
            "🔧",
            format!("Capabilities · {} total", project.capabilities.len())
        )
    );
    println!(
        "{}",
        ui.table(
            &["#", "Capability", "Category", "Confidence"],
            &capability_rows
        )
    );
    println!(
        "\n{}",
        ui.heading("🔑", format!("Keywords · {} total", project.keywords.len()))
    );
    println!(
        "{}",
        if project.keywords.is_empty() {
            "(none)".to_string()
        } else {
            project.keywords.join(", ")
        }
    );

    Ok(())
}

fn brand_status(path: PathBuf) -> Result<()> {
    let ui = Output::new();
    let status = brandi::status(&path).context("failed to inspect Brandi genome")?;
    println!(
        "{}",
        ui.heading("🎨", format!("Brandi status for {}", path.display()))
    );
    println!(
        "{}",
        ui.table(
            &["Configured", "Segments", "Narratives", "Missing files"],
            &[vec![
                status.configured.to_string(),
                status.segments.to_string(),
                status.narratives.to_string(),
                if status.missing_files.is_empty() {
                    "none".to_string()
                } else {
                    status.missing_files.join(", ")
                },
            ]],
        )
    );
    Ok(())
}

fn monitor_brand(path: PathBuf, data: PathBuf) -> Result<()> {
    let ui = Output::new();
    let project = ProjectReader::read_from_path(&path).context("failed to read project")?;
    let prospects = ContactDatabase::new(&data)
        .load_prospects()
        .context("failed to load research")?;
    let status = brandi::status(&path).context("failed to inspect Brandi genome")?;
    let plan = brandi::plan_maintenance(&path, &project, &prospects, false)
        .context("failed to compare market research with Brandi")?;
    println!(
        "{}",
        ui.heading("👁", format!("Monitoring Brandi for {}", project.name))
    );
    println!(
        "{}",
        ui.table(
            &[
                "Genome",
                "Segments",
                "Narratives",
                "Pending segments",
                "Pending narratives"
            ],
            &[vec![
                if status.configured {
                    "valid"
                } else {
                    "incomplete"
                }
                .to_string(),
                status.segments.to_string(),
                status.narratives.to_string(),
                plan.new_segments.len().to_string(),
                plan.additions.len().to_string(),
            ]]
        )
    );
    Ok(())
}

fn maintain_brand(path: PathBuf, data: PathBuf, apply: bool) -> Result<()> {
    let ui = Output::new();
    let project = ProjectReader::read_from_path(&path).context("failed to read project")?;
    let prospects = ContactDatabase::new(&data)
        .load_prospects()
        .context("failed to load research")?;
    let plan = brandi::plan_maintenance(&path, &project, &prospects, apply)
        .context("failed to plan Brandi maintenance")?;
    println!(
        "{}",
        ui.heading(
            "🛠",
            if apply {
                "Maintaining Brandi audience"
            } else {
                "Brandi maintenance plan"
            }
        )
    );
    println!(
        "{}",
        ui.table(
            &["New segments", "New narratives", "Applied", "File"],
            &[vec![
                plan.new_segments.len().to_string(),
                plan.additions.len().to_string(),
                plan.applied.to_string(),
                plan.path.display().to_string(),
            ]]
        )
    );
    if !apply && (!plan.new_segments.is_empty() || !plan.additions.is_empty()) {
        println!(
            "{}",
            ui.info("Re-run with --apply to amend .brandi/audience.yaml")
        );
    }
    Ok(())
}

fn parse_capability_status(value: &str) -> Result<CapabilityStatus> {
    match value.to_lowercase().as_str() {
        "existing" => Ok(CapabilityStatus::Existing),
        "latent" => Ok(CapabilityStatus::Latent),
        "reachable" => Ok(CapabilityStatus::Reachable),
        "speculative" => Ok(CapabilityStatus::Speculative),
        other => anyhow::bail!(
            "unknown --capability-status '{other}'; expected existing, latent, reachable, or speculative"
        ),
    }
}

fn parse_evidence_confidence(value: &str) -> Result<EvidenceConfidence> {
    match value.to_lowercase().as_str() {
        "observed" => Ok(EvidenceConfidence::Observed),
        "stated" => Ok(EvidenceConfidence::Stated),
        "inferred" => Ok(EvidenceConfidence::Inferred),
        "predicted" => Ok(EvidenceConfidence::Predicted),
        "none" => Ok(EvidenceConfidence::None),
        other => anyhow::bail!(
            "unknown --evidence '{other}'; expected observed, stated, inferred, predicted, or none"
        ),
    }
}

fn evolve_frontier(path: PathBuf) -> Result<()> {
    let ui = Output::new();
    let project = ProjectReader::read_from_path(&path).context("Failed to read project")?;
    let frontier = CapabilityFrontier::analyze(&project, &path);

    println!(
        "{}",
        ui.heading("🗺", format!("Capability frontier for {}", project.name))
    );

    println!(
        "\n{}",
        ui.heading(
            "✅",
            format!("Existing · {} total", frontier.existing.len())
        )
    );
    println!(
        "{}",
        ui.table(
            &["Capability", "Rationale"],
            &frontier
                .existing
                .iter()
                .map(|entry| vec![entry.capability.clone(), entry.rationale.clone()])
                .collect::<Vec<_>>(),
        )
    );

    println!(
        "\n{}",
        ui.heading("🌱", format!("Latent · {} total", frontier.latent.len()))
    );
    println!(
        "{}",
        ui.table(
            &["Capability", "Rationale"],
            &frontier
                .latent
                .iter()
                .map(|entry| vec![entry.capability.clone(), entry.rationale.clone()])
                .collect::<Vec<_>>(),
        )
    );

    println!(
        "\n{}",
        ui.heading(
            "🧭",
            format!("Reachable · {} total", frontier.reachable.len())
        )
    );
    println!(
        "{}",
        ui.table(
            &["Capability", "Rationale"],
            &frontier
                .reachable
                .iter()
                .map(|entry| vec![entry.capability.clone(), entry.rationale.clone()])
                .collect::<Vec<_>>(),
        )
    );

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn evolve_propose(
    path: PathBuf,
    description: String,
    outcome: String,
    capability: String,
    capability_status: String,
    evidence: String,
    base: Option<String>,
    head: Option<String>,
    record: bool,
    validate: bool,
) -> Result<()> {
    let ui = Output::new();
    let capability_status = parse_capability_status(&capability_status)?;
    let evidence_confidence = parse_evidence_confidence(&evidence)?;

    let candidate = TransformationCandidate {
        id: format!("candidate-{}", unix_timestamp_secs()),
        description,
        desired_outcome: outcome,
        required_capability: capability,
        capability_status,
        market_evidence: Vec::new(),
        evidence_confidence,
    };

    println!("{}", ui.heading("📐", "Measuring transformation distance"));
    let distance = transformation::measure(&path, base.as_deref(), head.as_deref())
        .context("Failed to measure transformation distance")?;

    println!(
        "{}",
        ui.table(
            &["Axis", "Distance"],
            &[
                vec![
                    "Component".to_string(),
                    format!("{:.2}", distance.component_distance)
                ],
                vec![
                    "Architecture".to_string(),
                    format!("{:.2}", distance.architecture_distance)
                ],
                vec![
                    "Capability".to_string(),
                    format!("{:.2}", distance.capability_distance)
                ],
                vec![
                    "Product identity".to_string(),
                    format!("{:.2}", distance.product_identity_distance)
                ],
                vec![
                    "Overall".to_string(),
                    format!("{:.2} ({})", distance.overall(), distance.band())
                ],
            ],
        )
    );

    let classification = PrClassification::from_distance(&distance);
    println!(
        "\n{}",
        ui.info(format!(
            "Evolutionary classification: {}",
            classification.as_str()
        ))
    );

    let validation = if validate {
        println!("\n{}", ui.heading("🧪", "Running cargo test"));
        run_validation(&path)
    } else {
        ValidationOutcome::NotRun
    };

    println!("\n{}", ui.heading("🚦", "Decision gates"));
    let gate_results = gates::evaluate(&candidate, &distance, &validation);
    let gate_rows = gate_results
        .iter()
        .map(|gate| {
            let status = match gate.status {
                GateStatus::Pass => ui.badge("pass", true),
                GateStatus::Fail => ui.badge("fail", false),
                GateStatus::NotEvaluated => ui.badge("not evaluated", false),
            };
            vec![gate.gate.clone(), status, gate.rationale.clone()]
        })
        .collect::<Vec<_>>();
    println!("{}", ui.table(&["Gate", "Status", "Rationale"], &gate_rows));

    let any_fail = gate_results
        .iter()
        .any(|gate| gate.status == GateStatus::Fail);
    if any_fail {
        println!(
            "\n{}",
            ui.warning("One or more gates failed; Ami should reject or decompose this candidate before implementing it")
        );
    } else {
        println!(
            "\n{}",
            ui.success("No gate failures; remaining NotEvaluated gates still require manual evidence before implementation")
        );
    }

    if record {
        println!("\n{}", ui.heading("🧠", "Recording proposal in Padagonia"));
        let result = match PadagoniaClient::from_env() {
            Ok(client) => {
                client
                    .record_transformation_proposal(&candidate, &distance, &gate_results)
                    .await
            }
            Err(error) => Err(error.context("failed to configure Padagonia")),
        };
        match result {
            Ok(recorded) => println!(
                "{}",
                ui.success(format!("Recorded {} research nodes", recorded))
            ),
            Err(error) => eprintln!(
                "{}",
                ui.warning(format!(
                    "Padagonia unavailable; proposal was not persisted: {error}"
                ))
            ),
        }
    }

    Ok(())
}

/// Actually run the project's own test suite rather than assuming a
/// candidate is validated. Only understands `cargo test` today; a project
/// without a `Cargo.toml` gets `NotRun` with an explanation, not a fabricated
/// result.
fn run_validation(path: &PathBuf) -> ValidationOutcome {
    if !path.join("Cargo.toml").exists() {
        return ValidationOutcome::NotRun;
    }
    match std::process::Command::new("cargo")
        .arg("test")
        .current_dir(path)
        .status()
    {
        Ok(status) if status.success() => ValidationOutcome::Passed,
        Ok(status) => ValidationOutcome::Failed(format!("exit status {status}")),
        Err(error) => ValidationOutcome::Failed(format!("failed to run cargo test: {error}")),
    }
}

fn evolve_trajectory(path: PathBuf, limit: usize, since: Option<String>) -> Result<()> {
    let ui = Output::new();
    println!(
        "{}",
        ui.heading(
            "📈",
            format!("Transformation trajectory for {}", path.display())
        )
    );

    let trajectory = Trajectory::walk(&path, limit, since.as_deref())
        .context("Failed to walk commit trajectory")?;

    if trajectory.steps.is_empty() {
        println!("{}", ui.info("No commits with a first parent in range"));
        return Ok(());
    }

    let rows = trajectory
        .steps
        .iter()
        .map(|step| {
            vec![
                step.commit.clone(),
                step.summary.clone(),
                format!("{:.2}", step.distance.overall()),
                step.distance.band().to_string(),
            ]
        })
        .collect::<Vec<_>>();
    println!(
        "{}",
        ui.table(&["Commit", "Summary", "Distance", "Band"], &rows)
    );

    let cumulative = trajectory.cumulative();
    println!(
        "\n{}",
        ui.table(
            &["Steps", "Cumulative distance", "Clamped band"],
            &[vec![
                trajectory.steps.len().to_string(),
                format!("{cumulative:.2}"),
                ami::types::band_for(cumulative.min(1.0)).to_string(),
            ]],
        )
    );

    if trajectory.hidden_by_decomposition() {
        println!(
            "\n{}",
            ui.warning(format!(
                "Cumulative distance ({cumulative:.2}) crosses a significant-change threshold that no single commit reached on its own — this range should be assessed as one transformation, not {} separate small ones",
                trajectory.steps.len()
            ))
        );
    } else {
        println!(
            "\n{}",
            ui.success("No cumulative drift hidden by decomposition detected in this range")
        );
    }

    Ok(())
}

fn unix_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
