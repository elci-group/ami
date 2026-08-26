use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Store nested serde values as one JSON-encoded CSV cell. The CSV crate only
/// supports scalar fields when serializing structs, while prospects contain
/// vectors and contact objects.
mod csv_json {
    use serde::de::DeserializeOwned;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Serialize,
        S: Serializer,
    {
        let json = serde_json::to_string(value).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&json)
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: DeserializeOwned,
        D: Deserializer<'de>,
    {
        let json = String::deserialize(deserializer)?;
        serde_json::from_str(&json).map_err(serde::de::Error::custom)
    }
}

/// Represents the development stage of a project
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DevelopmentStage {
    Prototype,
    MVP,
    Alpha,
    Beta,
    ReleaseCandidate,
    Stable,
}

impl DevelopmentStage {
    /// Returns appropriate contact strategies for this stage
    #[allow(dead_code)]
    pub fn contact_strategies(&self) -> Vec<ContactStrategy> {
        match self {
            DevelopmentStage::Prototype => vec![
                ContactStrategy::Friends,
                ContactStrategy::Hackers,
                ContactStrategy::Reddit,
            ],
            DevelopmentStage::MVP => vec![
                ContactStrategy::Friends,
                ContactStrategy::Hackers,
                ContactStrategy::Reddit,
                ContactStrategy::Discord,
            ],
            DevelopmentStage::Alpha => vec![
                ContactStrategy::Hackers,
                ContactStrategy::Reddit,
                ContactStrategy::Discord,
                ContactStrategy::EarlyAdopters,
            ],
            DevelopmentStage::Beta => vec![
                ContactStrategy::OpenSourceMaintainers,
                ContactStrategy::Companies,
                ContactStrategy::Newsletters,
                ContactStrategy::Discord,
            ],
            DevelopmentStage::ReleaseCandidate => vec![
                ContactStrategy::OpenSourceMaintainers,
                ContactStrategy::Companies,
                ContactStrategy::Newsletters,
                ContactStrategy::TechBloggers,
            ],
            DevelopmentStage::Stable => vec![
                ContactStrategy::Enterprise,
                ContactStrategy::Conferences,
                ContactStrategy::Universities,
                ContactStrategy::Companies,
            ],
        }
    }
}

/// Contact strategy based on development stage
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum ContactStrategy {
    Friends,
    Hackers,
    Reddit,
    Discord,
    EarlyAdopters,
    OpenSourceMaintainers,
    Companies,
    Newsletters,
    TechBloggers,
    Enterprise,
    Conferences,
    Universities,
}

/// A project being analyzed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub description: String,
    pub repository_url: Option<String>,
    pub website_url: Option<String>,
    pub capabilities: Vec<Capability>,
    pub keywords: Vec<String>,
    pub development_stage: DevelopmentStage,
    pub last_analyzed: DateTime<Utc>,
    pub commit_hash: Option<String>,
}

/// A capability extracted from the project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub name: String,
    pub category: CapabilityCategory,
    pub source: CapabilitySource,
    pub confidence: f32,
}

/// Category of capability
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityCategory {
    Technology,
    Domain,
    Platform,
    Language,
    Framework,
    Feature,
}

/// Where the capability was discovered
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilitySource {
    Readme,
    CargoToml,
    PackageJson,
    GitHistory,
    Tags,
    Roadmap,
    Website,
}

/// A potential adopter/prospect
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prospect {
    pub name: String,
    pub prospect_type: ProspectType,
    pub platform: Platform,
    pub url: String,
    #[serde(with = "csv_json")]
    pub reason_matched: Vec<String>,
    pub confidence: f32,
    pub audience_size: Option<u64>,
    pub contact_available: bool,
    #[serde(with = "csv_json")]
    pub contact_info: Vec<ContactChannel>,
    pub previous_contact: Option<DateTime<Utc>>,
    pub discovered_at: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
}

/// Type of prospect
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProspectType {
    Repository,
    Organization,
    User,
    Community,
    Company,
    Influencer,
    Publication,
    University,
    OpenSourceFoundation,
}

/// Platform where prospect was found
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Platform {
    GitHub,
    Reddit,
    HackerNews,
    DevTo,
    ProductHunt,
    Discord,
    Mastodon,
    Twitter,
    YouTube,
    Blog,
    CompanyPage,
    University,
    OpenSourceFoundation,
    PackageRegistry,
    StackOverflow,
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::GitHub => write!(f, "GitHub"),
            Platform::Reddit => write!(f, "Reddit"),
            Platform::HackerNews => write!(f, "Hacker News"),
            Platform::DevTo => write!(f, "Dev.to"),
            Platform::ProductHunt => write!(f, "Product Hunt"),
            Platform::Discord => write!(f, "Discord"),
            Platform::Mastodon => write!(f, "Mastodon"),
            Platform::Twitter => write!(f, "Twitter"),
            Platform::YouTube => write!(f, "YouTube"),
            Platform::Blog => write!(f, "Blog"),
            Platform::CompanyPage => write!(f, "Company Page"),
            Platform::University => write!(f, "University"),
            Platform::OpenSourceFoundation => write!(f, "Open Source Foundation"),
            Platform::PackageRegistry => write!(f, "Package Registry"),
            Platform::StackOverflow => write!(f, "Stack Overflow"),
        }
    }
}

/// Contact channel information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactChannel {
    pub channel_type: ContactType,
    pub value: String,
    pub verified: bool,
}

/// Type of contact channel
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContactType {
    Email,
    GitHub,
    LinkedIn,
    Twitter,
    Discord,
    Website,
    Newsletter,
    Press,
}

/// Addressable market metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressableMarketScore {
    pub potential_users: u64,
    pub reachable_users: u64,
    pub high_confidence: u64,
    pub immediate_contacts: u64,
    pub calculated_at: DateTime<Utc>,
}

/// Search phrase for discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchPhrase {
    pub phrase: String,
    pub source_capabilities: Vec<String>,
    pub weight: f32,
}

/// Discovery source configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct DiscoverySource {
    pub platform: Platform,
    pub enabled: bool,
    pub rate_limit_ms: Option<u64>,
    pub last_scanned: Option<DateTime<Utc>>,
}

/// Mailing list entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailingListEntry {
    pub prospect_name: String,
    pub prospect_url: String,
    pub prospect_type: ProspectType,
    pub confidence: f32,
    #[serde(with = "csv_json")]
    pub contact_channel: ContactChannel,
    pub list_type: MailingListType,
    pub added_at: DateTime<Utc>,
}

/// Type of mailing list
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MailingListType {
    Alpha,
    Beta,
    Enterprise,
    Education,
    Gaming,
    Terminal,
    OpenSource,
    Custom(String),
}

impl MailingListType {
    pub fn as_str(&self) -> &str {
        match self {
            MailingListType::Alpha => "alpha",
            MailingListType::Beta => "beta",
            MailingListType::Enterprise => "enterprise",
            MailingListType::Education => "education",
            MailingListType::Gaming => "gaming",
            MailingListType::Terminal => "terminal",
            MailingListType::OpenSource => "opensource",
            MailingListType::Custom(s) => s,
        }
    }
}

/// Scoring weights for prospect matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringWeights {
    pub keyword_overlap: f32,
    pub technology_overlap: f32,
    pub domain_overlap: f32,
    pub repository_activity: f32,
    pub audience_relevance: f32,
    pub prior_interactions: f32,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            keyword_overlap: 30.0,
            technology_overlap: 20.0,
            domain_overlap: 25.0,
            repository_activity: 15.0,
            audience_relevance: 10.0,
            prior_interactions: 0.0,
        }
    }
}
