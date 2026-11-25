#[derive(Clone, Debug, PartialEq)]
pub enum ScrollDirection {
    Down,
    Up,
    Top,
    End,
    Ref(String),
}

/// Tab actions for the agent DSL. `Favorite`/`Unfavorite` are aliases of
/// `Pin`/`Unpin`: favorites and pins are the same sidebar section.
#[derive(Clone, Debug, PartialEq)]
pub enum TabAction {
    New { url: Option<String> },
    Select(String),
    Close { target: Option<String> },
    Pin { target: Option<String> },
    Unpin { target: Option<String> },
    Favorite { target: Option<String> },
    Unfavorite { target: Option<String> },
}

#[derive(Clone, Debug, PartialEq)]
pub enum SplitAction {
    WithNext,
    WithUrl(String),
    Pair(String, String),
    Exit,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WaitCondition {
    Idle,
    Stable(u64),
    Stream { quiescent_ms: u64, timeout_ms: u64 },
    Millis(u64),
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum DragTarget {
    Ref(String),
    Coords(i32, i32),
}

impl DragTarget {
    pub(super) fn into_pair(self, other: Self) -> (Self, Self) {
        (self, other)
    }
}


#[derive(Clone, Debug, PartialEq)]
pub enum DblClickTarget {
    Target(String),
    Coords(i32, i32),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ActionOp {
    Click {
        ref_id: String,
    },
    ForceClick {
        ref_id: String,
    },
    DomClick {
        ref_id: String,
    },
    DblClick {
        target: DblClickTarget,
    },
    SemanticClick {
        kind: char,
        label: String,
    },
    ClearAndType {
        ref_id: String,
        text: String,
    },
    AppendText {
        ref_id: String,
        text: String,
    },
    TypeActive {
        text: String,
    },
    TypeSecret {
        ref_id: String,
        secret_name: String,
    },
    SemanticType {
        kind: char,
        label: String,
        text: String,
    },
    SetValue {
        target: String,
        value: String,
        enter: bool,
    },
    Paste {
        ref_id: Option<String>,
        text: Option<String>,
    },
    Key {
        chord: String,
    },
    KeyDown {
        chord: String,
    },
    KeyUp {
        chord: String,
    },
    Submit,
    Go {
        url: String,
    },
    Back,
    Fwd,
    Reload,
    Scroll {
        direction: ScrollDirection,
    },
    Hover {
        ref_id: String,
    },
    Focus {
        ref_id: String,
    },
    Drag {
        from: DragTarget,
        to: DragTarget,
    },
    XY {
        x: i32,
        y: i32,
    },
    Upload {
        ref_id: String,
        file_paths: Vec<String>,
    },
    Dialog {
        accept: bool,
    },
    Tab {
        action: TabAction,
    },
    Space {
        target: String,
    },
    Split {
        action: SplitAction,
    },
    Wait {
        condition: WaitCondition,
    },
}

