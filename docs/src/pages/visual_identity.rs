use maud::html;

struct AssetLink {
    label: &'static str,
    href: &'static str,
}

struct IdentitySection {
    title: &'static str,
    description: Option<&'static str>,
    preview_src: &'static str,
    preview_alt: &'static str,
    links: Vec<AssetLink>,
}

fn sections() -> Vec<IdentitySection> {
    vec![
        IdentitySection {
            title: "White Logo",
            description: Some("Use on dark backgrounds."),
            preview_src: "/assets/logo/white.svg",
            preview_alt: "Lince white logo",
            links: vec![
                AssetLink {
                    label: "SVG",
                    href: "/assets/logo/white.svg",
                },
                AssetLink {
                    label: "PNG",
                    href: "/assets/logo/white.png",
                },
                AssetLink {
                    label: "JPG",
                    href: "/assets/logo/white.jpg",
                },
            ],
        },
        IdentitySection {
            title: "White on Black",
            description: Some("Logo with fixed dark background."),
            preview_src: "/assets/logo/white_in_black.svg",
            preview_alt: "Lince white logo on black background",
            links: vec![
                AssetLink {
                    label: "SVG",
                    href: "/assets/logo/white_in_black.svg",
                },
                AssetLink {
                    label: "PNG",
                    href: "/assets/logo/white_in_black.png",
                },
                AssetLink {
                    label: "JPG",
                    href: "/assets/logo/white_in_black.jpg",
                },
            ],
        },
        IdentitySection {
            title: "Black Logo",
            description: Some("Use on light backgrounds."),
            preview_src: "/assets/logo/black.svg",
            preview_alt: "Lince black logo",
            links: vec![
                AssetLink {
                    label: "SVG",
                    href: "/assets/logo/black.svg",
                },
                AssetLink {
                    label: "PNG",
                    href: "/assets/logo/black.png",
                },
                AssetLink {
                    label: "JPG",
                    href: "/assets/logo/black.jpg",
                },
            ],
        },
        IdentitySection {
            title: "Black on White",
            description: None,
            preview_src: "/assets/logo/black_in_white.svg",
            preview_alt: "Lince black logo on white background",
            links: vec![
                AssetLink {
                    label: "SVG",
                    href: "/assets/logo/black_in_white.svg",
                },
                AssetLink {
                    label: "PNG",
                    href: "/assets/logo/black_in_white.png",
                },
                AssetLink {
                    label: "JPG",
                    href: "/assets/logo/black_in_white.jpg",
                },
                AssetLink {
                    label: "ICO",
                    href: "/assets/black_in_white.ico",
                },
            ],
        },
    ]
}

pub fn page_visual_identity() -> String {
    let sections = sections();

    html! {
        main.main-content {
            section.content-block {
                h1.content-block__title { "Visual Identity" }
                p.content-block__body {
                    "Official logo files for download in SVG, PNG, JPG, and ICO formats."
                }
            }

            @for section_data in sections {
                section.content-block.content-block--with-image {
                    .content-block__text {
                        h2.content-block__title { (section_data.title) }
                        @if let Some(desc) = section_data.description {
                            p.content-block__body { (desc) }
                        }
                        p.content-block__body {
                            @for (i, link) in section_data.links.iter().enumerate() {
                                @if i > 0 { " • " }
                                a href=(link.href) { (link.label) }
                            }
                        }
                    }
                    .content-block__image {
                        img src=(section_data.preview_src) alt=(section_data.preview_alt);
                    }
                }
            }
        }
    }
    .0
}
