use crate::native::credits::Attribution;

pub(crate) const CREDITS: &[Attribution] = &[
    Attribution {
        name: "FFmpeg",
        author: "The FFmpeg developers · https://ffmpeg.org · external media decoder",
        license: include_str!("../../../licenses/native/ffmpeg-GPLv3.txt"),
    },
    Attribution {
        name: "PDFium Render",
        author: "Alastair Carey · https://github.com/ajrcarey/pdfium-render",
        license: include_str!("../../../licenses/native/document/pdfium-render-Apache.txt"),
    },
    Attribution {
        name: "PDFium binaries and bundled dependencies",
        author: "PDFium authors, Benoit Blanchon and contributors · https://github.com/bblanchon/pdfium-binaries",
        license: concat!(
            include_str!("../../../licenses/native/document/pdfium-binaries-MIT.txt"),
            "\n",
            include_str!("../../../licenses/native/document/pdfium-third-party.txt")
        ),
    },
    Attribution {
        name: "BLAKE3",
        author: "Jack O'Connor, Samuel Neves and contributors",
        license: include_str!("../../../licenses/native/document/blake3-CC0.txt"),
    },
    Attribution {
        name: "Reqwest",
        author: "Sean McArthur and contributors",
        license: include_str!("../../../licenses/native/reqwest-MIT.txt"),
    },
    Attribution {
        name: "ZIP",
        author: "ZIP Rust contributors",
        license: include_str!("../../../licenses/native/document/zip-MIT.txt"),
    },
    Attribution {
        name: "Chrono",
        author: "Kang Seonghoon and contributors",
        license: include_str!("../../../licenses/chrono.txt"),
    },
    Attribution {
        name: "URL",
        author: "The rust-url developers",
        license: include_str!("../../../licenses/native/url-MIT.txt"),
    },
    Attribution {
        name: "Tempfile",
        author: "Steven Allen and contributors",
        license: include_str!("../../../licenses/native/document/tempfile-MIT.txt"),
    },
    Attribution {
        name: "Quick XML",
        author: "Quick XML contributors",
        license: include_str!("../../../licenses/native/document/quick-xml-MIT.txt"),
    },
    Attribution {
        name: "Image",
        author: "Image Rust contributors",
        license: include_str!("../../../licenses/native/document/image-MIT.txt"),
    },
    Attribution {
        name: "Base64",
        author: "Base64 contributors",
        license: include_str!("../../../licenses/native/document/base64-MIT.txt"),
    },
    Attribution {
        name: "Hayro",
        author: "Laurenz V and contributors · https://github.com/LaurenzV/hayro",
        license: include_str!("../../../licenses/native/document/hayro-MIT.txt"),
    },
    Attribution {
        name: "Hayro standard PDF fonts",
        author: "Foxit Software",
        license: include_str!("../../../licenses/native/document/Foxit.txt"),
    },
    Attribution {
        name: "PDF CMaps",
        author: "Adobe",
        license: include_str!("../../../licenses/native/document/Adobe-CMaps.txt"),
    },
    Attribution {
        name: "CGATS color profile",
        author: "NPES and ICC",
        license: include_str!("../../../licenses/native/document/CGATS.txt"),
    },
    Attribution {
        name: "Rbook",
        author: "Devin Sterling · https://github.com/DevinSterling/rbook",
        license: include_str!("../../../licenses/native/document/rbook-Apache.txt"),
    },
    Attribution {
        name: "Blitz",
        author: "Dioxus Labs and contributors · https://github.com/DioxusLabs/blitz",
        license: include_str!("../../../licenses/native/document/blitz-MIT.txt"),
    },
    Attribution {
        name: "Stylo 0.20.0",
        author: "Mozilla and Servo contributors · source: https://crates.io/crates/stylo/0.20.0",
        license: include_str!("../../../licenses/native/document/stylo-MPL.txt"),
    },
    Attribution {
        name: "AnyRender",
        author: "Dioxus Labs and contributors",
        license: include_str!("../../../licenses/native/document/anyrender-MIT.txt"),
    },
    Attribution {
        name: "Vello CPU",
        author: "Linebender contributors",
        license: include_str!("../../../licenses/native/document/vello-MIT.txt"),
    },
    Attribution {
        name: "Parley",
        author: "Linebender contributors",
        license: include_str!("../../../licenses/native/document/parley-MIT.txt"),
    },
    Attribution {
        name: "Rust File Dialog",
        author: "Poly Meilex and contributors",
        license: include_str!("../../../licenses/native/document/rfd-MIT.txt"),
    },
    Attribution {
        name: "Lato",
        author: "Łukasz Dziedzic",
        license: crate::native::credits::LATO_LICENSE,
    },
    crate::native::credits::DEJAVU,
    Attribution {
        name: "Bevy",
        author: "Bevy contributors",
        license: crate::native::credits::BEVY_LICENSE,
    },
];
