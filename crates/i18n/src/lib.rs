#![forbid(unsafe_code)]

use std::sync::{Arc, OnceLock};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    English,
    PortugueseBrazil,
    Ithkuil,
}

impl Language {
    pub const ALL: [Self; 3] = [Self::English, Self::PortugueseBrazil, Self::Ithkuil];

    pub fn from_tag(tag: &str) -> Option<Self> {
        if tag.eq_ignore_ascii_case("en") || tag.eq_ignore_ascii_case("en-us") {
            Some(Self::English)
        } else if tag.eq_ignore_ascii_case("pt-br") || tag.eq_ignore_ascii_case("pt") {
            Some(Self::PortugueseBrazil)
        } else if tag.eq_ignore_ascii_case("art-x-ithkuil") {
            Some(Self::Ithkuil)
        } else {
            None
        }
    }

    pub const fn tag(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::PortugueseBrazil => "pt-BR",
            Self::Ithkuil => "art-x-ithkuil",
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::PortugueseBrazil => "Português (Brasil)",
            Self::Ithkuil => "New Ithkuil (TNIL)",
        }
    }

    pub const fn catalog(self) -> &'static Catalog {
        match self {
            Self::English => &ENGLISH,
            Self::PortugueseBrazil => &PORTUGUESE,
            Self::Ithkuil => &ITHKUIL,
        }
    }
}

pub struct Catalog {
    pub language: Language,
    entries: [&'static str; Token::ALL.len()],
}

impl Catalog {
    #[inline]
    pub fn get(&self, token: Token) -> &'static str {
        self.entries[token as usize]
    }
}

macro_rules! messages {
    ($($token:ident => ($english:literal, $portuguese:literal, $ithkuil:expr)),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(usize)]
        pub enum Token {
            $($token),+
        }

        impl Token {
            pub const ALL: &'static [Self] = &[$(Self::$token),+];

            #[inline]
            pub fn text(self) -> &'static str {
                catalog().get(self)
            }
        }

        static ENGLISH: Catalog = Catalog {
            language: Language::English,
            entries: [$($english),+],
        };

        static PORTUGUESE: Catalog = Catalog {
            language: Language::PortugueseBrazil,
            entries: [$($portuguese),+],
        };

        static ITHKUIL: Catalog = Catalog {
            language: Language::Ithkuil,
            entries: [$(match $ithkuil {
                Some(text) => text,
                None => $english,
            }),+],
        };
    };
}

messages! {
    EditMode => ("Edit mode", "Modo de edição", None),
    DoneEditing => ("Done editing", "Concluir edição", None),
    General => ("General", "Geral", None),
    SandStore => ("Sand store", "Loja de Sands", None),
    Customization => ("Customization", "Personalização", None),
    Workspaces => ("Workspaces", "Áreas de trabalho", None),
    Shortcuts => ("Shortcuts", "Atalhos", None),
    ShortcutUnfocus => ("Focus / Unfocus", "Focar / Desfocar", None),
    FocusTarget => ("Focus", "Focar", None),
    UnfocusTarget => ("Unfocus", "Desfocar", None),
    ShortcutDeleteSand => ("Delete selected Sands or Castles", "Excluir Sands ou Castles selecionados", None),
    ShortcutToggleEdit => ("Toggle edit mode", "Alternar modo de edição", None),
    ShortcutOverview => ("Overview", "Visão geral", None),
    OverviewMonitor => ("Monitor", "Monitor", None),
    OverviewCurrent => ("Current", "Atual", None),
    OverviewKeys => ("Arrow keys: choose · Enter: open · Escape: back", "Setas: escolher · Enter: abrir · Escape: voltar", None),
    ShortcutDesktopEdit => ("Toggle desktop edit mode", "Alternar edição da área de trabalho", None),
    ShortcutOpenOperation => ("Open Operation", "Abrir Operation", None),
    ShortcutForward => ("Move forward in 3D", "Avançar em 3D", None),
    ShortcutBackward => ("Move backward in 3D", "Recuar em 3D", None),
    ShortcutLeft => ("Move left in 3D", "Mover para a esquerda em 3D", None),
    ShortcutRight => ("Move right in 3D", "Mover para a direita em 3D", None),
    ShortcutDown => ("Move down in 3D", "Descer em 3D", None),
    ShortcutUp => ("Move up in 3D", "Subir em 3D", None),
    ShortcutTurnLeft => ("Turn left in 3D", "Virar à esquerda em 3D", None),
    ShortcutTurnRight => ("Turn right in 3D", "Virar à direita em 3D", None),
    ShortcutLookUp => ("Look up in 3D", "Olhar para cima em 3D", None),
    ShortcutLookDown => ("Look down in 3D", "Olhar para baixo em 3D", None),
    ShortcutFocusLeft => ("Focus the item on the left", "Focar o item à esquerda", None),
    ShortcutFocusDown => ("Focus the item below", "Focar o item abaixo", None),
    ShortcutFocusUp => ("Focus the item above", "Focar o item acima", None),
    ShortcutFocusRight => ("Focus the item on the right", "Focar o item à direita", None),
    ShortcutMoveLeft => ("Move the focused item left", "Mover o item focado à esquerda", None),
    ShortcutMoveDown => ("Move the focused item down", "Mover o item focado para baixo", None),
    ShortcutMoveUp => ("Move the focused item up", "Mover o item focado para cima", None),
    ShortcutMoveRight => ("Move the focused item right", "Mover o item focado à direita", None),
    ShortcutPointerMove => ("Move an item with the mouse", "Mover um item com o mouse", None),
    ShortcutPointerResize => ("Resize an item with the mouse", "Redimensionar um item com o mouse", None),
    ShortcutFloat => ("Toggle floating for the focused item", "Alternar flutuação do item focado", None),
    ShortcutTiling => ("Toggle workspace tiling", "Alternar organização da área de trabalho em mosaico", None),
    ShortcutCycleWidth => ("Cycle window width", "Alternar largura da janela", None),
    ShortcutShrinkWidth => ("Decrease item width", "Diminuir largura do item", None),
    ShortcutGrowWidth => ("Increase item width", "Aumentar largura do item", None),
    ShortcutShrinkHeight => ("Decrease item or row height", "Diminuir altura do item ou da linha", None),
    ShortcutGrowHeight => ("Increase item or row height", "Aumentar altura do item ou da linha", None),
    ShortcutCycleRowHeight => ("Cycle row height", "Alternar altura da linha", None),
    TilingOn => ("Tiling on", "Mosaico ativo", None),
    TilingOff => ("Tiling off", "Mosaico inativo", None),
    ShortcutFullscreen => ("Toggle fullscreen for the focused item", "Alternar tela cheia do item focado", None),
    ShortcutMaximize => ("Toggle maximize for the focused item", "Alternar maximização do item focado", None),
    ShortcutDesktopOperation => ("Open desktop Operation", "Abrir Operation na área de trabalho", None),
    ShortcutFocusLeftArrow => ("Focus left with an arrow key", "Focar à esquerda com a tecla de seta", None),
    ShortcutFocusDownArrow => ("Focus down with an arrow key", "Focar abaixo com a tecla de seta", None),
    ShortcutFocusUpArrow => ("Focus up with an arrow key", "Focar acima com a tecla de seta", None),
    ShortcutFocusRightArrow => ("Focus right with an arrow key", "Focar à direita com a tecla de seta", None),
    ShortcutClose => ("Close the focused item", "Fechar o item focado", None),
    ShortcutWorkspace1 => ("Select workspace 1", "Selecionar área de trabalho 1", None),
    ShortcutWorkspace2 => ("Select workspace 2", "Selecionar área de trabalho 2", None),
    ShortcutWorkspace3 => ("Select workspace 3", "Selecionar área de trabalho 3", None),
    ShortcutWorkspace4 => ("Select workspace 4", "Selecionar área de trabalho 4", None),
    ShortcutWorkspace5 => ("Select workspace 5", "Selecionar área de trabalho 5", None),
    ShortcutWorkspace6 => ("Select workspace 6", "Selecionar área de trabalho 6", None),
    ShortcutWorkspace7 => ("Select workspace 7", "Selecionar área de trabalho 7", None),
    ShortcutWorkspace8 => ("Select workspace 8", "Selecionar área de trabalho 8", None),
    ShortcutWorkspace9 => ("Select workspace 9", "Selecionar área de trabalho 9", None),
    ShortcutWorkspacePrevious => ("Previous workspace", "Área de trabalho anterior", None),
    ShortcutWorkspaceNext => ("Next workspace", "Próxima área de trabalho", None),
    ShortcutWorkspaceEarlier => ("Move workspace earlier", "Mover área de trabalho para antes", None),
    ShortcutWorkspaceLater => ("Move workspace later", "Mover área de trabalho para depois", None),
    ShortcutMoveWorkspace1 => ("Move item to workspace 1", "Mover item para a área de trabalho 1", None),
    ShortcutMoveWorkspace2 => ("Move item to workspace 2", "Mover item para a área de trabalho 2", None),
    ShortcutMoveWorkspace3 => ("Move item to workspace 3", "Mover item para a área de trabalho 3", None),
    ShortcutMoveWorkspace4 => ("Move item to workspace 4", "Mover item para a área de trabalho 4", None),
    ShortcutMoveWorkspace5 => ("Move item to workspace 5", "Mover item para a área de trabalho 5", None),
    ShortcutMoveWorkspace6 => ("Move item to workspace 6", "Mover item para a área de trabalho 6", None),
    ShortcutMoveWorkspace7 => ("Move item to workspace 7", "Mover item para a área de trabalho 7", None),
    ShortcutMoveWorkspace8 => ("Move item to workspace 8", "Mover item para a área de trabalho 8", None),
    ShortcutMoveWorkspace9 => ("Move item to workspace 9", "Mover item para a área de trabalho 9", None),
    ShortcutFocusMonitorLeft => ("Focus the monitor on the left", "Focar o monitor à esquerda", None),
    ShortcutFocusMonitorDown => ("Focus the monitor below", "Focar o monitor abaixo", None),
    ShortcutFocusMonitorUp => ("Focus the monitor above", "Focar o monitor acima", None),
    ShortcutFocusMonitorRight => ("Focus the monitor on the right", "Focar o monitor à direita", None),
    ShortcutMoveMonitorLeft => ("Move the focused item to the left monitor", "Mover o item focado para o monitor à esquerda", None),
    ShortcutMoveMonitorDown => ("Move the focused item to the monitor below", "Mover o item focado para o monitor abaixo", None),
    ShortcutMoveMonitorUp => ("Move the focused item to the monitor above", "Mover o item focado para o monitor acima", None),
    ShortcutMoveMonitorRight => ("Move the focused item to the right monitor", "Mover o item focado para o monitor à direita", None),
    ShortcutWorkspaceMonitorLeft => ("Move the workspace to the left monitor", "Mover a área de trabalho para o monitor à esquerda", None),
    ShortcutWorkspaceMonitorDown => ("Move the workspace to the monitor below", "Mover a área de trabalho para o monitor abaixo", None),
    ShortcutWorkspaceMonitorUp => ("Move the workspace to the monitor above", "Mover a área de trabalho para o monitor acima", None),
    ShortcutWorkspaceMonitorRight => ("Move the workspace to the right monitor", "Mover a área de trabalho para o monitor à direita", None),
    ShortcutRowMonitorLeft => ("Move row to the left monitor", "Mover linha para o monitor à esquerda", None),
    ShortcutRowMonitorDown => ("Move row to the monitor below", "Mover linha para o monitor abaixo", None),
    ShortcutRowMonitorUp => ("Move row to the monitor above", "Mover linha para o monitor acima", None),
    ShortcutRowMonitorRight => ("Move row to the right monitor", "Mover linha para o monitor à direita", None),
    ShortcutWindowRecord => ("Window properties", "Propriedades da janela", None),
    ShortcutStackLeft => ("Stack the focused item into the column on the left", "Agrupar o item focado na coluna à esquerda", None),
    ShortcutStackRight => ("Stack the focused item into the column on the right", "Agrupar o item focado na coluna à direita", None),
    ShortcutTerminal => ("Open the chosen terminal", "Abrir o terminal escolhido", None),
    ShortcutLauncher => ("Open the chosen application launcher", "Abrir o lançador de aplicativos escolhido", None),
    ExternalBrightnessDown => ("Decrease brightness", "Diminuir brilho", None),
    ExternalBrightnessUp => ("Increase brightness", "Aumentar brilho", None),
    ExternalVolumeDown => ("Decrease volume", "Diminuir volume", None),
    ExternalVolumeUp => ("Increase volume", "Aumentar volume", None),
    ExternalVolumeMute => ("Toggle audio mute", "Ativar ou desativar áudio", None),
    ExternalMicrophoneMute => ("Toggle microphone mute", "Ativar ou desativar microfone", None),
    ExternalMediaPlayPause => ("Play or pause media", "Reproduzir ou pausar mídia", None),
    ExternalMediaNext => ("Next media track", "Próxima faixa", None),
    ExternalMediaPrevious => ("Previous media track", "Faixa anterior", None),
    ExternalScreenshotScreen => ("Capture the screen", "Capturar a tela", None),
    ExternalScreenshotRegion => ("Capture a region", "Capturar uma região", None),
    ExternalEditor => ("Open the chosen editor", "Abrir o editor escolhido", None),
    ShortcutLock => ("Request the chosen screen locker", "Solicitar o bloqueador de tela escolhido", None),
    ShortcutLogout => ("Log out of Park", "Sair do Park", None),
    ShortcutRecentNext => ("Next recent window", "Próxima janela recente", None),
    ShortcutRecentPrevious => ("Previous recent window", "Janela recente anterior", None),
    RecentWindows => ("Recent windows", "Janelas recentes", None),
    RecentHelp => ("Release the shared hold modifier to switch. Escape cancels; a click selects.", "Solte o modificador compartilhado para alternar. Escape cancela; um clique seleciona.", None),
    ParkLogoutTitle => ("Log out of Park?", "Sair do Park?", None),
    ParkLogoutDescription => ("Save Lince changes and end this Park session.", "Salvar as alterações do Lince e encerrar esta sessão do Park.", None),
    ParkLogoutBusy => ("Park could not accept logout. Your session remains open; try again.", "O Park não pôde aceitar a saída. Sua sessão continua aberta; tente novamente.", None),
    ParkSettingsDescription => ("Configure monitors, layout and the shared commands for Park and the canvas.", "Configure monitores, disposição e os comandos compartilhados entre Park e a tela.", None),
    ParkMonitors => ("Monitors", "Monitores", None),
    ParkArrangeMonitors => ("Arrange monitors", "Organizar monitores", None),
    ParkMonitorHost => ("Physical monitors belong to the host compositor in Sandbox.", "No Sandbox, os monitores físicos são controlados pelo compositor do sistema.", None),
    ParkMonitorDrag => ("Drag a monitor or edit its coordinates. Apply tests the layout before a 15-second confirmation.", "Arraste um monitor ou edite suas coordenadas. Aplicar testa a disposição antes da confirmação de 15 segundos.", None),
    ParkMonitorIdentify => ("Identify", "Identificar", None),
    ParkMonitorScale => ("Scale", "Escala", None),
    ParkMonitorModes => ("Resolution and refresh rate", "Resolução e taxa de atualização", None),
    ParkMonitorTest => ("Test", "Testar", None),
    ParkMonitorKeep => ("Keep changes", "Manter alterações", None),
    ParkMonitorRevert => ("Revert", "Reverter", None),
    ParkMonitorReference => ("Reference", "Referência", None),
    ParkMonitorLeft => ("Left of", "À esquerda de", None),
    ParkMonitorRight => ("Right of", "À direita de", None),
    ParkMonitorAbove => ("Above", "Acima de", None),
    ParkMonitorBelow => ("Below", "Abaixo de", None),
    ParkMonitorStale => ("Monitor configuration changed. Reload this draft before applying it.", "A configuração dos monitores mudou. Atualize este rascunho antes de aplicá-lo.", None),
    AreasInfluence => ("Areas of influence", "Áreas de influência", None),
    CloseEditMode => ("Close edit mode", "Fechar modo de edição", None),
    AlwaysControlsOn => ("Always show controls: On", "Sempre mostrar controles: ativado", None),
    AlwaysControlsOff => ("Always show controls: Off", "Sempre mostrar controles: desativado", None),
    EditComponent => ("Edit native component", "Editar componente nativo", None),
    SurfaceEffect => ("Sand surface effect…", "Efeito de superfície do Sand…", None),
    NewWorkspace => ("New workspace", "Nova área de trabalho", None),
    LessRetention => ("Keep 1 day less", "Manter por um dia a menos", None),
    MoreRetention => ("Keep 1 day more", "Manter por um dia a mais", None),
    TrashWorkspace => ("Move workspace to Trash", "Mover área de trabalho para a lixeira", None),
    KeepWorkspace => ("Keep workspace", "Manter área de trabalho", None),
    Filter => ("Filter", "Filtrar", None),
    FilterSandStore => ("Filter Sand store", "Filtrar loja de Sands", None),
    InWorkspace => ("In this workspace", "Nesta área de trabalho", None),
    CloseNotifications => ("Close notifications", "Fechar notificações", None),
    NoNotificationsShort => ("No notifications.", "Nenhuma notificação.", None),
    DeleteNotifications => ("Delete all notifications", "Excluir todas as notificações", None),
    CloseToast => ("Close notification toast", "Fechar aviso de notificação", None),
    DeleteNotification => ("Delete notification", "Excluir notificação", None),
    NewFile => ("New file", "Novo arquivo", None),
    Choose => ("Choose…", "Escolher…", None),
    Up => ("Up", "Subir", None),
    NewFolder => ("New folder", "Nova pasta", None),
    DeletePrompt => ("Delete…", "Excluir…", None),
    UndoDelete => ("Undo delete", "Desfazer exclusão", None),
    FindPath => ("Find a path in the selected roots", "Encontrar um caminho nas raízes selecionadas", None),
    Tree => ("Tree", "Árvore", None),
    Grid => ("Grid", "Grade", None),
    Ignored => ("Ignored", "Ignorados", None),
    Extract => ("Extract", "Extrair", None),
    OpenPrompt => ("Open…", "Abrir…", None),
    SaveAs => ("Save as…", "Salvar como…", None),
    Redo => ("Redo", "Refazer", None),
    CloseTab => ("Close tab", "Fechar aba", None),
    Discard => ("Discard…", "Descartar…", None),
    RecoverDrafts => ("Recover drafts", "Recuperar rascunhos", None),
    Find => ("Find", "Localizar", None),
    LanguageTools => ("Language tools", "Ferramentas de linguagem", None),
    Format => ("Format", "Formatar", None),
    CompleteCode => ("Complete code", "Completar código", None),
    EncryptedBackup => ("Encrypted owner backup", "Backup criptografado do proprietário", None),
    NewBackupFile => ("New backup file", "Novo arquivo de backup", None),
    BackupPassphrase => ("Backup passphrase", "Frase secreta do backup", None),
    RepeatBackupPassphrase => ("Repeat backup passphrase", "Repita a frase secreta do backup", None),
    BackupReopen => ("Back up and reopen Lince", "Fazer backup e reabrir o Lince", None),
    PassphraseHelp => ("Use a long passphrase and keep it with your backup; it is required to open the file.", "Use uma frase secreta longa e guarde-a com o backup; ela é necessária para abrir o arquivo.", None),
    LocalOwner => ("Local Organ · owner connection", "Organ local · conexão do proprietário", None),
    LoadingUsers => ("Loading users and Roles…", "Carregando usuários e Roles…", None),
    NewPassword => ("New password · leave blank to keep it", "Nova senha · deixe em branco para manter a atual", None),
    SaveUser => ("Save user details", "Salvar dados do usuário", None),
    DeleteRole => ("Delete Role", "Excluir Role", None),
    DeleteUser => ("Soft-delete user", "Excluir usuário sem apagar o histórico", None),
    VocabularyLanguageHelp => ("Vocabulary wording can change independently. Choose the interface language in Configuration and restart Lince.", "Os termos do vocabulário podem mudar de forma independente. Escolha o idioma da interface em Configuração e reinicie o Lince.", None),
    Save => ("Save", "Salvar", None),
    Cancel => ("Cancel", "Cancelar", None),
    Close => ("Close", "Fechar", None),
    Open => ("Open", "Abrir", None),
    Delete => ("Delete", "Excluir", None),
    Remove => ("Remove", "Remover", None),
    Add => ("Add", "Adicionar", None),
    Edit => ("Edit", "Editar", None),
    Refresh => ("Refresh", "Atualizar", None),
    Previous => ("Previous", "Anterior", None),
    Next => ("Next", "Próximo", None),
    Back => ("Back", "Voltar", None),
    Search => ("Search", "Buscar", None),
    Clear => ("Clear", "Limpar", None),
    Reset => ("Reset", "Redefinir", None),
    Apply => ("Apply", "Aplicar", None),
    Undo => ("Undo", "Desfazer", None),
    Name => ("Name", "Nome", Some("adnila")),
    Title => ("Title", "Título", Some("ardnila")),
    Description => ("Description", "Descrição", Some("afçäla")),
    Message => ("Message", "Mensagem", Some("amäla")),
    Information => ("Information", "Informações", Some("aţčäla")),
    Settings => ("Settings", "Configurações", None),
    Configuration => ("Configuration", "Configuração", None),
    Language => ("Language", "Idioma", None),
    InterfaceLanguage => ("Interface language", "Idioma da interface", None),
    RestartRequired => ("Changes to the interface language take effect the next time Lince starts.", "As alterações de idioma da interface entram em vigor na próxima inicialização do Lince.", None),
    LanguageSaved => ("Language saved. Restart Lince to apply it.", "Idioma salvo. Reinicie o Lince para aplicá-lo.", None),
    IthkuilFallback => ("New Ithkuil is experimental. Missing translations use English.", "O novo Ithkuil é experimental. Traduções ausentes usam inglês.", None),
    ActiveLanguage => ("Current interface language", "Idioma atual da interface", None),
    NextStartupLanguage => ("Language for the next startup", "Idioma para a próxima inicialização", None),
    ChooseLanguage => ("Choose a language for the next startup", "Escolha um idioma para a próxima inicialização", None),
    Loading => ("Loading…", "Carregando…", None),
    Saving => ("Saving…", "Salvando…", None),
    Saved => ("Saved.", "Salvo.", None),
    LoadingConfiguration => ("Loading configuration…", "Carregando configuração…", None),
    WaitForRequest => ("Wait for the current request to finish", "Aguarde a conclusão da solicitação atual", None),
    SavedRefresh => ("Saved. Refresh to read the current settings.", "Salvo. Atualize para ler as configurações atuais.", None),
    LoadedSettings => ("Loaded. Changes are saved when submitted.", "Carregado. As alterações são salvas ao serem enviadas.", None),
    LaboratoryUnavailable => ("Changes are unavailable in the Laboratory", "As alterações estão indisponíveis no Laboratório", None),
    ConfigurationUnavailable => ("Configuration is unavailable in the Laboratory", "A configuração está indisponível no Laboratório", None),
    CellUnavailable => ("The Cell is unavailable", "A Cell está indisponível", None),
    RuntimeUnavailable => ("The local runtime is unavailable", "O ambiente local de execução está indisponível", None),
    ConfigurationClosed => ("Configuration is closed", "A configuração está fechada", None),
    LoadConfigurationFirst => ("Load the configuration first", "Carregue a configuração primeiro", None),
    Cell => ("Cell", "Cell", None),
    Discovery => ("Discovery", "Descoberta", None),
    Storage => ("Storage", "Armazenamento", None),
    Contacts => ("Contacts", "Contatos", None),
    LocalIdentity => ("How this Organ identifies itself locally", "Como este Organ se identifica localmente", None),
    DescriptiveAddress => ("Descriptive address", "Endereço descritivo", None),
    SaveIdentity => ("Save identity", "Salvar identidade", None),
    DiscoveryHelp => ("Where this Cell can be found. These settings do not create trust.", "Onde esta Cell pode ser encontrada. Estas configurações não estabelecem confiança.", None),
    FindLan => ("Find devices on this Wi-Fi", "Encontrar dispositivos neste Wi-Fi", None),
    LanDuration => ("Visible for 15 minutes. Uses LAN only.", "Visível por 15 minutos. Usa apenas a rede local.", None),
    AdvertiseLan => ("Find and advertise Cells on this LAN", "Encontrar e anunciar Cells nesta rede local", None),
    InternetReachability => ("Internet reachability", "Acesso pela internet", None),
    DirectConnections => ("Allow direct internet connections (reveals this machine’s address)", "Permitir conexões diretas pela internet (revela o endereço desta máquina)", None),
    UnknownConversations => ("Accept unknown conversations", "Aceitar conversas desconhecidas", None),
    LanMinutes => ("LAN presence in minutes (0 means no time limit)", "Presença na rede local em minutos (0 significa sem limite)", None),
    RelayUrls => ("HTTPS connection relay URLs, separated by commas (blank uses Iroh preset)", "URLs HTTPS de retransmissão, separadas por vírgulas (em branco usa o padrão do Iroh)", None),
    SaveDiscovery => ("Save discovery and restart connections", "Salvar descoberta e reiniciar conexões", None),
    DiscoveryRestartHelp => ("Changing reach or relay selection restarts live connections. Connection relays help devices communicate while online; selected Lince mailboxes retain encrypted messages while a person is offline. Local-only reach uses no connection relay.", "Alterar o alcance ou os retransmissores reinicia as conexões ativas. Retransmissores conectam dispositivos online; caixas postais do Lince guardam mensagens criptografadas enquanto a pessoa está offline. O alcance apenas local não usa retransmissores.", None),
    PeerPort => ("Peer UDP port (0 chooses automatically)", "Porta UDP dos pares (0 escolhe automaticamente)", None),
    SavePeerPort => ("Save peer port", "Salvar porta dos pares", None),
    LoadingPeers => ("Loading peer addresses…", "Carregando endereços dos pares…", None),
    DiskBudget => ("Disk budget in MiB (0 means unlimited)", "Limite de disco em MiB (0 significa ilimitado)", None),
    SaveDiskBudget => ("Save disk budget", "Salvar limite de disco", None),
    LoadingDisk => ("Loading disk usage…", "Carregando uso do disco…", None),
    SavingDisk => ("Saving disk budget…", "Salvando limite de disco…", None),
    SelectContactHelp => ("Select a contact above to edit its properties", "Selecione um contato acima para editar suas propriedades", None),
    NoContactSelected => ("Trust: no contact selected", "Confiança: nenhum contato selecionado", None),
    Trust => ("Trust", "Confiança", None),
    Proximity => ("Proximity (non-negative whole number)", "Proximidade (número inteiro não negativo)", None),
    SaveContact => ("Save contact", "Salvar contato", None),
    NoContacts => ("No contacts yet", "Ainda não há contatos", None),
    On => ("on", "ativado", None),
    Off => ("off", "desativado", None),
    Unlimited => ("unlimited", "ilimitado", None),
    Unknown => ("unknown", "desconhecido", None),
    Known => ("known", "conhecido", None),
    Blocked => ("blocked", "bloqueado", None),
    Licenses => ("Licenses and credits", "Licenças e créditos", None),
    Square => ("Square", "Quadrado", None),
    PlainText => ("Plain text", "Texto simples", Some("aňala")),
    EditableText => ("Editable text", "Texto editável", None),
    Operation => ("Operation", "Operação", None),
    TimeCastle => ("Time Castle", "Castelo do tempo", None),
    AccessControl => ("Access Control", "Controle de acesso", None),
    Visibility => ("Visibility", "Visibilidade", None),
    HandCamera => ("Hand Camera Castle", "Castelo de câmera da mão", None),
    Video => ("Video Castle", "Castelo de vídeo", None),
    Sync => ("Sync", "Sincronização", None),
    Terminal => ("Terminal", "Terminal", None),
    Organ => ("Organ", "Organ", None),
    Todo => ("Todo", "Tarefas", None),
    Ontology => ("Ontology", "Ontologia", None),
    EmptySpaceHelp => ("An empty space to compose with.", "Um espaço vazio para compor.", None),
    PlainTextHelp => ("A simple text label.", "Um rótulo de texto simples.", None),
    EditableTextHelp => ("A note you can write in.", "Uma nota em que você pode escrever.", None),
    OperationHelp => ("Run commands or set a Record quantity to zero by slug.", "Execute comandos ou zere a quantidade de um Record pelo slug.", None),
    TimeCastleHelp => ("A quiet clock for upcoming work. Unfold the timeline or open its scheduling and stopwatch controls.", "Um relógio discreto para o trabalho futuro. Expanda a linha do tempo ou abra os controles de agendamento e cronômetro.", None),
    AccessControlHelp => ("Manage local users, Roles and permissions.", "Gerencie usuários locais, Roles e permissões.", None),
    VisibilityHelp => ("Compose Organ and proximity rules for Record data, saved places, and live locations.", "Componha regras de Organ e proximidade para dados de Records, lugares salvos e localizações ao vivo.", None),
    TrustHelp => ("Review signed contact disclosures and permitted paths for transfer evidence analysis.", "Revise divulgações assinadas de contatos e caminhos permitidos para analisar evidências de transferências.", None),
    HandCameraHelp => ("Move the 2D camera with an open hand using your webcam.", "Mova a câmera 2D com a mão aberta usando a webcam.", None),
    VideoHelp => ("Watch live memory-only Signals and play GIF videos.", "Assista a Signals ao vivo mantidos apenas na memória e reproduza vídeos GIF.", None),
    SyncHelp => ("Sync a Protein to a directory as .lingua or Markdown.", "Sincronize um Protein com uma pasta como .lingua ou Markdown.", None),
    TerminalHelp => ("A local shell powered by libghostty.", "Um shell local com libghostty.", None),
    ConfigurationHelp => ("Cell identity, discovery, storage, and contacts.", "Identidade da Cell, descoberta, armazenamento e contatos.", None),
    OrganHelp => ("Pairing, contacts, nearby chat, discovery, devices, sharing and mail.", "Pareamento, contatos, conversa por proximidade, descoberta, dispositivos, compartilhamento e mensagens.", None),
    OntologyHelp => ("Manage Linguas, concepts, hierarchy and Record assertions.", "Gerencie Linguas, conceitos, hierarquia e afirmações de Records.", None),
    TodoHelp => ("A live task queue with completion, undo, and saved Proteins.", "Uma fila de tarefas ao vivo com conclusão, desfazer e Proteins salvos.", None),
    Recenter => ("Recenter", "Centralizar", None),
    BringHere => ("Bring Sands here", "Trazer Sands para cá", None),
    ZoomOut => ("Zoom out", "Diminuir zoom", None),
    ResetZoom => ("Reset zoom to 100%", "Redefinir zoom para 100%", None),
    ZoomIn => ("Zoom in", "Aumentar zoom", None),
    Notifications => ("Notifications", "Notificações", None),
    NoNotifications => ("No notifications yet.", "Ainda não há notificações.", None),
    DeleteAll => ("Delete all", "Excluir tudo", None),
    Dismiss => ("Dismiss", "Dispensar", None),
    DismissAll => ("Dismiss all", "Dispensar tudo", None),
    Details => ("Details", "Detalhes", None),
    Source => ("Source", "Origem", None),
    Version => ("Version", "Versão", None),
    Commit => ("Commit", "Commit", None),
    Directory => ("Directory", "Pasta", None),
    CheckUpdates => ("Check for updates", "Verificar atualizações", None),
    DownloadRestart => ("Download and restart", "Baixar e reiniciar", None),
    Restart => ("Restart", "Reiniciar", None),
    Laboratory => ("Laboratory", "Laboratório", None),
    OpenInformation => ("Open Information", "Abrir informações", None),
    Vocabularies => ("Vocabularies", "Vocabulários", None),
    ConceptHierarchy => ("Concepts and hierarchy", "Conceitos e hierarquia", None),
    RecordAssertions => ("Record assertions", "Afirmações de Records", None),
    TermsTranslations => ("Terms and translations", "Termos e traduções", None),
    RecordHistory => ("Record history", "Histórico de Records", None),
    KnowledgeSources => ("Knowledge sources", "Fontes de conhecimento", None),
    LoadingOntology => ("Loading ontology…", "Carregando ontologia…", None),
    TermName => ("Term or original name", "Termo ou nome original", None),
    LanguageTag => ("Language tag (en, pt-BR, art-x-ithkuil; empty for all languages)", "Código de idioma (en, pt-BR, art-x-ithkuil; vazio para todos)", None),
    DefinitionNew => ("Definition (new Concept or new version)", "Definição (novo Concept ou nova versão)", None),
    CreateMeaning => ("Create a different meaning", "Criar um significado diferente", None),
    ReviseDefinition => ("Save new definition version", "Salvar nova versão da definição", None),
    InspectMeaning => ("Inspect meaning and its uses", "Inspecionar significado e seus usos", None),
    AddAlternative => ("Add alternative", "Adicionar alternativa", None),
    EditTerm => ("Edit selected term", "Editar termo selecionado", None),
    RemoveTerm => ("Remove selected term", "Remover termo selecionado", None),
    PreferredTerm => ("Toggle preferred term", "Alternar termo preferido", None),
    PersonalTerm => ("Toggle personal term", "Alternar termo pessoal", None),
    UseDisplayLanguage => ("Use this vocabulary language", "Usar este idioma do vocabulário", None),
    UseContextWording => ("Use selected wording in this context", "Usar a expressão selecionada neste contexto", None),
    UseEnteredWording => ("Use my entered wording in this context", "Usar minha expressão digitada neste contexto", None),
    HideSourceTerm => ("Hide selected source term", "Ocultar termo de origem selecionado", None),
    ShowSourceTerm => ("Show selected source term", "Mostrar termo de origem selecionado", None),
    OriginalWording => ("Restore original wording", "Restaurar expressão original", None),
    AutomaticWording => ("Use automatic wording", "Usar expressão automática", None),
    FeatureMeanings => ("Feature meanings", "Significados de funcionalidades", None),
    ExportTranslations => ("Export shared translations", "Exportar traduções compartilhadas", None),
    InspectTranslations => ("Inspect translation package", "Inspecionar pacote de traduções", None),
    ImportTranslations => ("Add reviewed translations", "Adicionar traduções revisadas", None),
    TodoFocus => ("Todo focus", "Foco nas tarefas", None),
    SavedProteinQuery => ("Saved Protein name, slug or uid (blank uses the focus queue)", "Nome, slug ou uid de um Protein salvo (em branco usa a fila de foco)", None),
    UseProtein => ("Use Protein", "Usar Protein", None),
    ShowIdsOff => ("Show ids: off", "Mostrar identificadores: desativado", None),
    ShowIdsOn => ("Show ids: on", "Mostrar identificadores: ativado", None),
    UndoCompletion => ("Undo completion", "Desfazer conclusão", None),
    ConnectingQueue => ("Connecting to the focus queue…", "Conectando à fila de foco…", None),
    NewTask => ("New task", "Nova tarefa", None),
    AddTask => ("Add task", "Adicionar tarefa", None),
    Complete => ("Complete", "Concluir", None),
    NoTasks => ("No tasks", "Nenhuma tarefa", None),
    Focus => ("Focus", "Foco", None),
    Workspace => ("Workspace", "Área de trabalho", None),
    Appearance => ("Appearance", "Aparência", None),
    KeyboardShortcuts => ("Shortcuts", "Atalhos", None),
    RestoreDefaults => ("Restore defaults", "Restaurar padrões", None),
    Enabled => ("Enabled", "Ativado", None),
    Disabled => ("Disabled", "Desativado", None),
    Confirm => ("Confirm", "Confirmar", None),
    Password => ("Password", "Senha", None),
    Username => ("Username", "Nome de usuário", None),
    SignIn => ("Sign in", "Entrar", None),
    SignOut => ("Sign out", "Sair", None),
    CreateUser => ("Create user", "Criar usuário", None),
    Users => ("Users", "Usuários", None),
    Roles => ("Roles", "Roles", None),
    Permissions => ("Permissions", "Permissões", None),
    OwnerBackup => ("Owner backup", "Backup do proprietário", None),
    Passphrase => ("Passphrase", "Frase secreta", None),
    ChooseFile => ("Choose file", "Escolher arquivo", None),
    CreateBackup => ("Create backup", "Criar backup", None),
    RestoreBackup => ("Restore backup", "Restaurar backup", None),
    Export => ("Export", "Exportar", None),
    Import => ("Import", "Importar", None),
    Copy => ("Copy", "Copiar", None),
    Paste => ("Paste", "Colar", None),
    Cut => ("Cut", "Recortar", None),
    SelectAll => ("Select all", "Selecionar tudo", None),
    Rename => ("Rename", "Renomear", None),
    Duplicate => ("Duplicate", "Duplicar", None),
    Move => ("Move", "Mover", None),
    Connect => ("Connect", "Conectar", None),
    Disconnect => ("Disconnect", "Desconectar", None),
    Retry => ("Retry", "Tentar novamente", None),
    Ready => ("Ready", "Pronto", None),
    Start => ("Start", "Iniciar", None),
    Stop => ("Stop", "Parar", None),
    Pause => ("Pause", "Pausar", None),
    Resume => ("Resume", "Retomar", None),
}

pub const ITHKUIL_LEXICON: &str = "https://ithkuil.net/New_Ithkuil_Lexicon.pdf";
pub const ITHKUIL_LEXICON_VERSION: &str = "1.0.1.1";
pub const TNIL_LICENSE: &str = include_str!("../licenses/tnil-MIT.txt");

static ACTIVE: OnceLock<&'static Catalog> = OnceLock::new();
static NATIVE: OnceLock<Result<Vec<Arc<lince_ithkuil::Script>>, &'static str>> = OnceLock::new();

#[inline]
pub fn catalog() -> &'static Catalog {
    ACTIVE.get().copied().unwrap_or(&ENGLISH)
}

fn select(selection: &OnceLock<&'static Catalog>, language: Language) -> Result<(), &'static str> {
    let active = selection.get_or_init(|| language.catalog());
    if active.language == language {
        Ok(())
    } else {
        Err("The interface language is already selected; restart Lince to change it")
    }
}

pub fn initialize(language: Language) -> Result<(), &'static str> {
    if ACTIVE.get().is_some() {
        return select(&ACTIVE, language);
    }
    if language == Language::Ithkuil {
        NATIVE
            .get_or_init(|| {
                Token::ALL
                    .iter()
                    .filter_map(|token| {
                        let text = ITHKUIL.get(*token);
                        (text != ENGLISH.get(*token)).then_some(text)
                    })
                    .map(|text| {
                        lince_ithkuil::render(text)
                            .map(Arc::new)
                            .map_err(|_| "A built-in Ithkuil translation cannot be rendered")
                    })
                    .collect()
            })
            .as_ref()
            .map_err(|error| *error)?;
    }
    select(&ACTIVE, language)
}

pub fn native_text(value: &str) -> Option<&'static Arc<lince_ithkuil::Script>> {
    if catalog().language != Language::Ithkuil {
        return None;
    }
    NATIVE
        .get()?
        .as_ref()
        .ok()?
        .iter()
        .find(|script| script.romanization() == value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tnil::gloss::{Gloss, GlossFlags};

    #[test]
    fn every_token_has_english_and_brazilian_portuguese() {
        for token in Token::ALL {
            assert!(!ENGLISH.get(*token).is_empty(), "{token:?}");
            assert!(!PORTUGUESE.get(*token).is_empty(), "{token:?}");
            assert!(!ITHKUIL.get(*token).is_empty(), "{token:?}");
        }
        assert_eq!(PORTUGUESE.get(Token::Save), "Salvar");
        assert_eq!(PORTUGUESE.get(Token::Close), "Fechar");
    }

    #[test]
    fn ithkuil_falls_back_to_the_same_static_english_text() {
        assert!(std::ptr::eq(
            ITHKUIL.get(Token::RestartRequired),
            ENGLISH.get(Token::RestartRequired),
        ));
        assert_ne!(ITHKUIL.get(Token::Name), ENGLISH.get(Token::Name));
    }

    #[test]
    fn ithkuil_terms_parse_with_the_expected_roots_and_specifications() {
        for (token, gloss) in [
            (Token::Name, "S1-dn-OBJ"),
            (Token::Title, "S1-rdn-OBJ"),
            (Token::Description, "S1-fç-CTE"),
            (Token::Message, "S1-m-CTE"),
            (Token::Information, "S1-ţč-CTE"),
            (Token::PlainText, "S1-ň"),
        ] {
            let word: tnil::prelude::Word = ITHKUIL.get(token).parse().unwrap();
            assert_eq!(word.gloss(GlossFlags::NONE), gloss, "{token:?}");
            let reparsed: tnil::prelude::Word = word.to_string().parse().unwrap();
            assert_eq!(word, reparsed);
        }
    }

    #[test]
    fn selection_is_immutable_and_lookups_reuse_static_storage() {
        let selection = OnceLock::new();
        select(&selection, Language::PortugueseBrazil).unwrap();
        let active = *selection.get().unwrap();
        let first = active.get(Token::Save);
        for _ in 0..100_000 {
            assert!(std::ptr::eq(first, active.get(Token::Save)));
        }
        select(&selection, Language::PortugueseBrazil).unwrap();
        assert!(select(&selection, Language::Ithkuil).is_err());
        assert_eq!(
            selection.get().unwrap().language,
            Language::PortugueseBrazil
        );
    }

    #[test]
    fn language_tags_are_explicit_and_case_insensitive() {
        for language in Language::ALL {
            assert_eq!(Language::from_tag(language.tag()), Some(language));
        }
        assert_eq!(
            Language::from_tag("PT-br"),
            Some(Language::PortugueseBrazil)
        );
        assert_eq!(Language::from_tag("art-x-ITHKUIL"), Some(Language::Ithkuil));
        assert_eq!(Language::from_tag("en"), Some(Language::English));
        assert_eq!(Language::from_tag("unsupported"), None);
    }
}
