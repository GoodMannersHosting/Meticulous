//! DAG (Directed Acyclic Graph) construction and validation.
//!
//! Validates the dependency graph for cycles and unreachable nodes.

use crate::error::{ErrorCode, ParseDiagnostics, ParseError, SourceLocation};
use indexmap::{IndexMap, IndexSet};
use std::collections::VecDeque;

/// A node in the dependency graph.
#[derive(Debug, Clone)]
pub struct DagNode {
    /// Node identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Dependencies (IDs of nodes that must complete first).
    pub depends_on: Vec<String>,
    /// Source location for error reporting.
    pub source: SourceLocation,
}

/// Validated DAG with topological ordering.
#[derive(Debug)]
pub struct ValidatedDag {
    /// Nodes in topological order.
    pub order: Vec<String>,
    /// Adjacency list (node -> dependents).
    pub adjacency: IndexMap<String, Vec<String>>,
    /// Reverse adjacency (node -> dependencies).
    pub reverse_adjacency: IndexMap<String, Vec<String>>,
}

impl ValidatedDag {
    /// Get nodes with no dependencies (entry points).
    pub fn entry_nodes(&self) -> impl Iterator<Item = &String> {
        self.order.iter().filter(|id| {
            self.reverse_adjacency
                .get(*id)
                .map_or(true, |deps| deps.is_empty())
        })
    }

    /// Get nodes that depend on the given node.
    pub fn dependents(&self, id: &str) -> impl Iterator<Item = &String> {
        self.adjacency
            .get(id)
            .into_iter()
            .flat_map(|deps| deps.iter())
    }

    /// Get dependencies of the given node.
    pub fn dependencies(&self, id: &str) -> impl Iterator<Item = &String> {
        self.reverse_adjacency
            .get(id)
            .into_iter()
            .flat_map(|deps| deps.iter())
    }
}

/// Build and validate a DAG from nodes.
pub fn build_dag(nodes: &[DagNode], diagnostics: &mut ParseDiagnostics) -> Option<ValidatedDag> {
    let mut node_map: IndexMap<&str, &DagNode> = IndexMap::new();
    let mut adjacency: IndexMap<String, Vec<String>> = IndexMap::new();
    let mut reverse_adjacency: IndexMap<String, Vec<String>> = IndexMap::new();

    // Check for duplicate IDs and build node map
    for node in nodes {
        if node_map.contains_key(node.id.as_str()) {
            diagnostics.push(
                ParseError::new(
                    ErrorCode::E2005,
                    format!("duplicate workflow/job ID: {}", node.id),
                )
                .with_source(node.source.clone())
                .with_hint("each ID must be unique within the pipeline"),
            );
        } else {
            node_map.insert(&node.id, node);
            adjacency.insert(node.id.clone(), Vec::new());
            reverse_adjacency.insert(node.id.clone(), Vec::new());
        }
    }

    // Validate dependencies and build adjacency lists
    for node in nodes {
        for dep in &node.depends_on {
            // Check for self-dependency
            if dep == &node.id {
                diagnostics.push(
                    ParseError::new(
                        ErrorCode::E5003,
                        format!("self-dependency not allowed: {}", node.id),
                    )
                    .with_source(node.source.clone()),
                );
                continue;
            }

            // Check if dependency exists
            if !node_map.contains_key(dep.as_str()) {
                diagnostics.push(
                    ParseError::new(
                        ErrorCode::E5002,
                        format!("unknown dependency '{}' referenced by '{}'", dep, node.id),
                    )
                    .with_source(node.source.clone())
                    .with_hint(format!(
                        "available IDs: {}",
                        node_map
                            .keys()
                            .take(5)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                );
                continue;
            }

            // Add edges
            adjacency
                .entry(dep.clone())
                .or_default()
                .push(node.id.clone());
            reverse_adjacency
                .entry(node.id.clone())
                .or_default()
                .push(dep.clone());
        }
    }

    // Don't continue if there are errors so far
    if diagnostics.has_errors() {
        return None;
    }

    // Detect cycles using Kahn's algorithm
    let order = match topological_sort(&adjacency, &reverse_adjacency) {
        Ok(order) => order,
        Err(cycle) => {
            diagnostics.push(
                ParseError::new(
                    ErrorCode::E5001,
                    format!("cycle detected in dependency graph: {}", cycle.join(" -> ")),
                )
                .with_hint("remove circular dependencies to fix this"),
            );
            return None;
        }
    };

    // Check for unreachable nodes (optional warning)
    let reachable = find_reachable(&adjacency, &reverse_adjacency);
    for node in nodes {
        if !reachable.contains(&node.id) {
            diagnostics.push(
                ParseError::warning(
                    ErrorCode::E5004,
                    format!("node '{}' is unreachable from entry points", node.id),
                )
                .with_source(node.source.clone()),
            );
        }
    }

    Some(ValidatedDag {
        order,
        adjacency,
        reverse_adjacency,
    })
}

/// Perform topological sort using Kahn's algorithm.
/// Returns Err with a cycle path if a cycle is detected.
fn topological_sort(
    adjacency: &IndexMap<String, Vec<String>>,
    reverse_adjacency: &IndexMap<String, Vec<String>>,
) -> Result<Vec<String>, Vec<String>> {
    let mut in_degree: IndexMap<String, usize> = IndexMap::new();
    for id in adjacency.keys() {
        in_degree.insert(
            id.clone(),
            reverse_adjacency.get(id).map_or(0, |deps| deps.len()),
        );
    }

    // Queue of nodes with no incoming edges
    let mut queue: VecDeque<String> = in_degree
        .iter()
        .filter(|&(_, deg)| *deg == 0)
        .map(|(id, _)| id.clone())
        .collect();

    let mut result = Vec::new();

    while let Some(node) = queue.pop_front() {
        result.push(node.clone());

        if let Some(dependents) = adjacency.get(&node) {
            for dependent in dependents {
                if let Some(deg) = in_degree.get_mut(dependent) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(dependent.clone());
                    }
                }
            }
        }
    }

    // If not all nodes are in the result, there's a cycle
    if result.len() != adjacency.len() {
        let cycle = find_cycle(adjacency, reverse_adjacency);
        return Err(cycle);
    }

    Ok(result)
}

/// Find a cycle in the graph (for error reporting).
fn find_cycle(
    adjacency: &IndexMap<String, Vec<String>>,
    reverse_adjacency: &IndexMap<String, Vec<String>>,
) -> Vec<String> {
    let mut visited: IndexSet<String> = IndexSet::new();
    let mut rec_stack: IndexSet<String> = IndexSet::new();
    let mut cycle_path: Vec<String> = Vec::new();

    fn dfs(
        node: &str,
        adjacency: &IndexMap<String, Vec<String>>,
        visited: &mut IndexSet<String>,
        rec_stack: &mut IndexSet<String>,
        path: &mut Vec<String>,
    ) -> bool {
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());
        path.push(node.to_string());

        if let Some(deps) = adjacency.get(node) {
            for dep in deps {
                if !visited.contains(dep) {
                    if dfs(dep, adjacency, visited, rec_stack, path) {
                        return true;
                    }
                } else if rec_stack.contains(dep) {
                    path.push(dep.clone());
                    return true;
                }
            }
        }

        rec_stack.shift_remove(node);
        path.pop();
        false
    }

    // Use reverse adjacency to find cycle in the direction of dependencies
    for node in reverse_adjacency.keys() {
        if !visited.contains(node) {
            if dfs(
                node,
                reverse_adjacency,
                &mut visited,
                &mut rec_stack,
                &mut cycle_path,
            ) {
                // Trim to just the cycle
                if let Some(start) = cycle_path.last() {
                    if let Some(pos) = cycle_path.iter().position(|n| n == start) {
                        if pos < cycle_path.len() - 1 {
                            return cycle_path[pos..].to_vec();
                        }
                    }
                }
                return cycle_path;
            }
        }
    }

    vec!["unknown".to_string()]
}

/// Find all nodes reachable from entry points.
fn find_reachable(
    adjacency: &IndexMap<String, Vec<String>>,
    reverse_adjacency: &IndexMap<String, Vec<String>>,
) -> IndexSet<String> {
    let mut reachable = IndexSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();

    // Start from nodes with no dependencies (entry points)
    for (id, deps) in reverse_adjacency {
        if deps.is_empty() {
            queue.push_back(id.clone());
            reachable.insert(id.clone());
        }
    }

    // BFS to find all reachable nodes
    while let Some(node) = queue.pop_front() {
        if let Some(dependents) = adjacency.get(&node) {
            for dependent in dependents {
                if !reachable.contains(dependent) {
                    reachable.insert(dependent.clone());
                    queue.push_back(dependent.clone());
                }
            }
        }
    }

    reachable
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(id: &str, depends_on: Vec<&str>) -> DagNode {
        DagNode {
            id: id.to_string(),
            name: id.to_string(),
            depends_on: depends_on.into_iter().map(String::from).collect(),
            source: SourceLocation::new(1, 1),
        }
    }

    #[test]
    fn test_valid_dag() {
        let nodes = vec![
            make_node("a", vec![]),
            make_node("b", vec!["a"]),
            make_node("c", vec!["a"]),
            make_node("d", vec!["b", "c"]),
        ];

        let mut diag = ParseDiagnostics::new();
        let dag = build_dag(&nodes, &mut diag);

        assert!(!diag.has_errors());
        let dag = dag.unwrap();
        assert_eq!(dag.order.len(), 4);
        assert!(dag.order.iter().position(|x| x == "a") < dag.order.iter().position(|x| x == "b"));
        assert!(dag.order.iter().position(|x| x == "a") < dag.order.iter().position(|x| x == "c"));
        assert!(dag.order.iter().position(|x| x == "b") < dag.order.iter().position(|x| x == "d"));
    }

    #[test]
    fn test_cycle_detection() {
        let nodes = vec![
            make_node("a", vec!["c"]),
            make_node("b", vec!["a"]),
            make_node("c", vec!["b"]),
        ];

        let mut diag = ParseDiagnostics::new();
        let dag = build_dag(&nodes, &mut diag);

        assert!(diag.has_errors());
        assert!(dag.is_none());
        assert!(diag.all()[0].message.contains("cycle"));
    }

    #[test]
    fn test_self_dependency() {
        let nodes = vec![make_node("a", vec!["a"])];

        let mut diag = ParseDiagnostics::new();
        build_dag(&nodes, &mut diag);

        assert!(diag.has_errors());
        assert!(diag.all()[0].message.contains("self-dependency"));
    }

    #[test]
    fn test_unknown_dependency() {
        let nodes = vec![make_node("a", vec!["nonexistent"])];

        let mut diag = ParseDiagnostics::new();
        build_dag(&nodes, &mut diag);

        assert!(diag.has_errors());
        assert!(diag.all()[0].message.contains("unknown dependency"));
    }

    #[test]
    fn test_duplicate_id() {
        let nodes = vec![make_node("a", vec![]), make_node("a", vec![])];

        let mut diag = ParseDiagnostics::new();
        build_dag(&nodes, &mut diag);

        assert!(diag.has_errors());
        assert!(diag.all()[0].message.contains("duplicate"));
    }
}
