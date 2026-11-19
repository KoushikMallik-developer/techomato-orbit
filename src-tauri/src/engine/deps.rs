//! Start order from service dependencies (pure).

use std::collections::{HashMap, HashSet};

/// A dependency cycle as the ids along it, first id repeated at the end (`[a, b, a]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cycle(pub Vec<String>);

/// `nodes` is `(id, ids it depends on)`. Returns start levels: level 0 depends on nothing, and
/// everything in a level may start together once the earlier levels are up. Dependencies on ids
/// that are not in `nodes` are ignored.
pub fn order(nodes: &[(String, Vec<String>)]) -> Result<Vec<Vec<String>>, Cycle> {
    let known: HashSet<&str> = nodes.iter().map(|(id, _)| id.as_str()).collect();
    let deps: HashMap<&str, Vec<&str>> = nodes
        .iter()
        .map(|(id, d)| {
            let d = d
                .iter()
                .map(String::as_str)
                .filter(|x| known.contains(x))
                .collect();
            (id.as_str(), d)
        })
        .collect();
    let mut placed: HashSet<&str> = HashSet::new();
    let mut levels: Vec<Vec<String>> = Vec::new();
    while placed.len() < known.len() {
        let level: Vec<&str> = nodes
            .iter()
            .map(|(id, _)| id.as_str())
            .filter(|id| !placed.contains(id) && deps[id].iter().all(|d| placed.contains(d)))
            .collect();
        if level.is_empty() {
            return Err(find_cycle(nodes, &deps, &placed));
        }
        placed.extend(level.iter().copied());
        levels.push(level.into_iter().map(String::from).collect());
    }
    Ok(levels)
}

/// Every unplaced node has an unplaced dependency, so walking those edges must loop.
fn find_cycle(
    nodes: &[(String, Vec<String>)],
    deps: &HashMap<&str, Vec<&str>>,
    placed: &HashSet<&str>,
) -> Cycle {
    let start = nodes
        .iter()
        .map(|(id, _)| id.as_str())
        .find(|id| !placed.contains(id))
        .expect("a cycle needs an unplaced node");
    let mut path = vec![start];
    loop {
        let current = *path.last().expect("path is never empty");
        let next = deps[current]
            .iter()
            .copied()
            .find(|d| !placed.contains(d))
            .expect("an unplaced node has an unplaced dependency");
        if let Some(pos) = path.iter().position(|p| *p == next) {
            let mut cycle: Vec<String> = path[pos..].iter().map(|s| s.to_string()).collect();
            cycle.push(next.to_string());
            return Cycle(cycle);
        }
        path.push(next);
    }
}

/// Stop order: dependants first, so the reverse of the start levels.
pub fn stop_order(levels: &[Vec<String>]) -> Vec<Vec<String>> {
    levels.iter().rev().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(id: &str, deps: &[&str]) -> (String, Vec<String>) {
        (id.into(), deps.iter().map(|d| d.to_string()).collect())
    }
    fn lv(levels: &[&[&str]]) -> Vec<Vec<String>> {
        levels
            .iter()
            .map(|l| l.iter().map(|s| s.to_string()).collect())
            .collect()
    }

    #[test]
    fn independent_nodes_share_one_level() {
        assert_eq!(
            order(&[n("x", &[]), n("y", &[]), n("z", &[])]),
            Ok(lv(&[&["x", "y", "z"]]))
        );
    }

    #[test]
    fn a_chain_has_one_node_per_level_dependencies_first() {
        let got = order(&[n("c", &["b"]), n("b", &["a"]), n("a", &[])]);
        assert_eq!(got, Ok(lv(&[&["a"], &["b"], &["c"]])));
    }

    #[test]
    fn a_diamond_starts_the_middle_together() {
        let got = order(&[
            n("d", &["b", "c"]),
            n("b", &["a"]),
            n("c", &["a"]),
            n("a", &[]),
        ]);
        assert_eq!(got, Ok(lv(&[&["a"], &["b", "c"], &["d"]])));
    }

    #[test]
    fn edges_across_projects_are_ordinary_edges() {
        let got = order(&[n("web/ui", &["api/server"]), n("api/server", &[])]);
        assert_eq!(got, Ok(lv(&[&["api/server"], &["web/ui"]])));
    }

    #[test]
    fn dependencies_on_unknown_ids_are_ignored() {
        assert_eq!(order(&[n("a", &["gone"])]), Ok(lv(&[&["a"]])));
    }

    #[test]
    fn a_self_dependency_is_a_cycle() {
        assert_eq!(
            order(&[n("a", &["a"])]),
            Err(Cycle(vec!["a".into(), "a".into()]))
        );
    }

    #[test]
    fn a_two_cycle_is_reported_with_its_members() {
        let Err(Cycle(path)) = order(&[n("a", &["b"]), n("b", &["a"])]) else {
            panic!("expected a cycle");
        };
        assert_eq!(path.len(), 3);
        assert_eq!(path.first(), path.last());
        assert!(path.contains(&"a".to_string()) && path.contains(&"b".to_string()));
    }

    #[test]
    fn a_three_cycle_is_found_even_with_innocent_nodes_around_it() {
        let Err(Cycle(path)) = order(&[
            n("ok", &[]),
            n("a", &["b"]),
            n("b", &["c"]),
            n("c", &["a", "ok"]),
            n("after", &["a"]),
        ]) else {
            panic!("expected a cycle");
        };
        assert_eq!(path.first(), path.last());
        assert_eq!(path.len(), 4);
        assert!(!path.contains(&"ok".to_string()) && !path.contains(&"after".to_string()));
    }

    #[test]
    fn stopping_reverses_the_start_levels() {
        let start = lv(&[&["a"], &["b", "c"], &["d"]]);
        assert_eq!(stop_order(&start), lv(&[&["d"], &["b", "c"], &["a"]]));
    }
}
