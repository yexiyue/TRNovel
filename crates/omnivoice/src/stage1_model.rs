use candle_core::{IndexOp, Result as CandleResult, Tensor};
use candle_nn::Module;

use crate::{
    codec_layers::{
        CodecConv1d, CodecConvTranspose1d, CodecEmbedding, CodecLinear, CodecWeightSource,
    },
    contracts::I64Tensor2,
    error::{OmniVoiceError, Result},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage1ModelConfig {
    pub active_quantizer_indices: Vec<usize>,
    pub codebook_size: usize,
    pub codebook_dim: usize,
    pub quantizer_output_dim: usize,
    pub decoder_input_dim: usize,
    pub decoder_hidden_dim: usize,
    pub decoder_strides: Vec<usize>,
}

#[derive(Debug)]
pub struct Stage1DecodeTrace {
    pub project_out: Tensor,
    pub quantizer_output: Tensor,
    pub fc2_output: Tensor,
    pub decoder_input: Tensor,
    pub decoder_block_outputs: Vec<Tensor>,
    pub raw_waveform: Tensor,
}

#[derive(Debug)]
pub struct Stage1Model {
    quantizer: ResidualVectorQuantizer,
    fc2: CodecLinear,
    decoder: AcousticDecoder,
}

impl Stage1Model {
    pub fn load(ws: &CodecWeightSource, config: &Stage1ModelConfig) -> Result<Self> {
        let quantizer = ResidualVectorQuantizer::load(
            &ws.pp("quantizer"),
            &config.active_quantizer_indices,
            config.codebook_size,
            config.codebook_dim,
            config.quantizer_output_dim,
        )?;
        let fc2 = ws.load_linear_named(
            config.quantizer_output_dim,
            config.decoder_input_dim,
            true,
            "fc2.weight",
            "fc2.bias",
        )?;
        let decoder = AcousticDecoder::load(&ws.pp("acoustic_decoder"), config)?;
        Ok(Self {
            quantizer,
            fc2,
            decoder,
        })
    }

    pub fn decode_tokens(
        &self,
        tokens: &I64Tensor2,
        device: &candle_core::Device,
    ) -> Result<Tensor> {
        let tokens = tokens.to_candle(device)?.unsqueeze(0)?;
        self.decode_tensor(&tokens)
    }

    pub fn decode_tokens_with_trace(
        &self,
        tokens: &I64Tensor2,
        device: &candle_core::Device,
    ) -> Result<Stage1DecodeTrace> {
        let tokens = tokens.to_candle(device)?.unsqueeze(0)?;
        self.decode_tensor_with_trace(&tokens)
    }

    pub fn decode_tensor(&self, tokens: &Tensor) -> Result<Tensor> {
        let dims = tokens.dims();
        match dims {
            [_, _, _] => {}
            _ => {
                return Err(OmniVoiceError::InvalidTensorShape {
                    name: "stage1_model.tokens".to_string(),
                    expected: "(B, C, T)".to_string(),
                    actual: format!("{dims:?}"),
                });
            }
        }
        let quantizer_output = self.quantizer.decode_codes(tokens)?;
        let fc2_output = quantizer_output.apply(&self.fc2)?;
        let decoder_input = fc2_output.transpose(1, 2)?;
        self.decoder.forward(&decoder_input).map_err(Into::into)
    }

    pub fn decode_tensor_with_trace(&self, tokens: &Tensor) -> Result<Stage1DecodeTrace> {
        let dims = tokens.dims();
        match dims {
            [_, _, _] => {}
            _ => {
                return Err(OmniVoiceError::InvalidTensorShape {
                    name: "stage1_model.tokens".to_string(),
                    expected: "(B, C, T)".to_string(),
                    actual: format!("{dims:?}"),
                });
            }
        }
        let (project_out, quantizer_output) = self.quantizer.decode_codes_with_trace(tokens)?;
        let fc2_output = project_out.apply(&self.fc2)?;
        let decoder_input = fc2_output.transpose(1, 2)?;
        let (decoder_block_outputs, raw_waveform) =
            self.decoder.forward_with_trace(&decoder_input)?;
        Ok(Stage1DecodeTrace {
            project_out,
            quantizer_output,
            fc2_output,
            decoder_input,
            decoder_block_outputs,
            raw_waveform,
        })
    }
}

#[derive(Debug)]
struct VectorQuantizer {
    codebook: CodecEmbedding,
    project_out: CodecLinear,
}

impl VectorQuantizer {
    fn load(
        ws: &CodecWeightSource,
        codebook_size: usize,
        codebook_dim: usize,
        output_dim: usize,
    ) -> Result<Self> {
        Ok(Self {
            codebook: ws.load_embedding_at(codebook_size, codebook_dim, "codebook.embed")?,
            project_out: ws
                .pp("project_out")
                .load_linear(codebook_dim, output_dim, true)?,
        })
    }

    fn decode_codes(&self, ids: &Tensor) -> Result<Tensor> {
        let quantized = ids.apply(&self.codebook)?;
        quantized.apply(&self.project_out).map_err(Into::into)
    }

    fn decode_codes_with_trace(&self, ids: &Tensor) -> Result<(Tensor, Tensor)> {
        let quantized = ids.apply(&self.codebook)?;
        let projected = quantized.apply(&self.project_out)?;
        Ok((quantized, projected))
    }
}

#[derive(Debug)]
struct ResidualVectorQuantizer {
    quantizers: Vec<VectorQuantizer>,
}

impl ResidualVectorQuantizer {
    fn load(
        ws: &CodecWeightSource,
        active_quantizer_indices: &[usize],
        codebook_size: usize,
        codebook_dim: usize,
        output_dim: usize,
    ) -> Result<Self> {
        let quantizers = active_quantizer_indices
            .iter()
            .map(|index| {
                VectorQuantizer::load(
                    &ws.pp("quantizers").pp(*index),
                    codebook_size,
                    codebook_dim,
                    output_dim,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { quantizers })
    }

    fn decode_codes(&self, codes: &Tensor) -> Result<Tensor> {
        let (_, codebooks, _) = codes.dims3()?;
        if codebooks != self.quantizers.len() {
            return Err(OmniVoiceError::InvalidTensorShape {
                name: "stage1_model.codes".to_string(),
                expected: format!("(B, {}, T)", self.quantizers.len()),
                actual: format!("{:?}", codes.dims()),
            });
        }

        let mut projected_sum = None;
        for (index, quantizer) in self.quantizers.iter().enumerate() {
            let ids = codes.i((.., index, ..))?;
            let projected = quantizer.decode_codes(&ids)?;
            projected_sum = Some(match projected_sum {
                None => projected,
                Some(current) => (current + projected)?,
            });
        }

        projected_sum.ok_or_else(|| {
            OmniVoiceError::InvalidData("stage1 quantizer set must not be empty".to_string())
        })
    }

    fn decode_codes_with_trace(&self, codes: &Tensor) -> Result<(Tensor, Tensor)> {
        let (_, codebooks, _) = codes.dims3()?;
        if codebooks != self.quantizers.len() {
            return Err(OmniVoiceError::InvalidTensorShape {
                name: "stage1_model.codes".to_string(),
                expected: format!("(B, {}, T)", self.quantizers.len()),
                actual: format!("{:?}", codes.dims()),
            });
        }

        let mut projected_sum = None;
        for (index, quantizer) in self.quantizers.iter().enumerate() {
            let ids = codes.i((.., index, ..))?;
            let (_, projected) = quantizer.decode_codes_with_trace(&ids)?;
            projected_sum = Some(match projected_sum {
                None => projected,
                Some(current) => (current + projected)?,
            });
        }

        let project_out = projected_sum.ok_or_else(|| {
            OmniVoiceError::InvalidData("stage1 quantizer set must not be empty".to_string())
        })?;
        let quantizer_output = project_out.transpose(1, 2)?;
        Ok((project_out, quantizer_output))
    }
}

#[derive(Debug, Clone)]
struct Snake1d {
    alpha: Tensor,
}

impl Snake1d {
    fn load(channels: usize, ws: &CodecWeightSource) -> Result<Self> {
        let alpha = match ws.get((1, channels, 1), "alpha") {
            Ok(alpha) => alpha,
            Err(_) => ws.get((channels, 1), "alpha")?.reshape((1, channels, 1))?,
        };
        Ok(Self { alpha })
    }
}

impl Module for Snake1d {
    fn forward(&self, xs: &Tensor) -> CandleResult<Tensor> {
        let xs_shape = xs.shape();
        let xs = xs.flatten_from(2)?;
        let sin = self.alpha.broadcast_mul(&xs)?.sin()?;
        let sin = (&sin * &sin)?;
        (xs + (&self.alpha + 1e-9)?.recip()?.broadcast_mul(&sin)?)?.reshape(xs_shape)
    }
}

#[derive(Debug, Clone)]
struct ResidualUnit {
    snake1: Snake1d,
    conv1: CodecConv1d,
    snake2: Snake1d,
    conv2: CodecConv1d,
}

impl ResidualUnit {
    fn load(dim: usize, dilation: usize, ws: &CodecWeightSource) -> Result<Self> {
        Ok(Self {
            snake1: Snake1d::load(dim, &ws.pp("snake1"))?,
            conv1: ws.pp("conv1").load_conv1d_flexible(
                dim,
                dim,
                7,
                dilation,
                ((7 - 1) * dilation) / 2,
            )?,
            snake2: Snake1d::load(dim, &ws.pp("snake2"))?,
            conv2: ws.pp("conv2").load_conv1d_flexible(dim, dim, 1, 1, 0)?,
        })
    }
}

impl Module for ResidualUnit {
    fn forward(&self, xs: &Tensor) -> CandleResult<Tensor> {
        let ys = xs
            .apply(&self.snake1)?
            .apply(&self.conv1)?
            .apply(&self.snake2)?
            .apply(&self.conv2)?;
        let pad = (xs.dim(candle_core::D::Minus1)? - ys.dim(candle_core::D::Minus1)?) / 2;
        if pad > 0 {
            Ok((&ys + xs.narrow(candle_core::D::Minus1, pad, ys.dim(candle_core::D::Minus1)?)?)?)
        } else {
            Ok((ys + xs)?)
        }
    }
}

#[derive(Debug, Clone)]
struct DecoderBlock {
    snake1: Snake1d,
    conv_t1: CodecConvTranspose1d,
    res_unit1: ResidualUnit,
    res_unit2: ResidualUnit,
    res_unit3: ResidualUnit,
}

impl DecoderBlock {
    fn load(in_dim: usize, out_dim: usize, stride: usize, ws: &CodecWeightSource) -> Result<Self> {
        Ok(Self {
            snake1: Snake1d::load(in_dim, &ws.pp("snake1"))?,
            conv_t1: ws.pp("conv_t1").load_conv_transpose1d(
                in_dim,
                out_dim,
                2 * stride,
                stride,
                stride.div_ceil(2),
                stride % 2,
            )?,
            res_unit1: ResidualUnit::load(out_dim, 1, &ws.pp("res_unit1"))?,
            res_unit2: ResidualUnit::load(out_dim, 3, &ws.pp("res_unit2"))?,
            res_unit3: ResidualUnit::load(out_dim, 9, &ws.pp("res_unit3"))?,
        })
    }
}

impl Module for DecoderBlock {
    fn forward(&self, xs: &Tensor) -> CandleResult<Tensor> {
        xs.apply(&self.snake1)?
            .apply(&self.conv_t1)?
            .apply(&self.res_unit1)?
            .apply(&self.res_unit2)?
            .apply(&self.res_unit3)
    }
}

#[derive(Debug, Clone)]
struct AcousticDecoder {
    conv1: CodecConv1d,
    blocks: Vec<DecoderBlock>,
    snake1: Snake1d,
    conv2: CodecConv1d,
}

impl AcousticDecoder {
    fn load(ws: &CodecWeightSource, config: &Stage1ModelConfig) -> Result<Self> {
        let conv1 = ws.pp("conv1").load_conv1d_flexible(
            config.decoder_input_dim,
            config.decoder_hidden_dim,
            7,
            1,
            3,
        )?;
        let mut channels = config.decoder_hidden_dim;
        let mut blocks = Vec::with_capacity(config.decoder_strides.len());
        for (index, stride) in config.decoder_strides.iter().enumerate() {
            let block =
                DecoderBlock::load(channels, channels / 2, *stride, &ws.pp("block").pp(index))?;
            channels /= 2;
            blocks.push(block);
        }
        Ok(Self {
            conv1,
            blocks,
            snake1: Snake1d::load(channels, &ws.pp("snake1"))?,
            conv2: ws.pp("conv2").load_conv1d_flexible(channels, 1, 7, 1, 3)?,
        })
    }

    fn forward_with_trace(&self, xs: &Tensor) -> Result<(Vec<Tensor>, Tensor)> {
        let mut xs = xs.apply(&self.conv1)?;
        let mut block_outputs = Vec::with_capacity(self.blocks.len());
        for block in self.blocks.iter() {
            xs = xs.apply(block)?;
            block_outputs.push(xs.clone());
        }
        let raw_waveform = xs.apply(&self.snake1)?.apply(&self.conv2)?;
        Ok((block_outputs, raw_waveform))
    }
}

impl Module for AcousticDecoder {
    fn forward(&self, xs: &Tensor) -> CandleResult<Tensor> {
        let mut xs = xs.apply(&self.conv1)?;
        for block in self.blocks.iter() {
            xs = xs.apply(block)?;
        }
        xs.apply(&self.snake1)?.apply(&self.conv2)
    }
}
