//! The choices Desktop's settings model keeps (`settings/model.tsx`) beyond a switch: each a
//! closed set saved by its name, read back as Desktop's default when missing or unknown.

/// A closed choice saved by name.
pub trait Choice: Copy + PartialEq + Default + 'static {
    const ALL: &'static [Self];
    fn name(self) -> &'static str;
    /// What Settings' select shows, in the current language.
    fn label(self) -> &'static str;

    fn parse(name: Option<&str>) -> Self {
        Self::ALL.iter().copied().find(|c| Some(c.name()) == name).unwrap_or_default()
    }
}

/// Defines a closed settings choice enum implementing [`Choice`] and [`Default`].
#[macro_export]
macro_rules! choice {
    ($(#[$doc:meta])* $ty:ident { $($(#[$vdoc:meta])* $v:ident => $name:literal, $label:literal),+ $(,)? } default $d:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $ty {
            $($(#[$vdoc])* $v,)+
        }

        impl Default for $ty {
            fn default() -> Self { $ty::$d }
        }

        impl $crate::prefs::Choice for $ty {
            const ALL: &'static [Self] = &[$($ty::$v),+];
            fn name(self) -> &'static str { match self { $($ty::$v => $name),+ } }
            fn label(self) -> &'static str { match self { $($ty::$v => opencode_i18n::t!($label)),+ } }
        }
    };
}
pub use crate::choice;

choice! {
    /// Where Ctrl+N and New session start a draft (`workspaces.startIn`): the project on screen
    /// (or the last one looked at), or the server's default directory (`GET /api/location`).
    StartIn { Project => "project", "rune.settings.newSession.startIn.project", Directory => "directory", "rune.settings.newSession.startIn.directory" } default Project
}

choice! {
    /// Which checkout of the current project a new draft uses (`workspaces.worktree`).
    Worktree { Default => "default", "rune.settings.newSession.worktree.default", New => "new", "rune.settings.newSession.worktree.new", LastUsed => "last-used", "rune.settings.newSession.worktree.lastUsed" } default Default
}

impl Worktree {
    /// The setting this replaced (`workspaces.defaultDestination`): only an explicit New
    /// worktree carries over, since Last used was written for everyone as the old default.
    pub fn migrate(old: Option<&str>) -> Worktree {
        match old {
            Some("new") => Worktree::New,
            _ => Worktree::Default,
        }
    }
}

choice! {
    /// What Enter does while the agent works (`general.followUpBehavior`); the alternate chord does
    /// the other.
    FollowUp { Queue => "queue", "settings.general.row.followUpBehavior.queue", Steer => "steer", "settings.general.row.followUpBehavior.steer" } default Steer
}

/// The key that moves a question dock on, or submits it on the last question
/// (`general.questionSubmitKey`, anomalyco/opencode#66169); the other Enter selects an answer or
/// starts a new line in the typed one. Saved by its chord, which Settings shows as its label.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum QuestionSubmitKey {
    #[default]
    Enter,
    ModEnter,
}

impl QuestionSubmitKey {
    pub const ALL: [QuestionSubmitKey; 2] = [QuestionSubmitKey::Enter, QuestionSubmitKey::ModEnter];

    /// The saved name, which is the key's chord too.
    pub fn name(self) -> &'static str {
        match self {
            QuestionSubmitKey::Enter => "enter",
            QuestionSubmitKey::ModEnter => "mod+enter",
        }
    }

    /// Desktop's default when missing or unknown.
    pub fn parse(name: Option<&str>) -> Self {
        Self::ALL.into_iter().find(|c| Some(c.name()) == name).unwrap_or_default()
    }

    /// The Enter that does the other thing, as the row's description names it
    /// (`settings/general/general.tsx:153-155`).
    pub fn other(self) -> &'static str {
        match self {
            QuestionSubmitKey::Enter => "shift+enter",
            QuestionSubmitKey::ModEnter => "enter",
        }
    }
}

choice! {
    /// The timeline's column (`general.timelineWidth`).
    TimelineWidth { Focused => "focused", "settings.timeline.row.width.focused", Comfortable => "comfortable", "settings.timeline.row.width.comfortable", Wide => "wide", "settings.timeline.row.width.wide" } default Comfortable
}

impl TimelineWidth {
    pub fn width(self) -> opencode_ui::style::Width {
        match self {
            TimelineWidth::Focused => opencode_ui::style::Width::Focused,
            TimelineWidth::Comfortable => opencode_ui::style::Width::Comfortable,
            TimelineWidth::Wide => opencode_ui::style::Width::Wide,
        }
    }
}

choice! {
    /// Where running subagents and shells show over the composer (`general.backgroundActivity`).
    BackgroundActivity { Off => "off", "settings.timeline.row.backgroundActivity.off", Left => "left", "settings.timeline.row.backgroundActivity.left", Center => "center", "settings.timeline.row.backgroundActivity.center", Right => "right", "settings.timeline.row.backgroundActivity.right" } default Left
}

choice! {
    /// How side tabs list sessions (`general.sidebarOrganize`, `settings/model.tsx:141`): one
    /// list of tabs, Customize's grouped, sorted and filtered sections, Recent's pinned and recent
    /// sessions in place of tabs, or Consolidated's navigation prototype (pinned sessions & groups,
    /// and Tabs or List). `Projects` is retired (#64727) and omitted from [`Organize::options`];
    /// keeping the variant preserves a saved `"projects"` value until the user picks a new option
    /// (its label, never offered, is Home's `home.projects`: official dropped the old one)
    /// while [`Organize::effective`] resolves it to Consolidated (local/dev) or Customize.
    Organize { List => "list", "settings.experimental.sidebarOrganize.list", Projects => "projects", "home.projects", Customize => "customize", "settings.experimental.sidebarOrganize.customize", Recent => "recent", "settings.experimental.sidebarOrganize.recent", Consolidated => "consolidated", "settings.experimental.sidebarOrganize.consolidated" } default List
}

impl Organize {
    /// What Settings offers (`sidebarOrganizeOptions`, `settings/model.tsx`): Recent and
    /// Consolidated only in local and dev desktop builds (`recentSessionsAvailable`). A PR
    /// build (channel `pr-<n>`) offers what a dev build does: it is an internal build of a
    /// change on its way to `dev`, and it has to be able to show what that change does there.
    /// Official has no such channel. This is Settings' own copy of the channel
    /// (`SettingsConfig`); the updater and the app's folders read theirs and are not asked here.
    pub fn options() -> &'static [Organize] {
        let channel = crate::build_channel();
        if !cfg!(target_arch = "wasm32") && (matches!(channel, "local" | "dev") || channel.starts_with("pr-")) {
            &[Organize::List, Organize::Customize, Organize::Recent, Organize::Consolidated]
        } else {
            &[Organize::List, Organize::Customize]
        }
    }

    /// The effective mode (`resolveSidebarOrganize`, `settings/model.tsx`): retired `Projects`
    /// resolves to Consolidated where offered and Customize elsewhere; any other unsupported value
    /// falls back to One list while keeping the stored value intact on disk.
    pub fn effective(self) -> Organize {
        if self == Organize::Projects {
            return if Organize::options().contains(&Organize::Consolidated) {
                Organize::Consolidated
            } else {
                Organize::Customize
            };
        }
        if Organize::options().contains(&self) { self } else { Organize::default() }
    }
}

choice! {
    /// The navigation prototype's layout (`appearance.navigation`, Settings › Preferences ›
    /// Navigation, #64668): keep active sessions as tabs, or see every session in one list.
    Navigation { Tabs => "tabs", "settings.general.row.navigation.tabs", List => "list", "settings.general.row.navigation.list" } default Tabs
}

choice! {
    /// How Consolidated's open tabs are grouped (`appearance.tabGrouping`, Settings > Preferences >
    /// Group tabs, anomalyco/opencode#66350): not at all, by their project, or by their server.
    TabGrouping { None => "none", "settings.general.row.tabGrouping.none", Project => "project", "settings.general.row.tabGrouping.project", Server => "server", "settings.general.row.tabGrouping.server" } default None
}

/// Only local and dev builds (and internal `pr-*` builds) offer the composer-integrated New
/// question UI (`settings/model.tsx:357`, `opencode#66825`).
pub fn question_composer_available() -> bool {
    let channel = crate::build_channel();
    matches!(channel, "local" | "dev") || channel.starts_with("pr-")
}

impl crate::Settings {
    /// Experimental › New question UI (`general.questionComposer`, `opencode#66825`): offered only
    /// on local and dev builds, defaulting to `default` when unset (true live, false in fixtures).
    pub fn question_composer_with_default(&self, default: bool) -> bool {
        question_composer_available() && self.question_composer.unwrap_or(default)
    }

    /// Experimental › New question UI (`general.questionComposer`, `opencode#66825`): on by default
    /// on local and dev builds.
    pub fn question_composer(&self) -> bool {
        self.question_composer_with_default(true)
    }
}

/// Desktop's sound effects (`shell/notifications/sound.ts`), by id and label key.
pub const SOUNDS: &[(&str, opencode_i18n::Key)] = &[
    ("alert-01", opencode_i18n::key!("sound.option.alert01")), ("alert-02", opencode_i18n::key!("sound.option.alert02")), ("alert-03", opencode_i18n::key!("sound.option.alert03")), ("alert-04", opencode_i18n::key!("sound.option.alert04")), ("alert-05", opencode_i18n::key!("sound.option.alert05")),
    ("alert-06", opencode_i18n::key!("sound.option.alert06")), ("alert-07", opencode_i18n::key!("sound.option.alert07")), ("alert-08", opencode_i18n::key!("sound.option.alert08")), ("alert-09", opencode_i18n::key!("sound.option.alert09")), ("alert-10", opencode_i18n::key!("sound.option.alert10")),
    ("bip-bop-01", opencode_i18n::key!("sound.option.bipbop01")), ("bip-bop-02", opencode_i18n::key!("sound.option.bipbop02")), ("bip-bop-03", opencode_i18n::key!("sound.option.bipbop03")), ("bip-bop-04", opencode_i18n::key!("sound.option.bipbop04")), ("bip-bop-05", opencode_i18n::key!("sound.option.bipbop05")),
    ("bip-bop-06", opencode_i18n::key!("sound.option.bipbop06")), ("bip-bop-07", opencode_i18n::key!("sound.option.bipbop07")), ("bip-bop-08", opencode_i18n::key!("sound.option.bipbop08")), ("bip-bop-09", opencode_i18n::key!("sound.option.bipbop09")), ("bip-bop-10", opencode_i18n::key!("sound.option.bipbop10")),
    ("staplebops-01", opencode_i18n::key!("sound.option.staplebops01")), ("staplebops-02", opencode_i18n::key!("sound.option.staplebops02")), ("staplebops-03", opencode_i18n::key!("sound.option.staplebops03")), ("staplebops-04", opencode_i18n::key!("sound.option.staplebops04")),
    ("staplebops-05", opencode_i18n::key!("sound.option.staplebops05")), ("staplebops-06", opencode_i18n::key!("sound.option.staplebops06")), ("staplebops-07", opencode_i18n::key!("sound.option.staplebops07")),
    ("nope-01", opencode_i18n::key!("sound.option.nope01")), ("nope-02", opencode_i18n::key!("sound.option.nope02")), ("nope-03", opencode_i18n::key!("sound.option.nope03")), ("nope-04", opencode_i18n::key!("sound.option.nope04")), ("nope-05", opencode_i18n::key!("sound.option.nope05")), ("nope-06", opencode_i18n::key!("sound.option.nope06")),
    ("nope-07", opencode_i18n::key!("sound.option.nope07")), ("nope-08", opencode_i18n::key!("sound.option.nope08")), ("nope-09", opencode_i18n::key!("sound.option.nope09")), ("nope-10", opencode_i18n::key!("sound.option.nope10")), ("nope-11", opencode_i18n::key!("sound.option.nope11")), ("nope-12", opencode_i18n::key!("sound.option.nope12")),
    ("yup-01", opencode_i18n::key!("sound.option.yup01")), ("yup-02", opencode_i18n::key!("sound.option.yup02")), ("yup-03", opencode_i18n::key!("sound.option.yup03")), ("yup-04", opencode_i18n::key!("sound.option.yup04")), ("yup-05", opencode_i18n::key!("sound.option.yup05")), ("yup-06", opencode_i18n::key!("sound.option.yup06")),
];

/// What each event plays (`sounds`): a sound id, or `None` for none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sounds {
    pub agent: Option<String>,
    pub permissions: Option<String>,
    pub questions: Option<String>,
    pub errors: Option<String>,
    /// Interface sounds: the wordmark's.
    pub interface: bool,
}

impl Default for Sounds {
    fn default() -> Self {
        let on = |id: &str| Some(id.to_owned());
        Sounds {
            agent: on("staplebops-01"),
            permissions: on("staplebops-02"),
            questions: on("staplebops-05"),
            errors: on("nope-03"),
            interface: true,
        }
    }
}

impl Sounds {
    pub fn to_json(&self) -> serde_json::Value {
        let pair = |s: &Option<String>, d: &str| (s.is_some(), s.clone().unwrap_or_else(|| d.to_owned()));
        let d = Sounds::default();
        let (ae, a) = pair(&self.agent, d.agent.as_deref().unwrap_or_default());
        let (pe, p) = pair(&self.permissions, d.permissions.as_deref().unwrap_or_default());
        let (qe, q) = pair(&self.questions, d.questions.as_deref().unwrap_or_default());
        let (ee, e) = pair(&self.errors, d.errors.as_deref().unwrap_or_default());
        serde_json::json!({
            "agentEnabled": ae, "agent": a, "permissionsEnabled": pe, "permissions": p,
            "questionsEnabled": qe, "questions": q, "errorsEnabled": ee, "errors": e, "interfaceEnabled": self.interface,
        })
    }

    /// Desktop's shape: each event's `<kind>Enabled` and its sound id.
    pub fn from_json(v: &serde_json::Value) -> Sounds {
        let d = Sounds::default();
        let read = |kind: &str, d: &Option<String>| {
            let on = v[format!("{kind}Enabled")].as_bool().unwrap_or(true);
            on.then(|| v[kind].as_str().filter(|id| SOUNDS.iter().any(|(s, _)| s == id)).map(str::to_owned).or_else(|| d.clone())).flatten()
        };
        Sounds {
            agent: read("agent", &d.agent),
            permissions: read("permissions", &d.permissions),
            questions: read("questions", &d.questions),
            errors: read("errors", &d.errors),
            interface: v["interfaceEnabled"].as_bool().unwrap_or(true),
        }
    }
}

choice! {
    /// Where a narrow window keeps its titlebar and view switcher (`general.mobileTitlebarPosition`).
    TitlebarPosition { Top => "top", "settings.general.row.mobileTitlebarBottom.title", Bottom => "bottom", "settings.general.row.mobileTitlebarBottom.title" } default Top
}
