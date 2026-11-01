/* Some model APIs reject legal boolean property schemas emitted by MCP servers. */
function objectFormSchema(schema) {
    if (schema === true) return {};
    if (schema === false) return { not: {} };
    if (!schema || typeof schema !== 'object' || Array.isArray(schema)) return schema;
    const result = { ...schema };
    for (const key of ['properties', 'patternProperties', '$defs', 'definitions', 'dependentSchemas']) {
        if (schema[key] && typeof schema[key] === 'object') {
            result[key] = Object.fromEntries(Object.entries(schema[key])
                .map(([name, value]) => [name, objectFormSchema(value)]));
        }
    }
    for (const key of ['items', 'contains', 'propertyNames', 'not', 'if', 'then', 'else']) {
        if (key in schema) result[key] = Array.isArray(schema[key])
            ? schema[key].map(objectFormSchema) : objectFormSchema(schema[key]);
    }
    for (const key of ['allOf', 'anyOf', 'oneOf', 'prefixItems']) {
        if (Array.isArray(schema[key])) result[key] = schema[key].map(objectFormSchema);
    }
    // These keywords allow booleans directly in model APIs. Recurse into their
    // object form, preserving false (closed objects) and true as written.
    for (const key of ['additionalProperties', 'unevaluatedProperties', 'additionalItems', 'unevaluatedItems']) {
        if (schema[key] && typeof schema[key] === 'object') result[key] = objectFormSchema(schema[key]);
    }
    return result;
}

const schemaContext = SillyTavern.getContext();
schemaContext.eventSource.on(schemaContext.eventTypes.CHAT_COMPLETION_SETTINGS_READY, payload => {
    for (const tool of payload.tools || []) {
        if (tool.type === 'function' && tool.function?.parameters !== undefined) {
            tool.function.parameters = objectFormSchema(tool.function.parameters);
        }
    }
});
