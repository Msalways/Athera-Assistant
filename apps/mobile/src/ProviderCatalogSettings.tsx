import { useEffect, useMemo, useState } from "react";
import { getProviderCatalog } from "./service";
import type {
  AuthOptionSpec,
  ConfigFieldSpec,
  ProviderCatalogPayload,
  ProviderDefinition,
  ProviderProfileDraft,
} from "./types";

function isVisible(
  field: ConfigFieldSpec,
  values: Record<string, string>,
): boolean {
  return field.visible_when.every(
    (rule) => values[rule.field_id] === rule.equals,
  );
}

function FieldInput({
  field,
  value,
  secret,
  onChange,
}: {
  field: ConfigFieldSpec;
  value: string;
  secret: boolean;
  onChange: (value: string) => void;
}) {
  if (field.kind === "select") {
    return (
      <label>
        {field.label}
        <select
          value={value}
          required={field.required}
          onChange={(e) => onChange(e.target.value)}
        >
          <option value="">Select…</option>
          {field.options.map((option) => (
            <option key={option} value={option}>
              {option}
            </option>
          ))}
        </select>
      </label>
    );
  }
  if (field.kind === "boolean") {
    return (
      <label>
        <input
          type="checkbox"
          checked={value === "true"}
          onChange={(e) => onChange(e.target.checked ? "true" : "false")}
        />
        {field.label}
      </label>
    );
  }
  return (
    <label>
      {field.label}
      <input
        type={
          secret || field.kind === "secret"
            ? "password"
            : field.kind === "integer"
              ? "number"
              : "text"
        }
        value={value}
        required={field.required}
        placeholder={field.help_text ?? undefined}
        autoComplete="off"
        onChange={(e) => onChange(e.target.value)}
      />
    </label>
  );
}

function defaultValues(provider: ProviderDefinition): Record<string, string> {
  if (
    provider.default_base_url &&
    provider.endpoint_fields.some((f) => f.id === "base_url")
  ) {
    return { base_url: provider.default_base_url };
  }
  return {};
}

export function ProviderCatalogSettings({
  loadCatalog = getProviderCatalog,
  onSave,
}: {
  loadCatalog?: () => Promise<ProviderCatalogPayload>;
  onSave: (draft: ProviderProfileDraft) => void;
}) {
  const [catalog, setCatalog] = useState<ProviderCatalogPayload | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState("");
  const [authId, setAuthId] = useState("");
  const [values, setValues] = useState<Record<string, string>>({});
  const [secrets, setSecrets] = useState<Record<string, string>>({});

  useEffect(() => {
    loadCatalog()
      .then((payload) => {
        setCatalog(payload);
        const first = payload.providers.find(
          (p) => p.availability === "available",
        );
        if (first) {
          setSelectedId(first.id);
          setAuthId(first.auth_options[0]?.id ?? "");
          setValues(defaultValues(first));
          setSecrets({});
        }
      })
      .catch((e: unknown) =>
        setError(e instanceof Error ? e.message : "Catalog unavailable"),
      );
  }, [loadCatalog]);

  const provider: ProviderDefinition | undefined = useMemo(
    () => catalog?.providers.find((p) => p.id === selectedId),
    [catalog, selectedId],
  );
  const auth: AuthOptionSpec | undefined = provider?.auth_options.find(
    (a) => a.id === authId,
  );

  if (error) return <p role="alert">{error}</p>;
  if (!catalog || !provider) return <p>Loading providers…</p>;

  const setValue = (id: string, value: string) =>
    setValues((prev) => ({ ...prev, [id]: value }));
  const setSecret = (id: string, value: string) =>
    setSecrets((prev) => ({ ...prev, [id]: value }));

  return (
    <form
      className="settings-form"
      onSubmit={(e) => {
        e.preventDefault();
        onSave({
          provider_id: provider.id,
          auth_option_id: auth?.id ?? "",
          values,
          secrets,
        });
      }}
    >
      <label>
        Provider
        <select
          value={selectedId}
          onChange={(e) => {
            const next = catalog.providers.find((p) => p.id === e.target.value);
            setSelectedId(e.target.value);
            setAuthId(next?.auth_options[0]?.id ?? "");
            setValues(next ? defaultValues(next) : {});
            setSecrets({});
          }}
        >
          {catalog.providers.map((p) => (
            <option
              key={p.id}
              value={p.id}
              disabled={p.availability !== "available"}
            >
              {p.display_name}
              {p.availability !== "available" ? " (unavailable)" : ""}
            </option>
          ))}
        </select>
      </label>
      {provider.endpoint_fields.filter((f) => isVisible(f, values)).map((f) => (
        <FieldInput
          key={f.id}
          field={f}
          value={values[f.id] ?? ""}
          secret={false}
          onChange={(v) => setValue(f.id, v)}
        />
      ))}
      {provider.auth_options.length > 1 && (
        <label>
          Authentication
          <select value={authId} onChange={(e) => setAuthId(e.target.value)}>
            {provider.auth_options.map((a) => (
              <option key={a.id} value={a.id}>
                {a.label}
              </option>
            ))}
          </select>
        </label>
      )}
      {auth?.fields
        .filter((f) => isVisible(f, { ...values, ...secrets }))
        .map((f) => (
          <FieldInput
            key={f.id}
            field={f}
            value={f.secret ? (secrets[f.id] ?? "") : (values[f.id] ?? "")}
            secret={f.secret}
            onChange={(v) => (f.secret ? setSecret(f.id, v) : setValue(f.id, v))}
          />
        ))}
      <button type="submit">Save provider</button>
    </form>
  );
}
