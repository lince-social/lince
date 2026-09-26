use crate::credits::Attribution;

pub(super) const CREDITS: &[Attribution] = &[
    Attribution {
        name: "ZIP",
        author: "ZIP Rust contributors",
        license: include_str!("../../licenses/document/zip-MIT.txt"),
    },
    Attribution {
        name: "Quick XML",
        author: "Quick XML contributors",
        license: include_str!("../../licenses/document/quick-xml-MIT.txt"),
    },
    Attribution {
        name: "Image",
        author: "Image Rust contributors",
        license: include_str!("../../licenses/document/image-MIT.txt"),
    },
    Attribution {
        name: "Base64",
        author: "Base64 contributors",
        license: include_str!("../../licenses/document/base64-MIT.txt"),
    },
    Attribution {
        name: "Hayro",
        author: "Laurenz V and contributors · https://github.com/LaurenzV/hayro",
        license: include_str!("../../licenses/document/hayro-MIT.txt"),
    },
    Attribution {
        name: "Hayro standard PDF fonts",
        author: "Foxit Software",
        license: include_str!("../../licenses/document/Foxit.txt"),
    },
    Attribution {
        name: "PDF CMaps",
        author: "Adobe",
        license: include_str!("../../licenses/document/Adobe-CMaps.txt"),
    },
    Attribution {
        name: "CGATS color profile",
        author: "NPES and ICC",
        license: include_str!("../../licenses/document/CGATS.txt"),
    },
    Attribution {
        name: "Rbook",
        author: "Devin Sterling · https://github.com/DevinSterling/rbook",
        license: include_str!("../../licenses/document/rbook-Apache.txt"),
    },
    Attribution {
        name: "Blitz",
        author: "Dioxus Labs and contributors · https://github.com/DioxusLabs/blitz",
        license: include_str!("../../licenses/document/blitz-MIT.txt"),
    },
    Attribution {
        name: "Stylo 0.20.0",
        author: "Mozilla and Servo contributors · source: https://crates.io/crates/stylo/0.20.0",
        license: include_str!("../../licenses/document/stylo-MPL.txt"),
    },
    Attribution {
        name: "AnyRender",
        author: "Dioxus Labs and contributors",
        license: include_str!("../../licenses/document/anyrender-MIT.txt"),
    },
    Attribution {
        name: "Vello CPU",
        author: "Linebender contributors",
        license: include_str!("../../licenses/document/vello-MIT.txt"),
    },
    Attribution {
        name: "Parley",
        author: "Linebender contributors",
        license: include_str!("../../licenses/document/parley-MIT.txt"),
    },
    Attribution {
        name: "Rust File Dialog",
        author: "Poly Meilex and contributors",
        license: include_str!("../../licenses/document/rfd-MIT.txt"),
    },
    Attribution {
        name: "Lato",
        author: "Łukasz Dziedzic",
        license: crate::credits::LATO_LICENSE,
    },
    crate::credits::DEJAVU,
    Attribution {
        name: "Bevy",
        author: "Bevy contributors",
        license: crate::credits::BEVY_LICENSE,
    },
];
