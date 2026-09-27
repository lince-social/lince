pub const CHRONO_LICENSE: &str = include_str!("../licenses/chrono.txt");
pub const BEVY_LICENSE: &str = include_str!("../licenses/bevy-MIT.txt");
pub const SYMBOLS_LICENSE: &str =
    include_str!("../../../institute/assets/fonts/NotoSansSymbols2/OFL.txt");
pub const LATO_LICENSE: &str = include_str!("../../../institute/assets/fonts/Lato/OFL.txt");

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
        name: "Chrono",
        author: "Kang Seonghoon and Chrono contributors",
        license: CHRONO_LICENSE,
    },
    SYMBOLS,
    DEJAVU,
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
