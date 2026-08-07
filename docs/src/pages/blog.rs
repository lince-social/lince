use crate::{html::page, i18n::Translations};
use maud::{PreEscaped, html};
use std::{
    collections::HashMap,
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::Command,
};

const BLOG_POSTS_ROOT: &str = "content/blog/posts";

#[derive(Default)]
struct BlogMetadata {
    title: Option<String>,
    date: Option<String>,
    video_url: Option<String>,
}

pub fn compile_blog_body(source_path: &str) -> String {
    // 1. Run the CLI: typst compile <path> --format html -
    // The "-" at the end tells typst to output to stdout instead of a file
    let output = Command::new("typst")
        .arg("compile")
        .arg(source_path)
        .arg("--root")
        .arg(".")
        .arg("--format")
        .arg("html")
        .arg("--features")
        .arg("html")
        .arg("-")
        .output()
        .expect("Typst CLI not found. Install it with 'cargo install typst-cli'");

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        panic!("Typst Error in {}: {}", source_path, err);
    }

    let full_html = String::from_utf8_lossy(&output.stdout).to_string();

    // 2. Extract the inner body
    extract_body(full_html)
}

fn generate_svg_sidecar(stem: &str, source_path: &Path) -> Option<String> {
    let sidecar_dir = Path::new("output/assets/blog/.cache");
    let _ = fs::create_dir_all(sidecar_dir);
    let fingerprint = blog_sidecar_fingerprint(source_path)?;
    let sidecar_path = format!("output/assets/blog/.cache/{stem}-{fingerprint:016x}.svg");
    let sidecar = Path::new(&sidecar_path);

    let valid_cached_sidecar = fs::read_to_string(sidecar)
        .ok()
        .map(|s| s.contains("<svg class=\"typst-doc\""))
        .unwrap_or(false);

    if valid_cached_sidecar {
        return Some(sidecar_path);
    }

    let output = Command::new("tinymist")
        .arg("compile")
        .arg(source_path)
        .arg(&sidecar_path)
        .arg("--root")
        .arg(".")
        .arg("--format")
        .arg("svg")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    Some(sidecar_path)
}

fn blog_sidecar_fingerprint(source_path: &Path) -> Option<u64> {
    let mut hasher = DefaultHasher::new();
    source_path.to_string_lossy().hash(&mut hasher);
    fs::read(source_path).ok()?.hash(&mut hasher);
    fs::read("content/blog/components.typ")
        .ok()?
        .hash(&mut hasher);
    fs::read("content/blog/tmil.typ").ok()?.hash(&mut hasher);
    Some(hasher.finish())
}

fn extract_first_block(input: &str, start_marker: &str, end_marker: &str) -> Option<String> {
    let start = input.find(start_marker)?;
    let end_rel = input[start..].find(end_marker)?;
    let end = start + end_rel + end_marker.len();
    Some(input[start..end].to_string())
}

fn extract_balanced_svg(input: &str, start: usize) -> Option<String> {
    let mut cursor = start;
    let mut depth = 0usize;

    while cursor < input.len() {
        let next_open = input[cursor..].find("<svg").map(|p| cursor + p);
        let next_close = input[cursor..].find("</svg>").map(|p| cursor + p);

        match (next_open, next_close) {
            (Some(open), Some(close)) if open < close => {
                depth += 1;
                cursor = open + 4;
            }
            (_, Some(close)) => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                cursor = close + "</svg>".len();
                if depth == 0 {
                    return Some(input[start..cursor].to_string());
                }
            }
            _ => return None,
        }
    }

    None
}

fn extract_typst_doc_svg(input: &str) -> Option<String> {
    if let Some(app_start) = input.find("<div id=\"typst-app\"")
        && let Some(svg_rel_start) = input[app_start..].find("<svg") {
            let svg_start = app_start + svg_rel_start;
            if let Some(svg) = extract_balanced_svg(input, svg_start) {
                return Some(svg);
            }
        }

    // Support static Tinymist HTML exports where the document SVG is emitted directly.
    if let Some(svg_start) = input.find("<svg class=\"typst-doc\"") {
        return extract_balanced_svg(input, svg_start);
    }

    None
}

fn tinymist_native_html(sidecar_path: &str, stem: &str) -> Option<String> {
    let raw = fs::read_to_string(sidecar_path).ok()?;
    let resources = extract_first_block(&raw, "<svg id=\"typst-svg-resources\"", "</svg>")
        .or_else(|| extract_first_block(&raw, "<svg class=\"typst-svg-resources\"", "</svg>"))
        .unwrap_or_default();
    let doc_svg = extract_typst_doc_svg(&raw)?;

    Some(format!(
        r##"<div class="blog_post_embed">
  <div class="tinymist-native" id="tinymist-native-{stem}">
    {resources}
    {doc_svg}
  </div>
</div>
<script>
(function() {{
  const root = document.getElementById("tinymist-native-{stem}");
  if (!root) {{
    return;
  }}
  const svg = root.querySelector("svg.typst-doc");
  if (!svg) {{
    return;
  }}

  const pages = Array.from(svg.querySelectorAll("g.typst-page"));
  let y = 0;
  const pageGap = 10;
  for (const page of pages) {{
    let targetY = y;
    let advance = Number(page.getAttribute("data-page-height")) || 0;
    try {{
      // Measure only page content so pages collapse without huge blank bands.
      const content = page.querySelector("g.typst-group") || page;
      const box = content.getBBox();
      if (box.height > 0) {{
        targetY = y - box.y;
        advance = box.height + pageGap;
      }}
    }} catch (_err) {{
      // Keep data-page-height fallback when geometry isn't measurable.
    }}
    page.setAttribute("transform", `translate(0, ${{targetY}})`);
    y += advance;
  }}

  const width = Number(svg.getAttribute("data-width"))
    || Number((pages[0] && pages[0].getAttribute("data-page-width")) || 596);
  if (width > 0 && y > 0) {{
    svg.setAttribute("viewBox", `0 0 ${{width}} ${{y}}`);
  }}

  svg.removeAttribute("height");
  svg.setAttribute("width", "100%");
  svg.style.height = "auto";
  svg.style.display = "block";
  svg.style.maxWidth = "100%";
  svg.style.background = "transparent";

  for (const outer of svg.querySelectorAll(".typst-page-outer")) {{
    outer.remove();
  }}
  for (const inner of svg.querySelectorAll(".typst-page-inner")) {{
    inner.remove();
  }}
  for (const ind of svg.querySelectorAll(".typst-preview-canvas-page-number, .typst-preview-svg-page-number")) {{
    ind.remove();
  }}

}})();
</script>"##
    ))
}

fn extract_body(full_html: String) -> String {
    if let (Some(start), Some(end)) = (full_html.find("<body>"), full_html.rfind("</body>")) {
        full_html[start + 6..end].trim().to_string()
    } else {
        full_html
    }
}

fn lang_suffix(lang: &str) -> &str {
    match lang {
        "en" => "",
        "pt-br" => ".pt-br",
        _ => ".zh",
    }
}

fn should_skip_blog_post(stem: &str) -> bool {
    stem == "0000_template" || stem.ends_with("_template")
}

fn collect_blog_post_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_blog_post_files(&path, files);
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) == Some("typ") {
            files.push(path);
        }
    }
}

fn slug_from_path(file_path: &Path) -> Option<String> {
    let rel = file_path.strip_prefix(BLOG_POSTS_ROOT).ok()?;
    let no_ext = rel.with_extension("");
    Some(no_ext.to_string_lossy().replace('\\', "/"))
}

fn extract_parenthesized_block(input: &str, marker: &str) -> Option<String> {
    let start = input.find(marker)? + marker.len();
    let mut depth = 1usize;
    let mut in_string = false;
    let mut escaped = false;

    for (idx, ch) in input[start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(input[start..start + idx].to_string());
                }
            }
            _ => {}
        }
    }

    None
}

fn extract_named_string(args: &str, key: &str) -> Option<String> {
    let marker = format!("{key}:");
    let start = args.find(&marker)? + marker.len();
    let value = args[start..].trim_start();
    if !value.starts_with('"') {
        return None;
    }

    let mut out = String::new();
    let mut escaped = false;
    for ch in value[1..].chars() {
        if escaped {
            out.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            return Some(out);
        }
        out.push(ch);
    }

    None
}

fn extract_let_string(content: &str, key: &str) -> Option<String> {
    let marker = format!("#let {key} = \"");
    let start = content.find(&marker)? + marker.len();
    let rest = &content[start..];
    let end = rest.find('"')?;
    let value = rest[..end].trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn parse_tmil_post_title_call(value: &str, mdate: Option<(u32, u32, u32)>) -> Option<String> {
    let call = extract_parenthesized_block(value, "tmil_post_title(")?;
    let parts: Vec<&str> = call.split(',').map(|p| p.trim()).collect();
    if parts.len() < 2 {
        return None;
    }
    let year = if parts.first()? == &"mdate.year()" {
        mdate?.0
    } else {
        parts.first()?.parse::<u32>().ok()?
    };
    let month = if parts.get(1)? == &"mdate.month()" {
        mdate?.1
    } else {
        parts.get(1)?.parse::<u32>().ok()?
    };
    Some(format!("This Month in Lince | {year:04}-{month:02}"))
}

fn parse_tmil_post_date_call(value: &str, mdate: Option<(u32, u32, u32)>) -> Option<String> {
    let call = extract_parenthesized_block(value, "tmil_post_date(")?;
    let parts: Vec<&str> = call.split(',').map(|p| p.trim()).collect();
    if parts.len() < 3 {
        return None;
    }
    let year = if parts.first()? == &"mdate.year()" {
        mdate?.0
    } else {
        parts.first()?.parse::<u32>().ok()?
    };
    let month = if parts.get(1)? == &"mdate.month()" {
        mdate?.1
    } else {
        parts.get(1)?.parse::<u32>().ok()?
    };
    let day = if parts.get(2)? == &"mdate.day()" {
        mdate?.2
    } else {
        parts.get(2)?.parse::<u32>().ok()?
    };
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn parse_tmil_post_publish_date_call(
    value: &str,
    mdate: Option<(u32, u32, u32)>,
) -> Option<String> {
    let call = extract_parenthesized_block(value, "tmil_post_publish_date(")?;
    let parts: Vec<&str> = call.split(',').map(|p| p.trim()).collect();
    if parts.len() < 2 {
        return None;
    }

    let base_year = if parts.first()? == &"mdate.year()" {
        mdate?.0
    } else {
        parts.first()?.parse::<u32>().ok()?
    };
    let base_month = if parts.get(1)? == &"mdate.month()" {
        mdate?.1
    } else {
        parts.get(1)?.parse::<u32>().ok()?
    };

    let publish_year = base_year + if base_month == 12 { 1 } else { 0 };
    let publish_month = if base_month == 12 { 1 } else { base_month + 1 };
    Some(format!("{publish_year:04}-{publish_month:02}-01"))
}

fn extract_named_title(args: &str, key: &str, mdate: Option<(u32, u32, u32)>) -> Option<String> {
    extract_named_string(args, key).or_else(|| {
        let marker = format!("{key}:");
        let start = args.find(&marker)? + marker.len();
        let value = args[start..].trim_start();
        parse_tmil_post_title_call(value, mdate)
    })
}

fn extract_named_date(args: &str, key: &str, mdate: Option<(u32, u32, u32)>) -> Option<String> {
    let marker = format!("{key}:");
    let start = args.find(&marker)? + marker.len();
    let tail = args[start..].trim_start();
    if let Some(parsed) = parse_tmil_post_publish_date_call(tail, mdate) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_tmil_post_date_call(tail, mdate) {
        return Some(parsed);
    }

    let datetime_block = extract_parenthesized_block(tail, "datetime(")?;

    let mut year: Option<u32> = None;
    let mut month: Option<u32> = None;
    let mut day: Option<u32> = None;

    for part in datetime_block.split(',') {
        let trimmed = part.trim();
        if let Some(value) = trimmed.strip_prefix("year:") {
            year = value.trim().parse::<u32>().ok();
        } else if let Some(value) = trimmed.strip_prefix("month:") {
            month = value.trim().parse::<u32>().ok();
        } else if let Some(value) = trimmed.strip_prefix("day:") {
            day = value.trim().parse::<u32>().ok();
        }
    }

    match (year, month, day) {
        (Some(y), Some(m), Some(d)) => Some(format!("{y:04}-{m:02}-{d:02}")),
        _ => None,
    }
}

fn extract_mdate(content: &str) -> Option<(u32, u32, u32)> {
    let args = extract_parenthesized_block(content, "#let mdate = datetime(")?;
    let mut year: Option<u32> = None;
    let mut month: Option<u32> = None;
    let mut day: Option<u32> = None;

    for part in args.split(',') {
        let trimmed = part.trim();
        if let Some(value) = trimmed.strip_prefix("year:") {
            year = value.trim().parse::<u32>().ok();
        } else if let Some(value) = trimmed.strip_prefix("month:") {
            month = value.trim().parse::<u32>().ok();
        } else if let Some(value) = trimmed.strip_prefix("day:") {
            day = value.trim().parse::<u32>().ok();
        }
    }

    match (year, month, day) {
        (Some(y), Some(m), Some(d)) => Some((y, m, d)),
        _ => None,
    }
}

fn extract_post_metadata(file_path: &str) -> BlogMetadata {
    let Ok(content) = fs::read_to_string(file_path) else {
        return BlogMetadata::default();
    };
    let Some(post_args) = extract_parenthesized_block(&content, "#post(") else {
        return BlogMetadata::default();
    };
    let mdate = extract_mdate(&content);

    BlogMetadata {
        title: extract_named_title(&post_args, "title", mdate),
        date: extract_named_date(&post_args, "date", mdate),
        video_url: extract_let_string(&content, "video_url")
            .or_else(|| extract_named_string(&post_args, "video_url"))
            .or_else(|| extract_named_string(&post_args, "youtube_url"))
            .and_then(|v| {
                let trimmed = v.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }),
    }
}

/// Extract the title from a Typst file by looking for the first heading
fn extract_title_from_typst(file_path: &str) -> String {
    if let Ok(content) = fs::read_to_string(file_path) {
        // Look for lines starting with `=` (Typst heading syntax)
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('=') {
                // Remove leading `=` and trim whitespace
                let title = trimmed.trim_start_matches('=').trim();
                return title.to_string();
            }
        }
    }
    // Fallback: use the filename without extension
    std::path::Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string()
}

fn build_blog_neighbors(
    ordered_posts: &[(String, String, String)],
) -> HashMap<String, (Option<String>, Option<String>)> {
    let mut neighbors: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
    for (idx, (slug, _, _)) in ordered_posts.iter().enumerate() {
        let newer = if idx > 0 {
            Some(ordered_posts[idx - 1].0.clone())
        } else {
            None
        };
        let older = ordered_posts.get(idx + 1).map(|p| p.0.clone());
        neighbors.insert(slug.clone(), (older, newer));
    }
    neighbors
}

fn render_blog_nav_script(older_href: Option<&str>, newer_href: Option<&str>) -> String {
    let older = older_href
        .map(|s| format!("\"{s}\""))
        .unwrap_or_else(|| "null".to_string());
    let newer = newer_href
        .map(|s| format!("\"{s}\""))
        .unwrap_or_else(|| "null".to_string());

    format!(
        r#"(function() {{
  const olderUrl = {older};
  const newerUrl = {newer};
  const shouldIgnore = (el) => {{
    if (!el) return false;
    const tag = (el.tagName || "").toLowerCase();
    return el.isContentEditable || tag === "input" || tag === "textarea" || tag === "select";
  }};
  window.addEventListener("keydown", (ev) => {{
    if (ev.defaultPrevented || ev.altKey || ev.ctrlKey || ev.metaKey || ev.shiftKey) return;
    if (shouldIgnore(document.activeElement)) return;
    if (ev.key === "ArrowLeft" && olderUrl) {{
      window.location.href = olderUrl;
    }} else if (ev.key === "ArrowRight" && newerUrl) {{
      window.location.href = newerUrl;
    }}
  }});
}})();"#,
    )
}

/// Get all blog posts with their metadata
pub fn get_blog_posts() -> Vec<(String, String, String)> {
    let mut posts = Vec::new();
    let mut files = Vec::new();
    collect_blog_post_files(Path::new(BLOG_POSTS_ROOT), &mut files);

    for file_path in files {
        let Some(stem) = file_path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if should_skip_blog_post(stem) {
            continue;
        }
        let Some(path_str) = file_path.to_str() else {
            continue;
        };
        let meta = extract_post_metadata(path_str);
        let title = meta
            .title
            .unwrap_or_else(|| extract_title_from_typst(path_str));
        let date = meta.date.unwrap_or_default();
        let slug = slug_from_path(&file_path).unwrap_or_else(|| stem.to_string());
        posts.push((slug, title, date));
    }

    // Always sort reverse alphabetically by lowercase slug (latest first).
    posts.sort_by(|a, b| b.0.to_lowercase().cmp(&a.0.to_lowercase()));
    posts
}
pub fn generate_blog_posts(t: &Translations, suffix: &str, show_home: bool) {
    let blog_href = format!("/blog{}.html", suffix);
    let ordered_posts = get_blog_posts(); // latest first
    let neighbors = build_blog_neighbors(&ordered_posts);

    let mut files = Vec::new();
    collect_blog_post_files(Path::new(BLOG_POSTS_ROOT), &mut files);
    files.sort_by(|a, b| {
        let sa = slug_from_path(a).unwrap_or_default().to_lowercase();
        let sb = slug_from_path(b).unwrap_or_default().to_lowercase();
        sa.cmp(&sb)
    });

    for file_path in files {
        let Some(stem) = file_path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if should_skip_blog_post(stem) {
            continue;
        }
        let Some(path_str) = file_path.to_str() else {
            continue;
        };
        let meta = extract_post_metadata(path_str);
        let slug = slug_from_path(&file_path).unwrap_or_else(|| stem.to_string());
        let (older_slug, newer_slug) = neighbors.get(&slug).cloned().unwrap_or((None, None));
        let older_href = older_slug
            .as_ref()
            .map(|older| format!("/blog/{}{}.html", older, suffix));
        let newer_href = newer_slug
            .as_ref()
            .map(|newer| format!("/blog/{}{}.html", newer, suffix));
        let cache_key = slug.replace('/', "__");

        // Prefer Tinymist-rendered sidecar HTML when available.
        // Fallback to Typst CLI HTML if sidecar parsing fails.
        let sidecar_path = generate_svg_sidecar(&cache_key, &file_path);

        let body = if let Some(sidecar_path) = sidecar_path {
            tinymist_native_html(&sidecar_path, &cache_key)
                .unwrap_or_else(|| compile_blog_body(path_str))
        } else {
            compile_blog_body(path_str)
        };

        // Wrap in Maud with breadcrumbs
        let markup = html! {
            main.main-content.blog-post-content {
                nav.breadcrumbs.blog-breadcrumbs {
                    a.blog-back-link href=(blog_href.clone()) { (t.blog_back_to_posts) }
                    @if let Some(video_url) = &meta.video_url {
                        a.blog-video-link href=(video_url) target="_blank" rel="noopener noreferrer" {
                            (t.blog_watch_video)
                        }
                    }
                }
                @if older_slug.is_some() || newer_slug.is_some() {
                    nav.blog-post-pager {
                        @if let Some(ref older) = older_slug {
                            a.blog-post-nav-link.blog-post-nav-left href=(format!("/blog/{}{}.html", older, suffix)) { "← Older" }
                        } @else {
                            span.blog-post-nav-link.blog-post-nav-left.disabled { "← Older" }
                        }
                        @if let Some(ref newer) = newer_slug {
                            a.blog-post-nav-link.blog-post-nav-right href=(format!("/blog/{}{}.html", newer, suffix)) { "Newer →" }
                        } @else {
                            span.blog-post-nav-link.blog-post-nav-right.disabled { "Newer →" }
                        }
                    }
                    script {
                        (PreEscaped(render_blog_nav_script(
                            older_href.as_deref(),
                            newer_href.as_deref(),
                        )))
                    }
                }
                article.blog_post { (PreEscaped(body)) }
            }
        };
        let blog_post_page = format!("blog/{}", slug);
        let final_html = page(&markup.0, t, &blog_post_page, show_home);
        let output_path = format!("output/blog/{}{}.html", slug, suffix);
        if let Some(parent) = Path::new(&output_path).parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(output_path, final_html).unwrap();
    }
}

pub fn page_blog(t: &Translations) -> String {
    let suffix = lang_suffix(t.lang_code);
    let posts = get_blog_posts();

    html! {
        main.main-content {
            section.blog-header {
                h1.section-title { (t.blog_title) }
            }

            @if posts.is_empty() {
                section.blog-posts-container {
                    p.no-posts { "No blog posts yet." }
                }
            } @else {
                section.blog-posts-container {
                    ul.blog-posts-list {
                        @for (slug, title, date) in posts {
                            li.blog-post-item {
                                a.blog-post-link href=(format!("/blog/{}{}.html", slug, suffix)) {
                                    h3.blog-post-title { (title) }
                                    span.blog-post-dots aria-hidden="true" {}
                                    p.blog-post-date { (date) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    .0
}

#[cfg(test)]
mod tests {
    use super::{
        build_blog_neighbors, collect_blog_post_files, extract_mdate, extract_post_metadata,
        get_blog_posts, render_blog_nav_script,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn blog_post_titles_are_not_filename_fallbacks() {
        let posts = get_blog_posts();
        assert!(
            !posts.is_empty(),
            "Expected at least one blog post when validating titles"
        );

        let offenders: Vec<String> = posts
            .iter()
            .filter_map(|(slug, title, _date)| {
                let stem = Path::new(slug).file_name()?.to_str()?;
                if title.trim() == stem {
                    Some(format!("{slug} -> {title}"))
                } else {
                    None
                }
            })
            .collect();

        assert!(
            offenders.is_empty(),
            "Blog listing is using filename fallback titles for: {}",
            offenders.join(", ")
        );
    }

    #[test]
    fn blog_navigation_neighbors_match_post_order() {
        let posts = get_blog_posts(); // latest first
        assert!(
            !posts.is_empty(),
            "Expected at least one blog post when validating neighbors"
        );

        let neighbors = build_blog_neighbors(&posts);
        for (idx, (slug, _, _)) in posts.iter().enumerate() {
            let (older, newer) = neighbors
                .get(slug)
                .unwrap_or_else(|| panic!("Missing neighbor entry for slug: {slug}"));

            let expected_older = posts.get(idx + 1).map(|p| p.0.clone());
            let expected_newer = if idx > 0 {
                Some(posts[idx - 1].0.clone())
            } else {
                None
            };

            assert_eq!(
                older, &expected_older,
                "Older neighbor mismatch for slug {slug}"
            );
            assert_eq!(
                newer, &expected_newer,
                "Newer neighbor mismatch for slug {slug}"
            );

            if let Some(older_slug) = older {
                let (_olders_older, olders_newer) =
                    neighbors.get(older_slug).unwrap_or_else(|| {
                        panic!("Missing reverse neighbor entry for older slug: {older_slug}")
                    });
                assert_eq!(
                    olders_newer,
                    &Some(slug.clone()),
                    "Reciprocal navigation mismatch: older({slug}) should point newer back"
                );
            }
            if let Some(newer_slug) = newer {
                let (newers_older, _newers_newer) =
                    neighbors.get(newer_slug).unwrap_or_else(|| {
                        panic!("Missing reverse neighbor entry for newer slug: {newer_slug}")
                    });
                assert_eq!(
                    newers_older,
                    &Some(slug.clone()),
                    "Reciprocal navigation mismatch: newer({slug}) should point older back"
                );
            }
        }
    }

    #[test]
    fn blog_posts_have_non_empty_title_and_parseable_date() {
        let posts = get_blog_posts();
        assert!(
            !posts.is_empty(),
            "Expected at least one blog post when validating metadata"
        );

        for (slug, title, date) in posts {
            assert!(
                !title.trim().is_empty(),
                "Post title is empty for slug: {slug}"
            );
            assert!(
                is_valid_iso_date(&date),
                "Post date is not valid YYYY-MM-DD for slug {slug}: {date}"
            );
        }
    }

    #[test]
    fn tmil_posts_publish_one_month_after_mdate() {
        let mut files = Vec::new();
        collect_blog_post_files(Path::new("content/blog/posts"), &mut files);
        let tmil_files: Vec<PathBuf> = files
            .into_iter()
            .filter(|p| {
                p.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.ends_with("_tmil"))
                    .unwrap_or(false)
            })
            .collect();
        assert!(
            !tmil_files.is_empty(),
            "Expected TMIL files for date-rule check"
        );

        for file_path in tmil_files {
            let Some(path_str) = file_path.to_str() else {
                continue;
            };
            let content = std::fs::read_to_string(path_str)
                .unwrap_or_else(|e| panic!("Failed to read {path_str}: {e}"));
            let mdate = extract_mdate(&content).unwrap_or_else(|| {
                panic!("Missing/invalid #let mdate datetime(...) in {path_str}")
            });
            let meta = extract_post_metadata(path_str);
            let actual_date = meta
                .date
                .unwrap_or_else(|| panic!("Missing parsed post date in {path_str}"));
            let expected = expected_publish_date(mdate.0, mdate.1);
            assert_eq!(
                actual_date, expected,
                "TMIL publish date must be next-month day 1 in {path_str}"
            );
        }
    }

    #[test]
    fn blog_posts_are_sorted_latest_first() {
        let posts = get_blog_posts();
        assert!(!posts.is_empty(), "Expected posts for ordering check");

        for pair in posts.windows(2) {
            let a = pair[0].0.to_lowercase();
            let b = pair[1].0.to_lowercase();
            assert!(
                a >= b,
                "Posts are not sorted latest-first by slug: {} then {}",
                pair[0].0,
                pair[1].0
            );
        }
    }

    #[test]
    fn tmil_template_has_required_structure() {
        let template_path = "content/blog/YYYY_MM_DD_tmil.typ";
        let content = std::fs::read_to_string(template_path)
            .unwrap_or_else(|e| panic!("Failed to read template {template_path}: {e}"));

        for needle in [
            "#let mdate = datetime(",
            "#let author_name = ",
            "#let author_email = ",
            "#let video_url = ",
            "#let roadmap_items = (",
            "date: tmil_post_publish_date(",
            "\"Roteiro | 路线图 | Roadmap\"",
        ] {
            assert!(
                content.contains(needle),
                "Template missing required pattern `{needle}`"
            );
        }
    }

    #[test]
    fn generated_blog_links_point_to_existing_output_html() {
        let posts = get_blog_posts();
        assert!(!posts.is_empty(), "Expected posts for output-link check");
        for (slug, _title, _date) in posts {
            let output = format!("output/blog/{slug}.html");
            assert!(
                Path::new(&output).exists(),
                "Missing generated blog output file for slug {slug}: {output}"
            );
        }
    }

    #[test]
    fn generated_blog_outputs_are_unique_per_slug() {
        let posts = get_blog_posts();
        assert!(!posts.is_empty(), "Expected posts for uniqueness check");

        let mut rendered = Vec::new();
        for (slug, _title, _date) in posts {
            let output = format!("output/blog/{slug}.html");
            let body = std::fs::read_to_string(&output)
                .unwrap_or_else(|e| panic!("Failed to read generated output {output}: {e}"));
            rendered.push((slug, body));
        }

        for i in 0..rendered.len() {
            for j in (i + 1)..rendered.len() {
                assert_ne!(
                    rendered[i].1, rendered[j].1,
                    "Generated blog outputs are identical for {} and {}",
                    rendered[i].0, rendered[j].0
                );
            }
        }
    }

    #[test]
    fn blog_nav_script_contains_arrow_key_navigation() {
        let script = render_blog_nav_script(Some("/blog/older.html"), Some("/blog/newer.html"));
        assert!(script.contains("ArrowLeft"), "Missing ArrowLeft handler");
        assert!(script.contains("ArrowRight"), "Missing ArrowRight handler");
        assert!(
            script.contains("\"/blog/older.html\""),
            "Missing older URL in nav script"
        );
        assert!(
            script.contains("\"/blog/newer.html\""),
            "Missing newer URL in nav script"
        );
    }

    #[test]
    fn tmil_referenced_media_assets_exist() {
        let mut targets = vec![PathBuf::from("content/blog/YYYY_MM_DD_tmil.typ")];
        let mut files = Vec::new();
        collect_blog_post_files(Path::new("content/blog/posts"), &mut files);
        targets.extend(files.into_iter().filter(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.ends_with("_tmil"))
                .unwrap_or(false)
        }));

        for file_path in targets {
            let Some(path_str) = file_path.to_str() else {
                continue;
            };
            let content = std::fs::read_to_string(path_str)
                .unwrap_or_else(|e| panic!("Failed to read {path_str}: {e}"));
            for asset in extract_media_references(&content) {
                let full = Path::new("content/blog").join(&asset);
                assert!(
                    full.exists(),
                    "Missing media asset `{}` referenced in {}",
                    asset,
                    path_str
                );
            }
        }
    }

    fn expected_publish_date(year: u32, month: u32) -> String {
        let publish_year = year + if month == 12 { 1 } else { 0 };
        let publish_month = if month == 12 { 1 } else { month + 1 };
        format!("{publish_year:04}-{publish_month:02}-01")
    }

    fn is_valid_iso_date(value: &str) -> bool {
        if value.len() != 10 {
            return false;
        }
        let bytes = value.as_bytes();
        bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes
                .iter()
                .enumerate()
                .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    }

    fn extract_media_references(content: &str) -> Vec<String> {
        let mut refs = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("photo: \"") {
                if let Some(end) = rest.find('"') {
                    let candidate = &rest[..end];
                    if candidate.starts_with("media/") {
                        refs.push(candidate.to_string());
                    }
                }
            }
        }

        let mut offset = 0usize;
        while let Some(pos) = content[offset..].find("#image(\"") {
            let start = offset + pos + "#image(\"".len();
            let Some(end_rel) = content[start..].find('"') else {
                break;
            };
            let candidate = &content[start..start + end_rel];
            if candidate.starts_with("media/") {
                refs.push(candidate.to_string());
            }
            offset = start + end_rel + 1;
        }

        refs
    }
}
