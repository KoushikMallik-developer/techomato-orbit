//! Matching listening endpoints to Orbit's services.

use crate::engine::supervisor::ServiceRuntime;
use crate::models::service::ServiceSpec;
use crate::windows::net::Endpoint;
use serde::Serialize;

/// An endpoint plus which Orbit service (if any) it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortRow {
    #[serde(flatten)]
    pub endpoint: Endpoint,
    pub service_id: Option<String>,
    pub service_name: Option<String>,
    /// A different program holds the port a stopped/failed service is configured to use.
    pub conflicts_with: Option<String>,
}

/// Matches by owner PID (or its children, which covers `cmd /c node` shells), then by configured
/// port. A configured-port match on a running service means the service owns it; on any other
/// status the listener is a conflict.
pub fn annotate(
    endpoints: Vec<Endpoint>,
    runtimes: &[ServiceRuntime],
    specs: &[ServiceSpec],
    children: &dyn Fn(u32) -> Vec<u32>,
) -> Vec<PortRow> {
    // pid -> owning service id, covering the service process and all its descendants.
    let mut owner: std::collections::HashMap<u32, &str> = std::collections::HashMap::new();
    for r in runtimes {
        let Some(root) = r.pid else { continue };
        // The whole process tree: `npm run dev` is cmd -> node -> cmd -> node, and the one that
        // listens is the last. The seen-check also stops a reused PID from looping.
        let mut todo = vec![root];
        while let Some(pid) = todo.pop() {
            if owner.insert(pid, &r.id).is_none() {
                todo.extend(children(pid));
            }
        }
    }
    let name_of = |id: &str| specs.iter().find(|s| s.id == id).map(|s| s.name.clone());
    endpoints
        .into_iter()
        .map(|endpoint| {
            let service_id = owner.get(&endpoint.pid).map(|s| s.to_string());
            let conflicts_with = if service_id.is_some() {
                None
            } else {
                specs
                    .iter()
                    .find(|s| s.port == Some(endpoint.port) && s.enabled)
                    .map(|s| s.name.clone())
            };
            PortRow {
                service_name: service_id.as_deref().and_then(name_of),
                service_id,
                conflicts_with,
                endpoint,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::state::Status;
    use crate::windows::net::Proto;

    fn ep(port: u16, pid: u32) -> Endpoint {
        Endpoint {
            proto: Proto::Tcp,
            local_addr: "127.0.0.1".into(),
            port,
            state: Some("LISTEN".into()),
            pid,
            created_at_ms: Some(1),
            process_name: Some("x.exe".into()),
            path: None,
        }
    }
    fn rt(id: &str, status: Status, pid: Option<u32>) -> ServiceRuntime {
        ServiceRuntime {
            id: id.into(),
            status,
            pid,
            parent_pid: None,
            started_at_ms: None,
            exit_code: None,
            restart_count: 0,
            last_error: None,
        }
    }
    fn spec(id: &str, name: &str, port: Option<u16>) -> ServiceSpec {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": name, "category": "custom", "projectId": null,
            "workingDir": "C:\\", "executable": "x", "args": [], "runThroughShell": false,
            "env": [], "port": port, "startupTimeoutSecs": 5, "shutdownTimeoutSecs": 5,
            "restart": {"kind": "never"}, "health": null, "keepRunningOnQuit": false,
            "enabled": true
        }))
        .unwrap()
    }
    const NONE: &dyn Fn(u32) -> Vec<u32> = &|_| vec![];

    #[test]
    fn an_endpoint_owned_by_a_service_pid_is_attributed_to_it() {
        let rows = annotate(
            vec![ep(3000, 10)],
            &[rt("a", Status::Running, Some(10))],
            &[spec("a", "API", None)],
            NONE,
        );
        assert_eq!(rows[0].service_id.as_deref(), Some("a"));
        assert_eq!(rows[0].service_name.as_deref(), Some("API"));
        assert_eq!(rows[0].conflicts_with, None);
    }

    #[test]
    fn an_endpoint_owned_by_a_child_of_a_service_is_attributed_to_it() {
        let kids = |p: u32| if p == 10 { vec![11] } else { vec![] };
        let rows = annotate(
            vec![ep(3000, 11)],
            &[rt("a", Status::Running, Some(10))],
            &[spec("a", "API", None)],
            &kids,
        );
        assert_eq!(rows[0].service_id.as_deref(), Some("a"));
    }

    #[test]
    fn a_listener_deep_in_the_process_tree_is_attributed_to_the_service() {
        // npm.cmd (10) -> node npm (11) -> cmd (12) -> node vite (13), which listens.
        let kids = |p: u32| match p {
            10 => vec![11],
            11 => vec![12],
            12 => vec![13],
            _ => vec![],
        };
        let rows = annotate(
            vec![ep(5173, 13)],
            &[rt("a", Status::Running, Some(10))],
            &[spec("a", "Web", None)],
            &kids,
        );
        assert_eq!(rows[0].service_id.as_deref(), Some("a"));
    }

    #[test]
    fn a_foreign_listener_on_a_stopped_services_port_is_a_conflict() {
        let rows = annotate(
            vec![ep(8080, 99)],
            &[rt("a", Status::Stopped, None)],
            &[spec("a", "API", Some(8080))],
            NONE,
        );
        assert_eq!(rows[0].service_id, None);
        assert_eq!(rows[0].conflicts_with.as_deref(), Some("API"));
    }

    #[test]
    fn an_unrelated_endpoint_has_no_annotations() {
        let rows = annotate(
            vec![ep(5000, 99)],
            &[rt("a", Status::Running, Some(10))],
            &[spec("a", "API", Some(8080))],
            NONE,
        );
        assert_eq!(rows[0].service_id, None);
        assert_eq!(rows[0].conflicts_with, None);
    }
}
