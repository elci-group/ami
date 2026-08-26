use crate::types::{MailingListEntry, MailingListType, Prospect, ProspectType};
use anyhow::{Context, Result};
use csv::WriterBuilder;
use std::fs;
use std::path::Path;

pub struct ContactDatabase {
    base_path: std::path::PathBuf,
}

impl ContactDatabase {
    pub fn new(base_path: &Path) -> Self {
        Self {
            base_path: base_path.to_path_buf(),
        }
    }

    /// Initialize the database directory structure
    pub fn initialize(&self) -> Result<()> {
        fs::create_dir_all(&self.base_path).context("Failed to create database directory")?;

        let lists_dir = self.base_path.join("lists");
        fs::create_dir_all(&lists_dir).context("Failed to create lists directory")?;

        Ok(())
    }

    /// Save prospects to CSV
    pub fn save_prospects(&self, prospects: &[Prospect]) -> Result<()> {
        let prospects_path = self.base_path.join("prospects.csv");

        let mut wtr = WriterBuilder::new()
            .has_headers(true)
            .from_path(&prospects_path)
            .context("Failed to create prospects CSV")?;

        for prospect in prospects {
            wtr.serialize(prospect)
                .context("Failed to serialize prospect")?;
        }

        wtr.flush().context("Failed to flush prospects CSV")?;

        Ok(())
    }

    /// Load prospects from CSV
    pub fn load_prospects(&self) -> Result<Vec<Prospect>> {
        let prospects_path = self.base_path.join("prospects.csv");

        if !prospects_path.exists() {
            return Ok(Vec::new());
        }

        let mut rdr =
            csv::Reader::from_path(&prospects_path).context("Failed to open prospects CSV")?;

        let mut prospects = Vec::new();

        for result in rdr.deserialize() {
            let prospect: Prospect = result.context("Failed to deserialize prospect")?;
            prospects.push(prospect);
        }

        Ok(prospects)
    }

    /// Append a single prospect to the database
    #[allow(dead_code)]
    pub fn append_prospect(&self, prospect: &Prospect) -> Result<()> {
        let prospects_path = self.base_path.join("prospects.csv");

        let file_exists = prospects_path.exists();
        let mut wtr = WriterBuilder::new()
            .has_headers(!file_exists)
            .from_path(&prospects_path)
            .context("Failed to create/open prospects CSV")?;

        wtr.serialize(prospect)
            .context("Failed to serialize prospect")?;

        wtr.flush().context("Failed to flush prospects CSV")?;

        Ok(())
    }

    /// Generate mailing list by type
    pub fn generate_mailing_list(
        &self,
        prospects: &[Prospect],
        list_type: MailingListType,
    ) -> Result<Vec<MailingListEntry>> {
        let mut entries = Vec::new();

        for prospect in prospects {
            // Filter based on list type and confidence
            let should_include = match &list_type {
                MailingListType::Alpha => prospect.confidence >= 0.6,
                MailingListType::Beta => prospect.confidence >= 0.7,
                MailingListType::Enterprise => {
                    prospect.confidence >= 0.8 && prospect.audience_size.unwrap_or(0) > 1000
                }
                MailingListType::Education => {
                    matches!(prospect.prospect_type, ProspectType::University)
                }
                MailingListType::Gaming => {
                    prospect.name.to_lowercase().contains("game")
                        || prospect
                            .reason_matched
                            .iter()
                            .any(|r| r.to_lowercase().contains("game"))
                }
                MailingListType::Terminal => {
                    prospect.name.to_lowercase().contains("terminal")
                        || prospect
                            .reason_matched
                            .iter()
                            .any(|r| r.to_lowercase().contains("terminal"))
                }
                MailingListType::OpenSource => matches!(
                    prospect.prospect_type,
                    ProspectType::Repository | ProspectType::OpenSourceFoundation
                ),
                MailingListType::Custom(_) => true, // Include all for custom lists
            };

            if should_include && prospect.contact_available {
                for contact in &prospect.contact_info {
                    entries.push(MailingListEntry {
                        prospect_name: prospect.name.clone(),
                        prospect_url: prospect.url.clone(),
                        prospect_type: prospect.prospect_type.clone(),
                        confidence: prospect.confidence,
                        contact_channel: contact.clone(),
                        list_type: list_type.clone(),
                        added_at: chrono::Utc::now(),
                    });
                }
            }
        }

        Ok(entries)
    }

    /// Save mailing list to CSV
    pub fn save_mailing_list(
        &self,
        entries: &[MailingListEntry],
        list_type: &MailingListType,
    ) -> Result<()> {
        let lists_dir = self.base_path.join("lists");
        let filename = format!("{}.csv", list_type.as_str());
        let list_path = lists_dir.join(&filename);

        let mut wtr = WriterBuilder::new()
            .has_headers(true)
            .from_path(&list_path)
            .context("Failed to create mailing list CSV")?;

        for entry in entries {
            wtr.serialize(entry)
                .context("Failed to serialize mailing list entry")?;
        }

        wtr.flush().context("Failed to flush mailing list CSV")?;

        Ok(())
    }

    /// Load mailing list from CSV
    #[allow(dead_code)]
    pub fn load_mailing_list(&self, list_type: &MailingListType) -> Result<Vec<MailingListEntry>> {
        let lists_dir = self.base_path.join("lists");
        let filename = format!("{}.csv", list_type.as_str());
        let list_path = lists_dir.join(&filename);

        if !list_path.exists() {
            return Ok(Vec::new());
        }

        let mut rdr =
            csv::Reader::from_path(&list_path).context("Failed to open mailing list CSV")?;

        let mut entries = Vec::new();

        for result in rdr.deserialize() {
            let entry: MailingListEntry =
                result.context("Failed to deserialize mailing list entry")?;
            entries.push(entry);
        }

        Ok(entries)
    }

    /// Generate all mailing lists for a development stage
    pub fn generate_all_lists(&self, prospects: &[Prospect]) -> Result<()> {
        let list_types = vec![
            MailingListType::Alpha,
            MailingListType::Beta,
            MailingListType::Enterprise,
            MailingListType::Education,
            MailingListType::Gaming,
            MailingListType::Terminal,
            MailingListType::OpenSource,
        ];

        for list_type in list_types {
            let entries = self.generate_mailing_list(prospects, list_type.clone())?;
            self.save_mailing_list(&entries, &list_type)?;
        }

        Ok(())
    }

    /// Get database statistics
    pub fn get_stats(&self) -> Result<DatabaseStats> {
        let prospects = self.load_prospects()?;

        let total_prospects = prospects.len();
        let high_confidence = prospects.iter().filter(|p| p.confidence >= 0.7).count();
        let with_contacts = prospects.iter().filter(|p| p.contact_available).count();

        let lists_dir = self.base_path.join("lists");
        let mailing_list_count = if lists_dir.exists() {
            fs::read_dir(&lists_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "csv"))
                .count()
        } else {
            0
        };

        Ok(DatabaseStats {
            total_prospects,
            high_confidence,
            with_contacts,
            mailing_list_count,
        })
    }
}

#[derive(Debug)]
pub struct DatabaseStats {
    pub total_prospects: usize,
    pub high_confidence: usize,
    pub with_contacts: usize,
    pub mailing_list_count: usize,
}
