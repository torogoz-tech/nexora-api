#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum UnknownMode {
    Policy,
    Fail,
    Allow,
}
impl UnknownMode {
    pub fn override_value(self) -> Option<bool> {
        match self {
            Self::Policy => None,
            Self::Fail => Some(true),
            Self::Allow => Some(false),
        }
    }
}
