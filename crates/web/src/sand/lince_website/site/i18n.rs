use std::collections::HashMap;

pub const YOUTUBE_URL: &str = "https://www.youtube.com/@lince.social";
pub const GITHUB_LATEST_RELEASE_URL: &str = "https://github.com/lince-social/lince/releases/latest";
pub const INSTINTO_URL: &str = "https://raw.githubusercontent.com/lince-social/lince/main/documentation/lince-documentation-dark.pdf";
pub const LATEST_LINUX_DOWNLOAD_URL: &str =
    "https://github.com/lince-social/lince/releases/latest/download/lince-x86_64-unknown-linux-gnu";
pub const LATEST_MACOS_DOWNLOAD_URL: &str =
    "https://github.com/lince-social/lince/releases/latest/download/lince-aarch64-apple-darwin";
pub const LATEST_WINDOWS_DOWNLOAD_URL: &str = "https://github.com/lince-social/lince/releases/latest/download/lince-x86_64-pc-windows-msvc.exe";

/// Image configuration for content blocks
#[derive(Clone, Default)]
pub struct ContentImage {
    /// Path to the image file (relative to output directory)
    pub src: &'static str,
    /// Alt text for accessibility
    pub alt: &'static str,
    /// CSS class for styling (e.g., "img-rounded", "img-shadow", "img-small")
    pub class: &'static str,
}

/// A content block that can have text and optionally an image
#[derive(Clone)]
pub struct ContentBlock {
    pub title: &'static str,
    pub text: &'static str,
    /// Optional image - when present, text wraps on left, image on right
    pub image: Option<ContentImage>,
}

impl ContentBlock {
    pub const fn text_only(title: &'static str, text: &'static str) -> Self {
        Self {
            title,
            text,
            image: None,
        }
    }

    #[allow(dead_code)]
    pub const fn with_image(
        title: &'static str,
        text: &'static str,
        src: &'static str,
        alt: &'static str,
        class: &'static str,
    ) -> Self {
        Self {
            title,
            text,
            image: Some(ContentImage { src, alt, class }),
        }
    }
}

/// A link item for navigation, footer, or quick links sections
#[derive(Clone)]
pub struct LinkItem {
    pub href: &'static str,
    pub text: &'static str,
    /// Optional CSS class (e.g., "btn btn-primary")
    pub class: &'static str,
}

impl LinkItem {
    pub const fn new(href: &'static str, text: &'static str) -> Self {
        Self {
            href,
            text,
            class: "",
        }
    }

    pub const fn with_class(href: &'static str, text: &'static str, class: &'static str) -> Self {
        Self { href, text, class }
    }
}

/// A group of links with a title (for footer sections, etc.)
#[derive(Clone)]
pub struct LinkGroup {
    pub title: &'static str,
    pub links: Vec<LinkItem>,
}

/// Translation structure for easy AI agent updates
/// To update translations:
/// 1. Modify the English (en) text as the source of truth
/// 2. Ask an AI agent to update pt_br and zh translations to match
/// 3. Each field is clearly labeled for context
#[derive(Clone)]
pub struct Translations {
    pub lang_code: &'static str,

    // Navigation
    pub nav_home: &'static str,
    pub nav_blog: &'static str,
    pub nav_github: &'static str,
    pub nav_download: &'static str,
    pub nav_youtube: &'static str,
    pub nav_theme: &'static str,

    // Hero Section
    pub hero_tagline: &'static str,
    pub hero_title: &'static str,
    pub hero_subtitle: &'static str,
    pub hero_install_label: &'static str,
    pub hero_install_copy: &'static str,
    pub hero_install_copied: &'static str,
    pub hero_install_or: &'static str,
    pub hero_linux_executable: &'static str,
    pub hero_macos_executable: &'static str,
    pub hero_windows_executable: &'static str,
    pub hero_doc_buttons: Vec<LinkItem>,

    // Index Page - Quick Links
    // quick_links removed - merged into `footer_sections` as a LinkGroup

    // Index Page - Main Content (between hero and quicklinks)
    pub index_content: Vec<ContentBlock>,

    // Footer
    pub footer_sections: Vec<LinkGroup>,

    // Blog
    pub blog_title: &'static str,
    pub blog_back_to_posts: &'static str,
    pub blog_watch_video: &'static str,
}

pub fn get_translations() -> HashMap<&'static str, Translations> {
    let mut map = HashMap::new();

    // ============================================================
    // ENGLISH (Source of Truth)
    // ============================================================
    map.insert("en", Translations {
        lang_code: "en",


        // Navigation
        nav_home: "Home",
        nav_blog: "Blog",
        nav_github: "GitHub",
        nav_download: "Download",
        nav_youtube: "YouTube",
        nav_theme: "Theme",

        // Hero Section
        hero_tagline: "Open Source • Non-Profit • Local First • Data-Powered",
        hero_title: "Lince",
        hero_subtitle: "A tool for registry, interconnection, and automation of Needs and Contributions with open scope",
        hero_install_label: "Install:",
        hero_install_copy: "Copy",
        hero_install_copied: "Copied",
        hero_install_or: "or download the",
        hero_linux_executable: "Linux executable",
        hero_macos_executable: "macOS executable",
        hero_windows_executable: "Windows executable",
        hero_doc_buttons: vec![LinkItem::with_class(
            INSTINTO_URL,
            "Documentation",
            "btn btn-primary",
        )],

        // Index Page - Main Content
        index_content: vec![
            ContentBlock::text_only(
                "The Lince Institute",
                r#"
                Lince: an Open Source, Free Forever tool, built by a community,
                backed by the non-profit Lince Institute.
                <br>
                <br>
                Everyone can do something for the documentation, code, design, legal, financial, marketing, tidying, etc.
                If you are interested, join the <a href="https://matrix.to/#/#lince:matrix.org">Matrix</a> or
                <a href="https://discord.gg/3Gr9rYWHpu">Discord</a> and check the end of the
                <a href="https://raw.githubusercontent.com/lince-social/lince/dev/documentation/lince-documentation-dark.pdf">Documentation (Dark Mode)</a>
                <a href="https://raw.githubusercontent.com/lince-social/lince/dev/documentation/lince-documentation-light.pdf">Documentation (Light Mode)</a>

                for tasks (soon to be a DNA).
                "#,
            ),
            ContentBlock::text_only(
                "What is Lince?",
                r#"
                At its core, Lince is a personal database for managing anything through the lens of Needs and Contributions.
                <br>
                <br>
                You can model anything in a Record: a task, item, goal, etc. This Record has some qualitative text fields to describe them, and some quantifiable number
                fields to measure and control them.
                <br>
                <br>
                Negative quantities in the Record are Needs: [ -1, Apple ] is a need of one apple.
                <br>
                Positive quantities are Contributions: [ +1, Apple ] is either a stock for consuming, donating or selling an apple.
                <br>
                <br>
                Inside your own instance of Lince you can model all your tasks, items, and goals.
                Many automations can be created around Records' properties, terminal commands and frequencies.
                <br>
                <br>
                When multiple parties interact, Transfers can happen. You can give 3 (three) moneys as a Contribution to satiate your need of 1 (one) apple.
                The possibilities are as many as are the ways for parties to trade resources.
                <br>
                <br>
                The goal of Lince is to first help people Contribute to their own Needs,
                by organizing themselves with Todo lists, resources needed and financial simulations... all in one place.
                Then, helping them to share part of that data with the world, connecting all production, to efficiently satiate our modeled Needs.
                "#,
            ),
        ],

        // Footer (includes quick links merged as a group)
        footer_sections: vec![
            LinkGroup {
                title: "Resources",
                links: vec![
                    LinkItem::new("https://github.com/lince-social/lince", "Lince Source"),
                    LinkItem::new(INSTINTO_URL, "Documentation"),
                    LinkItem::new(GITHUB_LATEST_RELEASE_URL, "Downloads"),
                    LinkItem::new("visual-identity.html", "Visual Identity"),
                    LinkItem::new("https://github.com/lince-social/lince-social.github.io", "Website Source"),
                ],
            },
            LinkGroup {
                title: "Community",
                links: vec![
                    LinkItem::new("https://matrix.to/#/#lince:matrix.org", "Matrix"),
                    LinkItem::new(YOUTUBE_URL, "Youtube"),
                    LinkItem::new("https://discord.gg/3Gr9rYWHpu", "Discord"),
                    LinkItem::new("https://www.instagram.com/lincesocial", "Instagram"),
                    LinkItem::new("https://github.com/lince-social/lince/discussions", "Discussions"),
                ],
            },
            LinkGroup {
                title: "Legal",
                links: vec![
                    LinkItem::new("https://github.com/lince-social/lince/blob/main/LICENSE", "License"),
                ],
            },
        ],

        // Blog
        blog_title: "Blog",

        blog_back_to_posts: "← Back to Blog Posts",
        blog_watch_video: "Watch on YouTube",
    });

    // ============================================================
    // BRAZILIAN PORTUGUESE
    // ============================================================
    map.insert("pt-br", Translations {
        lang_code: "pt-br",


        // Navigation
        nav_home: "Início",
        nav_blog: "Blog",
        nav_github: "GitHub",
        nav_download: "Baixar",
        nav_youtube: "YouTube",
        nav_theme: "Tema",

        // Hero Section
        hero_tagline: "Código Aberto • Sem Fins Lucrativos • Local First • Impulsionado por Dados",
        hero_title: "Lince",
        hero_subtitle: "Uma ferramenta para registro, interconexão e automação de Necessidades e Contribuições com escopo aberto",
        hero_install_label: "Instale com:",
        hero_install_copy: "Copiar",
        hero_install_copied: "Copiado",
        hero_install_or: "ou baixe o",
        hero_linux_executable: "executável para Linux",
        hero_macos_executable: "executável para macOS",
        hero_windows_executable: "executável para Windows",
        hero_doc_buttons: vec![
            LinkItem::with_class(INSTINTO_URL, "Instinto: Documentação Técnica", "btn btn-primary"),
        ],

        // Index Page - Main Content
        index_content: vec![
            ContentBlock::text_only(
                "O Instituto Lince",
                r#"
                A Lince: uma ferramenta de Código Aberto, sempre gratuita, construída por uma comunidade,
                apoiada pela organização sem fins lucrativos Instituto Lince.
                <br>
                <br>
                Todos podem contribuir com documentação, código, design, jurídico, finanças, marketing, organização, etc.
                Se tiver interesse, entre no
                <a href="https://matrix.to/#/#lince:matrix.org">Matrix</a> ou
                <a href="https://discord.gg/3Gr9rYWHpu">Discord</a> e confira o fim da
                <a href="https://raw.githubusercontent.com/lince-social/lince/dev/documentation/lince-documentation-dark.pdf">Documentação (Modo Escuro)</a>
                <a href="https://raw.githubusercontent.com/lince-social/lince/dev/documentation/lince-documentation-light.pdf">Documentação (Modo Claro)</a>
                para tarefas (em breve uma espécie de DNA).
                "#,
            ),
            ContentBlock::text_only(
                "O que é a Lince?",
                r#"
                Em essência, a Lince é um banco de dados pessoal para gerenciar qualquer coisa através da ótica de Necessidades e Contribuições.
                <br>
                <br>
                Você pode modelar qualquer coisa em um Registro: uma tarefa, item, meta, etc. Esse Registro tem alguns campos textuais qualitativos para descrevê-los, e alguns campos numéricos quantificáveis para medir e controlar.
                <br>
                <br>
                Quantidades negativas no Registro são Necessidades: [ -1, Maçã ] é a necessidade de uma maçã.
                <br>
                Quantidades positivas são Contribuições: [ +1, Maçã ] é um estoque disponível para consumir, doar ou vender.
                <br>
                <br>
                Dentro da sua própria instância da Lince você pode modelar todas as suas tarefas, itens e metas.
                Muitas automações podem ser criadas em torno das propriedades dos Registros, comandos de terminal e frequências.
                <br>
                <br>
                Quando múltiplas partes interagem, Transferências podem ocorrer. Você pode dar 3 (três) dinheiros como uma Contribuição para saciar sua necessidade de 1 (uma) maçã.
                As possibilidades são tantas quanto as formas de trocar recursos entre partes.
                <br>
                <br>
                O objetivo da Lince é, primeiro, ajudar as pessoas a Contribuir para suas próprias Necessidades,
                organizando-se com listas de Tarefas, recursos necessários e simulações financeiras... tudo em um só lugar.
                Em seguida, ajudar a compartilhar parte desses dados com o mundo, conectando toda a produção para saciar de forma eficiente as Necessidades que modelamos.
                "#,
            ),
        ],

        // Footer (matches English structure)
        footer_sections: vec![
            LinkGroup {
                title: "Recursos",
                links: vec![
                    LinkItem::new("https://github.com/lince-social/lince", "Código Fonte"),
                    LinkItem::new(INSTINTO_URL, "Documentação"),
                    LinkItem::new(GITHUB_LATEST_RELEASE_URL, "Downloads"),
                    LinkItem::new("visual-identity.pt-br.html", "Identidade Visual"),
                    LinkItem::new("https://github.com/lince-social/lince-social.github.io", "Código do Site"),
                ],
            },
            LinkGroup {
                title: "Comunidade",
                links: vec![
                    LinkItem::new("https://matrix.to/#/#lince:matrix.org", "Matrix"),
                    LinkItem::new(YOUTUBE_URL, "YouTube"),
                    LinkItem::new("https://discord.gg/3Gr9rYWHpu", "Discord"),
                    LinkItem::new("https://www.instagram.com/lincesocial", "Instagram"),
                    LinkItem::new("https://github.com/lince-social/lince/discussions", "Discussões"),
                ],
            },
            LinkGroup {
                title: "Legal",
                links: vec![
                    LinkItem::new("https://github.com/lince-social/lince/blob/main/LICENSE", "Licença"),
                ],
            },
        ],

        // Blog
        blog_title: "Blog",


        blog_back_to_posts: "← Voltar para Postagens do Blog",
        blog_watch_video: "Ver no YouTube",
    });

    // ============================================================
    // MANDARIN CHINESE (Simplified)
    // ============================================================
    map.insert("zh", Translations {
        lang_code: "zh",


        // Navigation
        nav_home: "首页",
        nav_blog: "博客",
        nav_github: "GitHub",
        nav_download: "下载",
        nav_youtube: "YouTube",
        nav_theme: "主题",

        // Hero Section
        hero_tagline: "开源 • 非营利 • 本地优先 • 数据驱动",
        hero_title: "Lince",
        hero_subtitle: "用于需求与贡献的登记、互联和自动化的工具，开放范围",
        hero_install_label: "安装命令：",
        hero_install_copy: "复制",
        hero_install_copied: "已复制",
        hero_install_or: "或下载",
        hero_linux_executable: "Linux 可执行文件",
        hero_macos_executable: "macOS 可执行文件",
        hero_windows_executable: "Windows 可执行文件",
        hero_doc_buttons: vec![
            LinkItem::with_class(INSTINTO_URL, "Instinto：技术文档", "btn btn-primary"),
        ],

        // Index Page - Main Content
        index_content: vec![
            ContentBlock::text_only(
                "Lince 研究所",
                r#"
                Lince：一个开源、永久免费工具，由社区构建，
                并由非营利机构 Lince 研究所支持。
                <br>
                <br>
                每个人都可以为文档、代码、设计、法律、财务、市场、整理等方面做出贡献。
                如果您有兴趣，请加入
                <a href="https://matrix.to/#/#lince:matrix.org">Matrix</a> 或
                <a href="https://discord.gg/3Gr9rYWHpu">Discord</a>，并查看
                <a href="https://raw.githubusercontent.com/lince-social/lince/dev/documentation/lince-documentation-dark.pdf">文档（深色模式）</a>
                <a href="https://raw.githubusercontent.com/lince-social/lince/dev/documentation/lince-documentation-light.pdf">文档（浅色模式）</a>
                的末尾以了解任务（很快会成为一种 DNA）。
                "#,
            ),
            ContentBlock::text_only(
                "什么是 Lince？",
                r#"
                本质上，Lince 是一个个人数据库，通过需求与贡献的视角来管理任何事物。
                <br>
                <br>
                你可以在一个记录中建模任何事物：任务、物品、目标等。该记录包含一些用于描述的定性文本字段，以及一些用于度量和控制的可量化数字字段。
                <br>
                <br>
                记录中的负数表示需求：[-1，苹果] 表示需要一个苹果。
                <br>
                正数表示贡献：[+1，苹果] 表示可用于消费、捐赠或出售的苹果存量。
                <br>
                <br>
                在你自己的 Lince 实例中，你可以建模所有的任务、物品和目标。
                可以围绕记录的属性、终端命令和频率创建许多自动化。
                <br>
                <br>
                当多个参与方发生交互时，可以发生转移。你可以作为贡献给出 3（三）货币来满足你对 1（一）个苹果的需求。
                可能性有很多，正如参与方交换资源的方式多样。
                <br>
                <br>
                Lince 的目标首先是帮助人们为他们自己的需求做出贡献，
                通过将待办事项列表、所需资源和财务模拟集中在一个地方来进行组织。
                然后，帮助他们将部分数据与世界共享，连接所有生产，以高效满足我们所建模的需求。
                "#,
            ),
        ],

        // Footer (matches English structure)
        footer_sections: vec![
            LinkGroup {
                title: "资源",
                links: vec![
                    LinkItem::new("https://github.com/lince-social/lince", "源代码"),
                    LinkItem::new(INSTINTO_URL, "文档"),
                    LinkItem::new(GITHUB_LATEST_RELEASE_URL, "下载"),
                    LinkItem::new("visual-identity.zh.html", "视觉识别"),
                    LinkItem::new("https://github.com/lince-social/lince-social.github.io", "网站源码"),
                ],
            },
            LinkGroup {
                title: "社区",
                links: vec![
                    LinkItem::new("https://matrix.to/#/#lince:matrix.org", "Matrix"),
                    LinkItem::new(YOUTUBE_URL, "YouTube"),
                    LinkItem::new("https://discord.gg/3Gr9rYWHpu", "Discord"),
                    LinkItem::new("https://www.instagram.com/lincesocial", "Instagram"),
                    LinkItem::new("https://github.com/lince-social/lince/discussions", "讨论"),
                ],
            },
            LinkGroup {
                title: "法律",
                links: vec![
                    LinkItem::new("https://github.com/lince-social/lince/blob/main/LICENSE", "许可证"),
                ],
            },
        ],

        // Blog
        blog_title: "博客",


        blog_back_to_posts: "← 返回博客文章",
        blog_watch_video: "在 YouTube 观看",
    });

    map
}
