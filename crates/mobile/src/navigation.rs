use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Page {
    Organ,
    #[default]
    Records,
    Record(String),
    Kanban,
    Karma,
    Frequency,
    Credits,
}

impl Page {
    pub const MENU: [Self; 6] = [
        Self::Records,
        Self::Kanban,
        Self::Organ,
        Self::Karma,
        Self::Frequency,
        Self::Credits,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            Self::Organ => "Organ",
            Self::Records => "Records",
            Self::Record(_) => "Record",
            Self::Kanban => "Kanban",
            Self::Karma => "Karma",
            Self::Frequency => "Frequency",
            Self::Credits => "Licenses and credits",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Navigation {
    pub current: Page,
    history: Vec<Page>,
    pub menu_open: bool,
}

impl Navigation {
    pub fn open(&mut self, page: Page) {
        self.menu_open = false;
        if page == self.current {
            return;
        }
        if self.history.len() == 32 {
            self.history.remove(0);
        }
        self.history
            .push(std::mem::replace(&mut self.current, page));
    }

    pub fn back(&mut self) -> bool {
        if self.menu_open {
            self.menu_open = false;
            return true;
        }
        match self.history.pop() {
            Some(page) => {
                self.current = page;
                true
            }
            None => false,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
