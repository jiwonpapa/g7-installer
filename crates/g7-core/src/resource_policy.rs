//! Resource preservation policy for destructive recovery commands.
//!
//! Reset and rollback are intentionally aggressive for installer-owned site
//! resources, but certificate tooling is preserved because Debian certbot purge
//! can delete `/etc/letsencrypt`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Package,
    Service,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceDisposition {
    DeleteAllowed,
    PreserveRequired,
    ConditionalPreserve,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceMatcher {
    Exact(&'static str),
    Prefix(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourcePolicy {
    pub kind: ResourceKind,
    pub matcher: ResourceMatcher,
    pub disposition: ResourceDisposition,
    pub reason: &'static str,
}

pub const RECOVERY_RESOURCE_POLICIES: &[ResourcePolicy] = &[
    ResourcePolicy {
        kind: ResourceKind::Package,
        matcher: ResourceMatcher::Exact("certbot"),
        disposition: ResourceDisposition::PreserveRequired,
        reason: "Debian certbot purge can remove /etc/letsencrypt lineage files.",
    },
    ResourcePolicy {
        kind: ResourceKind::Package,
        matcher: ResourceMatcher::Exact("letsencrypt"),
        disposition: ResourceDisposition::PreserveRequired,
        reason: "Legacy letsencrypt package names can own certificate tooling.",
    },
    ResourcePolicy {
        kind: ResourceKind::Package,
        matcher: ResourceMatcher::Prefix("python3-certbot"),
        disposition: ResourceDisposition::PreserveRequired,
        reason: "Certbot plugin package purge can remove shared certificate tooling.",
    },
    ResourcePolicy {
        kind: ResourceKind::Service,
        matcher: ResourceMatcher::Exact("certbot.timer"),
        disposition: ResourceDisposition::PreserveRequired,
        reason: "Certificate auto-renewal should survive reinstall reset.",
    },
];

pub fn resource_disposition(kind: ResourceKind, name: &str) -> ResourceDisposition {
    RECOVERY_RESOURCE_POLICIES
        .iter()
        .find(|policy| policy.kind == kind && policy.matcher.matches(name))
        .map(|policy| policy.disposition)
        .unwrap_or(ResourceDisposition::DeleteAllowed)
}

impl ResourceMatcher {
    fn matches(self, name: &str) -> bool {
        match self {
            Self::Exact(value) => name == value,
            Self::Prefix(value) => name.starts_with(value),
        }
    }
}

pub fn is_certbot_package(package: &str) -> bool {
    resource_disposition(ResourceKind::Package, package) == ResourceDisposition::PreserveRequired
}

pub fn is_certbot_timer(service: &str) -> bool {
    resource_disposition(ResourceKind::Service, service) == ResourceDisposition::PreserveRequired
}

pub fn preserve_package_on_reset(package: &str) -> bool {
    is_certbot_package(package)
}

pub fn preserve_service_on_reset(service: &str) -> bool {
    is_certbot_timer(service)
}

#[cfg(test)]
mod tests {
    use super::{
        RECOVERY_RESOURCE_POLICIES, ResourceDisposition, ResourceKind, is_certbot_package,
        is_certbot_timer, preserve_package_on_reset, preserve_service_on_reset,
        resource_disposition,
    };

    #[test]
    fn certbot_packages_are_preserved_by_policy_table() {
        assert!(
            RECOVERY_RESOURCE_POLICIES
                .iter()
                .any(|policy| policy.kind == ResourceKind::Package
                    && policy.disposition == ResourceDisposition::PreserveRequired)
        );
        assert!(is_certbot_package("certbot"));
        assert!(is_certbot_package("letsencrypt"));
        assert!(is_certbot_package("python3-certbot-nginx"));
        assert!(preserve_package_on_reset("python3-certbot"));
        assert_eq!(
            resource_disposition(ResourceKind::Package, "nginx"),
            ResourceDisposition::DeleteAllowed
        );
        assert!(!preserve_package_on_reset("nginx"));
    }

    #[test]
    fn certbot_timer_is_preserved() {
        assert!(is_certbot_timer("certbot.timer"));
        assert!(preserve_service_on_reset("certbot.timer"));
        assert!(!preserve_service_on_reset("nginx"));
    }
}
