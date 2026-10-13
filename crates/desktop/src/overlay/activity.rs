#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutOfSight {
    Behind,
    Minimized,
    OnDesktop(Option<u32>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cue {
    Click(u8),
    Scroll,
    Drag,
    Typing,
    Keys(Vec<Vec<String>>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    pub app: String,
    pub out_of_sight: OutOfSight,
    pub cue: Option<Cue>,
}
