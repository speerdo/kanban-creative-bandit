export type Health = {
  status: 'ok' | 'degraded';
  version: string;
  db: 'ok' | 'error';
};

async function get<T>(path: string): Promise<T> {
  const res = await fetch(`/api${path}`, { headers: { Accept: 'application/json' } });
  if (!res.ok && res.status !== 503) {
    throw new Error(`${res.status} ${res.statusText}`);
  }
  return res.json() as Promise<T>;
}

export const api = {
  health: () => get<Health>('/health'),
};
