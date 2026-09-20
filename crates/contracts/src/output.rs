//! Vendor-neutral, validated presentation blocks returned by an assistant run.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const OUTPUT_SCHEMA_V1: &str = "aethra.output.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssistantOutput {
    pub schema: String,
    pub blocks: Vec<OutputBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum OutputBlock {
    Markdown {
        id: String,
        markdown: String,
    },
    List {
        id: String,
        ordered: bool,
        items: Vec<String>,
    },
    Table {
        id: String,
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Sources {
        id: String,
        retrieved_at: u64,
        partial: bool,
        citations_resolved: bool,
        sources: Vec<OutputSource>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OutputSource {
    pub id: String,
    pub title: String,
    pub url: String,
    pub excerpt: String,
}

impl AssistantOutput {
    pub fn markdown(text: impl Into<String>) -> Self {
        Self {
            schema: OUTPUT_SCHEMA_V1.into(),
            blocks: vec![OutputBlock::Markdown {
                id: "answer".into(),
                markdown: text.into(),
            }],
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema != OUTPUT_SCHEMA_V1 || self.blocks.is_empty() || self.blocks.len() > 16 {
            return Err(Error::InvalidResponse);
        }
        if serde_json::to_vec(self)
            .map_err(|_| Error::InvalidResponse)?
            .len()
            > 256_000
        {
            return Err(Error::InvalidResponse);
        }
        let mut ids = BTreeSet::new();
        for block in &self.blocks {
            let id = match block {
                OutputBlock::Markdown { id, markdown } => {
                    bounded(markdown, 16_000)?;
                    id
                }
                OutputBlock::List { id, items, .. } => {
                    if items.is_empty() || items.len() > 100 {
                        return Err(Error::InvalidResponse);
                    }
                    for item in items {
                        bounded(item, 2_000)?;
                    }
                    id
                }
                OutputBlock::Table { id, columns, rows } => {
                    if columns.is_empty()
                        || columns.len() > 20
                        || rows.len() > 200
                        || columns.iter().any(|column| bounded(column, 100).is_err())
                        || rows.iter().any(|row| {
                            row.len() != columns.len()
                                || row.iter().any(|cell| bounded(cell, 2_000).is_err())
                        })
                    {
                        return Err(Error::InvalidResponse);
                    }
                    id
                }
                OutputBlock::Sources { id, sources, .. } => {
                    if sources.is_empty()
                        || sources.len() > 12
                        || sources.iter().any(|source| {
                            bounded(&source.id, 100).is_err()
                                || bounded(&source.title, 300).is_err()
                                || bounded(&source.excerpt, 4_000).is_err()
                                || source.url.len() > 2_048
                                || !source.url.starts_with("https://")
                        })
                    {
                        return Err(Error::InvalidResponse);
                    }
                    id
                }
            };
            bounded(id, 64)?;
            if !id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            }) {
                return Err(Error::InvalidResponse);
            }
            if !ids.insert(id) {
                return Err(Error::InvalidResponse);
            }
        }
        Ok(())
    }

    pub fn plain_text(&self) -> String {
        self.blocks
            .iter()
            .map(|block| match block {
                OutputBlock::Markdown { markdown, .. } => markdown.clone(),
                OutputBlock::List { ordered, items, .. } => items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        if *ordered {
                            format!("{}. {item}", index + 1)
                        } else {
                            format!("- {item}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                OutputBlock::Table { columns, rows, .. } => std::iter::once(columns.join(" | "))
                    .chain(rows.iter().map(|row| row.join(" | ")))
                    .collect::<Vec<_>>()
                    .join("\n"),
                OutputBlock::Sources { sources, .. } => sources
                    .iter()
                    .map(|source| format!("[{}] {}", source.id, source.url))
                    .collect::<Vec<_>>()
                    .join("\n"),
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

fn bounded(value: &str, max: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > max {
        Err(Error::InvalidResponse)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_markdown_list_and_table_blocks() {
        let output = AssistantOutput {
            schema: OUTPUT_SCHEMA_V1.into(),
            blocks: vec![
                OutputBlock::Markdown {
                    id: "summary".into(),
                    markdown: "Result".into(),
                },
                OutputBlock::List {
                    id: "items".into(),
                    ordered: false,
                    items: vec!["One".into(), "Two".into()],
                },
                OutputBlock::Table {
                    id: "table".into(),
                    columns: vec!["Name".into(), "Value".into()],
                    rows: vec![vec!["A".into(), "1".into()]],
                },
            ],
        };
        assert_eq!(output.validate(), Ok(()));
        assert!(output.plain_text().contains("Name | Value"));
    }

    #[test]
    fn rejects_unknown_schema_duplicate_ids_and_ragged_tables() {
        let mut output = AssistantOutput::markdown("ok");
        output.schema = "other".into();
        assert_eq!(output.validate(), Err(Error::InvalidResponse));

        let output = AssistantOutput {
            schema: OUTPUT_SCHEMA_V1.into(),
            blocks: vec![
                OutputBlock::Markdown {
                    id: "same".into(),
                    markdown: "one".into(),
                },
                OutputBlock::Table {
                    id: "same".into(),
                    columns: vec!["a".into(), "b".into()],
                    rows: vec![vec!["only one".into()]],
                },
            ],
        };
        assert_eq!(output.validate(), Err(Error::InvalidResponse));
    }
}
