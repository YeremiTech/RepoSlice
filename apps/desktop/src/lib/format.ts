export function normalizeDisplayPath(path: string): string {
  if (path.startsWith("\\\\?\\UNC\\")) {
    return `\\\\${path.slice(8)}`;
  }
  if (path.startsWith("\\\\?\\")) {
    return path.slice(4);
  }
  return path;
}

export function compactPath(path: string, maxLength = 82): string {
  const normalized = normalizeDisplayPath(path);
  if (normalized.length <= maxLength) {
    return normalized;
  }

  const head = normalized.slice(0, Math.floor(maxLength * 0.38));
  const tail = normalized.slice(-Math.floor(maxLength * 0.52));
  return `${head}…${tail}`;
}

export function relativeProjectPath(path: string, root: string): string {
  const normalizedPath = normalizeDisplayPath(path).replaceAll("\\", "/");
  const normalizedRoot = normalizeDisplayPath(root).replaceAll("\\", "/").replace(/\/$/, "");
  const pathLower = normalizedPath.toLowerCase();
  const rootLower = normalizedRoot.toLowerCase();

  if (normalizedRoot && (pathLower === rootLower || pathLower.startsWith(`${rootLower}/`))) {
    const relative = normalizedPath.slice(normalizedRoot.length).replace(/^\/+/, "");
    return relative || ".";
  }

  const repositoriesMarker = "/.reposlice/repositories/";
  const markerIndex = pathLower.indexOf(repositoriesMarker);
  if (markerIndex >= 0) {
    const rest = normalizedPath.slice(markerIndex + repositoriesMarker.length);
    const slash = rest.indexOf("/");
    return slash >= 0 ? rest.slice(slash + 1) || "." : rest;
  }

  return normalizedPath;
}

export function extractName(id: string): string {
  const segments = id.split(":");
  return segments[segments.length - 1] || id;
}

export function compatibilityLabel(level: string): string {
  const labels: Record<string, string> = {
    L0: "Filesystem",
    L1: "Syntax",
    L2: "Semantic",
    L3: "Framework",
    L4: "Runtime",
    L5: "Capsule"
  };

  return labels[level] ?? "Unknown";
}

export function componentTone(kind: string): string {
  const tones: Record<string, string> = {
    controller: "cyan",
    service: "violet",
    repository: "green",
    entity: "emerald",
    class: "blue",
    interface: "amber",
    record: "pink",
    enum: "orange",
    module: "cyan",
    function: "violet",
    file: "slate",
    trait: "pink",
    middleware: "amber",
    command: "orange",
    job: "cyan",
    event: "pink",
    listener: "violet",
    model: "emerald",
    migration: "amber",
    provider: "blue",
    "frontend-component": "cyan",
    directive: "violet",
    pipe: "pink",
    guard: "amber",
    view: "green",
    handler: "cyan"
  };

  return tones[kind.trim().toLowerCase()] ?? "slate";
}

export function httpMethod(name: string): string {
  return name.trim().split(" ")[0]?.toUpperCase() ?? "HTTP";
}

export function methodTone(method: string): string {
  const tones: Record<string, string> = {
    GET: "green",
    POST: "cyan",
    PUT: "amber",
    PATCH: "violet",
    DELETE: "red",
    HEAD: "blue",
    OPTIONS: "pink",
    CONNECT: "emerald",
    TRACE: "orange",
    ANY: "pink",
    ROUTE: "violet"
  };

  return tones[method.trim().toUpperCase()] ?? "slate";
}

export function formatDuration(milliseconds: number): string {
  if (!Number.isFinite(milliseconds) || milliseconds < 0) return "—";
  if (milliseconds < 1000) return `${Math.round(milliseconds)} ms`;
  if (milliseconds < 60_000) return `${(milliseconds / 1000).toFixed(milliseconds < 10_000 ? 2 : 1)} s`;
  const minutes = Math.floor(milliseconds / 60_000);
  const seconds = Math.round((milliseconds % 60_000) / 1000);
  return `${minutes} min ${seconds} s`;
}

export function formatDateTime(milliseconds: number): string {
  if (!milliseconds) return "—";
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "short",
    timeStyle: "medium"
  }).format(new Date(milliseconds));
}
