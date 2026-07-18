//! Maps `--model` names to fastembed variants. Default = multilingual-e5-small (384-dim).

use crate::core::error::{MemError, MemResult};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ModelChoice {
    MultilingualE5Small,
    AllMiniLML6V2,
    BGESmallENV15,
    BGESmallZHV15,
    NomicEmbedTextV15,
}

impl ModelChoice {
    pub const DEFAULT: ModelChoice = ModelChoice::MultilingualE5Small;

    pub fn name(&self) -> &'static str {
        match self {
            ModelChoice::MultilingualE5Small => "multilingual-e5-small",
            ModelChoice::AllMiniLML6V2 => "all-MiniLM-L6-v2",
            ModelChoice::BGESmallENV15 => "bge-small-en-v1.5",
            ModelChoice::BGESmallZHV15 => "bge-small-zh-v1.5",
            ModelChoice::NomicEmbedTextV15 => "nomic-embed-text-v1.5",
        }
    }

    pub fn dim(&self) -> usize {
        match self {
            ModelChoice::MultilingualE5Small => 384,
            ModelChoice::AllMiniLML6V2 => 384,
            ModelChoice::BGESmallENV15 => 384,
            ModelChoice::BGESmallZHV15 => 512,
            ModelChoice::NomicEmbedTextV15 => 768,
        }
    }

    pub fn from_name(s: &str) -> MemResult<ModelChoice> {
        match s {
            "multilingual-e5-small" => Ok(ModelChoice::MultilingualE5Small),
            "all-MiniLM-L6-v2" => Ok(ModelChoice::AllMiniLML6V2),
            "bge-small-en-v1.5" => Ok(ModelChoice::BGESmallENV15),
            "bge-small-zh-v1.5" => Ok(ModelChoice::BGESmallZHV15),
            "nomic-embed-text-v1.5" => Ok(ModelChoice::NomicEmbedTextV15),
            other => Err(MemError::InvalidArgument(format!(
                "unknown model '{other}'; supported: multilingual-e5-small, \
                 all-MiniLM-L6-v2, bge-small-en-v1.5, bge-small-zh-v1.5, nomic-embed-text-v1.5"
            ))),
        }
    }

    /// Slug used for the sidecar directory name, e.g. `multilingual-e5-small`.
    pub fn slug(&self) -> &'static str {
        self.name()
    }

    /// HuggingFace repo fastembed downloads from (the fastembed `model_code`
    /// field, verified in Task 1). Used to detect a pre-populated sidecar cache
    /// subdir (`models--<org>--<name>`).
    pub fn repo(&self) -> &'static str {
        match self {
            ModelChoice::MultilingualE5Small => "intfloat/multilingual-e5-small",
            ModelChoice::AllMiniLML6V2 => "Qdrant/all-MiniLM-L6-v2-onnx",
            ModelChoice::BGESmallENV15 => "Xenova/bge-small-en-v1.5",
            ModelChoice::BGESmallZHV15 => "Xenova/bge-small-zh-v1.5",
            ModelChoice::NomicEmbedTextV15 => "nomic-ai/nomic-embed-text-v1.5",
        }
    }

    pub fn to_fastembed(&self) -> fastembed::EmbeddingModel {
        match self {
            ModelChoice::MultilingualE5Small => fastembed::EmbeddingModel::MultilingualE5Small,
            ModelChoice::AllMiniLML6V2 => fastembed::EmbeddingModel::AllMiniLML6V2,
            ModelChoice::BGESmallENV15 => fastembed::EmbeddingModel::BGESmallENV15,
            ModelChoice::BGESmallZHV15 => fastembed::EmbeddingModel::BGESmallZHV15,
            ModelChoice::NomicEmbedTextV15 => fastembed::EmbeddingModel::NomicEmbedTextV15,
        }
    }
}
