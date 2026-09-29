pub const CHRONO_LICENSE: &str = include_str!("../licenses/chrono.txt");
pub const BEVY_LICENSE: &str = include_str!("../licenses/bevy-MIT.txt");
pub const SYMBOLS_LICENSE: &str =
    include_str!("../../../institute/assets/fonts/NotoSansSymbols2/OFL.txt");
pub const LATO_LICENSE: &str = include_str!("../../../institute/assets/fonts/Lato/OFL.txt");

pub const CJK: Attribution = Attribution {
    name: "Noto Sans Mono CJK JP Regular",
    author: "Adobe; Ryoko Nishizuka, Paul D. Hunt, Sandoll Communications, Soo-young Jang, and Joo-yeon Kang",
    license: concat!(
        include_str!("../../../institute/assets/fonts/NotoSansMonoCJK/CREDITS.txt"),
        "\n",
        include_str!("../../../institute/assets/fonts/NotoSansMonoCJK/OFL.txt"),
    ),
};

pub struct Attribution {
    pub name: &'static str,
    pub author: &'static str,
    pub license: &'static str,
}

pub const SYMBOLS: Attribution = Attribution {
    name: "Noto Sans Symbols 2",
    author: "The Noto Project Authors",
    license: SYMBOLS_LICENSE,
};

pub const DEJAVU: Attribution = Attribution {
    name: "DejaVu Sans",
    author: "DejaVu contributors, Bitstream, and Tavmjong Bah",
    license: include_str!("../../../institute/assets/fonts/DejaVuSans/LICENSE"),
};

pub const FONTIQUE: Attribution = Attribution {
    name: "Fontique",
    author: "The Parley Authors",
    license: include_str!("../licenses/fontique-MIT.txt"),
};

pub const ATTRIBUTIONS: &[Attribution] = &[
    Attribution {
        name: "image",
        author: "The image-rs Developers",
        license: include_str!("../licenses/image-MIT.txt"),
    },
    Attribution {
        name: "reqwest",
        author: "Sean McArthur and contributors",
        license: include_str!("../licenses/reqwest-MIT.txt"),
    },
    Attribution {
        name: "pulldown-cmark",
        author: "Raph Levien and pulldown-cmark contributors",
        license: include_str!("../licenses/pulldown-cmark-MIT.txt"),
    },
    Attribution {
        name: "qrcode-rust",
        author: "Kenneth Yip and contributors",
        license: include_str!("../licenses/qr/qrcode-MIT.txt"),
    },
    Attribution {
        name: "rqrr",
        author: "Wanja B. (WanzenBug), Daniel Beer, and contributors",
        license: include_str!("../licenses/qr/rqrr-LICENSES.txt"),
    },
    Attribution {
        name: "Chrono",
        author: "Kang Seonghoon and Chrono contributors",
        license: CHRONO_LICENSE,
    },
    SYMBOLS,
    DEJAVU,
    CJK,
    FONTIQUE,
    Attribution {
        name: "Bevy",
        author: "Bevy contributors",
        license: BEVY_LICENSE,
    },
    Attribution {
        name: "Lato",
        author: "Łukasz Dziedzic",
        license: LATO_LICENSE,
    },
];
