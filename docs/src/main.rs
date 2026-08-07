use crate::config::INCLUDE_BLOG;
use crate::{
    html::page,
    i18n::get_translations,
    pages::{
        blog::{generate_blog_posts, page_blog},
        index::page_index,
        visual_identity::page_visual_identity,
    },
};
use std::fs;

mod config;
mod html;
mod i18n;
mod macros;
mod pages;

fn main() {
    let translations = get_translations();
    if INCLUDE_BLOG {
        fs::create_dir_all("output/blog").expect("Failed to create output directory");
    }
    fs::create_dir_all("output").expect("Failed to create output directory");
    fs::write("output/install.sh", include_str!("../content/install.sh"))
        .expect("Failed to write install.sh");

    for (lang_code, t) in &translations {
        let suffix = if lang_code == &"en" {
            "".to_string()
        } else {
            format!(".{}", lang_code)
        };

        let mut pages: Vec<(&str, String)> = Vec::new();
        pages.push(("index", page_index(t)));
        pages.push(("visual-identity", page_visual_identity()));
        if INCLUDE_BLOG {
            pages.push(("blog", page_blog(t)));
        }

        let show_home = pages.len() > 1;

        if INCLUDE_BLOG {
            generate_blog_posts(t, &suffix, show_home);
        }

        for (name, content) in pages {
            let html_out = page(&content, t, name, show_home);
            fs::write(format!("output/{}{}.html", name, suffix), html_out).unwrap();
        }
    }
}
