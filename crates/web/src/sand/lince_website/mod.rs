use {
    crate::domain::lince_package::{LincePackage, PackageManifest},
    std::collections::BTreeMap,
};

#[path = "site/html.rs"]
mod html;
#[allow(dead_code)]
#[path = "site/i18n.rs"]
mod i18n;
#[path = "site/index.rs"]
mod index;
#[path = "site/visual_identity.rs"]
mod visual_identity;

pub(crate) const FEATURE_FLAG: &str = "sand.lince_website";

const STYLE_CSS: &[u8] = include_bytes!("style.css");
const CREDITS: &[u8] = include_bytes!("CREDITS.txt");
const LINCE_LICENSE: &[u8] = include_bytes!("../../../../../LICENSE");
const WHITE_SVG: &[u8] = include_bytes!("../../../../../assets/logo/white.svg");
const WHITE_PNG: &[u8] = include_bytes!("../../../../../assets/logo/branco.png");
const WHITE_IN_BLACK_SVG: &[u8] = include_bytes!("../../../../../assets/logo/white_in_black.svg");
const WHITE_IN_BLACK_PNG: &[u8] = include_bytes!("../../../../../assets/logo/branco_no_preto.png");
const BLACK_SVG: &[u8] = include_bytes!("../../../../../assets/logo/black.svg");
const BLACK_PNG: &[u8] = include_bytes!("../../../../../assets/logo/preto.png");
const BLACK_IN_WHITE_SVG: &[u8] = include_bytes!("../../../../../assets/logo/black_in_white.svg");
const BLACK_IN_WHITE_PNG: &[u8] = include_bytes!("../../../../../assets/logo/preto_no_branco.png");
const BLACK_IN_WHITE_ICO: &[u8] = include_bytes!("../../../../../assets/logo/black_in_white.ico");
const BLACK_IN_WHITE_ICNS: &[u8] = include_bytes!("../../../../../assets/logo/black_in_white.icns");
const EXCALIDRAW_SOURCE: &[u8] = include_bytes!("../../../../../assets/lince.excalidraw");
const LATO_REGULAR: &[u8] = include_bytes!("../../../../../assets/fonts/Lato/Lato-Regular.ttf");
const LATO_BOLD: &[u8] = include_bytes!("../../../../../assets/fonts/Lato/Lato-Bold.ttf");
const LATO_LICENSE: &[u8] = include_bytes!("../../../../../assets/fonts/Lato/OFL.txt");
const ALEO_VARIABLE: &[u8] =
    include_bytes!("../../../../../assets/fonts/Aleo/Aleo-VariableFont_wght.ttf");
const ALEO_LICENSE: &[u8] = include_bytes!("../../../../../assets/fonts/Aleo/OFL.txt");

fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "◉".into(),
        title: "Lince Website".into(),
        author: "Lince Institute".into(),
        version: "1.0.0".into(),
        description: "The Lince website and downloadable visual-identity kit.".into(),
        details: "A self-contained, multilingual Maud site with bundled styles, logos, editable artwork, fonts, and license notices. External resources open in a new tab.".into(),
        initial_width: 8,
        initial_height: 7,
        requires_server: false,
        permissions: vec![],
    }
}

fn render_page(language: &i18n::Translations, name: &str) -> String {
    let body = match name {
        "index" => index::page_index(language),
        "visual-identity" => visual_identity::page_visual_identity(),
        _ => unreachable!("the website package only renders known pages"),
    };
    html::page(&body, language, name, true, false)
}

pub(crate) fn package() -> LincePackage {
    let translations = i18n::get_translations();
    let english = translations
        .get("en")
        .expect("the website should have English translations");
    let portuguese = translations
        .get("pt-br")
        .expect("the website should have Portuguese translations");
    let chinese = translations
        .get("zh")
        .expect("the website should have Chinese translations");

    let index_html = render_page(english, "index");
    let assets = BTreeMap::from([
        (
            "index.pt-br.html".into(),
            render_page(portuguese, "index").into_bytes(),
        ),
        (
            "index.zh.html".into(),
            render_page(chinese, "index").into_bytes(),
        ),
        (
            "visual-identity.html".into(),
            render_page(english, "visual-identity").into_bytes(),
        ),
        (
            "visual-identity.pt-br.html".into(),
            render_page(portuguese, "visual-identity").into_bytes(),
        ),
        (
            "visual-identity.zh.html".into(),
            render_page(chinese, "visual-identity").into_bytes(),
        ),
        ("assets/style.css".into(), STYLE_CSS.to_vec()),
        ("assets/CREDITS.txt".into(), CREDITS.to_vec()),
        ("assets/LICENSE.txt".into(), LINCE_LICENSE.to_vec()),
        ("assets/logo/white.svg".into(), WHITE_SVG.to_vec()),
        ("assets/logo/white.png".into(), WHITE_PNG.to_vec()),
        (
            "assets/logo/white_in_black.svg".into(),
            WHITE_IN_BLACK_SVG.to_vec(),
        ),
        (
            "assets/logo/white_in_black.png".into(),
            WHITE_IN_BLACK_PNG.to_vec(),
        ),
        ("assets/logo/black.svg".into(), BLACK_SVG.to_vec()),
        ("assets/logo/black.png".into(), BLACK_PNG.to_vec()),
        (
            "assets/logo/black_in_white.svg".into(),
            BLACK_IN_WHITE_SVG.to_vec(),
        ),
        (
            "assets/logo/black_in_white.png".into(),
            BLACK_IN_WHITE_PNG.to_vec(),
        ),
        (
            "assets/black_in_white.ico".into(),
            BLACK_IN_WHITE_ICO.to_vec(),
        ),
        (
            "assets/black_in_white.icns".into(),
            BLACK_IN_WHITE_ICNS.to_vec(),
        ),
        ("assets/lince.excalidraw".into(), EXCALIDRAW_SOURCE.to_vec()),
        (
            "assets/fonts/Lato-Regular.ttf".into(),
            LATO_REGULAR.to_vec(),
        ),
        ("assets/fonts/Lato-Bold.ttf".into(), LATO_BOLD.to_vec()),
        ("assets/fonts/Lato-OFL.txt".into(), LATO_LICENSE.to_vec()),
        (
            "assets/fonts/Aleo-VariableFont_wght.ttf".into(),
            ALEO_VARIABLE.to_vec(),
        ),
        ("assets/fonts/Aleo-OFL.txt".into(), ALEO_LICENSE.to_vec()),
    ]);

    LincePackage::new_archive(
        Some("lince-website.lince".into()),
        manifest(),
        index_html,
        "index.html",
        assets,
    )
    .expect("the Lince website should render as a valid archive package")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn website_is_self_contained_and_navigates_between_maud_pages() {
        let package = package();
        let home = package.html_document();
        let identity = std::str::from_utf8(
            package
                .asset_bytes("visual-identity.html")
                .expect("visual identity page should be bundled"),
        )
        .expect("visual identity page should be UTF-8");

        assert!(home.contains("href=\"visual-identity.html\""));
        assert!(!home.contains("href=\"/visual-identity.html\""));
        let portuguese_home = std::str::from_utf8(
            package
                .asset_bytes("index.pt-br.html")
                .expect("Portuguese home page should be bundled"),
        )
        .expect("Portuguese home page should be UTF-8");
        assert!(portuguese_home.contains("href=\"visual-identity.pt-br.html\""));
        assert!(identity.contains(" download"));
        assert!(identity.contains("href=\"assets/logo/white.svg\""));
        assert!(identity.contains("target=\"_blank\""));
        assert!(home.contains("/board/frame.js"));

        for path in [
            "index.pt-br.html",
            "index.zh.html",
            "visual-identity.html",
            "visual-identity.pt-br.html",
            "visual-identity.zh.html",
            "assets/style.css",
            "assets/CREDITS.txt",
            "assets/LICENSE.txt",
            "assets/logo/white.svg",
            "assets/logo/white.png",
            "assets/logo/black.svg",
            "assets/logo/black.png",
            "assets/logo/black_in_white.svg",
            "assets/logo/black_in_white.png",
            "assets/black_in_white.ico",
            "assets/black_in_white.icns",
            "assets/lince.excalidraw",
            "assets/fonts/Lato-OFL.txt",
            "assets/fonts/Aleo-OFL.txt",
        ] {
            assert!(package.asset_bytes(path).is_some(), "missing {path}");
        }
    }
}
