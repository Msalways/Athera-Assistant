use assistant_contracts::*;

pub fn build(store: &dyn Store, task: &Task, config: &EngineConfig) -> Result<ContextBundle> {
    let candidates = store.search(&task.search_query, config.candidate_limit)?;
    let mut tools = Vec::new();
    let mut skills = Vec::new();
    for pinned in &task.active_skills {
        let current = super::registry::activate(store, &pinned.id)?;
        if &current != pinned {
            return Err(Error::Conflict);
        }
        for id in &current.tool_requirements {
            if let Capability::Tool(tool) = store.capability(id)? {
                if !tools.iter().any(|t: &ToolSpec| t.id == tool.id) {
                    tools.push(tool);
                }
            }
        }
        skills.push(current);
    }
    for candidate in &candidates {
        if tools.len() >= config.tool_limit.min(8) {
            break;
        }
        if let Capability::Tool(tool) = store.capability(&candidate.id)? {
            if tool.enabled && !tools.iter().any(|t| t.id == tool.id) {
                tools.push(tool);
            }
        }
    }
    if tools.len() > config.tool_limit.min(8) {
        return Err(Error::InvalidInput);
    }
    let history = store
        .tasks()?
        .into_iter()
        .filter(|t| {
            t.input.conversation_id == task.input.conversation_id
                && t.id != task.id
                && t.status == TaskStatus::Completed
        })
        .take(3)
        .map(|t| {
            format!(
                "User: {}\nAssistant: {}",
                clip(&t.input.text, 1000),
                clip(&t.message, 1500)
            )
        })
        .collect();
    let results = task
        .result_refs
        .iter()
        .rev()
        .take(3)
        .map(|id| {
            let value = store.result(*id)?;
            let visible = if value.get("schema").and_then(serde_json::Value::as_str)
                == Some(TOOL_RESULT_SCHEMA_V1)
            {
                let record: ToolResultRecord =
                    serde_json::from_value(value).map_err(|_| Error::InvalidResponse)?;
                record.validate()?;
                record.model_context.to_string()
            } else {
                value.to_string()
            };
            Ok(ResultExcerpt {
                id: *id,
                untrusted_data: clip(&visible, 8_000),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut context = ContextBundle {
        task_id: task.id,
        role: task.role,
        goal: task.input.text.clone(),
        plan: task.plan.clone(),
        handoff: task.handoff.clone(),
        history,
        results,
        skills,
        candidates,
        tools,
    };
    let budget = if task.role == Role::Fast {
        config.fast_context_bytes
    } else {
        config.cloud_context_bytes
    };
    while serde_json::to_vec(&context)
        .map_err(|_| Error::InvalidInput)?
        .len()
        > budget
    {
        if !context.history.is_empty() {
            context.history.pop();
        } else if !context.results.is_empty() {
            context.results.pop();
        } else if !context.candidates.is_empty() {
            context.candidates.pop();
        } else if context.tools.len() > 1 && context.skills.is_empty() {
            context.tools.pop();
        } else {
            return Err(Error::InvalidInput);
        }
    }
    Ok(context)
}

pub fn clip(value: &str, bytes: usize) -> String {
    if value.len() <= bytes {
        return value.to_string();
    }
    let mut end = bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{} [truncated]", &value[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use storage_sqlite::SqliteStore;

    #[test]
    fn raw_tool_payload_never_enters_model_context() {
        let store = SqliteStore::memory().unwrap();
        let mut task = Task::new(UserInput {
            conversation_id: Id::new_v4(),
            text: "research".into(),
            source: InputSource::Text,
        });
        let id = Id::new_v4();
        store
            .save_result(
                id,
                &json!({
                    "schema":TOOL_RESULT_SCHEMA_V1,
                    "source":"web.search",
                    "model_context":{"sources":[{"id":"source","untrusted_excerpt":"fact"}]},
                    "raw":{"provider_secret_debug":"must stay out"}
                }),
            )
            .unwrap();
        task.result_refs.push(id);
        let context = build(&store, &task, &EngineConfig::default()).unwrap();
        assert!(context.results[0].untrusted_data.contains("fact"));
        assert!(!context.results[0]
            .untrusted_data
            .contains("provider_secret_debug"));
    }
}
