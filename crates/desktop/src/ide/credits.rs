use crate::credits::Attribution;

pub(crate) const CREDITS: &[Attribution] = &[
    Attribution {
        name: "Regex",
        author: "The Rust Project Developers and contributors",
        license: include_str!("../../licenses/editor/regex-MIT.txt"),
    },
    Attribution {
        name: "Rustix",
        author: "The Rustix Project Developers",
        license: include_str!("../../licenses/editor/rustix-MIT.txt"),
    },
    Attribution {
        name: "DejaVu Sans Mono",
        author: crate::credits::DEJAVU.author,
        license: crate::credits::DEJAVU.license,
    },
    Attribution {
        name: "Libc",
        author: "The Rust Project Developers",
        license: include_str!("../../licenses/editor/libc-MIT.txt"),
    },
    Attribution {
        name: "Loro",
        author: "Loro contributors",
        license: crate::credits::LORO_LICENSE,
    },
    Attribution {
        name: "Ropey",
        author: "Nathan Vegdahl and contributors",
        license: include_str!("../../licenses/editor/ropey-MIT.txt"),
    },
    Attribution {
        name: "Similar",
        author: "Armin Ronacher and contributors",
        license: include_str!("../../licenses/editor/similar-Apache.txt"),
    },
    Attribution {
        name: "Notify",
        author: "Notify contributors",
        license: include_str!("../../licenses/editor/notify-CC0.txt"),
    },
    Attribution {
        name: "cap-std",
        author: "The Bytecode Alliance and contributors",
        license: include_str!("../../licenses/editor/cap-std-MIT.txt"),
    },
    Attribution {
        name: "Ignore",
        author: "Andrew Gallant and contributors",
        license: include_str!("../../licenses/editor/ignore-MIT.txt"),
    },
    Attribution {
        name: "SHA-2",
        author: "RustCrypto contributors",
        license: include_str!("../../licenses/editor/sha2-MIT.txt"),
    },
    Attribution {
        name: "Rust File Dialog",
        author: "Poly Meilex and contributors",
        license: include_str!("../../licenses/document/rfd-MIT.txt"),
    },
    Attribution {
        name: "Bevy",
        author: "Bevy contributors",
        license: crate::credits::BEVY_LICENSE,
    },
    Attribution {
        name: "Lato",
        author: "Łukasz Dziedzic",
        license: crate::credits::LATO_LICENSE,
    },
    crate::credits::DEJAVU,
    crate::credits::CJK,
];
