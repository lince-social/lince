use crate::infrastructure::paths;
use std::{fs, io, path::PathBuf};

#[derive(Debug, Clone)]
pub struct PromptFragment {
    pub file_name: String,
    pub body: String,
}

impl PromptFragment {
    pub fn summary(&self) -> String {
        let summary = first_meaningful_line(&self.body)
            .map(sanitize_summary_line)
            .filter(|line| !line.is_empty())
            .unwrap_or_else(|| "Sem resumo visivel no arquivo.".to_string());
        format!("{}: {}", self.file_name, summary)
    }
}

pub fn load_widget_builder_prompt_fragments() -> io::Result<Vec<PromptFragment>> {
    let dir = paths::widget_builder_prompt_dir();
    let mut entries = fs::read_dir(&dir)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter_map(|entry| {
            let path = entry.path();
            if path.is_file() { Some(path) } else { None }
        })
        .collect::<Vec<_>>();

    entries.sort_by(|left, right| left.file_name().cmp(&right.file_name()));

    let fragments = entries
        .into_iter()
        .map(load_prompt_fragment)
        .collect::<io::Result<Vec<_>>>()?;

    if fragments.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("No prompt fragments found in {}", dir.display()),
        ));
    }

    Ok(fragments)
}

pub fn build_widget_builder_system_prompt() -> io::Result<String> {
    let fragments = load_widget_builder_prompt_fragments()?;
    let mut prompt = String::new();

    for fragment in fragments {
        if !prompt.is_empty() {
            prompt.push_str("\n\n");
        }
        prompt.push_str(&format!("Prompt fragment: {}\n", fragment.file_name));
        prompt.push_str(fragment.body.trim());
    }

    Ok(prompt)
}

pub fn load_widget_builder_contract_summaries() -> io::Result<Vec<String>> {
    load_widget_builder_prompt_fragments().map(|fragments| {
        fragments
            .into_iter()
            .map(|fragment| fragment.summary())
            .collect()
    })
}

fn load_prompt_fragment(path: PathBuf) -> io::Result<PromptFragment> {
    let bytes = fs::read(&path)?;
    Ok(PromptFragment {
        file_name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string()),
        body: String::from_utf8_lossy(&bytes).into_owned(),
    })
}

fn first_meaningful_line(body: &str) -> Option<&str> {
    body.lines().map(str::trim).find(|line| {
        !line.is_empty()
            && !matches!(*line, "---" | "```")
            && !line.starts_with("```")
            && !line.starts_with('#')
    })
}

fn sanitize_summary_line(line: &str) -> String {
    line.trim_start_matches(['-', '*', '>', '`', ' '])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
