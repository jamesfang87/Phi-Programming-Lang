// TODO: why do we need a whole new module for this????

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Debug,
    Release,
}
