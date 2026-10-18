use super::*;
use std::path::Path;

pub(super) const CHECKPOINT: Checkpoint = Checkpoint {
    id: "english-modern-nli",
    revision: "d421c4545a438fd006fb43f8b981c5d908faa1e1",
    repository: "MoritzLaurer/ModernBERT-base-zeroshot-v2.0",
    license: "Apache-2.0",
    modern: true,
    artifacts: &[
        Artifact {
            name: "model.safetensors",
            bytes: 299_227_092,
            sha256: "5af4dac82bf3da16575d0c71c8c96ee7eb6621ae5f4f4726943d0a3583b44b46",
        },
        Artifact {
            name: "config.json",
            bytes: 1469,
            sha256: "d5c53ad1e5a23066144e958db475b1f8df7e8e45bf0ef6aba8a842c7f6450be1",
        },
        Artifact {
            name: "tokenizer.json",
            bytes: 3_583_228,
            sha256: "6c8aaa9a542084f2457eab775d4eeb51f92a70c0fd9de28d5edb0ddec3c08d30",
        },
        Artifact {
            name: "tokenizer_config.json",
            bytes: 20_839,
            sha256: "c5a7dfd44d93c5d7ed9b2ddcd7a6018e935ec7d53f1de45910ea9868e0ff1c17",
        },
    ],
};

pub(super) fn load(directory: &Path, mut raw: serde_json::Value) -> Result<Classifier, String> {
    if raw["model_type"] != "modernbert"
        || raw["classifier_pooling"] != "mean"
        || raw["classifier_bias"] != false
        || raw["id2label"]["0"] != "entailment"
        || raw["label2id"]["entailment"] != 0
        || raw["label2id"]["not_entailment"] != 1
    {
        return Err("Modern NLI checkpoint declares an unsupported classifier.".into());
    }
    let labels = raw["label2id"]
        .as_object_mut()
        .ok_or("Modern NLI labels are missing.")?;
    for value in labels.values_mut() {
        let id = value
            .as_u64()
            .ok_or("Modern NLI labels require numeric indices.")?;
        *value = serde_json::Value::String(id.to_string());
    }
    let config: laya::modernbert::Config =
        serde_json::from_value(raw).map_err(|error| error.to_string())?;
    let tokenizer = Tokenizer::from_file(directory.join("tokenizer.json"))
        .map_err(|error| error.to_string())?;
    let tensors = candle_core::safetensors::load(directory.join("model.safetensors"), &Device::Cpu)
        .and_then(|tensors| {
            tensors
                .into_iter()
                .map(|(name, tensor)| Ok((name, tensor.to_dtype(DType::F32)?)))
                .collect::<candle_core::Result<std::collections::HashMap<_, _>>>()
        })
        .map_err(|error| error.to_string())?;
    let builder = VarBuilder::from_tensors(tensors, DType::F32, &Device::Cpu);
    let encoder = laya::modernbert::ModernBert::load(builder.clone(), &config)
        .map_err(|error| error.to_string())?;
    let projection = candle_nn::linear_no_bias(
        config.hidden_size,
        config.hidden_size,
        builder.pp("head.dense"),
    )
    .map_err(|error| error.to_string())?;
    let norm = candle_nn::layer_norm_no_bias(
        config.hidden_size,
        config.layer_norm_eps,
        builder.pp("head.norm"),
    )
    .map_err(|error| error.to_string())?;
    let head = candle_nn::linear_no_bias(config.hidden_size, 2, builder.pp("classifier"))
        .map_err(|error| error.to_string())?;
    Ok(Classifier {
        tokenizer,
        graph: Graph::Modern {
            encoder,
            projection,
            norm,
            head,
        },
        maximum_tokens: config.max_position_embeddings.min(512),
    })
}
