use crate::config::INCLUDE_BLOG;
use crate::i18n::{GITHUB_LATEST_RELEASE_URL, Translations, YOUTUBE_URL};
use maud::{DOCTYPE, PreEscaped, html};

fn lang_suffix(lang: &str) -> &str {
    if lang == "en" {
        ""
    } else if lang == "pt-br" {
        ".pt-br"
    } else {
        ".zh"
    }
}

pub fn page(body: &str, t: &Translations, current_page: &str, show_home: bool) -> String {
    let suffix = lang_suffix(t.lang_code);

    // Prepare language suffixes and page links so generated pages point
    // to the actual files produced by `main.rs` (e.g. `index.pt-br.html`).
    let home_href = format!("/index{}.html", suffix);
    let blog_href = format!("/blog{}.html", suffix);
    let link_en = format!("/{}{}.html", current_page, "");
    let link_pt = format!("/{}{}.html", current_page, ".pt-br");
    let link_zh = format!("/{}{}.html", current_page, ".zh");

    html! {
            (DOCTYPE)
            html lang=(t.lang_code) data-theme="dark" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                meta http-equiv="X-UA-Compatible" content="ie=edge";
                meta name="description" content="Lince - Registry, Interconnection, and Automation of Needs and Contributions";
                link rel="icon" href="/assets/black_in_white.ico" type="image/x-icon";
                script {
                    (PreEscaped(r#"(function(){try{const s=localStorage.getItem('theme');if(s)document.documentElement.setAttribute('data-theme',s);else document.documentElement.setAttribute('data-theme','dark');}catch(e){} })();"#))
                }
                link rel="stylesheet" href="/assets/style.css";
                title { "Lince" }
            }
            body {
                nav.navbar {
                    .navbar-container {
                        a.navbar-brand href=(home_href) {
                            img src="/assets/logo/white.svg" alt="Lince Logo";
                            "Lince"
                        }
                        ul.navbar-menu {
                            @if show_home {
                                li {
                                    a class=(if current_page == "index" { "navbar-item active" } else { "navbar-item" })
                                        href=(home_href.clone()) {
                                        (t.nav_home)
                                    }
                                }
                            }
                            @if INCLUDE_BLOG {
                                li {
                                    a class=(if current_page.starts_with("blog") || current_page == "blog" { "navbar-item active" } else { "navbar-item" })
                                        href=(blog_href.clone()) {
                                        (t.nav_blog)
                                    }
                                }
                            }
                            li.desktop-only {
                                a.navbar-item href="https://github.com/lince-social/lince" {
                                    (t.nav_github)
                                }
                            }
                            li.desktop-only {
                                a.navbar-item href=(GITHUB_LATEST_RELEASE_URL) {
                                    (t.nav_download)
                                }
                            }
                            li.desktop-only {
                                a.navbar-item href=(YOUTUBE_URL) {
                                    (t.nav_youtube)
                                }
                            }
                            li.lang-switcher {
                                button.lang-btn onclick="toggleLangDropdown()" {
                                    @if t.lang_code == "en" { "EN" }
                                    @else if t.lang_code == "pt-br" { "PT" }
                                    @else { "中文" }
                                }
                                .lang-dropdown id="langDropdown" {
                                    a.lang-option href=(link_en) { "English" }
                                    a.lang-option href=(link_pt) { "Português" }
                                    a.lang-option href=(link_zh) { "中文" }
                                }
                            }
                            li {
                                button.theme-toggle onclick="toggleTheme()" title=(t.nav_theme) {
                                    "◐"
                                }
                            }
                        }
                    }
                }
                (PreEscaped(body))
                footer.footer {
                    .footer-container {
                        @for section in &t.footer_sections {
                            .footer-section {
                                h4 { (section.title) }
                                ul.footer-links {
                                    @for link in &section.links {
                                        @let href = if link.href.starts_with("http") {
                                            link.href.to_string()
                                        } else if link.href.starts_with("/") {
                                            link.href.to_string()
                                        } else {
                                            format!("/{}", link.href)
                                        };
                                        li { a href=(href) { (link.text) } }
                                    }
                                }
                            }
                        }
                    }
                }
                script {
                    (PreEscaped(r#"
                    function detectHeroDownloadOs() {
                        const platform = (
                            (navigator.userAgentData && navigator.userAgentData.platform) ||
                            navigator.platform ||
                            navigator.userAgent ||
                            ""
                        ).toLowerCase();

                        if (platform.includes('mac')) return 'macos';
                        if (platform.includes('win')) return 'windows';
                        return 'linux';
                    }
                    function configureHeroDownloadLink() {
                        const link = document.getElementById('hero-os-download');
                        if (!link) return;
                        const os = detectHeroDownloadOs();
                        const href = link.dataset[os + 'Href'] || link.dataset.linuxHref;
                        const text = link.dataset[os + 'Text'] || link.dataset.linuxText;
                        link.href = href;
                        link.textContent = text;
                    }
                    async function copyHeroInstall(button) {
                        const row = button.closest('.hero-install-row');
                        if (!row) return;
                        const command = row.querySelector('.hero-install-command');
                        if (!command) return;
                        const text = command.dataset.copyText || command.querySelector('.hero-install-text')?.textContent?.trim() || '';
                        if (!text) return;

                        try {
                            await navigator.clipboard.writeText(text);
                        } catch (err) {
                            const helper = document.createElement('textarea');
                            helper.value = text;
                            helper.setAttribute('readonly', '');
                            helper.style.position = 'fixed';
                            helper.style.opacity = '0';
                            helper.style.pointerEvents = 'none';
                            document.body.appendChild(helper);
                            helper.focus();
                            helper.select();
                            document.execCommand('copy');
                            document.body.removeChild(helper);
                        }

                        const defaultLabel = button.dataset.copyDefault || button.getAttribute('aria-label') || '';
                        const successLabel = button.dataset.copySuccess || defaultLabel;
                        button.setAttribute('aria-label', successLabel);
                        row.classList.add('is-copied');
                        window.setTimeout(() => {
                            button.setAttribute('aria-label', defaultLabel);
                            row.classList.remove('is-copied');
                        }, 1000);
                    }
                    function copyHeroInstallFromCommand(command) {
                        const button = command.querySelector('.hero-install-copy');
                        if (!button) return;
                        copyHeroInstall(button);
                    }
                    function toggleTheme() {
                        const html = document.documentElement;
                        const current = html.getAttribute('data-theme');
                        const next = current === 'dark' ? 'light' : 'dark';
                        html.setAttribute('data-theme', next);
                        localStorage.setItem('theme', next);
                    }
                    (function() {
                        const saved = localStorage.getItem('theme');
                        if (saved) {
                            document.documentElement.setAttribute('data-theme', saved);
                        }
                        configureHeroDownloadLink();
                    })();
                    function toggleLangDropdown() {
                        document.getElementById('langDropdown').classList.toggle('show');
                    }
                    document.addEventListener('click', function(e) {
                        if (!e.target.closest('.lang-switcher')) {
                            document.getElementById('langDropdown').classList.remove('show');
                        }
                    });
                    "#))
                }
            }
        }
    }
    .into_string()
}
