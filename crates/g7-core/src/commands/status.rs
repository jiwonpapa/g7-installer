#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallerStatus {
    pub installed: bool,
    pub components: Vec<ComponentStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentStatus {
    pub name: &'static str,
    pub state: &'static str,
}

pub fn read() -> InstallerStatus {
    InstallerStatus {
        installed: false,
        components: vec![
            ComponentStatus {
                name: "g7",
                state: "not installed",
            },
            ComponentStatus {
                name: "nginx",
                state: "unknown",
            },
            ComponentStatus {
                name: "database",
                state: "unknown",
            },
        ],
    }
}
