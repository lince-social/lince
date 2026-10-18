use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokenizers::Tokenizer;

pub const MAX_OPTION_TOKENS: usize = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QType {
    Choice,
    Score,
    Noul,
}

impl QType {
    pub fn index(self) -> usize {
        match self {
            QType::Choice => 0,
            QType::Score => 1,
            QType::Noul => 2,
        }
    }

    pub fn from_index(i: usize) -> Self {
        match i {
            0 => QType::Choice,
            1 => QType::Score,
            _ => QType::Noul,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            QType::Choice => "choice",
            QType::Score => "score",
            QType::Noul => "noul",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Question {
    #[serde(rename = "type")]
    pub qtype: QType,
    pub instructions: Value,
    #[serde(default)]
    pub criteria: Option<Value>,
}

impl Question {
    pub fn choice<I, S>(instructions: &str, options: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let opts: Vec<Value> = options
            .into_iter()
            .map(|o| Value::String(o.into()))
            .collect();
        Self {
            qtype: QType::Choice,
            instructions: Value::String(instructions.to_string()),
            criteria: Some(Value::Array(opts)),
        }
    }

    pub fn score<I, S>(instructions: &str, levels: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let levels: Vec<Value> = levels
            .into_iter()
            .map(|o| Value::String(o.into()))
            .collect();
        Self {
            qtype: QType::Score,
            instructions: Value::String(instructions.to_string()),
            criteria: Some(Value::Array(levels)),
        }
    }

    pub fn noul(instructions: &str) -> Self {
        Self {
            qtype: QType::Noul,
            instructions: Value::String(instructions.to_string()),
            criteria: None,
        }
    }

    pub(crate) fn instructions_text(&self) -> String {
        match &self.instructions {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        }
    }

    pub(crate) fn choice_keys(&self) -> Result<Vec<String>> {
        match &self.criteria {
            Some(Value::Array(a)) => a
                .iter()
                .map(|v| match v {
                    Value::String(s) => Ok(s.clone()),
                    other => Ok(other.to_string()),
                })
                .collect(),
            Some(Value::Object(m)) => Ok(m.keys().cloned().collect()),
            _ => bail!("choice question needs `criteria` as a list or an object"),
        }
    }

    pub fn render_options(&self) -> Result<Vec<String>> {
        match self.qtype {
            QType::Choice => match &self.criteria {
                Some(Value::Array(a)) => Ok(a
                    .iter()
                    .map(|v| match v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .collect()),
                Some(Value::Object(m)) => Ok(m
                    .iter()
                    .map(|(k, v)| match v {
                        Value::Null => k.clone(),
                        Value::String(s) if s.is_empty() => k.clone(),
                        Value::String(s) => format!("{k}: {s}"),
                        other => format!("{k}: {other}"),
                    })
                    .collect()),
                _ => bail!("choice question needs `criteria` as a list or an object"),
            },
            QType::Score => match &self.criteria {
                Some(Value::Array(a)) => Ok(a
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let c = match v {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        format!("level {i}: {c}")
                    })
                    .collect()),
                _ => bail!("score question needs `criteria` as a list of levels"),
            },
            QType::Noul => {
                let empty = Map::new();
                let m = match &self.criteria {
                    Some(Value::Object(m)) => m,
                    _ => &empty,
                };
                let side = |k: &str, default: &str| -> String {
                    match m.get(k) {
                        Some(Value::String(s)) if !s.is_empty() => s.clone(),
                        _ => default.to_string(),
                    }
                };
                Ok(vec![
                    format!(
                        "false: {}",
                        side("false", "no, the statement does not hold")
                    ),
                    format!("true: {}", side("true", "yes, the statement holds")),
                ])
            }
        }
    }

    pub(crate) fn score_levels(&self) -> Vec<String> {
        match &self.criteria {
            Some(Value::Array(a)) => a
                .iter()
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}

pub fn serialize_state(state: &Value) -> String {
    match state {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub(crate) struct Encoded {
    pub ids: Vec<u32>,
    pub markers: Vec<usize>,
    pub head_sep: usize,
    pub opts_sep: usize,
}

pub(crate) struct SpecialIds {
    pub cls: u32,
    pub sep: u32,
    pub mask: u32,
    pub pad: u32,
    pub mask_text: String,
}

fn encode_plain(tok: &Tokenizer, text: &str) -> Result<Vec<u32>> {
    tok.encode(text, false)
        .map(|e| e.get_ids().to_vec())
        .map_err(|e| anyhow::anyhow!("tokenization failed: {e}"))
}

pub(crate) fn build_sequence(
    tok: &Tokenizer,
    sp: &SpecialIds,
    state: &Value,
    q: &Question,
    max_len: usize,
    head_max_len: usize,
) -> Result<Encoded> {
    let opts = q.render_options()?;
    if opts.is_empty() {
        bail!(
            "a {} question needs at least one option in `criteria`",
            q.qtype.as_str()
        );
    }
    let scrub = |s: &str| s.replace(&sp.mask_text, " ");

    let ins = scrub(&q.instructions_text());
    let mut head_ids = encode_plain(tok, &format!("{} question: {}", q.qtype.as_str(), ins))?;

    let mut opt_ids: Vec<Vec<u32>> = Vec::with_capacity(opts.len());
    for o in &opts {
        let mut ids = vec![sp.mask];
        let mut body = encode_plain(tok, &format!(" {}", scrub(o)))?;
        body.truncate(MAX_OPTION_TOKENS);
        ids.extend(body);
        opt_ids.push(ids);
    }

    let used: usize = opt_ids.iter().map(|o| o.len()).sum();
    let mut opt_budget = head_max_len as isize - used as isize;
    if opt_budget < 16 {
        let per = std::cmp::max(
            4,
            head_max_len.saturating_sub(16) / std::cmp::max(1, opt_ids.len()),
        );
        for o in opt_ids.iter_mut() {
            o.truncate(per);
        }
        let used: usize = opt_ids.iter().map(|o| o.len()).sum();
        opt_budget = head_max_len as isize - used as isize;
    }
    head_ids.truncate(std::cmp::max(8, opt_budget.max(0) as usize));

    let mut ids = Vec::with_capacity(max_len);
    ids.push(sp.cls);
    ids.extend_from_slice(&head_ids);
    let head_sep = ids.len();
    ids.push(sp.sep);

    let mut markers = Vec::with_capacity(opt_ids.len());
    for o in &opt_ids {
        markers.push(ids.len());
        ids.extend_from_slice(o);
    }
    let opts_sep = ids.len();
    ids.push(sp.sep);

    let room = max_len.saturating_sub(ids.len() + 1);
    let mut st = encode_plain(tok, &scrub(&serialize_state(state)))?;
    st.truncate(room);
    ids.extend_from_slice(&st);
    ids.push(sp.sep);
    ids.truncate(max_len);

    let markers: Vec<usize> = markers.into_iter().filter(|m| *m < max_len).collect();
    if markers.len() != opts.len() {
        bail!(
            "options do not fit in head_max_len={head_max_len} tokens ({} of {} markers kept)",
            markers.len(),
            opts.len()
        );
    }
    Ok(Encoded {
        ids,
        markers,
        head_sep,
        opts_sep,
    })
}
