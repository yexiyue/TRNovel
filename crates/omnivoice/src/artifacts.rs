use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Seek, SeekFrom},
    ops::RangeInclusive,
    path::{Path, PathBuf},
};

use candle_core::quantized::{GgmlDType, gguf_file};
use half::{bf16, f16};
use safetensors::{Dtype, SafeTensors, tensor::TensorView};
use serde::Deserialize;

use crate::{
    contracts::{
        BoolTensor2, BoolTensor4, ChunkedPreparedPrompts, DecodedAudio, F32Tensor3, F32Tensor4,
        GeneratedTokens, GenerationConfig, GenerationMode, GenerationRequest, I64Tensor2,
        I64Tensor3, PreparedPrompt, PreparedPromptSequence, PromptTensorBundle, VoiceClonePrompt,
    },
    error::{OmniVoiceError, Result},
};

pub const RUNTIME_MANIFEST_FILE_NAME: &str = "omnivoice.artifacts.json";
const DEFAULT_GGUF_LM_FILE: &str = "omnivoice-base-f16.gguf";
const DEFAULT_GGUF_CODEC_FILE: &str = "omnivoice-tokenizer-f16.gguf";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelWeightDType {
    F32,
    F16,
    BF16,
    Quantized,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RuntimeArtifactManifest {
    pub version: u32,
    pub generator: GeneratorArtifactManifest,
    pub text_tokenizer: TextTokenizerArtifactManifest,
    pub audio_tokenizer: AudioTokenizerArtifactManifest,
    pub contracts: RuntimeContracts,
}

impl RuntimeArtifactManifest {
    pub fn from_model_root(model_root: impl AsRef<Path>) -> Result<Self> {
        let path = model_root.as_ref().join(RUNTIME_MANIFEST_FILE_NAME);
        let manifest: Self = serde_json::from_str(&fs::read_to_string(&path)?)?;
        if manifest.version != 1 {
            return Err(OmniVoiceError::Unsupported(format!(
                "unsupported runtime artifact manifest version {}",
                manifest.version
            )));
        }
        Ok(manifest)
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GeneratorArtifactManifest {
    pub config: PathBuf,
    pub weights: PathBuf,
    pub required_prefixes: Vec<String>,
    #[serde(default)]
    pub ignored_keys: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct TextTokenizerArtifactManifest {
    pub tokenizer: PathBuf,
    pub tokenizer_config: PathBuf,
    #[serde(default)]
    pub metadata: TextTokenizerMetadataArtifactManifest,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct TextTokenizerMetadataArtifactManifest {
    #[serde(default)]
    pub chat_template: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct AudioTokenizerArtifactManifest {
    pub config: PathBuf,
    pub weights: PathBuf,
    pub preprocessor_config: PathBuf,
    pub required_prefixes: Vec<String>,
    #[serde(default)]
    pub metadata: AudioTokenizerMetadataArtifactManifest,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct AudioTokenizerMetadataArtifactManifest {
    #[serde(default)]
    pub license: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RuntimeContracts {
    pub num_audio_codebooks: usize,
    pub audio_vocab_size: usize,
    pub audio_mask_id: i64,
    pub token_id_min: i64,
    pub token_id_max: i64,
    pub sample_rate: u32,
    pub hop_length: usize,
    pub frame_rate: usize,
}

impl RuntimeContracts {
    pub fn token_id_range(&self) -> RangeInclusive<i64> {
        self.token_id_min..=self.token_id_max
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeArtifacts {
    model_root: PathBuf,
    manifest_path: Option<PathBuf>,
    manifest: Option<RuntimeArtifactManifest>,
    generator: Option<GeneratorArtifacts>,
    text_tokenizer: Option<TextTokenizerArtifacts>,
    audio_tokenizer: Option<AudioTokenizerArtifacts>,
    gguf_bundle: Option<GgufBundleArtifacts>,
    contracts: RuntimeContracts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeArtifactFormat {
    SafetensorsManifest,
    GgufBundle,
}

#[derive(Debug, Clone)]
pub struct GgufBundleArtifacts {
    generator_path: PathBuf,
    audio_tokenizer_path: PathBuf,
}

impl RuntimeArtifacts {
    pub fn from_model_root(model_root: impl AsRef<Path>) -> Result<Self> {
        let model_root = model_root.as_ref().to_path_buf();
        if !model_root.exists() {
            return Err(OmniVoiceError::MissingArtifact { path: model_root });
        }

        let manifest_path = model_root.join(RUNTIME_MANIFEST_FILE_NAME);
        if manifest_path.exists() {
            let manifest = RuntimeArtifactManifest::from_model_root(&model_root)?;
            let generator = GeneratorArtifacts::resolve(&model_root, &manifest.generator)?;
            let text_tokenizer =
                TextTokenizerArtifacts::resolve(&model_root, &manifest.text_tokenizer)?;
            let audio_tokenizer =
                AudioTokenizerArtifacts::resolve(&model_root, &manifest.audio_tokenizer)?;
            validate_runtime_contracts(&generator, &audio_tokenizer, &manifest.contracts)?;

            return Ok(Self {
                model_root,
                manifest_path: Some(manifest_path),
                contracts: manifest.contracts.clone(),
                manifest: Some(manifest),
                generator: Some(generator),
                text_tokenizer: Some(text_tokenizer),
                audio_tokenizer: Some(audio_tokenizer),
                gguf_bundle: None,
            });
        }

        if let Ok(manifest_path) = crate::model_source::ensure_local_runtime_manifest(&model_root) {
            let manifest = RuntimeArtifactManifest::from_model_root(&model_root)?;
            let generator = GeneratorArtifacts::resolve(&model_root, &manifest.generator)?;
            let text_tokenizer =
                TextTokenizerArtifacts::resolve(&model_root, &manifest.text_tokenizer)?;
            let audio_tokenizer =
                AudioTokenizerArtifacts::resolve(&model_root, &manifest.audio_tokenizer)?;
            validate_runtime_contracts(&generator, &audio_tokenizer, &manifest.contracts)?;

            return Ok(Self {
                model_root,
                manifest_path: Some(manifest_path),
                contracts: manifest.contracts.clone(),
                manifest: Some(manifest),
                generator: Some(generator),
                text_tokenizer: Some(text_tokenizer),
                audio_tokenizer: Some(audio_tokenizer),
                gguf_bundle: None,
            });
        }

        let gguf_bundle = GgufBundleArtifacts::resolve(&model_root)?;
        let contracts = gguf_bundle.read_contracts()?;
        Ok(Self {
            model_root,
            manifest_path: None,
            manifest: None,
            generator: None,
            text_tokenizer: None,
            audio_tokenizer: None,
            gguf_bundle: Some(gguf_bundle),
            contracts,
        })
    }

    pub fn format(&self) -> RuntimeArtifactFormat {
        match self.gguf_bundle {
            Some(_) => RuntimeArtifactFormat::GgufBundle,
            None => RuntimeArtifactFormat::SafetensorsManifest,
        }
    }

    pub fn is_gguf_bundle(&self) -> bool {
        matches!(self.format(), RuntimeArtifactFormat::GgufBundle)
    }

    pub fn manifest(&self) -> Option<&RuntimeArtifactManifest> {
        self.manifest.as_ref()
    }

    pub fn contracts(&self) -> &RuntimeContracts {
        &self.contracts
    }

    pub fn model_root(&self) -> &Path {
        &self.model_root
    }

    pub fn manifest_path(&self) -> Option<&Path> {
        self.manifest_path.as_deref()
    }

    pub fn generator(&self) -> Result<&GeneratorArtifacts> {
        self.generator.as_ref().ok_or_else(|| {
            OmniVoiceError::Unsupported(
                "generator safetensors artifacts are unavailable for a GGUF runtime".to_string(),
            )
        })
    }

    pub fn text_tokenizer(&self) -> Result<&TextTokenizerArtifacts> {
        self.text_tokenizer.as_ref().ok_or_else(|| {
            OmniVoiceError::Unsupported(
                "text tokenizer json artifacts are unavailable for a GGUF runtime".to_string(),
            )
        })
    }

    pub fn audio_tokenizer(&self) -> Result<&AudioTokenizerArtifacts> {
        self.audio_tokenizer.as_ref().ok_or_else(|| {
            OmniVoiceError::Unsupported(
                "audio tokenizer safetensors artifacts are unavailable for a GGUF runtime"
                    .to_string(),
            )
        })
    }

    pub fn gguf_bundle(&self) -> Option<&GgufBundleArtifacts> {
        self.gguf_bundle.as_ref()
    }

    pub fn native_weight_dtype(&self) -> Result<ModelWeightDType> {
        if let Some(bundle) = self.gguf_bundle() {
            let content = bundle.open_generator_content()?;
            return Ok(classify_gguf_weight_dtype(
                content.tensor_infos.values().map(|info| info.ggml_dtype),
            ));
        }
        native_weight_dtype_from_safetensors(self.generator()?.weights_path())
    }
}

impl GgufBundleArtifacts {
    fn resolve(model_root: &Path) -> Result<Self> {
        let generator_path = resolve_gguf_candidate(
            model_root,
            &["omnivoice-base-", "base"],
            DEFAULT_GGUF_LM_FILE,
        )?;
        let audio_tokenizer_path = resolve_gguf_candidate(
            model_root,
            &["omnivoice-tokenizer-", "tokenizer", "codec"],
            DEFAULT_GGUF_CODEC_FILE,
        )?;
        Ok(Self {
            generator_path,
            audio_tokenizer_path,
        })
    }

    pub fn generator_path(&self) -> &Path {
        &self.generator_path
    }

    pub fn audio_tokenizer_path(&self) -> &Path {
        &self.audio_tokenizer_path
    }

    pub fn open_generator_content(&self) -> Result<gguf_file::Content> {
        read_gguf_content(&self.generator_path)
    }

    pub fn open_audio_tokenizer_content(&self) -> Result<gguf_file::Content> {
        read_gguf_content(&self.audio_tokenizer_path)
    }

    fn read_contracts(&self) -> Result<RuntimeContracts> {
        let generator = self.open_generator_content()?;
        let codec = self.open_audio_tokenizer_content()?;
        let num_audio_codebooks = gguf_u32(&generator, "omnivoice.num_audio_codebook")? as usize;
        let audio_vocab_size = gguf_u32(&generator, "omnivoice.audio_vocab_size")? as usize;
        let audio_mask_id = gguf_u32(&generator, "omnivoice.audio_mask_id")? as i64;
        let sample_rate = gguf_u32(&codec, "omnivoice.sample_rate")?;
        let hop_length = gguf_u32(&codec, "omnivoice.acoustic.hop_length")? as usize;
        let frame_rate = usize::try_from(sample_rate)
            .ok()
            .and_then(|sr| sr.checked_div(hop_length.max(1)))
            .filter(|rate| *rate > 0)
            .ok_or_else(|| {
                OmniVoiceError::InvalidData(format!(
                    "invalid GGUF frame-rate inputs sample_rate={sample_rate}, hop_length={hop_length}"
                ))
            })?;
        Ok(RuntimeContracts {
            num_audio_codebooks,
            audio_vocab_size,
            audio_mask_id,
            token_id_min: 0,
            token_id_max: audio_mask_id.saturating_sub(1),
            sample_rate,
            hop_length,
            frame_rate,
        })
    }
}

pub(crate) fn classify_gguf_weight_dtype<I>(types: I) -> ModelWeightDType
where
    I: IntoIterator<Item = GgmlDType>,
{
    let mut f32 = 0usize;
    let mut f16 = 0usize;
    let mut bf16 = 0usize;
    let mut quantized = 0usize;
    for ty in types {
        match ty {
            GgmlDType::F32 => f32 += 1,
            GgmlDType::F16 => f16 += 1,
            GgmlDType::BF16 => bf16 += 1,
            _ => quantized += 1,
        }
    }
    if quantized > 0 {
        return ModelWeightDType::Quantized;
    }
    if bf16 >= f16 && bf16 >= f32 && bf16 > 0 {
        ModelWeightDType::BF16
    } else if f16 >= f32 && f16 > 0 {
        ModelWeightDType::F16
    } else {
        ModelWeightDType::F32
    }
}

fn native_weight_dtype_from_safetensors(path: &Path) -> Result<ModelWeightDType> {
    let parsed = read_safetensor_header_map(path)?;
    let mut f32 = 0usize;
    let mut f16 = 0usize;
    let mut bf16 = 0usize;
    for (key, value) in parsed {
        if key == "__metadata__" {
            continue;
        }
        let Some(dtype) = value.get("dtype").and_then(|value| value.as_str()) else {
            continue;
        };
        match dtype {
            "F32" => f32 += 1,
            "F16" => f16 += 1,
            "BF16" => bf16 += 1,
            _ => {}
        }
    }
    if bf16 >= f16 && bf16 >= f32 && bf16 > 0 {
        Ok(ModelWeightDType::BF16)
    } else if f16 >= f32 && f16 > 0 {
        Ok(ModelWeightDType::F16)
    } else {
        Ok(ModelWeightDType::F32)
    }
}

fn resolve_gguf_candidate(model_root: &Path, patterns: &[&str], fallback: &str) -> Result<PathBuf> {
    let entries: Vec<PathBuf> = fs::read_dir(model_root)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file())
        .collect();
    for pattern in patterns {
        let needle = pattern.trim_matches('*').to_ascii_lowercase();
        if let Some(path) = entries.iter().find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_ascii_lowercase().contains(&needle))
                .unwrap_or(false)
        }) {
            return Ok(path.clone());
        }
    }
    let fallback_path = model_root.join(fallback);
    if fallback_path.is_file() {
        return Ok(fallback_path);
    }
    Err(OmniVoiceError::MissingArtifact {
        path: fallback_path,
    })
}

fn read_gguf_content(path: &Path) -> Result<gguf_file::Content> {
    let mut file = std::fs::File::open(path)?;
    Ok(gguf_file::Content::read(&mut file)?)
}

pub(crate) fn gguf_u32(content: &gguf_file::Content, key: &str) -> Result<u32> {
    let value = content
        .metadata
        .get(key)
        .ok_or_else(|| OmniVoiceError::InvalidData(format!("missing GGUF metadata key `{key}`")))?;
    gguf_value_u32(value, key)
}

pub(crate) fn gguf_f64(content: &gguf_file::Content, key: &str) -> Result<f64> {
    let value = content
        .metadata
        .get(key)
        .ok_or_else(|| OmniVoiceError::InvalidData(format!("missing GGUF metadata key `{key}`")))?;
    match value {
        gguf_file::Value::F32(v) => Ok((*v).into()),
        gguf_file::Value::F64(v) => Ok(*v),
        other => Err(OmniVoiceError::InvalidData(format!(
            "GGUF metadata `{key}` is not a float: {other:?}"
        ))),
    }
}

pub(crate) fn gguf_array_usize(content: &gguf_file::Content, key: &str) -> Result<Vec<usize>> {
    let value = content
        .metadata
        .get(key)
        .ok_or_else(|| OmniVoiceError::InvalidData(format!("missing GGUF metadata key `{key}`")))?;
    let items = value.to_vec().map_err(|_| {
        OmniVoiceError::InvalidData(format!("GGUF metadata `{key}` is not an array"))
    })?;
    items
        .iter()
        .map(|item| {
            usize::try_from(gguf_value_u32(item, key)?).map_err(|_| {
                OmniVoiceError::InvalidData(format!("GGUF metadata `{key}` exceeds usize"))
            })
        })
        .collect()
}

fn gguf_value_u32(value: &gguf_file::Value, key: &str) -> Result<u32> {
    match value {
        gguf_file::Value::U8(v) => Ok((*v).into()),
        gguf_file::Value::I8(v) if *v >= 0 => Ok(*v as u32),
        gguf_file::Value::U16(v) => Ok((*v).into()),
        gguf_file::Value::I16(v) if *v >= 0 => Ok(*v as u32),
        gguf_file::Value::U32(v) => Ok(*v),
        gguf_file::Value::I32(v) if *v >= 0 => Ok(*v as u32),
        gguf_file::Value::U64(v) => u32::try_from(*v)
            .map_err(|_| OmniVoiceError::InvalidData(format!("GGUF metadata `{key}` exceeds u32"))),
        gguf_file::Value::I64(v) if *v >= 0 => u32::try_from(*v)
            .map_err(|_| OmniVoiceError::InvalidData(format!("GGUF metadata `{key}` exceeds u32"))),
        other => Err(OmniVoiceError::InvalidData(format!(
            "GGUF metadata `{key}` is not a non-negative integer: {other:?}"
        ))),
    }
}

#[derive(Debug, Clone)]
pub struct GeneratorArtifacts {
    config_path: PathBuf,
    weights_path: PathBuf,
    required_prefixes: BTreeSet<String>,
    ignored_keys: BTreeSet<String>,
    observed_prefixes: BTreeSet<String>,
}

impl GeneratorArtifacts {
    fn resolve(model_root: &Path, manifest: &GeneratorArtifactManifest) -> Result<Self> {
        let config_path = resolve_required_path(model_root, &manifest.config)?;
        let weights_path = resolve_required_path(model_root, &manifest.weights)?;
        let required_prefixes = manifest.required_prefixes.iter().cloned().collect();
        let ignored_keys = manifest.ignored_keys.iter().cloned().collect();
        let observed_prefixes =
            inspect_safetensor_prefixes(&weights_path, &ignored_keys, "generator")?;

        validate_generator_prefixes(&observed_prefixes, &required_prefixes)?;

        Ok(Self {
            config_path,
            weights_path,
            required_prefixes,
            ignored_keys,
            observed_prefixes,
        })
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn weights_path(&self) -> &Path {
        &self.weights_path
    }

    pub fn required_prefixes(&self) -> &BTreeSet<String> {
        &self.required_prefixes
    }

    pub fn ignored_keys(&self) -> &BTreeSet<String> {
        &self.ignored_keys
    }

    pub fn observed_prefixes(&self) -> &BTreeSet<String> {
        &self.observed_prefixes
    }
}

#[derive(Debug, Clone)]
pub struct TextTokenizerArtifacts {
    tokenizer_path: PathBuf,
    tokenizer_config_path: PathBuf,
    chat_template_path: Option<PathBuf>,
}

impl TextTokenizerArtifacts {
    fn resolve(model_root: &Path, manifest: &TextTokenizerArtifactManifest) -> Result<Self> {
        let tokenizer_path = resolve_required_path(model_root, &manifest.tokenizer)?;
        let tokenizer_config_path = resolve_required_path(model_root, &manifest.tokenizer_config)?;
        let chat_template_path =
            resolve_optional_path(model_root, manifest.metadata.chat_template.as_ref())?;

        Ok(Self {
            tokenizer_path,
            tokenizer_config_path,
            chat_template_path,
        })
    }

    pub fn tokenizer_path(&self) -> &Path {
        &self.tokenizer_path
    }

    pub fn tokenizer_config_path(&self) -> &Path {
        &self.tokenizer_config_path
    }

    pub fn chat_template_path(&self) -> Option<&Path> {
        self.chat_template_path.as_deref()
    }
}

#[derive(Debug, Clone)]
pub struct AudioTokenizerArtifacts {
    config_path: PathBuf,
    weights_path: PathBuf,
    preprocessor_config_path: PathBuf,
    license_path: Option<PathBuf>,
    required_prefixes: BTreeSet<String>,
    observed_prefixes: BTreeSet<String>,
}

impl AudioTokenizerArtifacts {
    fn resolve(model_root: &Path, manifest: &AudioTokenizerArtifactManifest) -> Result<Self> {
        let config_path = resolve_required_path(model_root, &manifest.config)?;
        let weights_path = resolve_required_path(model_root, &manifest.weights)?;
        let preprocessor_config_path =
            resolve_required_path(model_root, &manifest.preprocessor_config)?;
        let required_prefixes = manifest.required_prefixes.iter().cloned().collect();
        let observed_prefixes =
            inspect_safetensor_prefixes(&weights_path, &BTreeSet::new(), "audio_tokenizer")?;
        validate_minimum_prefixes(&observed_prefixes, &required_prefixes, "audio_tokenizer")?;
        let license_path = resolve_optional_path(model_root, manifest.metadata.license.as_ref())?;

        Ok(Self {
            config_path,
            weights_path,
            preprocessor_config_path,
            license_path,
            required_prefixes,
            observed_prefixes,
        })
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn weights_path(&self) -> &Path {
        &self.weights_path
    }

    pub fn preprocessor_config_path(&self) -> &Path {
        &self.preprocessor_config_path
    }

    pub fn license_path(&self) -> Option<&Path> {
        self.license_path.as_deref()
    }

    pub fn required_prefixes(&self) -> &BTreeSet<String> {
        &self.required_prefixes
    }

    pub fn observed_prefixes(&self) -> &BTreeSet<String> {
        &self.observed_prefixes
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ArtifactCase {
    DebugAutoEnShort,
    CloneUserRef,
    DesignEnBritish,
    DesignZhControl,
}

impl ArtifactCase {
    fn directory_name(self) -> &'static str {
        match self {
            Self::DebugAutoEnShort => "debug_auto_en_short",
            Self::CloneUserRef => "clone_user_ref",
            Self::DesignEnBritish => "design_en_british",
            Self::DesignZhControl => "design_zh_control",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReferenceArtifactBundle {
    root: PathBuf,
}

impl ReferenceArtifactBundle {
    pub fn from_root(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        if !root.exists() {
            return Err(OmniVoiceError::MissingArtifact { path: root });
        }
        Ok(Self { root })
    }

    pub fn available_case_ids(&self) -> Result<Vec<String>> {
        let mut cases = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let case_json = entry.path().join("case.json");
            if case_json.exists() {
                cases.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
        cases.sort();
        Ok(cases)
    }

    pub fn case(&self, case: ArtifactCase) -> Result<ReferenceCaseHandle> {
        self.case_by_id(case.directory_name())
    }

    pub fn case_by_id(&self, case_id: &str) -> Result<ReferenceCaseHandle> {
        validate_case_id(case_id)?;
        let case_dir = self.root.join(case_id);
        if !case_dir.exists() {
            return Err(OmniVoiceError::MissingArtifact { path: case_dir });
        }
        Ok(ReferenceCaseHandle { case_dir })
    }
}

pub type ArtifactBundle = ReferenceArtifactBundle;

#[derive(Debug, Clone)]
pub struct ReferenceCaseHandle {
    case_dir: PathBuf,
}

pub type ArtifactCaseHandle = ReferenceCaseHandle;

#[derive(Debug, Deserialize)]
struct PreparedJson {
    #[serde(default)]
    style_text: Option<String>,
    #[serde(default)]
    full_text: Option<String>,
    #[serde(default)]
    style_token_ids: Option<Vec<u32>>,
    #[serde(default)]
    text_token_ids: Option<Vec<u32>>,
    #[serde(default)]
    target_start_idx: Option<usize>,
    #[serde(default)]
    total_length: Option<usize>,
    #[serde(default)]
    texts: Vec<String>,
    #[serde(default)]
    langs: Vec<String>,
    #[serde(default)]
    instructs: Vec<Option<String>>,
    #[serde(default)]
    ref_texts: Vec<Option<String>>,
    #[serde(default)]
    ref_rms: Vec<Option<f32>>,
    #[serde(default)]
    generation_config: Option<PreparedGenerationConfig>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    segments: Option<PromptSegments>,
    #[serde(default)]
    prompt_contracts: Vec<PreparedPromptContractJson>,
    #[serde(default)]
    chunk_plan: Option<ChunkPlanJson>,
}

#[derive(Debug, Clone, Deserialize)]
struct PreparedPromptContractJson {
    #[serde(default)]
    mode: Option<String>,
    style_text: String,
    full_text: String,
    style_token_ids: Vec<u32>,
    text_token_ids: Vec<u32>,
    target_start_idx: usize,
    total_length: usize,
    segments: PromptSegments,
    #[serde(default = "default_prepared_input_key")]
    prepared_input_key: String,
    #[serde(default = "default_audio_mask_key")]
    audio_mask_key: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ChunkPlanJson {
    kind: String,
    #[serde(default)]
    chunk_texts: Vec<String>,
    #[serde(default)]
    chunk_target_lens_actual: Vec<usize>,
}

#[derive(Debug, Clone, Deserialize)]
struct PromptSegments {
    #[serde(default)]
    style_length: usize,
    #[serde(default)]
    text_length: usize,
    #[serde(default)]
    ref_audio_length: usize,
    target_length: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct PreparedGenerationConfig {
    #[serde(default = "default_denoise")]
    denoise: bool,
    #[serde(default = "default_postprocess_output")]
    postprocess_output: bool,
}

fn default_num_step() -> usize {
    32
}

fn default_guidance_scale() -> f32 {
    2.0
}

fn default_t_shift() -> f32 {
    0.1
}

fn default_layer_penalty_factor() -> f32 {
    5.0
}

fn default_position_temperature() -> f32 {
    5.0
}

fn default_class_temperature() -> f32 {
    0.0
}

fn default_denoise() -> bool {
    true
}

fn default_preprocess_prompt() -> bool {
    true
}

fn default_postprocess_output() -> bool {
    true
}

fn default_prepared_input_key() -> String {
    "prepared_input_ids".to_string()
}

fn default_audio_mask_key() -> String {
    "audio_mask".to_string()
}

#[derive(Debug, Clone, Deserialize)]
struct ReferenceCaseJson {
    text: String,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    instruct: Option<String>,
    #[serde(default)]
    ref_audio: Option<String>,
    #[serde(default)]
    ref_text: Option<String>,
    generation: ReferenceCaseGenerationJson,
}

#[derive(Debug, Clone, Deserialize)]
struct ReferenceCaseGenerationJson {
    #[serde(default = "default_num_step")]
    num_step: usize,
    #[serde(default = "default_guidance_scale")]
    guidance_scale: f32,
    #[serde(default = "default_t_shift")]
    t_shift: f32,
    #[serde(default = "default_layer_penalty_factor")]
    layer_penalty_factor: f32,
    #[serde(default = "default_position_temperature")]
    position_temperature: f32,
    #[serde(default = "default_class_temperature")]
    class_temperature: f32,
    #[serde(default = "default_denoise")]
    denoise: bool,
    #[serde(default = "default_preprocess_prompt")]
    preprocess_prompt: bool,
    #[serde(default = "default_postprocess_output")]
    postprocess_output: bool,
    #[serde(default)]
    duration: Option<f32>,
    #[serde(default)]
    speed: Option<f32>,
    #[serde(default = "default_audio_chunk_duration")]
    audio_chunk_duration: f32,
    #[serde(default = "default_audio_chunk_threshold")]
    audio_chunk_threshold: f32,
}

fn default_audio_chunk_duration() -> f32 {
    15.0
}

fn default_audio_chunk_threshold() -> f32 {
    30.0
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceCaseDefinition {
    pub request: GenerationRequest,
    pub audio_chunk_duration: f32,
    pub audio_chunk_threshold: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReferencePreparedMetadata {
    pub text: String,
    pub lang: Option<String>,
    pub instruct: Option<String>,
    pub ref_text: Option<String>,
    pub ref_rms: Option<f32>,
    pub mode: GenerationMode,
    pub denoise: bool,
    pub postprocess_output: bool,
    pub style_length: usize,
    pub text_length: usize,
    pub ref_audio_length: usize,
    pub target_length: usize,
    pub target_start_idx: usize,
    pub total_length: usize,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
enum DebugCaptureLayer {
    Index(usize),
    Name(String),
}

#[derive(Debug, Clone, Deserialize)]
struct DebugJson {
    capture_steps: Vec<usize>,
    capture_layers: Vec<DebugCaptureLayer>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceStage0DebugConfig {
    pub num_step: usize,
    pub guidance_scale: f32,
    pub t_shift: f32,
    pub layer_penalty_factor: f32,
    pub capture_steps: Vec<usize>,
    pub capture_hidden_layers: Vec<usize>,
    pub capture_final_hidden: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugInputs {
    prepared_input_ids: I64Tensor3,
    batch_input_ids: I64Tensor3,
    batch_audio_mask: BoolTensor2,
    batch_attention_mask: BoolTensor4,
    tokens_init: I64Tensor3,
}

impl DebugInputs {
    pub fn prepared_input_ids_dims(&self) -> (usize, usize, usize) {
        self.prepared_input_ids.dims()
    }

    pub fn batch_input_ids_dims(&self) -> (usize, usize, usize) {
        self.batch_input_ids.dims()
    }

    pub fn batch_input_ids(&self) -> &I64Tensor3 {
        &self.batch_input_ids
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForwardStepZero {
    pub inputs_embeds: F32Tensor3,
    pub hidden_layers: BTreeMap<usize, F32Tensor3>,
    pub final_hidden: F32Tensor3,
}

impl ForwardStepZero {
    pub fn inputs_embeds_dims(&self) -> (usize, usize, usize) {
        self.inputs_embeds.dims()
    }

    pub fn final_hidden_dims(&self) -> (usize, usize, usize) {
        self.final_hidden.dims()
    }

    pub fn hidden_layers(&self) -> &BTreeMap<usize, F32Tensor3> {
        &self.hidden_layers
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StepCapture {
    pub c_logits: F32Tensor4,
    pub u_logits: F32Tensor4,
    pub pred_tokens: I64Tensor3,
    pub confidence_scores: F32Tensor3,
    pub tokens_after_step: I64Tensor3,
    pub batch_input_ids_before_step: I64Tensor3,
}

impl StepCapture {
    pub fn c_logits_dims(&self) -> (usize, usize, usize, usize) {
        self.c_logits.dims()
    }

    pub fn u_logits_dims(&self) -> (usize, usize, usize, usize) {
        self.u_logits.dims()
    }

    pub fn tokens_after_step_dims(&self) -> (usize, usize, usize) {
        self.tokens_after_step.dims()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceStage1DebugCapture {
    pub tensors: BTreeMap<String, F32Tensor3>,
    pub raw_audio: DecodedAudio,
    pub final_audio: DecodedAudio,
}

impl ReferenceCaseHandle {
    pub fn load_prepared_prompts(&self) -> Result<PreparedPromptSequence> {
        let prepared = self.load_prepared_json()?;
        with_safetensors(self.case_dir.join("inputs.safetensors"), |tensors| {
            let contracts = prepared_prompt_contracts(&prepared)?;

            let prompts = contracts
                .iter()
                .map(|contract| load_prompt_contract(contract, tensors))
                .collect::<Result<Vec<_>>>()?;

            if let Some(chunk_plan) = prepared.chunk_plan.clone() {
                if chunk_plan.kind == "chunked" {
                    return Ok(PreparedPromptSequence::Chunked(ChunkedPreparedPrompts {
                        prompts,
                        chunk_texts: chunk_plan.chunk_texts,
                        chunk_target_lens: chunk_plan.chunk_target_lens_actual,
                    }));
                }
                if chunk_plan.kind != "single" {
                    return Err(OmniVoiceError::InvalidData(format!(
                        "unsupported chunk plan kind {}",
                        chunk_plan.kind
                    )));
                }
            }

            match prompts.as_slice() {
                [prompt] => Ok(PreparedPromptSequence::Single(prompt.clone())),
                _ => Err(OmniVoiceError::InvalidData(
                    "single prepared.json must contain exactly one prompt contract".to_string(),
                )),
            }
        })
    }

    pub fn load_prepared_prompt(&self) -> Result<PreparedPrompt> {
        match self.load_prepared_prompts()? {
            PreparedPromptSequence::Single(prompt) => Ok(prompt),
            PreparedPromptSequence::Chunked(_) => Err(OmniVoiceError::Unsupported(
                "requested a single prepared prompt from a chunked reference case".to_string(),
            )),
        }
    }

    pub fn load_prepared_metadata(&self) -> Result<ReferencePreparedMetadata> {
        let prepared = self.load_prepared_json()?;
        let contracts = prepared_prompt_contracts(&prepared)?;
        let contract = contracts.first().ok_or_else(|| {
            OmniVoiceError::InvalidData(
                "prepared.json does not contain any prompt contract".to_string(),
            )
        })?;
        let text = prepared
            .texts
            .first()
            .cloned()
            .unwrap_or_else(|| contract.full_text.clone());
        let lang = prepared.langs.first().cloned().map(Some).unwrap_or(None);
        let instruct = prepared.instructs.first().cloned().unwrap_or(None);
        let ref_text = prepared.ref_texts.first().cloned().unwrap_or(None);
        let ref_rms = prepared.ref_rms.first().copied().unwrap_or(None);
        let denoise = prepared
            .generation_config
            .as_ref()
            .map(|config| config.denoise)
            .unwrap_or(true);

        Ok(ReferencePreparedMetadata {
            text,
            lang,
            instruct,
            ref_text,
            ref_rms,
            mode: resolve_generation_mode(
                contract.mode.as_deref(),
                contract.segments.ref_audio_length,
            )?,
            denoise,
            postprocess_output: prepared
                .generation_config
                .as_ref()
                .map(|config| config.postprocess_output)
                .unwrap_or(true),
            style_length: contract.segments.style_length,
            text_length: contract.segments.text_length,
            ref_audio_length: contract.segments.ref_audio_length,
            target_length: contract.segments.target_length,
            target_start_idx: contract.target_start_idx,
            total_length: contract.total_length,
        })
    }

    pub fn load_stage0_debug_config(&self) -> Result<ReferenceStage0DebugConfig> {
        let debug = self.load_debug_json()?;
        let case: ReferenceCaseJson =
            serde_json::from_str(&fs::read_to_string(self.case_dir.join("case.json"))?)?;
        let mut capture_hidden_layers = Vec::new();
        let mut capture_final_hidden = false;
        for layer in debug.capture_layers {
            match layer {
                DebugCaptureLayer::Index(index) => capture_hidden_layers.push(index),
                DebugCaptureLayer::Name(name) if name == "final" => {
                    capture_final_hidden = true;
                }
                DebugCaptureLayer::Name(name) => {
                    return Err(OmniVoiceError::InvalidData(format!(
                        "unsupported debug capture layer {name}"
                    )));
                }
            }
        }
        capture_hidden_layers.sort_unstable();
        capture_hidden_layers.dedup();
        Ok(ReferenceStage0DebugConfig {
            num_step: case.generation.num_step,
            guidance_scale: case.generation.guidance_scale,
            t_shift: case.generation.t_shift,
            layer_penalty_factor: case.generation.layer_penalty_factor,
            capture_steps: debug.capture_steps,
            capture_hidden_layers,
            capture_final_hidden,
        })
    }

    pub fn load_case_definition(&self) -> Result<ReferenceCaseDefinition> {
        let case: ReferenceCaseJson =
            serde_json::from_str(&fs::read_to_string(self.case_dir.join("case.json"))?)?;

        let mut request = GenerationRequest::new_text_only(case.text);
        request.languages = vec![case.language];
        request.instructs = vec![case.instruct];
        request.ref_texts = vec![case.ref_text.clone()];
        request.durations = vec![case.generation.duration];
        request.speeds = vec![case.generation.speed];
        request.generation_config = GenerationConfig {
            num_step: case.generation.num_step,
            guidance_scale: case.generation.guidance_scale,
            t_shift: case.generation.t_shift,
            layer_penalty_factor: case.generation.layer_penalty_factor,
            position_temperature: case.generation.position_temperature,
            class_temperature: case.generation.class_temperature,
            denoise: case.generation.denoise,
            preprocess_prompt: case.generation.preprocess_prompt,
            postprocess_output: case.generation.postprocess_output,
            audio_chunk_duration: case.generation.audio_chunk_duration,
            audio_chunk_threshold: case.generation.audio_chunk_threshold,
        };

        if case.ref_audio.is_some() {
            let metadata = self.load_prepared_metadata()?;
            let prepared = self.load_prepared_prompt()?;
            let audio_start = metadata.style_length + metadata.text_length;
            let mut tokens = I64Tensor2::zeros((
                prepared.prompt.input_ids.dims().1,
                metadata.ref_audio_length,
            ));
            for codebook in 0..prepared.prompt.input_ids.dims().1 {
                for index in 0..metadata.ref_audio_length {
                    tokens.set(
                        codebook,
                        index,
                        prepared
                            .prompt
                            .input_ids
                            .get(0, codebook, audio_start + index),
                    );
                }
            }
            request.voice_clone_prompts = vec![Some(VoiceClonePrompt {
                ref_audio_tokens: tokens,
                ref_text: metadata
                    .ref_text
                    .clone()
                    .or(case.ref_text.clone())
                    .unwrap_or_default(),
                ref_rms: metadata.ref_rms,
            })];
            request.ref_texts = vec![None];
        }

        Ok(ReferenceCaseDefinition {
            request,
            audio_chunk_duration: case.generation.audio_chunk_duration,
            audio_chunk_threshold: case.generation.audio_chunk_threshold,
        })
    }

    pub fn build_generation_request(&self) -> Result<GenerationRequest> {
        Ok(self.load_case_definition()?.request)
    }

    pub fn load_debug_inputs(&self) -> Result<DebugInputs> {
        with_safetensors(self.case_dir.join("inputs.safetensors"), |tensors| {
            Ok(DebugInputs {
                prepared_input_ids: load_i64_tensor3(tensors, "prepared_input_ids")?,
                batch_input_ids: load_i64_tensor3(tensors, "batch_input_ids_before_step_00")?,
                batch_audio_mask: load_bool_tensor2(tensors, "batch_audio_mask")?,
                batch_attention_mask: load_bool_tensor4(tensors, "batch_attention_mask")?,
                tokens_init: load_i64_tensor3(tensors, "tokens_init")?,
            })
        })
    }

    pub fn load_forward_step_zero(&self) -> Result<ForwardStepZero> {
        with_safetensors(
            self.case_dir.join("forward_step_00.safetensors"),
            |tensors| {
                let mut hidden_layers = BTreeMap::new();
                for name in tensors.names() {
                    let Some(index_text) = name.strip_prefix("hidden_layer_") else {
                        continue;
                    };
                    let index = index_text.parse::<usize>().map_err(|error| {
                        OmniVoiceError::InvalidData(format!(
                            "invalid hidden layer name {name}: {error}"
                        ))
                    })?;
                    hidden_layers.insert(index, load_f32_tensor3(tensors, name)?);
                }
                Ok(ForwardStepZero {
                    inputs_embeds: load_f32_tensor3(tensors, "inputs_embeds")?,
                    hidden_layers,
                    final_hidden: load_f32_tensor3(tensors, "final_hidden")?,
                })
            },
        )
    }

    pub fn load_step_capture(&self, step: usize) -> Result<StepCapture> {
        with_safetensors(
            self.case_dir
                .join("steps")
                .join(format!("step_{step:02}.safetensors")),
            |tensors| {
                Ok(StepCapture {
                    c_logits: load_f32_tensor4(tensors, "c_logits")?,
                    u_logits: load_f32_tensor4(tensors, "u_logits")?,
                    pred_tokens: load_i64_tensor3(tensors, "pred_tokens")?,
                    confidence_scores: load_f32_tensor3(tensors, "confidence_scores")?,
                    tokens_after_step: load_i64_tensor3(tensors, "tokens_after_step")?,
                    batch_input_ids_before_step: load_i64_tensor3(
                        tensors,
                        "batch_input_ids_before_step",
                    )?,
                })
            },
        )
    }

    pub fn load_generated_tokens(&self) -> Result<GeneratedTokens> {
        with_safetensors(self.case_dir.join("final_tokens.safetensors"), |tensors| {
            if tensors.names().contains(&"tokens") {
                return Ok(GeneratedTokens::Single(load_i64_tensor2(
                    tensors, "tokens",
                )?));
            }

            let mut chunk_names = tensors
                .names()
                .iter()
                .filter(|name| name.starts_with("chunk_"))
                .cloned()
                .collect::<Vec<_>>();
            chunk_names.sort();
            if chunk_names.is_empty() {
                return Err(OmniVoiceError::InvalidData(
                    "final_tokens.safetensors does not contain `tokens` or `chunk_*` entries"
                        .to_string(),
                ));
            }

            let chunks = chunk_names
                .into_iter()
                .map(|name| load_i64_tensor2(tensors, name))
                .collect::<Result<Vec<_>>>()?;
            Ok(GeneratedTokens::Chunked(chunks))
        })
    }

    pub fn load_final_tokens(&self) -> Result<I64Tensor2> {
        match self.load_generated_tokens()? {
            GeneratedTokens::Single(tokens) => Ok(tokens),
            GeneratedTokens::Chunked(_) => Err(OmniVoiceError::Unsupported(
                "requested a single token tensor from a chunked reference case".to_string(),
            )),
        }
    }

    pub fn load_decoded_raw_audio(&self) -> Result<DecodedAudio> {
        DecodedAudio::read_wav(self.case_dir.join("decoded_raw.wav"))
    }

    pub fn load_final_audio(&self) -> Result<DecodedAudio> {
        DecodedAudio::read_wav(self.case_dir.join("final.wav"))
    }

    pub fn load_stage1_debug_capture(&self) -> Result<ReferenceStage1DebugCapture> {
        with_safetensors(self.case_dir.join("stage1_debug.safetensors"), |tensors| {
            let mut debug_tensors = BTreeMap::new();
            for name in tensors.names() {
                debug_tensors.insert(name.to_string(), load_f32_tensor3(tensors, name)?);
            }
            Ok(ReferenceStage1DebugCapture {
                tensors: debug_tensors,
                raw_audio: self.load_decoded_raw_audio()?,
                final_audio: self.load_final_audio()?,
            })
        })
    }

    fn load_prepared_json(&self) -> Result<PreparedJson> {
        Ok(serde_json::from_str(&fs::read_to_string(
            self.case_dir.join("prepared.json"),
        )?)?)
    }

    fn load_debug_json(&self) -> Result<DebugJson> {
        Ok(serde_json::from_str(&fs::read_to_string(
            self.case_dir.join("debug.json"),
        )?)?)
    }
}

fn prepared_prompt_contracts(prepared: &PreparedJson) -> Result<Vec<PreparedPromptContractJson>> {
    if !prepared.prompt_contracts.is_empty() {
        return Ok(prepared.prompt_contracts.clone());
    }

    let style_text = prepared.style_text.clone().ok_or_else(|| {
        OmniVoiceError::InvalidData("prepared.json is missing style_text".to_string())
    })?;
    let full_text = prepared.full_text.clone().ok_or_else(|| {
        OmniVoiceError::InvalidData("prepared.json is missing full_text".to_string())
    })?;
    let style_token_ids = prepared.style_token_ids.clone().ok_or_else(|| {
        OmniVoiceError::InvalidData("prepared.json is missing style_token_ids".to_string())
    })?;
    let text_token_ids = prepared.text_token_ids.clone().ok_or_else(|| {
        OmniVoiceError::InvalidData("prepared.json is missing text_token_ids".to_string())
    })?;
    let target_start_idx = prepared.target_start_idx.ok_or_else(|| {
        OmniVoiceError::InvalidData("prepared.json is missing target_start_idx".to_string())
    })?;
    let total_length = prepared.total_length.ok_or_else(|| {
        OmniVoiceError::InvalidData("prepared.json is missing total_length".to_string())
    })?;
    let segments = prepared.segments.clone().ok_or_else(|| {
        OmniVoiceError::InvalidData("prepared.json is missing segments".to_string())
    })?;

    Ok(vec![PreparedPromptContractJson {
        mode: prepared.mode.clone(),
        style_text,
        full_text,
        style_token_ids,
        text_token_ids,
        target_start_idx,
        total_length,
        segments,
        prepared_input_key: default_prepared_input_key(),
        audio_mask_key: default_audio_mask_key(),
    }])
}

fn load_prompt_contract(
    contract: &PreparedPromptContractJson,
    tensors: &SafeTensors<'_>,
) -> Result<PreparedPrompt> {
    Ok(PreparedPrompt {
        mode: resolve_generation_mode(
            contract.mode.as_deref(),
            contract.segments.ref_audio_length,
        )?,
        style_text: contract.style_text.clone(),
        full_text: contract.full_text.clone(),
        style_token_ids: contract.style_token_ids.clone(),
        text_token_ids: contract.text_token_ids.clone(),
        prompt: PromptTensorBundle {
            input_ids: load_i64_tensor3(tensors, &contract.prepared_input_key)?,
            audio_mask: load_bool_tensor2(tensors, &contract.audio_mask_key)?,
        },
        target_start_idx: contract.target_start_idx,
        total_length: contract.total_length,
        target_length: contract.segments.target_length,
        audio_mask_id: 1024,
    })
}

#[derive(Debug, Deserialize)]
struct GeneratorContractConfig {
    audio_vocab_size: usize,
    audio_mask_id: i64,
    num_audio_codebook: usize,
}

#[derive(Debug, Deserialize)]
struct AudioTokenizerContractConfig {
    sample_rate: u32,
    downsample_factor: usize,
}

#[derive(Debug, Deserialize)]
struct PreprocessorContractConfig {
    sampling_rate: u32,
    hop_length: usize,
}

fn validate_runtime_contracts(
    generator: &GeneratorArtifacts,
    audio_tokenizer: &AudioTokenizerArtifacts,
    contracts: &RuntimeContracts,
) -> Result<()> {
    let generator_config: GeneratorContractConfig =
        serde_json::from_str(&fs::read_to_string(generator.config_path())?)?;
    if generator_config.num_audio_codebook != contracts.num_audio_codebooks {
        return Err(OmniVoiceError::InvalidData(format!(
            "generator num_audio_codebook {} does not match manifest {}",
            generator_config.num_audio_codebook, contracts.num_audio_codebooks
        )));
    }
    if generator_config.audio_vocab_size != contracts.audio_vocab_size {
        return Err(OmniVoiceError::InvalidData(format!(
            "generator audio_vocab_size {} does not match manifest {}",
            generator_config.audio_vocab_size, contracts.audio_vocab_size
        )));
    }
    if generator_config.audio_mask_id != contracts.audio_mask_id {
        return Err(OmniVoiceError::InvalidData(format!(
            "generator audio_mask_id {} does not match manifest {}",
            generator_config.audio_mask_id, contracts.audio_mask_id
        )));
    }
    if contracts.num_audio_codebooks == 0 {
        return Err(OmniVoiceError::InvalidData(
            "manifest num_audio_codebooks must be greater than zero".to_string(),
        ));
    }
    if contracts.audio_vocab_size == 0 {
        return Err(OmniVoiceError::InvalidData(
            "manifest audio_vocab_size must be greater than zero".to_string(),
        ));
    }
    let audio_vocab_size_i64 = i64::try_from(contracts.audio_vocab_size).map_err(|_| {
        OmniVoiceError::InvalidData(format!(
            "manifest audio_vocab_size {} does not fit into i64",
            contracts.audio_vocab_size
        ))
    })?;
    if contracts.audio_mask_id < 0 || contracts.audio_mask_id >= audio_vocab_size_i64 {
        return Err(OmniVoiceError::InvalidData(format!(
            "manifest audio_mask_id {} is outside audio vocabulary size {}",
            contracts.audio_mask_id, contracts.audio_vocab_size
        )));
    }
    if contracts.sample_rate == 0 {
        return Err(OmniVoiceError::InvalidData(
            "manifest sample_rate must be greater than zero".to_string(),
        ));
    }
    if contracts.hop_length == 0 {
        return Err(OmniVoiceError::InvalidData(
            "manifest hop_length must be greater than zero".to_string(),
        ));
    }
    if contracts.frame_rate == 0 {
        return Err(OmniVoiceError::InvalidData(
            "manifest frame_rate must be greater than zero".to_string(),
        ));
    }
    if contracts.token_id_min < 0 {
        return Err(OmniVoiceError::InvalidData(format!(
            "manifest token_id_min {} must be non-negative",
            contracts.token_id_min
        )));
    }
    if contracts.token_id_min > contracts.token_id_max {
        return Err(OmniVoiceError::InvalidData(format!(
            "invalid token range {}..={}",
            contracts.token_id_min, contracts.token_id_max
        )));
    }
    if contracts.token_id_max >= audio_vocab_size_i64 {
        return Err(OmniVoiceError::InvalidData(format!(
            "manifest token_id_max {} is outside audio vocabulary size {}",
            contracts.token_id_max, contracts.audio_vocab_size
        )));
    }
    if contracts
        .token_id_range()
        .contains(&contracts.audio_mask_id)
    {
        return Err(OmniVoiceError::InvalidData(format!(
            "audio_mask_id {} must stay outside generation token range {}..={}",
            contracts.audio_mask_id, contracts.token_id_min, contracts.token_id_max
        )));
    }

    let audio_config: AudioTokenizerContractConfig =
        serde_json::from_str(&fs::read_to_string(audio_tokenizer.config_path())?)?;
    let preprocessor: PreprocessorContractConfig = serde_json::from_str(&fs::read_to_string(
        audio_tokenizer.preprocessor_config_path(),
    )?)?;

    if audio_config.sample_rate != contracts.sample_rate {
        return Err(OmniVoiceError::InvalidData(format!(
            "audio tokenizer sample_rate {} does not match manifest {}",
            audio_config.sample_rate, contracts.sample_rate
        )));
    }
    if preprocessor.sampling_rate != contracts.sample_rate {
        return Err(OmniVoiceError::InvalidData(format!(
            "preprocessor sampling_rate {} does not match manifest {}",
            preprocessor.sampling_rate, contracts.sample_rate
        )));
    }
    if preprocessor.hop_length != contracts.hop_length {
        return Err(OmniVoiceError::InvalidData(format!(
            "preprocessor hop_length {} does not match manifest {}",
            preprocessor.hop_length, contracts.hop_length
        )));
    }
    if audio_config.downsample_factor == 0 {
        return Err(OmniVoiceError::InvalidData(
            "audio tokenizer downsample_factor must be > 0".to_string(),
        ));
    }
    if preprocessor.hop_length == 0 {
        return Err(OmniVoiceError::InvalidData(
            "preprocessor hop_length must be > 0".to_string(),
        ));
    }
    let derived_frame_rate = usize::try_from(audio_config.sample_rate).map_err(|_| {
        OmniVoiceError::InvalidData(format!(
            "sample_rate {} does not fit into usize",
            audio_config.sample_rate
        ))
    })? / preprocessor.hop_length;
    if derived_frame_rate != contracts.frame_rate {
        return Err(OmniVoiceError::InvalidData(format!(
            "derived frame_rate {} does not match manifest {}",
            derived_frame_rate, contracts.frame_rate
        )));
    }

    Ok(())
}

fn resolve_required_path(model_root: &Path, relative: &Path) -> Result<PathBuf> {
    validate_relative_artifact_path(relative)?;
    let path = model_root.join(relative);
    if !path.is_file() {
        return Err(OmniVoiceError::MissingArtifact { path });
    }
    Ok(path)
}

fn resolve_optional_path(model_root: &Path, relative: Option<&PathBuf>) -> Result<Option<PathBuf>> {
    let Some(relative) = relative else {
        return Ok(None);
    };
    validate_relative_artifact_path(relative)?;
    let path = model_root.join(relative);
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(path))
}

fn validate_relative_artifact_path(path: &Path) -> Result<()> {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.contains(':')
        || normalized
            .split('/')
            .any(|component| component == "." || component == "..")
    {
        return Err(OmniVoiceError::InvalidData(format!(
            "artifact path must be relative and stay within the model root: {}",
            path.display()
        )));
    }
    Ok(())
}

fn validate_case_id(case_id: &str) -> Result<()> {
    let normalized = case_id.replace('\\', "/");
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.contains(':')
        || normalized
            .split('/')
            .any(|component| component == "." || component == "..")
    {
        return Err(OmniVoiceError::InvalidData(format!(
            "reference case id must be a single relative directory name: {case_id}"
        )));
    }
    if normalized.contains('/') {
        return Err(OmniVoiceError::InvalidData(format!(
            "reference case id must be a single relative directory name: {case_id}"
        )));
    }
    Ok(())
}

fn inspect_safetensor_prefixes(
    path: &Path,
    ignored_keys: &BTreeSet<String>,
    label: &str,
) -> Result<BTreeSet<String>> {
    let keys = read_safetensor_names(path)?;
    let mut prefixes = BTreeSet::new();
    for key in keys {
        if ignored_keys.contains(&key) {
            continue;
        }
        let prefix = key.split('.').next().ok_or_else(|| {
            OmniVoiceError::InvalidData(format!("{label} contains empty safetensors key"))
        })?;
        prefixes.insert(prefix.to_string());
    }
    Ok(prefixes)
}

fn read_safetensor_names(path: impl AsRef<Path>) -> Result<Vec<String>> {
    let parsed = read_safetensor_header_map(path.as_ref())?;
    Ok(parsed
        .keys()
        .filter(|key| *key != "__metadata__")
        .cloned()
        .collect())
}

fn read_safetensor_header_map(path: &Path) -> Result<serde_json::Map<String, serde_json::Value>> {
    let mut file = fs::File::open(path)?;
    let file_len = file.metadata()?.len();
    if file_len < 8 {
        return Err(OmniVoiceError::InvalidData(format!(
            "safetensors file is shorter than its header prefix: {}",
            path.display()
        )));
    }
    let mut header_len_bytes = [0_u8; 8];
    file.read_exact(&mut header_len_bytes)?;
    let header_len = usize::try_from(u64::from_le_bytes(header_len_bytes)).map_err(|_| {
        OmniVoiceError::InvalidData(format!(
            "safetensors header length does not fit into usize: {}",
            path.display()
        ))
    })?;
    const MAX_SAFETENSORS_HEADER_BYTES: usize = 64 * 1024 * 1024;
    if header_len > MAX_SAFETENSORS_HEADER_BYTES {
        return Err(OmniVoiceError::InvalidData(format!(
            "safetensors header is too large ({} bytes): {}",
            header_len,
            path.display()
        )));
    }
    let header_len_u64 = u64::try_from(header_len).map_err(|_| {
        OmniVoiceError::InvalidData(format!(
            "safetensors header length does not fit into u64: {}",
            path.display()
        ))
    })?;
    if header_len_u64 > file_len - 8 {
        return Err(OmniVoiceError::InvalidData(format!(
            "safetensors header extends past end of file: {}",
            path.display()
        )));
    }

    let buffer_len = 8usize.checked_add(header_len).ok_or_else(|| {
        OmniVoiceError::InvalidData("safetensors header length overflow".to_string())
    })?;
    let mut header_buffer = vec![0_u8; buffer_len];
    header_buffer[..8].copy_from_slice(&header_len_bytes);
    file.seek(SeekFrom::Start(8))?;
    file.read_exact(&mut header_buffer[8..])?;

    let header = std::str::from_utf8(&header_buffer[8..]).map_err(|error| {
        OmniVoiceError::InvalidData(format!(
            "invalid safetensors header utf-8 for {}: {error}",
            path.display()
        ))
    })?;
    Ok(serde_json::from_str(header)?)
}

fn validate_generator_prefixes(
    observed: &BTreeSet<String>,
    required: &BTreeSet<String>,
) -> Result<()> {
    validate_minimum_prefixes(observed, required, "generator")?;
    let unexpected: Vec<_> = observed.difference(required).cloned().collect();
    if !unexpected.is_empty() {
        return Err(OmniVoiceError::InvalidData(format!(
            "generator contains unexpected prefixes: {}",
            unexpected.join(", ")
        )));
    }
    Ok(())
}

fn validate_minimum_prefixes(
    observed: &BTreeSet<String>,
    required: &BTreeSet<String>,
    label: &str,
) -> Result<()> {
    let missing: Vec<_> = required.difference(observed).cloned().collect();
    if !missing.is_empty() {
        return Err(OmniVoiceError::InvalidData(format!(
            "{label} is missing required prefixes: {}",
            missing.join(", ")
        )));
    }
    Ok(())
}

fn with_safetensors<T>(
    path: impl AsRef<Path>,
    callback: impl FnOnce(&SafeTensors<'_>) -> Result<T>,
) -> Result<T> {
    let bytes = fs::read(path)?;
    let tensors = SafeTensors::deserialize(&bytes)?;
    callback(&tensors)
}

fn load_i64_tensor3(tensors: &SafeTensors<'_>, name: &str) -> Result<I64Tensor3> {
    let view = tensors.tensor(name)?;
    if view.dtype() != Dtype::I64 {
        return Err(OmniVoiceError::InvalidData(format!(
            "{name} expected I64 dtype, got {:?}",
            view.dtype()
        )));
    }
    I64Tensor3::new(shape3(view.shape())?, decode_i64(&view)?)
}

fn load_f32_tensor3(tensors: &SafeTensors<'_>, name: &str) -> Result<F32Tensor3> {
    let view = tensors.tensor(name)?;
    F32Tensor3::new(shape3(view.shape())?, decode_float_as_f32(&view)?)
}

fn load_f32_tensor4(tensors: &SafeTensors<'_>, name: &str) -> Result<F32Tensor4> {
    let view = tensors.tensor(name)?;
    F32Tensor4::new(shape4(view.shape())?, decode_float_as_f32(&view)?)
}

fn load_i64_tensor2(tensors: &SafeTensors<'_>, name: &str) -> Result<I64Tensor2> {
    let view = tensors.tensor(name)?;
    if view.dtype() != Dtype::I64 {
        return Err(OmniVoiceError::InvalidData(format!(
            "{name} expected I64 dtype, got {:?}",
            view.dtype()
        )));
    }
    I64Tensor2::new(shape2(view.shape())?, decode_i64(&view)?)
}

fn load_bool_tensor2(tensors: &SafeTensors<'_>, name: &str) -> Result<BoolTensor2> {
    let view = tensors.tensor(name)?;
    if view.dtype() != Dtype::BOOL {
        return Err(OmniVoiceError::InvalidData(format!(
            "{name} expected BOOL dtype, got {:?}",
            view.dtype()
        )));
    }
    BoolTensor2::new(
        shape2(view.shape())?,
        view.data().iter().map(|byte| *byte != 0).collect(),
    )
}

fn load_bool_tensor4(tensors: &SafeTensors<'_>, name: &str) -> Result<BoolTensor4> {
    let view = tensors.tensor(name)?;
    if view.dtype() != Dtype::BOOL {
        return Err(OmniVoiceError::InvalidData(format!(
            "{name} expected BOOL dtype, got {:?}",
            view.dtype()
        )));
    }
    BoolTensor4::new(
        shape4(view.shape())?,
        view.data().iter().map(|byte| *byte != 0).collect(),
    )
}

fn decode_i64(view: &TensorView<'_>) -> Result<Vec<i64>> {
    if !view.data().len().is_multiple_of(8) {
        return Err(OmniVoiceError::InvalidData(format!(
            "I64 byte length {} is not divisible by 8",
            view.data().len()
        )));
    }
    Ok(view
        .data()
        .as_chunks::<8>()
        .0
        .iter()
        .map(|chunk| {
            let mut array = [0_u8; 8];
            array.copy_from_slice(chunk);
            i64::from_le_bytes(array)
        })
        .collect())
}

fn decode_f32(view: &TensorView<'_>) -> Result<Vec<f32>> {
    if !view.data().len().is_multiple_of(4) {
        return Err(OmniVoiceError::InvalidData(format!(
            "F32 byte length {} is not divisible by 4",
            view.data().len()
        )));
    }
    Ok(view
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| {
            let mut array = [0_u8; 4];
            array.copy_from_slice(chunk);
            f32::from_le_bytes(array)
        })
        .collect())
}

fn decode_f16(view: &TensorView<'_>) -> Result<Vec<f32>> {
    if !view.data().len().is_multiple_of(2) {
        return Err(OmniVoiceError::InvalidData(format!(
            "F16 byte length {} is not divisible by 2",
            view.data().len()
        )));
    }
    Ok(view
        .data()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| {
            let mut array = [0_u8; 2];
            array.copy_from_slice(chunk);
            f16::from_bits(u16::from_le_bytes(array)).to_f32()
        })
        .collect())
}

fn decode_bf16(view: &TensorView<'_>) -> Result<Vec<f32>> {
    if !view.data().len().is_multiple_of(2) {
        return Err(OmniVoiceError::InvalidData(format!(
            "BF16 byte length {} is not divisible by 2",
            view.data().len()
        )));
    }
    Ok(view
        .data()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| {
            let mut array = [0_u8; 2];
            array.copy_from_slice(chunk);
            bf16::from_bits(u16::from_le_bytes(array)).to_f32()
        })
        .collect())
}

fn decode_float_as_f32(view: &TensorView<'_>) -> Result<Vec<f32>> {
    match view.dtype() {
        Dtype::F32 => decode_f32(view),
        Dtype::F16 => decode_f16(view),
        Dtype::BF16 => decode_bf16(view),
        other => Err(OmniVoiceError::InvalidData(format!(
            "expected floating-point dtype for debug tensor, got {other:?}"
        ))),
    }
}

fn shape2(shape: &[usize]) -> Result<(usize, usize)> {
    if let [a, b] = shape {
        Ok((*a, *b))
    } else {
        Err(OmniVoiceError::InvalidData(format!(
            "expected 2D shape, got {shape:?}"
        )))
    }
}

fn shape3(shape: &[usize]) -> Result<(usize, usize, usize)> {
    if let [a, b, c] = shape {
        Ok((*a, *b, *c))
    } else {
        Err(OmniVoiceError::InvalidData(format!(
            "expected 3D shape, got {shape:?}"
        )))
    }
}

fn shape4(shape: &[usize]) -> Result<(usize, usize, usize, usize)> {
    if let [a, b, c, d] = shape {
        Ok((*a, *b, *c, *d))
    } else {
        Err(OmniVoiceError::InvalidData(format!(
            "expected 4D shape, got {shape:?}"
        )))
    }
}

fn resolve_generation_mode(mode: Option<&str>, ref_audio_length: usize) -> Result<GenerationMode> {
    if let Some(mode) = mode {
        return match mode {
            "auto" => Ok(GenerationMode::Auto),
            "clone" => Ok(GenerationMode::Clone),
            "design" => Ok(GenerationMode::Design),
            other => Err(OmniVoiceError::InvalidData(format!(
                "unsupported generation mode {other}"
            ))),
        };
    }

    if ref_audio_length > 0 {
        Ok(GenerationMode::Clone)
    } else {
        Ok(GenerationMode::Auto)
    }
}

#[cfg(test)]
mod tests {
    use super::{GgmlDType, ModelWeightDType, classify_gguf_weight_dtype};

    #[test]
    fn gguf_weight_dtype_follows_unquantized_weights() {
        assert_eq!(
            classify_gguf_weight_dtype([GgmlDType::BF16, GgmlDType::BF16, GgmlDType::F32]),
            ModelWeightDType::BF16
        );
        assert_eq!(
            classify_gguf_weight_dtype([GgmlDType::F16, GgmlDType::F16]),
            ModelWeightDType::F16
        );
        assert_eq!(
            classify_gguf_weight_dtype([GgmlDType::F32, GgmlDType::F32]),
            ModelWeightDType::F32
        );
    }

    #[test]
    fn quantized_gguf_keeps_quantized_weight_dtype() {
        assert_eq!(
            classify_gguf_weight_dtype([GgmlDType::Q4K, GgmlDType::Q4K, GgmlDType::F32]),
            ModelWeightDType::Quantized
        );
    }
}
