use assistant_contracts::*;

/// Validate imported data before it enters retrieval. Remote schemas cannot resolve URLs.
pub fn register(store: &dyn Store, capability: &Capability) -> Result<()> {
    if capability.id().is_empty()
        || capability.id().len() > 200
        || capability.description().len() > 4000
    {
        return Err(Error::InvalidInput);
    }
    match capability {
        Capability::Tool(tool) => {
            if tool.version.is_empty()
                || tool.input_schema.to_string().len() > 24_000
                || has_external_reference(&tool.input_schema)
            {
                return Err(Error::InvalidInput);
            }
            jsonschema::validator_for(&tool.input_schema).map_err(|_| Error::InvalidInput)?;
            if let Some(output) = &tool.output_schema {
                if output.to_string().len() > 24_000 || has_external_reference(output) {
                    return Err(Error::InvalidInput);
                }
                jsonschema::validator_for(output).map_err(|_| Error::InvalidInput)?;
            }
        }
        Capability::Skill(skill) => {
            if skill.version.is_empty()
                || skill.instructions.len() > 8_000
                || skill.tool_requirements.len() > 8
            {
                return Err(Error::InvalidInput);
            }
        }
    }
    store.put_capability(capability)
}

fn has_external_reference(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(v) => v.iter().any(|(key, value)| {
            (matches!(key.as_str(), "$ref" | "$dynamicRef")
                && !value.as_str().is_some_and(|s| s.starts_with('#')))
                || has_external_reference(value)
        }),
        serde_json::Value::Array(v) => v.iter().any(has_external_reference),
        _ => false,
    }
}

pub fn validate_call(tool: &ToolSpec, call: &ToolCall) -> Result<()> {
    if !tool.enabled || tool.id != call.tool_id || tool.version != call.version {
        return Err(Error::Denied);
    }
    let validator =
        jsonschema::validator_for(&tool.input_schema).map_err(|_| Error::InvalidInput)?;
    if !validator.is_valid(&call.arguments) {
        return Err(Error::InvalidInput);
    }
    Ok(())
}

pub fn activate(store: &dyn Store, id: &str) -> Result<SkillSpec> {
    if id.starts_with("personal:") {
        return super::personalization::PersonalizationService::new(store).skill(id);
    }
    let Capability::Skill(skill) = store.capability(id)? else {
        return Err(Error::InvalidInput);
    };
    if !skill.enabled {
        return Err(Error::Denied);
    }
    for id in &skill.tool_requirements {
        match store.capability(id)? {
            Capability::Tool(tool) if tool.enabled => (),
            _ => return Err(Error::Unavailable),
        }
    }
    Ok(skill)
}
