use crate::native::credits::Attribution;

pub(crate) const CREDITS: &[Attribution] = &[
    Attribution {
        name: "Tokio",
        author: "Tokio contributors",
        license: include_str!("../../../licenses/native/editor/tokio-MIT.txt"),
    },
    Attribution {
        name: "Serde JSON",
        author: "David Tolnay and contributors",
        license: include_str!("../../../licenses/native/editor/serde-json-MIT.txt"),
    },
    Attribution {
        name: "URL",
        author: "The rust-url developers",
        license: include_str!("../../../licenses/native/editor/url-MIT.txt"),
    },
    Attribution {
        name: "ICU4X word segmentation",
        author: "Unicode, Inc. and contributors",
        license: include_str!("../../../licenses/native/editor/icu4x-Unicode-3.0.txt"),
    },
    Attribution {
        name: "Regex",
        author: "The Rust Project Developers and contributors",
        license: include_str!("../../../licenses/native/editor/regex-MIT.txt"),
    },
    Attribution {
        name: "Rustix",
        author: "The Rustix Project Developers",
        license: include_str!("../../../licenses/native/editor/rustix-MIT.txt"),
    },
    Attribution {
        name: "DejaVu Sans Mono",
        author: crate::native::credits::DEJAVU.author,
        license: crate::native::credits::DEJAVU.license,
    },
    Attribution {
        name: "Libc",
        author: "The Rust Project Developers",
        license: include_str!("../../../licenses/native/editor/libc-MIT.txt"),
    },
    Attribution {
        name: "Loro",
        author: "Loro contributors",
        license: crate::native::credits::LORO_LICENSE,
    },
    Attribution {
        name: "Ropey",
        author: "Nathan Vegdahl and contributors",
        license: include_str!("../../../licenses/native/editor/ropey-MIT.txt"),
    },
    Attribution {
        name: "Similar",
        author: "Armin Ronacher and contributors",
        license: include_str!("../../../licenses/native/editor/similar-Apache.txt"),
    },
    Attribution {
        name: "Notify",
        author: "Notify contributors",
        license: include_str!("../../../licenses/native/editor/notify-CC0.txt"),
    },
    Attribution {
        name: "cap-std",
        author: "The Bytecode Alliance and contributors",
        license: include_str!("../../../licenses/native/editor/cap-std-MIT.txt"),
    },
    Attribution {
        name: "Ignore",
        author: "Andrew Gallant and contributors",
        license: include_str!("../../../licenses/native/editor/ignore-MIT.txt"),
    },
    Attribution {
        name: "SHA-2",
        author: "RustCrypto contributors",
        license: include_str!("../../../licenses/native/editor/sha2-MIT.txt"),
    },
    Attribution {
        name: "Rust File Dialog",
        author: "Poly Meilex and contributors",
        license: include_str!("../../../licenses/native/document/rfd-MIT.txt"),
    },
    Attribution {
        name: "Bevy",
        author: "Bevy contributors",
        license: crate::native::credits::BEVY_LICENSE,
    },
    Attribution {
        name: "Lato",
        author: "Łukasz Dziedzic",
        license: crate::native::credits::LATO_LICENSE,
    },
    crate::native::credits::DEJAVU,
    crate::native::credits::CJK,
];
