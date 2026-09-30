export type Tone = 'neutral' | 'working' | 'waiting' | 'done' | 'error';
export interface Snapshot {
  version: 1;
  title: string;
  status: string;
  tone: Tone;
  body: string;
  records: string;
  diagnostics: string;
  feedback: string;
  primary: string;
  primaryEnabled: boolean;
  stop: string;
  shortcut: string;
}

const tones: ReadonlySet<string> = new Set(['neutral', 'working', 'waiting', 'done', 'error']);
export function parseSnapshot(value: unknown): Snapshot | null {
  if (value === null || typeof value !== 'object') return null;
  const data = value as Record<string, unknown>;
  if (data.version !== 1 || typeof data.primaryEnabled !== 'boolean' || !tones.has(String(data.tone))) return null;
  for (const key of ['title', 'status', 'body', 'records', 'diagnostics', 'feedback', 'primary', 'stop', 'shortcut']) {
    if (typeof data[key] !== 'string' || (data[key] as string).length > 262144) return null;
  }
  return data as unknown as Snapshot;
}

export function summaryLines(body: string): string[] {
  return body.split(/\r?\n/).filter(line => line.trim().length > 0);
}
