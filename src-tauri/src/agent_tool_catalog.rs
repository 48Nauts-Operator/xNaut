// ABOUTME: Bounds each chat request while keeping every permitted tool discoverable.
// ABOUTME: Catalog operations expose schemas only; they never execute project work.
use serde_json::{json, Value};
use std::collections::HashSet;

pub const TOOL_LIMIT: usize = 128;
const SEARCH: &str = "xnaut_search_tools";
const LOAD: &str = "xnaut_load_tools";

pub fn name(tool: &Value) -> &str {
    tool.pointer("/function/name")
        .and_then(Value::as_str)
        .unwrap_or("")
}

pub fn is_catalog_call(name: &str) -> bool {
    matches!(name, SEARCH | LOAD)
}

pub struct ToolCatalog {
    all: Vec<Value>,
    pinned: Vec<Value>,
    selected: Vec<Value>,
    overflow: bool,
}

impl ToolCatalog {
    // Inputs have already been filtered to this turn's permitted capabilities.
    pub fn new(native: Vec<Value>, plugins: Vec<Value>) -> Self {
        let mut seen = HashSet::new();
        let mut unique = |tool: &Value| {
            let key = name(tool);
            !key.is_empty() && !is_catalog_call(key) && seen.insert(key.to_string())
        };
        let mut all: Vec<_> = native.into_iter().filter(&mut unique).collect();
        let native_len = all.len();
        all.extend(plugins.into_iter().filter(&mut unique));
        let overflow = all.len() > TOOL_LIMIT;
        // Leave at least one loadable slot, even if native tools grow past 125.
        let pinned = if overflow {
            all.iter()
                .take(native_len.min(TOOL_LIMIT - 3))
                .cloned()
                .collect()
        } else {
            all.clone()
        };
        Self {
            all,
            pinned,
            selected: Vec::new(),
            overflow,
        }
    }

    pub fn is_deferred(&self) -> bool {
        self.overflow
    }

    pub fn specs(&self) -> Vec<Value> {
        let mut specs = self.pinned.clone();
        if self.overflow {
            specs.extend(self.selected.iter().cloned());
            specs.push(json!({"type":"function","function":{
                "name":SEARCH,
                "description":"Find available tools by words in their name or description. Empty query lists all tools, with pagination. Use xnaut_load_tools with exact names before calling a tool that is not currently loaded. This only discovers tools; it does not execute work.",
                "parameters":{"type":"object","properties":{"query":{"type":"string"},"offset":{"type":"integer","minimum":0}},"required":["query"]}
            }}));
            specs.push(json!({"type":"function","function":{
                "name":LOAD,
                "description":"Load exact tool names for the NEXT assistant response. Replaces previously loaded optional tools; core tools stay available. Returns original schemas. Do not call a newly loaded tool in the same batch. Loading tools does not execute work.",
                "parameters":{"type":"object","properties":{"names":{"type":"array","items":{"type":"string"},"minItems":1}},"required":["names"]}
            }}));
        }
        debug_assert!(specs.len() <= TOOL_LIMIT);
        specs
    }

    pub fn handle(&mut self, tool: &str, args: &Value) -> Value {
        if !self.overflow {
            return json!({"ok":false,"error":"Tool discovery is not active for this turn"});
        }
        match tool {
            SEARCH => {
                let Some(query) = args["query"].as_str() else {
                    return json!({"ok":false,"error":"query must be a string"});
                };
                let terms: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
                let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
                let mut matches: Vec<_> = self
                    .all
                    .iter()
                    .filter_map(|tool| {
                        let haystack = format!(
                            "{} {}",
                            name(tool),
                            tool["function"]["description"].as_str().unwrap_or("")
                        )
                        .to_lowercase();
                        let score = terms
                            .iter()
                            .filter(|term| haystack.contains(term.as_str()))
                            .count();
                        if terms.is_empty() || score > 0 {
                            Some((score, tool))
                        } else {
                            None
                        }
                    })
                    .collect();
                matches.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| name(a.1).cmp(name(b.1))));
                let results: Vec<_> = matches.iter().skip(offset).take(20).map(|(_,tool)| json!({"name":name(tool),"description":tool["function"]["description"]})).collect();
                let next = offset.saturating_add(results.len());
                json!({"ok":true,"tools":results,"total":matches.len(),"next_offset":if next < matches.len() { Some(next) } else { None },"instruction":"Load exact names with xnaut_load_tools, then call those tools in a subsequent response. Discovery has not executed any work."})
            }
            LOAD => {
                let Some(names) = args["names"].as_array().filter(|a| !a.is_empty()) else {
                    return json!({"ok":false,"error":"names must be a non-empty array of exact tool names"});
                };
                let mut selected = Vec::new();
                let mut schemas = Vec::new();
                let mut seen = HashSet::new();
                for requested in names {
                    let Some(requested) = requested.as_str() else {
                        return json!({"ok":false,"error":"Every tool name must be a string"});
                    };
                    let Some(schema) = self.all.iter().find(|t| name(t) == requested) else {
                        return json!({"ok":false,"error":format!("Tool is not available to this turn: {requested}")});
                    };
                    if !seen.insert(requested) {
                        continue;
                    }
                    schemas.push(schema.clone());
                    if !self.pinned.iter().any(|t| name(t) == requested) {
                        selected.push(schema.clone());
                    }
                }
                let capacity = TOOL_LIMIT - 2 - self.pinned.len();
                if selected.len() > capacity {
                    return json!({"ok":false,"error":format!("Load at most {capacity} optional tools at once; use several batches for larger workflows")});
                }
                self.selected = selected;
                json!({"ok":true,"schemas":schemas,"instruction":"These schemas are available in your NEXT response. Previously loaded optional tools were replaced. No work has executed; now call the required tools."})
            }
            _ => json!({"ok":false,"error":"Unknown catalog operation"}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixtures(count: usize) -> Vec<Value> {
        (0..count).map(|n| json!({"type":"function","function":{"name":format!("plugin__tool_{n:03}"),"description":format!("Capability {n}"),"parameters":{"type":"object","properties":{"id":{"type":"integer"}},"required":["id"]}}})).collect()
    }
    #[test]
    fn requests_stay_bounded_at_and_beyond_provider_limit() {
        for count in [0, 42, 128, 129, 155, 500] {
            let all = fixtures(count);
            let catalog = ToolCatalog::new(
                all.iter().take(42).cloned().collect(),
                all.iter().skip(42).cloned().collect(),
            );
            assert!(catalog.specs().len() <= 128);
            if count <= 128 {
                assert_eq!(catalog.specs(), all);
            }
            assert_eq!(catalog.is_deferred(), count > 128);
        }
    }
    #[test]
    fn every_tool_is_discoverable_and_loadable_with_original_schema() {
        let all = fixtures(500);
        let mut catalog = ToolCatalog::new(all[..42].to_vec(), all[42..].to_vec());
        let mut found = Vec::new();
        let mut offset = 0;
        loop {
            let page = catalog.handle(SEARCH, &json!({"query":"","offset":offset}));
            found.extend(
                page["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|t| t["name"].as_str().unwrap().to_string()),
            );
            if let Some(next) = page["next_offset"].as_u64() {
                offset = next;
            } else {
                break;
            }
        }
        assert_eq!(found.len(), 500);
        for (schema, found) in all.iter().zip(found) {
            assert_eq!(name(schema), found);
            assert_eq!(catalog.handle(LOAD, &json!({"names":[found]}))["ok"], true);
            assert!(catalog.specs().contains(schema));
            assert!(catalog.specs().len() <= 128);
        }
    }
    #[test]
    fn invalid_or_oversized_load_cannot_change_active_tools() {
        let all = fixtures(155);
        let mut catalog = ToolCatalog::new(all[..42].to_vec(), all[42..].to_vec());
        catalog.handle(LOAD, &json!({"names":["plugin__tool_154"]}));
        let before = catalog.specs();
        for args in [
            json!({"names":["plugin__tool_150","forbidden_write"]}),
            json!({"names":[3]}),
            json!({"names":[]}),
            json!({"names": all[42..].iter().map(name).collect::<Vec<_>>()}),
        ] {
            assert_eq!(catalog.handle(LOAD, &args)["ok"], false);
            assert_eq!(catalog.specs(), before);
        }
    }
    #[test]
    fn loading_replaces_optional_tools_but_preserves_core_and_round_snapshot() {
        let all = fixtures(155);
        let mut catalog = ToolCatalog::new(all[..42].to_vec(), all[42..].to_vec());
        let advertised = catalog.specs();
        catalog.handle(
            LOAD,
            &json!({"names":["plugin__tool_154","plugin__tool_154"]}),
        );
        assert!(!advertised.iter().any(|t| name(t) == "plugin__tool_154"));
        assert!(catalog.specs().contains(&all[154]));
        catalog.handle(LOAD, &json!({"names":["plugin__tool_153"]}));
        assert!(!catalog.specs().contains(&all[154]));
        assert!(all[..42].iter().all(|t| catalog.specs().contains(t)));
    }
    #[test]
    fn large_native_catalog_keeps_a_loadable_slot() {
        let mut catalog = ToolCatalog::new(fixtures(155), vec![]);
        assert_eq!(
            catalog.handle(LOAD, &json!({"names":["plugin__tool_154"]}))["ok"],
            true
        );
        assert_eq!(catalog.specs().len(), 128);
    }
    #[test]
    fn search_is_ranked_and_permissions_are_not_expanded() {
        let mut catalog = ToolCatalog::new(vec![], fixtures(155));
        assert_eq!(
            catalog.handle(SEARCH, &json!({"query":"Capability 154"}))["tools"][0]["name"],
            "plugin__tool_154"
        );
        assert_eq!(
            catalog.handle(SEARCH, &json!({"query":"forbidden_write"}))["total"],
            0
        );
        assert_eq!(
            catalog.handle(LOAD, &json!({"names":["forbidden_write"]}))["ok"],
            false
        );
        assert!(is_catalog_call(SEARCH));
        assert!(is_catalog_call(LOAD));
        assert!(!is_catalog_call("plugin__tool_154"));
    }
}
