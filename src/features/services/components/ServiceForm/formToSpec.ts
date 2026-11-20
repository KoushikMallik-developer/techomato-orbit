import type { Category, ServiceSpec } from '../../api/services.schema';

export type EnvRow = { key: string; value: string; isSecret: boolean };

/** Everything is a string here because that is what inputs hold; `formToSpec` does the parsing. */
export type ServiceFormValues = {
  id: string;
  name: string;
  description: string;
  category: Category;
  projectId: string | null;
  executable: string;
  argsText: string;
  workingDir: string;
  runThroughShell: boolean;
  env: EnvRow[];
  port: string;
  startupTimeoutSecs: string;
  shutdownTimeoutSecs: string;
  restartKind: 'never' | 'onFailure' | 'always';
  maxRetries: string;
  backoffSecs: string;
  healthKind: 'none' | 'tcp' | 'http' | 'command';
  healthPort: string;
  healthUrl: string;
  healthMin: string;
  healthMax: string;
  healthExe: string;
  healthArgsText: string;
  healthInterval: string;
  healthTimeout: string;
  keepRunningOnQuit: boolean;
  enabled: boolean;
};

export function defaultFormValues(): ServiceFormValues {
  return {
    id: '',
    name: '',
    description: '',
    category: 'custom',
    projectId: null,
    executable: '',
    argsText: '',
    workingDir: '',
    runThroughShell: false,
    env: [],
    port: '',
    startupTimeoutSecs: '30',
    shutdownTimeoutSecs: '10',
    restartKind: 'never',
    maxRetries: '3',
    backoffSecs: '2',
    healthKind: 'none',
    healthPort: '',
    healthUrl: '',
    healthMin: '200',
    healthMax: '399',
    healthExe: '',
    healthArgsText: '',
    healthInterval: '10',
    healthTimeout: '3',
    keepRunningOnQuit: false,
    enabled: true,
  };
}

/** One argument per non-empty line; quotes are kept literally (no shell parsing). */
const lines = (text: string) => text.split(/\r?\n/).filter((l) => l.trim() !== '');

function int(label: string, raw: string): number {
  const t = raw.trim();
  if (!/^\d+$/.test(t)) throw new Error(`${label} must be a whole number.`);
  return Number(t);
}

export function formToSpec(f: ServiceFormValues): ServiceSpec {
  const env: ServiceSpec['env'] = [];
  for (const r of f.env) {
    const key = r.key.trim();
    if (key === '') {
      if (r.isSecret) throw new Error('A secret variable needs a key.');
      continue;
    }
    // An empty secret value means "leave the stored one unchanged".
    env.push({ key, value: r.isSecret && r.value === '' ? null : r.value, isSecret: r.isSecret });
  }

  const restart: ServiceSpec['restart'] =
    f.restartKind === 'never'
      ? { kind: 'never' }
      : {
          kind: f.restartKind,
          maxRetries: int('Max retries', f.maxRetries),
          backoffSecs: int('Backoff', f.backoffSecs),
        };

  const timing = () => ({
    intervalSecs: int('Health interval', f.healthInterval),
    timeoutSecs: int('Health timeout', f.healthTimeout),
  });
  let health: ServiceSpec['health'] = null;
  if (f.healthKind === 'tcp') {
    health = { kind: 'tcp', port: int('Health port', f.healthPort), ...timing() };
  } else if (f.healthKind === 'http') {
    health = {
      kind: 'http',
      url: f.healthUrl.trim(),
      minStatus: int('Lowest status', f.healthMin),
      maxStatus: int('Highest status', f.healthMax),
      ...timing(),
    };
  } else if (f.healthKind === 'command') {
    health = {
      kind: 'command',
      exe: f.healthExe.trim(),
      args: lines(f.healthArgsText),
      ...timing(),
    };
  }

  return {
    id: f.id,
    name: f.name.trim(),
    description: f.description,
    category: f.category,
    projectId: f.projectId,
    workingDir: f.workingDir.trim(),
    executable: f.executable.trim(),
    args: lines(f.argsText),
    runThroughShell: f.runThroughShell,
    env,
    port: f.port.trim() === '' ? null : int('Port', f.port),
    startupTimeoutSecs: int('Startup timeout', f.startupTimeoutSecs),
    shutdownTimeoutSecs: int('Shutdown timeout', f.shutdownTimeoutSecs),
    restart,
    health,
    keepRunningOnQuit: f.keepRunningOnQuit,
    enabled: f.enabled,
  };
}

export function specToForm(s: ServiceSpec): ServiceFormValues {
  const d = defaultFormValues();
  const h = s.health;
  return {
    ...d,
    id: s.id,
    name: s.name,
    description: s.description,
    category: s.category,
    projectId: s.projectId,
    executable: s.executable,
    argsText: s.args.join('\n'),
    workingDir: s.workingDir,
    runThroughShell: s.runThroughShell,
    env: s.env.map((e) => ({ key: e.key, value: e.value ?? '', isSecret: e.isSecret })),
    port: s.port === null ? '' : String(s.port),
    startupTimeoutSecs: String(s.startupTimeoutSecs),
    shutdownTimeoutSecs: String(s.shutdownTimeoutSecs),
    restartKind: s.restart.kind,
    maxRetries: s.restart.kind === 'never' ? d.maxRetries : String(s.restart.maxRetries),
    backoffSecs: s.restart.kind === 'never' ? d.backoffSecs : String(s.restart.backoffSecs),
    healthKind: h ? h.kind : 'none',
    healthPort: h?.kind === 'tcp' ? String(h.port) : '',
    healthUrl: h?.kind === 'http' ? h.url : '',
    healthMin: h?.kind === 'http' ? String(h.minStatus) : d.healthMin,
    healthMax: h?.kind === 'http' ? String(h.maxStatus) : d.healthMax,
    healthExe: h?.kind === 'command' ? h.exe : '',
    healthArgsText: h?.kind === 'command' ? h.args.join('\n') : '',
    healthInterval: h ? String(h.intervalSecs) : d.healthInterval,
    healthTimeout: h ? String(h.timeoutSecs) : d.healthTimeout,
    keepRunningOnQuit: s.keepRunningOnQuit,
    enabled: s.enabled,
  };
}
