import { useEffect, useMemo, useRef, useState } from "react";
import { getProviderCatalog } from "./service";
import type {
  AuthOptionSpec,
  ConfigFieldSpec,
  ProviderCatalogPayload,
  ProviderDefinition,
  ProviderProfile,
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
  stored,
  error,
  onChange,
}: {
  field: ConfigFieldSpec;
  value: string;
  secret: boolean;
  stored: boolean;
  error?: string;
  onChange: (value: string) => void;
}) {
  const helpId = field.help_text ? `${field.id}-help` : undefined;
  const errorId = error ? `${field.id}-error` : undefined;
  const describedBy = [helpId, errorId].filter(Boolean).join(" ") || undefined;
  const required = field.required && !(secret && stored);
  if (field.kind === "select") {
    return (
      <label>
        {field.label}
        <select
          value={value}
          required={required}
          aria-label={field.label}
          aria-invalid={error ? true : undefined}
          aria-describedby={describedBy}
          onChange={(e) => onChange(e.target.value)}
        >
          <option value="">Select…</option>
          {field.options.map((option) => (
            <option key={option} value={option}>
              {option}
            </option>
          ))}
        </select>
        {field.help_text && <small id={helpId}>{field.help_text}</small>}
        {error && (
          <small id={errorId} role="alert" className="field-error">
            {error}
          </small>
        )}
      </label>
    );
  }
  if (field.kind === "boolean") {
    return (
      <label>
        <input
          type="checkbox"
          checked={value === "true"}
          aria-label={field.label}
          aria-invalid={error ? true : undefined}
          aria-describedby={describedBy}
          onChange={(e) => onChange(e.target.checked ? "true" : "false")}
        />
        {field.label}
        {field.help_text && <small id={helpId}>{field.help_text}</small>}
        {error && (
          <small id={errorId} role="alert" className="field-error">
            {error}
          </small>
        )}
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
              : field.kind === "url"
                ? "url"
                : "text"
        }
        value={value}
        required={required}
        aria-label={field.label}
        aria-invalid={error ? true : undefined}
        aria-describedby={describedBy}
        autoComplete="off"
        onChange={(e) => onChange(e.target.value)}
      />
      {field.help_text && <small id={helpId}>{field.help_text}</small>}
      {error && (
        <small id={errorId} role="alert" className="field-error">
          {error}
        </small>
      )}
      {secret && stored && <small>Leave blank to keep the stored key.</small>}
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

function profileValues(profile: ProviderProfile): Record<string, string> {
  return Object.fromEntries(
    Object.entries(profile.non_secret_config).map(([key, value]) => [
      key,
      String(value ?? ""),
    ]),
  );
}

function validateField(
  field: ConfigFieldSpec,
  value: string,
  stored: boolean,
): string | null {
  const required = field.required && !(field.secret && stored);
  if (required && !value.trim()) return `${field.label} is required.`;
  if (!value) return null;
  const validation = field.validation;
  if (validation?.min_length != null && value.length < validation.min_length) {
    return `${field.label} is too short.`;
  }
  if (validation?.max_length != null && value.length > validation.max_length) {
    return `${field.label} is too long.`;
  }
  if (validation?.pattern) {
    try {
      if (!new RegExp(validation.pattern).test(value)) {
        return `${field.label} has an invalid format.`;
      }
    } catch {
      return `${field.label} could not be validated.`;
    }
  }
  if (
    field.kind === "integer" ||
    validation?.min_value != null ||
    validation?.max_value != null
  ) {
    const numeric = Number(value);
    if (!Number.isFinite(numeric)) return `${field.label} must be a number.`;
    if (validation?.min_value != null && numeric < validation.min_value) {
      return `${field.label} is below the minimum.`;
    }
    if (validation?.max_value != null && numeric > validation.max_value) {
      return `${field.label} is above the maximum.`;
    }
  }
  return null;
}

export function ProviderCatalogSettings({
  loadCatalog = getProviderCatalog,
  initialProfile,
  hasStoredSecret = false,
  onSave,
}: {
  loadCatalog?: () => Promise<ProviderCatalogPayload>;
  initialProfile?: ProviderProfile;
  hasStoredSecret?: boolean;
  onSave: (draft: ProviderProfileDraft) => Promise<boolean>;
}) {
  const [catalog, setCatalog] = useState<ProviderCatalogPayload | null>(null);
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const [catalogLoading, setCatalogLoading] = useState(true);
  const [loadAttempt, setLoadAttempt] = useState(0);
  const [selectedId, setSelectedId] = useState("");
  const [authId, setAuthId] = useState("");
  const [values, setValues] = useState<Record<string, string>>({});
  const [secrets, setSecrets] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({});
  const [saveError, setSaveError] = useState<string | null>(null);
  const [status, setStatus] = useState("");
  const hydratedProfile = useRef("");
  const initialProfileRef = useRef(initialProfile);
  initialProfileRef.current = initialProfile;

  useEffect(() => {
    let active = true;
    setCatalogLoading(true);
    setCatalogError(null);
    void loadCatalog()
      .then((payload) => {
        if (!active) return;
        setCatalog(payload);
        if (!initialProfileRef.current) {
          const first = payload.providers.find(
            (p) => p.availability === "available",
          );
          if (first) {
            setSelectedId(first.id);
            setAuthId(first.auth_options[0]?.id ?? "");
            setValues(defaultValues(first));
            setSecrets({});
          }
        }
      })
      .catch((e: unknown) => {
        if (!active) return;
        setCatalogError(
          e instanceof Error ? e.message : "Provider catalog unavailable.",
        );
      })
      .finally(() => {
        if (active) setCatalogLoading(false);
      });
    return () => {
      active = false;
    };
  }, [loadAttempt, loadCatalog]);

  useEffect(() => {
    if (!catalog || !initialProfile) return;
    const profileKey = `${initialProfile.provider_id}:${initialProfile.auth_option_id}:${initialProfile.updated_at}`;
    if (hydratedProfile.current === profileKey) return;
    const saved = catalog.providers.find(
      (candidate) => candidate.id === initialProfile.provider_id,
    );
    if (!saved) return;
    hydratedProfile.current = profileKey;
    setSelectedId(saved.id);
    setAuthId(initialProfile.auth_option_id);
    setValues({ ...defaultValues(saved), ...profileValues(initialProfile) });
    setSecrets({});
  }, [catalog, initialProfile]);

  const provider: ProviderDefinition | undefined = useMemo(
    () => catalog?.providers.find((p) => p.id === selectedId),
    [catalog, selectedId],
  );
  const auth: AuthOptionSpec | undefined = provider?.auth_options.find(
    (a) => a.id === authId,
  );
  const storedSecretAvailable = Boolean(
    hasStoredSecret &&
    initialProfile &&
    initialProfile.provider_id === provider?.id &&
    initialProfile.auth_option_id === auth?.id,
  );

  const setValue = (id: string, value: string) => {
    setValues((prev) => ({ ...prev, [id]: value }));
    setFieldErrors((prev) => {
      const next = { ...prev };
      delete next[id];
      return next;
    });
  };
  const setSecret = (id: string, value: string) => {
    setSecrets((prev) => ({ ...prev, [id]: value }));
    setFieldErrors((prev) => {
      const next = { ...prev };
      delete next[id];
      return next;
    });
  };

  function chooseProvider(nextId: string) {
    const next = catalog?.providers.find((p) => p.id === nextId);
    setSelectedId(nextId);
    setAuthId(next?.auth_options[0]?.id ?? "");
    setValues(next ? defaultValues(next) : {});
    setSecrets({});
    setFieldErrors({});
    setSaveError(null);
    setStatus("");
  }

  function chooseAuth(nextId: string) {
    const next = provider?.auth_options.find((option) => option.id === nextId);
    const allowed = new Set([
      ...(provider?.endpoint_fields.map((field) => field.id) ?? []),
      ...(next?.fields
        .filter((field) => !field.secret)
        .map((field) => field.id) ?? []),
    ]);
    setAuthId(nextId);
    setValues((prev) =>
      Object.fromEntries(
        Object.entries(prev).filter(([key]) => allowed.has(key)),
      ),
    );
    setSecrets({});
    setFieldErrors({});
    setSaveError(null);
    setStatus("");
  }

  async function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!provider || !auth || saving) return;
    const visibleFields = [
      ...provider.endpoint_fields.filter((field) => isVisible(field, values)),
      ...(auth.fields.filter((field) =>
        isVisible(field, { ...values, ...secrets }),
      ) ?? []),
    ];
    const nextErrors: Record<string, string> = {};
    for (const field of visibleFields) {
      const error = validateField(
        field,
        field.secret ? (secrets[field.id] ?? "") : (values[field.id] ?? ""),
        field.secret && storedSecretAvailable,
      );
      if (error) nextErrors[field.id] = error;
    }
    setFieldErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0) {
      setSaveError("Check the highlighted fields before saving.");
      return;
    }
    setSaving(true);
    setSaveError(null);
    setStatus("");
    try {
      const saved = await onSave({
        provider_id: provider.id,
        auth_option_id: auth.id,
        values,
        secrets,
      });
      if (!saved) return;
      setSecrets({});
      setStatus("Provider saved.");
    } catch (e) {
      setSaveError(e instanceof Error ? e.message : "Could not save provider.");
    } finally {
      setSaving(false);
    }
  }

  if (catalogError) {
    return (
      <div className="settings-form" role="alert">
        <p>{catalogError}</p>
        <button
          className="secondary"
          type="button"
          onClick={() => setLoadAttempt((value) => value + 1)}
        >
          Retry loading providers
        </button>
      </div>
    );
  }
  if (catalogLoading || !catalog) {
    return <p role="status">Loading providers…</p>;
  }
  if (!provider) {
    return <p className="empty-state">No available cloud providers.</p>;
  }

  return (
    <form className="settings-form" onSubmit={submit} noValidate>
      <label>
        Provider
        <select
          value={selectedId}
          disabled={saving}
          onChange={(e) => chooseProvider(e.target.value)}
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
      {provider.endpoint_fields
        .filter((f) => isVisible(f, values))
        .map((f) => (
          <FieldInput
            key={f.id}
            field={f}
            value={values[f.id] ?? ""}
            secret={false}
            stored={false}
            error={fieldErrors[f.id]}
            onChange={(v) => setValue(f.id, v)}
          />
        ))}
      {provider.auth_options.length > 1 && (
        <label>
          Authentication
          <select
            value={authId}
            disabled={saving}
            onChange={(e) => chooseAuth(e.target.value)}
          >
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
            stored={f.secret && storedSecretAvailable}
            error={fieldErrors[f.id]}
            onChange={(v) =>
              f.secret ? setSecret(f.id, v) : setValue(f.id, v)
            }
          />
        ))}
      {status && (
        <p className="field-help" role="status">
          {status}
        </p>
      )}
      {saveError && (
        <p className="inline-error" role="alert">
          {saveError}
        </p>
      )}
      <button className="primary" disabled={saving} type="submit">
        {saving
          ? "Saving…"
          : initialProfile?.provider_id === provider.id
            ? "Update provider"
            : "Save provider"}
      </button>
    </form>
  );
}
