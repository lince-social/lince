use crate::i18n::{
    LATEST_LINUX_DOWNLOAD_URL, LATEST_MACOS_DOWNLOAD_URL, LATEST_WINDOWS_DOWNLOAD_URL,
    Translations,
};
use maud::{PreEscaped, html};

const INSTALL_COMMAND: &str = "curl -fsSL https://lince.social/install.sh | bash";

pub fn page_index(t: &Translations) -> String {
    html! {
        section.hero {
            .hero-container {
                img.hero-logo src="assets/logo/white.svg" alt="Lince Logo";
                h1.hero-title { (t.hero_title) }
                p.hero-tagline { (t.hero_tagline) }
                p.hero-subtitle { (t.hero_subtitle) }

                .hero-button-stack {
                    .hero-buttons.hero-buttons--docs {
                        @for btn in &t.hero_doc_buttons {
                            a class=(btn.class) href=(btn.href) { (btn.text) }
                        }
                    }

                    .hero-install-row {
                        span.hero-install-label { (t.hero_install_label) }
                        .hero-install-command
                            data-copy-text=(INSTALL_COMMAND)
                            onclick="copyHeroInstallFromCommand(this)" {
                            code.hero-install-text { (INSTALL_COMMAND) }
                            button.hero-install-copy
                                type="button"
                                title=(t.hero_install_copy)
                                aria-label=(t.hero_install_copy)
                                data-copy-default=(t.hero_install_copy)
                                data-copy-success=(t.hero_install_copied)
                                onclick="event.stopPropagation(); copyHeroInstall(this)" {
                                span.copy-icon aria-hidden="true" {}
                            }
                        }
                        p.hero-install-alt {
                            (t.hero_install_or)
                            " "
                            a.hero-os-download
                                id="hero-os-download"
                                href=(LATEST_LINUX_DOWNLOAD_URL)
                                data-linux-href=(LATEST_LINUX_DOWNLOAD_URL)
                                data-macos-href=(LATEST_MACOS_DOWNLOAD_URL)
                                data-windows-href=(LATEST_WINDOWS_DOWNLOAD_URL)
                                data-linux-text=(t.hero_linux_executable)
                                data-macos-text=(t.hero_macos_executable)
                                data-windows-text=(t.hero_windows_executable) {
                                (t.hero_linux_executable)
                            }
                        }
                    }
                }
            }
        }

        main.main-content {
            @for block in &t.index_content {
                @if let Some(ref img) = block.image {
                    .content-block.content-block--with-image {
                        .content-block__text {
                            h2.content-block__title { (block.title) }
                            p.content-block__body { (PreEscaped(block.text)) }
                        }
                        .content-block__image {
                            img class=(img.class) src=(img.src) alt=(img.alt);
                        }
                    }
                } @else {
                    .content-block {
                        h2.content-block__title { (block.title) }
                        p.content-block__body { (PreEscaped(block.text)) }
                    }
                }
            }
        }
    }
    .0
}
