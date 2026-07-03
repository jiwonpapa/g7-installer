#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateKind {
    NginxSite,
    QueueService,
    ReverbService,
}

impl TemplateKind {
    pub fn path(self) -> &'static str {
        match self {
            Self::NginxSite => "templates/nginx/g7.conf.tera",
            Self::QueueService => "templates/systemd/g7-queue.service.tera",
            Self::ReverbService => "templates/systemd/g7-reverb.service.tera",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TemplateKind;

    #[test]
    fn template_paths_match_spec_layout() {
        assert_eq!(
            TemplateKind::NginxSite.path(),
            "templates/nginx/g7.conf.tera"
        );
    }
}
