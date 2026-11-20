import { describe, expect, test } from 'vitest';
import { ServiceSpecSchema } from './services.schema';

// Shaped exactly like Rust's serde output for `ServiceSpec`.
const rustJson = {
  id: 'a1',
  name: 'API',
  description: '',
  category: 'backend',
  projectId: null,
  workingDir: 'C:/work/api',
  executable: 'python',
  args: ['-m', 'http.server'],
  runThroughShell: false,
  env: [
    { key: 'PLAIN', value: '1', isSecret: false },
    { key: 'TOKEN', value: null, isSecret: true },
  ],
  port: 8000,
  startupTimeoutSecs: 30,
  shutdownTimeoutSecs: 10,
  restart: { kind: 'onFailure', maxRetries: 3, backoffSecs: 2 },
  health: {
    kind: 'http',
    url: 'http://localhost:8000',
    minStatus: 200,
    maxStatus: 399,
    intervalSecs: 5,
    timeoutSecs: 2,
  },
  keepRunningOnQuit: false,
  enabled: true,
};

describe('ServiceSpecSchema', () => {
  test('accepts what Rust sends', () => {
    expect(ServiceSpecSchema.parse(rustJson)).toEqual(rustJson);
  });

  test('accepts every restart policy and health kind', () => {
    for (const restart of [{ kind: 'never' }, { kind: 'always', maxRetries: 1, backoffSecs: 1 }]) {
      expect(ServiceSpecSchema.parse({ ...rustJson, restart }).restart).toEqual(restart);
    }
    for (const health of [
      { kind: 'tcp', port: 5432, intervalSecs: 5, timeoutSecs: 2 },
      { kind: 'command', exe: 'pg_isready', args: [], intervalSecs: 5, timeoutSecs: 2 },
      null,
    ]) {
      expect(ServiceSpecSchema.parse({ ...rustJson, health }).health).toEqual(health);
    }
  });

  test('rejects an unknown category', () => {
    expect(() => ServiceSpecSchema.parse({ ...rustJson, category: 'nope' })).toThrow();
  });

  test('rejects a missing required field', () => {
    const { executable: _omit, ...rest } = rustJson;
    expect(() => ServiceSpecSchema.parse(rest)).toThrow();
  });
});
